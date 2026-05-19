#!/usr/bin/env bash
# scripts/release/assert-co-tagging.sh — REQ-048 / AC-052 sibling-package
# co-tagging assertion. Run after publish-rust-t7-cli + publish-pypi +
# publish-npm complete; verifies that all three sibling crates exist on
# crates.io at the expected versions before the release is marked done.
#
#   $1 = Axis W version (the tag's bare version)
#
# Axis E version is read from the embedder-api crate manifest at HEAD,
# matching `dev/design/release.md § Version axes`.
#
# Env override (for tests):
#   ASSERT_CO_TAGGING_REGISTRY — base URL (e.g. http://127.0.0.1:PORT)
#                                used in place of https://crates.io.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
EMB_API_MANIFEST="$REPO_ROOT/src/rust/crates/fathomdb-embedder-api/Cargo.toml"

usage() {
  cat <<'USAGE' >&2
Usage: assert-co-tagging.sh <axis-W-version>

Asserts that fathomdb, fathomdb-embedder, and fathomdb-embedder-api
all exist in the registry at the expected versions for this tag.
USAGE
}

if [ "$#" -ne 1 ]; then
  usage
  exit 2
fi

VERSION="$1"
# SemVer 2.0: MAJOR.MINOR.PATCH with optional pre-release identifier
# (e.g. 0.6.0, 0.6.0-rc.2, 0.6.0-alpha.1). Build metadata (+...) not
# accepted — we never publish with it.
if ! printf '%s' "$VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'; then
  printf 'assert-co-tagging: invalid version "%s" — expected semver MAJOR.MINOR.PATCH[-PRERELEASE]\n' \
    "$VERSION" >&2
  usage
  exit 2
fi

# Read Axis E version from the embedder-api crate manifest. Matches the
# reader in scripts/set-version.sh.
read_embedder_api_version() {
  awk '
    /^\[package\]/ { in_block = 1; next }
    /^\[/          { in_block = 0 }
    in_block && /^version[[:space:]]*=[[:space:]]*"/ {
      n = split($0, parts, "\"")
      if (n >= 3) { print parts[2] }
      exit
    }
  ' "$EMB_API_MANIFEST"
}

EMB_API_VERSION="$(read_embedder_api_version)"
if [ -z "$EMB_API_VERSION" ]; then
  printf 'assert-co-tagging: cannot read Axis E version from %s\n' \
    "$EMB_API_MANIFEST" >&2
  exit 1
fi

REGISTRY_BASE="${ASSERT_CO_TAGGING_REGISTRY:-https://crates.io}"

# Returns 0 if $expected_version appears in versions[].num for $crate;
# 1 otherwise. Curl failure is fatal (network/registry outage at the
# release-finalize step is a real signal, not a soft-skip).
assert_crate_has_version() {
  local crate="$1" expected="$2"
  local url="${REGISTRY_BASE}/api/v1/crates/${crate}"
  local body
  if ! body="$(curl --silent --show-error --fail --max-time 30 \
        -H "User-Agent: fathomdb-release-co-tagging-check (https://github.com/coreyt/fathomdb)" \
        "$url" 2>&1)"; then
    printf 'assert-co-tagging: registry query failed for %s — %s\n' \
      "$crate" "$body" >&2
    return 2
  fi
  if printf '%s' "$body" | jq -e --arg v "$expected" \
       '.versions | map(.num) | index($v) != null' >/dev/null 2>&1; then
    return 0
  fi
  printf 'co-tagging-violation: %s %s not in registry\n' \
    "$crate" "$expected" >&2
  return 1
}

rc=0
assert_crate_has_version fathomdb              "$VERSION"         || rc=1
assert_crate_has_version fathomdb-embedder     "$VERSION"         || rc=1
assert_crate_has_version fathomdb-embedder-api "$EMB_API_VERSION" || rc=1

if [ "$rc" -eq 0 ]; then
  printf 'assert-co-tagging: ok — axis-W=%s, axis-E=%s present in registry\n' \
    "$VERSION" "$EMB_API_VERSION"
fi
exit "$rc"
