#!/usr/bin/env python3
"""ledgerwatch — emit only what changed in a monitored file since the last check.

The point of this tool is to keep *context* small while watching files that grow
without bound. It runs at the shell; only its (delta) stdout enters an agent's
context. It persists a per-file cursor in a state directory so each run prints
just what is new/changed and nothing else.

Strategy is chosen by file extension (override with --strategy):

  .jsonl/.ndjson/.log  -> tail    append-only positional cursor (byte offset),
                                   made rotation/truncation-safe by a first-line
                                   signature. Optional --select field filtering.
  .md/.markdown        -> section content-hash per heading. Detects edits made
                                   ANYWHERE in the document (intra-document), which
                                   a positional cursor would miss.
  everything else      -> diff    unified diff against a shadow copy.

Each run reports a MODE so the consumer knows how much to trust the delta:
  incremental  exact new/changed items since a valid cursor.
  cold         first run (or unreadable state): output is a baseline, not events.
  resync       cursor was invalidated (rotation/truncation/rewrite): output is a
               re-read baseline, NOT a list of N new events. (tail only)
In text mode the mode is announced on stderr only when it is not incremental; in
--json mode it is always the "mode" field of the envelope.

For the tail strategy, delta-validate is ON by default: any non-blank delta line
that is not valid JSON is flagged on stderr (so a torn/corrupt line is surfaced
even when --select would otherwise silently drop it). --validate runs a full-file
JSONL integrity scan instead (0 clean / 3 corrupt / 2 error; no delta, no cursor
advance).

--project selects a PROJECTION over an event-sourced ledger. Two are defined:

  latest   (the default; `--project` with no value) folds the ledger to its
           LATEST entry per `id` — the file is the event log, the fold is the
           state. Entries with no `id` are NOT an error: they are counted into
           an explicit, always-printed `unfoldable (no id): N` bucket. That
           bucket is the whole point: the recipe this mode replaces did
           `latest[r["id"]] = r` and died with KeyError on the first id-less
           entry, which is every entry of dev/steward/steward-ledger.jsonl.
  rulings  the RULING REGISTRY: which items have been ruled on, which are still
           open, and which the ledger does not say. See do_project_rulings for
           the projection key, the classifier, and their limits.

Like --prune and --validate, --project is a standalone READ/REPORT mode: it
never writes the ledger and never reads or advances the cursor, so running a
projection cannot silently consume the delta a later watch run is waiting for.

Flags: --json (structured envelope), --dry-run (compute delta without advancing
the cursor), --reset (discard cursor first), --select (tail field filter),
--strategy (override), --no-status (see below), --prune (drop cursors whose
source file is gone; takes no file argument), --validate (full-file JSONL scan),
--project [latest|rulings] (projection; bare = latest).

Exit status is grep-style by default, so a poller can branch on it without
capturing stdout:
  0  changed   — a delta was emitted on stdout
  1  no change — nothing new (a normal idle tick, NOT a failure)
  2  error     — missing file/argument, bad --select
A no-change run is silent on stdout in both text and --json mode; the status is
on the exit code. Pass --no-status to collapse 0/1 into 0 (success), reserving
nonzero for real errors (2) — for callers whose harness treats any nonzero as a
failure.
"""

import argparse
import difflib
import hashlib
import json
import os
import re
import sys

HEADING_RE = re.compile(r"^#{1,6}\s")

TAIL_EXTS = {".jsonl", ".ndjson", ".log"}
SECTION_EXTS = {".md", ".markdown"}


def sha1(data: bytes) -> str:
    return hashlib.sha1(data).hexdigest()


def read_bytes(path: str) -> bytes:
    with open(path, "rb") as fh:
        return fh.read()


def read_text(path: str) -> str:
    return read_bytes(path).decode("utf-8", errors="replace")


def write_text(path: str, text: str) -> None:
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(text)


# --- strategy selection -----------------------------------------------------


def choose_strategy(path: str, override: str | None) -> str:
    if override:
        return override
    ext = os.path.splitext(path)[1].lower()
    if ext in TAIL_EXTS:
        return "tail"
    if ext in SECTION_EXTS:
        return "section"
    return "diff"


# Each strategy is pure: it computes a structured payload + the next state and
# never writes. The driver renders the payload (text or json) and commits state
# (unless --dry-run). This split is what makes --json and --dry-run fall out for
# free. Strategies return: (payload, new_state, changed, mode, shadow_content).


# --- tail (append-only, rotation-safe) --------------------------------------


def first_line_sig(data: bytes) -> str:
    nl = data.find(b"\n")
    seg = data if nl < 0 else data[:nl]
    return sha1(seg)


def is_json(line: str) -> bool:
    """True if the line parses as JSON (used by delta-validate and --validate)."""
    try:
        json.loads(line)
        return True
    except Exception:
        return False


def match_select(line: str, select: dict) -> bool:
    try:
        obj = json.loads(line)
    except Exception:
        return False
    for field, values in select.items():
        if str(obj.get(field)) not in values:
            return False
    return True


def strat_tail(path: str, state: dict, select: dict):
    data = read_bytes(path)
    size = len(data)
    head_sig = first_line_sig(data)
    prev_off = state.get("offset", 0)
    prev_head = state.get("head_sig")

    if prev_head is None:
        mode = "cold"  # no usable prior cursor (first run / corrupt / reset)
    elif prev_head != head_sig or prev_off > size:
        # Rotation / truncation / in-place rewrite: the bytes we already
        # committed are no longer a stable prefix. Re-read from the start so we
        # never silently skip the replacement content.
        mode = "resync"
    else:
        mode = "incremental"
    start = 0 if mode != "incremental" else prev_off

    # Only commit up to the last complete line; a partial trailing line (writer
    # mid-append) is left for the next run so it is emitted exactly once.
    last_nl = data.rfind(b"\n")
    commit = last_nl + 1 if last_nl >= 0 else 0
    if commit < start:
        commit = start

    chunk = data[start:commit]
    raw_lines = chunk.decode("utf-8", errors="replace").splitlines()

    # Delta-validate (default, tail only): flag any non-empty delta line that is
    # not valid JSON, so a torn/corrupt line is surfaced even when --select would
    # otherwise silently drop it. Scoped to the delta, so it never re-spams old
    # corruption (which lies before the cursor).
    lines_before = data[:start].count(b"\n")
    invalid = [
        {"line": lines_before + idx + 1, "text": ln[:120]}
        for idx, ln in enumerate(raw_lines)
        if ln.strip() and not is_json(ln)
    ]

    lines = raw_lines
    if select:
        lines = [ln for ln in raw_lines if match_select(ln, select)]

    payload = {"lines": lines}
    if invalid:
        payload["invalid"] = invalid
    new_state = {
        "strategy": "tail",
        "offset": commit,
        "head_sig": head_sig,
        "size": size,
        "path": path,
    }
    return payload, new_state, bool(lines), mode, None


# --- section (content-hash per heading, intra-document safe) -----------------


def parse_sections(text: str):
    blocks = []
    cur_key = "(preamble)"
    cur = []
    for ln in text.splitlines(keepends=True):
        if HEADING_RE.match(ln):
            blocks.append((cur_key, "".join(cur)))
            cur_key = ln.strip()
            cur = [ln]
        else:
            cur.append(ln)
    blocks.append((cur_key, "".join(cur)))

    result = []
    seen = {}
    for key, body in blocks:
        if key == "(preamble)" and not body.strip():
            continue
        seen[key] = seen.get(key, 0) + 1
        disambiguated = key if seen[key] == 1 else f"{key} #{seen[key]}"
        result.append((disambiguated, body))
    return result


def strat_section(path: str, state: dict):
    text = read_text(path)
    current = parse_sections(text)
    has_prev = "sections" in state
    prev = state.get("sections", {})
    mode = "incremental" if has_prev else "cold"  # content-addressed: never resyncs

    cur_map = {}
    sections = []
    for key, body in current:
        # Hash on stripped content so the blank line that separates sections
        # (which re-attaches to a different section when sections are reordered)
        # does not register as a spurious change.
        digest = sha1(body.strip().encode("utf-8"))
        cur_map[key] = digest
        if key not in prev:
            sections.append({"key": key, "kind": "new", "body": body.rstrip("\n")})
        elif prev[key] != digest:
            sections.append({"key": key, "kind": "changed", "body": body.rstrip("\n")})
    for key in prev:
        if key not in cur_map:
            sections.append({"key": key, "kind": "removed", "body": ""})

    payload = {"sections": sections}
    new_state = {"strategy": "section", "sections": cur_map, "path": path}
    return payload, new_state, bool(sections), mode, None


# --- diff (shadow copy, works for any text file) ----------------------------


def strat_diff(path: str, state: dict, shadow_path: str):
    new = read_text(path)
    has_shadow = os.path.exists(shadow_path)
    old = read_text(shadow_path) if has_shadow else ""
    mode = "incremental" if has_shadow else "cold"

    diff = list(
        difflib.unified_diff(
            old.splitlines(keepends=True),
            new.splitlines(keepends=True),
            fromfile="prev",
            tofile="cur",
        )
    )
    payload = {"diff": "".join(diff)}
    new_state = {
        "strategy": "diff",
        "shadow": os.path.basename(shadow_path),
        "path": path,
    }
    # shadow_content is committed by the driver (skipped on --dry-run).
    return payload, new_state, bool(diff), mode, new


# --- rendering --------------------------------------------------------------


def render_text(strategy: str, payload: dict) -> str:
    if strategy == "tail":
        lines = payload["lines"]
        return ("\n".join(lines) + "\n") if lines else ""
    if strategy == "section":
        parts = []
        for s in payload["sections"]:
            if s["kind"] == "removed":
                parts.append(f"===== {s['key']} [removed] =====\n")
            else:
                parts.append(f"===== {s['key']} [{s['kind']}] =====\n{s['body']}\n")
        return "\n".join(parts)
    return payload["diff"]


# --- state I/O --------------------------------------------------------------


def load_state(state_path: str) -> dict:
    try:
        with open(state_path, "r", encoding="utf-8") as fh:
            return json.load(fh)
    except Exception:
        # Missing or corrupt state -> cold start. Never crash the monitor.
        return {}


def save_state(state_path: str, state: dict) -> None:
    tmp = state_path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as fh:
        json.dump(state, fh)
    os.replace(tmp, state_path)


def parse_select(items):
    select = {}
    for item in items or []:
        if "=" not in item:
            raise ValueError(f"--select must be field=value[,value]: {item!r}")
        field, values = item.split("=", 1)
        select.setdefault(field, set()).update(v for v in values.split(",") if v)
    return select


def do_prune(state_dir: str, out, err) -> int:
    """Drop cursor (+shadow) files whose recorded source path no longer exists.

    Unparseable state files, and ones with no recorded path, are left untouched
    (we cannot prove what they track). Safe to run anytime; `rm -rf` on the
    state dir is an equally valid full reset.
    """
    if not os.path.isdir(state_dir):
        print("ledgerwatch: pruned 0 stale cursor(s)", file=err)
        return 0
    pruned = []
    for name in sorted(os.listdir(state_dir)):
        if not name.endswith(".json"):
            continue
        st = load_state(os.path.join(state_dir, name))
        path = st.get("path")
        if not path or os.path.exists(path):
            continue
        os.remove(os.path.join(state_dir, name))
        try:
            os.remove(os.path.join(state_dir, name[:-5] + ".shadow"))
        except FileNotFoundError:
            pass
        pruned.append(path)
    for p in pruned:
        print(f"ledgerwatch: pruned {p}", file=err)
    print(f"ledgerwatch: pruned {len(pruned)} stale cursor(s)", file=err)
    return 0


def do_validate(path: str, as_json: bool, out, err) -> int:
    """Full-file JSONL integrity scan (opt-in `--validate`).

    Reports every non-blank line that is not valid JSON, plus an unterminated
    final line, as a bounded summary. It reads the whole file (O(file) disk) but
    caps its output so context stays O(problems), not O(file). Distinct exit code
    so it never muddies the delta status: 0 = clean, 3 = corruption found,
    2 = read error. Blank/whitespace-only lines are ignored (never written by a
    well-behaved appender, harmless if present).
    """
    try:
        data = read_bytes(path)
    except OSError as exc:
        print(f"ledgerwatch: cannot read {path}: {exc}", file=err)
        return 2
    text = data.decode("utf-8", errors="replace")
    unterminated = bool(data) and not data.endswith(b"\n")
    rows = text.split("\n")
    invalid = []
    last = len(rows) - 1
    for i, ln in enumerate(rows):
        if i == last and ln == "":
            continue  # the empty tail produced by a final newline is not a line
        if ln.strip() == "":
            continue  # ignore blank lines
        if not is_json(ln):
            invalid.append((i + 1, ln[:120]))
    clean = not invalid and not unterminated
    cap = 20
    shown = invalid[:cap]

    if as_json:
        envelope = {
            "file": path,
            "mode": "validate",
            "valid": clean,
            "invalid_count": len(invalid),
            "invalid_lines": [{"line": n, "text": t} for n, t in shown],
            "truncated": len(invalid) > cap,
            "unterminated_final_line": unterminated,
        }
        out.write(json.dumps(envelope) + "\n")
    elif clean:
        print(f"ledgerwatch: {path}: valid (no invalid lines)", file=err)
    else:
        for n, t in shown:
            print(f"ledgerwatch: {path}:{n} invalid JSON: {t}", file=err)
        if len(invalid) > cap:
            print(
                f"ledgerwatch: ...and {len(invalid) - cap} more invalid line(s)",
                file=err,
            )
        if unterminated:
            print(
                f"ledgerwatch: {path}: unterminated final line (no trailing newline)",
                file=err,
            )
    return 0 if clean else 3


def do_project(path: str, as_json: bool, out, err) -> int:
    """Fold an event-sourced JSONL ledger to its latest entry per `id`.

    These ledgers are event logs: an item's current state is the LAST entry
    carrying its `id`, and every earlier entry for that id is history. This mode
    computes that fold. Last occurrence in FILE order wins — the files are
    append-ordered (and `check-ledgers.sh` gates that their seq runs are
    contiguous), so file order is append order.

    Entries with no usable `id` are a first-class, expected case, not an error:
    dev/steward/steward-ledger.jsonl is a decision trail whose 107/107 entries
    carry no `id` at all. They are counted into the `unfoldable (no id): N`
    bucket, which is printed on EVERY run (including when it is 0) so a caller
    can never mistake "nothing to fold" for "everything folded". A line that is
    not valid JSON, or is valid JSON but not an object, goes to a separate
    `malformed` bucket (use --validate for the line-by-line integrity report).

    Read/report only: never writes the ledger, and — like --prune/--validate —
    never touches the cursor state directory, so a projection cannot surprise a
    caller by consuming the delta their next watch run is waiting for.

    Text mode prints one compact JSON object per folded id on stdout, sorted
    lexicographically by id; --json prints a single envelope. Exit codes are NOT
    grep-style here (there is no "no change" notion for a projection):
    0 = projection emitted (even if it is empty), 2 = read error.
    """
    try:
        data = read_bytes(path)
    except OSError as exc:
        print(f"ledgerwatch: cannot read {path}: {exc}", file=err)
        return 2

    text = data.decode("utf-8", errors="replace")
    latest: dict[str, dict] = {}
    entries = 0
    no_id = 0
    malformed = 0

    for raw in text.split("\n"):
        if raw.strip() == "":
            continue  # blank lines are not entries
        entries += 1
        try:
            obj = json.loads(raw)
        except Exception:
            malformed += 1
            continue
        if not isinstance(obj, dict):
            malformed += 1
            continue
        ident = obj.get("id")
        if ident is None or str(ident) == "":
            no_id += 1
            continue
        latest[str(ident)] = obj  # last write wins (file order == append order)

    rows = [latest[key] for key in sorted(latest)]

    if as_json:
        envelope = {
            "file": path,
            "mode": "project",
            "entries": entries,
            "folded_ids": len(rows),
            "unfoldable_no_id": no_id,
            "malformed": malformed,
            "latest": rows,
        }
        out.write(json.dumps(envelope) + "\n")
    else:
        for row in rows:
            out.write(json.dumps(row, sort_keys=True) + "\n")

    print(
        f"ledgerwatch: --project {path}: {len(rows)} id(s) folded "
        f"from {entries} entries; unfoldable (no id): {no_id}; "
        f"malformed: {malformed}",
        file=err,
    )
    return 0


# --- rulings projection (the ruling registry) --------------------------------

# The ONE literal status this tool reads. It is not a vocabulary this tool
# invents: `open` is declared in dev/todos-and-considerations-ledger-readme.md
# §2.1 as the ledger's own non-terminal status, and it is the dominant literal
# value in the data (46 `open` + 9 `OPEN` of 76 entries). Everything else —
# `watching`, `in_progress`, `converged-pending-hitl`, `build-authorized`,
# `placed`, `resolved`, `closed`, `ratified-both-sides`, … — is echoed VERBATIM
# and never interpreted. Inventing a status vocabulary is a refused move in this
# effort, and the README's own TERMINAL vocabulary (`done`/`wont-do`/
# `superseded`) matches ZERO entries in any of the three real ledgers, so there
# is no declared terminal set to read either.
OPEN_STATUS = "open"

# Emission order. Deterministic and stable: bucket first, then key.
VERDICT_ORDER = {"ruled": 0, "unruled": 1, "unclassified": 2}


def ruling_key(obj: dict, lineno: int):
    """Address an entry. Returns (key, from_id).

    `id` when the entry carries one — that is the join key the todos ledger
    declares, and it is what makes the fold-to-latest reuse work. Otherwise
    `seq-N`: dev/steward/steward-ledger.jsonl holds most of this repo's rulings
    and 107/107 of its entries have NO `id`, and ADDING one is banned (that file
    is a decision trail, not an item register). `seq` is stamped by ledgerwrite
    on every entry of all three ledgers and check-ledgers.sh gates that it is
    contiguous, so `seq-N` is a real, stable address — it is simply scoped to
    ONE file, which is why this mode takes one ledger at a time. A `seq`-less
    entry falls back to `line-N` so that no entry is ever dropped for want of a
    key (the KeyError class T1b closed).
    """
    ident = obj.get("id")
    if ident is not None and str(ident) != "":
        return str(ident), True
    seq = obj.get("seq")
    if seq is not None and str(seq) != "":
        return "seq-%s" % seq, False
    return "line-%d" % lineno, False


def do_project_rulings(path: str, as_json: bool, out, err) -> int:
    """Project a ledger to its RULING REGISTRY: ruled / unruled / unclassified.

    WHY THIS EXISTS: rulings live in >=4 homes with no index, and the 0.8.20
    board listed >=4 already-ruled items as still open — which is the direct
    generator of re-deciding settled calls, the most expensive failure this
    program has. A registry has to be derived from the ledger, not narrated
    beside it.

    THE PROJECTION KEY is `id` when present, else `seq-N` (see ruling_key).
    Items are folded to their LATEST entry per key — the same fold `--project
    latest` performs, for the same reason: the file is an event log, so the
    item's current fields are its last entry's.

    THE CLASSIFIER, and its limits, stated honestly:

      ruled         ANY entry under this key has `kind == "decision"`.
                    `kind` is the only field present on 193/193 entries of all
                    three ledgers, and `decision` is its actual cross-ledger
                    value for "this entry records a ruling" (68 steward + 6
                    todos + 5 OPP-12). ANY entry, not just the latest: a ruling
                    once recorded is not un-recorded by a later observation.
                    `ruling_seqs` names exactly which entries carry it.
      unruled       no ruling entry, and the folded entry's `status` is `open`
                    (case-insensitively — the todos ledger contains both `open`
                    and `OPEN`). See OPEN_STATUS for why that one literal.
      unclassified  everything else. This bucket is LARGE and that is the point:
                    it is where the ledgers genuinely do not say. Real instance:
                    TC-7 walked open -> in_progress -> converged-pending-hitl ->
                    resolved with NO `kind: decision` entry anywhere, so a
                    registry cannot see that it was ruled. Guessing from
                    `resolved` would be inventing the vocabulary this effort
                    already refused. Every unclassified row is still EMITTED,
                    with its literal `kind`/`status`, so it is visible and
                    countable — never silently dropped.

    LIMITS a caller must know: (1) `seq` is unique per FILE, so this mode reports
    one ledger at a time and its keys must not be merged across files without a
    file qualifier; (2) it classifies ENTRIES, not the truth of a ruling — it can
    say "the ledger records a decision here", never "this decision is correct";
    (3) an item ruled in prose but never entered as `kind: decision` reads as
    unruled or unclassified, which is a finding about the LEDGER, not a defect
    here.

    Read/report only: never writes the ledger, never touches the cursor state
    directory. Exit 0 = registry emitted (even if empty), 2 = read error.
    """
    try:
        data = read_bytes(path)
    except OSError as exc:
        print(f"ledgerwatch: cannot read {path}: {exc}", file=err)
        return 2

    text = data.decode("utf-8", errors="replace")
    order: list[str] = []
    latest: dict[str, dict] = {}
    ruling_seqs: dict[str, list] = {}
    from_id: dict[str, bool] = {}
    entries = 0
    malformed = 0

    for lineno, raw in enumerate(text.split("\n"), start=1):
        if raw.strip() == "":
            continue  # blank lines are not entries
        entries += 1
        try:
            obj = json.loads(raw)
        except Exception:
            malformed += 1
            continue
        if not isinstance(obj, dict):
            malformed += 1
            continue
        key, keyed_by_id = ruling_key(obj, lineno)
        if key not in latest:
            order.append(key)
            ruling_seqs[key] = []
            from_id[key] = keyed_by_id
        latest[key] = obj  # last write wins (file order == append order)
        if obj.get("kind") == "decision":
            ruling_seqs[key].append(obj.get("seq", lineno))

    rows = []
    counts = {"ruled": 0, "unruled": 0, "unclassified": 0}
    for key in order:
        obj = latest[key]
        status = obj.get("status")
        if ruling_seqs[key]:
            verdict = "ruled"
        elif status is not None and str(status).casefold() == OPEN_STATUS:
            verdict = "unruled"
        else:
            verdict = "unclassified"
        counts[verdict] += 1
        rows.append(
            {
                "key": key,
                "verdict": verdict,
                "seq": obj.get("seq"),
                "kind": obj.get("kind"),
                "status": status,
                "decider": obj.get("decider"),
                "ruling_seqs": ruling_seqs[key],
                "summary": obj.get("summary"),
            }
        )

    rows.sort(key=lambda r: (VERDICT_ORDER[r["verdict"]], r["key"]))
    keyed_by_seq = sum(1 for key in order if not from_id[key])

    if as_json:
        envelope = {
            "file": path,
            "mode": "project",
            "projection": "rulings",
            "entries": entries,
            "items": len(rows),
            "ruled": counts["ruled"],
            "unruled": counts["unruled"],
            "unclassified": counts["unclassified"],
            "malformed": malformed,
            "keyed_by_seq": keyed_by_seq,
            "rows": rows,
        }
        out.write(json.dumps(envelope) + "\n")
    else:
        for row in rows:
            out.write(json.dumps(row, sort_keys=True) + "\n")

    # EVERY bucket on EVERY run, including the zeros. Same discipline as
    # `unfoldable (no id): N`: a bucket that only appears when non-empty lets
    # "nothing to classify" read as "everything classified".
    print(
        f"ledgerwatch: --project rulings {path}: {len(rows)} item(s) "
        f"from {entries} entries; ruled: {counts['ruled']}; "
        f"unruled: {counts['unruled']}; "
        f"unclassified: {counts['unclassified']}; "
        f"malformed: {malformed}; "
        f"keyed by seq (no id): {keyed_by_seq}",
        file=err,
    )
    return 0


PROJECTIONS = {"latest": do_project, "rulings": do_project_rulings}


# --- driver -----------------------------------------------------------------


def run(argv, out=sys.stdout, err=sys.stderr) -> int:
    parser = argparse.ArgumentParser(prog="ledgerwatch", add_help=True)
    parser.add_argument("file", nargs="?")
    parser.add_argument(
        "--state-dir", default=os.environ.get("LEDGERWATCH_STATE", ".ledgerwatch")
    )
    parser.add_argument("--strategy", choices=["tail", "section", "diff"], default=None)
    parser.add_argument(
        "--select",
        action="append",
        default=[],
        help="tail only: field=value[,value]; repeatable (AND across fields)",
    )
    parser.add_argument(
        "--json", action="store_true", help="emit a structured envelope"
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="show the delta without advancing the cursor",
    )
    parser.add_argument(
        "--reset", action="store_true", help="discard saved cursor first"
    )
    parser.add_argument(
        "--no-status",
        action="store_true",
        help="opt out of grep-style codes: exit 0 on success (changed or not), 2 on error",
    )
    parser.add_argument(
        "--prune", action="store_true", help="drop cursors whose source file is gone"
    )
    parser.add_argument(
        "--validate",
        action="store_true",
        help="JSONL integrity scan of the whole file (0 clean / 3 corrupt / 2 error); "
        "no delta, no cursor advance",
    )
    parser.add_argument(
        "--project",
        nargs="?",
        const="latest",
        default=None,
        metavar="PROJECTION",
        help="project the ledger: `latest` (the default, and what a bare "
        "--project means) folds to the latest entry per id, reporting id-less "
        "entries in an 'unfoldable (no id): N' bucket; `rulings` emits the "
        "ruled/unruled registry, keyed by id or else `seq-N` (the steward "
        "ledger carries NO ids and must not be made to), classified by "
        "`kind == \"decision\"` plus the single literal status `open`, with "
        "everything the ledger does not say landing in a printed "
        "`unclassified` bucket rather than being guessed at or dropped. "
        "Read-only: does NOT write the ledger and does NOT advance the cursor "
        "(0 = projection emitted / 2 = read error)",
    )
    args = parser.parse_args(argv)

    # `--project` takes an OPTIONAL value, so `--project dev/x.jsonl` (the form
    # the ledger READMEs document) would otherwise have the PATH swallowed as
    # the projection name. Re-read it as the file when it is not a projection
    # name. A path that collides with a reserved name (`latest`/`rulings`) must
    # be given positionally.
    if args.project is not None and args.project not in PROJECTIONS:
        if args.file is None:
            args.file, args.project = args.project, "latest"
        else:
            print(
                "ledgerwatch: unknown projection %r (choose from %s)"
                % (args.project, ", ".join(sorted(PROJECTIONS))),
                file=err,
            )
            return 2

    if args.prune:
        return do_prune(args.state_dir, out, err)

    if not args.file:
        print("ledgerwatch: file argument required (or use --prune)", file=err)
        return 2

    abspath = os.path.abspath(args.file)
    if not os.path.isfile(abspath):
        print(f"ledgerwatch: no such file: {args.file}", file=err)
        return 2

    if args.validate:
        # Opt-in full-file integrity scan; a standalone mode (like --prune) that
        # does not compute a delta or advance the cursor.
        return do_validate(abspath, args.json, out, err)

    # Projections. Dispatched HERE, before the state dir is created below, so
    # --project provably cannot create, read or advance a cursor (see
    # do_project's docstring for why that is the safe choice).
    if args.project is not None:
        return PROJECTIONS[args.project](abspath, args.json, out, err)

    try:
        select = parse_select(args.select)
    except ValueError as exc:
        print(f"ledgerwatch: {exc}", file=err)
        return 2

    os.makedirs(args.state_dir, exist_ok=True)
    key = sha1(abspath.encode("utf-8"))
    state_path = os.path.join(args.state_dir, key + ".json")
    shadow_path = os.path.join(args.state_dir, key + ".shadow")

    if args.reset:
        for p in (state_path, shadow_path):
            try:
                os.remove(p)
            except FileNotFoundError:
                pass

    strategy = choose_strategy(abspath, args.strategy)
    if select and strategy != "tail":
        print("ledgerwatch: --select ignored for non-tail strategy", file=err)

    state = load_state(state_path)
    if state.get("strategy") != strategy:
        state = {}  # strategy changed since last run -> cold start

    if strategy == "tail":
        payload, new_state, changed, mode, shadow_content = strat_tail(
            abspath, state, select
        )
    elif strategy == "section":
        payload, new_state, changed, mode, shadow_content = strat_section(
            abspath, state
        )
    else:
        payload, new_state, changed, mode, shadow_content = strat_diff(
            abspath, state, shadow_path
        )

    # Delta-validate (tail, on by default): warn on stderr about any invalid-JSON
    # line in this delta, independent of `changed` — so a corrupt line that
    # --select silently drops (lines empty → changed False) is still surfaced.
    for item in payload.get("invalid", []):
        print(
            f"ledgerwatch: {abspath}:{item['line']} invalid JSON in delta: {item['text']}",
            file=err,
        )

    # stdout carries the payload only, and only when there is one. A no-change
    # run is silent on stdout in BOTH modes; the exit code reports the status, so
    # an idle tick stays free and "empty stdout" is never ambiguous (errors exit
    # 2, so empty + success-code can only mean no change).
    if changed:
        if args.json:
            envelope = {
                "file": abspath,
                "strategy": strategy,
                "mode": mode,
                "changed": True,
            }
            envelope.update(payload)
            out.write(json.dumps(envelope) + "\n")
        else:
            out.write(render_text(strategy, payload))
            if mode != "incremental":
                note = (
                    "baseline, first run"
                    if mode == "cold"
                    else "re-synced after rotation/truncation; treat as baseline, not new events"
                )
                print(f"ledgerwatch: mode={mode} ({note})", file=err)

    if not args.dry_run:
        save_state(state_path, new_state)
        if shadow_content is not None:
            write_text(shadow_path, shadow_content)

    # Default: grep-style status code. 0 = changed, 1 = no change, 2 = error
    # (errors returned earlier). --no-status collapses 0/1 into 0 for callers
    # whose harness reads any nonzero as failure.
    if args.no_status:
        return 0
    return 0 if changed else 1


if __name__ == "__main__":  # pragma: no cover
    sys.exit(run(sys.argv[1:]))
