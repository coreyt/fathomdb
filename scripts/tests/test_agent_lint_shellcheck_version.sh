#!/usr/bin/env bash
# scripts/tests/test_agent_lint_shellcheck_version.sh — pin-drift guard for the
# lint-shell leg added in 0.8.21 Slice 30, mirroring
# test_agent_lint_ruff_version.sh / test_agent_lint_actionlint_version.sh.
#
# WHY A PIN AT ALL: shellcheck's finding set changes between releases. An
# unpinned linter silently redefines what "green" means underneath the repo —
# not hypothetical here, pyright is unpinned at >=1.1.380 and a point release
# red-lined `main` for ~2 days. So agent-lint.sh must REJECT a shellcheck whose
# version is not the pin, and say which version it wanted and which it got.
#
# Arm 3 is the one that matters most: HOME points at the fixture, so the
# ~/.local/bin candidate that find_shellcheck_bin prefers is the fixture's fake
# — a real 0.11.0 elsewhere on the machine must not rescue the run.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
# shellcheck source=../lib/shellcheck-version.sh
. "$REPO_ROOT/scripts/lib/shellcheck-version.sh"

FIX="$(mktemp -d)"
trap 'rm -rf "$FIX"' EXIT

mkdir -p "$FIX/scripts/lib" "$FIX/bin" "$FIX/home/.local/bin"
cp "$REPO_ROOT/scripts/agent-lint.sh" "$FIX/scripts/agent-lint.sh"
cp "$REPO_ROOT/scripts/agent-lint-shell.sh" "$FIX/scripts/agent-lint-shell.sh"
cp "$REPO_ROOT/scripts/lib/agent-output.sh" "$FIX/scripts/lib/agent-output.sh"
cp "$REPO_ROOT/scripts/lib/actionlint-version.sh" "$FIX/scripts/lib/actionlint-version.sh"
cp "$REPO_ROOT/scripts/lib/shellcheck-version.sh" "$FIX/scripts/lib/shellcheck-version.sh"
printf '#!/usr/bin/env bash\nprintf "1.7.12\\n"\n' >"$FIX/bin/actionlint"
printf '#!/usr/bin/env bash\nprintf "ruff 0.15.17\\n"\n' >"$FIX/bin/ruff"
chmod +x "$FIX/scripts/agent-lint.sh" "$FIX/scripts/agent-lint-shell.sh" \
  "$FIX/bin/actionlint" "$FIX/bin/ruff"
(
  cd "$FIX"
  git init -q
)

# A fake shellcheck reporting a version that is NOT the pin. Banner shape copied
# from the real tool, so this exercises read_shellcheck_version for real.
WRONG_VERSION="0.10.0"
cat >"$FIX/home/.local/bin/shellcheck" <<EOF
#!/usr/bin/env bash
printf 'ShellCheck - shell script analysis tool\nversion: $WRONG_VERSION\n'
EOF
chmod +x "$FIX/home/.local/bin/shellcheck"

run_fixture_lint() {
  set +e
  OUT="$(cd "$FIX" && HOME="$FIX/home" PATH="$FIX/bin:/usr/bin:/bin" bash scripts/agent-lint.sh 2>&1)"
  RC=$?
  set -e
}

fail_arm() {
  printf 'FAIL  %s\n' "$1" >&2
  exit 1
}

# --- arm 1: the pin this repo enforces is the one the lint leg uses ---------
if [ "$SHELLCHECK_VERSION" != "0.11.0" ]; then
  fail_arm "the recorded pin moved to $SHELLCHECK_VERSION; update this test deliberately, do not loosen the gate"
fi
printf 'PASS  scripts/lib/shellcheck-version.sh pins shellcheck %s\n' "$SHELLCHECK_VERSION"

# --- arm 2: a WRONG version is rejected, loudly and specifically ------------
run_fixture_lint
printf '%s\n' "$OUT"
printf 'exit=%d\n' "$RC"

[ "$RC" -ne 0 ] || fail_arm "agent-lint.sh accepted shellcheck $WRONG_VERSION against the $SHELLCHECK_VERSION pin"
grep -Fq "shellcheck $SHELLCHECK_VERSION is required" <<<"$OUT" \
  || fail_arm "shellcheck version guard did not name the REQUIRED version"
grep -Fq "$WRONG_VERSION" <<<"$OUT" \
  || fail_arm "shellcheck version guard did not name the SELECTED version"
grep -Fq 'scripts/bootstrap.sh in a clean non-worktree checkout' <<<"$OUT" \
  || fail_arm "shellcheck version guard lacks bootstrap remediation"
printf 'PASS  agent-lint.sh rejects a shellcheck that differs from the pin\n'

# --- arm 3: a pinned binary in ~/.local/bin is PREFERRED over PATH ----------
# A host/CI-image shellcheck of the wrong version on PATH must not defeat a
# correctly bootstrapped machine, and must not rescue a broken one either.
cat >"$FIX/bin/shellcheck" <<'EOF'
#!/usr/bin/env bash
printf 'ShellCheck - shell script analysis tool\nversion: 0.9.0\n'
EOF
chmod +x "$FIX/bin/shellcheck"
resolved="$(cd "$FIX" && HOME="$FIX/home" PATH="$FIX/bin:/usr/bin:/bin" bash -c '
  . scripts/lib/shellcheck-version.sh; find_shellcheck_bin')"
[ "$resolved" = "$FIX/home/.local/bin/shellcheck" ] \
  || fail_arm "find_shellcheck_bin resolved '$resolved', not the ~/.local/bin candidate"
printf 'PASS  find_shellcheck_bin prefers the bootstrap-installed ~/.local/bin binary over PATH\n'

# --- arm 4: shellcheck ABSENT is a FAILURE, never a skip (TC-37) ------------
rm -f "$FIX/home/.local/bin/shellcheck" "$FIX/bin/shellcheck"
run_fixture_lint
printf '%s\n' "$OUT"
printf 'exit=%d\n' "$RC"

[ "$RC" -ne 0 ] || fail_arm "agent-lint.sh exited 0 with shellcheck genuinely unavailable (TC-37 vacuous green)"
grep -Fq 'shellcheck' <<<"$OUT" || fail_arm "missing-shellcheck message does not name the tool"
grep -Fq 'is required but not installed' <<<"$OUT" \
  || fail_arm "missing-shellcheck message does not say the tool is required"
grep -Fq 'scripts/bootstrap.sh' <<<"$OUT" \
  || fail_arm "missing-shellcheck message does not tell the operator how to fix it"
printf 'PASS  agent-lint.sh hard-fails when shellcheck is absent (no silent skip)\n'

printf 'All shellcheck version-pin tests passed\n'
