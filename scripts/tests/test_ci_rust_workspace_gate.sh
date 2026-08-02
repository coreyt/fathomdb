#!/usr/bin/env bash
# Behavioral workflow guard for the temporary serial Rust-workspace gate.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
WORKFLOW="$REPO_ROOT/.github/workflows/ci.yml"

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

if [ ! -f "$WORKFLOW" ]; then
  fail "ci workflow is missing: $WORKFLOW"
  exit 1
fi

CONTRACT="$TMPROOT/workflow-contract.json"
if ! python3 - "$WORKFLOW" >"$CONTRACT" <<'PY'
import json
import sys
import yaml

workflow = yaml.safe_load(open(sys.argv[1]))
jobs = workflow["jobs"]

def steps(name):
    return jobs[name].get("steps", [])

def runs(name):
    return [step["run"] for step in steps(name) if isinstance(step, dict) and "run" in step]

def has_run(name, needle):
    return any(needle in body for body in runs(name))

def direct_workspace_cargo(name):
    return any("cargo test --workspace" in body for body in runs(name))

reporter = jobs.get("rust-workspace-race-report", {})
reporter_steps = reporter.get("steps", [])
reporter_run = next(
    (step["run"] for step in reporter_steps
     if isinstance(step, dict) and "run" in step and "--parallel-report" in step["run"]),
    None,
)
upload = next(
    (step for step in reporter_steps
     if isinstance(step, dict) and step.get("uses", "").startswith("actions/upload-artifact@")),
    None,
)

print(json.dumps({
    "verify_indirect": has_run("verify", "bash scripts/agent-verify.sh"),
    "windows_serial": has_run("rust-windows", "bash scripts/test-rust-workspace.sh --serial"),
    "macos_serial": has_run("rust-macos", "bash scripts/test-rust-workspace.sh --serial"),
    "windows_direct_cargo": direct_workspace_cargo("rust-windows"),
    "macos_direct_cargo": direct_workspace_cargo("rust-macos"),
    "reporter_linux": reporter.get("runs-on") == "ubuntu-latest",
    "reporter_timeout": bool(reporter.get("timeout-minutes")),
    "reporter_run": reporter_run,
    "upload_always": upload is not None and upload.get("if") == "always()",
    "upload_pinned": upload is not None and upload.get("uses") == "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a",
    "upload_path": upload.get("with", {}).get("path") if upload else None,
    "upload_exact_log": upload is not None and upload.get("with", {}).get("path") == "${{ runner.temp }}/rust-workspace-parallel.log",
}, sort_keys=True))
PY
then
  fail "ci.yml did not parse as YAML"
  exit 1
fi

contract_value() {
  python3 - "$CONTRACT" "$1" <<'PY'
import json
import sys
print(json.load(open(sys.argv[1]))[sys.argv[2]])
PY
}

assert_true() {
  local key="$1" description="$2"
  if [ "$(contract_value "$key")" = "True" ]; then
    pass "$description"
  else
    fail "$description"
  fi
}

assert_true verify_indirect "verify retains agent-verify as its indirect serial route"
assert_true windows_serial "Windows calls the canonical serial runner"
assert_true macos_serial "macOS calls the canonical serial runner"
if [ "$(contract_value windows_direct_cargo)" = "False" ] && [ "$(contract_value macos_direct_cargo)" = "False" ]; then
  pass "native gating legs contain no direct cargo test --workspace call"
else
  fail "a native gating leg retains a direct cargo test --workspace call"
fi
assert_true reporter_linux "parallel reporter is a distinct Linux job"
assert_true reporter_timeout "parallel reporter has a finite timeout"
assert_true upload_always "parallel reporter uploads its log with if: always()"
assert_true upload_pinned "parallel reporter pins actions/upload-artifact"
assert_true upload_exact_log "parallel reporter uploads the complete reporter log"

REPORTER_BODY="$(contract_value reporter_run)"
UPLOAD_PATH="$(contract_value upload_path)"
if [ "$UPLOAD_PATH" = 'None' ] || [ -z "$REPORTER_BODY" ] || [ "$REPORTER_BODY" = 'None' ]; then
  fail "parallel reporter and its complete log upload must both be present"
else
  FAKE_BIN="$TMPROOT/bin"
  RUNNER_TEMP="$TMPROOT/runner-temp"
  SUMMARY="$TMPROOT/summary.md"
  mkdir -p "$FAKE_BIN" "$RUNNER_TEMP"
  cat >"$FAKE_BIN/bash" <<'EOF'
#!/usr/bin/bash
if [ "$1" = "scripts/test-rust-workspace.sh" ] && [ "$2" = "--parallel-report" ]; then
  printf 'known stdout\n'
  printf 'known stderr\n' >&2
  exit 73
fi
exec /usr/bin/bash "$@"
EOF
  chmod +x "$FAKE_BIN/bash"
  BODY="$TMPROOT/reporter-body.sh"
  printf '%s\n' "$REPORTER_BODY" >"$BODY"
  set +e
  PATH="$FAKE_BIN:$PATH" RUNNER_TEMP="$RUNNER_TEMP" GITHUB_STEP_SUMMARY="$SUMMARY" \
    GITHUB_SHA=deadbeef GITHUB_RUN_ATTEMPT=7 bash "$BODY" >"$TMPROOT/reporter.out" 2>&1
  BODY_RC=$?
  set -e
  LOG_PATH="$RUNNER_TEMP/rust-workspace-parallel.log"
  if [ "$BODY_RC" -eq 0 ] && grep -q '73' "$SUMMARY" && grep -q 'rust-workspace-parallel.log' "$SUMMARY" \
    && grep -q 'known stdout' "$LOG_PATH" && grep -q 'known stderr' "$LOG_PATH"; then
    pass "executed reporter preserves raw output and reports the immediate non-zero code before returning zero"
  else
    fail "reporter execution did not preserve diagnostic semantics (rc=$BODY_RC, summary=$(cat "$SUMMARY" 2>/dev/null || true), output=$(cat "$TMPROOT/reporter.out"))"
  fi
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nall CI Rust-workspace gate tests passed\n'
