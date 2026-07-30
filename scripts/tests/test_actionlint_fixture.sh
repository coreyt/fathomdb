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

# 0.8.20 R-20-HARNESS: accumulate-then-exit, not fail-fast-on-first-item.
# The original loops below `exit 1`ed on the FIRST failing label/tier, which
# is exactly why only one publish-rust tier (t1-embedder-api) ever surfaced
# as red and a second, independent tier could stay hidden behind it. This
# does NOT fix either red suite (that stays Slice 40's, and both live in
# .github/workflows/release.yml, which this unit does not touch) — it only
# ensures every failing label/tier is reported in one pass.
FIXTURE_FAILED=0

for label in linux-x64-gnu darwin-x64 darwin-arm64 win32-x64-msvc; do
  if ! grep -qE "label:[[:space:]]+${label}\$" "$RELEASE_YML"; then
    printf 'FAIL  release.yml missing canonical napi label: %s\n' "$label" >&2
    FIXTURE_FAILED=$((FIXTURE_FAILED + 1))
  fi
done
if [ "$FIXTURE_FAILED" -eq 0 ]; then
  printf 'PASS  release.yml carries all 4 canonical napi labels\n'
fi

# Sibling-dep resolution: cargo publish --dry-run requires every in-workspace
# dep to be resolvable from the registry. The 0.6.0-rc.1 bootstrap publish
# (scripts/release/publish-rc1-bootstrap.sh, operator-run) seeds crates.io
# with all 7 axis-W crates + axis-E embedder-api so subsequent dispatches
# (rc.2, rc.3, …, GA) can use the canonical `cargo publish --dry-run` gate.
# Lock that gate in here; forbid the cargo-package workaround that briefly
# replaced it pre-bootstrap.
TIER_FAILED=0
for tier in t1-embedder-api t2-schema t3-query t4-engine t5-embedder t6-facade t7-cli; do
  block=$(awk "/publish-rust-${tier}:/{flag=1} flag; /^  [a-z]/&&!/publish-rust-${tier}:/{if(flag){flag=0}}" "$RELEASE_YML")
  if ! grep -qE 'cargo publish --dry-run -p ' <<<"$block"; then
    printf 'FAIL  publish-rust-%s dry-run branch is not cargo publish --dry-run -p\n' "$tier" >&2
    TIER_FAILED=$((TIER_FAILED + 1))
  fi
  if grep -qE 'cargo package --allow-dirty --no-verify' <<<"$block"; then
    printf 'FAIL  publish-rust-%s still uses cargo package --allow-dirty --no-verify (forbidden post-bootstrap)\n' "$tier" >&2
    TIER_FAILED=$((TIER_FAILED + 1))
  fi
done
if [ "$TIER_FAILED" -eq 0 ]; then
  printf 'PASS  release.yml publish-rust-t1..t7 dry-run uses cargo publish --dry-run\n'
fi

FIXTURE_FAILED=$((FIXTURE_FAILED + TIER_FAILED))
if [ "$FIXTURE_FAILED" -gt 0 ]; then
  printf '\n%d assertion(s) failed across the label/tier loops above\n' "$FIXTURE_FAILED" >&2
  exit 1
fi
