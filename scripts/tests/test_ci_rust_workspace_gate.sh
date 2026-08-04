#!/usr/bin/env bash
# Behavioral workflow guard for the temporary serial Rust-workspace gate.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
WORKFLOW="$REPO_ROOT/.github/workflows/ci.yml"
CHECK_SCRIPT="$REPO_ROOT/scripts/check.sh"
AGENT_TEST="$REPO_ROOT/scripts/agent-test.sh"
NODE_BIN="${NODE_BIN:-node}"
YAML_MODULE="${YAML_MODULE:-$REPO_ROOT/node_modules/js-yaml}"

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

TMPROOT="$(mktemp -d)"
cleanup() {
  case "$TMPROOT" in
    "${TMPDIR:-/tmp}"/*|/tmp/*) rm -rf "$TMPROOT" ;;
    *) printf 'refusing to remove unexpected temp path: %s\n' "$TMPROOT" >&2 ;;
  esac
}
trap cleanup EXIT

if [ ! -f "$WORKFLOW" ] || [ ! -f "$CHECK_SCRIPT" ] || [ ! -f "$AGENT_TEST" ]; then
  fail "required CI control file is missing"
  exit 1
fi
if ! command -v "$NODE_BIN" >/dev/null 2>&1 || [ ! -d "$YAML_MODULE" ]; then
  fail "declared js-yaml tooling is unavailable; run bash scripts/bootstrap.sh"
  exit 1
fi
if ! command -v actionlint >/dev/null 2>&1; then
  fail "actionlint is unavailable; run bash scripts/bootstrap.sh"
  exit 1
fi
if actionlint "$WORKFLOW"; then
  pass "actionlint accepts the real workflow"
else
  fail "actionlint rejects the real workflow"
  exit 1
fi

write_contract() {
  local workflow_path="$1" output_path="$2"
  "$NODE_BIN" - "$workflow_path" "$YAML_MODULE" >"$output_path" <<'JS'
const fs = require("fs");
const [workflowPath, yamlModule] = process.argv.slice(2);
const workflow = require(yamlModule).load(fs.readFileSync(workflowPath, "utf8"));
const jobs = workflow.jobs;

function steps(name) { return (jobs[name] || {}).steps || []; }
function runs(name) { return steps(name).filter((step) => step.run).map((step) => step.run); }
function runStep(name, needle) {
  return steps(name).find((step) => step.run && step.run.includes(needle));
}
function directWorkspaceCargo(name) {
  return runs(name).some((body) => body.includes("cargo test --workspace"));
}

const reporter = jobs["rust-workspace-race-report"] || {};
const reporterSteps = reporter.steps || [];
const reporterStep = reporterSteps.find((step) => step.run && step.run.includes("--parallel-report"));
const reporterRun = reporterStep ? reporterStep.run : null;
const upload = reporterSteps.find((step) => (step.uses || "").startsWith("actions/upload-artifact@"));
const artifactTemplate = "rust-workspace-parallel-${{ github.run_id }}-${{ github.run_attempt }}";
const artifactAssignment = 'artifact_name="rust-workspace-parallel-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}"';

console.log(JSON.stringify({
  verify_indirect: runs("verify").some((body) => body.includes("bash scripts/agent-verify.sh")),
  deferred_native_jobs_absent: !jobs["rust-windows"] && !jobs["rust-macos"],
  gating_direct_workspace_cargo: ["verify"].some(directWorkspaceCargo),
  reporter_linux: reporter["runs-on"] === "ubuntu-latest",
  reporter_timeout: Boolean(reporter["timeout-minutes"]),
  reporter_run: reporterRun,
  upload_always: Boolean(upload && upload.if === "always()"),
  upload_pinned: Boolean(upload && upload.uses === "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a"),
  upload_exact_log: Boolean(upload && upload.with && upload.with.path === "${{ runner.temp }}/rust-workspace-parallel.log"),
  upload_artifact_template: Boolean(upload && upload.with && upload.with.name === artifactTemplate),
  warning_names_dynamic_artifact: Boolean(reporterRun && reporterRun.includes(artifactAssignment)
    && reporterRun.includes("artifact=${artifact_name}")),
}, null, 2));
JS
}

CONTRACT="$TMPROOT/workflow-contract.json"
if ! write_contract "$WORKFLOW" "$CONTRACT"
then
  fail "ci.yml did not parse with declared js-yaml tooling"
  exit 1
fi

contract_value_from() {
  "$NODE_BIN" - "$1" "$2" <<'JS'
const fs = require("fs");
const [contractPath, key] = process.argv.slice(2);
const value = JSON.parse(fs.readFileSync(contractPath, "utf8"))[key];
process.stdout.write(value === null || value === undefined ? "None" : String(value));
JS
}

contract_value() { contract_value_from "$CONTRACT" "$1"; }

assert_true() {
  local key="$1" description="$2"
  if [ "$(contract_value "$key")" = "true" ]; then
    pass "$description"
  else
    fail "$description"
  fi
}

assert_true verify_indirect "verify retains agent-verify as its indirect serial route"
assert_true deferred_native_jobs_absent "deferred macOS/Windows jobs are absent from 0.8.20 CI"
if [ "$(contract_value gating_direct_workspace_cargo)" = "false" ]; then
  pass "the parsed Linux gating Rust-workspace job avoids direct Cargo"
else
  fail "the parsed Linux gating Rust-workspace job retains a direct cargo test --workspace call"
fi
assert_true reporter_linux "parallel reporter is a distinct Linux job"
assert_true reporter_timeout "parallel reporter has a finite timeout"
assert_true upload_always "parallel reporter uploads its log with if: always()"
assert_true upload_pinned "parallel reporter pins actions/upload-artifact"
assert_true upload_exact_log "parallel reporter uploads the complete reporter log"
assert_true upload_artifact_template "parallel reporter upload has the reviewed dynamic artifact name"
assert_true warning_names_dynamic_artifact "parallel warning names its exact dynamic artifact"

if grep -qE '^[[:space:]]*AGENT_LONG=1[[:space:]]+bash[[:space:]]+scripts/test-rust-workspace\.sh[[:space:]]+--serial$' "$CHECK_SCRIPT" \
  && ! grep -q 'cargo test --workspace' "$CHECK_SCRIPT"; then
  pass "check.sh retains long-test coverage through the canonical serial runner"
else
  fail "check.sh must route its long Rust workspace test through --serial without direct Cargo"
fi
if grep -qE '^run_tier_suite heavy test-rust bash scripts/test-rust-workspace\.sh --serial$' "$AGENT_TEST" \
  && ! grep -q 'cargo test --workspace' "$AGENT_TEST"; then
  pass "agent-test heavy tier has no direct workspace Cargo invocation"
else
  fail "agent-test must register workspace Rust tests in the heavy tier through the canonical runner"
fi

MUTATED_WORKFLOW="$TMPROOT/verify-direct-cargo.yml"
"$NODE_BIN" - "$WORKFLOW" "$YAML_MODULE" "$MUTATED_WORKFLOW" <<'JS'
const fs = require("fs");
const [source, yamlModule, destination] = process.argv.slice(2);
const yaml = require(yamlModule);
const workflow = yaml.load(fs.readFileSync(source, "utf8"));
workflow.jobs.verify.steps.push({name: "mutation", run: "cargo test --workspace"});
fs.writeFileSync(destination, yaml.dump(workflow));
JS
MUTATED_CONTRACT="$TMPROOT/verify-direct-cargo-contract.json"
if write_contract "$MUTATED_WORKFLOW" "$MUTATED_CONTRACT" \
  && [ "$(contract_value_from "$MUTATED_CONTRACT" gating_direct_workspace_cargo)" = "true" ]; then
  pass "parsed workflow guard rejects a direct workspace Cargo call injected into verify"
else
  fail "parsed workflow guard must detect a direct workspace Cargo call injected into verify"
fi

REPORTER_BODY="$(contract_value reporter_run)"
if [ -z "$REPORTER_BODY" ] || [ "$REPORTER_BODY" = 'None' ]; then
  fail "parallel reporter shell body is missing"
else
  REAL_BASH="$(command -v bash)"
  FAKE_BIN="$TMPROOT/bin"
  RUNNER_TEMP="$TMPROOT/runner-temp"
  SUMMARY="$TMPROOT/summary.md"
  mkdir -p "$FAKE_BIN" "$RUNNER_TEMP"
  cat >"$FAKE_BIN/bash" <<EOF
#!$REAL_BASH
if [ "\$1" = "scripts/test-rust-workspace.sh" ] && [ "\$2" = "--parallel-report" ]; then
  printf 'known stdout\\n'
  printf 'known stderr\\n' >&2
  exit 73
fi
exec "$REAL_BASH" "\$@"
EOF
  chmod +x "$FAKE_BIN/bash"
  BODY="$TMPROOT/reporter-body.sh"
  printf '%s\n' "$REPORTER_BODY" >"$BODY"

  set +e
  PATH="$FAKE_BIN:$PATH" RUNNER_TEMP="$RUNNER_TEMP" GITHUB_STEP_SUMMARY="$SUMMARY" \
    GITHUB_SHA=deadbeef GITHUB_RUN_ID=99 GITHUB_RUN_ATTEMPT=7 "$REAL_BASH" "$BODY" >"$TMPROOT/reporter.out" 2>&1
  BODY_RC=$?
  set -e
  LOG_PATH="$RUNNER_TEMP/rust-workspace-parallel.log"
  if [ "$BODY_RC" -eq 0 ] && grep -q 'exit_code=73' "$SUMMARY" \
    && grep -q 'artifact_name=rust-workspace-parallel-99-7' "$SUMMARY" \
    && grep -q 'artifact=rust-workspace-parallel-99-7' "$TMPROOT/reporter.out" \
    && grep -q 'known stdout' "$LOG_PATH" && grep -q 'known stderr' "$LOG_PATH"; then
    pass "executed reporter records raw failure and names the uploaded artifact before returning zero"
  else
    fail "reporter execution did not preserve diagnostic semantics (rc=$BODY_RC, summary=$(cat "$SUMMARY" 2>/dev/null || true), output=$(cat "$TMPROOT/reporter.out"))"
  fi

  set +e
  PATH="$FAKE_BIN:$PATH" RUNNER_TEMP="$RUNNER_TEMP" GITHUB_STEP_SUMMARY="$TMPROOT/missing/summary.md" \
    GITHUB_SHA=deadbeef GITHUB_RUN_ID=99 GITHUB_RUN_ATTEMPT=7 "$REAL_BASH" "$BODY" >"$TMPROOT/summary-failure.out" 2>&1
  SUMMARY_FAILURE_RC=$?
  set -e
  if [ "$SUMMARY_FAILURE_RC" -ne 0 ]; then
    pass "reporter fails when its summary cannot be written after raw Cargo capture"
  else
    fail "reporter must not hide a summary-write failure after raw Cargo capture"
  fi
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nall CI Rust-workspace gate tests passed\n'
