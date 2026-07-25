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
rm -f "$FIX/dev/plans/release-state-9.9.9.json"
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

if [ -n "$REAL_CELL" ]; then
  pass "the real board carries a status-unblocks region to assert against"
else
  fail "arm 8b: no status-unblocks region found in $REAL_BOARD"
fi

if [ "$REAL_PRESIGN" = "PRE_SIGNED" ]; then
  pass "the real state file records the publish gate as PRE_SIGNED (master F-34)"
  if ! grep -q 'still unsigned' <<<"$REAL_CELL" \
     && ! grep -qE 'Publish remains blocked on AC-[0-9]+' <<<"$REAL_CELL"; then
    pass "the real board does NOT claim publish awaits an already-given AC signature"
  else
    fail "arm 8b: the board restates a PRE-SIGNED gate as unsigned/blocking: $REAL_CELL"
  fi
  if grep -q 'is PRE-SIGNED' <<<"$REAL_CELL" \
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

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll check-release-state-views tests passed\n'
