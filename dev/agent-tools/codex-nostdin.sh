#!/usr/bin/env bash
# codex-nostdin.sh — invoke `codex` with stdin CLOSED so it can never block on
# "Reading additional input from stdin...".
#
# WHY THIS EXISTS (fix-the-tooling-not-the-actor, `guardrail-failures-fix-tooling-not-people`):
# bare `codex exec ...` reads from stdin. In a detached/background agent shell,
# or when a stray/locked codex session is holding the terminal, codex blocks
# forever on stdin instead of running — a hang that looks like "slow analysis"
# but is actually a deadlock (observed twice during the 0.8.18 Slice-0 design
# review, 2026-07-09). Closing stdin makes the failure impossible.
#
# USAGE — forward ALL args verbatim to codex; this wrapper only closes stdin:
#   dev/agent-tools/codex-nostdin.sh exec review --base <sha> --dangerously-bypass-approvals-and-sandbox
#   dev/agent-tools/codex-nostdin.sh exec --dangerously-bypass-approvals-and-sandbox "<prompt>"
#
# ALWAYS invoke codex through this wrapper (the orchestrator hand-off §6 points
# here). Do not call bare `codex exec` in an agent shell.
set -euo pipefail

if ! command -v codex >/dev/null 2>&1; then
  echo "codex-nostdin.sh: 'codex' not found on PATH" >&2
  exit 127
fi

# The one load-bearing line: run codex with stdin redirected from /dev/null so
# it cannot wait on interactive input. All arguments pass through untouched.
exec codex "$@" </dev/null
