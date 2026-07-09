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
# Registry-safety (Fix-2): the version QUERY honours
# CARGO_PUBLISH_IF_NEW_REGISTRY, but `cargo publish` has no URL knob and always
# targets prod crates.io from ambient config. So a staging/test run whose query
# was redirected to a non-prod index could still PUBLISH to the real crates.io —
# the exact split-brain hole closed for npm (--registry) and PyPI
# (--repository-url). Rule, mirroring that posture:
#   • query NOT overridden (real release)         → publish to crates.io (unchanged);
#   • query overridden + publish-registry mapped   → cargo publish --registry <name>
#     (a [registries] alt configured to the same staging host);
#   • query overridden + NO publish-registry map   → FAIL CLOSED (loud, non-zero).
#     Never silently publish to prod when the query was redirected.
#
# Test env overrides (NOT for production CI):
#   CARGO_PUBLISH_IF_NEW_REGISTRY         — base URL in place of https://crates.io
#                                           (redirects the version QUERY only)
#   CARGO_PUBLISH_IF_NEW_PUBLISH_REGISTRY — cargo alt-registry NAME (a configured
#                                           [registries] entry / CARGO_REGISTRIES_
#                                           <NAME>_INDEX) to route the PUBLISH to,
#                                           so query and publish hit the same host
#   CARGO_PUBLISH_IF_NEW_LOCAL_VERSION    — skip manifest read; use this version
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
  # Fail-closed: validate the body is parseable JSON before inspecting it.
  # A malformed/truncated 200 response would otherwise silently fall through
  # as "version not found" (return 1) and trigger an unnecessary publish attempt.
  if ! printf '%s' "$body" | jq . >/dev/null 2>&1; then
    printf 'cargo-publish-if-new: registry returned malformed JSON for %s\n' \
      "$crate" >&2
    return 2
  fi
  if printf '%s' "$body" | jq -e --arg v "$version" \
       '.versions | map(.num) | index($v) != null' >/dev/null 2>&1; then
    return 0
  fi
  return 1
}

LOCAL_VERSION="$(resolve_local_version)"

# Dependent crates: cannot be dry-run-published because `cargo package`
# rewrites path deps to versioned deps and then resolves them against the
# registry. The just-"published" sibling versions aren't actually on
# crates.io during a dry-run (no real upload happened), so the resolve
# fails with "failed to select a version for the requirement …".
# `--no-verify` only skips the verify step (compile the packaged tarball),
# NOT the package step where the resolve happens. This matches the
# rationale already documented in .github/workflows/release.yml L153-170:
# build-rust packages leaf crates only, for the same reason.
#
# Manifest correctness for dependent crates is enforced at real publish
# time inside `cargo publish`; the local-dry-run.sh script + build-rust
# job cover everything else (compile, leaf packaging, leaf publish path).
is_dependent_crate() {
  case "$1" in
    fathomdb-engine|fathomdb-embedder|fathomdb|fathomdb-cli) return 0 ;;
    *) return 1 ;;
  esac
}

if [ "$DRY_RUN" -eq 1 ]; then
  if is_dependent_crate "$CRATE"; then
    printf 'cargo-publish-if-new: %s@%s — --dry-run skipped (dependent crate; cross-tier workspace-dep resolve only succeeds after real publish; manifest validated at real publish time)\n' \
      "$CRATE" "$LOCAL_VERSION"
    exit 0
  fi
  printf 'cargo-publish-if-new: %s@%s — --dry-run; forwarding to cargo publish --dry-run --no-verify\n' \
    "$CRATE" "$LOCAL_VERSION"
  exec cargo publish --dry-run --no-verify -p "$CRATE"
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
    # SAFETY (Fix-2): registry-routing split-brain guard. The query above may
    # have been redirected by CARGO_PUBLISH_IF_NEW_REGISTRY; a bare
    # `cargo publish` would still target prod crates.io. See the header rule.
    if [ -n "${CARGO_PUBLISH_IF_NEW_REGISTRY:-}" ]; then
      if [ -n "${CARGO_PUBLISH_IF_NEW_PUBLISH_REGISTRY:-}" ]; then
        # Query overridden + publish-registry mapped: route the publish to the
        # SAME staging host the query hit. A staging/test run is structurally
        # incapable of touching prod crates.io.
        exec cargo publish -p "$CRATE" \
          --registry "$CARGO_PUBLISH_IF_NEW_PUBLISH_REGISTRY" \
          --token "$CARGO_REGISTRY_TOKEN"
      fi
      # Query overridden but no publish-registry mapping: FAIL CLOSED. Never run
      # a default-crates.io `cargo publish` when the query was redirected.
      printf 'cargo-publish-if-new: CARGO_PUBLISH_IF_NEW_REGISTRY is overridden (%s) but CARGO_PUBLISH_IF_NEW_PUBLISH_REGISTRY is unset; refusing to run a default-crates.io `cargo publish` for a redirected query (fail-closed split-brain guard)\n' \
        "${CARGO_PUBLISH_IF_NEW_REGISTRY}" >&2
      exit 1
    fi
    # Not overridden (real release): publish to crates.io — behaviour unchanged.
    exec cargo publish -p "$CRATE" --token "$CARGO_REGISTRY_TOKEN"
    ;;
  *)
    exit "$rc"
    ;;
esac
