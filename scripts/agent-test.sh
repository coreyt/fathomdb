#!/usr/bin/env bash
# Run unit tests across language surfaces.
#
# COLLECT-ALL-THEN-REPORT (0.8.20 R-20-HARNESS, "Slice 39.5"): this script
# runs EVERY registered suite regardless of individual failures, then prints
# a summary and exits non-zero iff any suite failed. See
# scripts/lib/agent-suite-run.sh for the recording wrapper and its header
# for why it is a separate file from scripts/lib/agent-output.sh (run_capped
# itself is unchanged and still fail-fast for its four other callers).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/agent-output.sh
. "$SCRIPT_DIR/lib/agent-output.sh"
# shellcheck source=lib/agent-suite-run.sh
. "$SCRIPT_DIR/lib/agent-suite-run.sh"
# shellcheck source=lib/agent-python-env.sh
. "$SCRIPT_DIR/lib/agent-python-env.sh"
cd_repo_root
use_checkout_venv_python_path "$PWD"

# ---------------------------------------------------------------------------
# Arg parsing — BEFORE anything runs. Supports ONLY --exclude-suite=LABEL,
# repeatable. This is a demonstration/debugging flag for proving the
# collect-all harness's non-vacuous zero-exit direction (excluding
# already-owned red suites to show a clean tree exits 0) — it is NEVER read
# from an environment variable, and it is NEVER a default: every ordinary
# invocation (`bash scripts/agent-test.sh`, no args) runs every suite.
# ---------------------------------------------------------------------------
usage() {
  cat >&2 <<'USAGE'
Usage: agent-test.sh [--exclude-suite=LABEL ...]

  --exclude-suite=LABEL   Exclude the suite registered under LABEL from this
                          run. Repeatable. A demonstration/debugging flag
                          only, NEVER read from an environment variable and
                          NEVER a default — every ordinary run executes
                          every registered suite.

No positional arguments are accepted.
USAGE
}

_exclude_args=()
for _arg in "$@"; do
  case "$_arg" in
    --exclude-suite=*)
      _val="${_arg#--exclude-suite=}"
      if [ -z "$_val" ]; then
        usage
        exit 2
      fi
      _exclude_args+=("$_val")
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done
unset _arg _val

for _label in "${_exclude_args[@]:-}"; do
  [ -z "$_label" ] && continue
  exclude_suite "$_label"
done
unset _label

if [ "${#_exclude_args[@]}" -gt 0 ]; then
  printf '=== EXCLUDED SUITES: %s ===\n' "${_exclude_args[*]}"
  printf 'EXCLUDED suites were NOT run -- this exit code is NOT a full-tree result.\n'
fi

# Scripts (bash): set-version.sh two-axis enforcement.
run_suite test-set-version bash scripts/tests/test_set_version.sh

# Scripts (bash): release-cut fields deliberately outside set-version.sh.
run_suite test-release-version-surfaces bash scripts/tests/test_release_version_surfaces.sh
run_suite test-platform-capabilities bash scripts/tests/test_platform_capabilities.sh
run_suite test-public-doc-truth bash scripts/tests/test_public_doc_truth.sh

# Scripts (bash): 0.8.20 R-20-HARNESS ("Slice 39.5", no ladder slot) —
# recurrence guard for the collect-all conversion of THIS script: proves
# run_suite/skip_suite record all four states (PASS/FAIL/SKIP/EXCL), a crash
# is FAIL never skipped, the summary names every failure, run_capped's
# return contract is untouched, agent-suite-run.sh is sourced by no other
# script, and the arg-parse usage errors above exit 2 before any suite
# runs. RED-first fixtures + real-entry-point arg-parse arms; nothing here
# writes into this checkout, and no full agent-test.sh run is ever driven
# to completion by this suite.
run_suite test-agent-test-collect-all bash scripts/tests/test_agent_test_collect_all.sh

# Scripts (bash): release-time preflight (tag/--check-files/CHANGELOG/metadata).
run_suite test-verify-release-gates bash scripts/tests/test_verify_release_gates.sh

# Exact Rust, npm, actionlint, and dispatch-tag alignment between local
# prework and the release workflow.
run_suite test-runtime-release-alignment bash scripts/tests/test_runtime_release_alignment.sh

# Scripts (bash): offline fake-Cargo coverage for every release Rust tier. The
# helper executes Cargo dry-runs for the three leaf crates and explicitly skips
# the four dependent crates whose sibling registry dependencies cannot resolve
# until real preceding tiers publish.
run_suite test-cargo-publish-if-new bash scripts/tests/test_cargo_publish_if_new.sh

# Scripts (bash): 0.8.20 Slice 39 (R-20-DOC) — the license type + license-
# SHIPPING gate (scripts/check-license-consistency.sh). Closes an 0.8.x-long
# silent drift: the repo-root LICENSE said MIT, all four publishable manifests
# said Apache-2.0, and NO artifact carried a license file at all — measured with
# `cargo package --list` and `npm pack --dry-run`. crates.io versions are
# IMMUTABLE, so tagging that would have been unfixable rather than merely wrong.
# RED-first fixtures under mktemp -d plus two real-repo regression arms; no real
# manifest, LICENSE or lockfile is ever written.
#
# ⚠ HISTORICAL NOTE — why this entry sits here, not at the end of the
# `scripts/` block. Before 0.8.20 R-20-HARNESS, this file ran under
# `set -euo pipefail` with BARE `run_capped` calls and aborted at the FIRST
# failing suite. `test-check-governed-surface-pin` (below) FAILS on the tree
# as of 0.8.20 Slice 39 — its pin's `git_blob_sha1` provenance claim is
# stale — so everything registered after it was UNREACHABLE and would have
# been a vacuous pass; this entry had to sit ahead of that abort point to
# ever run at all.
#
# That abort is GONE: every suite now runs through `run_suite`
# (scripts/lib/agent-suite-run.sh), which records each outcome and always
# returns 0, so `set -e` can no longer stop the run partway through — a
# failing test-check-governed-surface-pin no longer hides anything
# downstream (test-agent-test-collect-all, right above, proves this: it is
# registered after several suites and asserts it actually ran). Registration
# order is therefore now about REPORT READABILITY (grouping related suites,
# reading top-to-bottom in the summary), not REACHABILITY. This entry is
# left in its historical position rather than moved, since moving it is not
# required by anything and the note above records why it was here.
#
# CI wiring (.github/) is Slice 40's exclusive territory this release and has
# been handed to it explicitly.
run_suite test-check-license-consistency bash scripts/tests/test_check_license_consistency.sh

# Scripts (bash): TC-RUBRIC-5 landing guard — preflight.sh --landing must HARD-fail
# in the primary checkout and pass in a linked worktree. Builds its own throwaway
# repo + worktree under mktemp -d; never git-writes into this checkout.
run_suite test-preflight-landing bash scripts/tests/test_preflight_landing.sh

# Scripts (bash): status-board-currency-enforcement items 2+3 — the shared
# scripts/check-board-currency.sh predicate plus its --landing wiring in
# preflight.sh. Builds its own throwaway repos + worktrees under mktemp -d;
# never git-writes into this checkout.
run_suite test-check-board-currency bash scripts/tests/test_check_board_currency.sh

# Scripts (bash): DOC-HYGIENE-2 T1b — the shared scripts/check-ledgers.sh
# predicate (sidecar == max(seq); seq contiguous), its --landing wiring in
# preflight.sh, and a static assertion that its CI job is always-on. Fixture
# roots are plain dirs under mktemp -d (plus throwaway git repos for the
# preflight arms); no real .jsonl / .jsonl.seq is ever touched.
run_suite test-check-ledgers bash scripts/tests/test_check_ledgers.sh

# Scripts (bash): DOC-HYGIENE-3 TC-88 — scripts/check-staged-ledger-sidecars.sh,
# the COMMIT-TIME half of the same invariant. check-ledgers.sh reads the WORKING
# TREE, which is consistent on the machine that stages a `.jsonl` without its
# `.seq` sidecar — only the COMMIT is torn, so both existing homes went green for
# the author and red for everyone downstream (41a81c17, 3e660f95). This gate
# reads the INDEX and reuses check-ledgers.sh --root over a materialised staged
# tree, so the two predicates cannot diverge. Fixtures are throwaway git repos
# under mktemp -d; no real ledger is ever touched.
run_suite test-staged-ledger-sidecar bash scripts/tests/test_staged_ledger_sidecar.sh

# Scripts (bash): DOC-HYGIENE-2 T1e — the shared scripts/check-governed-surface-pin.sh
# predicate (content hash + member lists + counts + REQ-054 against
# scripts/governed-surface-pin.json), its --landing wiring in preflight.sh, and a
# static assertion that its CI job is always-on. Fixtures are COPIES of the
# allowlist under mktemp -d (plus throwaway git repos for the preflight arms);
# src/conformance/governed-surface-allowlist.json is never written.
run_suite test-check-governed-surface-pin bash scripts/tests/test_check_governed_surface_pin.sh

# 0.8.21 Slice 40: an offline GitHub-advisory snapshot and recorded
# no-override evidence make a root npm override fail the moment it becomes
# vulnerable, obsolete, or undocumented. The fixture includes the historical
# js-yaml@4.2.0 regression and malformed-input fail-closed arms.
run_suite test-check-pinned-override-rot bash scripts/tests/test_check_pinned_override_rot.sh

# Scripts (bash): R-20-H7 — the shared scripts/check-c1-conformance.sh predicate
# (contract content pin + clause-registry bijection + pinned counts + the 26
# CHECKABLE clause assertions against as-built code), its --landing wiring in
# preflight.sh, and a static assertion that its CI job is always-on. Fixtures are
# COPIES of the contract, of the pin and of the source root under mktemp -d (plus
# throwaway git repos for the preflight arms); neither the ratified contract nor
# the real src/ tree is ever written.
run_suite test-check-c1-conformance bash scripts/tests/test_check_c1_conformance.sh

# Scripts (bash): TC-86 transcript hygiene — the ONE shared agent-state pattern
# (scripts/lib/agent-state-paths.sh), the capture-time filter folded into
# dev/agent-tools/codex-nostdin.sh, and the shared
# scripts/check-transcript-hygiene.sh predicate with both of its wirings
# (`preflight.sh --landing` and an always-on CI job). Guards against a codex §9
# transcript carrying another project's raw Claude Code session JSONL into this
# PUBLIC repo, which happened once (caught pre-land; reachability in history is
# ZERO). Dirty fixtures and throwaway repos live under mktemp -d; no real
# transcript under dev/plans/runs/** is ever written.
run_suite test-check-transcript-hygiene bash scripts/tests/test_check_transcript_hygiene.sh

# Scripts (bash): agent-seat-hardening ASH-B — the PreToolUse write-path guard
# .claude/hooks/seat-path-guard.sh, which enforces dev/design/orchestration.md
# § 1.2 (coordinating seats must never write src/**, engine/** or test sources;
# the implementer seat must never be blocked). The hook ships UNWIRED and arm 16
# asserts that; wiring it is an HITL-gated Phase-2 act. Pure stdin->stdout
# function under test — the suite feeds synthetic PreToolUse payloads and never
# touches the filesystem beyond one mktemp -d fixture.
run_suite test-seat-path-guard bash scripts/tests/test_seat_path_guard.sh

# Scripts (bash): sibling-package co-tagging assert (AC-052). Offline via
# python3 -m http.server fixture; never hits crates.io.
run_suite test-assert-co-tagging bash scripts/tests/test_assert_co_tagging.sh

# Scripts (bash): Axis-E published-API drift guard (prevents the v0.8.9
# partial-publish — embedder-api surface moved without an Axis-E bump).
# Offline via a fixture http router; never hits crates.io.
run_suite test-embedder-api-no-drift bash scripts/tests/test_verify_embedder_api_no_drift.sh

# Scripts (bash): structural shape of the post-publish smoke scripts.
# NOT integration — see test header for why behavior is exercised at tag
# time by the release workflow, not here.
run_suite test-smoke-scripts bash scripts/tests/test_smoke_scripts.sh

# Scripts (bash): 0.8.18 Slice 20 (#11-full publish) — static release.yml scope
# assertions (matrix gated to x86_64-linux, tiered ordering, non-latest npm
# dist-tag). Pure python3+PyYAML parse; never runs the workflow.
run_suite test-release-workflow-scope bash scripts/tests/test_release_workflow_scope.sh

# Slice 40 / seq-234: Linux x86_64 is the 0.8.20 native-artifact scope.
# Static assertions here complement actionlint's workflow syntax/schema check.
run_suite test-linux-first-platform-scope bash scripts/tests/test_linux_first_platform_scope.sh
run_suite test-linux-aarch64-release-artifacts bash scripts/tests/test_linux_aarch64_release_artifacts.sh

# Slice 40: the CI verify job must leave enough time for its clean bootstrap
# plus the same full agent-verify gate required locally.
run_suite test-verify-ci-timeout-budget bash scripts/tests/test_verify_ci_timeout_budget.sh

# Slice 40: generic TypeScript prework skips only the seven network-gated arms;
# the warmed default-embedder CI job must own their complementary live and
# release-surface coverage.
run_suite test-ts-cache-coverage-split bash scripts/tests/test_ts_cache_coverage_split.sh

# Scripts (bash): coordinated-publish resilience (R-REL-4b/4c) — REAL npm
# local-registry round-trip (publish -> query-no-op -> install -> loader) +
# crates.io SIMULATED (real crates registry infeasible in-harness). node-only.
run_suite test-idempotent-republish bash scripts/tests/test_idempotent_republish.sh

# Scripts (bash): REAL PyPI round-trip (R-REL-4b) — genuine twine upload to a
# minimal local index -> query-sees-it -> re-run no-op. Self-provisions twine<6
# (twine 6 blocks --skip-existing on non-prod repos); SKIPS loudly if it cannot.
run_suite test-pypi-publish-roundtrip bash scripts/tests/test_pypi_publish_roundtrip.sh

# Scripts (bash): Fix-1 publish-registry SAFETY — a staging/test run can never
# publish to prod (npm publish --registry $BASE; twine upload --repository-url).
run_suite test-publish-registry-safety bash scripts/tests/test_publish_registry_safety.sh

# Scripts (bash): poll-for-resolvability guard that replaced the fixed 60s
# index-propagation sleep (R-REL-4c). Offline fixture http server.
run_suite test-wait-for-crate-version bash scripts/tests/test_wait_for_crate_version.sh

# Scripts (bash): publish-time npm optionalDependencies injection (R-REL-4f) —
# napi per-platform split. Pure filesystem fixture; no registry.
run_suite test-npm-inject-optional-deps bash scripts/tests/test_npm_inject_optional_deps.sh

# actionlint binary present + rejects deliberately-broken fixture.
run_suite test-actionlint-fixture bash scripts/tests/test_actionlint_fixture.sh

# Go-installed actionlint prefixes its exact version with `v`; bootstrap and
# agent-lint must normalize that conventional form without accepting drift.
run_suite test-actionlint-go-install-version bash scripts/tests/test_actionlint_go_install_version.sh

# Local lint preflight must use CI's exact Ruff version rather than reporting a
# false green from an older environment. Fixture provides only a stale Ruff and
# asserts the wrapper fails before attempting any other lint leg.
run_suite test-agent-lint-ruff-version bash scripts/tests/test_agent_lint_ruff_version.sh
run_suite test-agent-lint-actionlint-version bash scripts/tests/test_agent_lint_actionlint_version.sh
run_suite test-agent-typecheck-pyright-version bash scripts/tests/test_agent_typecheck_pyright_version.sh
run_suite test-pyright-pin-consistency bash scripts/tests/test_pyright_pin_consistency.sh
run_suite test-ci-run-hygiene bash scripts/tests/test_ci_run_hygiene.sh
run_suite test-ci-run-hygiene-ci-env bash scripts/tests/test_ci_run_hygiene_ci_env.sh

# Shell lint (0.8.21 Slice 30). The version guard mirrors the ruff/actionlint
# ones; the gate suite is the red-first proof that agent-lint.sh rejects the
# masked-return and early-exiting-consumer shapes and that both ratchets can
# only shrink.
run_suite test-agent-lint-shellcheck-version bash scripts/tests/test_agent_lint_shellcheck_version.sh
run_suite test-agent-lint-shellcheck-gate bash scripts/tests/test_agent_lint_shellcheck_gate.sh
run_suite test-install-shellcheck bash scripts/tests/test_install_shellcheck.sh

# Shell lint in CI (0.8.21 Slice 35). Pins the `shell-lint` job's ALWAYS-ON shape
# (no if:/needs:), its minimal setup, that it GATES rather than advises, and the
# workflow concurrency group's `main` carve-out — plus the behavioural half: the
# job's ACTUAL command rejects the Slice 25 `cmd | head` shape, mutant-proven
# against a stub gate that accepts the same fixture.
run_suite test-shell-lint-ci-job bash scripts/tests/test_shell_lint_ci_job.sh

# CI observability (0.8.21 Slice 45): every collect-all result, including a
# green run's status/timing, reaches the GitHub step summary; failed suites
# annotate the run; and the otherwise-ephemeral spill logs are artifacted only
# after redaction. The fixture test executes the real harness and collector.
run_suite test-ci-verify-observability bash scripts/tests/test_ci_verify_observability.sh

# TC-37 recurrence guard: agent-lint-md.sh must HARD-fail (not skip_notice/exit 0)
# when markdownlint-cli2 is genuinely unresolvable. Builds its own throwaway
# fixture repo under mktemp -d; never touches this checkout's node_modules.
run_suite test-lint-md-hard-fail-on-missing-linter bash scripts/tests/test_lint_md_hard_fail_on_missing_linter.sh

# T3/9: dev/plans/*.md must carry a valid `status:` frontmatter value (recurrence
# guard for archival banners drifting silently). RED-fixture proven inline.
run_suite test-plans-status-frontmatter bash scripts/tests/test_plans_status_frontmatter.sh

# T1d: recurrence guard for the ACTIVE-plan line-anchor ban AND — the arm that
# carries the weight — for the mandatory symbol-existence check. Mutation-proven:
# stubbing the existence check to always succeed turns this suite red, so a green
# here is not vacuous. RED fixtures built inline under mktemp -d.
run_suite test-lint-plan-anchors bash scripts/tests/test_lint_plan_anchors.sh

# T2a: recurrence guard for the single-writer release-state file and its
# marker-delimited generated views — regenerate-and-diff, marker well-formedness,
# the orphan-marker confinement rule, and the TC-37 zero-blocks hard fail. RED
# fixtures built inline under mktemp -d; also asserts the CI job is always-on.
run_suite test-check-release-state-views bash scripts/tests/test_check_release_state_views.sh

# T3a: recurrence guard for the stateless Steward cold-start briefing — the
# <=4096-byte cap, "writes no file", the zero-result hard fail, the release being
# derived from the LIVE BOARD FILENAME (not a hardcoded version), the SIBLING
# <root>-worktrees/ resolution, and the board-CLOSED predicate being SHARED with
# check-board-currency.sh. Mutation-proven four ways: neutering the zero-result
# guard reddens 7 arms; narrowing the shared window to `head -n 5`, resolving the
# worktrees dir as a child, and hardcoding the release each redden their own arm.
run_suite test-steward-orient bash scripts/tests/test_steward_orient.sh

# T3b: recurrence guard for the generated commission manifest — the arms that
# carry the weight are "a cited path does not exist" and "zero citations
# emitted" (TC-37), both of which must HARD-fail rather than emit a brief with a
# dead pointer in it. Also asserts the real 0.8.20 Slice-20 manifest still
# resolves end to end. RED fixtures built inline under mktemp -d.
run_suite test-commission-manifest bash scripts/tests/test_commission_manifest.sh

# Markdown generators (shell): context-clarity.sh / memory-clarity.sh emit
# gate-compliant markdown. Their output trees (and the dev/plans/runs/** reports
# from the Python generators) are markdownlint-ignored, so the normal md gate never
# sees a regenerated report. The Python generators (aggregate / m1_verdict_run /
# s15a_embedder_probe) are guarded by src/python/tests/test_md_generator_hygiene.py
# in the pytest step below.
run_suite test-md-generators bash scripts/tests/test_md_generators.sh

# AC-051a / AC-051b: cross-ecosystem version-skew resolver fixtures.
run_suite test-cargo-skew bash dev/release/tests/cargo_skew.sh
run_suite test-pip-skew bash dev/release/tests/pip_skew.sh

# Scripts (bash): temporary TC-74 evidence control. The canonical runner owns
# the exact Cargo invocation, so the local agent loop and every gated Rust CI
# leg use the same serial mode.
run_suite test-rust-workspace-gate bash scripts/tests/test_rust_workspace_gate.sh
run_suite test-ci-rust-workspace-gate bash scripts/tests/test_ci_rust_workspace_gate.sh

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
run_suite test-aarch64-candle-feature-closure bash scripts/tests/test_aarch64_candle_feature_closure.sh
run_suite test-aarch64-candle-cpu bash scripts/tests/test_aarch64_candle_cpu.sh
run_suite test-rust bash scripts/test-rust-workspace.sh --serial

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
    run_suite test-python env FATHOMDB_TESTS_ALLOW_REBUILD=1 \
      "$python_bin" -m pytest -q src/python/tests
  else
    run_suite test-python "$python_bin" -m pytest -q src/python/tests
  fi
else
  skip_suite test-python "pytest not installed or no tests dir"
fi

# ledgerwatch (dev/agent-tools): pure-stdlib pytest suite, no fathomdb binding
# needed, so it runs under whichever interpreter was resolved above without the
# maturin-rebuild dance. Wired in by DOC-HYGIENE-2 T1b — the suite existed but
# no harness ran it, so its --project arms (fold-to-latest-per-id, and the
# "unfoldable (no id)" bucket that the deleted readme recipe crashed on) would
# otherwise never have been exercised in CI.
if [ -n "$python_bin" ] && "$python_bin" -c 'import pytest' >/dev/null 2>&1; then
  run_suite test-ledgerwatch "$python_bin" -m pytest -q dev/agent-tools/ledgerwatch
else
  skip_suite test-ledgerwatch "pytest not installed"
fi

# TypeScript
if [ -d src/ts/node_modules ]; then
  # The seven default-embedder TypeScript arms remain part of this ordinary
  # prework gate, but skip their live-model bodies here.  CI's
  # default-embedder-tests job owns the same suite after warming the BGE cache
  # and enables its release-surface arm there.
  run_suite test-ts env FATHOMDB_SKIP_NETWORK_TESTS=1 bash -c 'cd src/ts && npm test --silent'
else
  skip_suite test-ts "src/ts/node_modules not installed"
fi

# The release-surface test executes from tsc's `dist/tests` layout. Keep its
# repository-root calculation pinned independently so its opt-in CI arm cannot
# fail after the native debug build has already run.
run_suite test-release-surface-repo-root bash scripts/tests/test_release_surface_repo_root.sh
run_suite test-release-surface-native-api bash scripts/tests/test_release_surface_native_api.sh
run_suite test-ts-cache-coverage-no-rg bash scripts/tests/test_ts_cache_coverage_split_no_rg.sh

# SIGPIPE / fail-open guards over the shell gates (0.8.21 Slice 25, from
# dev/design/ci-verify-robustness-review.md §3.1.2-§3.1.3). Deterministic: each
# arm shims only the PRODUCER with a megabyte-scale one, so the pipe-buffer race
# that the real sites lose intermittently is lost every single time.
run_suite test-shell-pipefail-guards bash scripts/tests/test_shell_pipefail_guards.sh

# Collect-all summary — the deliverable. Prints every suite's outcome (full
# table on any FAIL or AGENT_VERBOSE=1; a one-line summary otherwise) and
# exits: 0 iff zero FAILs, 1 if any FAIL, 2 for a harness usage error (an
# --exclude-suite label that matched no registration). MUST be the last
# executable line — nothing registered after it could ever run.
suite_summary_and_exit
