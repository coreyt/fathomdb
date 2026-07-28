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
# SECOND JOB — TRANSCRIPT HYGIENE AT CAPTURE TIME (TC-86, steward `seq-129`,
# todos `TC-86`, master `F-36`, HITL-approved 2026-07-28):
# codex's stdout is filtered through the ONE shared agent-state pattern
# (scripts/lib/agent-state-paths.sh) before it leaves this wrapper.
#
# WHY HERE. On 2026-07-28 a §9 review transcript arrived carrying 216 lines of
# raw Claude Code session JSONL: codex, running under
# --dangerously-bypass-approvals-and-sandbox (the flag is precisely what lets it
# read outside the repo), ran `rg` across ~/.claude and slurped the results into
# its own stdout — which TC-RUBRIC-7 then required be `tee`d to a TRACKED path
# under dev/plans/runs/codex/, in a PUBLIC repository. The lines carried
# conversation content from three OTHER projects. Caught and redacted before
# landing; `git grep` proved reachability in history is ZERO.
#
# That is STRUCTURAL, not a one-off — every §9 runs with the bypass flag, every
# transcript must be persisted, and nothing in between looked. This wrapper is
# already MANDATED for every codex invocation (hand-off §6), so folding the
# filter in here needs no new instruction to adopt and cannot be forgotten:
# capture time is the only place the line can be stopped BEFORE it exists on
# disk. scripts/check-transcript-hygiene.sh is the land-time backstop for
# everything this wrapper never saw, and both read the SAME pattern from the
# SAME file so they cannot drift.
#
# USAGE — forward ALL args verbatim to codex; this wrapper closes stdin and
# redacts agent-state paths out of codex's stdout:
#   dev/agent-tools/codex-nostdin.sh exec review --base <sha> --dangerously-bypass-approvals-and-sandbox
#   dev/agent-tools/codex-nostdin.sh exec --dangerously-bypass-approvals-and-sandbox "<prompt>"
#
# ALWAYS invoke codex through this wrapper (the orchestrator hand-off §6 points
# here). Do not call bare `codex exec` in an agent shell.
#
# CONTRACT NOTES for anyone editing this file:
#   * codex's EXIT CODE is still this script's exit code. The filter forces a
#     pipeline, and a pipeline reports its LAST command's status, so PIPESTATUS
#     is captured on the line IMMEDIATELY after the pipeline. Get that wrong and
#     every review reports `sed`'s success instead of codex's verdict — the
#     exact exit-code-capture bug this repo has been bitten by twice.
#   * codex's stdout is now a PIPE rather than this script's stdout. In practice
#     it already was (every mandated invocation is `| tee` to the TC-RUBRIC-7
#     transcript), so tools that adapt their output to a TTY behave as they
#     always have.
#   * STDERR IS NOT FILTERED. It passes through untouched, deliberately: merging
#     it into stdout would reorder a reviewer's diagnostics against its findings,
#     and the leak vector is codex's `exec` tool output, which is stdout.
set -euo pipefail

# Resolve the shared pattern WITHOUT external commands (`cd`/`pwd` are builtins),
# so this still works when PATH is empty — which is exactly how the fixture suite
# exercises the "codex not found" arm below.
_self="${BASH_SOURCE[0]}"
case "$_self" in
  */*) _self_dir="${_self%/*}" ;;
  *)   _self_dir="." ;;
esac
_repo_root="$(cd -- "$_self_dir/../.." && pwd)"
_shared_lib="$_repo_root/scripts/lib/agent-state-paths.sh"

# ANTI-FAIL-OPEN: if the pattern is unavailable, REFUSE — do not run codex
# unfiltered. An unfiltered run is the incident.
if [ ! -f "$_shared_lib" ]; then
  echo "codex-nostdin.sh: cannot read the shared agent-state pattern at $_shared_lib" >&2
  echo "codex-nostdin.sh: refusing to run codex UNFILTERED (TC-86) — fix the checkout instead" >&2
  exit 2
fi
# shellcheck source=../../scripts/lib/agent-state-paths.sh
. "$_shared_lib"

if ! command -v codex >/dev/null 2>&1; then
  echo "codex-nostdin.sh: 'codex' not found on PATH" >&2
  exit 127
fi

# The two load-bearing lines. codex runs with stdin redirected from /dev/null so
# it cannot wait on interactive input; all arguments pass through untouched; and
# its stdout is redacted line-by-line on the way out.
#
# `set +e` around the pipeline because `set -o pipefail` + `set -e` would abort
# the script on a non-zero codex BEFORE PIPESTATUS could be read — which would
# turn a §9 FAIL into a silent 1 and lose the specific code.
set +e
codex "$@" </dev/null | agent_state_redact_stream
rc=${PIPESTATUS[0]}
set -e
exit "$rc"
