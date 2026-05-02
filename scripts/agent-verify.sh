#!/usr/bin/env bash
# Run lint -> typecheck -> test in latency order. Short-circuit on first failure.
# This is the agent-loop gate. The broader CI gate is scripts/check.sh.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd_repo_root() { cd "$(git rev-parse --show-toplevel)"; }
cd_repo_root

start=$(date +%s)

run_step() {
  local step="$1"
  if ! "$SCRIPT_DIR/agent-$step.sh"; then
    local end
    end=$(date +%s)
    printf 'FAIL verify at step=%s (%ss elapsed)\n' "$step" "$((end - start))"
    return 1
  fi
}

run_step lint || exit 1
run_step typecheck || exit 1
run_step test || exit 1

end=$(date +%s)
if [ "${AGENT_VERBOSE:-0}" = "1" ]; then
  printf 'ok verify %ss\n' "$((end - start))"
fi
