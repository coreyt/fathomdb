#!/usr/bin/env bash
# Run lint -> typecheck -> test in latency order. Short-circuit on first failure.
# This is the agent-loop gate. The broader CI gate is scripts/check.sh.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# `git rev-parse` failing here used to degrade to `cd ""` — a bash no-op that
# leaves the script running in an arbitrary cwd. Bind and check it instead.
cd_repo_root() {
  local _repo_toplevel
  _repo_toplevel="$(git rev-parse --show-toplevel)" || return 1
  cd "$_repo_toplevel" || return 1
}
cd_repo_root

usage() {
  cat >&2 <<'USAGE'
Usage: agent-verify.sh [--tier=fast|heavy|all]

  --tier=fast|heavy|all  Select the corresponding agent-test tier. all is the
                         local default and preserves the full verifier.
USAGE
}

verify_tier="all"
if [ "$#" -gt 1 ]; then
  usage
  exit 2
fi
if [ "$#" -eq 1 ]; then
  case "$1" in
    --tier=fast|--tier=heavy|--tier=all)
      verify_tier="${1#--tier=}"
      ;;
    *)
      usage
      exit 2
      ;;
  esac
fi

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

if [ "$verify_tier" != "heavy" ]; then
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
fi

if ! bash "$SCRIPT_DIR/agent-test.sh" "--tier=$verify_tier"; then
  end=$(date +%s)
  printf 'FAIL verify at step=test tier=%s (%ss elapsed)\n' "$verify_tier" "$((end - start))"
  exit 1
fi

end=$(date +%s)
if [ "${AGENT_VERBOSE:-0}" = "1" ]; then
  printf 'ok verify tier=%s %ss\n' "$verify_tier" "$((end - start))"
fi
