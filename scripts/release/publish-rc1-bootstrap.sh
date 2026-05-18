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
for c in "${TIERS[@]}"; do
  # Idempotent: skip if already on crates.io at RC_VERSION.
  if cargo search "$c" --limit 1 | grep -qE "^${c} = \"${RC_VERSION}\""; then
    printf 'SKIP  %s %s already on crates.io\n' "$c" "$RC_VERSION"
    continue
  fi
  printf '==> publishing %s %s\n' "$c" "$RC_VERSION"
  cargo publish -p "$c" --token "${CARGO_REGISTRY_TOKEN}"
  printf '==> sleeping 60s for index propagation\n'
  sleep 60
done
printf 'DONE bootstrap publish %s\n' "$RC_VERSION"
