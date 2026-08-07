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

assert_next_line() {
  local label="$1" file="$2" command="$3" expected="$4"
  if awk -v command="$command" -v expected="$expected" '
    $0 == command { getline; if ($0 == expected) { found = 1 } }
    END { exit(found ? 0 : 1) }
  ' "$file"; then
    pass "$label"
  else
    fail "$label (native command is not immediately followed by: $expected)"
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
  # Version regex — SemVer 2.0 (MAJOR.MINOR.PATCH with optional
  # pre-release identifier) guard on $1.
  assert_matches  "$label_prefix: version regex guard on \$1" "$script" \
    '\^\[0-9\]\+\\\.\[0-9\]\+\\\.\[0-9\]\+\(-\[0-9A-Za-z\.-\]\+\)\?\$'
  assert_contains "$label_prefix: mktemp -d for fixture dir" "$script" 'mktemp -d'
  assert_contains "$label_prefix: EXIT trap cleanup" "$script" "trap 'rm -rf"
}

CRATES="$SMOKE_DIR/smoke-crates-cli.sh"
PYPI="$SMOKE_DIR/smoke-pypi-wheel.sh"
NPM="$SMOKE_DIR/smoke-npm-package.sh"
PYPI_WINDOWS="$SMOKE_DIR/smoke-pypi-wheel.ps1"
NPM_WINDOWS="$SMOKE_DIR/smoke-npm-package.ps1"

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
  'pip install --quiet "fathomdb==${PIP_VERSION}"'
# PEP 440 normalization helper present (SemVer -rc.N -> PEP 440 rcN).
assert_contains "smoke-pypi-wheel: PEP 440 normalization" "$PYPI" \
  'pep440_normalize()'
assert_contains "smoke-pypi-wheel: open/close exercise" "$PYPI" 'Engine.open'
assert_contains "smoke-pypi-wheel: close call" "$PYPI" 'e.close()'
# Canonical writes require provenance. Keep this structural assertion alongside
# the real-registry smoke contract so a future edit cannot reintroduce a
# publish-only WriteValidationError.
assert_contains "smoke-pypi-wheel: write carries source_id" "$PYPI" \
  '"source_id": "smoke:pypi-wheel"'

check_common "$NPM" "smoke-npm-package"
assert_contains "smoke-npm-package: fresh npm init" "$NPM" 'npm init -y'
assert_contains "smoke-npm-package: pinned npm install" "$NPM" \
  'npm install --silent "fathomdb@${VERSION}"'
assert_contains "smoke-npm-package: Engine.open exercise" "$NPM" 'Engine.open'
assert_contains "smoke-npm-package: close call" "$NPM" 'await e.close()'
# TypeScript accepts camelCase at the N-API boundary; the opaque id is the
# smoke fixture's provenance, not caller content.
assert_contains "smoke-npm-package: write carries sourceId" "$NPM" \
  'sourceId: "smoke:npm-package"'

# Windows uses PowerShell rather than assuming a Unix shell. Its scripts retain
# the same registry-pinned open/write/search/close contract as the bash legs.
assert_contains "windows PyPI smoke: creates a fresh venv" "$PYPI_WINDOWS" 'python -m venv'
assert_contains "windows PyPI smoke: pinned install" "$PYPI_WINDOWS" '"fathomdb==$Version"'
assert_contains "windows PyPI smoke: closes engine" "$PYPI_WINDOWS" 'engine.close()'
assert_contains "windows npm smoke: pinned install" "$NPM_WINDOWS" '"fathomdb@$Version"'
assert_contains "windows npm smoke: closes engine" "$NPM_WINDOWS" 'await engine.close()'

# `$ErrorActionPreference` does not reliably turn a nonzero native-process
# exit into a terminating PowerShell error.  Every native command therefore
# needs an adjacent `$LASTEXITCODE` guard, so a later successful command
# cannot mask an install or runtime failure.
native_exit_guard='  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }'
assert_next_line "windows PyPI smoke: venv propagates native failure" "$PYPI_WINDOWS" \
  '  python -m venv "$work/venv"' "$native_exit_guard"
assert_next_line "windows PyPI smoke: pip upgrade propagates native failure" "$PYPI_WINDOWS" \
  '  & $python -m pip install --quiet --upgrade pip' "$native_exit_guard"
assert_next_line "windows PyPI smoke: wheel install propagates native failure" "$PYPI_WINDOWS" \
  '  & $python -m pip install --quiet "fathomdb==$Version"' "$native_exit_guard"
assert_next_line "windows PyPI smoke: SDK exercise propagates native failure" "$PYPI_WINDOWS" \
  "'@ | & \$python - \$db" "$native_exit_guard"
assert_next_line "windows npm smoke: npm init propagates native failure" "$NPM_WINDOWS" \
  '  npm init -y | Out-Null' "$native_exit_guard"
assert_next_line "windows npm smoke: npm install propagates native failure" "$NPM_WINDOWS" \
  '  npm install --silent "fathomdb@$Version"' "$native_exit_guard"
assert_next_line "windows npm smoke: Node exercise propagates native failure" "$NPM_WINDOWS" \
  "  node smoke.mjs (Join-Path \$work 'smoke.fdb')" "$native_exit_guard"

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
