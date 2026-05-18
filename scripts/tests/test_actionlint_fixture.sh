#!/usr/bin/env bash
# scripts/tests/test_actionlint_fixture.sh — proves actionlint is
# installed, runnable, and rejects the deliberately-broken fixture under
# scripts/tests/fixtures/. Existence of this test is the contract that
# scripts/agent-lint.sh's workflow-validation step is non-trivial.
#
# WHY this fixture and not a .github/workflows/* file: the agent-lint glob
# is `.github/workflows/*.yml` and would catch a broken file there as a
# real failure. The fixture lives outside that glob so the suite can
# exercise the bad-input path without breaking the canonical workflow
# directory.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIX="$SCRIPT_DIR/fixtures/actionlint-bad.yml"

if ! command -v actionlint >/dev/null 2>&1; then
  printf 'SKIP  actionlint not installed (run scripts/bootstrap.sh)\n'
  exit 0
fi

if actionlint "$FIX" >/dev/null 2>&1; then
  printf 'FAIL  actionlint accepted the deliberately-broken fixture\n' >&2
  exit 1
fi

printf 'PASS  actionlint rejects deliberately-broken fixture\n'

# release.yml regression assertions (Phase 12-RC1-WF-FIX-1).
# napi-rs only resolves prebuilt binaries by the exact platform-label triples
# enumerated in src/ts/src/binding.ts; if release.yml uploads under a
# non-canonical label, install-from-npm silently falls back to "no native
# addon found" at runtime. Lock the four labels we ship to RC1 here.
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
RELEASE_YML="$REPO_ROOT/.github/workflows/release.yml"

for label in linux-x64-gnu darwin-x64 darwin-arm64 win32-x64-msvc; do
  if ! grep -qE "label:[[:space:]]+${label}\$" "$RELEASE_YML"; then
    printf 'FAIL  release.yml missing canonical napi label: %s\n' "$label" >&2
    exit 1
  fi
done
printf 'PASS  release.yml carries all 4 canonical napi labels\n'

# cargo publish --dry-run rejects workspaces with un-published sibling deps
# (T2..T7 fail at "no matching package named ... found" before the registry
# would ever see the upload). cargo package --allow-dirty --no-verify packs
# the crate identically without touching the registry — equivalent gate for
# our manifests, and the only one that succeeds across the whole tier chain
# pre-publish. Forbid the publish --dry-run regression in the workflow.
for tier in t1-embedder-api t2-schema t3-query t4-engine t5-embedder t6-facade t7-cli; do
  block=$(awk "/publish-rust-${tier}:/{flag=1} flag; /^  [a-z]/&&!/publish-rust-${tier}:/{if(flag){flag=0}}" "$RELEASE_YML")
  if ! grep -qE 'cargo package --allow-dirty --no-verify -p ' <<<"$block"; then
    printf 'FAIL  publish-rust-%s dry-run branch is not cargo package --allow-dirty --no-verify\n' "$tier" >&2
    exit 1
  fi
  if grep -qE 'cargo publish --dry-run' <<<"$block"; then
    printf 'FAIL  publish-rust-%s still uses cargo publish --dry-run (forbidden — fails on sibling deps)\n' "$tier" >&2
    exit 1
  fi
done
printf 'PASS  release.yml publish-rust-t1..t7 dry-run uses cargo package\n'
