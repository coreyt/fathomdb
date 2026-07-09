#!/usr/bin/env bash
# scripts/release/wait-for-crate-version.sh — poll-for-resolvability guard for
# the tiered cargo-publish pipeline (release.yml T1..T7).
#
# Replaces the fixed `sleep 60` index-propagation heuristic. After a tier
# publishes crate X@V, the NEXT tier cannot resolve its dependency on X@V until
# the crates.io index has propagated the new version. A fixed sleep is both
# flaky (propagation occasionally exceeds 60 s) and wasteful (usually resolves
# in a few seconds). This script POLLS the registry until the exact version is
# resolvable, then returns 0 — bounded by a timeout so a genuinely stuck
# propagation fails loudly instead of hanging.
#
#   $1 = crate name
#   $2 = version (OPTIONAL) — must appear in versions[].num before we return 0.
#        If omitted, it is resolved from the crate's local Cargo.toml (Axis-E
#        inline version for fathomdb-embedder-api; workspace-inherited Axis W
#        otherwise), so each tier waits for exactly the version it just
#        published across BOTH version axes.
#
# Env:
#   WAIT_FOR_CRATE_TIMEOUT   overall budget in seconds (default 300)
#   WAIT_FOR_CRATE_INTERVAL  poll interval in seconds (default 5)
#   WAIT_FOR_CRATE_REGISTRY  base URL in place of https://crates.io (tests)
#
# Exit: 0 = version is resolvable; 1 = timed out; 2 = usage error.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ROOT_MANIFEST="$REPO_ROOT/Cargo.toml"

usage() {
  cat <<'USAGE' >&2
Usage: wait-for-crate-version.sh <crate-name> [<version>]

Polls crates.io until <crate-name>@<version> is present in the index
(bounded by WAIT_FOR_CRATE_TIMEOUT seconds), then exits 0. Exits 1 on
timeout so the pipeline fails loudly rather than resolving a stale index.
<version> defaults to the crate's local Cargo.toml version.
USAGE
}

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ] || [ -z "${1:-}" ]; then
  usage
  exit 2
fi

CRATE="$1"
VERSION="${2:-}"

# --- manifest version resolution (mirrors cargo-publish-if-new.sh) ----------
read_workspace_version() {
  awk '
    /^\[workspace\.package\]/ { in_block = 1; next }
    /^\[/                     { in_block = 0 }
    in_block && /^version[[:space:]]*=[[:space:]]*"/ {
      n = split($0, parts, "\""); if (n >= 3) { print parts[2] }; exit
    }
  ' "$ROOT_MANIFEST"
}
read_crate_version() {
  awk '
    /^\[package\]/ { in_block = 1; next }
    /^\[/          { in_block = 0 }
    in_block && /^version[[:space:]]*\.workspace[[:space:]]*=[[:space:]]*true/ { print "WORKSPACE"; exit }
    in_block && /^version[[:space:]]*=[[:space:]]*"/ {
      n = split($0, parts, "\""); if (n >= 3) { print parts[2] }; exit
    }
  ' "$1"
}

if [ -z "$VERSION" ]; then
  crate_manifest="$REPO_ROOT/src/rust/crates/$CRATE/Cargo.toml"
  if [ ! -f "$crate_manifest" ]; then
    printf 'wait-for-crate-version: manifest not found at %s\n' "$crate_manifest" >&2
    exit 2
  fi
  VERSION="$(read_crate_version "$crate_manifest")"
  if [ "$VERSION" = "WORKSPACE" ]; then
    VERSION="$(read_workspace_version)"
  fi
  if [ -z "$VERSION" ]; then
    printf 'wait-for-crate-version: cannot resolve local version for %s\n' "$CRATE" >&2
    exit 2
  fi
fi
TIMEOUT="${WAIT_FOR_CRATE_TIMEOUT:-300}"
INTERVAL="${WAIT_FOR_CRATE_INTERVAL:-5}"
BASE="${WAIT_FOR_CRATE_REGISTRY:-https://crates.io}"
URL="${BASE}/api/v1/crates/${CRATE}"

# Returns 0 if $VERSION is present in the crate's versions[].num, else 1.
# A network/registry error returns 1 (treated as "not yet resolvable" so we
# keep polling until the timeout — a transient 5xx should not abort the wait).
version_present() {
  local body
  if ! body="$(curl --silent --show-error --fail --max-time 30 \
        -H "User-Agent: fathomdb-release-poll (https://github.com/coreyt/fathomdb)" \
        "$URL" 2>/dev/null)"; then
    return 1
  fi
  printf '%s' "$body" | jq -e --arg v "$VERSION" \
    '.versions | map(.num) | index($v) != null' >/dev/null 2>&1
}

deadline=$(( $(date +%s) + TIMEOUT ))
attempt=0
while :; do
  attempt=$((attempt + 1))
  if version_present; then
    printf 'wait-for-crate-version: %s@%s resolvable after %d poll(s)\n' \
      "$CRATE" "$VERSION" "$attempt"
    exit 0
  fi
  now=$(date +%s)
  if [ "$now" -ge "$deadline" ]; then
    printf 'wait-for-crate-version: TIMEOUT — %s@%s not resolvable after %ds\n' \
      "$CRATE" "$VERSION" "$TIMEOUT" >&2
    exit 1
  fi
  sleep "$INTERVAL"
done
