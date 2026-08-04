#!/usr/bin/env bash
# scripts/tests/test_check_release_state_views.sh — T2a recurrence guard
# (DOC-HYGIENE-2): the single-writer release-state file + its generated views.
#
# The incident this closes: 0.8.20's release state — landed slices + SHAs, the
# SCHEMA version, the next slice, AC status, ruled/unruled decisions — was
# narrated across a 5-12 file write-side fan-out with NOTHING checking that the
# copies agreed. One reconciliation commit (b70629e5) had to touch SEVEN files,
# and the live board is ~79 KB, so nobody reads it whole and a wrong copy can
# sit unnoticed for weeks.
#
# Predicate under test (see scripts/check-release-state-views.sh for the full
# statement): for every view a `dev/plans/release-state-*.json` file declares,
# the bytes between its BEGIN/END markers must EQUAL the bytes the renderer
# produces from that state file — plus marker well-formedness, the orphan-marker
# confinement rule, an unknown-view-id failure, and the TC-37 vacuous-pass guard.
#
# RED-first: the predicate passes on the real repo today, so asserting against
# the real checkout alone would prove nothing (a `true` script would pass it).
# Every failure arm below runs against a purpose-built fixture repo in which the
# fault is deliberately planted, so an arm can only go green because the
# predicate actually fired. The real-repo arm is the regression half of the pair.
#
# NON-VACUITY: the suite honours $GATE_UNDER_TEST, so a MUTANT gate (one whose
# region diff always compares equal) can be pointed at it. That mutant turns
# this suite RED — which is the evidence that a green here is load-bearing
# rather than a script that merely exits 0.
#
# Isolation: fixtures are throwaway git repos under mktemp -d (the gate does
# `cd "$(git rev-parse --show-toplevel)"`, so each fixture needs to BE a repo).
# Nothing here writes into the real checkout.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="${GATE_UNDER_TEST:-$REPO_ROOT/scripts/check-release-state-views.sh}"
CI_YML="$REPO_ROOT/.github/workflows/ci.yml"

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

TMPROOT="$(mktemp -d)"
cleanup() {
  case "$TMPROOT" in
    "${TMPDIR:-/tmp}"/*|/tmp/*) rm -rf "$TMPROOT" ;;
    *) printf 'refusing to remove unexpected temp path: %s\n' "$TMPROOT" >&2 ;;
  esac
}
trap cleanup EXIT

FIX="$TMPROOT/repo"
OUT=""
RC=0

B_MASTER='<!-- BEGIN GENERATED release-state:9.9.9:master-ladder-progress -->'
E_MASTER='<!-- END GENERATED release-state:9.9.9:master-ladder-progress -->'
B_HANDOFF='<!-- BEGIN GENERATED release-state:9.9.9:handoff-next-step -->'
E_HANDOFF='<!-- END GENERATED release-state:9.9.9:handoff-next-step -->'
# T2b: the live-open-set COUNT in the board's §4 banner.
B_OPEN='<!-- BEGIN GENERATED release-state:9.9.9:status-live-open-count -->'
E_OPEN='<!-- END GENERATED release-state:9.9.9:status-live-open-count -->'
# The board's §1 `**Unblocks**` cell — the publish-gate sentence.
B_UNBLOCKS='<!-- BEGIN GENERATED release-state:9.9.9:status-unblocks -->'
E_UNBLOCKS='<!-- END GENERATED release-state:9.9.9:status-unblocks -->'

# The publish-gate fact set, in each of the three states the model must keep
# DISTINCT. This is the defect that made these arms necessary: the predecessor
# model carried ONE `state_word: "unsigned"`, and `status-unblocks` rendered from
# it — so the board told readers publish was blocked awaiting an AC-079 signature
# the HITL had ALREADY GIVEN (pre-signed 2026-07-25, master F-34). "Pre-signed
# but not yet minted" and "not signed at all" are different facts and the fixture
# proves the renderer branches on them rather than on one collapsed word.
#
# GATE_SENTENCE is written out LONGHAND here, deliberately: it is an INDEPENDENT
# restatement of what the renderer must emit, so a renderer change that alters
# the claim cannot also silently alter the expectation.
gate_facts() {
  case "${1:-presigned}" in
    presigned)
      GATE_JSON='"ac": "AC-999",
      "covers": "the accumulated governed-surface delta",
      "pre_sign_state": "PRE_SIGNED",
      "pre_sign": {"on": "2026-01-02", "by": "HITL", "source": "master F-99",
                   "pinned_to": "src/conformance/governed-surface-allowlist.json",
                   "reopens_if": "any diff to that file re-opens it (the pin)"},
      "minted": false, "minted_as": "SIGNED", "sign_off_slice": 40,
      "publish_gated_by": "the separate HITL publish gate",
      "board_ref": "§4 #1"'
      GATE_SENTENCE='**AC-999 is PRE-SIGNED** — the HITL signed off on the accumulated governed-surface delta on 2026-01-02 (master F-99), pinned to the content of `src/conformance/governed-surface-allowlist.json`; any diff to that file re-opens it (the pin). Pre-signing is NOT minting: AC-999 is minted and recorded as SIGNED at Slice 40 (§4 #1). **Publish is gated by the separate HITL publish gate, not by this AC.**'
      ;;
    notpresigned)
      GATE_JSON='"ac": "AC-999",
      "covers": "the accumulated governed-surface delta",
      "pre_sign_state": "NOT_PRE_SIGNED",
      "minted": false, "minted_as": "SIGNED", "sign_off_slice": 40,
      "publish_gated_by": "the separate HITL publish gate",
      "board_ref": "§4 #1"'
      GATE_SENTENCE='**Publish remains blocked on AC-999**, which is **NOT pre-signed** — the accumulated governed-surface delta still awaits HITL sign-off, and AC-999 is minted and recorded as SIGNED at Slice 40 (§4 #1). Publish is additionally gated by the separate HITL publish gate.'
      ;;
    minted)
      GATE_JSON='"ac": "AC-999",
      "covers": "the accumulated governed-surface delta",
      "pre_sign_state": "PRE_SIGNED",
      "pre_sign": {"on": "2026-01-02", "by": "HITL", "source": "master F-99",
                   "pinned_to": "src/conformance/governed-surface-allowlist.json",
                   "reopens_if": "any diff to that file re-opens it (the pin)"},
      "minted": true, "minted_as": "SIGNED", "sign_off_slice": 40,
      "publish_gated_by": "the separate HITL publish gate",
      "board_ref": "§4 #1"'
      GATE_SENTENCE='**AC-999 is MINTED and recorded as SIGNED** at Slice 40 (§4 #1), covering the accumulated governed-surface delta. **Publish is gated by the separate HITL publish gate, not by this AC.**'
      ;;
    retired-state-word)
      # The RETIRED field, deliberately reintroduced. A renderer must never be
      # able to read it again, and a state file carrying it must go red rather
      # than have a consumer quietly fall back to it.
      GATE_JSON='"ac": "AC-999",
      "covers": "the accumulated governed-surface delta",
      "state_word": "unsigned",
      "pre_sign_state": "PRE_SIGNED",
      "pre_sign": {"on": "2026-01-02", "by": "HITL", "source": "master F-99",
                   "pinned_to": "src/conformance/governed-surface-allowlist.json",
                   "reopens_if": "any diff to that file re-opens it (the pin)"},
      "minted": false, "minted_as": "SIGNED", "sign_off_slice": 40,
      "publish_gated_by": "the separate HITL publish gate",
      "board_ref": "§4 #1"'
      GATE_SENTENCE='(unrenderable)'
      ;;
    *) printf 'gate_facts: unknown mode %q\n' "$1" >&2; exit 2 ;;
  esac
}

# A minimal but REAL fixture: one state file, three fenced views in three
# documents, each region byte-identical to what the renderers emit. Every arm
# mutates one thing from this baseline, so a red arm isolates exactly one fault.
setup_fixture() {
  gate_facts "${1:-presigned}"
  rm -rf "$FIX"
  mkdir -p "$FIX/dev/plans/runs" "$FIX/scripts"
  cp "$GATE" "$FIX/scripts/check-release-state-views.sh"
  chmod +x "$FIX/scripts/check-release-state-views.sh"
  (cd "$FIX" && git init -q && git config user.email t@example.com && git config user.name t)

  cat >"$FIX/dev/plans/release-state-9.9.9.json" <<EOF
{
  "release": "9.9.9",
  "schema_version": 42,
  "ladder": [
    {"slice": 0,  "short": "X0",   "depends_on": [],          "status": "LANDED",      "sha": "aaaa1111"},
    {"slice": 5,  "short": "R-A",  "depends_on": [0],         "status": "LANDED",      "sha": "bbbb2222"},
    {"slice": 10, "short": "R-B",  "depends_on": [5],         "status": "UNBLOCKED",   "sha": null},
    {"slice": 20, "short": "R-C",  "depends_on": [5],         "status": "UNBLOCKED",   "sha": null},
    {"slice": 30, "short": "H7",   "depends_on": [5, 10, 20], "status": "NOT_STARTED", "sha": null},
    {"slice": 40, "short": "PUB",  "depends_on": [30],        "status": "NOT_STARTED", "sha": null}
  ],
  "landed": [0, 5],
  "next_slice": 10,
  "remaining_ladder": [10, 20, 30, 40],
  "unblocked": [10, 20],
  "unblocked_by": {"requirement": "R-A", "gloss": "the thing"},
  "publish_precondition_slice": 30,
  "acceptance": {
    "publish_gate": {
      ${GATE_JSON}
    }
  },
  "decisions": {
    "unruled": [
      {"id": "batched-surface", "title": "the batched surface decision"},
      {"id": "publish",         "title": "PUBLISH"}
    ],
    "ruled": [
      {"id": "already-settled", "ruling": "CLOSED BY DECISION", "ruled_on": "2026-01-01"}
    ]
  },
  "generated_views": [
    {"id": "master-ladder-progress",  "file": "dev/plans/master.md"},
    {"id": "status-unblocks",         "file": "dev/plans/runs/board.md"},
    {"id": "status-live-open-count",  "file": "dev/plans/runs/board.md"},
    {"id": "handoff-next-step",       "file": "dev/plans/runs/handoff.md"}
  ]
}
EOF

  # T2b: the board's §4 banner. The fence is deliberately NARROW — the count
  # word only — and it sits INSIDE a blockquote, mid-line. Both properties are
  # load-bearing and are asserted below: a BEGIN marker at the head of a
  # blockquote line would be an HTML block that swallows the whole line
  # (CommonMark), and the surrounding sentence is not renderable from facts.
  cat >"$FIX/dev/plans/runs/board.md" <<EOF
# Board

## 1. Current state

| | |
|---|---|
| **Unblocks** | ${B_UNBLOCKS}**Slices 10 and 20 are NOW UNBLOCKED** — R-A (the thing) now exists. Slice 30 (H7) depends on 5/10/20. ${GATE_SENTENCE}${E_UNBLOCKS} |

## 4. Open HITL decisions

> **⚠ HISTORICAL QUEUE, NOT THE LIVE OPEN SET.** Rows 1-3 are retained as the
> decision record; do not act on them as open.
>
> **THE LIVE OPEN SET IS EXACTLY ${B_OPEN}TWO${E_OPEN}:** (1) the batched
> surface decision; and (2) PUBLISH (hard gate).

| # | Decision | Recommendation |
|---|---|---|
| 1 | A settled thing | retained as the decision record |
EOF

  cat >"$FIX/dev/plans/master.md" <<EOF
# Master

Prose that is NOT generated and must never be touched.

| Release | Notes |
|---|---|
| **9.9.9** | Lead-in prose. **✅ LADDER PROGRESS: ${B_MASTER}Slices 0 (\`aaaa1111\`) · 5 (\`bbbb2222\`) are all LANDED on \`origin/main\`; SCHEMA is 42; remaining ladder = 10 → 20 → 30 → 40.${E_MASTER}** Trailing prose. |
EOF

  cat >"$FIX/dev/plans/runs/handoff.md" <<EOF
# Hand-off

## Next step

${B_HANDOFF}
**The 9.9.9 ladder is between slices: 0 → 5 are all LANDED; 10 is next.**${E_HANDOFF} More prose.
EOF

  (cd "$FIX" && git add -A && git commit -qm 'fixture: tracked baseline')
}

run_gate() {
  set +e
  OUT="$(cd "$FIX" && ./scripts/check-release-state-views.sh "$@" 2>&1)"
  RC=$?
  set -e
}

# --- Arm 0: the BASELINE fixture is GREEN ----------------------------------
# Without this, every RED arm below could be passing for an unrelated reason.
setup_fixture
run_gate
if [ "$RC" -eq 0 ]; then
  pass "baseline fixture — every fenced region matches its render (exit 0)"
else
  fail "arm 0 (baseline green): rc=$RC out=$OUT"
fi

# --- Arm 0b: stale nested worktrees are not this checkout's Markdown --------
# The gate used to walk the physical tree, so an untracked linked-worktree copy
# under `.claude/worktrees` could contribute an orphan marker and fail the
# current checkout. Only `git ls-files` inputs are contractual.
mkdir -p "$FIX/.claude/worktrees/stale/dev/plans/runs"
printf '%s\n' '<!-- BEGIN GENERATED release-state:9.9.9:unowned -->' \
  >"$FIX/.claude/worktrees/stale/dev/plans/runs/STATUS-9.9.9.md"
run_gate
if [ "$RC" -eq 0 ]; then
  pass "untracked nested worktree Markdown is excluded from the orphan scan"
else
  fail "arm 0b (tracked-only scan): rc=$RC out=$OUT"
fi

# An untracked state file must be ignored too: only tracked writers can own
# generated views in this checkout.
printf '%s\n' '{ not valid JSON' >"$FIX/dev/plans/release-state-8.8.8.json"
run_gate
if [ "$RC" -eq 0 ]; then
  pass "untracked release-state JSON is excluded from discovery"
else
  fail "arm 0c (tracked-only state discovery): rc=$RC out=$OUT"
fi

# --- Arm 1: STALE BLOCK — a hand-edit INSIDE the markers -------------------
# The primary failure this gate exists to catch: somebody "just fixes" a SHA in
# the prose and the state file no longer agrees.
setup_fixture
perl -0777 -pi -e 's/bbbb2222/deadbeef/' "$FIX/dev/plans/master.md"
run_gate
if [ "$RC" -ne 0 ] && grep -q 'is STALE' <<<"$OUT" && grep -q 'master-ladder-progress' <<<"$OUT"; then
  pass "stale block — a hand-edit inside the markers HARD-fails and names the block"
else
  fail "arm 1 (stale block): rc=$RC out=$OUT"
fi

# --- Arm 1b: the failure message shows BOTH sides of the diff --------------
# A gate that says "stale" without showing what differs makes the fix a guess.
if grep -q 'IN THE DOCUMENT' <<<"$OUT" && grep -q 'RENDERED FROM THE STATE FILE' <<<"$OUT"; then
  pass "the stale message prints the document bytes AND the rendered bytes"
else
  fail "arm 1b (diagnostic shows both sides): out=$OUT"
fi

# --- Arm 2: STATE DRIFT — a fact changes and nothing is regenerated --------
# The other direction of the same seam, and the one the fan-out actually
# produced: the single writer is updated, the restatements are not.
setup_fixture
perl -0777 -pi -e 's/"schema_version": 42/"schema_version": 43/' \
  "$FIX/dev/plans/release-state-9.9.9.json"
run_gate
if [ "$RC" -ne 0 ] && grep -q 'is STALE' <<<"$OUT" && grep -q 'SCHEMA is 43' <<<"$OUT"; then
  pass "state drift — a fact changed without a regenerate HARD-fails"
else
  fail "arm 2 (state drift): rc=$RC out=$OUT"
fi

# --- Arm 2b: --write repairs the drift, and --check is then green ----------
run_gate --write
if [ "$RC" -eq 0 ] && grep -q 'SCHEMA is 43' "$FIX/dev/plans/master.md"; then
  run_gate
  if [ "$RC" -eq 0 ]; then
    pass "--write regenerates in place; --check is green immediately after"
  else
    fail "arm 2b (check after write): rc=$RC out=$OUT"
  fi
else
  fail "arm 2b (--write): rc=$RC out=$OUT"
fi

# --- Arm 2c: --write is a NO-OP on an already-current tree -----------------
# Reproduction proof in miniature: if regenerating a current document changed
# it, the renderer would not be reproducing what it claims to own.
setup_fixture
BEFORE="$(cat "$FIX/dev/plans/master.md" "$FIX/dev/plans/runs/handoff.md" "$FIX/dev/plans/runs/board.md")"
run_gate --write
AFTER="$(cat "$FIX/dev/plans/master.md" "$FIX/dev/plans/runs/handoff.md" "$FIX/dev/plans/runs/board.md")"
if [ "$RC" -eq 0 ] && [ "$BEFORE" = "$AFTER" ]; then
  pass "--write on a current tree is byte-for-byte a no-op (the renderer reproduces)"
else
  fail "arm 2c (write is a no-op): rc=$RC"
fi

# --- Arm 2d (T2b): the LIVE-OPEN-SET COUNT drifts from the single writer ---
# THE measured failure this tranche exists for: the board's §4 listed >=4
# ALREADY-RULED items as still open. The count of the live open set is the one
# fact in that banner a renderer can derive, so a third unruled item appearing
# in the state file must turn the board RED until it is regenerated.
setup_fixture
perl -0777 -pi -e 's/\{"id": "publish",         "title": "PUBLISH"\}/{"id": "publish", "title": "PUBLISH"},\n      {"id": "third-thing", "title": "a newly-opened call"}/' \
  "$FIX/dev/plans/release-state-9.9.9.json"
run_gate
if [ "$RC" -ne 0 ] && grep -q 'is STALE' <<<"$OUT" \
   && grep -q 'status-live-open-count' <<<"$OUT" && grep -q "'THREE'" <<<"$OUT"; then
  pass "live-open-set drift — a THIRD unruled decision HARD-fails the board's count"
else
  fail "arm 2d (live-open-set drift): rc=$RC out=$OUT"
fi

# --- Arm 2e (T2b): the opposite direction — an item gets RULED -------------
# The exact shape of the incident: a decision is settled, the state file records
# it, and the board still says "EXACTLY TWO". That must be red, not silent.
setup_fixture
perl -0777 -pi -e 's/\{"id": "batched-surface", "title": "the batched surface decision"\},\n\s*//' \
  "$FIX/dev/plans/release-state-9.9.9.json"
run_gate
if [ "$RC" -ne 0 ] && grep -q 'status-live-open-count' <<<"$OUT" && grep -q "'ONE'" <<<"$OUT"; then
  pass "a decision becoming RULED shrinks the count and HARD-fails a stale board"
else
  fail "arm 2e (ruled item shrinks the count): rc=$RC out=$OUT"
fi

# --- Arm 2f (T2b): a hand-edit of the count word inside the markers --------
setup_fixture
perl -0777 -pi -e 's/\Q'"$B_OPEN"'\ETWO/'"$B_OPEN"'FOUR/' "$FIX/dev/plans/runs/board.md"
run_gate
if [ "$RC" -ne 0 ] && grep -q 'is STALE' <<<"$OUT" && grep -q 'status-live-open-count' <<<"$OUT"; then
  pass "hand-editing the count inside the markers HARD-fails"
else
  fail "arm 2f (hand-edited count): rc=$RC out=$OUT"
fi

# --- Arm 2g (T2b): --write repairs the count and DELETES NOTHING -----------
# Bounding condition 3 of the HITL pre-sign, on the section that carries the
# historical decision record: regenerating must touch the count word and
# nothing else — the retained rows and the banner prose stay byte-identical.
setup_fixture
perl -0777 -pi -e 's/\{"id": "publish",         "title": "PUBLISH"\}/{"id": "publish", "title": "PUBLISH"},\n      {"id": "third-thing", "title": "a newly-opened call"}/' \
  "$FIX/dev/plans/release-state-9.9.9.json"
run_gate --write
if [ "$RC" -eq 0 ] \
   && grep -q "${B_OPEN}THREE${E_OPEN}" "$FIX/dev/plans/runs/board.md" \
   && grep -q 'HISTORICAL QUEUE, NOT THE LIVE OPEN SET' "$FIX/dev/plans/runs/board.md" \
   && grep -q 'retained as the decision record' "$FIX/dev/plans/runs/board.md" \
   && grep -q '(1) the batched' "$FIX/dev/plans/runs/board.md" \
   && grep -q '(2) PUBLISH (hard gate)' "$FIX/dev/plans/runs/board.md"; then
  pass "--write updates ONLY the count; the retained rows and banner prose survive"
else
  fail "arm 2g (write deletes nothing): rc=$RC"
fi

# --- Arm 2h (T2b): the fence does not start a blockquote line --------------
# CommonMark: a blockquote line whose content BEGINS with `<!--` is an HTML
# block that swallows the REST OF THE LINE, so a BEGIN marker placed at the head
# of the banner line would stop the sentence rendering as markdown. This is the
# same hazard render_handoff_next_step documents; assert the placement rather
# than trusting it to stay right.
setup_fixture
if ! grep -qE '^>[[:space:]]*<!-- BEGIN GENERATED' "$FIX/dev/plans/runs/board.md" \
   && grep -qE '^> \*\*THE LIVE OPEN SET' "$FIX/dev/plans/runs/board.md"; then
  pass "the blockquote fence is mid-line — no marker heads a quoted line (CommonMark)"
else
  fail "arm 2h (marker placement): a BEGIN marker heads a blockquote line"
fi

# ===========================================================================
# THE PUBLISH-GATE MODEL (arms 2i-2n). The incident: `status-unblocks` rendered
# "**Publish remains blocked on AC-079**, which is **still unsigned**" from a
# single `state_word`, while the state file's own `pre_signed` field recorded
# that the HITL had PRE-SIGNED that delta on 2026-07-25 (master F-34). The
# sentence told every reader a settled call was still open, which is how a
# settled call gets re-decided. These arms pin that the renderer branches on
# DISTINCT facts — pre-sign, minting, and who actually gates publish — in BOTH
# directions, so the fix cannot be "hardcode the pre-signed wording".
# ===========================================================================

# --- Arm 2i: PRE-SIGNED renders the pre-sign, and NOT a stale "unsigned" ---
setup_fixture presigned
run_gate
CELL="$(perl -0777 -ne 'print $1 if /\Q'"$B_UNBLOCKS"'\E(.*?)\Q'"$E_UNBLOCKS"'\E/s' \
  "$FIX/dev/plans/runs/board.md")"
if [ "$RC" -eq 0 ] \
   && grep -q 'is PRE-SIGNED' <<<"$CELL" \
   && grep -q 'Pre-signing is NOT minting' <<<"$CELL" \
   && grep -q 'minted and recorded as SIGNED at Slice 40' <<<"$CELL" \
   && grep -q 'Publish is gated by the separate HITL publish gate' <<<"$CELL" \
   && ! grep -q 'still unsigned' <<<"$CELL" \
   && ! grep -q 'Publish remains blocked on AC-999' <<<"$CELL"; then
  pass "pre-signed gate — renders pre-sign + mints-at-40 + the SEPARATE publish gate"
else
  fail "arm 2i (pre-signed render): rc=$RC cell=$CELL"
fi

# --- Arm 2j: the NOT-pre-signed direction still says BLOCKED ---------------
# Without this arm the fix would be indistinguishable from hardcoding the happy
# path: a gate that is genuinely awaiting sign-off must still read as blocked.
setup_fixture notpresigned
run_gate
CELL="$(perl -0777 -ne 'print $1 if /\Q'"$B_UNBLOCKS"'\E(.*?)\Q'"$E_UNBLOCKS"'\E/s' \
  "$FIX/dev/plans/runs/board.md")"
if [ "$RC" -eq 0 ] \
   && grep -q 'Publish remains blocked on AC-999' <<<"$CELL" \
   && grep -q 'NOT pre-signed' <<<"$CELL" \
   && grep -q 'still awaits HITL sign-off' <<<"$CELL" \
   && ! grep -q 'is PRE-SIGNED' <<<"$CELL"; then
  pass "NOT-pre-signed gate — still renders a blocked / awaiting-sign-off sentence"
else
  fail "arm 2j (not-pre-signed render): rc=$RC cell=$CELL"
fi

# --- Arm 2k: the MINTED direction -----------------------------------------
setup_fixture minted
run_gate
CELL="$(perl -0777 -ne 'print $1 if /\Q'"$B_UNBLOCKS"'\E(.*?)\Q'"$E_UNBLOCKS"'\E/s' \
  "$FIX/dev/plans/runs/board.md")"
if [ "$RC" -eq 0 ] \
   && grep -q 'is MINTED and recorded as SIGNED' <<<"$CELL" \
   && ! grep -q 'Publish remains blocked on AC-999' <<<"$CELL"; then
  pass "minted gate — renders the completed sign-off, not a pending one"
else
  fail "arm 2k (minted render): rc=$RC cell=$CELL"
fi

# --- Arm 2l: the renderer BRANCHES — flipping the fact turns the doc red ---
# The sharpest form of the question "did you fix the model or the wording?": the
# document keeps the pre-signed sentence, the state file flips to NOT pre-signed,
# and the gate must go STALE with the blocked wording on the rendered side.
setup_fixture presigned
python3 - "$FIX/dev/plans/release-state-9.9.9.json" <<'PY'
import json, sys
p = sys.argv[1]
st = json.load(open(p, encoding="utf-8"))
g = st["acceptance"]["publish_gate"]
g["pre_sign_state"] = "NOT_PRE_SIGNED"
g.pop("pre_sign", None)
json.dump(st, open(p, "w", encoding="utf-8"), ensure_ascii=False, indent=2)
PY
run_gate
if [ "$RC" -ne 0 ] && grep -q 'is STALE' <<<"$OUT" && grep -q 'status-unblocks' <<<"$OUT" \
   && grep -q 'NOT pre-signed' <<<"$OUT"; then
  pass "pre-sign is a FACT the renderer reads — flipping it turns the stale board RED"
else
  fail "arm 2l (renderer branches on pre_sign_state): rc=$RC out=$OUT"
fi

# --- Arm 2m: the RETIRED `state_word` cannot come back silently ------------
# `state_word` is what collapsed the three facts into one. A state file that
# still carries it must HARD-fail: a consumer quietly reading it again, or a
# stale reference rendering an empty string, is the recurrence.
setup_fixture retired-state-word
run_gate
if [ "$RC" -ne 0 ] && grep -qi 'state_word' <<<"$OUT"; then
  pass "the retired \`state_word\` field HARD-fails — it cannot be reintroduced"
else
  fail "arm 2m (retired state_word): rc=$RC out=$OUT"
fi

# --- Arm 2n: an unknown pre_sign_state fails loudly, never renders blank ---
setup_fixture presigned
perl -0777 -pi -e 's/"PRE_SIGNED"/"MAYBE"/' "$FIX/dev/plans/release-state-9.9.9.json"
run_gate
if [ "$RC" -ne 0 ] && grep -q 'pre_sign_state' <<<"$OUT"; then
  pass "an unrecognised pre_sign_state HARD-fails rather than rendering an empty claim"
else
  fail "arm 2n (unknown pre_sign_state): rc=$RC out=$OUT"
fi

# --- Arm 2o: a hand-edit INSIDE the status-unblocks markers ---------------
# The staleness half, on this specific region: somebody "just corrects" the
# publish-gate sentence in the board instead of editing the single writer.
setup_fixture presigned
perl -0777 -pi -e 's/\Q'"$B_UNBLOCKS"'\E/'"$B_UNBLOCKS"'HAND-EDITED /' \
  "$FIX/dev/plans/runs/board.md"
run_gate
if [ "$RC" -ne 0 ] && grep -q 'is STALE' <<<"$OUT" && grep -q 'status-unblocks' <<<"$OUT"; then
  pass "hand-editing inside the status-unblocks markers HARD-fails"
else
  fail "arm 2o (hand-edited unblocks cell): rc=$RC out=$OUT"
fi

# --- Arm 3: MISSING MARKER — a declared view that is not fenced ------------
# A view that silently stops being checked is worse than no view at all.
setup_fixture
perl -0777 -pi -e 's/\Q'"$B_HANDOFF"'\E\n//' "$FIX/dev/plans/runs/handoff.md"
run_gate
if [ "$RC" -ne 0 ] && grep -q 'EXACTLY ONE BEGIN and ONE END' <<<"$OUT"; then
  pass "missing BEGIN marker — a declared-but-unfenced view HARD-fails, never skips"
else
  fail "arm 3 (missing marker): rc=$RC out=$OUT"
fi

# --- Arm 3b: DUPLICATED marker pair ---------------------------------------
setup_fixture
{
  printf '\n%s\n' "$B_HANDOFF"
  printf 'a second, ambiguous copy%s\n' "$E_HANDOFF"
} >>"$FIX/dev/plans/runs/handoff.md"
run_gate
if [ "$RC" -ne 0 ] && grep -q 'EXACTLY ONE BEGIN and ONE END' <<<"$OUT"; then
  pass "duplicated markers — an ambiguous region HARD-fails"
else
  fail "arm 3b (duplicate markers): rc=$RC out=$OUT"
fi

# --- Arm 3c: markers in the WRONG ORDER (END before BEGIN) ----------------
setup_fixture
cat >"$FIX/dev/plans/runs/handoff.md" <<EOF
# Hand-off

${E_HANDOFF}
**The 9.9.9 ladder is between slices: 0 → 5 are all LANDED; 10 is next.**${B_HANDOFF}
EOF
run_gate
if [ "$RC" -ne 0 ] && grep -q 'END marker BEFORE its BEGIN marker' <<<"$OUT"; then
  pass "inverted markers — END before BEGIN HARD-fails"
else
  fail "arm 3c (inverted markers): rc=$RC out=$OUT"
fi

# --- Arm 4: ORPHAN MARKER — the confinement rule --------------------------
# This is what mechanically holds "generated regions are confined to the named
# locations, nothing else". A marker anywhere a state file has not declared is
# unowned and therefore unchecked, so it must fail.
setup_fixture
cat >"$FIX/dev/plans/stray.md" <<EOF
# Stray

${B_HANDOFF}
whatever${E_HANDOFF}
EOF
(cd "$FIX" && git add dev/plans/stray.md)
run_gate
if [ "$RC" -ne 0 ] && grep -q 'ORPHAN generated-region marker' <<<"$OUT"; then
  pass "orphan marker — a generated region outside every declared location HARD-fails"
else
  fail "arm 4 (orphan marker): rc=$RC out=$OUT"
fi

# --- Arm 5: UNKNOWN VIEW ID — declared but unrenderable -------------------
setup_fixture
perl -0777 -pi -e 's/"id": "handoff-next-step"/"id": "no-such-renderer"/' \
  "$FIX/dev/plans/release-state-9.9.9.json"
run_gate
if [ "$RC" -ne 0 ] && grep -q 'has no renderer' <<<"$OUT"; then
  pass "unknown view id — an unrenderable view HARD-fails, not an unchecked region"
else
  fail "arm 5 (unknown view id): rc=$RC out=$OUT"
fi

# --- Arm 5b: a view naming a file that does not exist ---------------------
setup_fixture
perl -0777 -pi -e 's{dev/plans/runs/handoff\.md}{dev/plans/runs/gone.md}' \
  "$FIX/dev/plans/release-state-9.9.9.json"
run_gate
if [ "$RC" -ne 0 ] && grep -q 'not a file in the worktree' <<<"$OUT"; then
  pass "a view naming a nonexistent file HARD-fails"
else
  fail "arm 5b (missing target file): rc=$RC out=$OUT"
fi

# --- Arm 5c: an unparseable state file ------------------------------------
setup_fixture
printf 'this is not json\n' >"$FIX/dev/plans/release-state-9.9.9.json"
run_gate
if [ "$RC" -ne 0 ] && grep -q 'not parseable as JSON' <<<"$OUT"; then
  pass "a corrupt state file HARD-fails (it cannot silently stop owning its views)"
else
  fail "arm 5c (corrupt state file): rc=$RC out=$OUT"
fi

# --- Arm 6: TC-37 vacuous-pass guard — ZERO state files -------------------
setup_fixture
git -C "$FIX" rm -q dev/plans/release-state-9.9.9.json
rm -f "$FIX/dev/plans/master.md" "$FIX/dev/plans/runs/handoff.md" "$FIX/dev/plans/runs/board.md"
run_gate
if [ "$RC" -ne 0 ] && grep -q 'ZERO release-state files' <<<"$OUT"; then
  pass "vacuity guard — zero state files discovered -> hard FAIL, not a silent exit 0"
else
  fail "arm 6 (zero state files): rc=$RC out=$OUT"
fi

# --- Arm 6b: TC-37 vacuous-pass guard — ZERO generated blocks -------------
# The named stop condition: a gate that finds no blocks must hard-fail. With an
# empty `generated_views` the regenerate-and-diff loop never runs at all, so an
# exit 0 would be a gate vouching for nothing.
setup_fixture
rm -f "$FIX/dev/plans/master.md" "$FIX/dev/plans/runs/handoff.md" "$FIX/dev/plans/runs/board.md"
perl -0777 -pi -e 's/"generated_views": \[.*\]/"generated_views": []/s' \
  "$FIX/dev/plans/release-state-9.9.9.json"
run_gate
if [ "$RC" -ne 0 ] && grep -qE 'ZERO generated blocks|`generated_views` is EMPTY' <<<"$OUT"; then
  pass "vacuity guard — zero generated blocks checked -> hard FAIL (TC-37)"
else
  fail "arm 6b (zero blocks): rc=$RC out=$OUT"
fi

# --- Arm 7: REVERSIBILITY — removing the markers restores the documents ----
# Bounding condition 4 of the HITL pre-sign, asserted mechanically rather than
# by inspection: strip every marker comment and the fixture documents are
# byte-identical to their unfenced originals.
setup_fixture
mkdir -p "$TMPROOT/unfenced"
for f in dev/plans/master.md dev/plans/runs/handoff.md dev/plans/runs/board.md; do
  mkdir -p "$TMPROOT/unfenced/$(dirname "$f")"
  perl -0777 -pe 's/<!-- (?:BEGIN|END) GENERATED release-state:[^>]*-->\n?//g' \
    "$FIX/$f" >"$TMPROOT/unfenced/$f"
done
if grep -q 'Lead-in prose' "$TMPROOT/unfenced/dev/plans/master.md" \
   && grep -q 'Trailing prose' "$TMPROOT/unfenced/dev/plans/master.md" \
   && grep -q 'must never be touched' "$TMPROOT/unfenced/dev/plans/master.md" \
   && ! grep -q 'GENERATED release-state' "$TMPROOT/unfenced/dev/plans/master.md" \
   && grep -q 'Slices 0 (`aaaa1111`)' "$TMPROOT/unfenced/dev/plans/master.md" \
   && grep -q 'More prose' "$TMPROOT/unfenced/dev/plans/runs/handoff.md" \
   && grep -q 'IS EXACTLY TWO:' "$TMPROOT/unfenced/dev/plans/runs/board.md" \
   && grep -q 'retained as the decision record' "$TMPROOT/unfenced/dev/plans/runs/board.md" \
   && ! grep -q 'GENERATED release-state' "$TMPROOT/unfenced/dev/plans/runs/board.md"; then
  pass "reversibility — stripping the markers leaves the prose AND the fenced content intact"
else
  fail "arm 7 (reversibility): stripped documents lost content"
fi

# --- Arm 8: the REAL repo is green (the regression half) ------------------
set +e
REAL_OUT="$("$REPO_ROOT/scripts/check-release-state-views.sh" 2>&1)"
REAL_RC=$?
set -e
if [ "$REAL_RC" -eq 0 ]; then
  pass "the real checkout's generated regions all reproduce from their state file"
else
  fail "arm 8 (real repo green): rc=$REAL_RC out=$REAL_OUT"
fi

# --- Arm 8b: the REAL board must not claim a signature already given -------
# The concrete defect, asserted against the shipped document rather than a
# fixture: 0.8.20's `status-unblocks` cell said publish was blocked on AC-079
# "which is still unsigned" AFTER the HITL pre-signed that delta (2026-07-25,
# master F-34). Restating a settled call as open is how it gets re-decided.
REAL_STATE="$REPO_ROOT/dev/plans/release-state-0.8.20.json"
REAL_BOARD="$REPO_ROOT/dev/plans/runs/STATUS-0.8.20.md"
RB='<!-- BEGIN GENERATED release-state:0.8.20:status-unblocks -->'
RE_='<!-- END GENERATED release-state:0.8.20:status-unblocks -->'
REAL_CELL="$(perl -0777 -ne 'print $1 if /\Q'"$RB"'\E(.*?)\Q'"$RE_"'\E/s' "$REAL_BOARD")"
REAL_PRESIGN="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["acceptance"]["publish_gate"].get("pre_sign_state",""))' "$REAL_STATE")"

REAL_MINTED="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["acceptance"]["publish_gate"].get("minted",False))' "$REAL_STATE")"
REAL_NEXT="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("next_slice"))' "$REAL_STATE")"

if [ -n "$REAL_CELL" ]; then
  pass "the real board carries a status-unblocks region to assert against"
elif [ "$REAL_NEXT" = "None" ]; then
  pass "end-of-ladder state retires the status-unblocks view instead of rendering an empty unblocks claim"
else
  fail "arm 8b: no status-unblocks region found in $REAL_BOARD"
fi

if [ "$REAL_PRESIGN" = "PRE_SIGNED" ]; then
  pass "the real state file records the publish gate as PRE_SIGNED (master F-34)"
  if ! grep -q 'still unsigned' <<<"$REAL_BOARD" \
     && ! grep -qE 'Publish remains blocked on AC-[0-9]+' <<<"$REAL_BOARD"; then
    pass "the real board does NOT claim publish awaits an already-given AC signature"
  else
    fail "arm 8b: the board restates a PRE-SIGNED gate as unsigned/blocking"
  fi
  if [ "$REAL_NEXT" = "None" ] && [ "$REAL_MINTED" = "True" ] \
     && grep -q 'AC-079 is MINTED and SIGNED' "$REAL_BOARD" \
     && grep -q 'separate explicit HITL.*PUBLISH.*gate' "$REAL_BOARD"; then
    pass "end-of-ladder board records the mint and the separate explicit publish gate"
  elif grep -q 'is PRE-SIGNED' <<<"$REAL_CELL" \
     && grep -q 'Pre-signing is NOT minting' <<<"$REAL_CELL" \
     && grep -qE 'minted and recorded as [A-Z]+ at Slice [0-9]+' <<<"$REAL_CELL" \
     && grep -q 'Publish is gated by the separate HITL publish gate' <<<"$REAL_CELL"; then
    pass "the real board conveys pre-signed + mints-at-sign-off-slice + the separate gate"
  else
    fail "arm 8b: the board omits one of pre-sign / minting / the separate gate: $REAL_CELL"
  fi
else
  fail "arm 8b: the real state file's pre_sign_state is '$REAL_PRESIGN', not PRE_SIGNED"
fi


# ===========================================================================
# TC-89, arms 10a-10f: the PLAN's "IMMEDIATE NEXT" pointer is a GENERATED VIEW.
# ===========================================================================
# THE MEASURED FAILURE. `dev/plans/plan-0.8.20.md` §9 carried a HAND-WRITTEN
# "IMMEDIATE NEXT: Slice N" pointer while `release-state-0.8.20.json` already
# owned `next_slice` and was already kept true at every landing. It went stale at
# THREE CONSECUTIVE COMMISSIONS. Twice measured in a single session: at the Slice
# 21 commission it still said "Slice 30" although Slice 30 had landed at
# 9b3ed0e3 (fixed at 62486a01, but only because the Steward happened to read the
# anchor); hours later, immediately after Slice 21 landed, it said "Slice 21".
#
# WHY IT IS WORSE THAN ORDINARY DOC ROT. `scripts/commission-manifest.sh`
# resolves `## 9. Immediate next slice` as the `{{MANDATE}}` anchor for an
# orchestrator brief, and its CHECK 2 verifies that the HEADING EXISTS, not that
# the prose under it is current. A heading cannot rot; the prose under it does.
# So a generated, path-verified brief handed an orchestrator a hand-written
# pointer at the wrong slice, with the manifest's authority behind it, and the
# staleness was copied into the next commission.
#
# THE REMEDY IS THE ONE DOC-HYGIENE-2 ALREADY BUILT, not a second mechanism: a
# `generated_views` entry + a renderer here + marker-delimited region, so drift
# is mechanically impossible rather than something each Steward must remember at
# land time. The interim mitigation that was in place — a warning blockquote
# telling the reader to re-verify against `next_slice` — is a NOTE, not a
# control, and notes are what this repo has ruled against.
#
# The expectations below are written LONGHAND, exactly as GATE_SENTENCE is, so a
# renderer change that alters the claim cannot also silently alter the
# expectation.

B_PLANNEXT='<!-- BEGIN GENERATED release-state:9.9.9:plan-immediate-next -->'
E_PLANNEXT='<!-- END GENERATED release-state:9.9.9:plan-immediate-next -->'

# Adds the plan document, its fenced region and the view declaration to the
# baseline fixture. Kept OUT of setup_fixture on purpose: arms 6/6b delete the
# fixture's view targets to exercise the vacuity guards, and a fifth target they
# do not know about would make those arms fail for the wrong reason.
plannext_fixture() {
  python3 - "$FIX/dev/plans/release-state-9.9.9.json" <<'PY'
import json, sys
p = sys.argv[1]
s = json.load(open(p))
titles = {0: "the design gate", 5: "the keystone", 10: "the widget leg",
          20: "the second leg", 30: "can-i-deploy contract gate", 40: "publish"}
for e in s["ladder"]:
    e["title"] = titles[e["slice"]]
s["generated_views"].append({"id": "plan-immediate-next",
                             "file": "dev/plans/plan-9.9.9.md"})
json.dump(s, open(p, "w"), indent=2)
PY
  cat >"$FIX/dev/plans/plan-9.9.9.md" <<EOF
# 9.9.9 — Plan

## 9. Immediate next slice

${B_PLANNEXT}
**IMMEDIATE NEXT: Slice 10** (\`R-B\`) — the widget leg

**Remaining ladder:** 10 → 20 → 30 → 40.${E_PLANNEXT}

Hand-written prose below the region, which must never be touched.
EOF
}

# --- Arm 10a: the plan's pointer renders from `next_slice` -----------------
# RED before the renderer existed: `plan-immediate-next` had no entry in
# RENDERERS, so the gate exited 1 with "has no renderer ... an unrenderable view
# is a failure, not an unchecked region" — which is the correct behaviour of the
# OLD gate and the proof this arm was measuring something absent.
setup_fixture
plannext_fixture
run_gate
if [ "$RC" -eq 0 ]; then
  pass "plan-immediate-next — the plan's IMMEDIATE NEXT pointer renders from the state file (TC-89)"
else
  fail "arm 10a (plan-immediate-next baseline): rc=$RC out=$OUT"
fi

# --- Arm 10b: the pointer goes STALE the moment `next_slice` moves ---------
# The whole point. This is the exact shape of the measured incident: a slice
# lands, the single writer is updated, and the plan's prose is not.
setup_fixture
plannext_fixture
perl -0777 -pi -e 's/"next_slice": 10/"next_slice": 20/' \
  "$FIX/dev/plans/release-state-9.9.9.json"
run_gate
if [ "$RC" -ne 0 ] && grep -q 'is STALE' <<<"$OUT" && grep -q 'plan-immediate-next' <<<"$OUT" \
   && grep -q 'Slice 20' <<<"$OUT"; then
  pass "plan-immediate-next — a landing that moves \`next_slice\` turns the plan's pointer RED"
else
  fail "arm 10b (next_slice drift): rc=$RC out=$OUT"
fi

# --- Arm 10c: --write repairs it, and the repair names the NEW slice -------
run_gate --write
run_gate
PLAN_DOC="$(cat "$FIX/dev/plans/plan-9.9.9.md")"
if [ "$RC" -eq 0 ] \
   && grep -q 'IMMEDIATE NEXT: Slice 20' <<<"$PLAN_DOC" \
   && ! grep -q 'IMMEDIATE NEXT: Slice 10' <<<"$PLAN_DOC" \
   && grep -q 'the second leg' <<<"$PLAN_DOC" \
   && grep -q 'Hand-written prose below the region' <<<"$PLAN_DOC"; then
  pass "plan-immediate-next — --write repoints the pointer and leaves the surrounding prose intact"
else
  fail "arm 10c (write repairs the pointer): rc=$RC doc=$PLAN_DOC"
fi

# --- Arm 10d: a HAND-EDIT inside the markers is caught --------------------
# The other direction of the same seam, and the one a well-meaning Steward
# actually produces: somebody "just fixes" the slice number in the prose.
setup_fixture
plannext_fixture
perl -0777 -pi -e 's/IMMEDIATE NEXT: Slice 10/IMMEDIATE NEXT: Slice 99/' \
  "$FIX/dev/plans/plan-9.9.9.md"
run_gate
if [ "$RC" -ne 0 ] && grep -q 'is STALE' <<<"$OUT" && grep -q 'plan-immediate-next' <<<"$OUT"; then
  pass "plan-immediate-next — hand-editing the slice number inside the markers HARD-fails"
else
  fail "arm 10d (hand-edited pointer): rc=$RC out=$OUT"
fi

# --- Arm 10e: END OF LADDER — the renderer REFUSES, it does not blank ------
# `next_slice: null` is a real end-of-release state. A renderer that emitted
# "Slice None" would put a fabricated pointer in front of the next orchestrator
# with the gate's authority behind it — the precise failure class this view
# exists to close, wearing a different hat. It must fail with the REASON.
setup_fixture
plannext_fixture
perl -0777 -pi -e 's/"next_slice": 10/"next_slice": null/' \
  "$FIX/dev/plans/release-state-9.9.9.json"
run_gate
# `plan-immediate-next` must be NAMED in the failure: `handoff-next-step` also
# reads `next_slice` and also refuses to render it, so an assertion that merely
# checked "something failed" would be satisfied by the neighbouring view and
# would still pass with this renderer absent entirely.
if [ "$RC" -ne 0 ] && grep -q 'plan-immediate-next' <<<"$OUT" \
   && grep -qi 'next_slice' <<<"$OUT" && ! grep -q 'Slice None' <<<"$OUT"; then
  pass "plan-immediate-next — an end-of-ladder \`next_slice: null\` fails with the reason, never renders a blank pointer"
else
  fail "arm 10e (null next_slice): rc=$RC out=$OUT"
fi

# --- Arm 10f: the REAL plan's pointer is generated and CURRENT -------------
# TC-89 asserted against the shipped documents, DERIVED from the state file so it
# cannot become the time-bombed literal TC-81 named. It reads `next_slice` out of
# the live state file and requires the live plan's fenced region to name that
# slice — and it requires the region to EXIST, so deleting the fence to dodge the
# check is itself a failure.
REAL_PLAN_STATE="$REPO_ROOT/dev/plans/release-state-0.8.20.json"
REAL_PLAN_PATH="$(python3 -c '
import json, sys
s = json.load(open(sys.argv[1]))
for v in s.get("generated_views") or []:
    if v.get("id") == "plan-immediate-next":
        print(v.get("file", "")); break
' "$REAL_PLAN_STATE")"
REAL_NEXT="$(python3 -c '
import json, sys
print(json.load(open(sys.argv[1])).get("next_slice"))' "$REAL_PLAN_STATE")"
if [ "$REAL_NEXT" = "None" ]; then
  # This arm USED to read `! rg -q <marker> <plan>`. `rg` is not in this repo's
  # guaranteed tool baseline (§2.4B: three CI runs lost to `rg: command not
  # found`, and there is a dedicated `test-ts-cache-coverage-no-rg` suite), and
  # an absent rg exits 127, which `!` inverts to TRUE — so the arm reported
  # `pass` WITHOUT EVER OPENING THE FILE. That is a TC-37 vacuous pass, and it
  # is the worse direction of the rg hazard: the 2026-08-01 incident failed
  # loudly and wrongly, this one passed silently and wrongly.
  #
  # Fixed three ways: (a) the tool is `grep`, which is POSIX and used
  # throughout this file already; (b) the file's existence is asserted, since
  # `grep` on a missing file also exits non-zero and would invert the same way;
  # (c) the rc is READ EXPLICITLY, so "no match" (1) is distinguished from
  # "the tool could not run" (>=2 / 127), and only the former can pass.
  REAL_PLAN_MD="$REPO_ROOT/dev/plans/plan-0.8.20.md"
  if [ ! -f "$REAL_PLAN_MD" ]; then
    fail "arm 10f: $REAL_PLAN_MD is missing, so the retired-pointer assertion cannot be evaluated (a check that cannot run must not pass — TC-37)"
  else
    set +e
    grep -qF 'BEGIN GENERATED release-state:0.8.20:plan-immediate-next' "$REAL_PLAN_MD"
    MARKER_RC=$?
    set -e
    if [ "$MARKER_RC" -gt 1 ]; then
      fail "arm 10f: grep could not read $REAL_PLAN_MD (rc=$MARKER_RC) — the marker check did not run, so it does not pass"
    elif [ -z "$REAL_PLAN_PATH" ] && [ "$MARKER_RC" -eq 1 ]; then
      pass "real repo — end-of-ladder state retires the generated next-slice pointer instead of fabricating one"
    else
      fail "arm 10f: end-of-ladder state must retire plan-immediate-next and its markers; path=$REAL_PLAN_PATH marker_rc=$MARKER_RC"
    fi
  fi
elif [ -z "$REAL_PLAN_PATH" ]; then
  fail "arm 10f: a live next_slice requires a declared plan-immediate-next view — TC-89's pointer is hand-written"
elif [ ! -f "$REPO_ROOT/$REAL_PLAN_PATH" ]; then
  fail "arm 10f: the declared plan-immediate-next target \`$REAL_PLAN_PATH\` is not a file"
else
  RPB='<!-- BEGIN GENERATED release-state:0.8.20:plan-immediate-next -->'
  RPE='<!-- END GENERATED release-state:0.8.20:plan-immediate-next -->'
  # `perl` is the second undeclared tool dependency in this file (§3.1.3). It is
  # kept — extracting a multi-line region between two literal markers is what it
  # is for — but its absence is now named, not left to abort the suite mid-arm
  # with a bare 127. Absent tool => loud FAIL, never a pass and never a skip.
  if ! command -v perl >/dev/null 2>&1; then
    fail "arm 10f: perl is not on PATH, so the generated region could not be read — the check did not run, and a check that cannot run does not pass (TC-37)"
    REAL_PTR=""
  else
    REAL_PTR="$(perl -0777 -ne 'print $1 if /\Q'"$RPB"'\E(.*?)\Q'"$RPE"'\E/s' "$REPO_ROOT/$REAL_PLAN_PATH")"
  fi
  if [ -n "$REAL_PTR" ] \
     && [ "$REAL_NEXT" != "None" ] \
     && grep -qF "IMMEDIATE NEXT: Slice $REAL_NEXT" <<<"$REAL_PTR"; then
    pass "real repo — the plan's IMMEDIATE NEXT pointer is a generated region and names Slice $REAL_NEXT, as the state file says"
  else
    fail "arm 10f (live plan pointer): next=$REAL_NEXT region=[$REAL_PTR]"
  fi
fi

# --- Arm 10g: an empty ladder renders as NONE, never as an empty claim -----
# The final landing is a real state: `remaining_ladder: []`. The master and
# landed roll-up remain generated, so their rendered prose must say "none";
# "remaining ladder = ." is grammatically broken and can be misread as a
# missing fact rather than an explicit complete ladder.
setup_fixture
python3 - "$FIX/dev/plans/release-state-9.9.9.json" <<'PY'
import json, sys
p = sys.argv[1]
s = json.load(open(p))
s["remaining_ladder"] = []
json.dump(s, open(p, "w"), indent=2)
PY
run_gate
if [ "$RC" -ne 0 ] && grep -qF 'remaining ladder = none.' <<<"$OUT" \
   && ! grep -qF 'remaining ladder = .' <<<"$OUT"; then
  pass "end-of-ladder master view renders an explicit remaining ladder = none"
else
  fail "arm 10g (empty remaining ladder): rc=$RC out=$OUT"
fi

# ===========================================================================
# Arms 11a-11h — SLICE-ID-HARDENING (0.8.20 cross-cutting unit, brief §1b).
#
# WHY A DEDICATED FIXTURE AND NOT THE REAL STATE FILE. `release-state-0.8.20.json`
# carries NO fractional slice id anywhere: `landed` is all ints, `next_slice` is
# the int 40, every `ladder[].slice` is an int. Against that state every defect
# below is INERT — `"%d" % 40` == `str(40)` == `_slice_str(40)` == `"40"` — so an
# arm pointed at the real checkout would be green before the fix and green after
# it. Vacuous by construction. The whole point of these arms is a state file that
# carries fractional AND integral-float ids, which is what `frac_fixture` builds.
#
# WHAT THE FIXTURE PLANTS, and which call site each value reaches:
#   landed              [0, 5.0, 10.5]   -> :278 handoff chain (bare str)
#   remaining_ladder    [20, 30.5, 40.0] -> :168 master ladder (bare str)
#   unblocked           [20, 30.0]       -> :154 _and_join (bare str), via :264
#   publish_precondition_slice  30.5     -> :260 by[int(...)] (INDEX truncation)
#   ladder 30.5 .slice  30.5             -> :263 `Slice %d` (RENDER truncation)
#   ladder 30.5 .depends_on [5,10.0,20]  -> :265 str(d) (bare str)
#   sign_off_slice      40.5             -> :233/:242/:248 `Slice %d` (truncation)
#
# THE FENCED REGIONS BELOW HOLD THE POST-FIX BYTES, HAND-WRITTEN. They are NOT
# produced by running the gate with --write: a fixture written by the tool under
# test and then checked by that same tool agrees with itself whatever the tool
# does, which is the canary-hashing-nothing failure this release already had
# once. Writing the expectation out longhand is what makes the diff evidence.
# ===========================================================================

# The publish-gate sentence with a FRACTIONAL sign_off_slice, longhand — an
# independent restatement of what the renderer must emit, exactly as gate_facts
# does for the integral case.
FRAC_GATE_SENTENCE='**AC-999 is PRE-SIGNED** — the HITL signed off on the accumulated governed-surface delta on 2026-01-02 (master F-99), pinned to the content of `src/conformance/governed-surface-allowlist.json`; any diff to that file re-opens it (the pin). Pre-signing is NOT minting: AC-999 is minted and recorded as SIGNED at Slice 40.5 (§4 #1). **Publish is gated by the separate HITL publish gate, not by this AC.**'
FRAC_GATE_SENTENCE_NOTPRE='**Publish remains blocked on AC-999**, which is **NOT pre-signed** — the accumulated governed-surface delta still awaits HITL sign-off, and AC-999 is minted and recorded as SIGNED at Slice 40.5 (§4 #1). Publish is additionally gated by the separate HITL publish gate.'
FRAC_GATE_SENTENCE_MINTED='**AC-999 is MINTED and recorded as SIGNED** at Slice 40.5 (§4 #1), covering the accumulated governed-surface delta. **Publish is gated by the separate HITL publish gate, not by this AC.**'

# Rewrites the baseline fixture into the fractional one. Call AFTER
# setup_fixture. $1 selects the publish-gate branch (presigned|notpresigned|
# minted) so all THREE `Slice %d` sites (:233, :242, :248) are reachable — a fix
# applied to one branch only would leave the other two live.
frac_fixture() {
  local mode="${1:-presigned}" sentence
  case "$mode" in
    presigned)    sentence="$FRAC_GATE_SENTENCE" ;;
    notpresigned) sentence="$FRAC_GATE_SENTENCE_NOTPRE" ;;
    minted)       sentence="$FRAC_GATE_SENTENCE_MINTED" ;;
    *) printf 'frac_fixture: unknown gate mode %q\n' "$mode" >&2; exit 2 ;;
  esac

  python3 - "$FIX/dev/plans/release-state-9.9.9.json" "$mode" <<'PY'
import json, sys
p, mode = sys.argv[1], sys.argv[2]
s = json.load(open(p))
# Two NEW ladder entries whose ids are fractional, sitting alongside their
# integer neighbours 30 and 40 — which must survive untouched. 10.5 is LANDED so
# the landed roll-up has to carry it.
s["ladder"].append({"slice": 10.5, "short": "R-B5", "depends_on": [5],
                    "status": "LANDED", "sha": "cccc3333"})
s["ladder"].append({"slice": 30.5, "short": "H7b", "depends_on": [5, 10.0, 20],
                    "status": "NOT_STARTED", "sha": None})
s["ladder"].append({"slice": 40.5, "short": "PUB2", "depends_on": [30.5],
                    "status": "NOT_STARTED", "sha": None})
# The INTEGER neighbour Slice 30 gets the same integral-float dependency list.
# Without this, :265's defect is MASKED by :260's: the unfixed gate indexes
# by[int(30.5)] == by[30] and renders Slice 30's all-integer depends_on, so the
# bare-str() bug at :265 never shows and its arm would be vacuously green
# pre-fix. Planted in BOTH entries, :265 is red whichever entry is selected —
# which is what makes it independent evidence rather than a shadow of :260.
for _e in s["ladder"]:
    if _e["slice"] == 30:
        _e["depends_on"] = [5, 10.0, 20]
s["landed"]                     = [0, 5.0, 10.5]
s["remaining_ladder"]           = [20, 30.5, 40.0]
s["unblocked"]                  = [20, 30.0]
s["publish_precondition_slice"] = 30.5
s["next_slice"]                 = 20
g = s["acceptance"]["publish_gate"]
g["sign_off_slice"] = 40.5
if mode == "notpresigned":
    g["pre_sign_state"] = "NOT_PRE_SIGNED"
    g.pop("pre_sign", None)
    g["minted"] = False
elif mode == "minted":
    g["minted"] = True
else:
    g["minted"] = False
json.dump(s, open(p, "w"), indent=2)
PY

  # master §4 — remaining_ladder must render `40`, never `40.0` (:168), and the
  # landed roll-up must render `5`, never `5.0` (:167, already helper-correct).
  cat >"$FIX/dev/plans/master.md" <<EOF
# Master

Prose that is NOT generated and must never be touched.

| Release | Notes |
|---|---|
| **9.9.9** | Lead-in prose. **✅ LADDER PROGRESS: ${B_MASTER}Slices 0 (\`aaaa1111\`) · 5 (\`bbbb2222\`) · 10.5 (\`cccc3333\`) are all LANDED on \`origin/main\`; SCHEMA is 42; remaining ladder = 20 → 30.5 → 40.${E_MASTER}** Trailing prose. |
EOF

  # board §1 — `20 and 30` (:154), the H7b entry selected by 30.5 (:260),
  # `Slice 30.5` rendered (:263), `5/10/20` (:265), `Slice 40.5` (gate).
  cat >"$FIX/dev/plans/runs/board.md" <<EOF
# Board

## 1. Current state

| | |
|---|---|
| **Unblocks** | ${B_UNBLOCKS}**Slices 20 and 30 are NOW UNBLOCKED** — R-A (the thing) now exists. Slice 30.5 (H7b) depends on 5/10/20. ${sentence}${E_UNBLOCKS} |

## 4. Open HITL decisions

> **⚠ HISTORICAL QUEUE, NOT THE LIVE OPEN SET.** Rows 1-3 are retained as the
> decision record; do not act on them as open.
>
> **THE LIVE OPEN SET IS EXACTLY ${B_OPEN}TWO${E_OPEN}:** (1) the batched
> surface decision; and (2) PUBLISH (hard gate).

| # | Decision | Recommendation |
|---|---|---|
| 1 | A settled thing | retained as the decision record |
EOF

  # hand-off — the landed chain must render `5`, never `5.0` (:278).
  cat >"$FIX/dev/plans/runs/handoff.md" <<EOF
# Hand-off

## Next step

${B_HANDOFF}
**The 9.9.9 ladder is between slices: 0 → 5 → 10.5 are all LANDED; 20 is next.**${E_HANDOFF} More prose.
EOF
}

# The RENDER, read straight out of the gate's own diff diagnostic.
#
# WHY NOT JUST ASSERT rc=0. A green gate prints one `ok` line and nothing else,
# so a green run carries NO evidence about what was rendered — an assertion of
# the form "the output does not contain 40.0" is satisfied trivially by a gate
# that prints nothing at all. That is the same shape as this release's canary
# that hashed nothing on both sides and compared equal.
#
# So each arm below deliberately PERTURBS one byte inside the region it cares
# about. The gate then reports that region STALE and prints, verbatim, the bytes
# it rendered from the state file. Those bytes are the evidence, and they are
# available identically before and after the fix — which is what lets one arm
# assert the WRONG string pre-fix and the RIGHT string post-fix, positively, in
# both directions.
#
# NON-VACUITY: an empty render, or a run that did not fail, fails the arm.
#
# The render is extracted FOR A NAMED VIEW ID, never as "the first render in the
# output". Against the UNFIXED gate several regions are stale at once, so a
# positional grep would hand every arm the master-ladder render and each arm
# would then be asserting about a view it does not own — a verification-apparatus
# defect of exactly the class that consumed five of six codex fix rounds across
# Slices 32/33.
FRAC_RENDER=""
render_of() {     # $1 = view id; reads $OUT
  awk -v vid="$1" '
    index($0, "generated region `" vid "` is STALE") { in_block = 1 }
    in_block && /RENDERED FROM THE STATE FILE/       { want = 1; next }
    in_block && want                                 { print; exit }
  ' <<<"$OUT"
}
frac_render() {   # $1 = doc under $FIX, $2 = perl perturbation, $3 = view id, $4 = gate mode
  setup_fixture
  frac_fixture "${4:-presigned}"
  perl -0777 -pi -e "$2" "$FIX/$1"
  run_gate
  FRAC_RENDER="$(render_of "$3")"
}

# --- Arm 11a: the fractional fixture is GREEN end to end -------------------
# The aggregate. Every arm after this one names ONE call site; this one is what
# goes red against the unfixed gate for all of them at once, and it is the arm
# that proves the hand-written expectations are jointly satisfiable.
setup_fixture
frac_fixture presigned
run_gate
if [ "$RC" -eq 0 ]; then
  pass "fractional fixture — every fenced region matches its render when the state file carries fractional and integral-float slice ids"
else
  fail "arm 11a (fractional fixture green): rc=$RC out=$OUT"
fi

# --- Arm 11b: :168 — remaining_ladder must not render an integral float ----
# `str(40.0)` -> "40.0" where the SIBLING renderer on the same field (:369) uses
# `_slice_str(40.0)` -> "40": the same fact rendered two ways in two documents.
frac_render "dev/plans/master.md" 's/SCHEMA is 42/SCHEMA is 43/' master-ladder-progress
if [ "$RC" -ne 0 ] && [ -n "$FRAC_RENDER" ] \
   && grep -qF 'remaining ladder = 20 → 30.5 → 40.' <<<"$FRAC_RENDER" \
   && ! grep -qF '40.0' <<<"$FRAC_RENDER"; then
  pass ":168 — the master ladder renders remaining_ladder through _slice_str (40, never 40.0)"
else
  fail "arm 11b (:168 divergent render): rc=$RC rendered=[$FRAC_RENDER]"
fi

# --- Arm 11c: :278 — the hand-off landed chain, same defect ----------------
frac_render "dev/plans/runs/handoff.md" 's/is next\./is nextZZ./' handoff-next-step
if [ "$RC" -ne 0 ] && [ -n "$FRAC_RENDER" ] \
   && grep -qF 'between slices: 0 → 5 → 10.5 are all LANDED' <<<"$FRAC_RENDER" \
   && ! grep -qF '5.0' <<<"$FRAC_RENDER"; then
  pass ":278 — the hand-off landed chain renders through _slice_str (5, never 5.0)"
else
  fail "arm 11c (:278 divergent render): rc=$RC rendered=[$FRAC_RENDER]"
fi

# The board render carries FOUR of the sites at once (:154, :260, :263, :265)
# plus the publish-gate `Slice %d`. One perturbation, four assertions.
frac_render "dev/plans/runs/board.md" 's/R-A \(the thing\)/R-A (the thingZZ)/' status-unblocks
BOARD_RENDER="$FRAC_RENDER"
BOARD_RC="$RC"

# --- Arm 11d: :260 — by[int(...)] selects the INTEGER NEIGHBOUR's entry ----
# `by[int(30.5)]` is `by[30]` — the Slice 30 entry, short `H7` — not the Slice
# 30.5 entry, short `H7b`. The helper's own docstring names exactly this: "never
# int(), which would collapse a fractional slice onto its integer neighbour and
# SILENTLY OVERWRITE it". `H7b` in the render is what proves the INDEX was right.
if [ "$BOARD_RC" -ne 0 ] && [ -n "$BOARD_RENDER" ] \
   && grep -qF '(H7b) depends on' <<<"$BOARD_RENDER" \
   && ! grep -qF '(H7) depends on' <<<"$BOARD_RENDER"; then
  pass ":260 — publish_precondition_slice 30.5 indexes the Slice 30.5 ladder entry (H7b), not Slice 30's (H7)"
else
  fail "arm 11d (:260 index truncation): rc=$BOARD_RC rendered=[$BOARD_RENDER]"
fi

# --- Arm 11e: :263 — the RENDER of pre["slice"], a SECOND defect -----------
# The brief is explicit that :263's `%d` is fed by `pre["slice"]` (:265), NOT by
# :260's `int()`. This assertion is on the printed slice id, independent of which
# entry was selected; arm 11e2 proves the independence by execution.
if [ "$BOARD_RC" -ne 0 ] && [ -n "$BOARD_RENDER" ] \
   && grep -qF 'Slice 30.5 (H7b)' <<<"$BOARD_RENDER"; then
  pass ":263 — pre[\"slice\"] renders as Slice 30.5, not truncated to Slice 30"
else
  fail "arm 11e (:263 render truncation): rc=$BOARD_RC rendered=[$BOARD_RENDER]"
fi

# --- Arm 11e2: :263 and :260 are INDEPENDENT — proven with a mutant --------
# "Two distinct defects at one site, not one" is a claim, and this repo's rule is
# that a claim in a gate is graded by execution. The mutant reverts ONLY the
# render while leaving the index fixed; if the two were one defect the mutant
# would be indistinguishable from the fixed gate. It renders `Slice 30 (H7b)` —
# the right entry under the wrong number.
#
# NON-VACUITY: if the revert does not apply, the arm FAILS rather than grading
# nothing.
MUT_GATE="$TMPROOT/mutant-render-only.sh"
cp "$GATE" "$MUT_GATE"
perl -0777 -pi -e 's/_slice_str\(pre\["slice"\]\)/int(pre["slice"])/;
                   s/Slice \%s \(\%s\) depends on/Slice %d (%s) depends on/' "$MUT_GATE" || true
if grep -qF 'int(pre["slice"])' "$MUT_GATE" && grep -qF 'Slice %d (%s) depends on' "$MUT_GATE"; then
  chmod +x "$MUT_GATE"
  setup_fixture
  frac_fixture presigned
  perl -0777 -pi -e 's/R-A \(the thing\)/R-A (the thingZZ)/' "$FIX/dev/plans/runs/board.md"
  cp "$MUT_GATE" "$FIX/scripts/check-release-state-views.sh"
  run_gate
  MUT_RENDER="$(render_of status-unblocks)"
  if [ "$RC" -ne 0 ] && grep -qF 'Slice 30 (H7b)' <<<"$MUT_RENDER"; then
    pass ":263 is INDEPENDENT of :260 — a gate with the index fixed but the render reverted emits 'Slice 30 (H7b)', the right entry under the wrong number"
  else
    fail "arm 11e2 (:263 mutant): rc=$RC rendered=[$MUT_RENDER]"
  fi
else
  fail "arm 11e2 (:263 mutant): the render-only revert did not apply to the gate — the arm graded nothing"
fi

# --- Arm 11f: :265 — depends_on renders through the helper -----------------
# NOT in the brief's measured six. Found by the Duty-3 sweep: `pre["depends_on"]`
# is a list of SLICE IDS joined with bare `str(d)`.
if [ "$BOARD_RC" -ne 0 ] && [ -n "$BOARD_RENDER" ] \
   && grep -qF 'depends on 5/10/20.' <<<"$BOARD_RENDER" \
   && ! grep -qF '10.0/' <<<"$BOARD_RENDER"; then
  pass ":265 — depends_on renders through _slice_str (5/10/20, never 5/10.0/20)"
else
  fail "arm 11f (:265 divergent render): rc=$BOARD_RC rendered=[$BOARD_RENDER]"
fi

# --- Arm 11g: :154 — _and_join is a SLICE-ID renderer ----------------------
# NOT in the brief's measured six either. `_and_join` is only ever called on
# `st["unblocked"]` (:264), a list of slice ids, and its own docstring examples
# are slice numbers — yet it rendered them with bare `str(i)`.
if [ "$BOARD_RC" -ne 0 ] && [ -n "$BOARD_RENDER" ] \
   && grep -qF '**Slices 20 and 30 are NOW UNBLOCKED**' <<<"$BOARD_RENDER" \
   && ! grep -qF '30.0' <<<"$BOARD_RENDER"; then
  pass ":154 — _and_join renders the unblocked slice ids through _slice_str (20 and 30, never 20 and 30.0)"
else
  fail "arm 11g (:154 divergent render): rc=$BOARD_RC rendered=[$BOARD_RENDER]"
fi

# --- Arm 11h: sign_off_slice — ALL THREE publish-gate branches -------------
# :233 (minted), :242 (pre-signed) and :248 (not pre-signed) each carry their OWN
# `Slice %d` fed by `int(gate["sign_off_slice"])`. A fix applied to whichever
# branch the baseline fixture happens to exercise would leave the other two live,
# so each is driven through its own state — and each is asserted on the rendered
# bytes, not merely on the exit code.
for frac_mode in presigned notpresigned minted; do
  frac_render "dev/plans/runs/board.md" 's/R-A \(the thing\)/R-A (the thingZZ)/' status-unblocks "$frac_mode"
  if [ "$RC" -ne 0 ] && [ -n "$FRAC_RENDER" ] \
     && grep -qF 'at Slice 40.5 (§4 #1)' <<<"$FRAC_RENDER" \
     && ! grep -qF 'at Slice 40 (' <<<"$FRAC_RENDER"; then
    pass "sign_off_slice 40.5 renders as Slice 40.5 in the $frac_mode publish-gate branch"
  else
    fail "arm 11h ($frac_mode branch): rc=$RC rendered=[$FRAC_RENDER]"
  fi
  # …and the whole fixture is green once the perturbation is gone.
  setup_fixture
  frac_fixture "$frac_mode"
  run_gate
  if [ "$RC" -eq 0 ]; then
    pass "the fractional fixture is green end to end in the $frac_mode publish-gate branch"
  else
    fail "arm 11h ($frac_mode green): rc=$RC out=$OUT"
  fi
done

# --- Arm 11i: _by_slice must REFUSE a duplicate normalised key -------------
# The helper's docstring says a collapsed fractional id would "SILENTLY OVERWRITE"
# its neighbour — but the helper itself had no duplicate guard, so two ladder
# entries that normalise to the SAME key (30 and 30.0, or 30 and "30") lost one
# of them without a word. Found by the Duty-3 sweep. Refusing is the only safe
# answer: which entry wins is dict-insertion order, i.e. state-file line order.
setup_fixture
python3 - "$FIX/dev/plans/release-state-9.9.9.json" <<'PY'
import json, sys
p = sys.argv[1]
s = json.load(open(p))
s["ladder"].append({"slice": 30.0, "short": "IMPOSTOR", "depends_on": [],
                    "status": "NOT_STARTED", "sha": None})
json.dump(s, open(p, "w"), indent=2)
PY
run_gate
# The message must NAME the collapsed id and the hazard, not merely fail: the
# operator has to be told WHICH two entries collided, because the survivor is
# decided by state-file line order and is otherwise invisible.
if [ "$RC" -ne 0 ] \
   && grep -qF 'resolve to the same slice id 30' <<<"$OUT" \
   && grep -qF 'SILENTLY OVERWRITE' <<<"$OUT"; then
  pass "_by_slice — two ladder entries collapsing onto one key HARD-fail, naming the id, instead of silently overwriting"
else
  fail "arm 11i (duplicate ladder key): rc=$RC out=$OUT"
fi

# --- Arm 11j: the ALL-INTEGER render is byte-for-byte UNCHANGED ------------
# The output-neutrality claim, asserted rather than assumed. Every fix above is a
# no-op on a state file whose ids are all integers — which is what
# release-state-0.8.20.json is today — so the baseline fixture's regions, written
# before any of this, must still match byte for byte. Without this arm a "fix"
# that changed integer rendering would sail through every arm above and churn
# the real boards.
setup_fixture
run_gate
if [ "$RC" -eq 0 ]; then
  pass "output-neutrality — the all-integer baseline fixture still renders byte-identically after the fractional-id fixes"
else
  fail "arm 11j (all-int neutrality): rc=$RC out=$OUT"
fi

# --- Arm 9: the CI job is ALWAYS-ON, and reuses the shared script ----------
# Same reasoning as board-currency / ledger-integrity / plan-anchors: the push
# that breaks a generated view is a LANDING push, which the docs_only fast path
# excludes by construction. A currency gate that cannot run on the push that
# invalidates the fact is decorative.
CI_JOB_BLOCK="$(awk '
  /^  release-state-views:/ { inblock = 1; print; next }
  inblock && /^  [A-Za-z0-9_-]+:/ { inblock = 0 }
  inblock { print }
' "$CI_YML")"

if [ -n "$CI_JOB_BLOCK" ]; then
  pass "ci.yml defines a release-state-views job"
else
  fail "ci.yml has no release-state-views job"
fi
if printf '%s' "$CI_JOB_BLOCK" | grep -q 'scripts/check-release-state-views.sh'; then
  pass "the CI job runs the SHARED scripts/check-release-state-views.sh"
else
  fail "the CI job must invoke scripts/check-release-state-views.sh, not a reimplementation"
fi
if printf '%s' "$CI_JOB_BLOCK" | grep -qE '^\s*(if|needs):'; then
  fail "the release-state-views job must be ALWAYS-ON (no if:/needs: gate); block: $CI_JOB_BLOCK"
else
  pass "the release-state-views job is always-on (no if:, no needs:, not docs_only-gated)"
fi

# --- Arm R (remote-landing guard) ------------------------------------------
# THE DEFECT THIS PINS. Both `render_master_ladder_progress` and
# `render_plan_landed_roll_up` emit the literal words "LANDED on `origin/main`",
# but every fact they render comes from the state file's `landed` array. Nothing
# ever asked Git whether those SHAs were reachable from the remote, so a slice
# counted as REMOTELY landed at local-commit time. On 2026-08-03 local `main`
# sat 21 commits ahead of `origin/main` with three of 0.8.21's five slices
# unpushed while the plan asserted all five were on `origin/main` in full, and
# this gate passed. Without these arms the defect returns the next time a
# steward commits without pushing.
#
# The baseline fixture has no remote at all, which is why arm 0 stays green:
# with no `origin/main` to consult the claim is unverifiable, not false.
remote_fixture() {
  setup_fixture
  (
    cd "$FIX"
    git branch -M main
    git init -q --bare "$TMPROOT/remote.git"
    git remote add origin "$TMPROOT/remote.git"
    git push -q origin main

    # Slice 0 -> the pushed baseline commit (reachable from origin/main).
    # Slice 5 -> a commit made AFTER the push (local only). This is the drift.
    pushed="$(git rev-parse --short HEAD)"
    echo "local-only change" >local-only.txt
    git add local-only.txt && git commit -qm 'local: not pushed'
    unpushed="$(git rev-parse --short HEAD)"

    python3 - "$pushed" "$unpushed" <<'PY'
import json, sys
p = "dev/plans/release-state-9.9.9.json"
st = json.load(open(p))
for e in st["ladder"]:
    if e["slice"] == 0:  e["sha"] = sys.argv[1]
    if e["slice"] == 5:  e["sha"] = sys.argv[2]
json.dump(st, open(p, "w"), indent=2)
PY
    # Regenerate the views so the ONLY fault under test is the push state,
    # never a stale render.
    ./scripts/check-release-state-views.sh --write >/dev/null 2>&1
    git add -A && git commit -qm 'fixture: real SHAs, one unpushed'
  )
}

remote_fixture
run_gate
if [ "$RC" -ne 0 ] && printf '%s' "$OUT" | grep -q 'not reachable from origin/main'; then
  pass "arm R: a landed SHA absent from origin/main HARD-fails on \`main\`"
else
  fail "arm R (unpushed landed SHA must fail on main): rc=$RC out=$OUT"
fi

if printf '%s' "$OUT" | grep -q 'slice 5'; then
  pass "arm R: the failure NAMES the offending slice, not just a count"
else
  fail "arm R (must name slice 5): out=$OUT"
fi

if printf '%s' "$OUT" | grep -q 'slice 0'; then
  fail "arm R: slice 0 IS on origin/main and must not be reported: out=$OUT"
else
  pass "arm R: a slice that IS on origin/main is not falsely reported"
fi

# --- Arm R2: pushing the commit turns the claim true -----------------------
(cd "$FIX" && git push -q origin main)
run_gate
if [ "$RC" -eq 0 ] && ! printf '%s' "$OUT" | grep -q 'not reachable'; then
  pass "arm R2: once pushed, the origin/main claim verifies and the gate is green"
else
  fail "arm R2 (green after push): rc=$RC out=$OUT"
fi

# --- Arm R3: ADVISORY on a branch, HARD only on `main` ---------------------
# A PR branch legitimately carries a landed SHA the remote lacks — that is what
# the PR is for. Hard-failing there would block the merge that resolves it.
(
  cd "$FIX"
  git checkout -q -b feature/in-flight
  echo "branch change" >branch-only.txt
  git add branch-only.txt && git commit -qm 'branch: not pushed'
  branch_head="$(git rev-parse --short HEAD)"
  python3 - "$branch_head" <<'PY'
import json, sys
p = "dev/plans/release-state-9.9.9.json"
st = json.load(open(p))
for e in st["ladder"]:
    if e["slice"] == 5:  e["sha"] = sys.argv[1]
json.dump(st, open(p, "w"), indent=2)
PY
  ./scripts/check-release-state-views.sh --write >/dev/null 2>&1
  git add -A && git commit -qm 'fixture: in-flight slice on a branch'
)
run_gate
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q 'Advisory on a non-`main` branch'; then
  pass "arm R3: the same condition is ADVISORY on a non-main branch (exit 0)"
else
  fail "arm R3 (advisory off main): rc=$RC out=$OUT"
fi

# --- Arm R4: the guard is not vacuous -------------------------------------
# The first implementation of this check keyed its enablement off a state-file
# key that does not exist (`slices` rather than `ladder`), so it ran on nothing
# and every arm above would have passed against a no-op. Prove the code path
# actually executes by asserting it reports on a repo that HAS a remote.
if printf '%s' "$OUT" | grep -q '9.9.9 claims'; then
  pass "arm R4: the guard actually executes and names the release (not a no-op)"
else
  fail "arm R4 (guard must not be vacuous): out=$OUT"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll check-release-state-views tests passed\n'
