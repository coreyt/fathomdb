#!/usr/bin/env bash
# Run unit tests across language surfaces.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/agent-output.sh
. "$SCRIPT_DIR/lib/agent-output.sh"
cd_repo_root

# Scripts (bash): set-version.sh two-axis enforcement.
run_capped test-set-version bash scripts/tests/test_set_version.sh

# Scripts (bash): release-time preflight (tag/--check-files/CHANGELOG/metadata).
run_capped test-verify-release-gates bash scripts/tests/test_verify_release_gates.sh

# Scripts (bash): sibling-package co-tagging assert (AC-052). Offline via
# python3 -m http.server fixture; never hits crates.io.
run_capped test-assert-co-tagging bash scripts/tests/test_assert_co_tagging.sh

# Scripts (bash): Axis-E published-API drift guard (prevents the v0.8.9
# partial-publish — embedder-api surface moved without an Axis-E bump).
# Offline via a fixture http router; never hits crates.io.
run_capped test-embedder-api-no-drift bash scripts/tests/test_verify_embedder_api_no_drift.sh

# Scripts (bash): structural shape of the post-publish smoke scripts.
# NOT integration — see test header for why behavior is exercised at tag
# time by the release workflow, not here.
run_capped test-smoke-scripts bash scripts/tests/test_smoke_scripts.sh

# Scripts (bash): 0.8.18 Slice 20 (#11-full publish) — static release.yml scope
# assertions (matrix gated to x86_64-linux, tiered ordering, non-latest npm
# dist-tag). Pure python3+PyYAML parse; never runs the workflow.
run_capped test-release-workflow-scope bash scripts/tests/test_release_workflow_scope.sh

# Scripts (bash): coordinated-publish resilience (R-REL-4c) — per-registry
# idempotent no-op across crates.io + npm + PyPI. Offline fixture http server.
run_capped test-idempotent-republish bash scripts/tests/test_idempotent_republish.sh

# Scripts (bash): poll-for-resolvability guard that replaced the fixed 60s
# index-propagation sleep (R-REL-4c). Offline fixture http server.
run_capped test-wait-for-crate-version bash scripts/tests/test_wait_for_crate_version.sh

# Scripts (bash): publish-time npm optionalDependencies injection (R-REL-4f) —
# napi per-platform split. Pure filesystem fixture; no registry.
run_capped test-npm-inject-optional-deps bash scripts/tests/test_npm_inject_optional_deps.sh

# actionlint binary present + rejects deliberately-broken fixture.
run_capped test-actionlint-fixture bash scripts/tests/test_actionlint_fixture.sh

# Markdown generators (shell): context-clarity.sh / memory-clarity.sh emit
# gate-compliant markdown. Their output trees (and the dev/plans/runs/** reports
# from the Python generators) are markdownlint-ignored, so the normal md gate never
# sees a regenerated report. The Python generators (aggregate / m1_verdict_run /
# s15a_embedder_probe) are guarded by src/python/tests/test_md_generator_hygiene.py
# in the pytest step below.
run_capped test-md-generators bash scripts/tests/test_md_generators.sh

# AC-051a / AC-051b: cross-ecosystem version-skew resolver fixtures.
run_capped test-cargo-skew bash dev/release/tests/cargo_skew.sh
run_capped test-pip-skew bash dev/release/tests/pip_skew.sh

# Rust
run_capped test-rust cargo test --workspace --quiet --no-fail-fast

# Python
python_bin=""
if [ -x .venv/bin/python ]; then
  python_bin=".venv/bin/python"
elif command -v python3 >/dev/null 2>&1; then
  python_bin="$(command -v python3)"
fi

if [ -n "$python_bin" ] && "$python_bin" -c 'import pytest' >/dev/null 2>&1 && [ -d src/python/tests ]; then
  run_capped test-python "$python_bin" -m pytest -q src/python/tests
else
  skip_notice test-python "pytest not installed or no tests dir"
fi

# TypeScript
if [ -d src/ts/node_modules ]; then
  run_capped test-ts bash -c 'cd src/ts && npm test --silent'
else
  skip_notice test-ts "src/ts/node_modules not installed"
fi
