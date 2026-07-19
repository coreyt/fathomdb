#!/usr/bin/env bash
# scripts/tests/test_preflight_landing.sh — coverage for `preflight.sh --landing`.
#
# TC-RUBRIC-5 (HITL-ADOPTED 2026-07-11) requires that release orchestration and
# ALL landing git-writes run in a dedicated linked worktree, never in the primary
# checkout. `--landing` is the mechanical enforcement of that rule, per the
# standing guardrail-failures-fix-tooling-not-people principle.
#
# Detection under test: primary checkout <=> `git rev-parse --git-dir` equals
# `git rev-parse --git-common-dir`, compared AFTER resolving both to absolute,
# symlink-resolved paths.
#
# On the canonicalization: on git 2.43 invoked from a SUBDIRECTORY, --git-dir
# returns an absolute path while --git-common-dir returns a relative one
# ('../.git'). preflight.sh cds to the repo toplevel before the check, which
# normalizes that away today — so a raw string compare currently behaves the same
# and the subdir arms below pass either way. The canonicalization still matters:
# measured, with that toplevel cd removed, a RAW compare from a subdirectory of
# the primary checkout fails OPEN (exit 0, "cleared for landing") while the
# canonicalized compare still HARD-fails. Since no behavioral arm can distinguish
# the two while the cd stands, Arm 7 asserts the canonicalization structurally —
# same idiom as the 'User-Agent:' grep in test_assert_co_tagging.sh.
#
# Isolation: every arm runs against a throwaway repo built under mktemp -d. The
# test never git-writes into the real checkout and does not care whether the
# developer's tree is clean.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PREFLIGHT="$REPO_ROOT/scripts/preflight.sh"

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

TMPROOT="$(mktemp -d)"
cleanup() {
  # Only ever remove a path we created under the system temp dir.
  case "$TMPROOT" in
    "${TMPDIR:-/tmp}"/*|/tmp/*) rm -rf "$TMPROOT" ;;
    *) printf 'refusing to remove unexpected temp path: %s\n' "$TMPROOT" >&2 ;;
  esac
}
trap cleanup EXIT

# --- Fixture: a throwaway primary checkout + a linked worktree inside it -------
# preflight.sh does `git rev-parse main` under `set -euo pipefail`, so the fixture
# repo must actually have a `main` branch with a commit.
#
# The fixture must not inherit the developer's/CI image's global git config: a
# global `commit.gpgsign = true` makes the fixture commit try to GPG-sign with the
# throwaway identity and abort (measured: exit 128, "failed to write commit
# object"), and a global `core.hooksPath` — or an `init.templateDir` that seeds
# .git/hooks — runs a foreign pre-commit hook that can veto it (measured: exit 1).
# Either aborts the script before a single assertion runs, so an unrelated
# user-local setting would fail agent-test.sh. All three are neutralized in the
# fixture repo's LOCAL config; the real checkout's config is never touched.
# Arm 8 is the regression guard. A linked worktree shares the common config file,
# so the local settings cover it too — no separate treatment needed.
NO_HOOKS="$TMPROOT/no-hooks"
mkdir -p "$NO_HOOKS"

# make_fixture <primary-dir> <linked-dir> — builds a self-contained repo + worktree.
make_fixture() {
  local primary="$1" linked="$2"
  mkdir -p "$primary"
  git init -q -b main "$primary"
  git -C "$primary" config user.email preflight-test@example.invalid
  git -C "$primary" config user.name 'Preflight Test'
  git -C "$primary" config commit.gpgsign false
  git -C "$primary" config core.hooksPath "$NO_HOOKS"
  mkdir -p "$primary/src" "$primary/scripts"
  printf 'fixture\n' >"$primary/src/keep.txt"
  git -C "$primary" add -A
  git -C "$primary" commit -q -m 'fixture: initial commit'
  git -C "$primary" worktree add -q -b landing-fixture "$linked" >/dev/null 2>&1
}

PRIMARY="$TMPROOT/primary"
LINKED="$TMPROOT/linked"
make_fixture "$PRIMARY" "$LINKED"

# run_preflight <cwd> [args...] -> sets RC and OUT (stdout+stderr merged)
run_preflight() {
  local cwd="$1"; shift
  set +e
  OUT="$(cd "$cwd" && bash "$PREFLIGHT" "$@" 2>&1)"
  RC=$?
  set -e
}

# --- Arm 1: --landing in a linked worktree => exit 0, JSON reports pass --------
run_preflight "$LINKED" --landing
if [ "$RC" -eq 0 ]; then
  pass "--landing in a linked worktree exits 0"
else
  fail "--landing in a linked worktree should exit 0; got rc=$RC, out: $OUT"
fi
if printf '%s' "$OUT" | grep -q '"preflight":"pass"'; then
  pass "--landing in a linked worktree reports \"preflight\":\"pass\""
else
  fail "expected \"preflight\":\"pass\" in summary; got: $OUT"
fi

# --- Arm 2: --landing in the primary checkout => HARD fail, non-zero ----------
run_preflight "$PRIMARY" --landing
if [ "$RC" -ne 0 ]; then
  pass "--landing in the primary checkout exits non-zero"
else
  fail "--landing in the primary checkout MUST fail; got rc=0, out: $OUT"
fi
if printf '%s' "$OUT" | grep -q '^HARD .*TC-RUBRIC-5'; then
  pass "--landing in the primary checkout emits a HARD line naming TC-RUBRIC-5"
else
  fail "expected a HARD line naming TC-RUBRIC-5; got: $OUT"
fi
if printf '%s' "$OUT" | grep -q '"preflight":"fail"'; then
  pass "--landing in the primary checkout reports \"preflight\":\"fail\""
else
  fail "expected \"preflight\":\"fail\" in summary; got: $OUT"
fi

# --- Arm 3: regression guard — no --landing, primary checkout still passes -----
run_preflight "$PRIMARY"
if [ "$RC" -eq 0 ]; then
  pass "regression guard: primary checkout without --landing still exits 0"
else
  fail "plain invocation on the primary checkout must still pass; got rc=$RC, out: $OUT"
fi
if printf '%s' "$OUT" | grep -q 'TC-RUBRIC-5'; then
  fail "plain invocation must not mention TC-RUBRIC-5; got: $OUT"
else
  pass "regression guard: plain invocation says nothing about TC-RUBRIC-5"
fi

# --- Arm 4: invoked from a SUBDIRECTORY of each tree --------------------------
# Guards the cwd-independence of the check as a whole (the toplevel cd plus the
# canonicalization). See the header for why this arm cannot, on its own,
# distinguish a raw compare from a canonicalized one.
run_preflight "$PRIMARY/scripts" --landing
if [ "$RC" -ne 0 ]; then
  pass "--landing from a subdir of the primary checkout still fails (canonicalized)"
else
  fail "subdir of primary MUST still fail (relative vs absolute git-dir); got rc=0, out: $OUT"
fi

mkdir -p "$LINKED/scripts"
run_preflight "$LINKED/scripts" --landing
if [ "$RC" -eq 0 ]; then
  pass "--landing from a subdir of a linked worktree still passes"
else
  fail "subdir of a linked worktree should pass; got rc=$RC, out: $OUT"
fi

# --- Arm 5: --landing composes with the existing flags ------------------------
run_preflight "$LINKED" --landing --worktree "$LINKED" --min-disk-gb 1
if [ "$RC" -eq 0 ]; then
  pass "--landing composes with --worktree/--min-disk-gb in a linked worktree"
else
  fail "--landing + --worktree should pass in a linked worktree; got rc=$RC, out: $OUT"
fi

run_preflight "$PRIMARY" --worktree "$LINKED" --landing --min-disk-gb 1
if [ "$RC" -ne 0 ]; then
  pass "--landing composes with --worktree and still fails on the primary checkout"
else
  fail "--landing must fail on primary even alongside --worktree; got rc=0, out: $OUT"
fi

# --- Arm 6: usage header documents --landing ----------------------------------
if grep -q -- '--landing' "$PREFLIGHT"; then
  pass "preflight.sh documents/handles --landing"
else
  fail "preflight.sh has no --landing handling"
fi

# --- Arm 7: structural — the landing compare is canonicalized, not raw --------
# Regression guard for a fail-open no behavioral arm can currently reach: if the
# comparands are ever reverted to raw `git rev-parse` output, removing or moving
# the toplevel cd silently clears the primary checkout for landing.
if grep -qE 'GITDIR_ABS="\$\(abs_dir ' "$PREFLIGHT" \
  && grep -qE 'COMMONDIR_ABS="\$\(abs_dir ' "$PREFLIGHT"; then
  pass "landing check compares canonicalized paths (abs_dir), not raw git output"
else
  fail "landing check must canonicalize both git-dir and git-common-dir via abs_dir"
fi

if grep -qE '^\s*GITDIR_ABS="\$\(git rev-parse' "$PREFLIGHT" \
  || grep -qE '^\s*COMMONDIR_ABS="\$\(git rev-parse' "$PREFLIGHT"; then
  fail "landing check compares RAW git rev-parse output — fails open without the toplevel cd"
else
  pass "landing check does not compare raw git rev-parse output"
fi

# --- Arm 8: the fixture survives a hostile inherited global git config --------
# Regression guard for the three neutralizations above. Reproduces the exact
# conditions measured to abort the fixture before any assertion: forced commit
# signing (exit 128) and a foreign pre-commit hook via core.hooksPath (exit 1).
# init.templateDir is covered by the same core.hooksPath override, which takes
# precedence over the .git/hooks the template seeds.
HOSTILE_HOME="$TMPROOT/hostile"
mkdir -p "$HOSTILE_HOME/hooks"
printf '#!/bin/sh\nexit 1\n' >"$HOSTILE_HOME/hooks/pre-commit"
chmod +x "$HOSTILE_HOME/hooks/pre-commit"
{
  printf '[commit]\n\tgpgsign = true\n'
  printf '[tag]\n\tgpgsign = true\n'
  printf '[core]\n\thooksPath = %s\n' "$HOSTILE_HOME/hooks"
  printf '[init]\n\ttemplateDir = %s\n' "$HOSTILE_HOME"
} >"$HOSTILE_HOME/gitconfig"

set +e
(
  # Re-arm errexit inside the subshell: the `set +e` above is inherited, and
  # without this the failed fixture commit would be skipped over and this arm
  # would report the status of a later command instead (measured: vacuous pass).
  set -e
  export GIT_CONFIG_GLOBAL="$HOSTILE_HOME/gitconfig"
  make_fixture "$TMPROOT/hostile-primary" "$TMPROOT/hostile-linked"
) >/dev/null 2>&1
HOSTILE_RC=$?
set -e
if [ "$HOSTILE_RC" -eq 0 ]; then
  pass "fixture builds under a global config forcing gpgsign + a vetoing hooksPath"
else
  fail "fixture must not inherit global git config; got rc=$HOSTILE_RC"
fi

# And the assertion path itself still works against that hostile-config fixture.
run_preflight "$TMPROOT/hostile-linked" --landing
if [ "$RC" -eq 0 ]; then
  pass "hostile-config fixture: --landing in the linked worktree still exits 0"
else
  fail "hostile-config fixture: linked worktree should pass; got rc=$RC, out: $OUT"
fi
run_preflight "$TMPROOT/hostile-primary" --landing
if [ "$RC" -ne 0 ]; then
  pass "hostile-config fixture: --landing in the primary checkout still fails"
else
  fail "hostile-config fixture: primary MUST fail; got rc=0, out: $OUT"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll preflight --landing tests passed\n'
