#!/usr/bin/env bash
# Shared output discipline for scripts/agent-*.sh.
#
# Output contract:
#   - Pass: silent on stdout (or "ok <verb> NNNms" if AGENT_VERBOSE=1).
#   - Fail: first 200 lines of captured output to stdout, then a footer
#           with the spill path and exit code. Spill file always written.
#
# Rationale: keep agent context clean on the happy path; on failure give
# the agent the *start* of the diagnostic (where the actionable error
# lives) and a path to the rest. Cap is on output, not capture — the
# spill file always has the full log.

set -u

# Usage: run_capped <verb> <command...>
# Returns the underlying command's exit code.
run_capped() {
  local verb="$1"
  shift

  local spill="/tmp/fathomdb-agent-${verb}-$$.log"
  local start_ms end_ms duration_ms
  local rc=0

  start_ms=$(date +%s%3N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1000))')

  # Capture combined stdout+stderr.
  set +e
  "$@" >"$spill" 2>&1
  rc=$?
  set -e
  set -u

  end_ms=$(date +%s%3N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1000))')
  duration_ms=$((end_ms - start_ms))

  if [ "$rc" -eq 0 ]; then
    if [ "${AGENT_VERBOSE:-0}" = "1" ]; then
      printf 'ok %s %sms\n' "$verb" "$duration_ms"
    fi
    rm -f "$spill"
    return 0
  fi

  local total_lines
  total_lines=$(wc -l <"$spill" | tr -d ' ')

  printf 'FAIL %s (exit=%d, %sms)\n' "$verb" "$rc" "$duration_ms"
  printf -- '----\n'
  head -n 200 "$spill"
  if [ "$total_lines" -gt 200 ]; then
    printf -- '----\n'
    printf 'output truncated (%s lines total); full log: %s\n' "$total_lines" "$spill"
  else
    printf -- '----\n'
    printf 'full log: %s\n' "$spill"
  fi
  return "$rc"
}

# Usage: skip_notice <verb> <reason>
# One-line skip notice. Returns 0.
skip_notice() {
  local verb="$1"; shift
  if [ "${AGENT_VERBOSE:-0}" = "1" ]; then
    printf 'skip %s: %s\n' "$verb" "$*"
  fi
  return 0
}

# Usage: cd_repo_root
cd_repo_root() {
  cd "$(git rev-parse --show-toplevel)"
}
