#!/usr/bin/env bash
# Exact local/CI runtime pins prevent a locally green preflight from using a
# different compiler or publisher than the release workflow.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
FAILED=0

pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

if grep -qx 'channel = "1.95.0"' "$REPO_ROOT/rust-toolchain.toml"; then
  pass "rust-toolchain.toml pins Rust 1.95.0"
else
  fail "rust-toolchain.toml must pin Rust 1.95.0"
fi

for workflow in ci.yml release.yml perf-canonical.yml; do
  path="$REPO_ROOT/.github/workflows/$workflow"
  rust_actions="$(grep -c 'uses: dtolnay/rust-toolchain@' "$path" || true)"
  exact_pins="$(grep -c 'toolchain: "1.95.0"' "$path" || true)"
  if [ "$rust_actions" -gt 0 ] && [ "$rust_actions" -eq "$exact_pins" ]; then
    pass "$workflow pins every Rust setup to 1.95.0"
  else
    fail "$workflow must pin every dtolnay/rust-toolchain setup to 1.95.0"
  fi
done

for manifest in "$REPO_ROOT/package.json" "$REPO_ROOT/src/ts/package.json"; do
  if grep -q '"packageManager": "npm@11.12.1"' "$manifest"; then
    pass "$(basename "$manifest") pins npm 11.12.1"
  else
    fail "$(basename "$manifest") must pin npm 11.12.1"
  fi
done

release="$REPO_ROOT/.github/workflows/release.yml"
if [ "$(grep -c 'NPM_BIN: "npm"' "$release" || true)" -eq 2 ] && ! grep -q 'npx npm@latest' "$release"; then
  pass "release publishing uses Node-bundled npm"
else
  fail "release publishing must use exactly the pinned Node-bundled npm"
fi

if grep -q 'actionlint/v1.7.12/scripts/download-actionlint.bash' "$release" \
  && grep -q 'bash -s -- 1.7.12' "$release" \
  && grep -q 'readonly ACTIONLINT_VERSION="1.7.12"' "$REPO_ROOT/scripts/agent-lint.sh" \
  && grep -q 'readonly ACTIONLINT_VERSION="1.7.12"' "$REPO_ROOT/scripts/bootstrap.sh"; then
  pass "actionlint is pinned identically in CI and local tooling"
else
  fail "actionlint must be pinned to 1.7.12 in CI and local tooling"
fi

if grep -A3 '^      release_version:$' "$release" | grep -q 'required: true' \
  && grep -q 'RELEASE_TAG:.*inputs.release_version' "$release" \
  && grep -q 'RELEASE_GATES_TAG: \${{ env.RELEASE_TAG }}' "$release" \
  && ! grep -q 'GITHUB_REF_NAME#v' "$release" \
  && grep -q 'tag_name: \${{ env.RELEASE_TAG }}' "$release"; then
  pass "dispatch derives all release consumers from its canonical tag"
else
  fail "release dispatch must derive preflight, smokes, co-tagging, and GitHub Release from RELEASE_TAG"
fi

# A manual dispatch defaults to the UI-selected branch unless every checkout
# explicitly switches to the canonical release tag. Count both the checkout
# actions and their guarded tag refs so adding a new publish job cannot escape
# the recovery-path safety boundary.
checkout_total="$(grep -c 'uses: actions/checkout@' "$release" || true)"
canonical_checkout_ref='ref: ${{ github.event_name == '\''workflow_dispatch'\'' && format('\''refs/tags/{0}'\'', env.RELEASE_TAG) || github.ref }}'
canonical_ref_total="$(grep -Fc "$canonical_checkout_ref" "$release" || true)"
if [ "$checkout_total" -gt 0 ] && [ "$checkout_total" -eq "$canonical_ref_total" ] \
  && grep -q 'RELEASE_GATES_REQUIRE_TAG_CHECKOUT: "1"' "$release"; then
  pass "dispatch checks out and verifies the canonical tag in every release job"
else
  fail "manual dispatch must not build the UI-selected ref; every release checkout needs the canonical tag"
fi

if [ "$FAILED" -gt 0 ]; then
  exit 1
fi
