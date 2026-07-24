#!/usr/bin/env bash
# scripts/tests/test_plans_status_frontmatter.sh — T3/8+9 recurrence guard.
#
# Proves scripts/lint-plans-status.sh:
#   1. FAILS a dev/plans/*.md file with no YAML frontmatter at all.
#   2. FAILS a dev/plans/*.md file with frontmatter but no `status:` key.
#   3. FAILS a dev/plans/*.md file whose `status:` value is outside the
#      allowed set (ACTIVE | COMPLETE | PROPOSED | SUPERSEDED | UNKNOWN).
#   4. PASSES a dev/plans/*.md file with a valid `status:` value.
#   4b. PASSES a dev/plans/*.md file with status: UNKNOWN (T3 fix-1 — the
#       honest "cannot be sourced" value; not a bypass, still requires the key).
#   5. Does NOT scan dev/plans/runs/** or dev/plans/prompts/** (scope guard —
#      a bad file planted there must not fail the gate).
#   6. The rule is TOTAL — no filename-based exception (T3 fix-1 removed the
#      dev/plans/plan-0.8.20.md carve-out; a same-named fixture file with no
#      frontmatter must still fail).
#
# Builds a throwaway fixture repo under mktemp -d; never touches this
# checkout's real dev/plans/.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

FIX="$(mktemp -d)"
trap 'rm -rf "$FIX"' EXIT

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

setup_fixture() {
  rm -rf "$FIX/repo"
  mkdir -p "$FIX/repo/scripts" "$FIX/repo/dev/plans/runs" "$FIX/repo/dev/plans/prompts"
  cp "$REPO_ROOT/scripts/lint-plans-status.sh" "$FIX/repo/scripts/lint-plans-status.sh"
  chmod +x "$FIX/repo/scripts/lint-plans-status.sh"
  (cd "$FIX/repo" && git init -q && git config user.email t@example.com && git config user.name t)
}

run_lint() {
  set +e
  OUT="$(cd "$FIX/repo" && bash scripts/lint-plans-status.sh 2>&1)"
  RC=$?
  set -e
}

# --- Arm 1: no frontmatter at all -> FAIL -------------------------------
setup_fixture
cat >"$FIX/repo/dev/plans/no-frontmatter.md" <<'EOF'
# A plan with no frontmatter

Body text.
EOF
run_lint
if [ "$RC" -ne 0 ] && grep -q "no-frontmatter.md" <<<"$OUT" && grep -qi "frontmatter" <<<"$OUT"; then
  pass "missing frontmatter -> non-zero exit + names the offending file"
else
  fail "missing frontmatter case: rc=$RC out=$OUT"
fi

# --- Arm 2: frontmatter present, no status: key -> FAIL -----------------
setup_fixture
cat >"$FIX/repo/dev/plans/no-status-key.md" <<'EOF'
---
title: Some plan
date: 2026-07-01
---

# Some plan
EOF
run_lint
if [ "$RC" -ne 0 ] && grep -q "no-status-key.md" <<<"$OUT" && grep -qi "status" <<<"$OUT"; then
  pass "frontmatter without status: key -> non-zero exit"
else
  fail "missing status key case: rc=$RC out=$OUT"
fi

# --- Arm 3: invalid status value -> FAIL ---------------------------------
setup_fixture
cat >"$FIX/repo/dev/plans/bad-status.md" <<'EOF'
---
title: Some plan
status: DONE
---

# Some plan
EOF
run_lint
if [ "$RC" -ne 0 ] && grep -q "bad-status.md" <<<"$OUT" && grep -qi "DONE" <<<"$OUT"; then
  pass "invalid status value (DONE) -> non-zero exit, names the bad value"
else
  fail "invalid status value case: rc=$RC out=$OUT"
fi

# --- Arm 4: valid status value -> PASS -----------------------------------
setup_fixture
cat >"$FIX/repo/dev/plans/good-status.md" <<'EOF'
---
title: Some plan
status: ACTIVE
---

# Some plan
EOF
run_lint
if [ "$RC" -eq 0 ]; then
  pass "valid status value (ACTIVE) -> exit 0"
else
  fail "valid status value case unexpectedly failed: rc=$RC out=$OUT"
fi

# --- Arm 5: scope guard — bad files under runs/ and prompts/ are ignored -
setup_fixture
cat >"$FIX/repo/dev/plans/good-status.md" <<'EOF'
---
status: COMPLETE
---
# ok
EOF
cat >"$FIX/repo/dev/plans/runs/STATUS-bad.md" <<'EOF'
# no frontmatter, but this is under runs/ — must NOT be scanned
EOF
cat >"$FIX/repo/dev/plans/prompts/some-prompt.md" <<'EOF'
# no frontmatter, but this is under prompts/ — must NOT be scanned
EOF
run_lint
if [ "$RC" -eq 0 ]; then
  pass "runs/ and prompts/ are out of scope — a bad file there does not fail the gate"
else
  fail "scope guard broken: runs/ or prompts/ leaked into the scan (rc=$RC out=$OUT)"
fi

# --- Arm 4b: status: UNKNOWN is accepted (T3 fix-1) ----------------------
setup_fixture
cat >"$FIX/repo/dev/plans/unknown-status.md" <<'EOF'
---
title: Some plan whose true status could not be sourced
status: UNKNOWN
---

# Some plan
EOF
run_lint
if [ "$RC" -eq 0 ]; then
  pass "status: UNKNOWN -> exit 0 (honest 'cannot be sourced', not a bypass)"
else
  fail "status: UNKNOWN unexpectedly failed: rc=$RC out=$OUT"
fi

# --- Arm 6: no filename-based exception (T3 fix-1 dropped the             -
# plan-0.8.20.md carve-out) — a same-named fixture file with no frontmatter
# must still fail, proving the rule is total, not name-keyed.
setup_fixture
cat >"$FIX/repo/dev/plans/plan-0.8.20.md" <<'EOF'
# no frontmatter — must fail, there is no carve-out for this filename anymore
EOF
run_lint
if [ "$RC" -ne 0 ] && grep -q "plan-0.8.20.md" <<<"$OUT"; then
  pass "no filename-based exception — plan-0.8.20.md is scanned like any other plan"
else
  fail "unexpected exception resurfaced for plan-0.8.20.md: rc=$RC out=$OUT"
fi

if [ "$FAILED" -ne 0 ]; then
  printf '%d arm(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf 'PASS  all arms — scripts/lint-plans-status.sh catches missing/invalid status, accepts UNKNOWN, stays scoped to dev/plans/*.md, and carries no filename exceptions\n'
