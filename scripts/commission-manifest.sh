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
# THE DESIGN TIER HAS TWO HALVES: DISCOVERY, AND CITATION (`design_refs`)
# ---------------------------------------------------------------------------
# The discovery half is the pair of selectors in scan_design(): a doc whose
# FILENAME names this release AND this slice, and a doc whose CONTENT mentions a
# token derived from the ladder entry's own `short`/`title`. Both are measured,
# and both MISS AN ENTIRE CLASS OF SLICE BY CONSTRUCTION:
#   * The filename selector hit 1 of the 12 slices in the 0.8.20 ladder.
#   * `plan-<release>.md` §3 is "frozen at Slice 0", so a RESERVED-GAP slice
#     minted mid-release by HITL ruling has requirement ids (R-20-CR, R-20-VC,
#     R-20-SV and their TC-* carries) that appear NOWHERE under dev/design/ —
#     the design package predates them — and it never got a §3.0 memo of its
#     own either.
# The TC-37 vacuous-pass guard therefore hard-failed on three well-designed
# slices in a row: 21, 22 and 23 (tracked as TC-92 and TC-94).
#
# `design_refs` on a ladder entry is the CITATION half. The Steward names the
# docs; nothing is inferred. Four properties are deliberate:
#   1. OPTIONAL AND INERT. No key ⇒ byte-for-byte the behaviour that predates
#      it. It adds a tier; it does not reinterpret the existing one.
#   2. CHECKED LIKE EVERY OTHER PATH. A curated citation carries the Steward's
#      authority, so a dead one misleads MORE than a dead scan hit, not less:
#      CHECK 1 applies with no exemption and the manifest is not emitted.
#   3. IT REACHES ANYTHING IN THE CHECKOUT, including tiers the walker does not
#      cover. (When this was written the walker covered `dev/design/` alone and
#      this was the ONLY route to `dev/adr/**` and `dev/interfaces/**`; TC-94
#      defect 1 has since put those two tiers inside the scan — see
#      DESIGN_ROOTS — so curation is no longer the only way to reach them. It is
#      still the only way to reach anything else, and it is still the only way to
#      put a BYTE-PINNED contract in front of an orchestrator, since a pinned
#      file can never be back-linked.) Curated docs often carry no `status:`
#      frontmatter at all, and the row then SAYS there is none rather than
#      inventing one, exactly as the UNREVIEWED rows refuse to launder.
#   4. IT IS NEVER BLENDED WITH THE SCAN. Curated rows are labelled CURATED and
#      kept in their own block, so a reader can always tell a doc somebody chose
#      from a doc a substring match found.
# It does NOT weaken the guard: an empty/absent `design_refs` AND an empty scan
# is still a hard failure.
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

# THE ROOTS THE DISCOVERY SCAN WALKS. Exactly three, and the list is a bound, not
# a starting point (TC-94 defect 1).
#
# It used to be `dev/design` alone, which made `dev/adr/**` and
# `dev/interfaces/**` unreachable BY CONSTRUCTION, at any status, no matter what
# was back-linked into them. That excluded precisely the strongest evidence: an
# ADR is a HIGHER tier of authority than a design memo
# (`ADR-0.6.0-error-taxonomy.md` is the ruling document for decision #18;
# `ADR-0.8.18-vector-equivalence-self-check.md` is HITL-SIGNED), and
# `dev/interfaces/*.md` is the surface AGENTS.md obliges an error-surface change
# to update — an obligation TC-39 already records as routinely missed.
#
# WHY NOT WIDER. Walking all of `dev/` would drag planning notes, run artifacts
# and hand-off memos into every brief; `dev/plans/**` alone is larger than the
# design tier and is narrative, not authority. The three roots below are the tiers
# that CARRY design authority, and the bound is asserted (test arm 12c3).
#
# WHY THIS IS SAFE ONLY ALONGSIDE THE WORD-BOUNDARY FIX. Widening the walker
# under substring matching would have multiplied TC-100's spurious hits across
# three times as many documents. The two changes ship together for that reason.
DESIGN_ROOTS = ("dev/design", "dev/adr", "dev/interfaces")
# The primary root, used where the prose names one tree (the `status:` frontmatter
# convention T2c established lives under it).
DESIGN_ROOT = DESIGN_ROOTS[0]

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


def slice_str(n):
    """Render a slice id: 40 -> "40", 39.5 -> "39.5", 40.0 -> "40".

    NEVER `%d`. `"%d" % 39.5` is `"39"`, and a truncated slice id in this file is
    not a cosmetic defect — it is a manifest confidently naming the WRONG UNIT,
    which is the fabricated-pointer incident class the whole unit that added this
    helper was commissioned over. `str()` alone is not enough either: it renders
    an integral float as `"40.0"`, so the same fact reaches two readers in two
    shapes. Same contract as check-release-state-views.sh's `_slice_str`.
    """
    return "%g" % n if isinstance(n, float) else str(n)


def is_slice_num(n):
    """Is `n` a slice id? int OR float — the HITL may mint a fractional slice
    (steward seq-202/seq-204). `bool` is excluded because `True` is an `int` in
    Python and would otherwise be admitted as Slice 1.

    THE MEASURED DEFECT this replaces: `isinstance(s, int)` filtered a fractional
    slice OUT of the landed set, so the base SHA silently skipped a whole unit's
    work and told the operator to branch from a commit that predates it — the
    named agent-worktree-stale-base trap, produced by the tool built to prevent
    it (arm 13a).
    """
    return not isinstance(n, bool) and isinstance(n, (int, float))


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
    # int OR float: the HITL may mint a fractional slice (steward seq-202 named
    # "Slice 39.5" as a deliberate one-off). Parse as int when integral so the
    # ladder-membership comparison below still matches an integer-keyed entry,
    # and NEVER int() a fraction — 39.5 -> 39 would silently select the WRONG
    # slice and generate a manifest for a slice the caller did not ask for.
    try:
        want = float(sel)
        if want.is_integer() and "." not in str(sel):
            want = int(want)
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
        # BARE-NUMBER LEGS (TC-94 defect 2). `#18` and `#99` matched none of the
        # patterns above, so two of 0.8.20 Slice 22's four legs — "Slice-15b
        # decision #18 error-variant" and "sqlite-vec #99 vec0 DELETE probe" —
        # contributed NOTHING to selection and no document could ever be matched
        # for them.
        #
        # TWO OR MORE DIGITS, and that floor is measured rather than tidy.
        # Word-bounded across the three roots at the time of this change: `#18`
        # occurs in 6 documents (errors.md, the sqlite-vec #99 probe memo,
        # ADR-0.6.0-error-taxonomy.md and all three dev/interfaces surfaces) and
        # `#99` in 3 — every one genuinely about that decision or that upstream
        # issue. `#3` occurs in 11 and `#1` in 16, none of them a leg: they are
        # prose ordinals ("item #3").
        #
        # THE COST OF ONE DIGIT IS NOT NOISE, IT IS THE GUARD. Slices 31/32/33
        # are titled "Library Sweep #3, leg N of 3" — a sweep ordinal, not a
        # design id. Slices 32 and 33 have no design of record yet and correctly
        # HARD-FAIL the TC-37 vacuous-pass guard; a one-digit token would have
        # matched them 11 incidental documents apiece and turned that honest hard
        # failure into a brief that merely LOOKS supported. That is TC-100's
        # disease reintroduced by TC-94 (2)'s cure, so the floor stays.
        r"(?<![0-9A-Za-z_])#[0-9]{2,}(?![0-9A-Za-z_])",
    ]
    tokens = []
    for pat in pats:
        for tok in re.findall(pat, blob):
            if tok not in tokens:
                tokens.append(tok)
    return tokens


def token_re(tok):
    """Compile a WHOLE-TOKEN matcher for `tok` (TC-100).

    The predecessor was `t in text`, a bare substring test. MEASURED: `"C-1" in
    "TC-15"` is True, so 0.8.20 Slice 15 cited EIGHTEEN documents on that one
    token — among them `0.8.4-graphrag-sensemaking.md` (which only ever says
    `C-10` and `C-15`) and four `fathomdb-memex-overall-roadmap/*` drafts. A
    required-reading list padded with documents that cannot inform the slice
    costs the orchestrator context AND makes thin coverage read as thorough,
    which is the exact condition the TC-37 guard exists to surface. Short ids are
    the hazard: `C-1`, `C-2`, and any `AC-NN`/`TC-NN` that is a prefix of a
    longer id.

    The boundaries are spelled out rather than written `\\b` because a token may
    BEGIN with a non-word character (`#18`), where `\\b` asserts the opposite of
    what is wanted. Over tokens that start and end in word characters this is
    exactly `\\b...\\b`; over `#18` it correctly requires that the `#` not be
    glued to a preceding word character and that the digits not run on into
    `#180`.

    Both sides matter and both are asserted (test arms 12a/12b/12d2): guarding
    only the leading edge would still match `TC-99` inside `TC-990`.
    """
    return re.compile(r"(?<![0-9A-Za-z_])" + re.escape(tok) + r"(?![0-9A-Za-z_])")


def curated_design_refs(entry, state_path, slice_no):
    """The CITATION half of the design tier: `design_refs` on a ladder entry.

    OPTIONAL. Absent means absent — the caller emits exactly what it emitted
    before this existed. Present, it is a list of repo-relative paths that the
    Steward chose for this slice, in the order they should be read.

    Everything about it is validated HERE and fatally, because the failure mode
    of a silently-dropped curation is that the slice falls straight back into
    the TC-37 vacuous-pass hard-failure this key was added to fix — with no
    explanation of why the curation the author wrote did nothing. A typo must
    say so. Two rules beyond "it is a list of non-empty strings":
      * REPO-RELATIVE ONLY. Every other cited path in the manifest is relative
        to the checkout root; an absolute path, or one that climbs out with
        `..`, would resolve on the author's machine and nowhere else — a
        citation that passes CHECK 1 for one person only is worse than a dead
        one, because it goes green.
      * DE-DUPLICATED, ORDER PRESERVED. A repeated path is the author's slip,
        not a second reading; printing the row twice would inflate the tier.
    Existence is NOT checked here: it is checked by CHECK 1 over the emitted
    citation list, so a curated path fails through the same machinery, with the
    same message, as every other path the brief names.
    """
    if "design_refs" not in entry:
        return []
    raw = entry.get("design_refs")
    where = "ladder entry for Slice %s in `%s`" % (slice_no, state_path)
    if not isinstance(raw, list):
        die(["FAIL commission-manifest: `design_refs` in the %s is %s, not a list of"
             % (where, type(raw).__name__),
             "  repo-relative paths. A malformed curation must fail loudly: dropping it",
             "  silently puts the slice back into the TC-37 vacuous-pass failure that",
             "  `design_refs` exists to fix, with nothing saying why."])
    refs = []
    for item in raw:
        if not isinstance(item, str) or not item.strip():
            die(["FAIL commission-manifest: `design_refs` in the %s contains %r, which is"
                 % (where, item),
                 "  not a non-empty path string."])
        path = item.strip()
        if path.startswith("/") or path.startswith("~") or os.path.isabs(path):
            die(["FAIL commission-manifest: `design_refs` entry `%s` in the %s is ABSOLUTE."
                 % (path, where),
                 "  Every cited path in this manifest is relative to the checkout root. An",
                 "  absolute path resolves on one machine and nowhere else, so CHECK 1 would",
                 "  go green for its author and dead for everyone the brief is written for."])
        parts = path.replace("\\", "/").split("/")
        if ".." in parts:
            die(["FAIL commission-manifest: `design_refs` entry `%s` in the %s climbs out of"
                 % (path, where),
                 "  the repository with `..`. A brief may only cite what the checkout contains."])
        if path not in refs:
            refs.append(path)
    return refs


def scan_design(tokens, release, slice_no):
    # Second selector, alongside the token scan: a doc whose FILENAME names this
    # release AND this slice is that slice's own design memo (SLICE-TEMPLATE §3.0
    # writes one per slice). The release is required in the name precisely
    # because `dev/design/slice-30-design.md` is 0.8.0's — pulling it into a
    # 0.8.20 Slice-30 brief would be a wrong citation that still resolves.
    #
    # THE SLICE ID IS A LITERAL, NOT A PATTERN, AND ITS TRAILING BOUNDARY REJECTS
    # A DOTTED CONTINUATION. Three properties, each measured against a candidate
    # matrix rather than reasoned about, because the two obvious fixes are both
    # wrong in different directions:
    #
    #   `%d` (what shipped)  `"%d" % 39.5` and `"%d" % 39` produce the
    #       BYTE-IDENTICAL pattern, so a fractional slice matches its integer
    #       neighbour's memo and vice versa. LIVE IN THIS REPO at the time of the
    #       fix: `0.8.20-slice-39-…` and `0.8.20-slice-39.5-…` both exist and the
    #       shipped pattern matched both for slice 39, so Slice 39's commission
    #       cited Slice 39.5's design of record as its own required reading
    #       (arms 13d, 13i). 7 of 24 matrix cases wrong.
    #   `%s` unescaped       yields `0*39.5(?![0-9])`, where `.` matches ANY
    #       character — `slice-39x5-design.md` resolves as Slice 39.5's memo
    #       (arm 13e). 6 of 24 wrong.
    #   escape only          `0*39\.5(?![0-9])` still matches `slice-39.5.1-…`,
    #       because the character after `39.5` is `.`, not a digit — so 39.5
    #       claims 39.5.1's memo, and 39 still claims 39.5's (arm 13f).
    #       4 of 24 wrong. Escaping is NECESSARY BUT NOT SUFFICIENT.
    #   `(?![0-9.])`         the obvious boundary tighten, and a NEW FALSE
    #       NEGATIVE: it rejects `9.9.9-slice-10.md`, where the character after
    #       the id is the EXTENSION dot, so the slice loses its own memo and the
    #       TC-37 guard hard-stops a slice that IS designed (arm 13g). 2 of 24
    #       wrong — in the safe-looking direction, which is worse.
    #
    # What must be rejected is a following DIGIT, or a following `.` that BEGINS
    # A LONGER ID — never a `.` on its own. Spelled as two lookaheads rather than
    # the compact `(?!\.?[0-9])` so each rejection reads as its own rule.
    # 24 of 24 matrix cases correct.
    name_re = re.compile(
        r"slice[-_ ]?0*" + re.escape(slice_str(slice_no)) + r"(?![0-9])(?!\.[0-9])",
        re.I)
    # Compiled ONCE per run, not once per (document, token) pair: the scan now
    # walks three roots, and re-compiling inside the inner loop would make the
    # cost quadratic in the tier for no benefit.
    token_res = [(t, token_re(t)) for t in tokens]
    hits = []
    seen = set()
    for design_root in DESIGN_ROOTS:
        for root, dirs, files in os.walk(design_root):
            dirs.sort()
            for name in sorted(files):
                if not name.endswith(".md"):
                    continue
                path = os.path.join(root, name)
                # The roots are disjoint today; de-duplicating anyway means a
                # future nested root cannot silently double-count a document.
                if path in seen:
                    continue
                seen.add(path)
                try:
                    text = read(path)
                except OSError:
                    continue
                # WHOLE-TOKEN, never substring (TC-100). See token_re().
                matched = [t for t, rx in token_res if rx.search(text)]
                if release in name and name_re.search(name):
                    matched.append("filename: %s slice %s" % (release, slice_no))
                if not matched:
                    continue
                status = frontmatter_status(path) or "(no status:)"
                hits.append((STATUS_RANK.get(status.split()[0], 7), path, status, matched))
    hits.sort(key=lambda h: (h[0], h[1]))
    return hits


PRE_SIGN_STATES = ("PRE_SIGNED", "NOT_PRE_SIGNED")


def emit_publish_gate(m, gate, state_path):
    """Brief the publish gate as THREE DISTINCT FACTS, never one status word.

    WHAT WENT WRONG HERE. This line used to print `gate["state_word"]`, a single
    word the state file derived from `signed: false`. It rendered "AC-079 —
    unsigned, minted=False" INTO A SLICE-20 COMMISSION MANIFEST, after the HITL
    had PRE-SIGNED that governed-surface delta (2026-07-25, master F-34), pinned
    to the exact content of the allowlist. Briefing the next orchestrator that a
    settled sign-off is still outstanding is how a settled call gets re-decided —
    the failure this whole manifest exists to prevent (see §8: "Ruled decisions
    are CITED, never re-decided or restated").

    The three facts, kept apart on purpose:
      1. `pre_sign_state` — has the HITL signed off on the CONTENT of the delta?
      2. `minted` / `sign_off_slice` — has the AC been minted and recorded SIGNED?
         PRE-SIGN IS NOT MINTING; the AC does not exist in the register yet.
      3. `publish_gated_by` — what actually holds publish. Not an unsigned AC.

    Every required field is read with `[...]`, not `.get(...)`. Under the
    predecessor model a `.get("state_word")` against a state file that no longer
    carried the key printed the literal text `AC-999 — None` and every gate still
    passed: a stale reference rendering a blank is the same defect wearing a
    different hat, so a missing fact dies here instead."""
    try:
        for retired in ("state_word", "signed"):
            if retired in gate:
                raise ValueError(
                    "`acceptance.publish_gate` carries the RETIRED field `%s`. That single "
                    "collapsed word is what made a Slice-20 manifest brief an already-given "
                    "HITL pre-sign as an outstanding one. Model the facts separately: "
                    "pre_sign_state + pre_sign, minted + minted_as + sign_off_slice, "
                    "publish_gated_by." % retired)

        state_word = gate["pre_sign_state"]
        if state_word not in PRE_SIGN_STATES:
            raise ValueError(
                "`acceptance.publish_gate.pre_sign_state` is %r; it must be one of %s. An "
                "unrecognised value must fail loudly rather than brief a guessed claim about "
                "whether the HITL has signed." % (state_word, " or ".join(PRE_SIGN_STATES)))

        ac       = gate["ac"]
        covers   = gate["covers"]
        minted   = gate["minted"]
        mint_slice = gate["sign_off_slice"]
        as_word  = gate["minted_as"]
        board_rf = gate["board_ref"]
        gated_by = gate["publish_gated_by"]
        pre      = gate["pre_sign"] if state_word == "PRE_SIGNED" else None
        if state_word == "PRE_SIGNED" and not isinstance(pre, dict):
            raise ValueError(
                "`acceptance.publish_gate.pre_sign_state` is PRE_SIGNED but there is no "
                "`pre_sign` object recording WHO signed, WHEN, on what authority and what "
                "the pre-sign is PINNED to. A pre-sign with no provenance is not citable.")
    except (KeyError, ValueError, TypeError) as exc:
        detail = exc.args[0] if exc.args else exc
        die(["FAIL commission-manifest: the publish gate in `%s` cannot be briefed —"
             % state_path,
             "  %s" % detail,
             "  A manifest that printed a blank or guessed sign-off status would brief the",
             "  next orchestrator with a claim nobody made (TC-37: a gate that cannot run",
             "  must not report a pass)."])

    if minted:
        head = "%s — MINTED and recorded as %s at Slice %s (board %s)" % (
            ac, as_word, mint_slice, board_rf)
    elif state_word == "PRE_SIGNED":
        head = "%s — PRE-SIGNED by %s on %s (%s); NOT YET MINTED" % (
            ac, pre["by"], pre["on"], pre["source"])
    else:
        head = "%s — NOT PRE-SIGNED, awaiting HITL sign-off; NOT YET MINTED" % ac

    m.out("  publish gate                      %s" % head)
    m.out("      covers                        %s" % covers)
    if pre:
        # REGISTERED, not printed. `pinned_to` names the file whose CONTENT the
        # HITL pre-sign is bound to — the single most load-bearing path in the
        # manifest, since the pin is what makes the pre-sign citable at all. It
        # used to be emitted as raw text, so CHECK 1 never saw it: a mistyped or
        # renamed pin path still printed, `--verify` still reported 0 dead
        # citations, and CI stayed green while the brief pointed an orchestrator
        # at a file that does not exist. Going through `m.cite` is what makes the
        # path existence-checked like every other cited path.
        m.out("      pinned to                     %s"
              % m.cite(pre["pinned_to"], label="publish-gate pre-sign pin"))
        m.out("      re-opens if                   %s" % pre["reopens_if"])
    if not minted:
        m.out("      mints at                      Slice %s, recorded as %s (board %s)"
              % (mint_slice, as_word, board_rf))
    if state_word == "PRE_SIGNED":
        m.out("      publish is gated by           %s — NOT by this AC." % gated_by)
        m.out("      The pre-sign is a RULED decision: cite it, never re-decide it. Do not")
        m.out("      brief it as outstanding and do not seek it again.")
    else:
        m.out("      publish is gated by           that sign-off AND %s." % gated_by)
        m.out("      Escalate for the sign-off; never re-decide it yourself.")


def build(release, state_path, state, slice_no, entry, curated=None):
    m = Manifest()
    curated = list(curated or [])
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
    # THE ROOTS ARE DISCLOSED IN THE BRIEF ITSELF. A reader has to be able to tell
    # what the scan could and could not have seen; before TC-94 defect 1 this line
    # named `dev/design/` alone and that was also the whole truth. Now it names
    # all three, because a manifest that understated its own reach would be the
    # same stale generated statement this file exists to prevent.
    m.out("(T2a) and from `status:` frontmatter under %s (T2c). Section set derived from"
          % ", ".join("`%s/`" % r for r in DESIGN_ROOTS))
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
    # `is_slice_num`, NEVER `isinstance(s, int)`. The int-only filter dropped a
    # FRACTIONAL landed slice out of the predecessor set entirely, so the base
    # SHA silently skipped a whole unit's work — measured on a fixture with a
    # landed Slice 10.5, the manifest for Slice 30 printed
    #   `base sha  bbbb2222  (Slice 5 — the newest LANDED slice STRICTLY BEFORE Slice 30)`
    # instructing the operator to branch from a commit that predates 10.5. That
    # is this repo's named agent-worktree-stale-base trap emitted by the tool
    # written to prevent it, and a confidently printed wrong SHA is precisely
    # what defeats the human sanity check (arm 13a). The same filter also made
    # `slice_no in landed_nums` false for a landed fractional slice, suppressing
    # the HISTORICAL banner (arm 13b).
    landed_nums = sorted({s for s in landed if is_slice_num(s)})
    prior_landed = [s for s in landed_nums if s < slice_no]
    later_landed = [s for s in landed_nums if s > slice_no]
    base_slice = max(prior_landed) if prior_landed else None
    base_sha = (ladder.get(base_slice) or {}).get("sha") if base_slice is not None else None
    if slice_no in landed_nums:
        m.out("  ⚠ HISTORICAL     Slice %s is ITSELF LANDED (%s), so this is a regeneration:"
              % (slice_str(slice_no), (ladder.get(slice_no) or {}).get("sha") or "no sha recorded"))
        m.out("                   the base below is that slice's PREDECESSOR, never its own merge.")
    if base_sha:
        m.out("  base sha         %s  (Slice %s — the newest LANDED slice STRICTLY BEFORE Slice %s)"
              % (base_sha, slice_str(base_slice), slice_str(slice_no)))
    elif base_slice is not None:
        m.out("  base sha         (Slice %s is the predecessor but records NO landing sha)"
              % slice_str(base_slice))
        m.out("                   The state file is incomplete: take the branch point from")
        m.out("                   `git log` for that landing and record it in output.json. Do NOT guess.")
    else:
        m.out("  base sha         (none — NO landed slice precedes Slice %s)" % slice_str(slice_no))
        # These two cases are MUTUALLY EXCLUSIVE. "No predecessor" means branch
        # from the tip ONLY while the tip is still where this slice was cut. Once
        # later slices have landed, origin/main carries their merges, so printing
        # the branch-from-tip instruction beside the TIP IS AHEAD warning emits
        # precisely the stale-base instruction this section exists to prevent —
        # and an operator follows the instruction, not the caveat next to it.
        if later_landed:
            m.out("  ⚠ TIP IS AHEAD   Slice(s) %s landed AFTER this one, so origin/main already"
                  % ", ".join(slice_str(s) for s in later_landed))
            m.out("                   carries work Slice %s never had. There is NO correct automatic"
                  % slice_str(slice_no))
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
              % ", ".join(slice_str(d) for d in dep_gap))
        m.out("                   base slice, so the base merge does not contain them. Re-derive the")
        m.out("                   branch point from `git log` before cutting the worktree.")
    m.out("  re-verify        `git rev-parse origin/main` at STEP 0 — a later landing may have")
    m.out("                   advanced the tip since the state file was written.")
    # `is_slice_num`, not `isinstance(s, int)` — the SAME filter as the base-SHA
    # computation above, in its third costume. A fractional slice landed at or
    # after the target carried no `⚠ at/after` mark, so the one line that would
    # have warned the reader that origin/main already contains work this slice
    # never had stayed silent (arm 13c).
    m.out("  landed so far    %s" % (", ".join(
        "%s (%s)%s" % (slice_str(s), (ladder.get(s) or {}).get("sha") or "no sha",
                       " ⚠ at/after Slice %s" % slice_str(slice_no)
                       if is_slice_num(s) and s >= slice_no else "")
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
    # §4 IS THE CONFLICT-WINS SHELF, so a curated contract belongs HERE and not
    # only in §6. The motivating case: 0.8.20 Slice 22 cites
    # `record-lifecycle-protocol/OPP-12-C1-converged-contract.md`, byte-pinned by
    # scripts/c1-conformance-pin.json (sha256 AND git blob sha1) — the ratified
    # contract a TC-67 implementation can turn RED. A pinned file can NEVER be
    # back-linked, because any edit breaks the pin, so citation is the only route
    # by which it can reach an orchestrator at all.
    #
    # The SELECTION PREDICATE IS UNCHANGED — the basename says `contract` — it is
    # merely applied to the curated set as well. A curated doc that is not a
    # contract stays in §6; otherwise §4 degenerates into a second copy of §6 and
    # stops meaning "the thing that wins".
    curated_contracts = [p for p in curated
                         if "contract" in os.path.basename(p).lower()]
    for path in curated_contracts:
        recorded = frontmatter_status(path)
        if recorded is None:
            status_word, flag = "no `status:` field recorded", " ⚠ unclassified (TC-50)"
        else:
            status_word = recorded
            flag = " ⚠ unclassified (TC-50)" if recorded.split()[0] in UNCLASSIFIED else ""
        m.out("  design contract  CURATED [%s]%s %s"
              % (status_word, flag,
                 m.cite(path, label="design_refs (curated contract)")))
    for _rank, path, status, _matched in design:
        # A curated contract the scan ALSO reached is already printed above; a
        # second row for one document would make the reader reconcile two.
        if path in curated_contracts:
            continue
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
    # TWO PROVENANCES, NEVER BLENDED. CURATED rows were chosen by the Steward in
    # the state file's `design_refs`; SCANNED rows were discovered by the two
    # selectors. A reader has to be able to tell them apart, because they carry
    # different warrants: a curated row is somebody's judgement, a scanned row is
    # a whole-token match that may still be incidental.
    m.out("## 6. DESIGN DOCS FOR THIS SLICE   [SLICE-TEMPLATE §1.4 {{READING}}]")
    if curated:
        m.out("   TWO PROVENANCES, kept apart. CURATED rows are named by this ladder entry's")
        m.out("   `design_refs` in the state file — chosen, not discovered — and may sit OUTSIDE")
        m.out("   the scanned roots, where the scan below cannot")
        m.out("   reach. SCANNED rows were found by the selectors and are listed after them.")
    m.out("   Selected by the slice's OWN tokens from the state file: %s" % ", ".join(tokens))
    m.out("   Status is AS RECORDED in `status:` frontmatter (T2c) — it is NOT a currency claim.")
    m.out("   scripts/lint-design-status.sh proves the PRESENCE of a status, never its TRUTH;")
    m.out("   UNREVIEWED means 'nobody has classified this yet' and the classification is owed")
    m.out("   at TC-50. Rows marked ⚠ unclassified are candidates, NOT design of record.")
    scan_matched = {h[1]: h[3] for h in design}
    if curated:
        m.out("  -- CURATED (`design_refs`, hand-cited in the state file) ------------------")
        for path in curated:
            # frontmatter_status() answers None for BOTH "no status: field" and
            # "unreadable/absent file". A missing file is caught by CHECK 1 and
            # the manifest is never emitted, so the honest wording for the row
            # that does get printed is the former: say the field is absent
            # rather than manufacture a classification the doc never made.
            recorded = frontmatter_status(path)
            if recorded is None:
                status_word = "no `status:` field recorded"
                flag = " ⚠ unclassified (TC-50)"
            else:
                status_word = recorded
                flag = (" ⚠ unclassified (TC-50)"
                        if recorded.split()[0] in UNCLASSIFIED else "")
            m.out("  CURATED [%s]%s %s"
                  % (status_word, flag, m.cite(path, label="design_refs (curated)")))
            # "OUTSIDE THE ROOTS" IS NOW A COMPUTED FACT, NOT A CONSTANT. This row
            # used to say "outside `dev/design/` — cited BECAUSE the scan cannot
            # reach it" for every curated path outside that one tree, which became
            # FALSE for `dev/adr/**` and `dev/interfaces/**` the moment the scan
            # was widened (TC-94 defect 1). A generated line that asserts the tool
            # cannot do something it now does is the same class of stale
            # restatement this whole file exists to remove, so the test is against
            # DESIGN_ROOTS and moves with it.
            if not any(path.startswith(r + "/") for r in DESIGN_ROOTS):
                m.out("        outside the scanned roots (%s) — cited BECAUSE the scan cannot reach it."
                      % ", ".join("`%s/`" % r for r in DESIGN_ROOTS))
            also = scan_matched.get(path)
            if also:
                m.out("        the scan reached it too, on: %s" % ", ".join(also))
            if os.path.exists(path) and not os.path.isfile(path):
                m.problems.append(
                    "FAIL commission-manifest: `design_refs` entry `%s` is a DIRECTORY, not a\n"
                    "  design document. Cite the file to be read, not the folder it sits in."
                    % path)
        m.out("  -- SCANNED (filename + token selectors over %s) ------------------"
              % ", ".join("`%s/`" % r for r in DESIGN_ROOTS))
    # A curated doc the scan ALSO reached is reported once, in the curated block,
    # with its scan hits folded in. Printing it twice would double-count the tier
    # and make the reader reconcile two rows for one document.
    scanned = [h for h in design if h[1] not in set(curated)]
    for _rank, path, status, matched in scanned:
        flag = " ⚠ unclassified (TC-50)" if status.split()[0] in UNCLASSIFIED else ""
        m.out("  [%s]%s %s" % (status, flag, m.cite(path)))
        m.out("        matched: %s" % ", ".join(matched))
    if curated:
        classified = [h for h in scanned if h[2].split()[0] not in UNCLASSIFIED]
        m.out("  -> %d curated doc(s) cited; %d further doc(s) matched by the scan: "
              "%d classified, %d unclassified."
              % (len(curated), len(scanned), len(classified), len(scanned) - len(classified)))
    else:
        classified = [h for h in design if h[2].split()[0] not in UNCLASSIFIED]
        m.out("  -> %d doc(s) matched: %d classified, %d unclassified."
              % (len(design), len(classified), len(design) - len(classified)))
    matched_tokens = {t for _r, _p, _s, ms in design for t in ms}
    # PER-TOKEN, not all-or-nothing: a token nothing mentions is named even when
    # its neighbours matched, so a slice with one weak incidental hit does not
    # read like a slice with full coverage. It stays a REPORT — the run exits 0.
    unmatched = [t for t in tokens if t not in matched_tokens]
    if unmatched:
        m.out("  -> NO design doc mentions: %s. That part of the slice has no design of record;"
              % ", ".join(unmatched))
        m.out("     its authority is whatever the plan's rulings/requirements sections say — read")
        m.out("     them there, and do not infer a design that does not exist.")
        if curated:
            m.out("     (The %d CURATED doc(s) above were cited by hand and are NOT token-matched, so"
                  % len(curated))
            m.out("     read them before concluding the token is genuinely undesigned.)")
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
        emit_publish_gate(m, gate, state_path)
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
    # RULED-WITH-WORK. A decision can be CLOSED and still owe an ACTION. Ruling one
    # moves it out of `unruled`, so it stops being rendered as a named HALT/GATED row
    # above and collapses into the bare count below — i.e. ruling a decision that
    # carries residual work makes that work LESS visible, not more. Any `ruled` entry
    # carrying a `residual_work` string is therefore rendered by name here.
    # Found by the Slice 40 manifest+brief adversarial review, 2026-07-31 (round 2):
    # the first instance (`axis-e-version`) had been patched by hand into ONE prose
    # section of plan-0.8.20.md, which is the same one-writer/many-copies defect the
    # same morning's b9a3a296 was written to end. Fix the tooling, not the instance.
    rww = [d for d in (dec.get("ruled") or []) if (d.get("residual_work") or "").strip()]
    for d in rww:
        m.out("  ⚠ RULED-WITH-WORK — %s" % (d.get("title") or d.get("id")))
        m.out("      the DECISION is closed; the WORK is not. Do NOT infer 'ruled' ⇒ 'nothing owed'.")
        m.out("      owed: %s" % d.get("residual_work"))
        m.out("      ruling: %s · source: %s" % (d.get("ruling") or "?", d.get("source") or "?"))
    m.out("  Ruled decisions are CITED, never re-decided or restated: %d ruling(s) recorded in the"
          % len(dec.get("ruled") or []))
    m.out("  state file's `decisions.ruled` with their sources; read them there.")
    if rww:
        m.out("  %d of those ruling(s) carry RESIDUAL WORK and are named in full above." % len(rww))
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
    # The guard is NOT weakened by `design_refs`: it now asks whether the design
    # tier is empty by EITHER route. A non-empty curation satisfies it (that is
    # the whole point — a reserved-gap slice both selectors miss by construction
    # is still a designed slice), but an empty or absent one leaves the original
    # hard failure exactly where it was. `design_refs: []` is not a design.
    curated = curated_design_refs(entry, state_path, slice_no)
    if not curated and not scan_design(tokens, release, slice_no):
        die(["FAIL commission-manifest: ZERO design docs matched %s Slice %s (tokens: %s)."
             % (release, slice_no, ", ".join(tokens) or "none derived"),
             "  The required-reading list would be EMPTY, so the manifest would brief nobody",
             "  while exiting 0 — this repo's named TC-37 vacuous-pass class. Fix it by",
             "  authoring/citing the design of record for this slice, by naming the slice's",
             "  requirement id in the state file's `short`/`title` so the scan can find it, or",
             "  — for a reserved-gap slice whose ids postdate the design package and which both",
             "  selectors therefore miss by construction — by listing the docs to read in this",
             "  ladder entry's `design_refs`."])

    m = build(release, state_path, state, slice_no, entry, curated)
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
