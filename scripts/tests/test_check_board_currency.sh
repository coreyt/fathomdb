#!/usr/bin/env bash
# scripts/tests/test_check_board_currency.sh — coverage for the shared board/git
# ancestry drift predicate (status-board-currency-enforcement design, items 2+3)
# AND for its wiring into `preflight.sh --landing`.
#
# The incident this closes: after a slice's merge commit reached `origin/main`,
# `dev/plans/runs/STATUS-0.8.20.md` still narrated it "not landed" for four days
# — nobody's explicit job to update the board at land time. Items 1-3 of
# dev/design/status-board-currency-enforcement.md close the mechanism (not the
# actor): a shared predicate script (`scripts/check-board-currency.sh`), invoked
# both by `preflight.sh --landing` (PREVENT, this file's arms 3-5) and by CI on
# `main` (DETECT, item 3 — same script, not duplicated).
#
# Predicate under test (see scripts/check-board-currency.sh header for the full
# statement): a live (non-CLOSED-banner) STATUS-0.8.z.md board must contain the
# short SHA of the most recent `merge(0.8.z): Slice N ...` commit reachable from
# the tip being checked, for every slice N such a commit exists for. Missing =
# STALE = HARD fail.
#
# Isolation: every arm runs against a throwaway repo built under mktemp -d. The
# test never git-writes into the real checkout and does not depend on the
# developer's tree being clean. Mirrors test_preflight_landing.sh's fixture
# hygiene (neutralized global git config: gpgsign / core.hooksPath /
# init.templateDir all defanged in the fixture's LOCAL config only).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PREFLIGHT="$REPO_ROOT/scripts/preflight.sh"
CHECKER="$REPO_ROOT/scripts/check-board-currency.sh"
# shellcheck source=lib/governed-surface-fixture.sh
. "$SCRIPT_DIR/lib/governed-surface-fixture.sh"
# shellcheck source=lib/c1-conformance-fixture.sh
. "$SCRIPT_DIR/lib/c1-conformance-fixture.sh"

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

NO_HOOKS="$TMPROOT/no-hooks"
mkdir -p "$NO_HOOKS"

# init_repo <dir> — bare repo with a neutralized local config (see header).
init_repo() {
  local dir="$1"
  mkdir -p "$dir"
  git init -q -b main "$dir"
  git -C "$dir" config user.email board-currency-test@example.invalid
  git -C "$dir" config user.name 'Board Currency Test'
  git -C "$dir" config commit.gpgsign false
  git -C "$dir" config core.hooksPath "$NO_HOOKS"
  # A minimal, CONSISTENT ledger + sidecar (committed by the first commit_all,
  # which runs `git add -A`). The preflight arms below invoke `--landing`, which
  # now also runs the ledger-integrity gate (DOC-HYGIENE-2 T1b); its TC-37
  # vacuous-pass guard HARD-fails a tree in which it discovers zero ledgers, so
  # these fixtures must model a real checkout and carry one. Nothing here is
  # ledger-specific beyond its presence — the ledger arms live in
  # scripts/tests/test_check_ledgers.sh.
  mkdir -p "$dir/dev/steward"
  printf '{"seq":1,"note":"fixture"}\n' >"$dir/dev/steward/steward-ledger.jsonl"
  printf '%s' 1 >"$dir/dev/steward/steward-ledger.jsonl.seq"
  # Same story one gate later: `--landing` also runs the governed-surface pin
  # gate (DOC-HYGIENE-2 T1e), which HARD-fails a tree whose pin it cannot read —
  # correctly, on the same TC-37 grounds. So the fixture carries a minimal,
  # self-consistent surface+pin pair as well. Seeded by the shared helper, which
  # verifies the pair against the real gate; nothing here is surface-specific
  # beyond its presence, and the gate's own arms live in
  # scripts/tests/test_check_governed_surface_pin.sh.
  seed_governed_surface_fixture "$dir"
  seed_c1_conformance_fixture "$dir"
}

# commit_all <dir> <message>
commit_all() {
  local dir="$1" msg="$2"
  git -C "$dir" add -A
  git -C "$dir" commit -q -m "$msg" >/dev/null
}

# --- Fixture A: a "current" board — mentions the landing merge's SHA -----------
CURRENT_REPO="$TMPROOT/current"
init_repo "$CURRENT_REPO"
mkdir -p "$CURRENT_REPO/dev/plans/runs" "$CURRENT_REPO/src" "$CURRENT_REPO/scripts"
printf 'fixture\n' >"$CURRENT_REPO/src/keep.txt"
printf '# STATUS — 0.8.99 fixture\n\nSlice 1: not started.\n' >"$CURRENT_REPO/dev/plans/runs/STATUS-0.8.99.md"
commit_all "$CURRENT_REPO" 'fixture: initial commit'

# Simulate the land: a feature branch merges, producing a merge commit with the
# repo's own landing-merge subject convention.
git -C "$CURRENT_REPO" checkout -q -b slice-1-fixture
printf 'work\n' >"$CURRENT_REPO/src/slice1.txt"
commit_all "$CURRENT_REPO" 'feat: slice 1 work'
git -C "$CURRENT_REPO" checkout -q main
git -C "$CURRENT_REPO" merge -q --no-ff -m 'merge(0.8.99): Slice 1 — fixture land' slice-1-fixture
LANDING_SHA="$(git -C "$CURRENT_REPO" rev-parse HEAD)"
SHORT_SHA="${LANDING_SHA:0:8}"

# Board is updated IN THE SAME COMMIT as the merge (the contract this gate
# enforces) — stamp LANDED@<sha> and commit.
printf '# STATUS — 0.8.99 fixture\n\nSlice 1: **LANDED %s**.\n' "$SHORT_SHA" \
  >"$CURRENT_REPO/dev/plans/runs/STATUS-0.8.99.md"
commit_all "$CURRENT_REPO" "docs: stamp STATUS-0.8.99 Slice 1 LANDED $SHORT_SHA"

# Canonical-state fixture: the recorded SHA is the evidence, even though this
# historical landing subject predates the `merge(<release>): Slice N` spelling
# and the concise board intentionally has no ladder table.
CANONICAL_REPO="$TMPROOT/canonical-state"
init_repo "$CANONICAL_REPO"
mkdir -p "$CANONICAL_REPO/dev/plans/runs" "$CANONICAL_REPO/src"
printf '# concise 0.8.99 board\n' >"$CANONICAL_REPO/dev/plans/runs/STATUS-0.8.99.md"
printf 'base\n' >"$CANONICAL_REPO/src/base.txt"
commit_all "$CANONICAL_REPO" 'fixture: base'
printf 'landed\n' >"$CANONICAL_REPO/src/landed.txt"
commit_all "$CANONICAL_REPO" 'historical landing subject without slice convention'
CANONICAL_SHA="$(git -C "$CANONICAL_REPO" rev-parse --short=8 HEAD)"
cat >"$CANONICAL_REPO/dev/plans/release-state-0.8.99.json" <<EOF
{"release":"0.8.99","board":"dev/plans/runs/STATUS-0.8.99.md",
 "landed":[5],"ladder":[{"slice":5,"status":"LANDED","sha":"$CANONICAL_SHA"}],
 "remaining_ladder":[]}
EOF
commit_all "$CANONICAL_REPO" 'fixture: record canonical landing state'

# --- Fixture B: a STALE board — same land, board never touched -----------------
# Built by replaying fixture A's history up to (and including) the merge, but
# WITHOUT the board-stamping commit — reproduces the exact incident shape: the
# merge commit is an ancestor of the tip, and the board still says "not started".
STALE_REPO="$TMPROOT/stale"
init_repo "$STALE_REPO"
mkdir -p "$STALE_REPO/dev/plans/runs" "$STALE_REPO/src" "$STALE_REPO/scripts"
printf 'fixture\n' >"$STALE_REPO/src/keep.txt"
printf '# STATUS — 0.8.99 fixture\n\nSlice 1: not started.\n' >"$STALE_REPO/dev/plans/runs/STATUS-0.8.99.md"
commit_all "$STALE_REPO" 'fixture: initial commit'
git -C "$STALE_REPO" checkout -q -b slice-1-fixture
printf 'work\n' >"$STALE_REPO/src/slice1.txt"
commit_all "$STALE_REPO" 'feat: slice 1 work'
git -C "$STALE_REPO" checkout -q main
git -C "$STALE_REPO" merge -q --no-ff -m 'merge(0.8.99): Slice 1 — fixture land' slice-1-fixture
STALE_LANDING_SHA="$(git -C "$STALE_REPO" rev-parse HEAD)"
STALE_SHORT_SHA="${STALE_LANDING_SHA:0:8}"
# Board deliberately left untouched — reproduces the 4-day incident.

# --- Fixture C: a CLOSED-banner board — must never be flagged (frozen/skipped) -
CLOSED_REPO="$TMPROOT/closed"
init_repo "$CLOSED_REPO"
mkdir -p "$CLOSED_REPO/dev/plans/runs" "$CLOSED_REPO/src" "$CLOSED_REPO/scripts"
printf 'fixture\n' >"$CLOSED_REPO/src/keep.txt"
printf '# STATUS — 0.8.99 fixture\n\n> CLOSED — historical record, archived in place.\n\nSlice 1: not started (historical).\n' \
  >"$CLOSED_REPO/dev/plans/runs/STATUS-0.8.99.md"
commit_all "$CLOSED_REPO" 'fixture: initial commit'
git -C "$CLOSED_REPO" checkout -q -b slice-1-fixture
printf 'work\n' >"$CLOSED_REPO/src/slice1.txt"
commit_all "$CLOSED_REPO" 'feat: slice 1 work'
git -C "$CLOSED_REPO" checkout -q main
git -C "$CLOSED_REPO" merge -q --no-ff -m 'merge(0.8.99): Slice 1 — fixture land' slice-1-fixture

# --- Fixture C2: CLOSED-banner board with YAML frontmatter pushing the banner
# below line 5 — regression guard. Real repo shape: STATUS-0.8.9.1.md carries
# T3 `status:` frontmatter, which pushes its CLOSED banner to line 10; a naive
# `head -n 5` scan misses it and would wrongly treat a closed board as live.
CLOSED_FM_REPO="$TMPROOT/closed-frontmatter"
init_repo "$CLOSED_FM_REPO"
mkdir -p "$CLOSED_FM_REPO/dev/plans/runs" "$CLOSED_FM_REPO/src" "$CLOSED_FM_REPO/scripts"
printf 'fixture\n' >"$CLOSED_FM_REPO/src/keep.txt"
{
  printf -- '---\n'
  printf 'title: STATUS fixture\n'
  printf 'date: 2026-01-01\n'
  printf 'desc: fixture\n'
  printf 'status: complete\n'
  printf -- '---\n\n'
  printf '# 0.8.99 fixture — CLOSING STATUS\n\n'
  printf '> CLOSED — historical record, archived in place.\n\n'
  printf 'Slice 1: not started (historical).\n'
} >"$CLOSED_FM_REPO/dev/plans/runs/STATUS-0.8.99.md"
commit_all "$CLOSED_FM_REPO" 'fixture: initial commit'
git -C "$CLOSED_FM_REPO" checkout -q -b slice-1-fixture
printf 'work\n' >"$CLOSED_FM_REPO/src/slice1.txt"
commit_all "$CLOSED_FM_REPO" 'feat: slice 1 work'
git -C "$CLOSED_FM_REPO" checkout -q main
git -C "$CLOSED_FM_REPO" merge -q --no-ff -m 'merge(0.8.99): Slice 1 — fixture land' slice-1-fixture

# --- Fixture D: superseded-merge — only the NEWEST land per slice must be cited
SUPERSEDED_REPO="$TMPROOT/superseded"
init_repo "$SUPERSEDED_REPO"
mkdir -p "$SUPERSEDED_REPO/dev/plans/runs" "$SUPERSEDED_REPO/src" "$SUPERSEDED_REPO/scripts"
printf 'fixture\n' >"$SUPERSEDED_REPO/src/keep.txt"
printf '# STATUS — 0.8.99 fixture\n\nSlice 1: not started.\n' >"$SUPERSEDED_REPO/dev/plans/runs/STATUS-0.8.99.md"
commit_all "$SUPERSEDED_REPO" 'fixture: initial commit'
git -C "$SUPERSEDED_REPO" checkout -q -b slice-1-partial
printf 'partial\n' >"$SUPERSEDED_REPO/src/slice1-partial.txt"
commit_all "$SUPERSEDED_REPO" 'feat: slice 1 partial work'
git -C "$SUPERSEDED_REPO" checkout -q main
git -C "$SUPERSEDED_REPO" merge -q --no-ff -m 'merge(0.8.99): Slice 1 PARTIAL — fixture partial land' slice-1-partial
PARTIAL_SHA="$(git -C "$SUPERSEDED_REPO" rev-parse HEAD)"
PARTIAL_SHORT="${PARTIAL_SHA:0:8}"
printf '# STATUS — 0.8.99 fixture\n\nSlice 1: **PARTIAL %s**.\n' "$PARTIAL_SHORT" \
  >"$SUPERSEDED_REPO/dev/plans/runs/STATUS-0.8.99.md"
commit_all "$SUPERSEDED_REPO" "docs: stamp STATUS-0.8.99 Slice 1 PARTIAL $PARTIAL_SHORT"
git -C "$SUPERSEDED_REPO" checkout -q -b slice-1-final
printf 'final\n' >"$SUPERSEDED_REPO/src/slice1-final.txt"
commit_all "$SUPERSEDED_REPO" 'feat: slice 1 final work'
git -C "$SUPERSEDED_REPO" checkout -q main
git -C "$SUPERSEDED_REPO" merge -q --no-ff -m 'merge(0.8.99): Slice 1 — fixture final land' slice-1-final
FINAL_SHA="$(git -C "$SUPERSEDED_REPO" rev-parse HEAD)"
FINAL_SHORT="${FINAL_SHA:0:8}"
printf '# STATUS — 0.8.99 fixture\n\nSlice 1: **LANDED %s** (supersedes PARTIAL %s).\n' "$FINAL_SHORT" "$PARTIAL_SHORT" \
  >"$SUPERSEDED_REPO/dev/plans/runs/STATUS-0.8.99.md"
commit_all "$SUPERSEDED_REPO" "docs: stamp STATUS-0.8.99 Slice 1 LANDED $FINAL_SHORT"
# Note: the PARTIAL commit's own SHA is deliberately never mentioned in the
# final board text (only the LANDED sha is) — this must NOT be flagged stale.

# --- Fixture E (fix-1): vacuous-pass hole — a LIVE board whose release has ZERO
# commits matching the landing-merge convention anywhere in reachable history
# (a squash-land, a reworded merge, convention drift, or just nothing landed
# yet). Pre-fix, the per-slice loop body never runs for this board, STALE never
# flips, and the checker reports "ok" — vouching for a board it never actually
# checked. Deliberately uses a DIFFERENT release (0.8.97) so it cannot collide
# with any other fixture's merge-subject matches.
NOMATCH_REPO="$TMPROOT/nomatch"
init_repo "$NOMATCH_REPO"
mkdir -p "$NOMATCH_REPO/dev/plans/runs" "$NOMATCH_REPO/src" "$NOMATCH_REPO/scripts"
printf 'fixture\n' >"$NOMATCH_REPO/src/keep.txt"
printf '# STATUS — 0.8.97 fixture\n\nSlice 1: not started.\n' >"$NOMATCH_REPO/dev/plans/runs/STATUS-0.8.97.md"
commit_all "$NOMATCH_REPO" 'fixture: initial commit'
# Plain (non-merge, non-landing-convention) commits only -- no "merge(0.8.97):
# Slice N" subject anywhere in this repo's history.
printf 'more work\n' >"$NOMATCH_REPO/src/plain.txt"
commit_all "$NOMATCH_REPO" 'feat: ordinary commit, not a landing merge'
printf 'even more\n' >>"$NOMATCH_REPO/src/plain.txt"
commit_all "$NOMATCH_REPO" 'chore: another ordinary commit'

# --- Fixture F (SLICE-ID-HARDENING site 1): FRACTIONAL slice id, collision -----
# The integer-only capture `Slice[-[:space:]]([0-9]+)` reads `Slice 39.5` as
# `39`. Because `git log` is newest-first, the NEWER 39.5 merge is processed
# FIRST and sets SEEN_SLICE[39]; the older, genuinely-distinct `Slice 39` merge
# is then swallowed by the "superseded intermediate" branch and ITS SHA CHECK
# NEVER RUNS. The board below cites 39.5's SHA and deliberately NOT 39's, so a
# correct gate must flag Slice 39 as stale.
#
# ⚠ THIS CANNOT BE REPRODUCED AGAINST REAL REPO HISTORY. `release-state-0.8.20.json`
# carries no fractional id and no `merge(0.8.20): Slice N` subject exists for 39
# (Slice 39 fast-forwarded at 91db34d8). The harm is PROSPECTIVE, so the arm is
# built here, in a throwaway fixture. Fractional ids are IN scope for fixtures
# and OUT of scope for real state, the ladder and the board.
#
# ⚠ COMMIT ORDER IS LOAD-BEARING: the 39.5 merge MUST be the NEWER commit. Land
# them the other way round and SEEN_SLICE[39] is set by the real Slice 39 merge,
# the collision never fires, and the arm silently proves nothing.
#
# Uses release 0.8.96 so its merge subjects cannot collide with any other fixture.
FRACTIONAL_REPO="$TMPROOT/fractional"
init_repo "$FRACTIONAL_REPO"
mkdir -p "$FRACTIONAL_REPO/dev/plans/runs" "$FRACTIONAL_REPO/src" "$FRACTIONAL_REPO/scripts"
printf 'fixture\n' >"$FRACTIONAL_REPO/src/keep.txt"
printf '# STATUS — 0.8.96 fixture\n\nSlice 39: not started.\nSlice 39.5: not started.\n' \
  >"$FRACTIONAL_REPO/dev/plans/runs/STATUS-0.8.96.md"
commit_all "$FRACTIONAL_REPO" 'fixture: initial commit'
# OLDER land: the integer slice.
git -C "$FRACTIONAL_REPO" checkout -q -b slice-39-fixture
printf 'work 39\n' >"$FRACTIONAL_REPO/src/slice39.txt"
commit_all "$FRACTIONAL_REPO" 'feat: slice 39 work'
git -C "$FRACTIONAL_REPO" checkout -q main
git -C "$FRACTIONAL_REPO" merge -q --no-ff -m 'merge(0.8.96): Slice 39 — fixture integer land' slice-39-fixture
FRAC_39_SHA="$(git -C "$FRACTIONAL_REPO" rev-parse HEAD)"
FRAC_39_SHORT="${FRAC_39_SHA:0:8}"
# NEWER land: the fractional slice. Processed FIRST by the newest-first walk.
git -C "$FRACTIONAL_REPO" checkout -q -b slice-39-5-fixture
printf 'work 39.5\n' >"$FRACTIONAL_REPO/src/slice39_5.txt"
commit_all "$FRACTIONAL_REPO" 'feat: slice 39.5 work'
git -C "$FRACTIONAL_REPO" checkout -q main
git -C "$FRACTIONAL_REPO" merge -q --no-ff -m 'merge(0.8.96): Slice 39.5 — fixture fractional land' slice-39-5-fixture
FRAC_395_SHA="$(git -C "$FRACTIONAL_REPO" rev-parse HEAD)"
FRAC_395_SHORT="${FRAC_395_SHA:0:8}"
# Board cites ONLY the fractional land. Slice 39's SHA is deliberately absent —
# that is the contradiction the gate must catch and, pre-fix, silently did not.
printf '# STATUS — 0.8.96 fixture\n\nSlice 39: not started.\nSlice 39.5: **LANDED %s**.\n' "$FRAC_395_SHORT" \
  >"$FRACTIONAL_REPO/dev/plans/runs/STATUS-0.8.96.md"
commit_all "$FRACTIONAL_REPO" "docs: stamp STATUS-0.8.96 Slice 39.5 LANDED $FRAC_395_SHORT"

# --- Fixture G (SLICE-ID-HARDENING, the FABRICATED POINTER) --------------------
# Same truncation, different visible consequence: here the board DOES cite the
# integer Slice 39's land but NOT the fractional 39.5's, so the gate correctly
# fails either way — and the defect is in WHAT IT PRINTS. Pre-fix the STALE line
# reports 39.5's SHA under the label `Slice 39`: the gate emitting its own
# fabricated pointer, which is the incident class this unit was ruled over.
# Release 0.8.95, again to avoid cross-fixture subject collisions.
FABRICATED_REPO="$TMPROOT/fabricated"
init_repo "$FABRICATED_REPO"
mkdir -p "$FABRICATED_REPO/dev/plans/runs" "$FABRICATED_REPO/src" "$FABRICATED_REPO/scripts"
printf 'fixture\n' >"$FABRICATED_REPO/src/keep.txt"
printf '# STATUS — 0.8.95 fixture\n\nSlice 39: not started.\n' \
  >"$FABRICATED_REPO/dev/plans/runs/STATUS-0.8.95.md"
commit_all "$FABRICATED_REPO" 'fixture: initial commit'
git -C "$FABRICATED_REPO" checkout -q -b slice-39-fixture
printf 'work 39\n' >"$FABRICATED_REPO/src/slice39.txt"
commit_all "$FABRICATED_REPO" 'feat: slice 39 work'
git -C "$FABRICATED_REPO" checkout -q main
git -C "$FABRICATED_REPO" merge -q --no-ff -m 'merge(0.8.95): Slice 39 — fixture integer land' slice-39-fixture
FAB_39_SHA="$(git -C "$FABRICATED_REPO" rev-parse HEAD)"
FAB_39_SHORT="${FAB_39_SHA:0:8}"
printf '# STATUS — 0.8.95 fixture\n\nSlice 39: **LANDED %s**.\n' "$FAB_39_SHORT" \
  >"$FABRICATED_REPO/dev/plans/runs/STATUS-0.8.95.md"
commit_all "$FABRICATED_REPO" "docs: stamp STATUS-0.8.95 Slice 39 LANDED $FAB_39_SHORT"
git -C "$FABRICATED_REPO" checkout -q -b slice-39-5-fixture
printf 'work 39.5\n' >"$FABRICATED_REPO/src/slice39_5.txt"
commit_all "$FABRICATED_REPO" 'feat: slice 39.5 work'
git -C "$FABRICATED_REPO" checkout -q main
git -C "$FABRICATED_REPO" merge -q --no-ff -m 'merge(0.8.95): Slice 39.5 — fixture fractional land' slice-39-5-fixture
FAB_395_SHA="$(git -C "$FABRICATED_REPO" rev-parse HEAD)"
FAB_395_SHORT="${FAB_395_SHA:0:8}"
# Board deliberately NOT restamped for 39.5.

# --- Fixture H (SLICE-ID-HARDENING regression guard): fractional board CURRENT -
# Both lands cited. Must exit 0 before AND after the fix — this arm is a
# regression guard, NOT a recurrence arm, and is labelled as such below.
FRAC_OK_REPO="$TMPROOT/fractional-ok"
init_repo "$FRAC_OK_REPO"
mkdir -p "$FRAC_OK_REPO/dev/plans/runs" "$FRAC_OK_REPO/src" "$FRAC_OK_REPO/scripts"
printf 'fixture\n' >"$FRAC_OK_REPO/src/keep.txt"
printf '# STATUS — 0.8.94 fixture\n\nSlice 39: not started.\n' \
  >"$FRAC_OK_REPO/dev/plans/runs/STATUS-0.8.94.md"
commit_all "$FRAC_OK_REPO" 'fixture: initial commit'
git -C "$FRAC_OK_REPO" checkout -q -b slice-39-fixture
printf 'work 39\n' >"$FRAC_OK_REPO/src/slice39.txt"
commit_all "$FRAC_OK_REPO" 'feat: slice 39 work'
git -C "$FRAC_OK_REPO" checkout -q main
git -C "$FRAC_OK_REPO" merge -q --no-ff -m 'merge(0.8.94): Slice 39 — fixture integer land' slice-39-fixture
FOK_39_SHORT="$(git -C "$FRAC_OK_REPO" rev-parse HEAD)"; FOK_39_SHORT="${FOK_39_SHORT:0:8}"
git -C "$FRAC_OK_REPO" checkout -q -b slice-39-5-fixture
printf 'work 39.5\n' >"$FRAC_OK_REPO/src/slice39_5.txt"
commit_all "$FRAC_OK_REPO" 'feat: slice 39.5 work'
git -C "$FRAC_OK_REPO" checkout -q main
git -C "$FRAC_OK_REPO" merge -q --no-ff -m 'merge(0.8.94): Slice 39.5 — fixture fractional land' slice-39-5-fixture
FOK_395_SHORT="$(git -C "$FRAC_OK_REPO" rev-parse HEAD)"; FOK_395_SHORT="${FOK_395_SHORT:0:8}"
printf '# STATUS — 0.8.94 fixture\n\nSlice 39: **LANDED %s**.\nSlice 39.5: **LANDED %s**.\n' \
  "$FOK_39_SHORT" "$FOK_395_SHORT" >"$FRAC_OK_REPO/dev/plans/runs/STATUS-0.8.94.md"
commit_all "$FRAC_OK_REPO" 'docs: stamp STATUS-0.8.94 both lands'

# ============================================================================
# TC-133 fixtures — the board/state CROSS-READ (§3a of the SLICE-ID-HARDENING
# brief, added mid-flight by HITL ruling seq-212 option (b)).
#
# ⚠ THIS IS NOT SITE 1. At site 1 the slice-number CAPTURE is wrong. Here the
# capture is CORRECT, the landing merge IS found, and its SHA IS present in the
# board file — and the gate still certifies a board whose HAND-WRITTEN rows
# contradict `dev/plans/release-state-<ver>.json` on every one of them.
#
# The real incident: for a long period `dev/plans/runs/STATUS-0.8.20.md` said
# Slice 39 was NOT_STARTED while `91db34d8` WAS in the file — but only inside
# the `<!-- BEGIN GENERATED -->` cells. The generated half was carrying the
# check for the hand-written half, and the always-on CI job passed too.
# Presence of a SHA is not currency of the row it belongs to.
#
# Every fixture below reproduces that shape literally: a matched landing merge,
# its short SHA present in the board, and a hand-written ladder row that still
# says "not started". An arm that merely REMOVES the SHA proves nothing new —
# fixture B (Arm 2) already covers absent-SHA.
# ============================================================================

# write_state <dir> <ver> <landed-json-array> <ladder-json-array> <remaining-json-array>
# Writes a minimal, well-formed release-state file. Only the fields the
# cross-read actually reads are modelled; the real file carries ~20 more.
write_state() {
  local dir="$1" ver="$2" landed="$3" ladder="$4" remaining="$5"
  mkdir -p "$dir/dev/plans"
  printf '{\n  "release": "%s",\n  "landed": %s,\n  "ladder": %s,\n  "remaining_ladder": %s,\n  "next_slice": 40\n}\n' \
    "$ver" "$landed" "$ladder" "$remaining" >"$dir/dev/plans/release-state-$ver.json"
}

# A release may legitimately have no landing merge only at activation: its
# state starts with no landed slices, the next slice is the first ladder entry,
# and the entire ladder remains. The board still has to expose the structural
# ladder which the unconditional TC-133 cross-read validates below.
make_initial_release_fixture() {
  local dir="$1" ver="$2" state="$3"
  init_repo "$dir"
  mkdir -p "$dir/dev/plans/runs" "$dir/dev/plans" "$dir/src" "$dir/scripts"
  printf 'fixture\n' >"$dir/src/keep.txt"
  {
    printf '# STATUS — %s fixture\n\n' "$ver"
    printf '| Slice | Title | Depends-on | Status |\n'
    printf '|------:|-------|------------|--------|\n'
    printf '| 5 | first release slice | — | not started |\n'
    printf '| 40 | release gate | 5 | not started |\n\n'
    printf '**Ladder remaining: 5 → 40.**\n'
  } >"$dir/dev/plans/runs/STATUS-$ver.md"
  printf '%s\n' "$state" >"$dir/dev/plans/release-state-$ver.json"
  commit_all "$dir" 'fixture: newly activated release'
}

INITIAL_RELEASE_STATE='{
  "release": "0.8.81",
  "board": "dev/plans/runs/STATUS-0.8.81.md",
  "landed": [],
  "ladder": [{"slice": 5, "status": "NOT_STARTED"}, {"slice": 40, "status": "NOT_STARTED"}],
  "remaining_ladder": [5, 40],
  "next_slice": 5
}'
INITIAL_REPO="$TMPROOT/initial-release"
make_initial_release_fixture "$INITIAL_REPO" 0.8.81 "$INITIAL_RELEASE_STATE"

INITIAL_UNRECORDED_LAND_STATE='{
  "release": "0.8.77",
  "board": "dev/plans/runs/STATUS-0.8.77.md",
  "landed": [],
  "ladder": [{"slice": 5, "status": "LANDED", "sha": "deadbeef"}, {"slice": 40, "status": "NOT_STARTED"}],
  "remaining_ladder": [5, 40],
  "next_slice": 5
}'
INITIAL_UNRECORDED_LAND_REPO="$TMPROOT/initial-unrecorded-land"
make_initial_release_fixture "$INITIAL_UNRECORDED_LAND_REPO" 0.8.77 "$INITIAL_UNRECORDED_LAND_STATE"

INITIAL_UNRECORDED_SHA_STATE='{
  "release": "0.8.76",
  "board": "dev/plans/runs/STATUS-0.8.76.md",
  "landed": [],
  "ladder": [{"slice": 5, "status": "NOT_STARTED", "sha": "deadbeef"}, {"slice": 40, "status": "NOT_STARTED"}],
  "remaining_ladder": [5, 40],
  "next_slice": 5
}'
INITIAL_UNRECORDED_SHA_REPO="$TMPROOT/initial-unrecorded-sha"
make_initial_release_fixture "$INITIAL_UNRECORDED_SHA_REPO" 0.8.76 "$INITIAL_UNRECORDED_SHA_STATE"

INITIAL_NO_TABLE_REPO="$TMPROOT/initial-no-table"
make_initial_release_fixture "$INITIAL_NO_TABLE_REPO" 0.8.81 "$INITIAL_RELEASE_STATE"
printf '# STATUS — 0.8.81 fixture\n\nInitial release, no ladder table.\n' \
  >"$INITIAL_NO_TABLE_REPO/dev/plans/runs/STATUS-0.8.81.md"
commit_all "$INITIAL_NO_TABLE_REPO" 'fixture: remove initial release ladder table'

INITIAL_NONFIRST_STATE='{
  "release": "0.8.80",
  "board": "dev/plans/runs/STATUS-0.8.80.md",
  "landed": [],
  "ladder": [{"slice": 5, "status": "NOT_STARTED"}, {"slice": 40, "status": "NOT_STARTED"}],
  "remaining_ladder": [5, 40],
  "next_slice": 40
}'
INITIAL_NONFIRST_REPO="$TMPROOT/initial-nonfirst"
make_initial_release_fixture "$INITIAL_NONFIRST_REPO" 0.8.80 "$INITIAL_NONFIRST_STATE"

INITIAL_LANDED_NO_SHA_STATE='{
  "release": "0.8.79",
  "board": "dev/plans/runs/STATUS-0.8.79.md",
  "landed": [5],
  "ladder": [{"slice": 5, "status": "LANDED"}, {"slice": 40, "status": "NOT_STARTED"}],
  "remaining_ladder": [40],
  "next_slice": 40
}'
INITIAL_LANDED_NO_SHA_REPO="$TMPROOT/initial-landed-no-sha"
make_initial_release_fixture "$INITIAL_LANDED_NO_SHA_REPO" 0.8.79 "$INITIAL_LANDED_NO_SHA_STATE"

INITIAL_MALFORMED_REPO="$TMPROOT/initial-malformed"
make_initial_release_fixture "$INITIAL_MALFORMED_REPO" 0.8.78 '{not valid JSON'

# --- Fixture J (TC-133 RECURRENCE): the board CONTRADICTS its own state file --
# Release 0.8.93. Slice 5 landed via a recognized `merge(0.8.93): Slice 5`
# subject; the state file records it LANDED with that short SHA; the short SHA
# IS in the board — inside a GENERATED cell — and the hand-written ladder row
# still reads `not started`.
#
# ⚠ RED-FIRST PROOF: against the UNFIXED checker this fixture exits **0**. The
# only board predicate is "does the short SHA appear anywhere in the file", and
# it does. That green is the defect.
CONTRADICT_REPO="$TMPROOT/contradicting"
init_repo "$CONTRADICT_REPO"
mkdir -p "$CONTRADICT_REPO/dev/plans/runs" "$CONTRADICT_REPO/src" "$CONTRADICT_REPO/scripts"
printf 'fixture\n' >"$CONTRADICT_REPO/src/keep.txt"
printf '# STATUS — 0.8.93 fixture\n\n## 2. Slice ladder\n\n| Slice | Title | Depends-on | Status |\n|------:|-------|-----------|--------|\n| 5 | fixture slice | — | not started |\n| 40 | fixture tail | 5 | not started |\n\n**Ladder remaining: 40 alone.**\n' \
  >"$CONTRADICT_REPO/dev/plans/runs/STATUS-0.8.93.md"
write_state "$CONTRADICT_REPO" 0.8.93 '[]' \
  '[{"slice": 5, "status": "NOT_STARTED"}, {"slice": 40, "status": "NOT_STARTED"}]' '[5, 40]'
commit_all "$CONTRADICT_REPO" 'fixture: initial commit'
git -C "$CONTRADICT_REPO" checkout -q -b slice-5-fixture
printf 'work\n' >"$CONTRADICT_REPO/src/slice5.txt"
commit_all "$CONTRADICT_REPO" 'feat: slice 5 work'
git -C "$CONTRADICT_REPO" checkout -q main
git -C "$CONTRADICT_REPO" merge -q --no-ff -m 'merge(0.8.93): Slice 5 — fixture land' slice-5-fixture
CONTRA_SHA="$(git -C "$CONTRADICT_REPO" rev-parse HEAD)"
CONTRA_SHORT="${CONTRA_SHA:0:8}"
# The state file is reconciled (Slice 5 LANDED). The board is NOT: its
# hand-written ladder row is untouched, and the SHA reaches the file only
# through the generated cell — the exact split that made the incident invisible.
{
  printf '# STATUS — 0.8.93 fixture\n\n'
  printf '## 2. Slice ladder\n\n'
  printf '| Slice | Title | Depends-on | Status |\n'
  printf '|------:|-------|-----------|--------|\n'
  printf '| 5 | fixture slice | — | not started |\n'
  printf '| 40 | fixture tail | 5 | not started |\n\n'
  printf '**Ladder remaining: 40 alone.**\n\n'
  printf -- '<!-- BEGIN GENERATED: fixture-cell -->\n'
  printf 'Landed: 5 (`%s`).\n' "$CONTRA_SHORT"
  printf -- '<!-- END GENERATED: fixture-cell -->\n'
} >"$CONTRADICT_REPO/dev/plans/runs/STATUS-0.8.93.md"
write_state "$CONTRADICT_REPO" 0.8.93 '[5]' \
  "$(printf '[{"slice": 5, "status": "LANDED", "sha": "%s"}, {"slice": 40, "status": "NOT_STARTED"}]' "$CONTRA_SHORT")" \
  '[40]'
commit_all "$CONTRADICT_REPO" "docs: reconcile release-state-0.8.93 to the Slice 5 land $CONTRA_SHORT"

# --- Fixture K (TC-133 RECURRENCE): `Ladder remaining:` prose disagrees -------
# Release 0.8.92. Every hand-written ladder row is CURRENT (Slice 5 cites its
# own landing SHA, no stale marker) — the ONLY contradiction is the prose claim
# `Ladder remaining: 40 and 41 alone` against `remaining_ladder: [40]`.
# Separated from fixture J on purpose so each half of the cross-read owns its
# own red; a single fixture tripping both would let either half rot unnoticed.
# ⚠ RED-FIRST PROOF: unfixed checker exits 0 (the SHA is cited).
LADDERPROSE_REPO="$TMPROOT/ladder-prose"
init_repo "$LADDERPROSE_REPO"
mkdir -p "$LADDERPROSE_REPO/dev/plans/runs" "$LADDERPROSE_REPO/src" "$LADDERPROSE_REPO/scripts"
printf 'fixture\n' >"$LADDERPROSE_REPO/src/keep.txt"
printf '# STATUS — 0.8.92 fixture\n\nSlice 5: not started.\n' \
  >"$LADDERPROSE_REPO/dev/plans/runs/STATUS-0.8.92.md"
write_state "$LADDERPROSE_REPO" 0.8.92 '[]' '[{"slice": 5, "status": "NOT_STARTED"}]' '[5, 40]'
commit_all "$LADDERPROSE_REPO" 'fixture: initial commit'
git -C "$LADDERPROSE_REPO" checkout -q -b slice-5-fixture
printf 'work\n' >"$LADDERPROSE_REPO/src/slice5.txt"
commit_all "$LADDERPROSE_REPO" 'feat: slice 5 work'
git -C "$LADDERPROSE_REPO" checkout -q main
git -C "$LADDERPROSE_REPO" merge -q --no-ff -m 'merge(0.8.92): Slice 5 — fixture land' slice-5-fixture
LP_SHORT="$(git -C "$LADDERPROSE_REPO" rev-parse HEAD)"; LP_SHORT="${LP_SHORT:0:8}"
{
  printf '# STATUS — 0.8.92 fixture\n\n'
  printf '| Slice | Title | Depends-on | Status |\n'
  printf '|------:|-------|-----------|--------|\n'
  printf '| 5 | fixture slice | — | **COMPLETE — LANDED `%s`** |\n' "$LP_SHORT"
  printf '| 40 | fixture tail | 5 | not started |\n\n'
  printf '**Ladder remaining: 40 and 41 alone**, then the HITL publish gate.\n'
} >"$LADDERPROSE_REPO/dev/plans/runs/STATUS-0.8.92.md"
write_state "$LADDERPROSE_REPO" 0.8.92 '[5]' \
  "$(printf '[{"slice": 5, "status": "LANDED", "sha": "%s"}, {"slice": 40, "status": "NOT_STARTED"}]' "$LP_SHORT")" \
  '[40]'
commit_all "$LADDERPROSE_REPO" "docs: stamp STATUS-0.8.92 Slice 5 LANDED $LP_SHORT"

# --- Fixture M (TC-133 policy): the state file EXISTS but is UNPARSEABLE ------
# Release 0.8.91. Board is fully current. The state file is corrupt. The gate
# must NOT quietly fall back to the SHA-only predicate: the release declares a
# single-writer state file and the cross-read cannot vouch for the board
# without it. Same failure family as the fix-1 vacuous-pass guard.
# ⚠ RED-FIRST PROOF: unfixed checker exits 0 (it never opens the state file).
BADSTATE_REPO="$TMPROOT/bad-state"
init_repo "$BADSTATE_REPO"
mkdir -p "$BADSTATE_REPO/dev/plans/runs" "$BADSTATE_REPO/dev/plans" "$BADSTATE_REPO/src" "$BADSTATE_REPO/scripts"
printf 'fixture\n' >"$BADSTATE_REPO/src/keep.txt"
printf '# STATUS — 0.8.91 fixture\n\nSlice 5: not started.\n' \
  >"$BADSTATE_REPO/dev/plans/runs/STATUS-0.8.91.md"
commit_all "$BADSTATE_REPO" 'fixture: initial commit'
git -C "$BADSTATE_REPO" checkout -q -b slice-5-fixture
printf 'work\n' >"$BADSTATE_REPO/src/slice5.txt"
commit_all "$BADSTATE_REPO" 'feat: slice 5 work'
git -C "$BADSTATE_REPO" checkout -q main
git -C "$BADSTATE_REPO" merge -q --no-ff -m 'merge(0.8.91): Slice 5 — fixture land' slice-5-fixture
BS_SHORT="$(git -C "$BADSTATE_REPO" rev-parse HEAD)"; BS_SHORT="${BS_SHORT:0:8}"
{
  printf '# STATUS — 0.8.91 fixture\n\n'
  printf '| Slice | Title | Depends-on | Status |\n'
  printf '|------:|-------|-----------|--------|\n'
  printf '| 5 | fixture slice | — | **COMPLETE — LANDED `%s`** |\n' "$BS_SHORT"
  printf '\n**Ladder remaining: 40 alone.**\n'
} >"$BADSTATE_REPO/dev/plans/runs/STATUS-0.8.91.md"
printf '{ this is not json,\n' >"$BADSTATE_REPO/dev/plans/release-state-0.8.91.json"
commit_all "$BADSTATE_REPO" "docs: stamp STATUS-0.8.91 Slice 5 LANDED $BS_SHORT"

# --- Fixture N (TC-133 regression guard, NOT a recurrence arm) ----------------
# Release 0.8.90. Board and state file AGREE on every hand-written row and on
# the `Ladder remaining:` claim. Must exit 0 before AND after the fix, so the
# cross-read cannot buy its red by failing every board that carries a state
# file. Also pins the NON-landed row: Slice 40 says `not started` and IS
# `not started` in the state file — the marker vocabulary must not fire there.
CROSSOK_REPO="$TMPROOT/cross-ok"
init_repo "$CROSSOK_REPO"
mkdir -p "$CROSSOK_REPO/dev/plans/runs" "$CROSSOK_REPO/src" "$CROSSOK_REPO/scripts"
printf 'fixture\n' >"$CROSSOK_REPO/src/keep.txt"
printf '# STATUS — 0.8.90 fixture\n\nSlice 5: not started.\n' \
  >"$CROSSOK_REPO/dev/plans/runs/STATUS-0.8.90.md"
write_state "$CROSSOK_REPO" 0.8.90 '[]' '[{"slice": 5, "status": "NOT_STARTED"}]' '[5, 40]'
commit_all "$CROSSOK_REPO" 'fixture: initial commit'
git -C "$CROSSOK_REPO" checkout -q -b slice-5-fixture
printf 'work\n' >"$CROSSOK_REPO/src/slice5.txt"
commit_all "$CROSSOK_REPO" 'feat: slice 5 work'
git -C "$CROSSOK_REPO" checkout -q main
git -C "$CROSSOK_REPO" merge -q --no-ff -m 'merge(0.8.90): Slice 5 — fixture land' slice-5-fixture
CO_SHORT="$(git -C "$CROSSOK_REPO" rev-parse HEAD)"; CO_SHORT="${CO_SHORT:0:8}"
{
  printf '# STATUS — 0.8.90 fixture\n\n'
  printf '| Slice | Title | Depends-on | Status |\n'
  printf '|------:|-------|-----------|--------|\n'
  printf '| **5** | fixture slice | — | **COMPLETE — LANDED `%s`** (merge). Carried to the next unit. |\n' "$CO_SHORT"
  printf '| 40 | fixture tail | 5 | not started |\n\n'
  printf '**Ladder remaining: 40 alone**, then the HITL publish gate.\n'
} >"$CROSSOK_REPO/dev/plans/runs/STATUS-0.8.90.md"
write_state "$CROSSOK_REPO" 0.8.90 '[5]' \
  "$(printf '[{"slice": 5, "status": "LANDED", "sha": "%s"}, {"slice": 40, "status": "NOT_STARTED"}]' "$CO_SHORT")" \
  '[40]'
commit_all "$CROSSOK_REPO" "docs: stamp STATUS-0.8.90 Slice 5 LANDED $CO_SHORT"

# --- Fixture P (TC-133 FALSE-POSITIVE guard, found by grading a matrix) -------
# Release 0.8.89. `remaining_ladder` is TWO ids, one of them FRACTIONAL, and the
# board writes the claim the natural way: `Ladder remaining: 39.5, 40`.
#
# ⚠ WHY THIS ARM EXISTS. The first draft of the (d) check cut the claim at the
# first punctuation mark. Measured against a matrix rather than one example,
# that read `39.5, 40` as {39.5} — dropping every id after the comma. Against
# this fixture it would HARD-FAIL a board that is perfectly current, i.e. block
# a land. (The same cut also truncated `40.5` to `40` at the `.`, which is this
# unit's own defect class re-introduced by the fix for it.) The claim is now
# read as an ID LIST. This fixture is green before AND after — it is a guard on
# the fix, not a recurrence arm.
COMMA_REPO="$TMPROOT/comma-ladder"
init_repo "$COMMA_REPO"
mkdir -p "$COMMA_REPO/dev/plans/runs" "$COMMA_REPO/src" "$COMMA_REPO/scripts"
printf 'fixture\n' >"$COMMA_REPO/src/keep.txt"
printf '# STATUS — 0.8.89 fixture\n\nSlice 5: not started.\n' \
  >"$COMMA_REPO/dev/plans/runs/STATUS-0.8.89.md"
write_state "$COMMA_REPO" 0.8.89 '[]' '[{"slice": 5, "status": "NOT_STARTED"}]' '[5, 39.5, 40]'
commit_all "$COMMA_REPO" 'fixture: initial commit'
git -C "$COMMA_REPO" checkout -q -b slice-5-fixture
printf 'work\n' >"$COMMA_REPO/src/slice5.txt"
commit_all "$COMMA_REPO" 'feat: slice 5 work'
git -C "$COMMA_REPO" checkout -q main
git -C "$COMMA_REPO" merge -q --no-ff -m 'merge(0.8.89): Slice 5 — fixture land' slice-5-fixture
CM_SHORT="$(git -C "$COMMA_REPO" rev-parse HEAD)"; CM_SHORT="${CM_SHORT:0:8}"
{
  printf '# STATUS — 0.8.89 fixture\n\n'
  printf '| Slice | Title | Depends-on | Status |\n'
  printf '|------:|-------|-----------|--------|\n'
  printf '| 5 | fixture slice | — | **COMPLETE — LANDED `%s`** |\n' "$CM_SHORT"
  printf '| 39.5 | fixture cross-cutting | 5 | not started |\n'
  printf '| 40 | fixture tail | 5 | not started |\n\n'
  printf '**Ladder remaining: 39.5, 40**, then the HITL publish gate.\n'
} >"$COMMA_REPO/dev/plans/runs/STATUS-0.8.89.md"
write_state "$COMMA_REPO" 0.8.89 '[5]' \
  "$(printf '[{"slice": 5, "status": "LANDED", "sha": "%s"}, {"slice": 39.5, "status": "NOT_STARTED"}, {"slice": 40, "status": "NOT_STARTED"}]' "$CM_SHORT")" \
  '[39.5, 40]'
commit_all "$COMMA_REPO" "docs: stamp STATUS-0.8.89 Slice 5 LANDED $CM_SHORT"

# --- Fixture Q (TC-133 RECURRENCE, companion to P): the list UNDER-claims -----
# Release 0.8.88. Identical shape, but the board's claim omits 39.5 while
# `remaining_ladder` still carries it. The list reader must catch that — proving
# fixture P's green comes from reading the whole list, not from the check having
# gone inert on comma-separated claims.
UNDERCLAIM_REPO="$TMPROOT/underclaim-ladder"
init_repo "$UNDERCLAIM_REPO"
mkdir -p "$UNDERCLAIM_REPO/dev/plans/runs" "$UNDERCLAIM_REPO/src" "$UNDERCLAIM_REPO/scripts"
printf 'fixture\n' >"$UNDERCLAIM_REPO/src/keep.txt"
printf '# STATUS — 0.8.88 fixture\n\nSlice 5: not started.\n' \
  >"$UNDERCLAIM_REPO/dev/plans/runs/STATUS-0.8.88.md"
write_state "$UNDERCLAIM_REPO" 0.8.88 '[]' '[{"slice": 5, "status": "NOT_STARTED"}]' '[5, 39.5, 40]'
commit_all "$UNDERCLAIM_REPO" 'fixture: initial commit'
git -C "$UNDERCLAIM_REPO" checkout -q -b slice-5-fixture
printf 'work\n' >"$UNDERCLAIM_REPO/src/slice5.txt"
commit_all "$UNDERCLAIM_REPO" 'feat: slice 5 work'
git -C "$UNDERCLAIM_REPO" checkout -q main
git -C "$UNDERCLAIM_REPO" merge -q --no-ff -m 'merge(0.8.88): Slice 5 — fixture land' slice-5-fixture
UC_SHORT="$(git -C "$UNDERCLAIM_REPO" rev-parse HEAD)"; UC_SHORT="${UC_SHORT:0:8}"
{
  printf '# STATUS — 0.8.88 fixture\n\n'
  printf '| Slice | Title | Depends-on | Status |\n'
  printf '|------:|-------|-----------|--------|\n'
  printf '| 5 | fixture slice | — | **COMPLETE — LANDED `%s`** |\n' "$UC_SHORT"
  printf '| 39.5 | fixture cross-cutting | 5 | not started |\n'
  printf '| 40 | fixture tail | 5 | not started |\n\n'
  printf '**Ladder remaining: 40 alone**, then the HITL publish gate.\n'
} >"$UNDERCLAIM_REPO/dev/plans/runs/STATUS-0.8.88.md"
write_state "$UNDERCLAIM_REPO" 0.8.88 '[5]' \
  "$(printf '[{"slice": 5, "status": "LANDED", "sha": "%s"}, {"slice": 39.5, "status": "NOT_STARTED"}, {"slice": 40, "status": "NOT_STARTED"}]' "$UC_SHORT")" \
  '[39.5, 40]'
commit_all "$UNDERCLAIM_REPO" "docs: stamp STATUS-0.8.88 Slice 5 LANDED $UC_SHORT"

# --- Fixture R (TC-133 RECURRENCE, fix-1): DUPLICATE ids in the STATE file ----
# Release 0.8.87. The BOARD is perfectly current — one row per slice, the landed
# row cites its own landing SHA, the `Ladder remaining:` claim agrees. The defect
# is entirely inside `release-state-0.8.87.json`: its ladder carries TWO entries
# for the same slice, written `30` and `30.0`.
#
# ⚠ WHY THOSE TWO TOKENS. They are distinct JSON numbers but `slice_str` renders
# both as "30" (and in Python `30 == 30.0` and `hash(30) == hash(30.0)`), so the
# cross-read's `by_slice[slice_str(...)] = entry` silently kept whichever entry
# came SECOND in the file — dict-insertion order, i.e. FILE LINE ORDER. That is
# the exact "SILENTLY OVERWRITE" collapse this whole unit exists to end, and the
# sibling reader of the same file, `_by_slice` in check-release-state-views.sh,
# already REFUSES it. Two readers of one file must not disagree about what it
# means.
#
# ⚠ RED-FIRST PROOF: against the UNFIXED checker this fixture exits **0**. Both
# entries here carry the SAME sha, so the last-writer-wins map still answers the
# (c) SHA check correctly and the gate CERTIFIES A MALFORMED STATE FILE — the
# quiet half of the failure. (The loud half is the same shape with differing
# shas, where the landed row is then compared against the WRONG entry's SHA.)
# The fixture is deliberately built on the quiet half: an arm whose red comes
# from a wrong-SHA diagnostic would also go red for reasons unrelated to the
# duplicate, and could not distinguish "the guard fired" from "the map happened
# to pick the other entry".
DUPSTATE_REPO="$TMPROOT/dup-state"
init_repo "$DUPSTATE_REPO"
mkdir -p "$DUPSTATE_REPO/dev/plans/runs" "$DUPSTATE_REPO/src" "$DUPSTATE_REPO/scripts"
printf 'fixture\n' >"$DUPSTATE_REPO/src/keep.txt"
printf '# STATUS — 0.8.87 fixture\n\nSlice 30: not started.\n' \
  >"$DUPSTATE_REPO/dev/plans/runs/STATUS-0.8.87.md"
write_state "$DUPSTATE_REPO" 0.8.87 '[]' '[{"slice": 30, "status": "NOT_STARTED"}]' '[30, 40]'
commit_all "$DUPSTATE_REPO" 'fixture: initial commit'
git -C "$DUPSTATE_REPO" checkout -q -b slice-30-fixture
printf 'work\n' >"$DUPSTATE_REPO/src/slice30.txt"
commit_all "$DUPSTATE_REPO" 'feat: slice 30 work'
git -C "$DUPSTATE_REPO" checkout -q main
git -C "$DUPSTATE_REPO" merge -q --no-ff -m 'merge(0.8.87): Slice 30 — fixture land' slice-30-fixture
DS_SHORT="$(git -C "$DUPSTATE_REPO" rev-parse HEAD)"; DS_SHORT="${DS_SHORT:0:8}"
{
  printf '# STATUS — 0.8.87 fixture\n\n'
  printf '| Slice | Title | Depends-on | Status |\n'
  printf '|------:|-------|-----------|--------|\n'
  printf '| 30 | fixture slice | — | **COMPLETE — LANDED `%s`** |\n' "$DS_SHORT"
  printf '| 40 | fixture tail | 30 | not started |\n\n'
  printf '**Ladder remaining: 40 alone**, then the HITL publish gate.\n'
} >"$DUPSTATE_REPO/dev/plans/runs/STATUS-0.8.87.md"
write_state "$DUPSTATE_REPO" 0.8.87 '[30]' \
  "$(printf '[{"slice": 30, "status": "LANDED", "sha": "%s"}, {"slice": 30.0, "status": "LANDED", "sha": "%s"}, {"slice": 40, "status": "NOT_STARTED"}]' "$DS_SHORT" "$DS_SHORT")" \
  '[40]'
commit_all "$DUPSTATE_REPO" "docs: stamp STATUS-0.8.87 Slice 30 LANDED $DS_SHORT"

# ============================================================================
# fix-2 fixtures — two NEW and DISTINCT fail-open paths in the TC-133 cross-read
# that Leg 3 added. Both are the same failure family as the fix-1 vacuous-pass
# guard: the gate exits 0 (or is DOWNGRADED to exit 0 by its caller) while being
# silently wrong, which is the precise defect class this unit exists to close.
#
#   FINDING A — a slice listed in `landed` whose `ladder` entry is missing, or
#     carries no `sha`, made check (c) — the LOAD-BEARING half of the cross-read,
#     "the hand-written row must cite the slice's own landing SHA" — a NO-OP for
#     that slice. The construct was `sha = (by_slice.get(key) or {}).get("sha")`
#     followed by `if sha and sha not in status:`. That is exactly the state-file
#     INCOMPLETENESS this check exists to catch, and it let the whole gate exit 0
#     on the strength of the fragile marker regex (b) alone.
#
#     ⚠ MEASURED AGAINST THE REAL STATE FILE BEFORE CHOOSING A HARD FAIL. All 14
#     ids in `landed` in dev/plans/release-state-0.8.20.json (0, 5, 10, 15, 20,
#     21, 22, 23, 25, 30, 31, 32, 33, 39) have a `ladder` entry AND a non-null
#     `sha`; zero exceptions. A hard fail therefore does not redden the live
#     gate. Had any real landed slice legitimately lacked one, this would have
#     been escalated rather than coded around.
#
#   FINDING B — Leg 3 validated KEY PRESENCE but not TYPE, so a state file whose
#     `landed` / `ladder` / `remaining_ladder` had the wrong type sailed past
#     validation and blew up in a later loop as an UNPREFIXED Python traceback.
#     The standalone checker exits non-zero, but `preflight.sh --landing`
#     converted only `STALE*` lines into HARD failures — every other line became
#     INFO — so the whole cross-read FAILED OPEN inside the landing gate. See the
#     preflight arms at the bottom of this file: pre-fix, `--landing` exits 0.
# ============================================================================

# --- Fixture S (fix-2 Finding A RECURRENCE): landed, but the ladder entry has
# no `sha`. Release 0.8.86. The board is otherwise IMPECCABLE — one row per
# slice, the landed row cites the real landing SHA, the `Ladder remaining:`
# claim agrees — so nothing else in the cross-read can fire. The defect is
# entirely the state file's: Slice 5 is in `landed` and its ladder entry records
# `status: LANDED` with NO `sha` key at all.
#
# ⚠ RED-FIRST PROOF: against the UNFIXED checker this fixture exits **0**.
# `(by_slice.get("5") or {}).get("sha")` is None, `if sha and ...` is False, and
# check (c) never runs — the gate certifies the row without ever reconciling it.
NOSHA_REPO="$TMPROOT/no-sha"
init_repo "$NOSHA_REPO"
mkdir -p "$NOSHA_REPO/dev/plans/runs" "$NOSHA_REPO/src" "$NOSHA_REPO/scripts"
printf 'fixture\n' >"$NOSHA_REPO/src/keep.txt"
printf '# STATUS — 0.8.86 fixture\n\nSlice 5: not started.\n' \
  >"$NOSHA_REPO/dev/plans/runs/STATUS-0.8.86.md"
write_state "$NOSHA_REPO" 0.8.86 '[]' '[{"slice": 5, "status": "NOT_STARTED"}]' '[5, 40]'
commit_all "$NOSHA_REPO" 'fixture: initial commit'
git -C "$NOSHA_REPO" checkout -q -b slice-5-fixture
printf 'work\n' >"$NOSHA_REPO/src/slice5.txt"
commit_all "$NOSHA_REPO" 'feat: slice 5 work'
git -C "$NOSHA_REPO" checkout -q main
git -C "$NOSHA_REPO" merge -q --no-ff -m 'merge(0.8.86): Slice 5 — fixture land' slice-5-fixture
NS_SHORT="$(git -C "$NOSHA_REPO" rev-parse HEAD)"; NS_SHORT="${NS_SHORT:0:8}"
{
  printf '# STATUS — 0.8.86 fixture\n\n'
  printf '| Slice | Title | Depends-on | Status |\n'
  printf '|------:|-------|-----------|--------|\n'
  printf '| 5 | fixture slice | — | **COMPLETE — LANDED `%s`** |\n' "$NS_SHORT"
  printf '| 40 | fixture tail | 5 | not started |\n\n'
  printf '**Ladder remaining: 40 alone**, then the HITL publish gate.\n'
} >"$NOSHA_REPO/dev/plans/runs/STATUS-0.8.86.md"
write_state "$NOSHA_REPO" 0.8.86 '[5]' \
  '[{"slice": 5, "status": "LANDED"}, {"slice": 40, "status": "NOT_STARTED"}]' \
  '[40]'
commit_all "$NOSHA_REPO" "docs: stamp STATUS-0.8.86 Slice 5 LANDED $NS_SHORT"

# --- Fixture T (fix-2 Finding A RECURRENCE): landed, but NO ladder entry at all
# Release 0.8.85. Same shape as S, one rung further: `landed` names Slice 5 and
# the `ladder` array has no entry for it whatsoever. Kept SEPARATE from S so each
# half of the completeness predicate owns its own red — a single fixture tripping
# both would let either half rot unnoticed.
#
# ⚠ RED-FIRST PROOF: unfixed checker exits **0**. `by_slice.get("5")` is None,
# `or {}` swallows it, and check (c) is again a silent no-op. Note the board DOES
# carry a row for Slice 5, so the pre-existing "has NO row for it" predicate
# cannot be what makes this arm red.
NOENTRY_REPO="$TMPROOT/no-ladder-entry"
init_repo "$NOENTRY_REPO"
mkdir -p "$NOENTRY_REPO/dev/plans/runs" "$NOENTRY_REPO/src" "$NOENTRY_REPO/scripts"
printf 'fixture\n' >"$NOENTRY_REPO/src/keep.txt"
printf '# STATUS — 0.8.85 fixture\n\nSlice 5: not started.\n' \
  >"$NOENTRY_REPO/dev/plans/runs/STATUS-0.8.85.md"
write_state "$NOENTRY_REPO" 0.8.85 '[]' '[{"slice": 5, "status": "NOT_STARTED"}]' '[5, 40]'
commit_all "$NOENTRY_REPO" 'fixture: initial commit'
git -C "$NOENTRY_REPO" checkout -q -b slice-5-fixture
printf 'work\n' >"$NOENTRY_REPO/src/slice5.txt"
commit_all "$NOENTRY_REPO" 'feat: slice 5 work'
git -C "$NOENTRY_REPO" checkout -q main
git -C "$NOENTRY_REPO" merge -q --no-ff -m 'merge(0.8.85): Slice 5 — fixture land' slice-5-fixture
NE_SHORT="$(git -C "$NOENTRY_REPO" rev-parse HEAD)"; NE_SHORT="${NE_SHORT:0:8}"
{
  printf '# STATUS — 0.8.85 fixture\n\n'
  printf '| Slice | Title | Depends-on | Status |\n'
  printf '|------:|-------|-----------|--------|\n'
  printf '| 5 | fixture slice | — | **COMPLETE — LANDED `%s`** |\n' "$NE_SHORT"
  printf '| 40 | fixture tail | 5 | not started |\n\n'
  printf '**Ladder remaining: 40 alone**, then the HITL publish gate.\n'
} >"$NOENTRY_REPO/dev/plans/runs/STATUS-0.8.85.md"
write_state "$NOENTRY_REPO" 0.8.85 '[5]' \
  '[{"slice": 40, "status": "NOT_STARTED"}]' \
  '[40]'
commit_all "$NOENTRY_REPO" "docs: stamp STATUS-0.8.85 Slice 5 LANDED $NE_SHORT"

# --- Fixture U (fix-2 Finding B RECURRENCE): `landed` is present but is NOT a
# JSON array. Release 0.8.84. The board is fully current; the ONLY defect is
# `"landed": 5` in the state file.
#
# ⚠ RED-FIRST PROOF, AND IT IS TWO-LEVEL. Leg 3's validation asked only `if key
# not in st`, so a wrong TYPE sails through it and detonates later:
#   * unfixed CHECKER: rc=1, but the output is a bare
#     `TypeError: 'int' object is not iterable` traceback with NO `STALE` line.
#   * unfixed `preflight.sh --landing`: **rc=0**. §7 promoted only `STALE*` lines
#     to HARD; every traceback line became INFO and the land was CERTIFIED. That
#     is the fail-open, and it is the arm that matters (Arm 20b below).
BADTYPE_REPO="$TMPROOT/bad-type-landed"
init_repo "$BADTYPE_REPO"
mkdir -p "$BADTYPE_REPO/dev/plans/runs" "$BADTYPE_REPO/src" "$BADTYPE_REPO/scripts"
printf 'fixture\n' >"$BADTYPE_REPO/src/keep.txt"
printf '# STATUS — 0.8.84 fixture\n\nSlice 5: not started.\n' \
  >"$BADTYPE_REPO/dev/plans/runs/STATUS-0.8.84.md"
write_state "$BADTYPE_REPO" 0.8.84 '[]' '[{"slice": 5, "status": "NOT_STARTED"}]' '[5, 40]'
commit_all "$BADTYPE_REPO" 'fixture: initial commit'
git -C "$BADTYPE_REPO" checkout -q -b slice-5-fixture
printf 'work\n' >"$BADTYPE_REPO/src/slice5.txt"
commit_all "$BADTYPE_REPO" 'feat: slice 5 work'
git -C "$BADTYPE_REPO" checkout -q main
git -C "$BADTYPE_REPO" merge -q --no-ff -m 'merge(0.8.84): Slice 5 — fixture land' slice-5-fixture
BT_SHORT="$(git -C "$BADTYPE_REPO" rev-parse HEAD)"; BT_SHORT="${BT_SHORT:0:8}"
{
  printf '# STATUS — 0.8.84 fixture\n\n'
  printf '| Slice | Title | Depends-on | Status |\n'
  printf '|------:|-------|-----------|--------|\n'
  printf '| 5 | fixture slice | — | **COMPLETE — LANDED `%s`** |\n' "$BT_SHORT"
  printf '| 40 | fixture tail | 5 | not started |\n\n'
  printf '**Ladder remaining: 40 alone**, then the HITL publish gate.\n'
} >"$BADTYPE_REPO/dev/plans/runs/STATUS-0.8.84.md"
# `landed` is a NUMBER, not an array. Key present -> Leg 3's check passed it.
write_state "$BADTYPE_REPO" 0.8.84 '5' \
  "$(printf '[{"slice": 5, "status": "LANDED", "sha": "%s"}, {"slice": 40, "status": "NOT_STARTED"}]' "$BT_SHORT")" \
  '[40]'
commit_all "$BADTYPE_REPO" "docs: stamp STATUS-0.8.84 Slice 5 LANDED $BT_SHORT"

# --- Fixture V (fix-2 Finding B RECURRENCE): `ladder` is an array of NUMBERS,
# not of objects. Release 0.8.83. This is the QUIET half of the type hole and it
# never raises at all: the `by_slice` builder guarded itself with
# `isinstance(entry, dict)`, so every non-object entry was SILENTLY DROPPED, the
# map came out EMPTY, and check (c) then went no-op for every landed slice via
# Finding A's construct. Two independent fail-opens composing into one green.
#
# ⚠ RED-FIRST PROOF: unfixed checker exits **0** — no traceback, no diagnostic,
# a malformed state file CERTIFIED. This is why "ladder must be a list of
# OBJECTS" is part of the type validation and not just "ladder must be a list".
BADLADDER_REPO="$TMPROOT/bad-type-ladder"
init_repo "$BADLADDER_REPO"
mkdir -p "$BADLADDER_REPO/dev/plans/runs" "$BADLADDER_REPO/src" "$BADLADDER_REPO/scripts"
printf 'fixture\n' >"$BADLADDER_REPO/src/keep.txt"
printf '# STATUS — 0.8.83 fixture\n\nSlice 5: not started.\n' \
  >"$BADLADDER_REPO/dev/plans/runs/STATUS-0.8.83.md"
write_state "$BADLADDER_REPO" 0.8.83 '[]' '[{"slice": 5, "status": "NOT_STARTED"}]' '[5, 40]'
commit_all "$BADLADDER_REPO" 'fixture: initial commit'
git -C "$BADLADDER_REPO" checkout -q -b slice-5-fixture
printf 'work\n' >"$BADLADDER_REPO/src/slice5.txt"
commit_all "$BADLADDER_REPO" 'feat: slice 5 work'
git -C "$BADLADDER_REPO" checkout -q main
git -C "$BADLADDER_REPO" merge -q --no-ff -m 'merge(0.8.83): Slice 5 — fixture land' slice-5-fixture
BL_SHORT="$(git -C "$BADLADDER_REPO" rev-parse HEAD)"; BL_SHORT="${BL_SHORT:0:8}"
{
  printf '# STATUS — 0.8.83 fixture\n\n'
  printf '| Slice | Title | Depends-on | Status |\n'
  printf '|------:|-------|-----------|--------|\n'
  printf '| 5 | fixture slice | — | **COMPLETE — LANDED `%s`** |\n' "$BL_SHORT"
  printf '| 40 | fixture tail | 5 | not started |\n\n'
  printf '**Ladder remaining: 40 alone**, then the HITL publish gate.\n'
} >"$BADLADDER_REPO/dev/plans/runs/STATUS-0.8.83.md"
write_state "$BADLADDER_REPO" 0.8.83 '[5]' '[5, 40]' '[40]'
commit_all "$BADLADDER_REPO" "docs: stamp STATUS-0.8.83 Slice 5 LANDED $BL_SHORT"

# --- Fixture W (fix-2 Finding B, the GENERAL fail-open): an UNEXPECTED
# exception anywhere in the cross-read. Release 0.8.82.
#
# The state file is well-formed and every type is right, so NONE of the
# validation added by this fix-round touches this fixture. The board carries a
# raw 0xFF byte, which is not valid UTF-8, so
# `open(board_path, encoding="utf-8").read()` raises UnicodeDecodeError — a
# ValueError, NOT an OSError, so the `except OSError` around that read does not
# see it. Result: an unprefixed traceback and rc=1.
#
# ⚠ THE 0xFF BYTE IS THE POINT AND MUST NOT BE "FIXED". This arm's subject is
# NOT UnicodeDecodeError; it is the INVARIANT that ANY unanticipated failure of
# the cross-read HARD-BLOCKS A LAND rather than degrading to INFO. Giving this
# one exception a bespoke handler would delete the arm and leave the invariant
# untested for the NEXT unanticipated exception — and codex's finding is
# explicitly about the general case, not this instance. It is also why the
# [DETERMINE] locus is preflight.sh (see Arm 22): a `try/except` inside the
# checker's Python can only ever cover exceptions raised INSIDE that Python, and
# the checker also fails at the BASH level (a missing lib/board-closed.sh, an
# unresolvable --tip, an unknown arg -> exit 2) with no STALE line at all.
#
# ⚠ RED-FIRST PROOF: unfixed `preflight.sh --landing` exits **0** on this tree.
BOOM_REPO="$TMPROOT/unexpected-exception"
init_repo "$BOOM_REPO"
mkdir -p "$BOOM_REPO/dev/plans/runs" "$BOOM_REPO/src" "$BOOM_REPO/scripts"
printf 'fixture\n' >"$BOOM_REPO/src/keep.txt"
printf '# STATUS — 0.8.82 fixture\n\nSlice 5: not started.\n' \
  >"$BOOM_REPO/dev/plans/runs/STATUS-0.8.82.md"
write_state "$BOOM_REPO" 0.8.82 '[]' '[{"slice": 5, "status": "NOT_STARTED"}]' '[5, 40]'
commit_all "$BOOM_REPO" 'fixture: initial commit'
git -C "$BOOM_REPO" checkout -q -b slice-5-fixture
printf 'work\n' >"$BOOM_REPO/src/slice5.txt"
commit_all "$BOOM_REPO" 'feat: slice 5 work'
git -C "$BOOM_REPO" checkout -q main
git -C "$BOOM_REPO" merge -q --no-ff -m 'merge(0.8.82): Slice 5 — fixture land' slice-5-fixture
BM_SHORT="$(git -C "$BOOM_REPO" rev-parse HEAD)"; BM_SHORT="${BM_SHORT:0:8}"
{
  printf '# STATUS — 0.8.82 fixture\n\n'
  printf '| Slice | Title | Depends-on | Status |\n'
  printf '|------:|-------|-----------|--------|\n'
  printf '| 5 | fixture slice | \xff | **COMPLETE — LANDED `%s`** |\n' "$BM_SHORT"
  printf '| 40 | fixture tail | 5 | not started |\n\n'
  printf '**Ladder remaining: 40 alone**, then the HITL publish gate.\n'
} >"$BOOM_REPO/dev/plans/runs/STATUS-0.8.82.md"
write_state "$BOOM_REPO" 0.8.82 '[5]' \
  "$(printf '[{"slice": 5, "status": "LANDED", "sha": "%s"}, {"slice": 40, "status": "NOT_STARTED"}]' "$BM_SHORT")" \
  '[40]'
commit_all "$BOOM_REPO" "docs: stamp STATUS-0.8.82 Slice 5 LANDED $BM_SHORT"

run_checker() {
  local dir="$1" checker="${2:-$CHECKER}"
  set +e
  OUT="$(cd "$dir" && bash "$checker" 2>&1)"
  RC=$?
  set -e
}

run_preflight() {
  local cwd="$1"; shift
  set +e
  OUT="$(cd "$cwd" && bash "$PREFLIGHT" "$@" 2>&1)"
  RC=$?
  set -e
}

# =========================== check-board-currency.sh ===========================

# --- Arm 1: current board — checker exits 0 -------------------------------------
run_checker "$CURRENT_REPO"
if [ "$RC" -eq 0 ]; then
  pass "check-board-currency.sh exits 0 on a current (SHA-stamped) board"
else
  fail "expected exit 0 on a current board; got rc=$RC, out: $OUT"
fi

# The board directory may contain untracked scratch STATUS files. They are not
# release evidence and must not create a vacuous/stale second board.
printf '# untracked scratch board\n' >"$CURRENT_REPO/dev/plans/runs/STATUS-0.8.98.md"
run_checker "$CURRENT_REPO"
if [ "$RC" -eq 0 ]; then
  pass "check-board-currency ignores untracked STATUS boards"
else
  fail "untracked board changed the currency verdict: rc=$RC, out=$OUT"
fi

run_checker "$CANONICAL_REPO"
if [ "$RC" -eq 0 ]; then
  pass "canonical landed SHA ancestry passes without a merge-subject match or board table"
else
  fail "canonical state facts should vouch for a concise board: rc=$RC, out=$OUT"
fi

python3 - "$CANONICAL_REPO/dev/plans/release-state-0.8.99.json" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
d['ladder'][0]['sha'] = 'deadbeef'
open(p, 'w').write(json.dumps(d))
PY
run_checker "$CANONICAL_REPO"
if [ "$RC" -ne 0 ] && grep -q 'release-state SHA deadbeef does not resolve' <<<"$OUT"; then
  pass "canonical machine facts fail closed when a landed SHA is unresolvable"
else
  fail "bad canonical SHA: rc=$RC out=$OUT"
fi

python3 - "$CANONICAL_REPO/dev/plans/release-state-0.8.99.json" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
d['ladder'][0]['sha'] = None
open(p, 'w').write(json.dumps(d))
PY
run_checker "$CANONICAL_REPO"
if [ "$RC" -ne 0 ] && grep -q 'has no non-empty ladder sha' <<<"$OUT"; then
  pass "canonical machine facts fail closed when a landed SHA is missing"
else
  fail "missing canonical SHA: rc=$RC out=$OUT"
fi

git -C "$CANONICAL_REPO" checkout -q -b unreachable-state-sha
printf 'unreachable\n' >"$CANONICAL_REPO/src/unreachable.txt"
commit_all "$CANONICAL_REPO" 'fixture: unreachable state commit'
UNREACHABLE_SHA="$(git -C "$CANONICAL_REPO" rev-parse --short=8 HEAD)"
git -C "$CANONICAL_REPO" checkout -q main
python3 - "$CANONICAL_REPO/dev/plans/release-state-0.8.99.json" "$UNREACHABLE_SHA" <<'PY'
import json, sys
p, sha = sys.argv[1:]
d = json.load(open(p))
d['ladder'][0]['sha'] = sha
open(p, 'w').write(json.dumps(d))
PY
run_checker "$CANONICAL_REPO"
if [ "$RC" -ne 0 ] && grep -q "release-state SHA $UNREACHABLE_SHA is not reachable" <<<"$OUT"; then
  pass "canonical machine facts fail closed when a landed SHA is unreachable"
else
  fail "unreachable canonical SHA: rc=$RC out=$OUT"
fi

# Terminal-release regression: once every tracked release is published,
# release-current.py returns success with no current board. Retained historical
# boards must not be currency-checked merely because they predate the CLOSED
# banner convention.
run_checker "$REPO_ROOT"
if [ "$RC" -eq 0 ] \
   && grep -q 'no live STATUS-0.8.z.md board contradicts git ancestry' <<<"$OUT" \
   && ! grep -q '0.8.20' <<<"$OUT"; then
  pass "all published releases leave no board-currency work"
else
  fail "terminal-release regression: rc=$RC out=$OUT"
fi

# --- Arm 2: stale board — checker exits non-zero, names the sha ----------------
run_checker "$STALE_REPO"
if [ "$RC" -ne 0 ]; then
  pass "check-board-currency.sh exits non-zero on a stale board"
else
  fail "expected non-zero exit on a stale board; got rc=0, out: $OUT"
fi
if printf '%s' "$OUT" | grep -q "STALE.*${STALE_SHORT_SHA}"; then
  pass "stale output names the un-referenced landing SHA"
else
  fail "expected a STALE line naming $STALE_SHORT_SHA; got: $OUT"
fi

# --- Arm 3: CLOSED-banner board — never flagged even though board text is stale
run_checker "$CLOSED_REPO"
if [ "$RC" -eq 0 ]; then
  pass "check-board-currency.sh skips a CLOSED-banner board (never flagged)"
else
  fail "a CLOSED-banner board must be skipped, not flagged; got rc=$RC, out: $OUT"
fi

# --- Arm 3b: CLOSED-banner board with YAML frontmatter — still skipped ---------
run_checker "$CLOSED_FM_REPO"
if [ "$RC" -eq 0 ]; then
  pass "check-board-currency.sh skips a CLOSED-banner board even behind YAML frontmatter"
else
  fail "frontmatter must not defeat the CLOSED-banner skip; got rc=$RC, out: $OUT"
fi

# --- Arm 4: superseded partial merge — only the newest land is required --------
run_checker "$SUPERSEDED_REPO"
if [ "$RC" -eq 0 ]; then
  pass "check-board-currency.sh does not require a superseded partial-merge SHA"
else
  fail "superseded-merge fixture should pass (only newest land required); got rc=$RC, out: $OUT"
fi

# --- Arm 4b (fix-1): vacuous-pass guard — zero-matched live board HARD-fails --
# The RED-first proof for this fix: pre-fix, this fixture's checker run passed
# (exit 0, "ok") because the per-slice loop body never executed for it — the
# gate silently vouched for a board it never actually checked. Post-fix it must
# be a HARD failure with a distinct, actionable message.
run_checker "$NOMATCH_REPO"
if [ "$RC" -ne 0 ]; then
  pass "check-board-currency.sh HARD-fails a live board with zero matched landing merges"
else
  fail "a live board with zero matched lands must not pass vacuously; got rc=0, out: $OUT"
fi
if printf '%s' "$OUT" | grep -q 'STALE.*no landing merge matched the convention'; then
  pass "vacuous-pass failure names the convention + cannot-vouch reason"
else
  fail "expected a STALE line naming the unmatched convention; got: $OUT"
fi

# --- Arm 4ba: state-backed initial release — zero matches are expected ---------
# This is intentionally RED before the activation exception: the board has no
# land yet, but its single-writer state proves this is the first slice of a
# newly activated release and TC-133 can structurally cross-read the ladder.
run_checker "$INITIAL_REPO"
if [ "$RC" -eq 0 ] \
   && grep -q 'note.*newly activated initial release' <<<"$OUT" \
   && ! grep -q 'STALE.*no landing merge matched the convention' <<<"$OUT"; then
  pass "a state-backed newly activated initial release may have zero landing merges"
else
  fail "initial release must pass only through the explicit state-backed exception; rc=$RC, out: $OUT"
fi

# This is RED before the initial-state predicate rejects a ladder which claims
# a land without including that slice in `landed`; a non-empty SHA alone is
# likewise landing evidence that cannot coexist with an initial release.
run_checker "$INITIAL_UNRECORDED_LAND_REPO"
if [ "$RC" -ne 0 ] && grep -q 'STALE.*no landing merge matched the convention' <<<"$OUT"; then
  pass "a ladder LANDED status absent from landed still HARD-fails"
else
  fail "unrecorded ladder land must not qualify for the initial-release exception; rc=$RC, out: $OUT"
fi

run_checker "$INITIAL_UNRECORDED_SHA_REPO"
if [ "$RC" -ne 0 ] && grep -q 'STALE.*no landing merge matched the convention' <<<"$OUT"; then
  pass "a ladder SHA absent from landed still HARD-fails"
else
  fail "unrecorded ladder SHA must not qualify for the initial-release exception; rc=$RC, out: $OUT"
fi

run_checker "$INITIAL_NO_TABLE_REPO"
if [ "$RC" -ne 0 ] && grep -q 'STALE.*carries no ladder table' <<<"$OUT"; then
  pass "a state-backed initial release without a STATUS ladder still HARD-fails"
else
  fail "initial release needs a STATUS ladder for the state-backed exception; rc=$RC, out: $OUT"
fi

# The exception is deliberately narrow: a later `next_slice`, malformed state,
# or any recorded landed slice stays a hard failure when no landing evidence is
# reachable. NOMATCH_REPO above already supplies the no-state case.
run_checker "$INITIAL_NONFIRST_REPO"
if [ "$RC" -ne 0 ] && grep -q 'STALE.*no landing merge matched the convention' <<<"$OUT"; then
  pass "a zero-land state whose next slice is not first still HARD-fails"
else
  fail "non-first initial state must retain the vacuous-pass failure; rc=$RC, out: $OUT"
fi

run_checker "$INITIAL_MALFORMED_REPO"
if [ "$RC" -ne 0 ] && grep -q 'STALE.*no landing merge matched the convention' <<<"$OUT"; then
  pass "a malformed initial state still HARD-fails"
else
  fail "malformed state must not qualify for the initial-release exception; rc=$RC, out: $OUT"
fi

run_checker "$INITIAL_LANDED_NO_SHA_REPO"
if [ "$RC" -ne 0 ] && grep -q 'STALE.*no landing merge matched the convention' <<<"$OUT"; then
  pass "a landed state missing its expected merge/SHA still HARD-fails"
else
  fail "landed state must not qualify for the initial-release exception; rc=$RC, out: $OUT"
fi

# --- Arm 4c (fix-1): regression guard — a live board WITH >=1 matched land ------
# still passes (the guard converts silent->loud ONLY for zero matches; it must
# never fire once real evidence exists). Exercises a board with a single match
# (CURRENT_REPO, already asserted in Arm 1) AND one with two matches across two
# slices worth of history (SUPERSEDED_REPO, already asserted in Arm 4) via a
# fresh, explicit assertion naming this fix directly.
run_checker "$CURRENT_REPO"
if [ "$RC" -eq 0 ]; then
  pass "fix-1 regression guard: a live board with >=1 matched land still exits 0"
else
  fail "fix-1 must not fail a board with a real matched land; got rc=$RC, out: $OUT"
fi
run_checker "$SUPERSEDED_REPO"
if [ "$RC" -eq 0 ]; then
  pass "fix-1 regression guard: a live board with 2 slices' worth of matched lands still exits 0"
else
  fail "fix-1 must not fail a multi-slice board with real matched lands; got rc=$RC, out: $OUT"
fi

# --- Arm 9 (SLICE-ID-HARDENING site 1): fractional id must not COLLIDE with its
# integer neighbour and swallow that neighbour's SHA check.
# RED-first proof: against the unfixed `([0-9]+)` capture this fixture exits 0
# (`Slice 39.5` -> `39` -> SEEN_SLICE[39] -> the real `Slice 39` merge is
# discarded as a "superseded intermediate"), so the gate silently vouches for a
# board that never cites Slice 39's landing commit.
run_checker "$FRACTIONAL_REPO"
if [ "$RC" -ne 0 ]; then
  pass "site 1: a fractional slice id does not swallow its integer neighbour's SHA check"
else
  fail "site 1 RECURRENCE: Slice 39.5 collided onto SEEN_SLICE[39] and Slice 39's stale SHA went unchecked; got rc=0, out: $OUT"
fi
if printf '%s' "$OUT" | grep -q "STALE.*Slice 39: landing commit ${FRAC_39_SHORT}"; then
  pass "site 1: the un-cited INTEGER land (Slice 39) is named with its own SHA"
else
  fail "expected a STALE line 'Slice 39: landing commit $FRAC_39_SHORT'; got: $OUT"
fi
# ANTI-VACUITY: the fractional land IS cited by the board, so it must NOT be
# flagged. If both were flagged the arm above could pass for the wrong reason.
if printf '%s' "$OUT" | grep -q "landing commit ${FRAC_395_SHORT}"; then
  fail "the CITED fractional land ($FRAC_395_SHORT) must not be flagged stale; out: $OUT"
else
  pass "site 1 anti-vacuity: the cited fractional land is not flagged"
fi

# --- Arm 10 (SLICE-ID-HARDENING): the STALE diagnostic must not FABRICATE a
# slice pointer. Pre-fix this fixture also exits non-zero — so exit code alone
# proves nothing here — but the line it prints reads `Slice 39: landing commit
# <39.5's sha>`, pointing the reader at the wrong unit. The RED-first proof is
# the message assertion, not the rc.
run_checker "$FABRICATED_REPO"
if [ "$RC" -ne 0 ]; then
  pass "fabricated-pointer fixture: an un-cited fractional land is still flagged"
else
  fail "an un-cited fractional land must be flagged; got rc=0, out: $OUT"
fi
if printf '%s' "$OUT" | grep -q "STALE.*Slice 39\.5: landing commit ${FAB_395_SHORT}"; then
  pass "the STALE diagnostic names the REAL slice id (39.5), not its truncation"
else
  fail "FABRICATED POINTER: expected 'Slice 39.5: landing commit $FAB_395_SHORT'; got: $OUT"
fi
if printf '%s' "$OUT" | grep -q "Slice 39: landing commit ${FAB_395_SHORT}"; then
  fail "FABRICATED POINTER: 39.5's SHA $FAB_395_SHORT is reported under the label 'Slice 39'; out: $OUT"
else
  pass "no STALE line attributes 39.5's SHA to Slice 39"
fi

# --- Arm 11 (regression guard, NOT a recurrence arm): a board that cites BOTH a
# fractional land and its integer neighbour still exits 0. Green before and
# after the fix; present so the fix cannot buy its red by failing everything.
run_checker "$FRAC_OK_REPO"
if [ "$RC" -eq 0 ]; then
  pass "regression guard: a board citing both Slice 39 and Slice 39.5 still exits 0"
else
  fail "a fully-current fractional board must not be flagged; got rc=$RC, out: $OUT"
fi

# ===================== TC-133 — the board/state CROSS-READ =====================
# §3a of the SLICE-ID-HARDENING brief (HITL ruling 2026-07-30, steward seq-212
# option (b); todos-ledger seq-197). A FIFTH site, independent of site 1.

# --- Arm 12 (TC-133 RECURRENCE): a board that contradicts its own state file ---
# The landing merge is found, its short SHA IS in the file, and the hand-written
# ladder row still says `not started`. Pre-fix: rc=0. Post-fix: HARD fail.
run_checker "$CONTRADICT_REPO"
if [ "$RC" -ne 0 ]; then
  pass "TC-133: a board whose hand-written row contradicts release-state HARD-fails"
else
  fail "TC-133 RECURRENCE: Slice 5 is LANDED in release-state-0.8.93.json, the board row still says 'not started', and the gate exited 0 because the SHA appears in a GENERATED cell; out: $OUT"
fi
# ANTI-VACUITY: the red must be attributable to the cross-read, not to some
# other predicate happening to trip on this fixture.
if printf '%s' "$OUT" | grep -q 'TC-133'; then
  pass "TC-133: the failure is attributed to the board/state cross-read"
else
  fail "expected the failure to name TC-133 (the cross-read), not some other predicate; got: $OUT"
fi
if printf '%s' "$OUT" | grep -q 'Slice 5'; then
  pass "TC-133: the failure names the contradicting slice (5)"
else
  fail "expected the failure to name Slice 5; got: $OUT"
fi
# ANTI-VACUITY: the SHA-presence predicate must still have PASSED here — the
# short SHA really is in the file. If the gate red were coming from the old
# "SHA not referenced anywhere" line, this arm would prove nothing new.
if printf '%s' "$OUT" | grep -q "is not referenced anywhere"; then
  fail "the pre-existing absent-SHA predicate fired — this fixture must be red ONLY via the cross-read (the SHA $CONTRA_SHORT IS present); out: $OUT"
else
  pass "TC-133 anti-vacuity: the absent-SHA predicate did NOT fire (the SHA is present); the red is the cross-read's"
fi
# Slice 40 is NOT landed and its row legitimately says `not started` — the
# marker vocabulary must not fire on a slice the state file agrees is pending.
if printf '%s' "$OUT" | grep -q 'Slice 40'; then
  fail "the cross-read flagged Slice 40, which release-state-0.8.93.json agrees is NOT_STARTED; out: $OUT"
else
  pass "TC-133 anti-vacuity: a legitimately-pending slice is not flagged"
fi

# --- Arm 13 (TC-133 RECURRENCE): `Ladder remaining:` prose vs remaining_ladder -
# Every ladder row is current; the ONLY contradiction is the prose claim.
# Pre-fix: rc=0. Post-fix: HARD fail.
run_checker "$LADDERPROSE_REPO"
if [ "$RC" -ne 0 ]; then
  pass "TC-133: 'Ladder remaining:' prose that disagrees with remaining_ladder HARD-fails"
else
  fail "TC-133 RECURRENCE: the board claims 'Ladder remaining: 40 and 41 alone' while remaining_ladder is [40], and the gate exited 0; out: $OUT"
fi
if printf '%s' "$OUT" | grep -q 'remaining_ladder'; then
  pass "TC-133: the failure names remaining_ladder as the contradicted field"
else
  fail "expected the failure to name remaining_ladder; got: $OUT"
fi
if printf '%s' "$OUT" | grep -q 'Slice 5'; then
  fail "Slice 5's row is current and must not be flagged — this arm's red must be the prose claim alone; out: $OUT"
else
  pass "TC-133 anti-vacuity: the current ladder row is not flagged by the prose arm"
fi

# --- Arm 13b (TC-133 false-positive guard): a comma-separated, FRACTIONAL ------
# `Ladder remaining: 39.5, 40` claim that AGREES with remaining_ladder must
# exit 0. The first draft of the claim reader cut at the first punctuation mark
# and read this as {39.5}, hard-failing a current board — a land-blocking false
# positive, and a `40.5`->`40` truncation of exactly the class this unit ends.
run_checker "$COMMA_REPO"
if [ "$RC" -eq 0 ]; then
  pass "TC-133: a comma-separated 'Ladder remaining: 39.5, 40' claim that agrees is not flagged"
else
  fail "FALSE POSITIVE: a current board whose remaining-ladder claim is a comma-separated list (with a fractional id) was hard-failed; got rc=$RC, out: $OUT"
fi

# --- Arm 13c (TC-133 RECURRENCE, companion to 13b): the list UNDER-claims -----
# Same shape, board omits 39.5. Proves 13b's green comes from reading the whole
# list, not from the claim check having gone inert on comma-separated claims.
run_checker "$UNDERCLAIM_REPO"
if [ "$RC" -ne 0 ]; then
  pass "TC-133: a remaining-ladder claim that omits a fractional id still HARD-fails"
else
  fail "TC-133: the board claims 'Ladder remaining: 40 alone' while remaining_ladder is [39.5, 40], and the gate exited 0; out: $OUT"
fi
if printf '%s' "$OUT" | grep -q '39\.5'; then
  pass "TC-133: the claim failure names the omitted fractional id (39.5), not its truncation"
else
  fail "expected the failure to name 39.5; got: $OUT"
fi

# --- Arm 14 (TC-133 policy): state file ABSENT — visible, not silent ----------
# Live boards exist for releases that predate the single-writer state file
# (DOC-HYGIENE-2 T2a), so a hard fail there would redden the gate for unrelated
# boards. The cross-read is SKIPPED — but it must SAY SO, because a silent skip
# rebuilds the blind spot this item closes. Pre-fix there is no such line at
# all, so the message assertion is this arm's RED-first proof; the rc is 0 both
# before and after by design.
run_checker "$CURRENT_REPO"
if [ "$RC" -eq 0 ]; then
  pass "TC-133: a live board with no release-state file is not hard-failed"
else
  fail "an absent release-state file must not redden a board; got rc=$RC, out: $OUT"
fi
if printf '%s' "$OUT" | grep -q 'cross-read (TC-133) DID NOT RUN'; then
  pass "TC-133: the skipped cross-read is announced in the output, not silent"
else
  fail "TC-133 BLIND SPOT: the cross-read was skipped with no visible notice; out: $OUT"
fi

# --- Arm 15 (TC-133 policy): state file PRESENT but UNPARSEABLE — HARD fail ---
# A corrupt state file is not the same as an absent one: the release declares a
# single writer, so the gate cannot vouch for the board without reading it.
# Pre-fix: rc=0 (the file is never opened).
run_checker "$BADSTATE_REPO"
if [ "$RC" -ne 0 ]; then
  pass "TC-133: an unparseable release-state file HARD-fails (cannot vouch)"
else
  fail "TC-133: a corrupt release-state-0.8.91.json let the board pass on the SHA predicate alone; got rc=0, out: $OUT"
fi
if printf '%s' "$OUT" | grep -q 'could not be read as release state'; then
  pass "TC-133: the unparseable-state failure says what it could not read"
else
  fail "expected a failure naming the unreadable release-state file; got: $OUT"
fi

# --- Arm 16 (TC-133 regression guard, NOT a recurrence arm) -------------------
# Board and state file agree everywhere. Green before AND after the fix, so the
# cross-read cannot buy its red by failing every board that carries a state
# file. The Slice 5 row deliberately contains the words "Carried to the next
# unit." — a bare `\bnext\b` marker would false-positive here and block a land.
run_checker "$CROSSOK_REPO"
if [ "$RC" -eq 0 ]; then
  pass "TC-133 regression guard: a board that agrees with its state file still exits 0"
else
  fail "a consistent board+state pair must not be flagged; got rc=$RC, out: $OUT"
fi
if printf '%s' "$OUT" | grep -q 'board/state cross-read (TC-133)'; then
  pass "TC-133 anti-vacuity: the green run really executed the cross-read (it reports itself)"
else
  fail "the cross-read did not report itself on a passing run — it may be inert; out: $OUT"
fi

# --- Arm 17 (TC-133 RECURRENCE, fix-1): DUPLICATE slice ids in the state file --
# The board is current in every respect; the state file's ladder carries `30` and
# `30.0`. The cross-read's own map collapsed them onto one key and kept the LAST
# writer, so a landed row could be reconciled against the WRONG entry's SHA —
# while the sibling `_by_slice` in check-release-state-views.sh already refuses
# the same collision. A present-but-malformed state file is a HARD fail (only an
# ABSENT one is the announced skip of Arm 14).
# Pre-fix: rc=0 (last-writer-wins; both entries carry the same sha, so nothing
# downstream noticed and the malformed file was CERTIFIED).
run_checker "$DUPSTATE_REPO"
if [ "$RC" -ne 0 ]; then
  pass "TC-133: two ladder entries resolving to the same slice id HARD-fail the cross-read"
else
  fail "TC-133 RECURRENCE: release-state-0.8.87.json carries BOTH \`30\` and \`30.0\`, one silently overwrote the other, and the gate exited 0; out: $OUT"
fi
if printf '%s' "$OUT" | grep -q 'same slice id 30'; then
  pass "TC-133: the duplicate-id failure names the colliding slice id"
else
  fail "expected the failure to name the colliding slice id 30; got: $OUT"
fi
if printf '%s' "$OUT" | grep -q '30 and 30\.0'; then
  pass "TC-133: the duplicate-id failure quotes BOTH raw values, so the reader can find them"
else
  fail "expected the failure to quote both offending raw values (30 and 30.0); got: $OUT"
fi

# --- Arm 18 (fix-2 Finding A RECURRENCE): a landed slice whose ladder entry
# carries NO `sha` must HARD-fail, not silently disable check (c).
# Pre-fix: rc=0 — `(by_slice.get(key) or {}).get("sha")` is None and
# `if sha and sha not in status:` never runs.
run_checker "$NOSHA_REPO"
if [ "$RC" -ne 0 ]; then
  pass "fix-2 A: a slice in \`landed\` whose ladder entry has no \`sha\` HARD-fails"
else
  fail "fix-2 A RECURRENCE: release-state-0.8.86.json lists Slice 5 in \`landed\` with no \`sha\`, so the SHA-row check (c) was a NO-OP and the gate exited 0; out: $OUT"
fi
if printf '%s' "$OUT" | grep -q 'TC-133'; then
  pass "fix-2 A: the missing-sha failure is attributed to the cross-read (TC-133)"
else
  fail "expected the missing-sha failure to name TC-133; got: $OUT"
fi
if printf '%s' "$OUT" | grep -q 'Slice 5'; then
  pass "fix-2 A: the missing-sha failure names the incomplete slice (5)"
else
  fail "expected the missing-sha failure to name Slice 5; got: $OUT"
fi
# ANTI-VACUITY: every OTHER cross-read predicate must have passed on this
# fixture — the board row is current, cites the real landing SHA, and the
# remaining-ladder claim agrees. If the red came from the marker regex, the
# absent-row check or the SHA-presence predicate, this arm would prove nothing.
for wrong_reason in 'still describes it as' 'has NO row for it' 'is not referenced anywhere' 'remaining_ladder'; do
  if printf '%s' "$OUT" | grep -q "$wrong_reason"; then
    fail "fix-2 A anti-vacuity: the red came from '$wrong_reason', not from the ladder-sha completeness check; out: $OUT"
  else
    pass "fix-2 A anti-vacuity: '$wrong_reason' did not fire — the red is the ladder-sha completeness check's"
  fi
done

# --- Arm 19 (fix-2 Finding A RECURRENCE): a landed slice with NO ladder entry
# at all. Same no-op, one rung further out. Pre-fix: rc=0.
run_checker "$NOENTRY_REPO"
if [ "$RC" -ne 0 ]; then
  pass "fix-2 A: a slice in \`landed\` with no ladder entry at all HARD-fails"
else
  fail "fix-2 A RECURRENCE: release-state-0.8.85.json lists Slice 5 in \`landed\` but its ladder has no entry for it, and the gate exited 0; out: $OUT"
fi
# Deliberately NOT a bare `grep -q ladder`: the cross-read's own PASSING summary
# line contains the word "remaining_ladder", so a bare match is vacuously green
# on the unfixed checker and would prove nothing.
if printf '%s' "$OUT" | grep -q 'STALE.*Slice 5.*carries no ladder entry'; then
  pass "fix-2 A: the missing-entry failure names Slice 5 and the absent ladder entry"
else
  fail "expected a STALE line naming Slice 5 and its absent ladder entry; got: $OUT"
fi
# ANTI-VACUITY: the BOARD carries a row for Slice 5, so the pre-existing
# "has NO row for it" predicate must NOT be what makes this arm red.
if printf '%s' "$OUT" | grep -q 'has NO row for it'; then
  fail "fix-2 A anti-vacuity: the board DOES carry a row for Slice 5 — this red must be the state file's missing ladder entry; out: $OUT"
else
  pass "fix-2 A anti-vacuity: the board-row predicate did not fire; the red is the state file's"
fi

# --- Arm 20 (fix-2 Finding B RECURRENCE): `landed` present but of the WRONG
# TYPE. The checker's rc is 1 both before and after (the traceback exits 1), so
# THE RC PROVES NOTHING HERE — the RED-first proof is the MESSAGE assertion: the
# diagnostic must carry the `STALE` prefix that `preflight.sh --landing`
# promotes to a HARD failure, and pre-fix it is a bare Python traceback.
run_checker "$BADTYPE_REPO"
if [ "$RC" -ne 0 ]; then
  pass "fix-2 B: a wrong-typed \`landed\` does not let the checker exit 0"
else
  fail "a wrong-typed \`landed\` must not pass; got rc=0, out: $OUT"
fi
if printf '%s' "$OUT" | grep -q '^STALE'; then
  pass "fix-2 B: the wrong-type diagnostic carries the STALE prefix preflight promotes to HARD"
else
  fail "fix-2 B RECURRENCE: a wrong-typed \`landed\` produced an UNPREFIXED diagnostic (a raw traceback), which preflight --landing downgrades to INFO; out: $OUT"
fi
if printf '%s' "$OUT" | grep -q 'not a JSON array'; then
  pass "fix-2 B: the diagnostic says which key had which wrong type"
else
  fail "expected a diagnostic naming the wrong type (not a JSON array); got: $OUT"
fi
if printf '%s' "$OUT" | grep -q 'Traceback (most recent call last)'; then
  fail "fix-2 B: the checker still emitted a raw Python traceback for a malformed state file; out: $OUT"
else
  pass "fix-2 B: a malformed state file is reported, not tracebacked"
fi

# --- Arm 21 (fix-2 Finding B RECURRENCE): `ladder` is a list of NUMBERS. The
# QUIET half — no exception at all, the non-object entries were silently dropped
# and check (c) then went no-op via Finding A's construct.
# Pre-fix: rc=0, a malformed state file CERTIFIED.
run_checker "$BADLADDER_REPO"
if [ "$RC" -ne 0 ]; then
  pass "fix-2 B: a \`ladder\` that is not a list of objects HARD-fails"
else
  fail "fix-2 B RECURRENCE: release-state-0.8.83.json's ladder is [5, 40] (numbers); every entry was silently dropped, check (c) went no-op, and the gate CERTIFIED the malformed file with rc=0; out: $OUT"
fi
if printf '%s' "$OUT" | grep -q 'not a JSON object'; then
  pass "fix-2 B: the diagnostic names the ladder entry that is not an object"
else
  fail "expected a diagnostic naming the non-object ladder entry; got: $OUT"
fi
if printf '%s' "$OUT" | grep -q '^STALE'; then
  pass "fix-2 B: the non-object-ladder diagnostic carries the STALE prefix"
else
  fail "fix-2 B: the non-object-ladder diagnostic must carry the STALE prefix; got: $OUT"
fi

# --- Arm 22a (fix-2 Finding B, the GENERAL fail-open — CHECKER HALF) ----------
# PREMISE PIN, green before AND after: for an UNEXPECTED exception the checker
# exits non-zero with NO `STALE` line. That is the checker behaving correctly —
# it refuses to vouch — and it is precisely the input on which preflight's §7
# used to fail open. This arm exists to keep that premise true, so that Arm 22b
# below is testing what it claims to test. It is NOT a recurrence arm.
run_checker "$BOOM_REPO"
if [ "$RC" -ne 0 ]; then
  pass "fix-2 B premise: an unexpected exception in the cross-read exits the checker non-zero"
else
  fail "an unexpected exception must not let the checker exit 0; got rc=0, out: $OUT"
fi
if printf '%s' "$OUT" | grep -q '^STALE'; then
  fail "fix-2 B premise BROKEN: this fixture is supposed to produce an UNPREFIXED failure — re-choose the fixture, Arm 22b is no longer testing the general fail-open; out: $OUT"
else
  pass "fix-2 B premise: the unexpected-exception failure carries NO STALE prefix (so only the caller can save the land)"
fi

# ============================ preflight.sh --landing ============================
# These arms are the RED-first proof: against the UNMODIFIED preflight.sh they
# demonstrate the gap (stale board incorrectly clears landing); after the gate
# is wired in they demonstrate the fix.

# --- Arm 5: --landing in a stale-board linked worktree MUST hard-fail ----------
STALE_LINKED="$TMPROOT/stale-linked"
git -C "$STALE_REPO" worktree add -q -b stale-landing-fixture "$STALE_LINKED" >/dev/null 2>&1
run_preflight "$STALE_LINKED" --landing
if [ "$RC" -ne 0 ]; then
  pass "--landing HARD-fails in a worktree whose board is stale vs git ancestry"
else
  fail "--landing MUST fail on a stale board; got rc=0 (this is the incident reproduced), out: $OUT"
fi
if printf '%s' "$OUT" | grep -q 'board-currency:'; then
  pass "--landing failure output names the board-currency check"
else
  fail "expected the failure to name the board-currency check; got: $OUT"
fi

# --- Arm 6: --landing in a current-board linked worktree still passes ---------
CURRENT_LINKED="$TMPROOT/current-linked"
git -C "$CURRENT_REPO" worktree add -q -b current-landing-fixture "$CURRENT_LINKED" >/dev/null 2>&1
run_preflight "$CURRENT_LINKED" --landing
if [ "$RC" -eq 0 ]; then
  pass "--landing still exits 0 in a worktree whose board is current"
else
  fail "--landing must not regress a current board; got rc=$RC, out: $OUT"
fi
# ANTI-VACUITY for the fixture repair: the arm above must be green because every
# --landing gate RAN and passed, never because one of them was absent or inert.
# Both gates that hard-fail an incomplete fixture (T1b's ledger integrity, T1e's
# governed-surface pin) must be visible in the output as an `ok` line.
for gate_line in 'ledger-integrity:' 'governed-surface-pin:'; do
  if printf '%s' "$OUT" | grep -qE "^ok +${gate_line}"; then
    pass "the current-board --landing run really executed the $gate_line gate (ok, not skipped)"
  else
    fail "$gate_line did not report ok in a passing --landing run — the fixture may be clearing the gate by not carrying its input; out: $OUT"
  fi
done

# --- Arm 7: --landing in a CLOSED-board worktree still passes (skip, not flag) -
CLOSED_LINKED="$TMPROOT/closed-linked"
git -C "$CLOSED_REPO" worktree add -q -b closed-landing-fixture "$CLOSED_LINKED" >/dev/null 2>&1
run_preflight "$CLOSED_LINKED" --landing
if [ "$RC" -eq 0 ]; then
  pass "--landing still exits 0 in a worktree whose only board is CLOSED-banner"
else
  fail "a CLOSED-banner board must not block a land; got rc=$RC, out: $OUT"
fi

# --- Arm 8: plain preflight (no --landing) never runs the board-currency check -
run_preflight "$STALE_LINKED"
if printf '%s' "$OUT" | grep -q 'board-currency:'; then
  fail "board-currency check must be --landing-only; ran without --landing: $OUT"
else
  pass "regression guard: board-currency check is inert without --landing"
fi

# --- Arm 9b (SLICE-ID-HARDENING site 1, through the REAL entry point) ----------
# check-board-currency.sh is not usually invoked directly at land time — it runs
# INSIDE `preflight.sh --landing` (and in the always-on CI board-currency job).
# Five of six codex fix rounds across Slices 32/33 were defects in the
# verification apparatus rather than the function under test, so the site-1
# recurrence is graded through the real caller as well as directly.
FRACTIONAL_LINKED="$TMPROOT/fractional-linked"
git -C "$FRACTIONAL_REPO" worktree add -q -b fractional-landing-fixture "$FRACTIONAL_LINKED" >/dev/null 2>&1
run_preflight "$FRACTIONAL_LINKED" --landing
if [ "$RC" -ne 0 ]; then
  pass "site 1 via the real entry point: --landing HARD-fails when a fractional id masked a stale integer land"
else
  fail "site 1 RECURRENCE in preflight --landing: a board missing Slice 39's SHA cleared landing; got rc=0, out: $OUT"
fi
if printf '%s' "$OUT" | grep -q "board-currency.*Slice 39: landing commit ${FRAC_39_SHORT}"; then
  pass "--landing surfaces the swallowed integer land as a board-currency HARD fail"
else
  fail "expected a board-currency HARD line naming Slice 39 / $FRAC_39_SHORT; got: $OUT"
fi

# --- Arm 12b (TC-133, through the REAL entry point) ---------------------------
# The cross-read is not usually invoked directly at land time — it runs INSIDE
# `preflight.sh --landing` and the always-on CI board-currency job, which is
# exactly why the real incident survived: BOTH of them were green while the
# board said NOT_STARTED. Five of six codex fix rounds across Slices 32/33 were
# defects in the verification apparatus rather than the function under test, so
# the TC-133 recurrence is graded through the real caller as well as directly.
CONTRADICT_LINKED="$TMPROOT/contradicting-linked"
git -C "$CONTRADICT_REPO" worktree add -q -b contradicting-landing-fixture "$CONTRADICT_LINKED" >/dev/null 2>&1
run_preflight "$CONTRADICT_LINKED" --landing
if [ "$RC" -ne 0 ]; then
  pass "TC-133 via the real entry point: --landing HARD-fails on a board that contradicts its state file"
else
  fail "TC-133 RECURRENCE in preflight --landing: a board saying 'not started' for a LANDED slice cleared landing; got rc=0, out: $OUT"
fi
if printf '%s' "$OUT" | grep -q 'board-currency.*TC-133'; then
  pass "--landing surfaces the board/state contradiction as a board-currency HARD fail"
else
  fail "expected a board-currency HARD line naming TC-133; got: $OUT"
fi

# ============ fix-2 Finding B — THE FAIL-OPEN IN THE LANDING GATE =============
# These are the load-bearing arms of fix-2. Everything above grades the CHECKER;
# these grade the LANDING GATE, which is where the fail-open lived: §7 of
# preflight.sh promoted only `STALE*` lines to HARD and turned every other line
# into INFO, so a non-zero rc carrying no STALE line was silently DOWNGRADED and
# the tree was certified for landing.
#
# [DETERMINE] — WHERE THE GENERAL FIX BELONGS. Two candidates were weighed and
# the choice was made BY EXECUTION, not assertion:
#   (i)  catch-and-re-emit inside check-board-currency.sh's embedded Python.
#        REJECTED. Measured: it cannot cover the failures that are NOT raised
#        inside that Python. `bash scripts/check-board-currency.sh --tip nosuch`
#        exits 2 printing `check-board-currency: --tip ... does not resolve`; an
#        unknown arg exits 2 the same way; and the script runs under
#        `set -euo pipefail` after sourcing scripts/lib/board-closed.sh, so a
#        missing/broken lib, a failing `git log`, or any future bash-level check
#        aborts it with no STALE line at all. A Python-level catch leaves every
#        one of those still downgraded to INFO. It closes the INSTANCE, not the
#        CLASS codex named.
#   (ii) harden preflight.sh §7 so a NON-ZERO EXIT is HARD regardless of what
#        was printed. CHOSEN. It is the more robust invariant, and its blast
#        radius is in fact the SMALLER of the two: it is additive, it can only
#        fire on a run that is ALREADY non-zero, and it therefore cannot turn a
#        green land red. It is also not a new idiom — §8 (ledger-integrity),
#        §9 (governed-surface-pin), §10 (c1-conformance) and §11
#        (transcript-hygiene) ALL already carry exactly this anti-fail-open
#        guard. §7 was the ONE landing sub-gate missing it, because it predates
#        the idiom. So this REMOVES AN ASYMMETRY rather than inventing an
#        invariant. (CI needs nothing: the board-currency job fails on any
#        non-zero exit already; preflight was the only downgrader.)
# Arms 22b and 22c are the required execution proof: (i) a fixture that fails
# the cross-read WITH a prefixed diagnostic, and (ii) one that raises an
# unexpected exception. Both must HARD-fail `--landing`.

# --- Arm 22b (fix-2 Finding B, PREFIXED diagnostic through the real caller) ---
# A wrong-typed `landed`. Post-fix the checker emits a `STALE` line, so this
# lands as a HARD via the normal §7 path — proving the type validation's
# diagnostic really does carry a prefix preflight PROMOTES.
# ⚠ RED-FIRST PROOF: pre-fix `--landing` exits **0** here. The checker exited 1,
# every traceback line became INFO, and the land was CERTIFIED.
BADTYPE_LINKED="$TMPROOT/bad-type-landed-linked"
git -C "$BADTYPE_REPO" worktree add -q -b bad-type-landing-fixture "$BADTYPE_LINKED" >/dev/null 2>&1
run_preflight "$BADTYPE_LINKED" --landing
if [ "$RC" -ne 0 ]; then
  pass "fix-2 B: --landing HARD-fails on a wrong-typed release-state file"
else
  fail "fix-2 B RECURRENCE (THE FAIL-OPEN): check-board-currency.sh exited non-zero on a malformed state file, preflight downgraded every line to INFO, and --landing CERTIFIED the tree with rc=0; out: $OUT"
fi
if printf '%s' "$OUT" | grep -qE '^HARD +board-currency: STALE'; then
  pass "fix-2 B: the wrong-type failure arrives as a HARD board-currency line, not INFO"
else
  fail "expected a 'HARD board-currency: STALE ...' line; got: $OUT"
fi

# --- Arm 22c (fix-2 Finding B, the GENERAL fail-open through the real caller) -
# An UNEXPECTED exception — nothing this fix-round validates touches this
# fixture, and the checker still emits NO STALE line (pinned by Arm 22a). The
# land must STILL be hard-blocked. This is the arm that discriminates between
# the two [DETERMINE] loci: a Python-level catch-and-re-emit could be made to
# pass this ONE case, but only by anticipating the exception — the invariant
# under test is that an UNANTICIPATED failure blocks the land on the strength of
# the exit code alone.
# ⚠ RED-FIRST PROOF: pre-fix `--landing` exits **0** here.
BOOM_LINKED="$TMPROOT/unexpected-exception-linked"
git -C "$BOOM_REPO" worktree add -q -b boom-landing-fixture "$BOOM_LINKED" >/dev/null 2>&1
run_preflight "$BOOM_LINKED" --landing
if [ "$RC" -ne 0 ]; then
  pass "fix-2 B: --landing HARD-fails when the cross-read dies with an UNPREFIXED, unanticipated failure"
else
  fail "fix-2 B RECURRENCE (THE GENERAL FAIL-OPEN): check-board-currency.sh exited non-zero with a bare traceback and --landing certified the tree anyway with rc=0; out: $OUT"
fi
if printf '%s' "$OUT" | grep -q 'without reporting a specific'; then
  pass "fix-2 B: the anti-fail-open guard names itself (checker exited N without a specific defect)"
else
  fail "expected the anti-fail-open guard's 'without reporting a specific ...' HARD line; got: $OUT"
fi

# --- Arm 22d (fix-2 Finding B REGRESSION GUARD, NOT a recurrence arm) ---------
# The anti-fail-open guard must fire ONLY on a non-zero rc. A tree whose board
# is current still clears landing, and a tree whose board is genuinely STALE
# still fails with its OWN diagnostic rather than the generic guard line — if
# the generic line appeared there, the guard would be masking real diagnostics.
run_preflight "$CURRENT_LINKED" --landing
if [ "$RC" -eq 0 ]; then
  pass "fix-2 B regression guard: the anti-fail-open guard does not fire on a green board-currency run"
else
  fail "fix-2 B must not redden a current board; got rc=$RC, out: $OUT"
fi
run_preflight "$STALE_LINKED" --landing
if printf '%s' "$OUT" | grep -q 'without reporting a specific'; then
  fail "fix-2 B: the generic anti-fail-open line fired on a run that DID report a specific STALE defect — it is masking the real diagnostic; out: $OUT"
else
  pass "fix-2 B regression guard: a run with a real STALE diagnostic does not also emit the generic guard line"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll check-board-currency tests passed\n'
