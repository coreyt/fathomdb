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
# AC-036/037/038/050a/050c. STRICT=1 promotes toolchain blockers to
# hard failures so the gate is real (rc=2 → exit). Local dev hosts
# without strace must run scripts/bootstrap.sh first.
#
# AC037_LIVE_OPTIONAL=1: this gate runs on ubuntu-latest (and most dev hosts),
# where unprivileged userns is blocked by AppArmor, so AC-037's LIVE netns
# layer cannot run here (rc=3, environmental). The AUTHORITATIVE AC-037-live
# gate is the dedicated ubuntu-22.04 `security` CI job (STRICT=1, no opt-in),
# and the offline catch + policy self-test still run STRICT here. So we accept
# the userns-unavailable downgrade for that ONE layer without failing verify —
# while a real egress VIOLATION, a catch failure, or any other toolchain
# BLOCKER still fails this gate. See scripts/security/lib-gate-policy.sh.
STRICT=1 AC037_LIVE_OPTIONAL=1 bash "$SCRIPT_DIR/agent-security.sh" || exit 1
run_step test || exit 1

end=$(date +%s)
if [ "${AGENT_VERBOSE:-0}" = "1" ]; then
  printf 'ok verify %ss\n' "$((end - start))"
fi
