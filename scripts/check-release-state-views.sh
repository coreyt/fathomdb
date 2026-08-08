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
# `git rev-parse` failing here used to degrade to `cd ""` — a bash no-op that
# leaves the script running in an arbitrary cwd. Bind and check it instead.
_repo_toplevel="$(git rev-parse --show-toplevel)" || exit 1
cd "$_repo_toplevel" || exit 1

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
import json, os, subprocess, sys

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
# Remote-landing guard.
#
# WHY THIS EXISTS. `render_master_ladder_progress` and
# `render_plan_landed_roll_up` both emit the literal words "LANDED on
# `origin/main`", but every fact they render comes from the state file's
# `landed` array. Neither ever asked Git whether those SHAs are reachable from
# the remote, so a slice was declared REMOTELY landed the instant it was
# written to the state file — which is local-commit time. On 2026-08-03 local
# `main` sat 21 commits ahead of `origin/main` with three of 0.8.21's five
# slices (0, 15, 20) unpushed, while the plan asserted all five were on
# `origin/main` in full. The board could not have caught it: it was
# structurally incapable of reporting the gap it was asserting away.
#
# WHY THIS IS A SEPARATE CHECK AND NOT A RENDER CHANGE. This script's contract
# is that committed Markdown equals generated Markdown. If the rendered STRING
# varied with local push state, the generated text would differ between a
# steward's checkout and CI, and every in-flight slice would report a spurious
# view mismatch. So the render stays deterministic over the state file and the
# push fact is verified alongside it.
#
# WHY IT IS ADVISORY OFF `main`. A PR branch legitimately carries a landed SHA
# that is not yet on `origin/main` — that is what the PR is for. Hard-failing
# there would block the very merge that resolves it. On `main`, the same
# condition is exactly the drift above and is a HARD failure.
# ---------------------------------------------------------------------------
def _git(*args):
    r = subprocess.run(["git", "--no-optional-locks"] + list(args),
                       check=False, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    return r.returncode, r.stdout.decode("utf-8", "replace").strip()


def check_remote_landing(release, st):
    """Verify every `landed` SHA is reachable from `origin/main`."""
    if _git("rev-parse", "--verify", "--quiet", "origin/main")[0] != 0:
        return  # No remote-tracking ref (fresh or detached clone): unverifiable.

    # A SHALLOW clone cannot answer this question. `actions/checkout` defaults to
    # `--depth=1`, so `origin/main` EXISTS but carries only the tip commit: every
    # historical landed SHA is simply absent from the object store, and
    # `merge-base --is-ancestor` cannot resolve it.
    #
    # WHY THIS GUARD EXISTS. Without it this check reported all 15 of 0.8.20's
    # and all 5 of 0.8.21's landed slices as "not reachable from origin/main" and
    # HARD-FAILED on `main` — on 2026-08-04 it red-lined four consecutive `main`
    # runs. Every one of those SHAs was genuinely on the remote; the clone just
    # could not see them. That is a false negative, and a permanently-red gate is
    # the very antipattern this repository documents (a gate that is always red
    # trains readers to discount red, which is how real failures survive).
    #
    # Absent history means UNVERIFIABLE, never FALSE — the same rule already
    # applied above for a missing remote ref. To make the check meaningful in CI,
    # give its job `fetch-depth: 0` rather than weakening the assertion here.
    if _git("rev-parse", "--is-shallow-repository")[1] == "true":
        if not QUIET:
            sys.stderr.write(
                "note  check-release-state-views: shallow clone — the `origin/main`\n"
                "  landing claim is UNVERIFIABLE here and was not checked. Give this\n"
                "  job `fetch-depth: 0` to verify it.\n")
        return

    by = _by_slice(st)
    unpushed = []
    for n in st["landed"]:
        sha = by[n]["sha"]
        rc, _ = _git("rev-parse", "--verify", "--quiet", sha + "^{commit}")
        if rc != 0:
            unpushed.append((n, sha, "not a commit in this repository"))
            continue
        if _git("merge-base", "--is-ancestor", sha, "origin/main")[0] != 0:
            unpushed.append((n, sha, "not reachable from origin/main"))

    if not unpushed:
        return

    detail = "\n".join("    slice %s (`%s`) — %s" % (_slice_str(n), sha, why)
                       for n, sha, why in unpushed)
    # `--write` REGENERATES; it does not assert. Hard-failing it would make the
    # views unregenerable precisely while a slice is unpushed — the state this
    # guard exists to surface — so the steward could not repair the document
    # without first defeating the check.
    on_main = (_git("rev-parse", "--abbrev-ref", "HEAD")[1] == "main"
               and MODE != "write")
    headline = (
        "check-release-state-views: %s claims %d slice(s) LANDED on `origin/main`\n"
        "  that the remote does not carry:\n%s\n"
        "  The generated views state `origin/main` as a fact. Push the branch (or\n"
        "  merge its PR) so the claim becomes true, or correct `landed` in the\n"
        "  state file." % (release, len(unpushed), detail))
    if on_main:
        bad("FAIL " + headline)
    elif not QUIET:
        sys.stderr.write("warn  " + headline + "\n"
                         "  Advisory on a non-`main` branch: expected while the PR is open.\n")

# ---------------------------------------------------------------------------
# Renderers. One per view id. Each takes the parsed state dict and returns the
# EXACT bytes that must sit between that view's markers.
#
# These are templates over FACTS, which is the whole point: the connective text
# is literal, every varying part comes from the state file. If a template can no
# longer reproduce the document, the honest move is to widen the fact model or
# narrow the region — never to paste the prose into the JSON.
# ---------------------------------------------------------------------------

def _slice_str(n):
    """Render a slice id: 40 -> "40", 39.5 -> "39.5". NEVER %d — that would
    truncate a fractional slice to the wrong slice, which is precisely the
    fabricated-pointer failure the caller's guard exists to prevent."""
    return "%g" % n if isinstance(n, float) else str(n)


def _by_slice(st):
    """Key by the slice id AS STORED — never int(), which would collapse a
    fractional slice onto its integer neighbour and SILENTLY OVERWRITE it
    (39.5 -> 39 clobbers Slice 39). Ints stay ints so existing lookups by
    literal integer keep working."""
    out = {}
    for e in st["ladder"]:
        k = e["slice"]
        if isinstance(k, str):
            k = float(k) if "." in k else int(k)
        # AND IT REFUSES A COLLISION RATHER THAN TAKING THE LAST WRITER. The
        # docstring above says a collapsed id would "SILENTLY OVERWRITE" its
        # neighbour, but nothing here stopped that happening for two ladder
        # entries that normalise to the SAME key — `30` and `30.0`, or `30` and
        # `"30"`. Which entry survived was dict-insertion order, i.e. the order
        # of lines in the state file, and every view downstream then rendered the
        # survivor's short/title/depends_on under the loser's number. In Python
        # `30 == 30.0` and `hash(30) == hash(30.0)`, so this is not a hypothetical
        # shape. A duplicate is a state-file defect and must be reported as one.
        if k in out:
            raise ValueError(
                "the ladder carries TWO entries that resolve to the same slice id %s "
                "(%r and %r). One would SILENTLY OVERWRITE the other — whichever came "
                "second in the file — and every view would then render the survivor's "
                "facts under the loser's number. Give each slice exactly one entry."
                % (_slice_str(k), out[k].get("slice"), e.get("slice")))
        out[k] = e
    return out

def _and_join(items):
    """`[20]` -> "20"; `[20, 25]` -> "20 and 25"; `[20, 25, 30]` -> "20, 25 and 30".

    ITS ITEMS ARE SLICE IDS, so they render through `_slice_str` like every other
    slice id in this file. Its only caller is `render_status_unblocks` on
    `st["unblocked"]`, and its own examples above are slice numbers — but it used
    bare `str()`, so `40.0` reached the board as "40.0" while the sibling
    renderers on `remaining_ladder` and `next_slice` wrote "40". Not found by the
    two passes that produced the brief; found by sweeping every path from a slice
    id to a renderer."""
    items = [_slice_str(i) for i in items]
    if len(items) <= 1:
        return "".join(items)
    return ", ".join(items[:-1]) + " and " + items[-1]


def _remaining_ladder(st):
    """Render the remaining ladder, including the explicit end-of-ladder fact."""
    remaining = " → ".join(_slice_str(n) for n in st["remaining_ladder"])
    return remaining or "none"


def _schema_context(st):
    """Render candidate and landed schema facts without conflating their scopes."""
    candidate = st["schema_version"]
    origin_main = st.get("origin_main_schema_version")
    if origin_main is not None and origin_main != candidate:
        return ("local candidate SCHEMA is %d; `origin/main` remains at %d until "
                "the candidate's unlanded schema migration lands"
                % (candidate, origin_main))
    return "SCHEMA is %d" % candidate


def render_master_ladder_progress(st):
    """master §4, the 0.8.20 row: landed slices + SHAs, SCHEMA, remaining ladder.

    Fenced sub-region only. The `**✅ LADDER PROGRESS (F-32, verified from git @
    ...):` lead-in and the closing `**` stay OUTSIDE the markers: the F-n
    reference is a findings-register citation and is deliberately not placed
    under generation."""
    by = _by_slice(st)
    landed = " · ".join("%s (`%s`)" % (_slice_str(n), by[n]["sha"]) for n in st["landed"])
    # `_slice_str`, not `str`: `render_plan_immediate_next` renders THIS SAME
    # FIELD through the helper, so a bare `str` here put the same fact in two
    # documents in two shapes (`40.0` here, `40` there).
    ladder = _remaining_ladder(st)
    if not landed:
        return ("No slices are LANDED on `origin/main`; %s; "
                "remaining ladder = %s." % (_schema_context(st), ladder))
    return ("Slices %s are all LANDED on `origin/main`; %s; "
            "remaining ladder = %s." % (landed, _schema_context(st), ladder))


PRE_SIGN_STATES = ("PRE_SIGNED", "NOT_PRE_SIGNED")


def publish_gate_facts(st):
    """Read `acceptance.publish_gate` STRICTLY, as THREE distinct facts.

    WHY THIS IS STRICT AND NOT A `.get()` PILE. The predecessor model carried a
    single `state_word`, and this file's `render_status_unblocks` rendered its
    whole publish sentence from it — so the board said "**Publish remains blocked
    on AC-079**, which is **still unsigned**" AFTER the HITL had PRE-SIGNED that
    delta (2026-07-25, master F-34). The state file already knew better: it
    carried a `pre_signed` provenance string the renderer never read. Collapsing
    "pre-signed but not yet minted" into "unsigned" told every reader a settled
    call was still open, which is how a settled call gets re-decided.

    The three facts, which a consumer must never re-collapse:
      1. `pre_sign_state` — has the HITL signed off on the CONTENT of the delta?
      2. `minted` / `sign_off_slice` — has the AC been minted and recorded SIGNED?
         PRE-SIGN IS NOT MINTING.
      3. `publish_gated_by` — what actually holds publish.

    Every lookup below is `[...]`, never `.get(...)`: a missing field must raise
    and fail the gate, because a renderer that quietly emits an empty string in
    place of a fact is the same failure in a new costume. The retired
    `state_word` is rejected outright so it cannot be reintroduced and silently
    re-read."""
    gate = st["acceptance"]["publish_gate"]
    for retired in ("state_word", "signed"):
        if retired in gate:
            raise ValueError(
                "acceptance.publish_gate carries the RETIRED field `%s`. It collapsed "
                "pre-sign, minting and the publish gate into one word and is what made "
                "the board claim an already-given signature was still outstanding. Model "
                "the facts separately: pre_sign_state + pre_sign, minted + minted_as + "
                "sign_off_slice, publish_gated_by." % retired)

    state = gate["pre_sign_state"]
    if state not in PRE_SIGN_STATES:
        raise ValueError(
            "acceptance.publish_gate.pre_sign_state is %r; it must be one of %s. An "
            "unrecognised value must fail loudly rather than render a blank or guessed "
            "claim about whether the HITL has signed." % (state, " or ".join(PRE_SIGN_STATES)))
    if state == "PRE_SIGNED" and not isinstance(gate.get("pre_sign"), dict):
        raise ValueError(
            "acceptance.publish_gate.pre_sign_state is PRE_SIGNED but there is no "
            "`pre_sign` object recording WHO signed, WHEN, on what authority, and what "
            "the pre-sign is PINNED to. A pre-sign with no provenance is not citable.")
    return gate


def render_publish_gate_sentence(gate):
    """The publish-gate sentence, branched on the FACTS — never one status word.

    Three branches because there are three genuinely different states, and the
    not-pre-signed branch is load-bearing: without it this would be a hardcoded
    happy path rather than a model, and a gate that really is awaiting sign-off
    would read as settled."""
    ac = gate["ac"]

    # `_slice_str`, NEVER `Slice %d` fed by `int(...)`, in ALL THREE branches.
    # `int(39.5)` renders "Slice 39" — the board naming a DIFFERENT slice as the
    # one that signed off, with this gate's authority behind it. Each branch
    # carries its own copy of the sentence, so a fix applied to one leaves the
    # other two live; all three are driven by their own arm (11h).
    if gate["minted"]:
        return ("**%s is MINTED and recorded as %s** at Slice %s (%s), covering %s. "
                "**Publish is gated by %s, not by this AC.**"
                % (ac, gate["minted_as"], _slice_str(gate["sign_off_slice"]),
                   gate["board_ref"], gate["covers"], gate["publish_gated_by"]))

    if gate["pre_sign_state"] == "PRE_SIGNED":
        ps = gate["pre_sign"]
        return ("**%s is PRE-SIGNED** — the %s signed off on %s on %s (%s), pinned to the "
                "content of `%s`; %s. Pre-signing is NOT minting: %s is minted and recorded "
                "as %s at Slice %s (%s). **Publish is gated by %s, not by this AC.**"
                % (ac, ps["by"], gate["covers"], ps["on"], ps["source"], ps["pinned_to"],
                   ps["reopens_if"], ac, gate["minted_as"],
                   _slice_str(gate["sign_off_slice"]),
                   gate["board_ref"], gate["publish_gated_by"]))

    return ("**Publish remains blocked on %s**, which is **NOT pre-signed** — %s still "
            "awaits HITL sign-off, and %s is minted and recorded as %s at Slice %s (%s). "
            "Publish is additionally gated by %s."
            % (ac, gate["covers"], ac, gate["minted_as"],
               _slice_str(gate["sign_off_slice"]),
               gate["board_ref"], gate["publish_gated_by"]))


def render_status_unblocks(st):
    """STATUS §1 `**Unblocks**` row: what is unblocked, by what, the
    publish-precondition slice, and the publish gate as three distinct facts."""
    by   = _by_slice(st)
    ub   = st["unblocked"]
    src  = st["unblocked_by"]
    # THE KEY AS STORED — never `int()`. `by[int(30.5)]` is `by[30]`, so the
    # renderer silently picked the INTEGER NEIGHBOUR'S ladder entry and printed
    # its short name, title and dependency list as the fractional slice's own.
    # That is the exact collapse `_by_slice`'s docstring was written to forbid,
    # bypassed at the one call site that indexes it (arm 11d).
    key = st["publish_precondition_slice"]
    if key not in by:
        raise ValueError(
            "`publish_precondition_slice` is %s but the ladder carries no such slice, "
            "so the board would name a publish precondition that does not exist. "
            "Fix the state file; do NOT round the id onto a neighbouring slice."
            % _slice_str(key))
    pre  = by[key]
    gate = publish_gate_facts(st)
    # `Slice %s` via `_slice_str`, NOT `Slice %d`. This `%d` is fed by
    # `pre["slice"]`, NOT by the lookup above — a SECOND, INDEPENDENT defect at
    # the same site, proven by execution in arm 11e2: a gate with the index fixed
    # and the render reverted emits `Slice 30 (H7b)`, the right entry under the
    # wrong number. `depends_on` is a list of slice ids too, and was joined with
    # bare `str()` (arm 11f) — not in the brief's measured set either.
    return ("**Slices %s are NOW UNBLOCKED** — %s (%s) now exists. "
            "Slice %s (%s) depends on %s. %s"
            % (_and_join(ub), src["requirement"], src["gloss"],
               _slice_str(pre["slice"]), pre["short"],
               "/".join(_slice_str(d) for d in pre["depends_on"]),
               render_publish_gate_sentence(gate)))


def render_handoff_next_step(st):
    """Steward hand-off ★ IMMEDIATE NEXT STEP: landed chain + next slice.

    The leading newline is NOT cosmetic. This region starts a markdown
    PARAGRAPH, and CommonMark treats a line that BEGINS with `<!--` as an HTML
    block that swallows the whole line — so an inline BEGIN marker in front of
    the sentence would stop the sentence rendering as markdown. The marker
    therefore sits on its own line and the region content starts with the
    newline that follows it."""
    # `_slice_str`, not `str`: `render_master_ladder_progress` renders THIS SAME
    # FIELD through the helper (:167), so a bare `str` here rendered the landed
    # chain one way in the master and another way in the hand-off.
    chain = " → ".join(_slice_str(n) for n in st["landed"])
    return ("\n**The %s ladder is between slices: %s are all LANDED; %s is next.**"
            % (st["release"], chain, _slice_str(st["next_slice"])))

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

def render_plan_immediate_next(st):
    """`plan-<release>.md` §9's IMMEDIATE NEXT pointer (TC-89).

    THE MEASURED FAILURE. That pointer was HAND-WRITTEN while this file already
    owned `next_slice` and already kept it true at every landing, and it went
    stale at THREE CONSECUTIVE COMMISSIONS — twice inside one session. At the
    Slice 21 commission it still read "Slice 30" although Slice 30 had landed at
    9b3ed0e3; it was fixed at 62486a01 only because the Steward happened to read
    the anchor the manifest cited. Hours later, right after Slice 21 landed, it
    read "Slice 21".

    WHY IT MATTERS MORE THAN ORDINARY DOC ROT. `scripts/commission-manifest.sh`
    resolves `## 9. Immediate next slice` as the `{{MANDATE}}` anchor of an
    orchestrator brief, and its CHECK 2 verifies that the HEADING EXISTS — not
    that the prose beneath it is current. A heading cannot rot; the prose under
    it does. So a generated, path-verified brief handed an orchestrator a
    hand-written pointer at the wrong slice, with the manifest's authority behind
    it, and the staleness was COPIED INTO THE NEXT COMMISSION. The mitigation
    that used to sit above it — a blockquote telling the reader to re-verify
    against `next_slice` — is a NOTE, not a control; this renderer is the control.

    THE FACTS, all three already owned here and all three already updated at
    every landing: `next_slice`, that slice's ladder entry (`short` + `title`),
    and `remaining_ladder`. Nothing is inferred and no prose is laundered through
    the JSON — the surrounding §9 narrative (deliverables, guardrails, the
    historical roll-up) stays hand-written and OUTSIDE the markers, exactly as
    the other four views leave their surroundings alone.

    IT REFUSES RATHER THAN GUESSES. `next_slice: null` is a real end-of-release
    state, and a renderer that emitted "Slice None" would put a FABRICATED
    pointer in front of the next orchestrator with this gate's authority behind
    it — the same failure this view exists to close, wearing a different hat. So
    a non-integer `next_slice`, or one naming a slice the ladder does not carry,
    raises with the reason and the gate reports it (arm 10e).

    The leading newline is load-bearing for the same CommonMark reason
    render_handoff_next_step() documents: the BEGIN marker sits on its own line,
    so the region content starts with the newline that follows it.
    """
    nxt = st["next_slice"]
    # int OR float: the HITL may mint a fractional slice (steward seq-202 named
    # 'Slice 39.5' as a deliberate one-off). The substantive check is the
    # ladder-membership test below — integer-ness was only ever a proxy for it.
    # bool is excluded because True would otherwise pass as 1.
    if isinstance(nxt, bool) or not isinstance(nxt, (int, float)):
        raise ValueError(
            "`next_slice` is %r, not a slice number. The plan's IMMEDIATE NEXT pointer "
            "cannot be rendered from it, and rendering a blank — or the word Slice "
            "followed by a null — would put a fabricated pointer in front of the next "
            "orchestrator with this gate's authority behind it. "
            "If the ladder is finished, remove this view from "
            "`generated_views` and unfence the region — do not let it render nothing."
            % (nxt,))
    by = _by_slice(st)
    if nxt not in by:
        raise ValueError(
            "`next_slice` is %s but the ladder carries no such slice, so the pointer would "
            "name a slice that does not exist." % _slice_str(nxt))
    entry = by[nxt]
    return ("\n**IMMEDIATE NEXT: Slice %s** (`%s`) — %s\n\n**Remaining ladder:** %s."
            % (_slice_str(nxt), entry["short"], entry["title"],
               " → ".join(_slice_str(n) for n in st["remaining_ladder"])))


def render_status_current_state(st):
    """STATUS board's next-slice row, derived from the release state."""
    nxt = st["next_slice"]
    if isinstance(nxt, bool) or not isinstance(nxt, (int, float)):
        raise ValueError(
            "`next_slice` is %r, not a slice number. The STATUS board's next-slice "
            "pointer cannot be rendered from it." % (nxt,))
    by = _by_slice(st)
    if nxt not in by:
        raise ValueError(
            "`next_slice` is %s but the ladder carries no such slice, so the STATUS "
            "board would name a slice that does not exist." % _slice_str(nxt))
    entry = by[nxt]
    landed = " · ".join(
        "%s (`%s`)" % (_slice_str(item["slice"]), item["sha"])
        for item in st["ladder"] if item["slice"] in st["landed"])
    return ("**Next is Slice %s (%s), %s.** Landed on `origin/main`: %s — "
            "verified reachable, not asserted."
            % (_slice_str(nxt), entry["short"], entry["status"], landed))


def render_status_next_action(st):
    """STATUS board's lifecycle-aware next action, derived from state.

    `next_slice` alone says which slice controls the release, not what a
    reader should do with it. A `REVIEWED_PENDING_INTEGRATION` entry has
    already completed implementation and independent review; telling the next
    orchestrator to commission it again is a false instruction. It must be
    landed through the repository's integration path instead. Likewise,
    `PREP_COMPLETE_PUBLISH_HELD` is not a commission: publication needs a new
    explicit authorization. Other live states retain the ordinary commission
    action.
    """
    nxt = st["next_slice"]
    entry = _by_slice(st)[nxt]
    action = {
        "REVIEWED_PENDING_INTEGRATION": "Land reviewed Slice",
        "PREP_COMPLETE_PUBLISH_HELD": "Await explicit publication authorization for Slice",
    }.get(entry["status"], "Commission Slice")
    return ("**%s %s (%s)** — %s. **Remaining ladder:** %s."
            % (action, _slice_str(nxt), entry["short"], entry["title"],
               " → ".join(_slice_str(item) for item in st["remaining_ladder"])))


def render_plan_landed_roll_up(st):
    """`plan-<release>.md` §9's LANDED roll-up (TC-89, second site).

    THE MEASURED FAILURE. §9 carried a SECOND hand-written pointer, below the
    generated IMMEDIATE NEXT block, phrased incrementally ("Landed since the
    Slice-20 narration above: ..."). Because it named only the slices landed
    since some earlier narration, every landing made it wrong by omission rather
    than by contradiction — the quietest possible rot. It named 25, 30, TC-86 and
    21 while 22, 23, 31, 32, 33 and 39 had all landed, and it went stale at three
    consecutive commissions, the same count that closed TC-89 for the pointer
    above it.

    WHY IT MATTERS. `## 9. Immediate next slice` is the `{{MANDATE}}` anchor of
    every generated commission brief, so a reader who reaches this section is a
    reader about to commission a slice. Two pointers under one anchor, one
    generated and one not, is worse than one hand-written pointer: the generated
    one lends its authority to the stale one.

    ABSOLUTE, NOT INCREMENTAL. It renders the WHOLE landed set from `landed` +
    each entry's `sha`, so there is no "since when" for a future landing to make
    silently wrong. The surrounding historical narrative stays hand-written and
    OUTSIDE the markers.

    The leading newline is load-bearing for the CommonMark reason
    render_handoff_next_step() documents.
    """
    by = _by_slice(st)
    landed = " · ".join("%s (`%s`)" % (_slice_str(n), by[n]["sha"]) for n in st["landed"])
    if not landed:
        return ("\n**LANDED on `origin/main`, in full:** no slices. %s; "
                "remaining ladder = %s."
                % (_schema_context(st), _remaining_ladder(st)))
    return ("\n**LANDED on `origin/main`, in full:** Slices %s. %s; remaining ladder = %s."
            % (landed, _schema_context(st),
               _remaining_ladder(st)))


RENDERERS = {
    "master-ladder-progress":  render_master_ladder_progress,
    "plan-landed-roll-up":     render_plan_landed_roll_up,
    "status-unblocks":         render_status_unblocks,
    "status-live-open-count":  render_status_live_open_count,
    "handoff-next-step":       render_handoff_next_step,
    "plan-immediate-next":     render_plan_immediate_next,
    "status-current-state":    render_status_current_state,
    "status-next-action":      render_status_next_action,
}


def validate_ladder_progress(st):
    """Reject contradictory terminal and next-slice facts before rendering."""
    landed = st.get("landed")
    remaining = st.get("remaining_ladder")
    next_slice = st.get("next_slice")
    if not isinstance(landed, list):
        raise ValueError("`landed` must be a list of slice ids")
    if not isinstance(remaining, list):
        raise ValueError("`remaining_ladder` must be a list of slice ids")
    overlap = sorted({_slice_str(item) for item in landed}
                     & {_slice_str(item) for item in remaining})
    if overlap:
        raise ValueError(
            "`landed` and `remaining_ladder` overlap at slice(s) %s; a slice cannot "
            "be both landed and remaining" % ", ".join(overlap))
    if not remaining:
        if next_slice is not None:
            raise ValueError(
                "`remaining_ladder` is empty but `next_slice` is %r; a terminal "
                "release must set `next_slice` to null" % (next_slice,))
        return
    if next_slice != remaining[0]:
        raise ValueError(
            "`remaining_ladder` starts at %s but `next_slice` is %r; a live release "
            "must name its first remaining slice as next"
            % (_slice_str(remaining[0]), next_slice))

# ---------------------------------------------------------------------------
# Discover tracked inputs. A stale linked worktree's state or Markdown copy is
# not this checkout's contract and cannot affect its generated-view verdict.
# ---------------------------------------------------------------------------
def tracked(pattern):
    result = subprocess.run(
        ["git", "--no-optional-locks", "ls-files", "-z", "--", pattern],
        check=False, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    if result.returncode:
        raise RuntimeError(result.stderr.decode("utf-8", "replace").strip())
    return sorted(p.decode("utf-8") for p in result.stdout.split(b"\0") if p)

try:
    state_paths = tracked(":(glob)dev/plans/release-state-*.json")
    markdown_paths = tracked(":(glob)**/*.md")
except RuntimeError as exc:
    print("FAIL check-release-state-views: cannot list tracked inputs — %s" % exc,
          file=sys.stderr)
    sys.exit(2)
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

    try:
        validate_ladder_progress(st)
    except ValueError as exc:
        bad("FAIL %s: invalid ladder progress — %s" % (sp, exc))
        continue

    # The `origin/main` claim the views are about to render is a fact about the
    # remote, not about this state file. Verify it before rendering it.
    if isinstance(st.get("landed"), list) and st.get("ladder"):
        check_remote_landing(release, st)

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
        # A renderer that cannot produce its bytes must fail with the REASON, not
        # a traceback and not a blank region: the fact model refusing to render is
        # a real finding about the state file (a retired field reintroduced, an
        # unrecognised enum value, a missing fact) and the operator has to be told
        # which one. Rendering "" and diffing it would delete the claim instead.
        try:
            want = RENDERERS[vid](st)
        except KeyError as exc:                               # noqa: PERF203
            bad("FAIL %s: view `%s` could NOT be rendered from %s — the state file is\n"
                "  MISSING the required fact %s. A renderer must never substitute an empty\n"
                "  string for a fact it cannot find; that is how a claim silently changes."
                % (path, vid, sp, exc))
            continue
        except (ValueError, TypeError) as exc:                # noqa: BLE001
            detail = exc.args[0] if exc.args else exc
            bad("FAIL %s: view `%s` could NOT be rendered from %s —\n    %s\n"
                "  A view that cannot be rendered is a hard failure, never an empty region."
                % (path, vid, sp, detail))
            continue
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
# RULE 3 — orphan-marker scan (the confinement rule), over tracked Markdown
# only. Scanning the physical worktree makes an unrelated stale linked worktree
# a false owner and violates the single-checkout contract.
# ---------------------------------------------------------------------------
for p in markdown_paths:
    try:
        with open(p, encoding="utf-8", errors="replace") as fh:
            text = fh.read()
    except OSError:
        bad("FAIL %s: tracked Markdown input could not be read." % p)
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
