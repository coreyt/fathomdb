#!/usr/bin/env bash
# Behavioral recurrence guard for the 0.8.21 CI observability contract.
#
# It drives the real collect-all harness in a disposable GitHub Actions-like
# environment, runs the real spill-log collector against disposable logs, and
# parses ci.yml with the repository's declared YAML tool. No workflow is run
# and no real /tmp/fathomdb-agent-* spill log is read or written.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
WORKFLOW="$REPO_ROOT/.github/workflows/ci.yml"
SUITE_LIB="$REPO_ROOT/scripts/lib/agent-suite-run.sh"
OUTPUT_LIB="$REPO_ROOT/scripts/lib/agent-output.sh"
COLLECTOR="$REPO_ROOT/scripts/collect-agent-spill-logs.sh"
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

if [ ! -f "$WORKFLOW" ] || [ ! -f "$SUITE_LIB" ] || [ ! -f "$OUTPUT_LIB" ] || [ ! -x "$COLLECTOR" ]; then
  fail "required CI observability control file is missing"
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

CONTRACT="$TMPROOT/workflow-contract.json"
"$NODE_BIN" - "$WORKFLOW" "$YAML_MODULE" >"$CONTRACT" <<'JS'
const fs = require("fs");
const [workflowPath, yamlModule] = process.argv.slice(2);
const workflow = require(yamlModule).load(fs.readFileSync(workflowPath, "utf8"));
function verifierContract(jobName, tier, spillDir) {
  const job = workflow.jobs[jobName] || {};
  const steps = job.steps || [];
  const verifyStep = steps.find((step) => step.run
    && step.run.includes(`bash scripts/agent-verify.sh --tier=${tier}`));
  const collectStep = steps.find((step) => step.run && step.run.includes("collect-agent-spill-logs.sh"));
  const uploadStep = steps.find((step) => (step.uses || "").startsWith("actions/upload-artifact@")
    && step.with && String(step.with.name || "").startsWith("fathomdb-agent-spill-logs-"));
  return {
    tier: Boolean(verifyStep),
    verbose: Boolean(verifyStep && verifyStep.env && String(verifyStep.env.AGENT_VERBOSE) === "1"),
    collect_on_failure: Boolean(collectStep && collectStep.if === "failure()"),
    collect_uses_runner_temp: Boolean(collectStep && collectStep.run.includes(`\${RUNNER_TEMP}/${spillDir}`)),
    upload_on_failure: Boolean(uploadStep && uploadStep.if === "failure()"),
    upload_pinned: Boolean(uploadStep && uploadStep.uses === "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a"),
    upload_exact_path: Boolean(uploadStep && uploadStep.with.path === `\${{ runner.temp }}/${spillDir}`),
    upload_no_files_hard_fail: Boolean(uploadStep && uploadStep.with["if-no-files-found"] === "error"),
  };
}
console.log(JSON.stringify({
  fast: verifierContract("verify-fast", "fast", "fathomdb-agent-spill-logs-fast"),
  heavy: verifierContract("verify", "heavy", "fathomdb-agent-spill-logs"),
}, null, 2));
JS

contract_value() {
  "$NODE_BIN" - "$CONTRACT" "$1" "$2" <<'JS'
const fs = require("fs");
const [contractPath, tier, key] = process.argv.slice(2);
process.stdout.write(String(JSON.parse(fs.readFileSync(contractPath, "utf8"))[tier][key]));
JS
}

assert_true() {
  local tier="$1" key="$2" description="$3" actual
  actual="$(contract_value "$tier" "$key")" || {
    fail "$description (could not read workflow contract)"
    return
  }
  if [ "$actual" = "true" ]; then
    pass "$description"
  else
    fail "$description"
  fi
}

for verifier_tier in fast heavy; do
  assert_true "$verifier_tier" tier "$verifier_tier verifier runs its explicit tier"
  assert_true "$verifier_tier" verbose "$verifier_tier verifier enables per-suite status/timing on a green run"
  assert_true "$verifier_tier" collect_on_failure "$verifier_tier spill collection runs only after a verifier failure"
  assert_true "$verifier_tier" collect_uses_runner_temp "$verifier_tier spill collection uses runner-owned temporary storage"
  assert_true "$verifier_tier" upload_on_failure "$verifier_tier spill artifact upload runs after a verifier failure"
  assert_true "$verifier_tier" upload_pinned "$verifier_tier spill artifact upload pins actions/upload-artifact"
  assert_true "$verifier_tier" upload_exact_path "$verifier_tier spill artifact uploads only the collector output directory"
  assert_true "$verifier_tier" upload_no_files_hard_fail "$verifier_tier spill artifact upload refuses a silently empty artifact"
done

DRIVER="$TMPROOT/github-summary-driver.sh"
SUMMARY="$TMPROOT/github-step-summary.md"
cat >"$DRIVER" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
. "$OUTPUT_LIB"
. "$SUITE_LIB"
trap 'rm -f "/tmp/fathomdb-agent-observe-fail-$$.log"' EXIT
run_suite observe-pass true
run_suite observe-fail false
suite_summary_and_exit
EOF
chmod +x "$DRIVER"

set +e
HARNESS_OUT="$(OUTPUT_LIB="$OUTPUT_LIB" SUITE_LIB="$SUITE_LIB" GITHUB_ACTIONS=true \
  GITHUB_STEP_SUMMARY="$SUMMARY" bash "$DRIVER" 2>&1)"
HARNESS_RC=$?
set -e

if [ "$HARNESS_RC" -eq 1 ]; then
  pass "a failed suite still makes the real collect-all harness exit 1"
else
  fail "collect-all harness should exit 1 for a failed suite, got rc=$HARNESS_RC: $HARNESS_OUT"
fi
if grep -qF '::error title=agent-test suite failed::observe-fail (exit=1' <<<"$HARNESS_OUT"; then
  pass "a failed suite emits a GitHub error annotation with its exact label and exit"
else
  fail "missing GitHub error annotation for the failed suite: $HARNESS_OUT"
fi
if grep -qF '| observe-pass | PASS | 0 |' "$SUMMARY" \
  && grep -qF '| observe-fail | FAIL | 1 |' "$SUMMARY" \
  && grep -qF 'FAILED SUITES: observe-fail' "$SUMMARY"; then
  pass "GitHub step summary receives the full suite table and failure list"
else
  fail "GitHub step summary lacks status/timing or failure evidence: $(cat "$SUMMARY" 2>/dev/null || true)"
fi

SOURCE="$TMPROOT/spill-source"
DESTINATION="$TMPROOT/spill-destination"
mkdir -p "$SOURCE"
cat >"$SOURCE/fathomdb-agent-test-123.log" <<'EOF'
useful compiler error
PATH=/private/runner/path
GITHUB_TOKEN=secret-token
NODE_PATH: /another/private/path
{"GITHUB_TOKEN":"dummy-secret","PATH":"/private/json/path"}
another useful diagnostic
EOF
printf 'outside the allowlist\n' >"$SOURCE/unrelated.log"
ln -s /etc/passwd "$SOURCE/fathomdb-agent-link-123.log"

set +e
COLLECTOR_OUT="$(bash "$COLLECTOR" --source-dir "$SOURCE" --destination "$DESTINATION" 2>&1)"
COLLECTOR_RC=$?
set -e
COLLECTED="$DESTINATION/fathomdb-agent-test-123.log"
if [ "$COLLECTOR_RC" -eq 0 ] && [ -f "$COLLECTED" ] \
  && grep -qF 'useful compiler error' "$COLLECTED" \
  && grep -qF 'another useful diagnostic' "$COLLECTED" \
  && ! grep -qF '/private/runner/path' "$COLLECTED" \
  && ! grep -qF '/another/private/path' "$COLLECTED" \
  && ! grep -qF '/private/json/path' "$COLLECTED" \
  && ! grep -qF 'dummy-secret' "$COLLECTED" \
  && ! grep -qF 'secret-token' "$COLLECTED"; then
  pass "collector preserves diagnostics while redacting environment transcript values"
else
  fail "collector did not safely preserve the spill log (rc=$COLLECTOR_RC): $COLLECTOR_OUT"
fi
if [ ! -e "$DESTINATION/fathomdb-agent-link-123.log" ] \
  && grep -qF 'skipped_nonregular=1' "$DESTINATION/MANIFEST.txt"; then
  pass "collector refuses symlinked spill logs and records that decision"
else
  fail "collector must not follow a spill-log symlink: $(cat "$DESTINATION/MANIFEST.txt" 2>/dev/null || true)"
fi

EMPTY_SOURCE="$TMPROOT/empty-spill-source"
EMPTY_DESTINATION="$TMPROOT/empty-spill-destination"
mkdir -p "$EMPTY_SOURCE"
set +e
EMPTY_OUT="$(bash "$COLLECTOR" --source-dir "$EMPTY_SOURCE" --destination "$EMPTY_DESTINATION" 2>&1)"
EMPTY_RC=$?
set -e
if [ "$EMPTY_RC" -ne 0 ] \
  && grep -qF 'no regular agent spill logs found' <<<"$EMPTY_OUT" \
  && [ ! -e "$EMPTY_DESTINATION/MANIFEST.txt" ]; then
  pass "collector hard-fails before creating an uploadable manifest when no regular spill logs exist"
else
  fail "collector must fail loudly without an uploadable manifest for zero logs (rc=$EMPTY_RC): $EMPTY_OUT"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nall CI verify observability tests passed\n'
