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

confirmation_input_block() {
  awk -v input="$1" '
    $0 == "      " input ":" { found = 1; in_block = 1; next }
    in_block && /^      [[:alnum:]_]+:$/ { exit }
    in_block { print }
    END { exit !found }
  ' "$RELEASE_YML"
}

if confirmation_block="$(confirmation_input_block confirm_release_version)"; then
  if grep -qE '^[[:space:]]+required:[[:space:]]+true' <<<"$confirmation_block"; then
    printf 'FAIL  release.yml confirmation input must be optional\n' >&2
    FIXTURE_FAILED=$((FIXTURE_FAILED + 1))
  elif grep -qE '^[[:space:]]+default:' <<<"$confirmation_block"; then
    printf 'FAIL  release.yml confirmation input must not have a default\n' >&2
    FIXTURE_FAILED=$((FIXTURE_FAILED + 1))
  else
    printf 'PASS  release.yml confirmation input is optional and has no default\n'
  fi
else
  printf 'FAIL  release.yml is missing the confirm_release_version input\n' >&2
  FIXTURE_FAILED=$((FIXTURE_FAILED + 1))
fi

# The no-default assertion needs a non-vacuous control against the same
# confirmation input, not the separately-defaulted dry_run input.
CONFIRMATION_NO_DEFAULT_GUARD="$REPO_ROOT/scripts/release/assert-confirm-release-version-no-default.sh"
if "$CONFIRMATION_NO_DEFAULT_GUARD" "$RELEASE_YML"; then
  printf 'PASS  confirmation no-default guard accepts release.yml\n'
else
  printf 'FAIL  confirmation no-default guard rejected release.yml\n' >&2
  FIXTURE_FAILED=$((FIXTURE_FAILED + 1))
fi

DEFAULTED_CONFIRMATION_FIXTURE="$(mktemp)"
trap 'rm -f "$DEFAULTED_CONFIRMATION_FIXTURE"' EXIT
awk '
  $0 == "      confirm_release_version:" { in_confirmation = 1 }
  in_confirmation && !inserted && $0 ~ /^        description:/ {
    print
    print "        default: \"unsafe-control\""
    inserted = 1
    next
  }
  { print }
  END { exit !inserted }
' "$RELEASE_YML" > "$DEFAULTED_CONFIRMATION_FIXTURE"

if "$CONFIRMATION_NO_DEFAULT_GUARD" "$DEFAULTED_CONFIRMATION_FIXTURE"; then
  printf 'FAIL  confirmation no-default guard accepted deliberately-defaulted fixture\n' >&2
  FIXTURE_FAILED=$((FIXTURE_FAILED + 1))
else
  printf 'PASS  confirmation no-default guard rejects deliberately-defaulted fixture\n'
fi

# Determination (Slice 40 B8): this test was stale, not the release workflow.
# The workflow delegates dry-runs to the idempotency helper, which prevents a
# rerun from trying to republish an already-published immutable version. Assert
# that helper and the shipped tier-to-crate order; never replace it with a
# direct cargo-publish invocation just to satisfy this fixture.
TIER_FAILED=0

job_block_exists() {
  awk -v job="$1" '$0 == "  " job ":" { found = 1 } END { exit !found }' "$RELEASE_YML"
}

for tier_and_crate in \
  't1-embedder-api:fathomdb-embedder-api' \
  't2-schema:fathomdb-schema' \
  't3-query:fathomdb-query' \
  't4-embedder:fathomdb-embedder' \
  't5-engine:fathomdb-engine' \
  't6-facade:fathomdb' \
  't7-cli:fathomdb-cli'; do
  tier="${tier_and_crate%%:*}"
  crate="${tier_and_crate#*:}"
  job="publish-rust-${tier}"
  if ! job_block_exists "$job"; then
    printf 'FAIL  release.yml is missing %s job block\n' "$job" >&2
    TIER_FAILED=$((TIER_FAILED + 1))
    continue
  fi
  block=$(awk "/publish-rust-${tier}:/{flag=1} flag; /^  [a-z]/&&!/publish-rust-${tier}:/{if(flag){flag=0}}" "$RELEASE_YML")
  if ! grep -Fq "bash scripts/release/cargo-publish-if-new.sh --dry-run ${crate}" <<<"$block"; then
    printf 'FAIL  publish-rust-%s dry-run branch does not use cargo-publish-if-new for %s\n' "$tier" "$crate" >&2
    TIER_FAILED=$((TIER_FAILED + 1))
  fi
  if grep -qE 'cargo package --allow-dirty --no-verify' <<<"$block"; then
    printf 'FAIL  publish-rust-%s still uses cargo package --allow-dirty --no-verify (forbidden post-bootstrap)\n' "$tier" >&2
    TIER_FAILED=$((TIER_FAILED + 1))
  fi
done

# Control: the job-existence assertion must reject an absent job rather than
# treating its empty awk block as a normal tier.
if job_block_exists 'publish-rust-intentionally-absent-control'; then
  printf 'FAIL  job-block-exists control unexpectedly found an absent job\n' >&2
  TIER_FAILED=$((TIER_FAILED + 1))
else
  printf 'PASS  job-block-exists assertion rejects an absent job\n'
fi
if [ "$TIER_FAILED" -eq 0 ]; then
  printf 'PASS  release.yml publish-rust-t1..t7 use cargo-publish-if-new in the shipped order\n'
fi

FIXTURE_FAILED=$((FIXTURE_FAILED + TIER_FAILED))
if [ "$FIXTURE_FAILED" -gt 0 ]; then
  printf '\n%d assertion(s) failed across the label/tier loops above\n' "$FIXTURE_FAILED" >&2
  exit 1
fi
