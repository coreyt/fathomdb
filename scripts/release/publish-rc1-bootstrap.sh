#!/usr/bin/env bash
# Bootstrap-publish 0.6.0-rc.1 of T1-T7 to crates.io so future
# workflow_dispatch dry-runs of rc.2 / GA resolve sibling deps
# against registry rc.1. One-time operation per the
# 12-RC1-WF-FIX-1 sentinel-publish design (see
# dev/design/release.md § RC1 bootstrap publish).
set -euo pipefail
: "${CARGO_REGISTRY_TOKEN:?CARGO_REGISTRY_TOKEN must be set}"
RC_VERSION="0.6.0-rc.1"
TIERS=(
  fathomdb-embedder-api
  fathomdb-schema
  fathomdb-query
  fathomdb-engine
  fathomdb-embedder
  fathomdb
  fathomdb-cli
)
# Sparse-index path per cargo registry convention. Enumerates
# every published version (unlike `cargo search`, which only
# returns the latest), so this stays idempotent after rc.2/GA
# land.
sparse_path() {
  local name="$1"
  local len="${#name}"
  case "$len" in
    1) printf '1/%s' "$name" ;;
    2) printf '2/%s' "$name" ;;
    3) printf '3/%s/%s' "${name:0:1}" "$name" ;;
    *) printf '%s/%s/%s' "${name:0:2}" "${name:2:2}" "$name" ;;
  esac
}
for c in "${TIERS[@]}"; do
  # Idempotent: skip if RC_VERSION appears anywhere in the
  # crate's sparse-index version list.
  url="https://index.crates.io/$(sparse_path "$c")"
  if curl -fsS "$url" 2>/dev/null | grep -qF "\"vers\":\"${RC_VERSION}\""; then
    printf 'SKIP  %s %s already on crates.io\n' "$c" "$RC_VERSION"
    continue
  fi
  printf '==> publishing %s %s\n' "$c" "$RC_VERSION"
  cargo publish -p "$c" --token "${CARGO_REGISTRY_TOKEN}"
  printf '==> sleeping 60s for index propagation\n'
  sleep 60
done
printf 'DONE bootstrap publish %s\n' "$RC_VERSION"
