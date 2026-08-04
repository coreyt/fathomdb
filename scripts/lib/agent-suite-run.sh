#!/usr/bin/env bash
# scripts/lib/agent-suite-run.sh — collect-all recording wrapper.
#
# Sourced by scripts/agent-test.sh ONLY. See
# scripts/tests/test_agent_test_collect_all.sh arm D for the static assertion
# that no other script in the repo sources this file.
#
# WHY THIS IS A SEPARATE FILE FROM scripts/lib/agent-output.sh — a justified
# deviation from the literal brief text "the collect-all wrapper lives in
# agent-test.sh", already approved by the orchestrator:
#
# scripts/lib/agent-output.sh's run_capped/skip_notice are sourced by FIVE
# `set -euo pipefail` fail-fast scripts — agent-test.sh, agent-lint.sh
# (5 uses), agent-typecheck.sh (3), agent-build.sh (3), agent-lint-md.sh (9)
# — and copied VERBATIM into a fixture repo by
# scripts/tests/test_lint_md_hard_fail_on_missing_linter.sh:39. If
# `run_capped` itself were changed to record-and-return-0 (the obvious
# reading of "run_capped is the seam"), all four OTHER callers would
# silently become continue-on-failure with a zero exit — manufacturing the
# exact vacuous green this unit exists to end, in four scripts nobody is
# looking at, plus a fixture that copies the file verbatim. So
# `run_capped`'s return contract (`return "$rc"`, its output capping, its
# spill file) is UNCHANGED here and everywhere.
#
# Instead, this file is a THIN RECORDING WRAPPER around the *existing*
# run_capped/skip_notice: it records (label, state, rc, ms) and always
# returns 0 so the `set -e` in scripts/agent-test.sh cannot abort the
# collect-all run. It is sourced by scripts/agent-test.sh alone, so the
# other four fail-fast callers of agent-output.sh are completely unaffected.
# Keeping it in its own file (rather than inlining into agent-test.sh) also
# makes the recording/summary machinery independently unit-testable —
# scripts/tests/test_agent_test_collect_all.sh arm A drives this file
# directly, sourced (never reimplemented), with disposable fake suites.
#
# ⚠ A subtlety that bit the first draft of this file: run_capped's own body
# does `set +e; "$@" ...; rc=$?; set -e` internally — it unconditionally
# turns errexit back ON before it returns, regardless of whatever errexit
# state its caller was in. Calling it as a bare statement under `set -e`
# (even inside a `set +e ... set -e` wrapper in the CALLER) still aborts the
# calling script, because errexit is evaluated against the state active at
# the moment the function's `return "$rc"` executes, and that state was
# reset to ON by run_capped itself. The fix is the standard `||` exemption:
# `run_capped ... || rc=$?` — a command immediately followed by `||` is
# exempt from errexit regardless of what it does internally. That is the
# ONLY reason `run_suite` below reads oddly if you expect a plain
# `set +e; run_capped ...; rc=$?; set -e` idiom; that idiom does NOT work
# here (proven empirically), because it is the callee, not the caller, that
# controls errexit at the point of return.
#
# THE INVARIANT (design doc §2): a run that had any failure MUST still exit
# non-zero. Continue-on-failure changes WHEN the harness stops, never
# WHETHER a failure counts. A crash is a FAILURE, never a skip. The summary
# is the deliverable. Ordering stays deterministic (registration order).

set -u

# Registration-order parallel arrays. Index i describes suite i.
_SUITE_LABELS=()
_SUITE_STATES=() # PASS | FAIL | SKIP | EXCL
_SUITE_RCS=()
_SUITE_MS=()

# Labels requested for exclusion (via exclude_suite), and which of those
# labels were actually matched by a real run_suite/skip_suite registration —
# used by suite_summary_and_exit to catch a typo'd --exclude-suite label
# that silently excluded nothing (a harness usage error, not a quiet no-op).
_EXCLUDED_LABELS=()
_EXCLUDED_MATCHED=()

# Usage: exclude_suite <label>
# Marks <label> for exclusion. Must be called (by scripts/agent-test.sh's
# arg-parsing, before any suite runs) ahead of the run_suite/skip_suite call
# site that registers <label>, or the exclusion has no effect on that site.
exclude_suite() {
  _EXCLUDED_LABELS+=("$1")
}

_is_excluded() {
  local label="$1" x
  for x in "${_EXCLUDED_LABELS[@]:-}"; do
    [ -n "$x" ] && [ "$x" = "$label" ] && return 0
  done
  return 1
}

_record() {
  local label="$1" state="$2" rc="$3" ms="$4"
  _SUITE_LABELS+=("$label")
  _SUITE_STATES+=("$state")
  _SUITE_RCS+=("$rc")
  _SUITE_MS+=("$ms")
}

# Escape data sent through GitHub's workflow-command protocol. Suite labels are
# currently static repository-owned strings, but this keeps a future label from
# opening a second command or annotation line.
_github_escape_command_data() {
  local value="$1"
  value="${value//%/%25}"
  value="${value//$'\r'/%0D}"
  value="${value//$'\n'/%0A}"
  printf '%s' "$value"
}

# Write a compact, deterministic result table to the GitHub run's front-page
# summary. It is intentionally emitted on success too: passing runs otherwise
# discard run_capped's per-suite timings, which made the slow tail impossible to
# measure. A missing summary path is a CI infrastructure failure, not a reason
# to silently drop observability.
_github_write_suite_summary() {
  local i n label state rc ms markdown_label

  [ -n "${GITHUB_ACTIONS:-}" ] || return 0
  if [ -z "${GITHUB_STEP_SUMMARY:-}" ]; then
    printf 'agent-test.sh: GITHUB_ACTIONS is set but GITHUB_STEP_SUMMARY is missing\n' >&2
    return 1
  fi

  n="${#_SUITE_LABELS[@]}"
  {
    printf '## FathomDB agent-test suite results\n\n'
    printf '| Suite | Status | Exit | Duration (ms) |\n'
    printf '| --- | --- | ---: | ---: |\n'
    for ((i = 0; i < n; i++)); do
      label="${_SUITE_LABELS[$i]}"
      state="${_SUITE_STATES[$i]}"
      rc="${_SUITE_RCS[$i]}"
      ms="${_SUITE_MS[$i]}"
      markdown_label="${label//|/\\|}"
      printf '| %s | %s | %s | %s |\n' "$markdown_label" "$state" "$rc" "$ms"
    done
    if [ "${#failed_labels[@]}" -gt 0 ]; then
      printf '\n### Failed suites\n\n'
      printf 'FAILED SUITES: %s\n' "${failed_labels[*]}"
    fi
  } >>"$GITHUB_STEP_SUMMARY"
}

_github_emit_failure_annotations() {
  local label escaped_label state i n

  [ -n "${GITHUB_ACTIONS:-}" ] || return 0
  n="${#_SUITE_LABELS[@]}"
  for ((i = 0; i < n; i++)); do
    state="${_SUITE_STATES[$i]}"
    [ "$state" = "FAIL" ] || continue
    label="${_SUITE_LABELS[$i]}"
    escaped_label="$(_github_escape_command_data "$label")"
    printf '::error title=agent-test suite failed::%s (exit=%s, duration=%sms)\n' \
      "$escaped_label" "${_SUITE_RCS[$i]}" "${_SUITE_MS[$i]}"
  done
}

# Usage: run_suite <label> <cmd...>
# Runs <cmd...> under the REAL run_capped (agent-output.sh), records the
# outcome, and ALWAYS returns 0 so `set -e` in the caller cannot abort the
# collect-all run. States: PASS (rc 0) / FAIL (rc != 0, including a crash —
# see the header note above on why NEVER a bare call). If <label> is
# excluded, the command is never invoked at all; state EXCL is recorded.
run_suite() {
  local label="$1"; shift
  local start_ms end_ms ms rc

  if _is_excluded "$label"; then
    _EXCLUDED_MATCHED+=("$label")
    _record "$label" "EXCL" "-" "0"
    return 0
  fi

  start_ms=$(date +%s%3N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1000))')
  rc=0
  run_capped "$label" "$@" || rc=$?
  end_ms=$(date +%s%3N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1000))')
  ms=$((end_ms - start_ms))

  if [ "$rc" -eq 0 ]; then
    _record "$label" "PASS" "$rc" "$ms"
  else
    _record "$label" "FAIL" "$rc" "$ms"
  fi
  return 0
}

# Usage: skip_suite <label> <reason>
# Wraps the REAL skip_notice, but records SKIP as a DISTINCT THIRD STATE —
# never a pass, never a failure. If <label> is excluded, records EXCL
# instead and does not call skip_notice at all.
skip_suite() {
  local label="$1"; shift

  if _is_excluded "$label"; then
    _EXCLUDED_MATCHED+=("$label")
    _record "$label" "EXCL" "-" "0"
    return 0
  fi

  skip_notice "$label" "$@"
  _record "$label" "SKIP" "0" "0"
  return 0
}

# Usage: suite_summary_and_exit
# Prints the deterministic collect-all summary in REGISTRATION ORDER and
# exits:
#   0 — zero FAILs
#   1 — at least one FAIL
#   2 — harness usage error (an --exclude-suite label matched no
#       registration at all — checked here, AFTER every run_suite/
#       skip_suite call site in the caller has executed, since that is the
#       first point at which "matched nothing" can be known).
suite_summary_and_exit() {
  local i n label state rc ms
  local registered=0 ran=0 passed=0 failed=0 skipped=0 excluded=0
  local failed_labels=()

  # Usage-error check FIRST: a requested exclusion that matched nothing
  # guards against a typo silently excluding nothing (falling through to a
  # normal, apparently-complete run).
  local x m matched
  for x in "${_EXCLUDED_LABELS[@]:-}"; do
    [ -z "$x" ] && continue
    matched=0
    for m in "${_EXCLUDED_MATCHED[@]:-}"; do
      if [ "$m" = "$x" ]; then
        matched=1
        break
      fi
    done
    if [ "$matched" -eq 0 ]; then
      printf 'agent-test.sh: usage error: --exclude-suite=%s matched no registered suite\n' "$x" >&2
      exit 2
    fi
  done

  n="${#_SUITE_LABELS[@]}"
  for ((i = 0; i < n; i++)); do
    state="${_SUITE_STATES[$i]}"
    registered=$((registered + 1))
    case "$state" in
      PASS)
        passed=$((passed + 1))
        ran=$((ran + 1))
        ;;
      FAIL)
        failed=$((failed + 1))
        ran=$((ran + 1))
        failed_labels+=("${_SUITE_LABELS[$i]}")
        ;;
      SKIP)
        skipped=$((skipped + 1))
        ;;
      EXCL)
        excluded=$((excluded + 1))
        ;;
    esac
  done

  if [ "$failed" -gt 0 ] || [ "${AGENT_VERBOSE:-0}" = "1" ]; then
    for ((i = 0; i < n; i++)); do
      label="${_SUITE_LABELS[$i]}"
      state="${_SUITE_STATES[$i]}"
      rc="${_SUITE_RCS[$i]}"
      ms="${_SUITE_MS[$i]}"
      printf '%s %s rc=%s %sms\n' "$state" "$label" "$rc" "$ms"
    done
    printf 'registered=%d ran=%d passed=%d failed=%d skipped=%d excluded=%d\n' \
      "$registered" "$ran" "$passed" "$failed" "$skipped" "$excluded"
    if [ "$failed" -gt 0 ]; then
      printf 'FAILED SUITES: %s\n' "${failed_labels[*]}"
    fi
  else
    printf 'agent-test.sh: %d/%d suites passed (skipped=%d excluded=%d)\n' \
      "$passed" "$registered" "$skipped" "$excluded"
  fi

  if ! _github_write_suite_summary; then
    printf 'agent-test.sh: failed to write GitHub suite summary\n' >&2
    exit 1
  fi
  _github_emit_failure_annotations

  if [ "$failed" -gt 0 ]; then
    exit 1
  fi
  exit 0
}
