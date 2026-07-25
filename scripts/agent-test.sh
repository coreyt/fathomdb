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

# Scripts (bash): TC-RUBRIC-5 landing guard — preflight.sh --landing must HARD-fail
# in the primary checkout and pass in a linked worktree. Builds its own throwaway
# repo + worktree under mktemp -d; never git-writes into this checkout.
run_capped test-preflight-landing bash scripts/tests/test_preflight_landing.sh

# Scripts (bash): status-board-currency-enforcement items 2+3 — the shared
# scripts/check-board-currency.sh predicate plus its --landing wiring in
# preflight.sh. Builds its own throwaway repos + worktrees under mktemp -d;
# never git-writes into this checkout.
run_capped test-check-board-currency bash scripts/tests/test_check_board_currency.sh

# Scripts (bash): DOC-HYGIENE-2 T1b — the shared scripts/check-ledgers.sh
# predicate (sidecar == max(seq); seq contiguous), its --landing wiring in
# preflight.sh, and a static assertion that its CI job is always-on. Fixture
# roots are plain dirs under mktemp -d (plus throwaway git repos for the
# preflight arms); no real .jsonl / .jsonl.seq is ever touched.
run_capped test-check-ledgers bash scripts/tests/test_check_ledgers.sh

# Scripts (bash): DOC-HYGIENE-2 T1e — the shared scripts/check-governed-surface-pin.sh
# predicate (content hash + member lists + counts + REQ-054 against
# scripts/governed-surface-pin.json), its --landing wiring in preflight.sh, and a
# static assertion that its CI job is always-on. Fixtures are COPIES of the
# allowlist under mktemp -d (plus throwaway git repos for the preflight arms);
# src/conformance/governed-surface-allowlist.json is never written.
run_capped test-check-governed-surface-pin bash scripts/tests/test_check_governed_surface_pin.sh

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

# Scripts (bash): coordinated-publish resilience (R-REL-4b/4c) — REAL npm
# local-registry round-trip (publish -> query-no-op -> install -> loader) +
# crates.io SIMULATED (real crates registry infeasible in-harness). node-only.
run_capped test-idempotent-republish bash scripts/tests/test_idempotent_republish.sh

# Scripts (bash): REAL PyPI round-trip (R-REL-4b) — genuine twine upload to a
# minimal local index -> query-sees-it -> re-run no-op. Self-provisions twine<6
# (twine 6 blocks --skip-existing on non-prod repos); SKIPS loudly if it cannot.
run_capped test-pypi-publish-roundtrip bash scripts/tests/test_pypi_publish_roundtrip.sh

# Scripts (bash): Fix-1 publish-registry SAFETY — a staging/test run can never
# publish to prod (npm publish --registry $BASE; twine upload --repository-url).
run_capped test-publish-registry-safety bash scripts/tests/test_publish_registry_safety.sh

# Scripts (bash): poll-for-resolvability guard that replaced the fixed 60s
# index-propagation sleep (R-REL-4c). Offline fixture http server.
run_capped test-wait-for-crate-version bash scripts/tests/test_wait_for_crate_version.sh

# Scripts (bash): publish-time npm optionalDependencies injection (R-REL-4f) —
# napi per-platform split. Pure filesystem fixture; no registry.
run_capped test-npm-inject-optional-deps bash scripts/tests/test_npm_inject_optional_deps.sh

# actionlint binary present + rejects deliberately-broken fixture.
run_capped test-actionlint-fixture bash scripts/tests/test_actionlint_fixture.sh

# TC-37 recurrence guard: agent-lint-md.sh must HARD-fail (not skip_notice/exit 0)
# when markdownlint-cli2 is genuinely unresolvable. Builds its own throwaway
# fixture repo under mktemp -d; never touches this checkout's node_modules.
run_capped test-lint-md-hard-fail-on-missing-linter bash scripts/tests/test_lint_md_hard_fail_on_missing_linter.sh

# T3/9: dev/plans/*.md must carry a valid `status:` frontmatter value (recurrence
# guard for archival banners drifting silently). RED-fixture proven inline.
run_capped test-plans-status-frontmatter bash scripts/tests/test_plans_status_frontmatter.sh

# T1d: recurrence guard for the ACTIVE-plan line-anchor ban AND — the arm that
# carries the weight — for the mandatory symbol-existence check. Mutation-proven:
# stubbing the existence check to always succeed turns this suite red, so a green
# here is not vacuous. RED fixtures built inline under mktemp -d.
run_capped test-lint-plan-anchors bash scripts/tests/test_lint_plan_anchors.sh

# T2a: recurrence guard for the single-writer release-state file and its
# marker-delimited generated views — regenerate-and-diff, marker well-formedness,
# the orphan-marker confinement rule, and the TC-37 zero-blocks hard fail. RED
# fixtures built inline under mktemp -d; also asserts the CI job is always-on.
run_capped test-check-release-state-views bash scripts/tests/test_check_release_state_views.sh

# T3a: recurrence guard for the stateless Steward cold-start briefing — the
# <=4096-byte cap, "writes no file", the zero-result hard fail, the release being
# derived from the LIVE BOARD FILENAME (not a hardcoded version), the SIBLING
# <root>-worktrees/ resolution, and the board-CLOSED predicate being SHARED with
# check-board-currency.sh. Mutation-proven four ways: neutering the zero-result
# guard reddens 7 arms; narrowing the shared window to `head -n 5`, resolving the
# worktrees dir as a child, and hardcoding the release each redden their own arm.
run_capped test-steward-orient bash scripts/tests/test_steward_orient.sh

# T3b: recurrence guard for the generated commission manifest — the arms that
# carry the weight are "a cited path does not exist" and "zero citations
# emitted" (TC-37), both of which must HARD-fail rather than emit a brief with a
# dead pointer in it. Also asserts the real 0.8.20 Slice-20 manifest still
# resolves end to end. RED fixtures built inline under mktemp -d.
run_capped test-commission-manifest bash scripts/tests/test_commission_manifest.sh

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
#
# TC-20 invariant: this line must NEVER reach `eu7_real_corpus_ac_validation`,
# a ~1.5h real-corpus embed measurement. Do NOT add `--all-features` here.
# Three gates keep it out, in order of what a change is most likely to break:
#   1. `required-features = ["operator"]` on the test target — not built at all
#      under the workspace default feature set (`default = []`);
#   2. file-level `#![cfg(feature = "default-embedder")]` — compiles to zero
#      tests even when `operator` IS on (e.g. the engine's operator suite);
#   3. `#[ignore]` on the test itself — holds no matter which features are
#      selected, so `--all-features` still would not run the body.
# Verify by inspection only (`-- --list --ignored`), never by running it.
run_capped test-rust cargo test --workspace --quiet --no-fail-fast

# Python
python_bin=""
if [ -x .venv/bin/python ]; then
  python_bin=".venv/bin/python"
elif command -v python3 >/dev/null 2>&1; then
  python_bin="$(command -v python3)"
fi

if [ -n "$python_bin" ] && "$python_bin" -c 'import pytest' >/dev/null 2>&1 && [ -d src/python/tests ]; then
  # TC-27 (0.8.20 Slice 5 fix-6): the editable binding built by the documented
  # `pip install -e 'src/python[dev]'` has no `test-hooks` surface, so
  # `tests/conftest.py` may rebuild it with `maturin develop` — which REBINDS the
  # active virtualenv to this source tree. This is the repo's own sanctioned dev
  # loop, so it authorizes that rebuild, but ONLY when the interpreter we picked
  # is the `.venv` INSIDE this checkout (`cd_repo_root` above, so in a linked
  # worktree that is the worktree's own venv). If we fell back to a system
  # `python3` — or to any environment that is not ours to rebind — we stay
  # silent and conftest degrades to visibly SKIPPING the hook-dependent tests
  # rather than repointing a shared venv. conftest re-checks venv ownership
  # itself; this is the outer half of a belt-and-suspenders pair.
  if [ "$python_bin" = ".venv/bin/python" ]; then
    run_capped test-python env FATHOMDB_TESTS_ALLOW_REBUILD=1 \
      "$python_bin" -m pytest -q src/python/tests
  else
    run_capped test-python "$python_bin" -m pytest -q src/python/tests
  fi
else
  skip_notice test-python "pytest not installed or no tests dir"
fi

# ledgerwatch (dev/agent-tools): pure-stdlib pytest suite, no fathomdb binding
# needed, so it runs under whichever interpreter was resolved above without the
# maturin-rebuild dance. Wired in by DOC-HYGIENE-2 T1b — the suite existed but
# no harness ran it, so its --project arms (fold-to-latest-per-id, and the
# "unfoldable (no id)" bucket that the deleted readme recipe crashed on) would
# otherwise never have been exercised in CI.
if [ -n "$python_bin" ] && "$python_bin" -c 'import pytest' >/dev/null 2>&1; then
  run_capped test-ledgerwatch "$python_bin" -m pytest -q dev/agent-tools/ledgerwatch
else
  skip_notice test-ledgerwatch "pytest not installed"
fi

# TypeScript
if [ -d src/ts/node_modules ]; then
  run_capped test-ts bash -c 'cd src/ts && npm test --silent'
else
  skip_notice test-ts "src/ts/node_modules not installed"
fi
