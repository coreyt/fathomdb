#!/usr/bin/env python3
"""ledgerwrite — append one well-formed JSON record to a JSONL ledger.

The write-side companion to ledgerwatch. Its whole reason to exist is the same
one ledgerwatch has on the read side: keep an agent's *context* small and its
attention un-drifted while it works a long-lived ledger. ledgerwatch lets you
read only the delta; ledgerwrite lets you append **without opening the file at
all** — so an agent never re-ingests old entries (the thing that chews context
and pulls attention back onto stale work) just to add a new one.

Appending a line is cheap on its own (`echo >> f.jsonl`). What this tool adds:

  * a stamped, structured record — UTC `ts` and a monotonic `seq` are filled in
    for you, so entries sort and cross-reference without you tracking them;
  * a validity guarantee — every record is emitted as exactly one line of valid
    JSON, so a downstream `ledgerwatch --select field=value` / `--json` reader
    can never choke on a hand-mangled line;
  * an atomic append (advisory-locked where the OS supports it), safe for the
    shared working tree;
  * it NEVER reads the ledger body — only a tiny sidecar counter — so the
    "don't re-read old work" discipline is enforced by the tool, not by hope.

It is deliberately generic (like ledgerwatch): it knows nothing about any
particular ledger's vocabulary. `--kind` and `--summary` are the two universal
fields of a ledger entry; everything else is `--field k=v`, `--ref R`, `--body`.
The meaning of the kinds is a convention of whoever owns the ledger.

Usage:
  ledgerwrite.py <ledger.jsonl> --kind decision --summary "..." \\
      [--field surface=roadmap] [--field decider=hitl] [--ref git:abc123] \\
      [--body "longer prose"] [--no-seq] [--dry-run] [--quiet]

Exit status:
  0  the record was appended (or, with --dry-run, would be) and echoed
  2  error — missing/invalid argument, bad --field, shell-substitution residue
     in a free-text argument, or an I/O failure
"""

import argparse
import json
import os
import re
import sys
import uuid
from datetime import datetime, timezone

try:
    import fcntl  # POSIX advisory locking; absent on Windows.
except ImportError:  # pragma: no cover - platform fallback
    fcntl = None


# ---------------------------------------------------------------------------
# THE SHELL-SUBSTITUTION RESIDUE GUARD (TC-53)
# ---------------------------------------------------------------------------
# WHAT WENT WRONG, twice, measured. ledgerwrite silently wrote two mangled
# entries: seq-95/96, where a backtick substitution ate one word, and
# seq-108/109, where `$?` expanded to `0` and a `$(...)` expanded to empty with
# `basename: missing operand` on stderr. A third is quoted inside steward
# seq-147. Every one was caught only because a human re-read the record, and the
# ledger is APPEND-ONLY: a mangled entry can be ANNOTATED by a follow-up entry
# and never repaired in place.
#
# THE LIMIT, STATED HONESTLY BECAUSE IT IS THE WHOLE SHAPE OF THE FIX. The
# failure happens AT THE CALLER. The shell expands before ledgerwrite is even
# executed, so this code can only ever see RESIDUE — never INTENT. NONE OF THE
# THREE REAL INCIDENTS ABOVE IS DETECTABLE HERE: `$?` becomes the ordinary
# character `0`, and an eaten word leaves nothing but a gap. That is why
# `dev/steward/README.md` carries the load-bearing half of TC-53 (single-quote
# `--summary` and `--body`, always) and why nothing below claims to catch a
# substitution that already succeeded.
#
# WHY THE REFUSAL SET IS SO SMALL, AND WHY THAT IS THE POINT. This tool sits on
# the ACTIVE commit path and is named by three seat/command definitions. A false
# refusal blocks the Steward from recording a ruling, and TC-121 is the live
# precedent for what happens next: `seat-path-guard.sh` read prose as a write
# target and two different agents independently routed around it. So the rule is
# WARN-AND-PROCEED wherever intent is ambiguous, and refuse ONLY residue that
# carries zero information. The boundary is not a taste call: it is calibrated
# against every `summary` and `body` in the repo's three live ledgers
# (test_guard_refuses_no_record_in_the_repos_three_real_ledgers), which is what
# demoted `$()` from a refusal to a warning — TC-53's own summary contains the
# literal text "an empty $() residue".
#
# Measured at the time of writing: 450 fields over 378 records, 0 refusals,
# 10 warnings (2.2%).

OVERRIDE_FLAG = "--accept-shell-residue"


# The generic writer remains vocabulary-free.  The todos profile is intentionally
# small and opt-in: this ledger needs a stable identity and an optimistic update
# guard, so it reads and folds its own history while holding the existing lock.
TODOS_KINDS = {"todo", "consideration", "caveat", "observation", "question"}
TODOS_STATUSES = {
    "open",
    "in-progress",
    "blocked",
    "watching",
    "done",
    "wont-do",
    "superseded",
}
TODOS_TERMINAL = {"done", "wont-do", "superseded"}
TODOS_TRANSITIONS = {
    "open": TODOS_STATUSES,
    "in-progress": TODOS_STATUSES - {"open"},
    "blocked": TODOS_STATUSES,
    "watching": TODOS_STATUSES,
    "done": set(),
    "wont-do": set(),
    "superseded": set(),
}
TODOS_ID_RE = re.compile(
    r"TC-(?:[0-9]+|[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$"
)


class TodosProfileError(Exception):
    """A refused profile write after the ledger lock has been acquired."""


class Finding:
    """One residue hit: a stable `name`, and prose that says what it implies."""

    def __init__(self, name, why):
        self.name = name
        self.why = why

    def __repr__(self):  # pragma: no cover - debugging aid
        return "Finding(%r)" % self.name


# REFUSALS — zero information. Each entry is (name, predicate, why), and each
# carries its own justification for being a hard stop rather than a warning.
#
#  * torn-expansion — an unclosed `$(` or `${`. bash cannot hand you this from a
#    substitution that RAN: an unterminated one is a syntax error and the command
#    never executes. Its presence means the argument was assembled from a
#    partially-quoted fragment, and the text is a fragment too. 0 in the corpus.
#  * empty-parameter-expansion — a literal `${}`. Also a bash syntax error, so it
#    cannot be a surviving literal from a working command line, and no author
#    types it in prose. 0 in the corpus. (Note `$()` is NOT here: it is valid
#    shell, it survives single quotes, and a real ledger entry contains it.)
#  * control-characters — C0/DEL other than tab and newline. This is what
#    capturing a colourised tool's output into a summary leaves behind. It says
#    nothing a reader can use and it corrupts every downstream consumer of the
#    JSONL stream. 0 in the corpus.
#  * residue-only — after stripping whitespace and shell punctuation, nothing is
#    left. Everything the author wrote expanded away and only the skeleton
#    remains; there is no entry here to record. 0 in the corpus.
_UNCLOSED = re.compile(r"\$\((?![^()]*\))|\$\{(?![^{}]*\})")
_CONTROL = re.compile(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]")
_RESIDUE_ONLY = re.compile(r"^[\s$(){}\[\]`'\"|;&<>*?~!#-]*$")

REFUSALS = (
    (
        "torn-expansion",
        _UNCLOSED.search,
        "an UNCLOSED `$(` or `${`. A substitution that ran cannot leave one "
        "(it is a shell syntax error), so this text is a fragment of a "
        "partially-quoted argument",
    ),
    (
        "empty-parameter-expansion",
        lambda t: "${}" in t,
        "a literal `${}`, which is a shell syntax error and is not prose either",
    ),
    (
        "control-characters",
        _CONTROL.search,
        "raw control characters (ANSI escapes from a colourised tool's output). "
        "They carry no information and corrupt every reader of the JSONL stream",
    ),
    (
        "residue-only",
        lambda t: bool(t) and bool(_RESIDUE_ONLY.match(t)),
        "nothing but whitespace and shell punctuation — every word expanded away "
        "and only the skeleton is left",
    ),
)

# WARNINGS — ambiguous, so they NEVER change the exit code.
#
#  * expansion-syntax-survived — the text still contains `$(`, `${`, `$NAME`,
#    `$0`/`$1`, `$?` or a backtick. These SURVIVED, which means this particular
#    call was quoted correctly; the warning is that the same text sent through
#    DOUBLE quotes would have been rewritten silently, and that this repo's
#    prose habits (`$0 probe`, `~$1.77`, `$15 spend`, single-quoted code spans)
#    put it one quoting slip away from the incident. 8/450 in the corpus.
#  * collapsed-whitespace — an interior run of two or more spaces. This is the
#    ONE real-incident shape the tool can see: `"An optional `design_refs`
#    array"` through double quotes becomes `"An optional  array"`, and the
#    doubled space is the only trace (steward seq-147 quotes exactly that).
#    2/450 in the corpus, both of them prose ABOUT an eaten word.
#  * empty-substitution-skeleton — a literal `$()`, `( )` or `[ ]`. Consistent
#    with a substitution that expanded to nothing, and also with prose quoting
#    the syntax, which is why it warns. 1/450 in the corpus (TC-53's own entry).
#  * unbalanced-backticks — an odd number of backticks: a torn code span. 0/450,
#    but a lone backtick discussing the character itself is plausible enough that
#    refusing would be a guess about intent.
_EXPANSION = re.compile(r"\$\(|\$\{|\$[0-9?!*@#$]|\$[A-Za-z_]|`")
_GAP = re.compile(r"\S {2,}\S")
_EMPTY_SKELETON = re.compile(r"\$\(\)|\(\s+\)|\[\s+\]")

WARNINGS = (
    (
        "expansion-syntax-survived",
        _EXPANSION.search,
        "shell expansion syntax that SURVIVED, so this call was quoted "
        "correctly — but the same text in DOUBLE quotes would have been "
        "rewritten with no error. Confirm this is the text you typed",
    ),
    (
        "collapsed-whitespace",
        _GAP.search,
        "an interior run of two or more spaces, which is exactly what a "
        "substitution that expanded to empty leaves behind (steward seq-147)",
    ),
    (
        "empty-substitution-skeleton",
        _EMPTY_SKELETON.search,
        "an empty `$()`, `( )` or `[ ]` — consistent with a substitution that "
        "expanded to nothing, and also with prose quoting the syntax",
    ),
    (
        "unbalanced-backticks",
        lambda t: t.count("`") % 2 == 1,
        "an odd number of backticks, i.e. a torn code span",
    ),
)


def scan_shell_residue(text):
    """Return (refusals, warnings) as lists of Finding for one free-text value.

    Pure and side-effect free so the calibration test can run it over every
    record in the live ledgers without going near argparse or the filesystem.
    """
    if not isinstance(text, str):
        return [], []
    refusals = [Finding(n, why) for n, pred, why in REFUSALS if pred(text)]
    warnings = [Finding(n, why) for n, pred, why in WARNINGS if pred(text)]
    return refusals, warnings


def _requote_hint(arg_label, err):
    """Say EXACTLY how to get the text through, and name the override.

    A refusal that leaves an agent with no sanctioned path forward is how a
    hand-append to an append-only ledger happens, which is strictly worse than
    the corruption being prevented. So every refusal prints both routes.
    """
    print(
        "  FIX — single-quote the argument so the shell cannot expand it at all:",
        file=err,
    )
    print(
        "    python3 dev/agent-tools/ledgerwrite/ledgerwrite.py <ledger.jsonl> \\",
        file=err,
    )
    print(
        "      --kind decision %s 'text with $VARS and `backticks` kept literal'"
        % arg_label,
        file=err,
    )
    print(
        "    A single-quoted string cannot contain a single quote; break out with",
        file=err,
    )
    print("    '\"'\"' or pass the text from a file via a shell variable.", file=err)
    print(
        "  OVERRIDE — if the residue really is the text you meant, re-run with %s."
        % OVERRIDE_FLAG,
        file=err,
    )


def guard_free_text(arg_label, text, err, override):
    """Apply the guard to one argument. Returns True if the write may proceed.

    Warnings always print and never block. Refusals block unless `override` is
    set, in which case they print anyway — naming each class — so the choice is
    auditable in the transcript rather than silent.
    """
    refusals, warnings = scan_shell_residue(text)
    for finding in warnings:
        print(
            "ledgerwrite: WARNING [%s] %s contains %s."
            % (finding.name, arg_label, finding.why),
            file=err,
        )
    if not refusals:
        return True
    names = ", ".join(f.name for f in refusals)
    if override:
        print(
            "ledgerwrite: %s given — proceeding despite shell-substitution "
            "residue in %s (%s)." % (OVERRIDE_FLAG, arg_label, names),
            file=err,
        )
        return True
    print(
        "ledgerwrite: REFUSED — shell-substitution residue in %s (%s)."
        % (arg_label, names),
        file=err,
    )
    for finding in refusals:
        print("    %s: %s." % (finding.name, finding.why), file=err)
    print(
        "  The shell expands BEFORE this tool runs, so what arrived here is not "
        "what",
        file=err,
    )
    print(
        "  you typed. The ledger is APPEND-ONLY: a mangled entry can only be "
        "annotated",
        file=err,
    )
    print("  by a follow-up entry, never repaired (TC-53).", file=err)
    _requote_hint(arg_label, err)
    return False


def utc_ts() -> str:
    """UTC ISO-8601 with millisecond precision and a trailing Z."""
    return (
        datetime.now(timezone.utc)
        .isoformat(timespec="milliseconds")
        .replace("+00:00", "Z")
    )


def parse_fields(items):
    """Turn repeated ``field=value`` items into a dict (last write wins).

    Splits on the first ``=`` only, so values may contain ``=``. A missing ``=``
    is a hard error — a silently-dropped field is worse than a loud one.
    """
    fields = {}
    for item in items or []:
        if "=" not in item:
            raise ValueError(f"--field must be key=value: {item!r}")
        key, value = item.split("=", 1)
        key = key.strip()
        if not key:
            raise ValueError(f"--field key must be non-empty: {item!r}")
        fields[key] = value
    return fields


def next_seq(seq_path: str) -> int:
    """Read → increment → write the sidecar counter. Never touches the ledger.

    A missing or corrupt counter starts at 1 (never crashes the write). The
    caller holds the ledger lock across this, so the read-modify-write is safe
    against a concurrent ledgerwrite on the same ledger.
    """
    try:
        with open(seq_path, "r", encoding="utf-8") as fh:
            current = int(fh.read().strip() or "0")
    except (FileNotFoundError, ValueError, OSError):
        current = 0
    # A missing/corrupt/nonsensical (e.g. negative) counter restarts at 1.
    if current < 0:
        current = 0
    nxt = current + 1
    tmp = seq_path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as fh:
        fh.write(str(nxt))
    os.replace(tmp, seq_path)
    return nxt


def build_record(args, fields):
    """Assemble the record body (``kind`` onward) with a stable key order.

    ``ts`` and ``seq`` are prepended by the caller inside the flock — ``ts`` so
    it is ordered consistently with ``seq`` under concurrency, ``seq`` because it
    needs the counter file. Arbitrary --field keys follow the reserved head,
    sorted. Reserved keys win over a colliding --field (with a warning) so the
    record shape stays predictable. Returns (tail, clobbered_reserved_keys).
    """
    record = {"kind": args.kind, "summary": args.summary}
    reserved = {"ts", "seq", "kind", "summary", "refs", "body"}
    extra = {k: v for k, v in fields.items() if k not in reserved}
    for key in sorted(extra):
        record[key] = extra[key]
    if args.ref:
        record["refs"] = list(args.ref)
    if args.body is not None:
        record["body"] = args.body
    return record, (set(fields) & reserved)


def todos_shape_error(args, fields):
    """Validate the profile-owned fields before any file is opened."""
    if args.no_seq:
        return "ledgerwrite: todos profile requires seq; --no-seq is not allowed"
    if args.open:
        if "id" in fields:
            return "ledgerwrite: todos --open allocates id; do not pass --field id=..."
        if args.expected_prior_seq is not None:
            return "ledgerwrite: todos --open does not accept --expected-prior-seq"
    else:
        if not fields.get("id"):
            return "ledgerwrite: todos profile requires --field id=..."
        if not TODOS_ID_RE.fullmatch(fields["id"]):
            return "ledgerwrite: invalid todos id (expected legacy TC-N or TC-UUID)"
        if args.expected_prior_seq is None:
            return "ledgerwrite: todos update requires --expected-prior-seq"
        if args.expected_prior_seq < 1:
            return "ledgerwrite: --expected-prior-seq must be positive"
    if args.kind not in TODOS_KINDS:
        return "ledgerwrite: invalid todos kind"
    if fields.get("status") not in TODOS_STATUSES:
        return "ledgerwrite: invalid todos status"
    return None


def read_todos_records(fd):
    """Read valid records for the opt-in profile while the caller holds flock."""
    size = os.fstat(fd).st_size
    if not size:
        return []
    data = os.pread(fd, size, 0) if hasattr(os, "pread") else os.read(fd, size)
    records = []
    for number, line in enumerate(data.decode("utf-8").splitlines(), 1):
        if not line.strip():
            continue
        try:
            record = json.loads(line)
        except json.JSONDecodeError as exc:
            raise ValueError("ledger has invalid JSON at line %d" % number) from exc
        if not isinstance(record, dict):
            raise ValueError("ledger has non-object JSON at line %d" % number)
        records.append(record)
    return records


def validate_todos_history(args, fields, records):
    """Return an error string, or None, using the profile's folded history."""
    by_id = {}
    for record in records:
        item_id = record.get("id")
        if isinstance(item_id, str):
            by_id[item_id] = record

    if args.open:
        item_id = "TC-" + str(uuid.uuid4())
        # UUID collisions are fantastically unlikely, but this is the exact
        # operation that must reject a reused identity rather than assume luck.
        while item_id in by_id:
            item_id = "TC-" + str(uuid.uuid4())
        fields["id"] = item_id
        return None

    previous = by_id.get(fields["id"])
    if previous is None:
        return "ledgerwrite: todos update id does not exist; use --open for a new item"
    if previous.get("kind") != args.kind:
        return "ledgerwrite: todos kind is immutable for an existing id"
    if previous.get("seq") != args.expected_prior_seq:
        return "ledgerwrite: expected prior seq does not match current item state"
    prior_status = previous.get("status")
    if prior_status not in TODOS_STATUSES:
        return "ledgerwrite: existing todos item has invalid status; repair via migration"
    if fields["status"] not in TODOS_TRANSITIONS[prior_status]:
        return "ledgerwrite: illegal todos status transition"
    return None


def run(argv, out=sys.stdout, err=sys.stderr) -> int:
    parser = argparse.ArgumentParser(prog="ledgerwrite", add_help=True)
    parser.add_argument("file", nargs="?")
    parser.add_argument("--kind", help="entry kind (e.g. decision, drift, reconcile)")
    parser.add_argument("--summary", help="one-line human summary of the entry")
    parser.add_argument(
        "--field",
        action="append",
        default=[],
        metavar="KEY=VALUE",
        help="arbitrary scalar field; repeatable (last write wins per key)",
    )
    parser.add_argument(
        "--ref",
        action="append",
        default=[],
        metavar="REF",
        help="a reference (git:sha, plan:path, seq:N); repeatable → refs[]",
    )
    parser.add_argument("--body", default=None, help="optional longer prose body")
    parser.add_argument(
        "--profile",
        choices=("todos",),
        help="opt-in ledger contract; generic writes remain vocabulary-free",
    )
    parser.add_argument(
        "--open",
        action="store_true",
        help="create a new todos item with a tool-allocated immutable id",
    )
    parser.add_argument(
        "--expected-prior-seq",
        type=int,
        help="todos update: seq of the current item entry being replaced",
    )
    parser.add_argument(
        "--no-seq", action="store_true", help="do not assign a monotonic seq"
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="validate and echo the record without writing it",
    )
    parser.add_argument(
        "--quiet", action="store_true", help="do not echo the record on success"
    )
    # ONE override, named so it is obvious in a shell history and a transcript
    # what was waived. See the residue-guard block above for why a sanctioned
    # escape hatch is load-bearing rather than a weakness.
    parser.add_argument(
        OVERRIDE_FLAG,
        dest="accept_shell_residue",
        action="store_true",
        help="write anyway despite shell-substitution residue (TC-53); the "
        "waived classes are named on stderr",
    )
    args = parser.parse_args(argv)

    if not args.file:
        print("ledgerwrite: ledger file argument required", file=err)
        return 2
    if not args.kind or not args.kind.strip():
        print("ledgerwrite: --kind is required and must be non-empty", file=err)
        return 2
    if not args.summary or not args.summary.strip():
        print("ledgerwrite: --summary is required and must be non-empty", file=err)
        return 2

    try:
        fields = parse_fields(args.field)
    except ValueError as exc:
        print(f"ledgerwrite: {exc}", file=err)
        return 2

    if args.open and args.profile != "todos":
        print("ledgerwrite: --open requires --profile todos", file=err)
        return 2
    if args.expected_prior_seq is not None and args.profile != "todos":
        print("ledgerwrite: --expected-prior-seq requires --profile todos", file=err)
        return 2
    if args.profile == "todos":
        profile_error = todos_shape_error(args, fields)
        if profile_error:
            print(profile_error, file=err)
            return 2

    # THE RESIDUE GUARD RUNS BEFORE --dry-run IS HONOURED, deliberately: --dry-run
    # is precisely the peek mode a careful caller uses to see what the shell did
    # to their string, so it is the LAST place the guard may be switched off. It
    # also runs before the seq counter is touched, so a refusal consumes nothing.
    #
    # SCOPE: the free-text arguments only. `--kind` and `--ref` are structured
    # tokens (`decision`, `git:abc123`) whose vocabulary is checked by their
    # readers; guarding them would add noise without adding a catch.
    guard_targets = [("--summary", args.summary)]
    if args.body is not None:
        guard_targets.append(("--body", args.body))
    for key in sorted(fields):
        guard_targets.append(("--field %s" % key, fields[key]))
    # A LIST, not a generator: `all()` would short-circuit and report only the
    # first bad argument, so a caller with two mangled values would fix one and
    # be refused again. Every target is scanned and every finding is printed.
    ok = [
        guard_free_text(label, text, err, args.accept_shell_residue)
        for label, text in guard_targets
    ]
    if not all(ok):
        return 2

    if args.dry_run:
        if args.profile == "todos" and args.open:
            # A dry-run demonstrates the exact immutable-ID shape without
            # creating state.  Real opens check the ledger under flock below.
            fields["id"] = "TC-" + str(uuid.uuid4())
        tail, clobbered = build_record(args, fields)
        for key in sorted(clobbered):
            print(
                f"ledgerwrite: --field {key}=... ignored (reserved key set by a flag)",
                file=err,
            )
        # Peek/validate: stamp ts and a placeholder seq so the echoed shape
        # matches a real write, but touch nothing on disk.
        record = {"ts": utc_ts()}
        if not args.no_seq:
            record["seq"] = None
        record.update(tail)
        line = json.dumps(record, ensure_ascii=False)
        if not args.quiet:
            out.write(line + "\n")
        return 0

    abspath = os.path.abspath(args.file)
    parent = os.path.dirname(abspath)
    if parent and not os.path.isdir(parent):
        print(f"ledgerwrite: no such directory: {parent}", file=err)
        return 2
    # The seq counter is intrinsic to the ledger, so it always lives beside it —
    # never a user-chosen dir, which would let the same ledger keep two
    # independent counters and reuse a seq.
    seq_path = abspath + ".seq"

    pre_existed = os.path.exists(abspath)
    fd = None
    line = None
    try:
        # O_APPEND: every write lands at EOF. Hold an exclusive advisory lock
        # across the (heal check + seq read-modify-write + append) so concurrent
        # writers can neither reuse a seq nor interleave a line.
        fd = os.open(abspath, os.O_RDWR | os.O_CREAT | os.O_APPEND, 0o644)
        if fcntl is not None:
            fcntl.flock(fd, fcntl.LOCK_EX)
        try:
            # Heal a torn last line: if the file has content whose final byte is
            # not a newline, some writer (a crash, a foreign appender, a hand
            # edit) left an unterminated line. Emit a leading newline so our
            # record lands on its own clean line instead of merging onto it.
            # Reading one byte is O(1) and never enters the agent's context, so
            # this does not compromise the token-efficiency contract.
            prefix = ""
            if hasattr(os, "pread"):
                size = os.fstat(fd).st_size
                if size > 0 and os.pread(fd, 1, size - 1) != b"\n":
                    prefix = "\n"
                    print(
                        "ledgerwrite: healed a torn trailing line "
                        "(a prior write left no newline)",
                        file=err,
                    )
            if args.profile == "todos":
                try:
                    profile_error = validate_todos_history(
                        args, fields, read_todos_records(fd)
                    )
                except (UnicodeDecodeError, ValueError) as exc:
                    raise TodosProfileError(
                        f"ledgerwrite: todos profile refused: {exc}"
                    ) from exc
                if profile_error:
                    raise TodosProfileError(profile_error)
            tail, clobbered = build_record(args, fields)
            for key in sorted(clobbered):
                print(
                    f"ledgerwrite: --field {key}=... ignored (reserved key set by a flag)",
                    file=err,
                )
            # Stamp ts inside the lock so it is ordered consistently with seq.
            record = {"ts": utc_ts()}
            if not args.no_seq:
                record["seq"] = next_seq(seq_path)
            record.update(tail)
            line = json.dumps(record, ensure_ascii=False)
            os.write(fd, (prefix + line + "\n").encode("utf-8"))
            os.fsync(fd)
        finally:
            if fcntl is not None:
                fcntl.flock(fd, fcntl.LOCK_UN)
    except (OSError, TodosProfileError) as exc:
        # If we created the ledger and failed before writing content, remove the
        # empty file so a rejected call leaves the ledger untouched.
        if not pre_existed and os.path.exists(abspath):
            try:
                if os.path.getsize(abspath) == 0:
                    os.remove(abspath)
            except OSError:
                pass
        if isinstance(exc, TodosProfileError):
            print(str(exc), file=err)
        else:
            print(f"ledgerwrite: write failed: {exc}", file=err)
        return 2
    finally:
        if fd is not None:
            try:
                os.close(fd)
            except OSError:
                pass

    if not args.quiet:
        out.write(line + "\n")
    return 0


if __name__ == "__main__":  # pragma: no cover
    sys.exit(run(sys.argv[1:]))
