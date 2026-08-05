#!/usr/bin/env bash
# Focused regression coverage for the public current-truth guard.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CHECKER="${CHECKER_UNDER_TEST:-$REPO_ROOT/scripts/check-public-doc-truth.py}"

TMPROOT="$(mktemp -d)"
trap 'rm -rf "$TMPROOT"' EXIT

make_fixture() {
  local root="$1"
  mkdir -p "$root/dev/plans" "$root/docs/getting-started" \
    "$root/docs/install" "$root/docs/compatibility"
  cp "$REPO_ROOT/README.md" "$root/README.md"
  cp "$REPO_ROOT/Cargo.toml" "$root/Cargo.toml"
  cp "$REPO_ROOT/dev/plans/release-state-0.8.21.json" "$root/dev/plans/"
  cp "$REPO_ROOT/dev/platform-capabilities.json" "$root/dev/"
  cp "$REPO_ROOT/docs/index.md" "$root/docs/"
  cp "$REPO_ROOT/docs/getting-started/index.md" "$root/docs/getting-started/"
  cp "$REPO_ROOT/docs/install/"{python,typescript,rust}.md "$root/docs/install/"
  cp "$REPO_ROOT/docs/compatibility/index.md" "$root/docs/compatibility/"
}

expect_pass() {
  local root="$1" description="$2"
  if REPO_ROOT="$root" python3 "$CHECKER" >/dev/null; then
    printf 'PASS  %s\n' "$description"
  else
    printf 'FAIL  %s\n' "$description" >&2
    exit 1
  fi
}

expect_fail() {
  local root="$1" description="$2"
  if REPO_ROOT="$root" python3 "$CHECKER" >/dev/null 2>&1; then
    printf 'FAIL  %s\n' "$description" >&2
    exit 1
  fi
  printf 'PASS  %s\n' "$description"
}

FIXTURE="$TMPROOT/fixture"
make_fixture "$FIXTURE"
expect_pass "$FIXTURE" 'baseline current public facts agree with release and platform state'

sed -i 's/v0\.8\.21 is published/v0.8.21 is not yet published/' "$FIXTURE/README.md"
expect_fail "$FIXTURE" 'rejects an unpublished claim for the published release'

make_fixture "$FIXTURE"
sed -i 's/aarch64 is published/aarch64 is not published/' "$FIXTURE/docs/compatibility/index.md"
expect_fail "$FIXTURE" 'requires the published aarch64 native-artifact assertion'

make_fixture "$FIXTURE"
sed -i 's/npm install fathomdb@next/npm install fathomdb@0.8.21/' "$FIXTURE/docs/install/typescript.md"
expect_fail "$FIXTURE" 'requires npm next install guidance'

make_fixture "$FIXTURE"
sed -i 's/Nine Rust workspace members/Eight Rust workspace members/' "$FIXTURE/README.md"
expect_fail "$FIXTURE" 'rejects a false Rust workspace member count'

make_fixture "$FIXTURE"
printf '\nHistorical note: 0.8.19 is not yet published in this example.\n' >>"$FIXTURE/docs/index.md"
expect_pass "$FIXTURE" 'does not treat historical-version prose as a current-release claim'

printf '\nAll public-doc-truth tests passed\n'
