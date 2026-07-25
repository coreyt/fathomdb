#!/usr/bin/env bash
# scripts/commission-manifest.sh — T3b (DOC-HYGIENE-2): the GENERATED COMMISSION
# MANIFEST. Given a release and a slice, emit the citation list an orchestrator
# brief needs — design-of-record paths, contract paths, plan section anchors, the
# base SHA, the worktree rules and the stop conditions — and HARD-FAIL if any of
# it has rotted.
#
# ---------------------------------------------------------------------------
# WHY (measured, not hypothetical)
# ---------------------------------------------------------------------------
# Commissioning a slice meant hand-assembling the same list every time out of a
# 5-12 file fan-out (the master, the plan, the STATUS board, the hand-off, the
# ledgers, the design tier), and NOTHING checked that the paths it cited still
# resolved. T1d measured what that costs on the anchor axis: the TC-45 pointers
# in `dev/plans/plan-0.8.20.md` were ~2,100 lines off and, per `git log`, had
# never been correct. A citation nobody checks rots silently.
#
# THIS REPLACES A HAND-MAINTAINED BRIEF TEMPLATE; it does not add one. That shape
# already exists twice in-repo, and the section set emitted below is DERIVED from
# what those two documents actually contain, so the generated manifest is a real
# substitute rather than a parallel invention:
#   * dev/plans/prompts/LIBRARY-BUMP-ORCHESTRATOR-TEMPLATE.md — Assignment
#     (branch-from tip, worktree path, coupling), STEP 0 Isolate, Escalate-to-LBS,
#     Closure output.
#   * dev/plans/prompts/0.8.0-SLICE-TEMPLATE.md — {{CONTRACT_REF}}, {{READING}},
#     {{MANDATE}}, {{DEFERRED}}, {{AC_IDS}}, {{GUARDRAILS}}, {{DOD}}, §6 scope
#     discipline, §7 recovery, §9 closure schema.
#
# ---------------------------------------------------------------------------
# WHERE THE FACTS COME FROM (it re-scrapes nothing that already has an owner)
# ---------------------------------------------------------------------------
#   * dev/plans/release-state-<release>.json (T2a) is the SINGLE WRITER for the
#     ladder, the landed slices + their landing SHAs, the SCHEMA version, the
#     next slice, the AC state and the ruled/unruled decision set. Every one of
#     those facts below is read from it — never re-derived from prose. Change a
#     fact THERE and the manifest follows.
#   * `status:` frontmatter on dev/design/**/*.md (T2c) selects the design tier,
#     so the design-of-record list is derived, not a hardcoded list.
#   * The plan's own `##` headings supply the section anchors, looked up by ROLE
#     keyword rather than by line number (T1d's ban) or by a frozen title string.
#
# ---------------------------------------------------------------------------
# HOW UNREVIEWED IS HANDLED (deliberate, and the honest part)
# ---------------------------------------------------------------------------
# T2c's backfill was conservative on purpose: most design docs are UNREVIEWED,
# which means "nobody has classified this yet" — NOT "this is current" — and
# scripts/lint-design-status.sh proves the PRESENCE of a status field, never its
# TRUTH (the classification is a separate owed slice, TC-50). So this manifest
# NEVER presents an UNREVIEWED doc as authoritative: each cited doc carries its
# recorded status inline, UNREVIEWED/UNKNOWN rows are explicitly marked
# unclassified with the TC-50 pointer, and the section header says the status is
# recorded rather than verified. The reader judges; the generator does not
# launder.
#
# ---------------------------------------------------------------------------
# WHAT IT ENFORCES (two hard checks + one guard)
# ---------------------------------------------------------------------------
# CHECK 1 — EVERY CITED PATH MUST EXIST. Always on, not a `--verify` extra. A
#   path that does not resolve is a HARD failure and the manifest is NOT emitted:
#   printing a brief with one dead citation in it is the artifact this replaces.
# CHECK 2 — EVERY CITED ANCHOR MUST OCCUR. Anchors are greppable section
#   headings/symbols, never `file:line` (T1d RULE 1), and each is verified by
#   literal match inside the file it names (T1d RULE 2). Swapping an unverified
#   NUMBER for an unverified SYMBOL would just launder a bad pointer.
# CHECK 3 — THE BASE SHA IS THE TARGET SLICE'S PREDECESSOR, never `max(landed)`.
#   For any slice that is not the next one, the newest landed slice can BE the
#   target (or later), and a manifest that says "branch from here" while naming
#   the target's own merge is the agent-worktree-stale-base trap in printed
#   form. §2 takes the greatest landed slice STRICTLY BELOW the target, marks a
#   regeneration of an already-landed slice as HISTORICAL, and says plainly
#   when no predecessor exists rather than emitting a blank or a wrong SHA.
# GUARD — TC-37 VACUOUS PASS (this repo's named failure class). Zero design
#   citations for the slice, zero citations overall, zero paths verified, or a
#   sweep that discovers zero state files: all HARD failures. A manifest that is
#   empty has briefed nobody while looking green.
#
# READ-ONLY: emits to stdout, writes no file into the repo. It records state and
# cites decisions; it changes no slot, scope, requirement, AC or ruling.
#
# Usage:
#   scripts/commission-manifest.sh <release> <slice|next> [--verify]
#   scripts/commission-manifest.sh --verify-all
# Exit: 0 = manifest emitted (or verified) with every path and anchor resolving;
#       1 = a dead citation, an unknown release/slice, or a vacuous manifest.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

usage() {
  cat <<'USAGE'
usage:
  scripts/commission-manifest.sh <release> <slice|next> [--verify]
  scripts/commission-manifest.sh --verify-all

  <release>   a release with a dev/plans/release-state-<release>.json (e.g. 0.8.20)
  <slice>     a slice number in that file's ladder, or the word `next`
  --verify    run the path/anchor existence checks and print only the counts
  --verify-all  verify the NEXT slice of every release-state file (CI shape)
USAGE
}

MODE="emit"
RELEASE=""
SLICE=""
for arg in "$@"; do
  case "$arg" in
    -h|--help) usage; exit 0 ;;
    --verify) MODE="verify" ;;
    --verify-all) MODE="verify-all" ;;
    -*) echo "FAIL commission-manifest: unknown option $arg" >&2; usage >&2; exit 1 ;;
    *)
      if [ -z "$RELEASE" ]; then RELEASE="$arg"
      elif [ -z "$SLICE" ]; then SLICE="$arg"
      else echo "FAIL commission-manifest: unexpected argument $arg" >&2; usage >&2; exit 1; fi ;;
  esac
done

if [ "$MODE" != "verify-all" ] && { [ -z "$RELEASE" ] || [ -z "$SLICE" ]; }; then
  echo "FAIL commission-manifest: a release AND a slice are required." >&2
  usage >&2
  exit 1
fi

# python3 carries the JSON parse, the design-tier scan and the citation checks.
# A gate that cannot run must fail loudly, never skip (TC-37).
if ! command -v python3 >/dev/null 2>&1; then
  echo "FAIL commission-manifest: python3 not found; the state-file parser and the" >&2
  echo "  citation existence checks cannot run. A tool that cannot check its own" >&2
  echo "  citations must not emit them (TC-37)." >&2
  exit 1
fi

MODE="$MODE" RELEASE="$RELEASE" SLICE="$SLICE" python3 - <<'PY'
import glob
import json
import os
import re
import sys

MODE = os.environ["MODE"]
RELEASE = os.environ["RELEASE"]
SLICE_SEL = os.environ["SLICE"]

STATE_GLOB = "dev/plans/release-state-*.json"

# Fixed infrastructure the manifest cites. These are SOURCES OF RECORD, not
# restatements: the manifest points at them and stops. Every one is existence-
# checked, so a rename fails here rather than being handed to an agent.
ORCH = "dev/design/orchestration.md"
HANDOFF = "dev/plans/prompts/0.8.x-RELEASE-ORCHESTRATOR-HANDOFF.md"
LBO_TEMPLATE = "dev/plans/prompts/LIBRARY-BUMP-ORCHESTRATOR-TEMPLATE.md"
SLICE_TEMPLATE = "dev/plans/prompts/0.8.0-SLICE-TEMPLATE.md"
PREFLIGHT = "scripts/preflight.sh"
ACCEPTANCE = "dev/acceptance.md"
AGENTS = "AGENTS.md"
SURFACE_ALLOWLIST = "src/conformance/governed-surface-allowlist.json"
SURFACE_PIN = "scripts/governed-surface-pin.json"
CODEX_DIR = "dev/plans/runs/codex"
DESIGN_ROOT = "dev/design"

# T1d RULE 1's shape, applied to this tool's OWN output: a manifest that emitted
# a `name:123` pointer would reintroduce the class T1d closed.
LINE_ANCHOR_RE = re.compile(r"`[A-Za-z_][A-Za-z0-9_.+-]*:[0-9]{3,}(-[0-9]+)?\+?`")

# Status ordering for the design tier. Lower sorts first. UNREVIEWED/UNKNOWN sit
# BELOW every classified value on purpose — they are queue markers, not verdicts.
STATUS_RANK = {
    "ACTIVE": 0, "locked": 1, "accepted": 1, "adopted": 1, "ratified": 1,
    "signed": 1, "proposal": 3, "proposed": 3, "COMPLETE": 2, "PROPOSED": 3,
    "UNKNOWN": 8, "UNREVIEWED": 9, "SUPERSEDED": 10,
}
UNCLASSIFIED = ("UNREVIEWED", "UNKNOWN")


def die(lines):
    for ln in lines:
        print(ln, file=sys.stderr)
    sys.exit(1)


def read(path):
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        return fh.read()


def frontmatter_status(path):
    """Parse `status:` exactly as scripts/lint-design-status.sh does."""
    try:
        text = read(path)
    except OSError:
        return None
    lines = text.split("\n")
    if not lines or lines[0].strip() != "---":
        return None
    block = []
    for ln in lines[1:]:
        if ln.rstrip() == "---":
            break
        block.append(ln)
    else:
        return None
    for ln in block:
        m = re.match(r"^status:\s*(.*?)\s*$", ln)
        if m and m.group(1):
            return m.group(1)
    return None


class Manifest:
    """Accumulates the emitted lines AND the citations they make, so the
    existence checks run over exactly what the brief says — not a parallel list
    that could drift from it."""

    def __init__(self):
        self.lines = []
        self.cites = []      # (path, needle|None, label)
        self.problems = []

    def out(self, text=""):
        self.lines.append(text)

    def cite(self, path, needle=None, label=""):
        self.cites.append((path, needle, label))
        if needle is not None:
            return "`%s` in `%s`" % (needle, path)
        return "`%s`" % path

    def anchor(self, path, keywords, role, required=True):
        """Resolve a section anchor by ROLE keyword against the file's own
        headings. Never a line number (T1d RULE 1); always verified (RULE 2)."""
        try:
            text = read(path)
        except OSError:
            self.problems.append(
                "FAIL commission-manifest: cannot read `%s` while resolving the %s anchor."
                % (path, role))
            self.cites.append((path, None, role))
            return None
        for ln in text.split("\n"):
            if not ln.startswith("#"):
                continue
            low = ln.lower()
            if all(k in low for k in keywords):
                return self.cite(path, ln.strip(), role)
        if required:
            self.problems.append(
                "FAIL commission-manifest: `%s` has no heading matching %s (role %s).\n"
                "  The section was renamed or removed, so the brief would cite a heading\n"
                "  that no longer exists. Re-point the role or restore the heading."
                % (path, " + ".join("'%s'" % k for k in keywords), role))
        return None


def load_state(release):
    path = "dev/plans/release-state-%s.json" % release
    known = sorted(
        re.sub(r"^dev/plans/release-state-(.*)\.json$", r"\1", p)
        for p in glob.glob(STATE_GLOB))
    if not known:
        die(["FAIL commission-manifest: ZERO release-state files under %s." % STATE_GLOB,
             "  There is nothing to generate a manifest FROM, so exiting 0 would vouch",
             "  for nothing (TC-37). Either the state files moved or this is not the repo."])
    if not os.path.isfile(path):
        die(["FAIL commission-manifest: unknown release '%s' — no release-state file at" % release,
             "  `%s`. Releases that DO have one: %s." % (path, ", ".join(known)),
             "  The state file is the single writer (T2a); a manifest is not generated",
             "  from prose."])
    try:
        return path, json.loads(read(path))
    except (OSError, ValueError) as exc:
        die(["FAIL commission-manifest: `%s` is not parseable as JSON: %s" % (path, exc)])


def pick_slice(state, state_path, sel):
    ladder = state.get("ladder") or []
    known = [e.get("slice") for e in ladder]
    if not ladder:
        die(["FAIL commission-manifest: `%s` declares an EMPTY ladder." % state_path,
             "  A manifest cannot be generated for a release with no slices (TC-37)."])
    if str(sel).lower() == "next":
        sel = state.get("next_slice")
        if sel is None:
            die(["FAIL commission-manifest: `%s` names no `next_slice`." % state_path])
    try:
        want = int(sel)
    except (TypeError, ValueError):
        die(["FAIL commission-manifest: slice '%s' is not a number (or the word `next`)." % sel])
    for entry in ladder:
        if entry.get("slice") == want:
            return want, entry
    die(["FAIL commission-manifest: slice %s is not in the %s ladder declared by `%s`."
         % (want, state.get("release", "?"), state_path),
         "  Known slices: %s." % ", ".join(str(s) for s in known)])


def slice_tokens(entry):
    """Derive the search tokens for the design tier from T2a's OWN fields, so the
    required-reading list follows the state file rather than a hardcoded map.
    Shapes only — snake_case identifiers and the repo's structured ids."""
    blob = "%s %s" % (entry.get("short") or "", entry.get("title") or "")
    pats = [
        r"\b[a-z][a-z0-9]*(?:_[a-z0-9]+)+\b",   # dense_readiness, flush_embeddings
        r"\bTC-[0-9]+\b",                        # carry/todo ids
        r"\bR-[0-9]+-[A-Za-z0-9]+\b",            # requirement ids: R-20-DR
        r"\bC-[0-9]+\b",                         # contract ids
        r"\bAC-[0-9]+\b",
        r"\bRUBRIC-[A-Za-z0-9]+\b",
        r"\b[A-Z]{2,}-[0-9]+\b",                 # OPP-12, EXP-…
    ]
    tokens = []
    for pat in pats:
        for tok in re.findall(pat, blob):
            if tok not in tokens:
                tokens.append(tok)
    return tokens


def scan_design(tokens, release, slice_no):
    # Second selector, alongside the token scan: a doc whose FILENAME names this
    # release AND this slice is that slice's own design memo (SLICE-TEMPLATE §3.0
    # writes one per slice). The release is required in the name precisely
    # because `dev/design/slice-30-design.md` is 0.8.0's — pulling it into a
    # 0.8.20 Slice-30 brief would be a wrong citation that still resolves.
    name_re = re.compile(r"slice[-_ ]?0*%d(?![0-9])" % slice_no, re.I)
    hits = []
    for root, dirs, files in os.walk(DESIGN_ROOT):
        dirs.sort()
        for name in sorted(files):
            if not name.endswith(".md"):
                continue
            path = os.path.join(root, name)
            try:
                text = read(path)
            except OSError:
                continue
            matched = [t for t in tokens if t in text]
            if release in name and name_re.search(name):
                matched.append("filename: %s slice %s" % (release, slice_no))
            if not matched:
                continue
            status = frontmatter_status(path) or "(no status:)"
            hits.append((STATUS_RANK.get(status.split()[0], 7), path, status, matched))
    hits.sort(key=lambda h: (h[0], h[1]))
    return hits


def build(release, state_path, state, slice_no, entry):
    m = Manifest()
    ladder = {e.get("slice"): e for e in state.get("ladder") or []}
    landed = state.get("landed") or []
    next_slice = state.get("next_slice")
    plan = state.get("plan") or ""
    board = state.get("board") or ""
    master = state.get("master") or ""

    tokens = slice_tokens(entry)
    design = scan_design(tokens, release, slice_no)

    # ---- 1. ASSIGNMENT (LBO "Assignment"; SLICE-TEMPLATE heading + §2) -----
    m.out("=" * 100)
    m.out("COMMISSION MANIFEST — %s Slice %s (%s)" % (release, slice_no, entry.get("short") or "?"))
    m.out("=" * 100)
    m.out("GENERATED, READ-ONLY. It CITES sources of record; it restates no ruling and decides")
    m.out("nothing. Facts come from the single-writer state file %s" % m.cite(state_path))
    m.out("(T2a) and from `status:` frontmatter under `%s/` (T2c). Section set derived from" % DESIGN_ROOT)
    m.out("%s" % m.cite(LBO_TEMPLATE))
    m.out("and %s — this REPLACES those as the brief" % m.cite(SLICE_TEMPLATE))
    m.out("assembly step; it does not add a third template to maintain.")
    m.out()
    m.out("## 1. ASSIGNMENT   [LBO 'Assignment (filled by LBS)'; SLICE-TEMPLATE title line + §2]")
    m.out("  release          %s (%s)" % (release, state.get("release_kind") or "kind not stated"))
    m.out("  slice            %s — %s" % (slice_no, entry.get("title") or "(no title)"))
    m.out("  requirement id   %s" % (entry.get("short") or "(none)"))
    m.out("  ladder status    %s" % (entry.get("status") or "?"))
    m.out("  depends on       %s" % (", ".join(
        "Slice %s [%s%s]" % (d, (ladder.get(d) or {}).get("status", "UNKNOWN"),
                             ", %s" % (ladder.get(d) or {}).get("sha")
                             if (ladder.get(d) or {}).get("sha") else "")
        for d in (entry.get("depends_on") or [])) or "— (none)"))
    if next_slice == slice_no:
        m.out("  sequencing       ✅ this IS the next slice per the state file")
    else:
        m.out("  sequencing       ⚠ this is NOT the next slice — the state file says next = %s"
              % next_slice)
    m.out("  plan             %s" % m.cite(plan))
    m.out("  board of record  %s" % m.cite(board))
    m.out("  master           %s" % m.cite(master))
    m.out("  codex §9         transcripts go under %s, release-namespaced (TC-RUBRIC-7)"
          % m.cite(CODEX_DIR))
    m.out()

    # ---- 2. BASE SHA (LBO "Branch from tip"; SLICE-TEMPLATE §0) ------------
    # The base is the TARGET SLICE'S PREDECESSOR: the greatest LANDED slice
    # STRICTLY BELOW slice_no. It is NOT max(landed). For any slice that is not
    # the next one — a historical regeneration, an out-of-order brief — max()
    # can select the target's OWN landing merge or a later one, i.e. tell an
    # operator to branch from the very work they are being commissioned to do.
    # A wrong branch point is this repo's named agent-worktree-stale-base trap
    # (it has already cost two slices), and a confidently printed wrong SHA is
    # exactly what defeats the human sanity check, so this is computed
    # relative to the target and never to the tip of `landed`.
    #
    # `depends_on` is deliberately NOT the base. Dependencies state the MINIMUM
    # ancestry a slice needs; branching from that minimum would silently drop
    # every slice landed since — the same stale-base trap from the other side.
    # The predecessor is the ancestry-maximal choice that still excludes the
    # target itself, so dependencies are CROSS-CHECKED against it instead.
    m.out("## 2. BASE SHA + BRANCH POINT   [LBO 'Branch from tip'; SLICE-TEMPLATE §0]")
    landed_nums = sorted({s for s in landed if isinstance(s, int)})
    prior_landed = [s for s in landed_nums if s < slice_no]
    later_landed = [s for s in landed_nums if s > slice_no]
    base_slice = max(prior_landed) if prior_landed else None
    base_sha = (ladder.get(base_slice) or {}).get("sha") if base_slice is not None else None
    if slice_no in landed_nums:
        m.out("  ⚠ HISTORICAL     Slice %s is ITSELF LANDED (%s), so this is a regeneration:"
              % (slice_no, (ladder.get(slice_no) or {}).get("sha") or "no sha recorded"))
        m.out("                   the base below is that slice's PREDECESSOR, never its own merge.")
    if base_sha:
        m.out("  base sha         %s  (Slice %s — the newest LANDED slice STRICTLY BEFORE Slice %s)"
              % (base_sha, base_slice, slice_no))
    elif base_slice is not None:
        m.out("  base sha         (Slice %s is the predecessor but records NO landing sha)"
              % base_slice)
        m.out("                   The state file is incomplete: take the branch point from")
        m.out("                   `git log` for that landing and record it in output.json. Do NOT guess.")
    else:
        m.out("  base sha         (none — NO landed slice precedes Slice %s)" % slice_no)
        # These two cases are MUTUALLY EXCLUSIVE. "No predecessor" means branch
        # from the tip ONLY while the tip is still where this slice was cut. Once
        # later slices have landed, origin/main carries their merges, so printing
        # the branch-from-tip instruction beside the TIP IS AHEAD warning emits
        # precisely the stale-base instruction this section exists to prevent —
        # and an operator follows the instruction, not the caveat next to it.
        if later_landed:
            m.out("  ⚠ TIP IS AHEAD   Slice(s) %s landed AFTER this one, so origin/main already"
                  % ", ".join(str(s) for s in later_landed))
            m.out("                   carries work Slice %s never had. There is NO correct automatic"
                  % slice_no)
            m.out("                   base for this regeneration: do NOT branch from the tip. Recover")
            m.out("                   the point of cut from `git log` and record it in output.json.")
        else:
            m.out("                   This is the first slice of the release to be cut, so there is no")
            m.out("                   predecessor merge: branch from `git rev-parse origin/main` at STEP 0.")
    dep_gap = [d for d in (entry.get("depends_on") or [])
               if (ladder.get(d) or {}).get("status") == "LANDED"
               and (base_slice is None or d > base_slice)]
    if dep_gap:
        m.out("  ⚠ DEP NOT IN BASE dependency Slice(s) %s are LANDED but do NOT sit at or below the"
              % ", ".join(str(d) for d in dep_gap))
        m.out("                   base slice, so the base merge does not contain them. Re-derive the")
        m.out("                   branch point from `git log` before cutting the worktree.")
    m.out("  re-verify        `git rev-parse origin/main` at STEP 0 — a later landing may have")
    m.out("                   advanced the tip since the state file was written.")
    m.out("  landed so far    %s" % (", ".join(
        "%s (%s)%s" % (s, (ladder.get(s) or {}).get("sha") or "no sha",
                       " ⚠ at/after Slice %s" % slice_no if isinstance(s, int) and s >= slice_no else "")
        for s in landed) or "none"))
    m.out("  SCHEMA           %s — %s"
          % (state.get("schema_version"),
             m.cite(schema_src_path(state), "pub const SCHEMA_VERSION", "schema source")))
    m.out()

    # ---- 3. WORKTREE RULES (LBO STEP 0; SLICE-TEMPLATE §0) ----------------
    m.out("## 3. WORKTREE RULES   [LBO 'STEP 0 — Isolate'; SLICE-TEMPLATE §0] — cite, do not restate")
    for label, kw in (("preflight gate ", ["preflight gate"]),
                      ("hard rules     ", ["hard rules"]),
                      ("worktree cleanup", ["worktree cleanup"])):
        a = m.anchor(ORCH, kw, label.strip())
        if a:
            m.out("  %s %s" % (label, a))
    for label, kw in (("session preflight", ["hard preflight checks"]),
                      ("mechanics       ", ["orchestration mechanics"])):
        a = m.anchor(HANDOFF, kw, label.strip())
        if a:
            m.out("  %s %s" % (label, a))
    m.out("  TC-RUBRIC-5      all landing git-writes run in a dedicated linked worktree;")
    m.out("                   %s hard-fails on the primary checkout"
          % m.cite(PREFLIGHT, "--landing", "TC-RUBRIC-5"))
    a = m.anchor(LBO_TEMPLATE, ["step 0"], "LBO isolate step")
    if a:
        m.out("  isolate step     %s" % a)
    m.out()

    # ---- 4. CONTRACT PATHS (SLICE-TEMPLATE §1 {{CONTRACT_REF}}) -----------
    m.out("## 4. CONTRACT PATHS   [SLICE-TEMPLATE §1.2 {{CONTRACT_REF}} — on any conflict with a")
    m.out("   brief, the CONTRACT wins and the conflict is flagged in output.json]")
    a = m.anchor(plan, ["requirements"], "requirements + ACs")
    if a:
        m.out("  requirements/ACs %s" % a)
    m.out("  AC register      %s" % m.cite(ACCEPTANCE))
    m.out("  invariants       %s (TDD mandatory; no DB mocking; never --no-verify)" % m.cite(AGENTS))
    m.out("  governed surface %s" % m.cite(SURFACE_ALLOWLIST))
    m.out("                   pinned by %s" % m.cite(SURFACE_PIN))
    for _rank, path, status, _matched in design:
        if "contract" in os.path.basename(path).lower():
            m.out("  design contract  [%s] %s" % (status, m.cite(path)))
    m.out()

    # ---- 5. PLAN SECTION ANCHORS (no line numbers — T1d) ------------------
    m.out("## 5. PLAN SECTION ANCHORS   [{{MANDATE}}/{{DEFERRED}}/{{DOD}}/{{GUARDRAILS}}] — greppable")
    m.out("   headings, never `file:line` (T1d ban); each is verified to occur in the file it names.")
    roles = [
        ("mandate    ", ["immediate next slice"], True),
        ("ladder     ", ["slice ladder"], True),
        ("scope      ", ["goal", "scope"], True),
        ("deferred   ", ["reserved-gap"], True),
        ("cross-cut DoD", ["cross-cutting dod"], True),
        ("do-not-relitigate", ["decisions already taken"], False),
        ("prerequisites", ["prerequisites"], False),
        ("rulings    ", ["decisions taken"], False),
        ("open queue ", ["open hitl"], False),
    ]
    for label, kw, required in roles:
        a = m.anchor(plan, kw, label.strip(), required=required)
        if a:
            m.out("  %-18s %s" % (label.strip(), a))
    m.out()

    # ---- 6. DESIGN DOCS ({{READING}}) -------------------------------------
    m.out("## 6. DESIGN DOCS FOR THIS SLICE   [SLICE-TEMPLATE §1.4 {{READING}}]")
    m.out("   Selected by the slice's OWN tokens from the state file: %s" % ", ".join(tokens))
    m.out("   Status is AS RECORDED in `status:` frontmatter (T2c) — it is NOT a currency claim.")
    m.out("   scripts/lint-design-status.sh proves the PRESENCE of a status, never its TRUTH;")
    m.out("   UNREVIEWED means 'nobody has classified this yet' and the classification is owed")
    m.out("   at TC-50. Rows marked ⚠ unclassified are candidates, NOT design of record.")
    for _rank, path, status, matched in design:
        flag = " ⚠ unclassified (TC-50)" if status.split()[0] in UNCLASSIFIED else ""
        m.out("  [%s]%s %s" % (status, flag, m.cite(path)))
        m.out("        matched: %s" % ", ".join(matched))
    classified = [h for h in design if h[2].split()[0] not in UNCLASSIFIED]
    m.out("  -> %d doc(s) matched: %d classified, %d unclassified."
          % (len(design), len(classified), len(design) - len(classified)))
    matched_tokens = {t for _r, _p, _s, ms in design for t in ms}
    unmatched = [t for t in tokens if t not in matched_tokens]
    if unmatched:
        m.out("  -> NO design doc mentions: %s. That part of the slice has no design of record;"
              % ", ".join(unmatched))
        m.out("     its authority is whatever the plan's rulings/requirements sections say — read")
        m.out("     them there, and do not infer a design that does not exist.")
    m.out()

    # ---- 7. ACCEPTANCE ({{AC_IDS}}) ---------------------------------------
    acc = state.get("acceptance") or {}
    gate = acc.get("publish_gate") or {}
    m.out("## 7. ACCEPTANCE STATE   [SLICE-TEMPLATE §3.1 {{AC_IDS}}] — from the state file, verbatim")
    m.out("  highest defined non-reserved AC   %s" % acc.get("highest_defined_non_reserved", "—"))
    for res in acc.get("reservations") or []:
        m.out("  reserved                          %s [%s] %s"
              % (res.get("ac"), res.get("state"), res.get("initiative") or ""))
    if gate:
        m.out("  publish gate                      %s — %s, minted=%s, signed at Slice %s (board %s)"
              % (gate.get("ac"), gate.get("state_word"), gate.get("minted"),
                 gate.get("sign_off_slice"), gate.get("board_ref")))
    if acc.get("re_verified_green"):
        m.out("  re-verified green                 %s" % ", ".join(acc["re_verified_green"]))
    m.out("  Mint no AC and change no AC here: %s is the register and the plan's" % m.cite(ACCEPTANCE))
    m.out("  requirements section is the contract.")
    m.out()

    # ---- 8. STOP CONDITIONS -----------------------------------------------
    m.out("## 8. STOP CONDITIONS   [LBO 'Escalate to LBS'; SLICE-TEMPLATE §6/§7; orchestration §10]")
    dec = state.get("decisions") or {}
    for d in dec.get("unruled") or []:
        verb = "⛔ HALT TO HITL" if d.get("halts_run") else "⚠ GATED (does not halt this run)"
        m.out("  %s — %s" % (verb, d.get("title") or d.get("id")))
        m.out("      gated at %s · source: %s" % (d.get("gated_at") or "?", d.get("source") or "?"))
    pubslice = state.get("publish_precondition_slice")
    if pubslice is not None:
        m.out("  ⛔ Slice %s is a PUBLISH PRECONDITION — absent-or-failing holds the release."
              % pubslice)
    for d in entry.get("depends_on") or []:
        dep = ladder.get(d) or {}
        if dep.get("status") != "LANDED":
            m.out("  ⛔ NOT COMMISSIONABLE YET — dependency Slice %s is %s, not LANDED."
                  % (d, dep.get("status")))
    m.out("  Ruled decisions are CITED, never re-decided or restated: %d ruling(s) recorded in the"
          % len(dec.get("ruled") or []))
    m.out("  state file's `decisions.ruled` with their sources; read them there.")
    for label, path, kw in (("hard rules", ORCH, ["hard rules"]),
                            ("scope discipline", SLICE_TEMPLATE, ["scope discipline"]),
                            ("recovery", SLICE_TEMPLATE, ["when something goes wrong"]),
                            ("escalate", LBO_TEMPLATE, ["escalate to lbs"])):
        a = m.anchor(path, kw, label)
        if a:
            m.out("  %-17s %s" % (label, a))
    m.out()

    # ---- 9. CLOSURE (orchestration §8; LBO 'Closure output') --------------
    m.out("## 9. CLOSURE   [orchestration §8 schema; LBO 'Closure output'; SLICE-TEMPLATE §9]")
    a = m.anchor(ORCH, ["closure output"], "closure schema")
    if a:
        m.out("  output.json      %s" % a)
    a = m.anchor(SLICE_TEMPLATE, ["closure"], "slice closure", required=False)
    if a:
        m.out("  slice closure    %s" % a)
    m.out("  write it LAST, after every commit, and report the head SHA to the orchestrator.")
    m.out()
    return m


def schema_src_path(state):
    """The state file states the SCHEMA source as prose ('pub const X in <path>');
    take the path out of it rather than hardcoding a second copy."""
    src = state.get("schema_version_source") or ""
    for tok in src.split():
        if "/" in tok:
            return tok
    return "src/rust/crates/fathomdb-schema/src/lib.rs"


def verify(m, release, slice_no):
    """CHECK 1 + CHECK 2 + the TC-37 guard. Returns (paths, anchors) verified."""
    problems = list(m.problems)
    paths_ok = 0
    anchors_ok = 0
    seen = set()
    for path, needle, label in m.cites:
        exists = os.path.exists(path)   # MUTATION POINT (see the test suite)
        if not exists:
            problems.append(
                "FAIL commission-manifest: cited path `%s` does NOT exist in the worktree\n"
                "  (citation role: %s). The file was renamed, moved or deleted; a brief built\n"
                "  on it would send an agent to a dead path." % (path, label or "unlabelled"))
            continue
        if path not in seen:
            seen.add(path)
            paths_ok += 1
        if needle is not None:
            if os.path.isdir(path):
                problems.append(
                    "FAIL commission-manifest: `%s` is a directory but the citation names the\n"
                    "  symbol `%s` (role: %s)." % (path, needle, label))
                continue
            if needle not in read(path):
                problems.append(
                    "FAIL commission-manifest: cited symbol `%s`\n"
                    "  does NOT occur in `%s` (role: %s). The heading or symbol was renamed —\n"
                    "  an unverified pointer must never be laundered as a verified one."
                    % (needle, path, label))
                continue
            anchors_ok += 1

    body = "\n".join(m.lines)
    hit = LINE_ANCHOR_RE.search(body)
    if hit:
        problems.append(
            "FAIL commission-manifest: the manifest emitted a bare line anchor %s.\n"
            "  `<name>:<line>` pointers are banned (T1d): emit a greppable section anchor."
            % hit.group(0))

    if not m.cites:
        problems.append(
            "FAIL commission-manifest: ZERO citations emitted for %s Slice %s.\n"
            "  An empty manifest briefs nobody while looking green — the TC-37 vacuous-pass\n"
            "  class. This is a hard failure, never an exit 0." % (release, slice_no))
    if paths_ok == 0 and not problems:
        problems.append(
            "FAIL commission-manifest: ZERO paths verified for %s Slice %s (TC-37)."
            % (release, slice_no))
    return paths_ok, anchors_ok, problems


def generate(release, slice_sel):
    state_path, state = load_state(release)
    slice_no, entry = pick_slice(state, state_path, slice_sel)

    tokens = slice_tokens(entry)
    if not scan_design(tokens, release, slice_no):
        die(["FAIL commission-manifest: ZERO design docs matched %s Slice %s (tokens: %s)."
             % (release, slice_no, ", ".join(tokens) or "none derived"),
             "  The required-reading list would be EMPTY, so the manifest would brief nobody",
             "  while exiting 0 — this repo's named TC-37 vacuous-pass class. Fix it by",
             "  authoring/citing the design of record for this slice, or by naming the slice's",
             "  requirement id in the state file's `short`/`title` so the scan can find it."])

    m = build(release, state_path, state, slice_no, entry)
    paths_ok, anchors_ok, problems = verify(m, release, slice_no)
    if problems:
        problems.append(
            "\ncommission-manifest: %d dead citation(s) for %s Slice %s — manifest NOT emitted."
            % (len(problems), release, slice_no))
        die(problems)
    return slice_no, entry, m, paths_ok, anchors_ok


if MODE == "verify-all":
    states = sorted(glob.glob(STATE_GLOB))
    if not states:
        die(["FAIL commission-manifest: ZERO release-state files under %s." % STATE_GLOB,
             "  The sweep loop never ran, so exiting 0 would vouch for nothing (TC-37)."])
    for sp in states:
        rel = re.sub(r"^dev/plans/release-state-(.*)\.json$", r"\1", sp)
        sl, entry, m, paths_ok, anchors_ok = generate(rel, "next")
        print("commission-manifest: %s next slice %s (%s) — %d path(s) and %d anchor(s) verified."
              % (rel, sl, entry.get("short") or "?", paths_ok, anchors_ok))
    sys.exit(0)

slice_no, entry, m, paths_ok, anchors_ok = generate(RELEASE, SLICE_SEL)
if MODE == "verify":
    print("commission-manifest: %s Slice %s (%s) — %d path(s) and %d anchor(s) verified, 0 dead."
          % (RELEASE, slice_no, entry.get("short") or "?", paths_ok, anchors_ok))
    sys.exit(0)

print("\n".join(m.lines))
print("-" * 100)
print("PATH VERIFICATION: %d distinct path(s) exist, %d anchor(s) verified to occur, 0 dead."
      % (paths_ok, anchors_ok))
print("Every citation above was checked at generation time. Re-run before reusing this brief.")
PY
