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
if [ "$(grep -c 'NPM_BIN: "npm"' "$release" || true)" -eq 6 ] && ! grep -q 'npx npm@latest' "$release"; then
  pass "release publishing uses Node-bundled npm"
else
  fail "release publishing must use the pinned Node-bundled npm in every npm publish job"
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

# A dry-run dispatch must receive an explicit immutable full SHA before the
# release tag exists. A non-dry-run recovery dispatch must instead use the
# canonical tag. Count every checkout so a new release job cannot escape either
# side of that boundary.
checkout_total="$(grep -c 'uses: actions/checkout@' "$release" || true)"
release_checkout_ref='ref: ${{ env.RELEASE_CHECKOUT_REF }}'
release_checkout_total="$(grep -Fc "$release_checkout_ref" "$release" || true)"
if [ "$checkout_total" -gt 0 ] && [ "$checkout_total" -eq "$release_checkout_total" ] \
  && grep -A3 '^      candidate_commit:$' "$release" | grep -q 'type: string' \
  && grep -Fq "RELEASE_GATES_CANDIDATE_COMMIT: \${{ inputs.candidate_commit || '' }}" "$release" \
  && grep -Fq "RELEASE_CHECKOUT_REF: \${{ github.event_name == 'workflow_dispatch' && inputs.dry_run == true && inputs.candidate_commit || github.event_name == 'workflow_dispatch' && format('refs/tags/v{0}', inputs.release_version) || github.ref }}" "$release" \
  && grep -Fq 'RELEASE_GATES_CANDIDATE_COMMIT: ${{ env.RELEASE_GATES_CANDIDATE_COMMIT }}' "$release" \
  && grep -Fq "RELEASE_GATES_REQUIRE_TAG_CHECKOUT: \${{ (github.event_name != 'workflow_dispatch' || inputs.dry_run != true) && '1' || '0' }}" "$release"; then
  pass "dry-run dispatch uses its explicit candidate SHA while publish/recovery dispatch verifies the canonical tag"
else
  fail "release checkout must use an explicit candidate SHA only for dry-runs and the canonical tag for publish/recovery"
fi

local_dry_run="$REPO_ROOT/scripts/release/local-dry-run.sh"
if grep -Fq 'GITHUB_EVENT_NAME=workflow_dispatch' "$local_dry_run" \
  && grep -Fq 'DRY_RUN=true' "$local_dry_run" \
  && grep -Fq 'RELEASE_GATES_REQUIRE_TAG_CHECKOUT=0' "$local_dry_run" \
  && grep -Fq 'RELEASE_GATES_HEAD_REF=refs/remotes/origin/main' "$local_dry_run"; then
  pass "local release rehearsal uses the pre-tag dry-run gate against origin/main"
else
  fail "local release rehearsal must not require an uncreated tag"
fi

# The local helper must exercise the same immutable-candidate gate as a
# workflow_dispatch dry run.  Its full release steps are deliberately not
# run here: a fixture gate records the supplied candidate then exits with a
# sentinel, proving the helper reaches the gate before any build or publish
# command could run.
LOCAL_DRY_RUN_FIXTURE="$(mktemp -d)"
cleanup_local_dry_run_fixture() { rm -rf "$LOCAL_DRY_RUN_FIXTURE"; }
trap cleanup_local_dry_run_fixture EXIT
mkdir -p "$LOCAL_DRY_RUN_FIXTURE/scripts/release"
cp "$local_dry_run" "$LOCAL_DRY_RUN_FIXTURE/scripts/release/local-dry-run.sh"
cat > "$LOCAL_DRY_RUN_FIXTURE/Cargo.toml" <<'EOF'
[workspace]
members = []

[workspace.package]
version = "0.8.22"
EOF
cat > "$LOCAL_DRY_RUN_FIXTURE/scripts/verify-release-gates.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
: "${RELEASE_GATES_CANDIDATE_COMMIT:?missing immutable candidate}"
printf '%s\n' "$RELEASE_GATES_CANDIDATE_COMMIT" > "$ROOT/candidate-commit"
exit 47
EOF
chmod +x "$LOCAL_DRY_RUN_FIXTURE/scripts/verify-release-gates.sh"
git -C "$LOCAL_DRY_RUN_FIXTURE" init -q
git -C "$LOCAL_DRY_RUN_FIXTURE" add Cargo.toml scripts
git -C "$LOCAL_DRY_RUN_FIXTURE" -c user.name=release-test -c user.email=release-test@example.invalid \
  commit -qm 'fixture'
fixture_head="$(git -C "$LOCAL_DRY_RUN_FIXTURE" rev-parse HEAD)"
if local_dry_run_out="$(bash "$LOCAL_DRY_RUN_FIXTURE/scripts/release/local-dry-run.sh" 2>&1)"; then
  fail "local release rehearsal fixture should stop at its sentinel gate"
elif [ -f "$LOCAL_DRY_RUN_FIXTURE/candidate-commit" ] \
  && [ "$(cat "$LOCAL_DRY_RUN_FIXTURE/candidate-commit")" = "$fixture_head" ] \
  && [ "$local_dry_run_out" = "$local_dry_run_out" ]; then
  pass "local release rehearsal reaches dispatch gate with its immutable candidate commit"
else
  fail "local release rehearsal must pass its immutable candidate commit to the dispatch gate; got: $local_dry_run_out"
fi

if [ "$(grep -c -- '--allow-dirty' "$local_dry_run" || true)" -eq 2 ]; then
  pass "local release rehearsal can package the uncommitted version-bump candidate"
else
  fail "local release rehearsal must use --allow-dirty for its local package and publish dry-runs"
fi

# v0.8.20 registry recovery is deliberately partial: a dispatch may republish
# the immutable tag's crates.io and PyPI artifacts while NPM remains untouched.
# Keep this mode opt-in and version-bound so it cannot silently weaken a later
# release's all-registry completion contract.
if grep -A3 '^      recovery_skip_npm:$' "$release" | grep -q 'type: boolean' \
  && grep -A4 '^      recovery_skip_npm:$' "$release" | grep -q 'default: false' \
  && grep -Fq 'name: Validate v0.8.20 recovery scope' "$release" \
  && grep -Fq '[ "$RELEASE_TAG" != "v0.8.20" ]' "$release"; then
  pass "recovery dispatch opt-in is bound to v0.8.20"
else
  fail "recovery dispatch must be an explicit v0.8.20-only opt-in"
fi

job_block() {
  awk -v job="$1" '
    $0 == "  " job ":" { in_job = 1; next }
    in_job && /^  [[:alnum:]_-]+:$/ { exit }
    in_job { print }
  ' "$release"
}

build_napi_block="$(job_block build-napi)"
all_builds_block="$(job_block all-builds-passed)"
t1_block="$(job_block publish-rust-t1-embedder-api)"
recovery_dispatch_expr="github.event_name == 'workflow_dispatch' && inputs.recovery_skip_npm == true && inputs.release_version == '0.8.20'"
if grep -Fq "if: \${{ !($recovery_dispatch_expr) }}" <<<"$build_napi_block" \
  && grep -Fq 'always()' <<<"$all_builds_block" \
  && grep -Fq "needs.verify-release.result == 'success'" <<<"$all_builds_block" \
  && grep -Fq "needs.build-python.result == 'success'" <<<"$all_builds_block" \
  && grep -Fq "needs.build-rust.result == 'success'" <<<"$all_builds_block" \
  && grep -Fq "$recovery_dispatch_expr) || needs.build-napi.result == 'success'" <<<"$all_builds_block" \
  && grep -Fq 'all-builds-passed' <<<"$t1_block"; then
  pass "recovery excludes the N-API build without weakening Python/Rust publish gating"
else
  fail "recovery graph must bypass only N-API while T1 stays gated on verify, Python, and Rust"
fi

npm_platform_block="$(job_block publish-npm-platform-linux-x64-gnu)"
npm_platform_arm_block="$(job_block publish-npm-platform-linux-arm64-gnu)"
npm_main_block="$(job_block publish-npm)"
if grep -Fq "if: \${{ !($recovery_dispatch_expr) }}" <<<"$npm_platform_block" \
  && grep -Fq "if: \${{ !($recovery_dispatch_expr) }}" <<<"$npm_platform_arm_block" \
  && grep -Fq "if: \${{ !($recovery_dispatch_expr) }}" <<<"$npm_main_block"; then
  pass "recovery dispatch explicitly skips both platform and main npm publish jobs"
else
  fail "recovery dispatch must skip both platform and main npm publish jobs"
fi

smoke_block="$(job_block post-publish-smoke)"
if grep -Fq 'needs.publish-rust-t7-cli.result == '\''success'\''' <<<"$smoke_block" \
  && grep -Fq 'needs.publish-pypi.result == '\''success'\''' <<<"$smoke_block" \
  && grep -Fq "$recovery_dispatch_expr) || needs.publish-npm.result == 'success'" <<<"$smoke_block" \
  && grep -Fq "fromJSON(($recovery_dispatch_expr) && '[\"crates-cli\",\"pypi-wheel\"]' || '[\"crates-cli\",\"pypi-wheel\",\"npm-package\"]')" <<<"$smoke_block"; then
  pass "recovery keeps crates and PyPI smokes while omitting npm smoke"
else
  fail "recovery dispatch must not let skipped npm block crates/PyPI smoke"
fi

co_tagging_block="$(job_block co-tagging-assert)"
github_release_block="$(job_block github-release)"
partial_record_block="$(job_block record-v0820-partial-registry-recovery)"
if grep -Fq "if: \${{ inputs.dry_run != true && !($recovery_dispatch_expr) }}" <<<"$co_tagging_block" \
  && grep -Fq "if: \${{ inputs.dry_run != true && !($recovery_dispatch_expr) }}" <<<"$github_release_block" \
  && grep -Fq "$recovery_dispatch_expr" <<<"$partial_record_block" \
  && grep -Fq 'v0.8.20 partial registry recovery' <<<"$partial_record_block" \
  && grep -Fq 'GITHUB_STEP_SUMMARY' <<<"$partial_record_block"; then
  pass "recovery skips npm-dependent finalization and records partial state"
else
  fail "recovery dispatch must record its partial state instead of completing the release"
fi

if [ "$FAILED" -gt 0 ]; then
  exit 1
fi
