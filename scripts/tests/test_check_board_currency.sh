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

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll check-board-currency tests passed\n'
