#!/usr/bin/env bash
# scripts/release/local-dry-run.sh — run release.yml's validatable subset
# locally before pushing a release tag.
#
# Purpose: catch release-pipeline failures on the developer's box, not by
# burning CI cycles. The CI workflow_dispatch dry-run mode becomes a final
# confirmation after this script is GREEN, not the primary debug loop.
#
# What this script does (mirrors .github/workflows/release.yml):
#   1. scripts/verify-release-gates.sh    (preflight: tag↔manifest lockstep,
#                                          CHANGELOG section, axis-W/E gates)
#   2. scripts/release/verify-embedder-api-no-drift.sh
#                                         (Axis-E published-API drift guard —
#                                          prevents the v0.8.9 partial publish;
#                                          mirrors the release.yml preflight)
#   3. cargo build --release --workspace  (matches build-rust step 1)
#   4. cargo package --no-verify on the three leaf crates
#                                         (matches build-rust steps 2-4)
#   5. cargo publish --dry-run --no-verify on the three leaf crates
#                                         (matches T1-T3 dry-run path)
#
# What this script does NOT validate (matches CI dry-run, which is also
# structurally unable to validate these without a real publish):
#   • Dependent-crate (engine/embedder/facade/cli) publish paths — cross-tier
#     workspace-dep resolve only succeeds after real publish. See the
#     rationale in release.yml L153-170 and cargo-publish-if-new.sh.
#   • Python wheel matrix (covered by build-python in CI).
#   • napi prebuilds (covered by build-napi in CI).
#   • npm publish path (covered by publish-npm in CI dry-run).
#
# Usage: bash scripts/release/local-dry-run.sh
# Exit 0 = GREEN; any non-zero = failure (line + crate identified).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

LEAVES=(fathomdb-embedder-api fathomdb-schema fathomdb-query)

log() { printf '\n=== %s ===\n' "$*"; }

workspace_version="$(awk '
  /^\[workspace\.package\]/ { in_block = 1; next }
  /^\[/ { in_block = 0 }
  in_block && /^version[[:space:]]*=/ {
    n = split($0, fields, "\"")
    if (n >= 3) { print fields[2] }
    exit
  }
' Cargo.toml)"
if [ -z "$workspace_version" ]; then
  printf 'local-dry-run: cannot read Cargo.toml workspace version\n' >&2
  exit 1
fi

# A workflow_dispatch rehearsal is anchored to an immutable commit rather
# than the mutable branch ref selected in the UI. Use the same full revision
# for this local rehearsal before any build or dry-run publish command runs.
candidate_commit="$(git -C "$REPO_ROOT" rev-parse --verify HEAD^{commit})"

log "Step 1/5: scripts/verify-release-gates.sh"
GITHUB_EVENT_NAME=workflow_dispatch \
DRY_RUN=true \
RELEASE_DISPATCH_VERSION="$workspace_version" \
RELEASE_GATES_TAG="v$workspace_version" \
RELEASE_GATES_CANDIDATE_COMMIT="$candidate_commit" \
RELEASE_GATES_REQUIRE_TAG_CHECKOUT=0 \
RELEASE_GATES_HEAD_REF=refs/remotes/origin/main \
bash scripts/verify-release-gates.sh

log "Step 2/5: scripts/release/verify-embedder-api-no-drift.sh"
bash scripts/release/verify-embedder-api-no-drift.sh

log "Step 3/5: cargo build --release --workspace"
cargo build --release --workspace

log "Step 4/5: cargo package --no-verify (leaves)"
for crate in "${LEAVES[@]}"; do
  printf -- '--- cargo package -p %s ---\n' "$crate"
  cargo package --allow-dirty --no-verify -p "$crate"
done

log "Step 5/5: cargo publish --dry-run --no-verify (leaves)"
for crate in "${LEAVES[@]}"; do
  printf -- '--- cargo publish --dry-run -p %s ---\n' "$crate"
  cargo publish --allow-dirty --dry-run --no-verify -p "$crate"
done

log "GREEN: local release dry-run passed"
printf 'Next step: push the release tag (RC or GA). The CI dry-run dispatch\n'
printf 'is a final confirmation; this script covers the same validatable surface.\n'
