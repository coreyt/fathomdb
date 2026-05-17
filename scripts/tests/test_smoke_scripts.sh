#!/usr/bin/env bash
# scripts/tests/test_smoke_scripts.sh — STRUCTURAL coverage for the three
# post-publish smoke scripts under scripts/release/smoke/.
#
# WHY this is structural (no shelled-out integration):
#   The smoke scripts install from real registries (crates.io / PyPI /
#   npm). Running them in unit tests would require network, the published
#   artifact already existing at the test version, and tens of seconds of
#   wall time per script — all flaky in CI. Their behavior is exercised
#   in production at tag time by the release workflow's post-publish-smoke
#   job. What we CAN test cheaply is that the script body has the
#   contract-shape we depend on:
#     * hardened bash (`set -euo pipefail`).
#     * version regex check on $1 BEFORE any registry call.
#     * mktemp -d + EXIT trap for cleanup (no leaked work dirs on failure).
#     * version-pinned install command (the published version is the
#       version under test, not "latest" or "*").
#   These are the structural invariants that, if broken, would cause the
#   real smoke job to install the wrong artifact or leak files.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
SMOKE_DIR="$REPO_ROOT/scripts/release/smoke"

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

# Asserts a string is present in the file body; fails with the literal
# needle in the diagnostic so debugging is one grep away.
assert_contains() {
  local label="$1" file="$2" needle="$3"
  if grep -qF -- "$needle" "$file"; then
    pass "$label"
  else
    fail "$label (missing literal: $needle)"
  fi
}

assert_matches() {
  local label="$1" file="$2" pattern="$3"
  if grep -qE -- "$pattern" "$file"; then
    pass "$label"
  else
    fail "$label (no match for pattern: $pattern)"
  fi
}

check_common() {
  local script="$1" label_prefix="$2"
  [ -x "$script" ] || fail "$label_prefix: not executable"
  # Shebang.
  if head -1 "$script" | grep -qE '^#!.*bash'; then
    pass "$label_prefix: bash shebang"
  else
    fail "$label_prefix: missing bash shebang"
  fi
  assert_contains "$label_prefix: set -euo pipefail" "$script" 'set -euo pipefail'
  # Version regex — semver MAJOR.MINOR.PATCH guard on $1.
  assert_matches  "$label_prefix: version regex guard on \$1" "$script" \
    '\^\[0-9\]\+\\\.\[0-9\]\+\\\.\[0-9\]\+\$'
  assert_contains "$label_prefix: mktemp -d for fixture dir" "$script" 'mktemp -d'
  assert_contains "$label_prefix: EXIT trap cleanup" "$script" "trap 'rm -rf"
}

CRATES="$SMOKE_DIR/smoke-crates-cli.sh"
PYPI="$SMOKE_DIR/smoke-pypi-wheel.sh"
NPM="$SMOKE_DIR/smoke-npm-package.sh"

check_common "$CRATES" "smoke-crates-cli"
# Version-pinned cargo install (--version "$VERSION", not "latest").
assert_contains "smoke-crates-cli: pinned cargo install" "$CRATES" \
  'cargo install fathomdb-cli --version "$VERSION"'
# JSON parses post-run.
assert_contains "smoke-crates-cli: jq parses check-integrity output" "$CRATES" \
  'jq -e . >/dev/null'

check_common "$PYPI" "smoke-pypi-wheel"
assert_contains "smoke-pypi-wheel: fresh venv" "$PYPI" 'python3 -m venv'
assert_contains "smoke-pypi-wheel: pinned pip install" "$PYPI" \
  'pip install --quiet "fathomdb==${VERSION}"'
assert_contains "smoke-pypi-wheel: open/close exercise" "$PYPI" 'Engine.open'
assert_contains "smoke-pypi-wheel: close call" "$PYPI" 'e.close()'

check_common "$NPM" "smoke-npm-package"
assert_contains "smoke-npm-package: fresh npm init" "$NPM" 'npm init -y'
assert_contains "smoke-npm-package: pinned npm install" "$NPM" \
  'npm install --silent "fathomdb@${VERSION}"'
assert_contains "smoke-npm-package: Engine.open exercise" "$NPM" 'Engine.open'
assert_contains "smoke-npm-package: close call" "$NPM" 'await e.close()'

# Run each smoke with a bad version arg — must exit non-zero BEFORE doing
# any network work, with a usage-shaped diagnostic.
for s in "$CRATES" "$PYPI" "$NPM"; do
  name="$(basename "$s")"
  if out="$("$s" not-a-semver 2>&1)"; then
    fail "$name: non-semver version should be rejected"
  else
    if printf '%s' "$out" | grep -qiE 'invalid|usage|version'; then
      pass "$name: non-semver rejected pre-install"
    else
      fail "$name: wrong diagnostic for non-semver; got: $out"
    fi
  fi
  if out="$("$s" 2>&1)"; then
    fail "$name: missing arg should fail"
  else
    if printf '%s' "$out" | grep -qi usage; then
      pass "$name: missing arg → usage"
    else
      fail "$name: wrong diagnostic for missing arg; got: $out"
    fi
  fi
done

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll smoke-script structural tests passed\n'
