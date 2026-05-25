#!/usr/bin/env bash
# scripts/release/cargo-publish-if-new.sh — idempotency guard for the
# release.yml T1..T7 cargo-publish tiers.
#
# Reads the local crate version from src/rust/crates/<crate>/Cargo.toml
# (resolving `version.workspace = true` to the root [workspace.package]),
# queries crates.io for already-published versions, and:
#
#   • if the target version is already on the registry → log
#     "already published; skipping" and exit 0 (idempotent no-op).
#   • otherwise → invoke `cargo publish -p <crate> --token "$CARGO_REGISTRY_TOKEN"`.
#
# With --dry-run, the registry check is bypassed and the helper always
# forwards to `cargo publish --dry-run -p <crate>` — dry runs exercise
# packaging regardless of registry state.
#
# Motivation: axis-W-only patch releases (e.g. 0.6.1, where axis-E
# fathomdb-embedder-api is held at 0.6.0) would otherwise fail at T1
# with "version already exists". Also covers partial-republish retries
# where an RC2 follows an RC1 that landed only some tiers.
#
# Test env overrides (NOT for production CI):
#   CARGO_PUBLISH_IF_NEW_REGISTRY      — base URL in place of https://crates.io
#   CARGO_PUBLISH_IF_NEW_LOCAL_VERSION — skip manifest read; use this version
set -euo pipefail

usage() {
  cat <<'USAGE' >&2
Usage: cargo-publish-if-new.sh [--dry-run] <crate-name>

Publishes <crate-name> to crates.io only if the local Cargo.toml
version is not already on the registry. --dry-run forwards to
`cargo publish --dry-run` regardless of registry state.

Environment:
  CARGO_REGISTRY_TOKEN  required when not in --dry-run mode
USAGE
}

DRY_RUN=0
CRATE=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --dry-run) DRY_RUN=1; shift ;;
    -h|--help) usage; exit 0 ;;
    --) shift; break ;;
    -*) printf 'cargo-publish-if-new: unknown flag %s\n' "$1" >&2; usage; exit 2 ;;
    *)  if [ -n "$CRATE" ]; then
          printf 'cargo-publish-if-new: unexpected extra arg %s\n' "$1" >&2
          usage; exit 2
        fi
        CRATE="$1"; shift ;;
  esac
done

if [ -z "$CRATE" ]; then
  usage
  exit 2
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ROOT_MANIFEST="$REPO_ROOT/Cargo.toml"
CRATE_MANIFEST="$REPO_ROOT/src/rust/crates/$CRATE/Cargo.toml"

# Read [workspace.package].version from the root manifest. Mirrors
# scripts/set-version.sh's reader semantics: first version line inside
# the [workspace.package] block.
read_workspace_version() {
  awk '
    /^\[workspace\.package\]/ { in_block = 1; next }
    /^\[/                     { in_block = 0 }
    in_block && /^version[[:space:]]*=[[:space:]]*"/ {
      n = split($0, parts, "\"")
      if (n >= 3) { print parts[2] }
      exit
    }
  ' "$ROOT_MANIFEST"
}

# Read [package].version from a crate manifest. Returns "WORKSPACE"
# (sentinel) when the crate inherits via `version.workspace = true`.
read_crate_version() {
  awk '
    /^\[package\]/ { in_block = 1; next }
    /^\[/          { in_block = 0 }
    in_block && /^version[[:space:]]*\.workspace[[:space:]]*=[[:space:]]*true/ {
      print "WORKSPACE"; exit
    }
    in_block && /^version[[:space:]]*=[[:space:]]*"/ {
      n = split($0, parts, "\"")
      if (n >= 3) { print parts[2] }
      exit
    }
  ' "$1"
}

resolve_local_version() {
  if [ -n "${CARGO_PUBLISH_IF_NEW_LOCAL_VERSION:-}" ]; then
    printf '%s' "$CARGO_PUBLISH_IF_NEW_LOCAL_VERSION"
    return 0
  fi
  if [ ! -f "$CRATE_MANIFEST" ]; then
    printf 'cargo-publish-if-new: manifest not found at %s\n' "$CRATE_MANIFEST" >&2
    return 1
  fi
  local v
  v="$(read_crate_version "$CRATE_MANIFEST")"
  if [ "$v" = "WORKSPACE" ]; then
    v="$(read_workspace_version)"
  fi
  if [ -z "$v" ]; then
    printf 'cargo-publish-if-new: cannot read version for %s\n' "$CRATE" >&2
    return 1
  fi
  printf '%s' "$v"
}

# Returns 0 if $version appears anywhere in versions[].num for $crate
# at the configured registry, 1 if absent, 2 if the registry query
# itself failed (network / 5xx / non-JSON body). Yanked versions are
# still treated as "already published" because cargo publish refuses
# to republish a yanked version number.
registry_has_version() {
  local crate="$1" version="$2"
  local base="${CARGO_PUBLISH_IF_NEW_REGISTRY:-https://crates.io}"
  local url="${base}/api/v1/crates/${crate}"
  local body
  if ! body="$(curl --silent --show-error --fail --max-time 30 \
        -H "User-Agent: fathomdb-release-publish-guard (https://github.com/coreyt/fathomdb)" \
        "$url" 2>&1)"; then
    printf 'cargo-publish-if-new: registry query failed for %s — %s\n' \
      "$crate" "$body" >&2
    return 2
  fi
  if printf '%s' "$body" | jq -e --arg v "$version" \
       '.versions | map(.num) | index($v) != null' >/dev/null 2>&1; then
    return 0
  fi
  return 1
}

LOCAL_VERSION="$(resolve_local_version)"

if [ "$DRY_RUN" -eq 1 ]; then
  printf 'cargo-publish-if-new: %s@%s — --dry-run; forwarding to cargo publish --dry-run\n' \
    "$CRATE" "$LOCAL_VERSION"
  exec cargo publish --dry-run -p "$CRATE"
fi

set +e
registry_has_version "$CRATE" "$LOCAL_VERSION"
rc=$?
set -e
case "$rc" in
  0)
    printf 'cargo-publish-if-new: %s@%s already published; skipping\n' \
      "$CRATE" "$LOCAL_VERSION"
    exit 0
    ;;
  1)
    printf 'cargo-publish-if-new: %s@%s not on registry; publishing\n' \
      "$CRATE" "$LOCAL_VERSION"
    if [ -z "${CARGO_REGISTRY_TOKEN:-}" ]; then
      printf 'cargo-publish-if-new: CARGO_REGISTRY_TOKEN not set\n' >&2
      exit 1
    fi
    exec cargo publish -p "$CRATE" --token "$CARGO_REGISTRY_TOKEN"
    ;;
  *)
    exit "$rc"
    ;;
esac
