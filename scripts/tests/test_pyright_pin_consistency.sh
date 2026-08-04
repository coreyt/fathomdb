#!/usr/bin/env bash
# The declared Python extras, runtime guard, and resolved lock entry are one
# typecheck-toolchain contract.  A version is deliberately not hardcoded here:
# this test proves those independently maintained surfaces agree, then mutates
# each one to prove a stale/mismatched surface makes the check fail.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
FIX="$(mktemp -d)"
trap 'rm -rf "$FIX"' EXIT

pin_for_extra() {
  local pyproject="$1"
  local extra="$2"
  awk -v extra="$extra" '
    $0 ~ "^" extra " = \\[" { in_extra = 1 }
    in_extra && /^\]/ { exit }
    in_extra {
      if (match($0, /"pyright==[0-9][0-9.]*"/)) {
        pin = substr($0, RSTART + 10, RLENGTH - 11)
        print pin
        exit
      }
    }
  ' "$pyproject"
}

guard_version() {
  sed -n 's/^readonly PYRIGHT_VERSION="\([0-9][0-9.]*\)"$/\1/p' "$1"
}

lock_version() {
  awk '
    /^\[\[package\]\]$/ { in_pyright = 0 }
    /^name = "pyright"$/ { in_pyright = 1; next }
    in_pyright && /^version = "/ {
      value = $0
      sub(/^version = "/, "", value)
      sub(/"$/, "", value)
      print value
      exit
    }
  ' "$1"
}

require_one() {
  local label="$1"
  local value="$2"
  if [ "$(printf '%s\n' "$value" | sed '/^$/d' | wc -l)" -ne 1 ]; then
    printf 'FAIL  expected exactly one %s, found %q\n' "$label" "$value" >&2
    return 1
  fi
}

check_consistent() {
  local root="$1"
  local typecheck_pin dev_pin runtime_pin resolved_pin
  typecheck_pin="$(pin_for_extra "$root/src/python/pyproject.toml" typecheck)"
  dev_pin="$(pin_for_extra "$root/src/python/pyproject.toml" dev)"
  runtime_pin="$(guard_version "$root/scripts/agent-typecheck.sh")"
  resolved_pin="$(lock_version "$root/src/python/uv.lock")"

  require_one 'pyproject typecheck Pyright exact pin' "$typecheck_pin"
  require_one 'pyproject dev Pyright exact pin' "$dev_pin"
  require_one 'agent-typecheck runtime PYRIGHT_VERSION guard' "$runtime_pin"
  require_one 'uv.lock Pyright package version' "$resolved_pin"

  if [ "$typecheck_pin" != "$dev_pin" ] || [ "$typecheck_pin" != "$runtime_pin" ] || \
    [ "$typecheck_pin" != "$resolved_pin" ]; then
    printf 'FAIL  Pyright version surfaces disagree: typecheck=%s dev=%s runtime=%s lock=%s\n' \
      "$typecheck_pin" "$dev_pin" "$runtime_pin" "$resolved_pin" >&2
    return 1
  fi
}

check_consistent "$REPO_ROOT"

mkdir -p "$FIX/src/python" "$FIX/scripts"
cp "$REPO_ROOT/src/python/pyproject.toml" "$FIX/src/python/pyproject.toml"
cp "$REPO_ROOT/src/python/uv.lock" "$FIX/src/python/uv.lock"
cp "$REPO_ROOT/scripts/agent-typecheck.sh" "$FIX/scripts/agent-typecheck.sh"

assert_mutation_fails() {
  local label="$1"
  if check_consistent "$FIX"; then
    printf 'FAIL  mismatched %s did not fail the Pyright consistency check\n' "$label" >&2
    exit 1
  fi
  printf 'PASS  mismatched %s fails the Pyright consistency check\n' "$label"
}

sed -i '0,/pyright==[0-9][0-9.]*/s//pyright==9.9.9/' "$FIX/src/python/pyproject.toml"
assert_mutation_fails 'pyproject typecheck extra'
cp "$REPO_ROOT/src/python/pyproject.toml" "$FIX/src/python/pyproject.toml"

sed -i '/^dev = /s/pyright==[0-9][0-9.]*/pyright==9.9.9/' "$FIX/src/python/pyproject.toml"
assert_mutation_fails 'pyproject dev extra'
cp "$REPO_ROOT/src/python/pyproject.toml" "$FIX/src/python/pyproject.toml"

sed -i 's/^readonly PYRIGHT_VERSION="[0-9][0-9.]*"$/readonly PYRIGHT_VERSION="9.9.9"/' \
  "$FIX/scripts/agent-typecheck.sh"
assert_mutation_fails 'runtime guard'
cp "$REPO_ROOT/scripts/agent-typecheck.sh" "$FIX/scripts/agent-typecheck.sh"

sed -i '/^name = "pyright"$/,/^\[\[package\]\]$/s/^version = "[0-9][0-9.]*"$/version = "9.9.9"/' \
  "$FIX/src/python/uv.lock"
assert_mutation_fails 'resolved lock package'

printf 'PASS  Pyright pins agree across extras, runtime guard, and resolved lock\n'
