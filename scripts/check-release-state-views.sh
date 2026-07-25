#!/usr/bin/env bash
# scripts/check-release-state-views.sh — T2a enforcement (DOC-HYGIENE-2):
# SINGLE-WRITER RELEASE STATE + GENERATED VIEWS.
#
# ---------------------------------------------------------------------------
# WHY (measured, not hypothetical)
# ---------------------------------------------------------------------------
# 0.8.20's release state — which slices have landed and at which SHAs, the
# SCHEMA version, which slice is next, the AC status, which decisions are ruled
# — was narrated across a 5-12 file write-side fan-out, and NOTHING checked that
# the copies agreed. One reconciliation commit (b70629e5) had to touch SEVEN
# files to bring them back into line, and the live board is ~79 KB: nobody reads
# it whole, so a copy can sit wrong for weeks without anyone noticing.
#
# The remedy is the standing rule's remedy — fix the tooling so it cannot recur
# for anyone, rather than writing a "remember to update all seven" note:
#
#   * ONE machine-readable state file per live release owns the facts:
#     dev/plans/release-state-<version>.json.
#   * Restatements of those facts in prose become MARKER-DELIMITED GENERATED
#     REGIONS, rendered from that file.
#   * This gate regenerates every region into a temp buffer and DIFFS it against
#     what is actually in the document. Any difference is a hard failure — so a
#     hand-edit of a view goes red, and a fact changed in the state file without
#     a regenerate goes red too.
#
# ---------------------------------------------------------------------------
# WHAT IT ENFORCES (four rules + one guard)
# ---------------------------------------------------------------------------
# RULE 1 — REGENERATE-AND-DIFF. For every view a state file declares, the bytes
#   between its BEGIN and END markers must equal the bytes the renderer produces
#   from that state file. Not a heuristic, not a wording check: byte equality.
#
# RULE 2 — MARKERS MUST BE WELL-FORMED AND PRESENT. A declared view whose BEGIN
#   or END marker is missing, duplicated, or out of order is a FAILURE, never a
#   skip. A view that silently stops being checked is worse than no view.
#
# RULE 3 — NO ORPHAN MARKERS (the confinement rule). Every
#   `BEGIN GENERATED release-state:` marker anywhere in the worktree's markdown
#   must belong to a view some state file declares. This is what mechanically
#   holds the HITL pre-sign's "generated regions are confined to exactly the
#   named locations, nothing else": a marker added anywhere else fails.
#
# RULE 4 — EVERY DECLARED VIEW MUST HAVE A RENDERER. A view id this script does
#   not know how to render cannot be checked, so it is a failure rather than a
#   silently-unchecked region.
#
# VACUOUS-PASS GUARD (TC-37 — this repo's named failure class: a gate that
#   reports green without having run). If ZERO state files are discovered, or
#   ZERO generated blocks are checked, the checking loop never executes and this
#   script would exit 0 having vouched for nothing. Both are HARD failures.
#   This is the same hole that let a genuinely-red `main` read green for three
#   weeks when markdownlint-cli2 was absent.
#
# ---------------------------------------------------------------------------
# WHAT IS *NOT* GENERATED (deliberate, and load-bearing)
# ---------------------------------------------------------------------------
# The three fenced regions are SUB-REGIONS of their locations, not whole
# sections. The surrounding prose — narrative, rationale, cross-references,
# history — stays hand-written and OUTSIDE the markers, untouched. A renderer
# cannot plausibly re-derive judgement prose, and pretending otherwise would
# mean laundering that prose through a JSON string, which buys nothing. Only
# mechanically-derivable fact restatements are fenced. Removing the marker
# comments restores the documents exactly, byte for byte: the fencing is fully
# reversible and deletes nothing.
#
# ---------------------------------------------------------------------------
# USAGE
# ---------------------------------------------------------------------------
#   scripts/check-release-state-views.sh            # check (default); exit 1 on drift
#   scripts/check-release-state-views.sh --write    # regenerate the regions in place
#   scripts/check-release-state-views.sh --quiet    # check, suppress the ok line
#
# Exit: 0 = every generated region matches its render (and >=1 state file and
#           >=1 block were seen);
#       1 = drift, a malformed/missing/orphan marker, an unknown view id, or a
#           vacuous scan;
#       2 = usage error / the gate could not run.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

MODE="check"
QUIET=0
while [ $# -gt 0 ]; do
  case "$1" in
    --write) MODE="write"; shift ;;
    --check) MODE="check"; shift ;;
    --quiet) QUIET=1; shift ;;
    *) printf 'check-release-state-views: unknown arg %q\n' "$1" >&2; exit 2 ;;
  esac
done

# `python3` carries the JSON parse + the byte-exact region diff. If it is absent
# the gate cannot run — and a gate that cannot run must fail loudly, never skip
# (TC-37). Same posture as lint-plan-anchors.sh's perl dependency.
if ! command -v python3 >/dev/null 2>&1; then
  echo "FAIL check-release-state-views: python3 not found; the state-file parser and" >&2
  echo "  the byte-exact region diff cannot run. A gate that cannot run must not" >&2
  echo "  report a pass (TC-37)." >&2
  exit 2
fi

MODE="$MODE" QUIET="$QUIET" python3 - <<'PY'
import glob, json, os, sys

MODE  = os.environ["MODE"]
QUIET = os.environ["QUIET"] == "1"

BEGIN = "<!-- BEGIN GENERATED release-state:{release}:{vid} -->"
END   = "<!-- END GENERATED release-state:{release}:{vid} -->"

# Bare BEGIN-marker prefix, used by the RULE-3 orphan scan. The scan reads only
# `*.md`, so this script's own source (a `.sh`) is never a false positive.
MARKER_PREFIX = "<!-- BEGIN GENERATED release-state:"

fail = 0
def bad(msg):
    global fail
    fail = 1
    sys.stderr.write(msg.rstrip("\n") + "\n")

# ---------------------------------------------------------------------------
# Renderers. One per view id. Each takes the parsed state dict and returns the
# EXACT bytes that must sit between that view's markers.
#
# These are templates over FACTS, which is the whole point: the connective text
# is literal, every varying part comes from the state file. If a template can no
# longer reproduce the document, the honest move is to widen the fact model or
# narrow the region — never to paste the prose into the JSON.
# ---------------------------------------------------------------------------

def _by_slice(st):
    return {int(e["slice"]): e for e in st["ladder"]}

def _and_join(items):
    """`[20]` -> "20"; `[20, 25]` -> "20 and 25"; `[20, 25, 30]` -> "20, 25 and 30"."""
    items = [str(i) for i in items]
    if len(items) <= 1:
        return "".join(items)
    return ", ".join(items[:-1]) + " and " + items[-1]

def render_master_ladder_progress(st):
    """master §4, the 0.8.20 row: landed slices + SHAs, SCHEMA, remaining ladder.

    Fenced sub-region only. The `**✅ LADDER PROGRESS (F-32, verified from git @
    ...):` lead-in and the closing `**` stay OUTSIDE the markers: the F-n
    reference is a findings-register citation and is deliberately not placed
    under generation."""
    by = _by_slice(st)
    landed = " · ".join("%d (`%s`)" % (n, by[n]["sha"]) for n in st["landed"])
    ladder = " → ".join(str(n) for n in st["remaining_ladder"])
    return ("Slices %s are all LANDED on `origin/main`; SCHEMA is %d; "
            "remaining ladder = %s." % (landed, st["schema_version"], ladder))

def render_status_unblocks(st):
    """STATUS §1 `**Unblocks**` row: what is unblocked, by what, and the
    publish-precondition + AC sign-off status."""
    by   = _by_slice(st)
    ub   = st["unblocked"]
    src  = st["unblocked_by"]
    pre  = by[int(st["publish_precondition_slice"])]
    gate = st["acceptance"]["publish_gate"]
    return ("**Slices %s are NOW UNBLOCKED** — %s (%s) now exists. "
            "Slice %d (%s) depends on %s. "
            "**Publish remains blocked on %s**, which is **still %s** — "
            "sign-off is gated to Slice %d (%s)."
            % (_and_join(ub), src["requirement"], src["gloss"],
               pre["slice"], pre["short"], "/".join(str(d) for d in pre["depends_on"]),
               gate["ac"], gate["state_word"], gate["sign_off_slice"], gate["board_ref"]))

def render_handoff_next_step(st):
    """Steward hand-off ★ IMMEDIATE NEXT STEP: landed chain + next slice.

    The leading newline is NOT cosmetic. This region starts a markdown
    PARAGRAPH, and CommonMark treats a line that BEGINS with `<!--` as an HTML
    block that swallows the whole line — so an inline BEGIN marker in front of
    the sentence would stop the sentence rendering as markdown. The marker
    therefore sits on its own line and the region content starts with the
    newline that follows it."""
    chain = " → ".join(str(n) for n in st["landed"])
    return ("\n**The %s ladder is between slices: %s are all LANDED; %d is next.**"
            % (st["release"], chain, st["next_slice"]))

# Spelled-out counts, because the sentence this renders into is prose. Beyond
# the table the numeral is used verbatim rather than inventing a spelling: an
# open set that large is a different problem than a wording one.
NUMBER_WORDS = {0: "ZERO", 1: "ONE", 2: "TWO", 3: "THREE", 4: "FOUR", 5: "FIVE",
                6: "SIX", 7: "SEVEN", 8: "EIGHT", 9: "NINE", 10: "TEN"}

def render_status_live_open_count(st):
    """STATUS §4, the banner: how many decisions are live-OPEN, spelled out.

    T2b (DOC-HYGIENE-2). The measured failure this pins: §4 listed >=4
    ALREADY-RULED items as still open, and §4 says so about itself in the file
    ("a hand-maintained duplicate of state that lives in three other files").
    The count of the live open set is the one fact in that banner that is
    mechanically derivable from `decisions.unruled`, so the count is what the
    fence owns — a third unruled item appearing in the state file now turns this
    gate RED until the banner is regenerated.

    Deliberately NARROW; see `why_only_the_count` on this view in the state file
    for the full statement. In short: the surrounding sentence cannot be
    rendered from facts (its two items have different prose shapes and it closes
    on an F-n citation), and reproducing it would mean pasting the sentence into
    the JSON — the move this script refuses above. The historical rows 1-22 of
    §4 are the retained decision record and are NOT under generation at all."""
    return NUMBER_WORDS.get(len(st["decisions"]["unruled"]),
                            str(len(st["decisions"]["unruled"])))

RENDERERS = {
    "master-ladder-progress":  render_master_ladder_progress,
    "status-unblocks":         render_status_unblocks,
    "status-live-open-count":  render_status_live_open_count,
    "handoff-next-step":       render_handoff_next_step,
}

# ---------------------------------------------------------------------------
# Discover state files.
# ---------------------------------------------------------------------------
state_paths = sorted(glob.glob("dev/plans/release-state-*.json"))
if not state_paths:
    bad("FAIL check-release-state-views: ZERO release-state files discovered under\n"
        "  dev/plans/release-state-*.json. The check loop never ran, so a pass here\n"
        "  would vouch for nothing (TC-37 vacuous-pass guard).")
    sys.exit(1)

blocks_checked = 0
rewritten      = []
declared       = set()   # (path, release, view-id) actually declared somewhere

for sp in state_paths:
    try:
        with open(sp, encoding="utf-8") as fh:
            st = json.load(fh)
    except Exception as exc:                                  # noqa: BLE001
        bad("FAIL %s: not parseable as JSON — %s" % (sp, exc))
        continue

    release = st.get("release")
    views   = st.get("generated_views")
    if not release or not isinstance(views, list):
        bad("FAIL %s: a release-state file must carry a `release` string and a\n"
            "  `generated_views` list naming the regions rendered from it." % sp)
        continue
    if not views:
        bad("FAIL %s: `generated_views` is EMPTY, so this state file owns no region\n"
            "  and nothing about it is checkable. A state file that gates nothing is a\n"
            "  vacuous pass, not a pass (TC-37)." % sp)
        continue

    for view in views:
        vid  = view.get("id")
        path = view.get("file")
        if not vid or not path:
            bad("FAIL %s: a `generated_views` entry needs both `id` and `file`." % sp)
            continue
        if vid not in RENDERERS:
            bad("FAIL %s: view `%s` has no renderer in check-release-state-views.sh, so\n"
                "  its region cannot be checked. An unrenderable view is a failure, not\n"
                "  an unchecked region." % (sp, vid))
            continue
        if not os.path.isfile(path):
            bad("FAIL %s: view `%s` names `%s`, which is not a file in the worktree."
                % (sp, vid, path))
            continue

        b = BEGIN.format(release=release, vid=vid)
        e = END.format(release=release, vid=vid)
        declared.add((path, b))

        with open(path, encoding="utf-8") as fh:
            doc = fh.read()

        if doc.count(b) != 1 or doc.count(e) != 1:
            bad("FAIL %s: view `%s` expects EXACTLY ONE BEGIN and ONE END marker; found\n"
                "  %d BEGIN and %d END. Markers:\n    %s\n    %s"
                % (path, vid, doc.count(b), doc.count(e), b, e))
            continue
        i = doc.index(b) + len(b)
        j = doc.index(e)
        if j < i:
            bad("FAIL %s: view `%s` has its END marker BEFORE its BEGIN marker."
                % (path, vid))
            continue

        have = doc[i:j]
        want = RENDERERS[vid](st)
        blocks_checked += 1

        if have == want:
            continue

        if MODE == "write":
            with open(path, "w", encoding="utf-8") as fh:
                fh.write(doc[:i] + want + doc[j:])
            rewritten.append("%s :: %s" % (path, vid))
            continue

        bad("FAIL %s: generated region `%s` is STALE — it does not match what\n"
            "  %s renders. Either the document was hand-edited inside the markers, or a\n"
            "  fact changed in the state file and the region was never regenerated.\n"
            "  IN THE DOCUMENT:\n    %r\n"
            "  RENDERED FROM THE STATE FILE:\n    %r\n"
            "  Fix: edit the FACT in %s, then run\n"
            "    scripts/check-release-state-views.sh --write\n"
            % (path, vid, sp, have, want, sp))

# ---------------------------------------------------------------------------
# RULE 3 — orphan-marker scan (the confinement rule).
# ---------------------------------------------------------------------------
PRUNE = {".git", "node_modules", "target", ".venv", "site", "dist", ".cache", ".wake"}
for root, dirs, files in os.walk("."):
    dirs[:] = [d for d in dirs if d not in PRUNE]
    for name in files:
        if not name.endswith(".md"):
            continue
        p = os.path.relpath(os.path.join(root, name), ".")
        try:
            with open(p, encoding="utf-8", errors="replace") as fh:
                text = fh.read()
        except OSError:
            continue
        if MARKER_PREFIX not in text:
            continue
        for line in text.split("\n"):
            k = line.find(MARKER_PREFIX)
            while k != -1:
                end = line.find("-->", k)
                if end == -1:
                    bad("FAIL %s: unterminated generated-region BEGIN marker." % p)
                    break
                marker = line[k:end + 3]
                if (p, marker) not in declared:
                    bad("FAIL %s: ORPHAN generated-region marker\n    %s\n"
                        "  No release-state file declares this view for this file. Generated\n"
                        "  regions are confined to the locations a state file names; a marker\n"
                        "  anywhere else is unowned and unchecked." % (p, marker))
                k = line.find(MARKER_PREFIX, end + 3)

# ---------------------------------------------------------------------------
# Vacuity guard + report.
# ---------------------------------------------------------------------------
if blocks_checked == 0:
    bad("FAIL check-release-state-views: ZERO generated blocks were checked across %d\n"
        "  release-state file(s). The regenerate-and-diff loop never executed, so a pass\n"
        "  here would vouch for nothing (TC-37 vacuous-pass guard)." % len(state_paths))

if fail:
    sys.exit(1)

if MODE == "write":
    if rewritten:
        for r in rewritten:
            print("wrote %s" % r)
    else:
        print("ok    check-release-state-views: %d generated block(s) already current"
              % blocks_checked)
    sys.exit(0)

if not QUIET:
    sys.stderr.write("ok    check-release-state-views: %d generated block(s) across %d "
                     "release-state file(s) match their render\n"
                     % (blocks_checked, len(state_paths)))
sys.exit(0)
PY
