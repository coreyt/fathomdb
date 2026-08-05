#!/usr/bin/env bash
# Regression coverage for the tracked current-release resolver.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
RESOLVER="${RESOLVER_UNDER_TEST:-$REPO_ROOT/scripts/release-current.py}"

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

TMPROOT="$(mktemp -d)"
cleanup() { rm -rf "$TMPROOT"; }
trap cleanup EXIT

make_repo() {
  local dir="$1"
  mkdir -p "$dir/dev/plans/runs"
  git init -q -b main "$dir"
  git -C "$dir" config user.email resolver-test@example.invalid
  git -C "$dir" config user.name resolver-test
  git -C "$dir" config commit.gpgsign false
}

write_pair() {
  local dir="$1" ver="$2" closed="$3" board="dev/plans/runs/STATUS-$2.md"
  printf '# %s\n%s\n' "$ver" "$closed" >"$dir/$board"
  cat >"$dir/dev/plans/release-state-$ver.json" <<EOF
{"release":"$ver","board":"$board"}
EOF
}

run() {
  local dir="$1"
  set +e
  OUT="$(cd "$dir" && "$RESOLVER" 2>&1)"
  RC=$?
  set -e
}

# Baseline: closed history plus exactly one live, linked state/board pair.
FIX="$TMPROOT/baseline"
make_repo "$FIX"
write_pair "$FIX" 0.8.20 'CLOSED — historical record'
write_pair "$FIX" 0.8.21 'LIVE'
(cd "$FIX" && git add -A && git commit -qm fixture)
run "$FIX"
if [ "$RC" -eq 0 ] && [ "$OUT" = $'0.8.21\tdev/plans/runs/STATUS-0.8.21.md\tdev/plans/release-state-0.8.21.json' ]; then
  pass 'one tracked live board and symmetric state link resolve deterministically'
else
  fail "baseline: rc=$RC out=$OUT"
fi

# Discovery is deliberately tracked-only: an untracked stale worktree-like
# document must not create a second current release.
mkdir -p "$FIX/.claude/worktrees/stale/dev/plans/runs"
printf '# stale\nLIVE\n' >"$FIX/.claude/worktrees/stale/dev/plans/runs/STATUS-9.9.9.md"
run "$FIX"
if [ "$RC" -eq 0 ] && [[ "$OUT" == 0.8.21$'\t'* ]]; then
  pass 'untracked nested worktree board is ignored'
else
  fail "untracked nested board: rc=$RC out=$OUT"
fi

# Published state is authoritative closure for an older retained board that
# predates the CLOSED-banner convention.
python3 - "$FIX/dev/plans/release-state-0.8.20.json" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
d['published'] = {'tag': 'v0.8.20'}
open(p, 'w').write(json.dumps(d))
PY
printf '# retained 0.8.20 board without a CLOSED banner\nLIVE\n' \
  >"$FIX/dev/plans/runs/STATUS-0.8.20.md"
run "$FIX"
if [ "$RC" -eq 0 ] && [[ "$OUT" == 0.8.21$'\t'* ]]; then
  pass 'published state retires a retained board without a CLOSED banner'
else
  fail "published state: rc=$RC out=$OUT"
fi

ZERO="$TMPROOT/zero"
make_repo "$ZERO"
write_pair "$ZERO" 0.8.20 'CLOSED — historical record'
(cd "$ZERO" && git add -A && git commit -qm fixture)
run "$ZERO"
if [ "$RC" -ne 0 ] && grep -qi 'exactly one.*live' <<<"$OUT"; then
  pass 'zero live releases hard-fails'
else
  fail "zero: rc=$RC out=$OUT"
fi

# A fully published schedule has no active release by design. That is a valid
# terminal state, distinct from an accidentally closed but unpublished board.
COMPLETE="$TMPROOT/complete"
make_repo "$COMPLETE"
write_pair "$COMPLETE" 0.8.21 'CLOSED — historical record'
python3 - "$COMPLETE/dev/plans/release-state-0.8.21.json" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
d['published'] = {'tag': 'v0.8.21'}
open(p, 'w').write(json.dumps(d))
PY
(cd "$COMPLETE" && git add -A && git commit -qm fixture)
run "$COMPLETE"
if [ "$RC" -eq 0 ] && [ -z "$OUT" ]; then
  pass 'all-published schedule resolves successfully with no active release tuple'
else
  fail "complete: rc=$RC out=$OUT"
fi

MULTI="$TMPROOT/multiple"
make_repo "$MULTI"
write_pair "$MULTI" 0.8.20 LIVE
write_pair "$MULTI" 0.8.21 LIVE
(cd "$MULTI" && git add -A && git commit -qm fixture)
run "$MULTI"
if [ "$RC" -ne 0 ] && grep -qi 'exactly one.*live' <<<"$OUT"; then
  pass 'multiple live releases hard-fail rather than choosing a highest version'
else
  fail "multiple: rc=$RC out=$OUT"
fi

BAD="$TMPROOT/inconsistent"
make_repo "$BAD"
write_pair "$BAD" 0.8.21 LIVE
python3 - "$BAD/dev/plans/release-state-0.8.21.json" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
d['board'] = 'dev/plans/runs/STATUS-other.md'
open(p, 'w').write(json.dumps(d))
PY
(cd "$BAD" && git add -A && git commit -qm fixture)
run "$BAD"
if [ "$RC" -ne 0 ] && grep -qi 'inconsistent' <<<"$OUT"; then
  pass 'state filename/release/board link inconsistency hard-fails'
else
  fail "inconsistent: rc=$RC out=$OUT"
fi

if [ "$FAILED" -ne 0 ]; then
  printf '\n%d release-current test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll release-current tests passed\n'
