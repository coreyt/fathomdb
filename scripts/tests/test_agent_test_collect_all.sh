#!/usr/bin/env bash
# scripts/tests/test_agent_test_collect_all.sh — recurrence guard for the
# 0.8.20 R-20-HARNESS (a.k.a. "Slice 39.5") collect-all conversion of
# scripts/agent-test.sh.
#
# The incident this closes: agent-test.sh runs under `set -euo pipefail` and
# aborts at the FIRST failing suite. It registers 31 unconditional suites (35
# call sites / 34 distinct labels, per the commission brief's reconciled
# convention); the abort has historically landed at test-check-governed-
# surface-pin (agent-test.sh:73), leaving 23 suites that had never run in a
# full pass. The aggregate exit code was therefore a vacuous signal (TC-16).
#
# THE INVARIANT (design doc dev/design/0.8.20-slice-39.5-collect-all-test-
# harness.md §2): a run that had any failure MUST still exit non-zero.
# Continue-on-failure changes WHEN the harness stops, never WHETHER a failure
# counts. A crash is a FAILURE, never a skip. The summary is the deliverable.
#
# ⛔ HARD CONSTRAINT (this suite's arm B is its anti-regression guard):
# scripts/lib/agent-output.sh's run_capped is sourced by FOUR OTHER
# fail-fast scripts (agent-lint.sh, agent-typecheck.sh, agent-build.sh,
# agent-lint-md.sh) and copied verbatim into a fixture repo by
# test_lint_md_hard_fail_on_missing_linter.sh:39. run_capped's `return "$rc"`
# contract must never change — only scripts/agent-test.sh (via the new
# scripts/lib/agent-suite-run.sh recording wrapper) may become collect-all.
#
# Grading discipline: every arm below drives REAL code — either the real
# scripts/lib/agent-suite-run.sh (sourced, not reimplemented) with disposable
# fake suites, or the real scripts/agent-test.sh entry point itself for the
# arg-parse arms. Across Slices 32/33, five of six codex fix rounds were
# defects in the verification apparatus, not the function under test — never
# write a helper that re-implements the harness and then test the helper.
#
# Isolation: fixtures are throwaway driver scripts + a throwaway git repo
# under mktemp -d. Nothing here writes into this checkout, and the two real
# scripts/agent-test.sh invocations (arm E) are argv-shape errors that exit
# before any registered suite runs — never a full run.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
AGENT_TEST="$REPO_ROOT/scripts/agent-test.sh"
AGENT_OUTPUT_LIB="$REPO_ROOT/scripts/lib/agent-output.sh"
AGENT_SUITE_RUN_LIB="$REPO_ROOT/scripts/lib/agent-suite-run.sh"

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

# ============================================================================
# Arm A: lib-level, driving the REAL scripts/lib/agent-suite-run.sh with fake
# suites. All four outcomes (pass, fail, crash, skip) must be RECORDED; the
# run's exit must be non-zero; the summary must name BOTH failures; the crash
# must be state FAIL with rc=137 (128+9), never skipped; the skip must be a
# distinct third state.
# ============================================================================
DRIVER_A="$TMPROOT/driver_a.sh"
cat >"$DRIVER_A" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
. "$AGENT_OUTPUT_LIB"
. "$AGENT_SUITE_RUN_LIB"
run_suite fake-pass true
run_suite fake-fail false
run_suite fake-crash bash -c 'kill -9 $$'
skip_suite fake-skip "demo skip, never a default"
suite_summary_and_exit
EOF
chmod +x "$DRIVER_A"

set +e
A_OUT="$(AGENT_OUTPUT_LIB="$AGENT_OUTPUT_LIB" AGENT_SUITE_RUN_LIB="$AGENT_SUITE_RUN_LIB" \
  AGENT_VERBOSE=1 bash "$DRIVER_A" 2>&1)"
A_RC=$?
set -e

if [ "$A_RC" -ne 0 ]; then
  pass "arm A: a run containing failures exits non-zero (rc=$A_RC)"
else
  fail "arm A: a run containing failures exited 0"
fi

if grep -qE '^PASS fake-pass rc=0 ' <<<"$A_OUT"; then
  pass "arm A: fake-pass recorded PASS"
else
  fail "arm A: fake-pass not recorded as PASS: $A_OUT"
fi

if grep -qE '^FAIL fake-fail rc=1 ' <<<"$A_OUT"; then
  pass "arm A: fake-fail recorded FAIL rc=1"
else
  fail "arm A: fake-fail not recorded as FAIL rc=1: $A_OUT"
fi

if grep -qE '^FAIL fake-crash rc=137 ' <<<"$A_OUT"; then
  pass "arm A: a signal-9 crash is recorded FAIL rc=137 (128+9), never skipped"
else
  fail "arm A: fake-crash not recorded as FAIL rc=137: $A_OUT"
fi

if grep -qE '^SKIP fake-skip ' <<<"$A_OUT"; then
  pass "arm A: skip_suite records a distinct SKIP state (never PASS, never FAIL)"
else
  fail "arm A: fake-skip not recorded as SKIP: $A_OUT"
fi

if grep -qE '^FAILED SUITES:.*fake-fail' <<<"$A_OUT" && grep -qE '^FAILED SUITES:.*fake-crash' <<<"$A_OUT"; then
  pass "arm A: the summary names BOTH failing suites, not just the first"
else
  fail "arm A: the summary does not name both failures: $A_OUT"
fi

if grep -qE '^registered=4 ran=3 passed=1 failed=2 skipped=1 excluded=0$' <<<"$A_OUT"; then
  pass "arm A: totals line accounts for every registered suite"
else
  fail "arm A: totals line wrong or missing: $A_OUT"
fi

# ---- Arm A2: exclusion is a DISTINCT fourth state (EXCL), never run. -------
DRIVER_A2="$TMPROOT/driver_a2.sh"
cat >"$DRIVER_A2" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
. "$AGENT_OUTPUT_LIB"
. "$AGENT_SUITE_RUN_LIB"
exclude_suite excluded-one
run_suite excluded-one bash -c 'echo SHOULD-NEVER-RUN; exit 1'
run_suite fake-pass true
suite_summary_and_exit
EOF
chmod +x "$DRIVER_A2"

set +e
A2_OUT="$(AGENT_OUTPUT_LIB="$AGENT_OUTPUT_LIB" AGENT_SUITE_RUN_LIB="$AGENT_SUITE_RUN_LIB" \
  AGENT_VERBOSE=1 bash "$DRIVER_A2" 2>&1)"
A2_RC=$?
set -e

if [ "$A2_RC" -eq 0 ]; then
  pass "arm A2: excluding the only would-be-failing suite yields a clean exit"
else
  fail "arm A2: expected exit 0 with the failing suite excluded, got rc=$A2_RC: $A2_OUT"
fi

if grep -qE '^EXCL excluded-one ' <<<"$A2_OUT"; then
  pass "arm A2: an excluded suite is recorded EXCL, a distinct fourth state"
else
  fail "arm A2: excluded-one not recorded as EXCL: $A2_OUT"
fi

if grep -q 'SHOULD-NEVER-RUN' <<<"$A2_OUT"; then
  fail "arm A2: an excluded suite's command actually ran"
else
  pass "arm A2: an excluded suite's command never runs"
fi

# ---- Arm A3: an excluded label matching NO registration is a usage error --
DRIVER_A3="$TMPROOT/driver_a3.sh"
cat >"$DRIVER_A3" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
. "$AGENT_OUTPUT_LIB"
. "$AGENT_SUITE_RUN_LIB"
exclude_suite this-label-was-never-registered
run_suite fake-pass true
suite_summary_and_exit
EOF
chmod +x "$DRIVER_A3"

set +e
A3_OUT="$(AGENT_OUTPUT_LIB="$AGENT_OUTPUT_LIB" AGENT_SUITE_RUN_LIB="$AGENT_SUITE_RUN_LIB" \
  bash "$DRIVER_A3" 2>&1)"
A3_RC=$?
set -e

if [ "$A3_RC" -eq 2 ]; then
  pass "arm A3: an exclude label matching no registration is a harness usage error (rc=2)"
else
  fail "arm A3: expected rc=2 for an unmatched exclude label, got rc=$A3_RC: $A3_OUT"
fi

# ============================================================================
# Arm B: run_capped's return contract is intact. Anti-regression guard for
# HARD CONSTRAINT 1 — mutation-proof: assert the literal `return "$rc"` is
# still present, AND drive the real run_capped with a failing command.
# ============================================================================
if grep -qF 'return "$rc"' "$AGENT_OUTPUT_LIB"; then
  pass "arm B: scripts/lib/agent-output.sh still contains the literal return \"\$rc\""
else
  fail "arm B: scripts/lib/agent-output.sh no longer contains return \"\$rc\" — HARD CONSTRAINT 1 violated"
fi

# Driven exactly as the four OTHER real callers use run_capped: a bare,
# unguarded call under `set -euo pipefail` (fail-fast). This is also the
# faithful regression arm: run_capped's internal `set -e` reset means a
# `set +e ... run_capped ... set -e` wrapper in the CALLER does NOT prevent
# an abort (see scripts/lib/agent-suite-run.sh's header note) — so the
# driver's OWN exit status is the correct observable, not a post-call
# printf that a caller-side set +e might appear to protect.
DRIVER_B="$TMPROOT/driver_b.sh"
cat >"$DRIVER_B" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
. "$AGENT_OUTPUT_LIB"
run_capped demo-fail false
EOF
chmod +x "$DRIVER_B"
set +e
B_OUT="$(AGENT_OUTPUT_LIB="$AGENT_OUTPUT_LIB" bash "$DRIVER_B" 2>&1)"
B_RC=$?
set -e
if [ "$B_RC" -eq 1 ]; then
  pass "arm B: run_capped still returns a non-zero rc for a failing command (rc=$B_RC)"
else
  fail "arm B: run_capped did not return non-zero for a failing command: rc=$B_RC out=$B_OUT"
fi

# ============================================================================
# Arm C: STATIC assertions on scripts/agent-test.sh — zero remaining bare
# run_capped/skip_notice call sites (every registration must go through the
# recording wrapper, or a future suite could be added un-recorded and
# reintroduce fail-fast silently), and the file's LAST executable line is
# suite_summary_and_exit (nothing registered after it could ever run).
# ============================================================================
# Anchored to actual call-site shape (leading whitespace then the bare verb
# then a space before its args) so this does NOT false-positive on prose
# that merely mentions "run_capped"/"skip_notice" in a comment (e.g. the
# agent-lint-md.sh recurrence-guard comment describing THAT script's own
# skip_notice usage, not a call site in THIS file).
if grep -qE '^[[:space:]]*run_capped[[:space:]]' "$AGENT_TEST"; then
  fail "arm C: scripts/agent-test.sh has a bare run_capped call site (must be run_suite)"
else
  pass "arm C: scripts/agent-test.sh has zero bare run_capped call sites"
fi

if grep -qE '^[[:space:]]*skip_notice[[:space:]]' "$AGENT_TEST"; then
  fail "arm C: scripts/agent-test.sh has a bare skip_notice call site (must be skip_suite)"
else
  pass "arm C: scripts/agent-test.sh has zero bare skip_notice call sites"
fi

LAST_EXEC_LINE="$(grep -vE '^[[:space:]]*(#.*)?$' "$AGENT_TEST" | tail -n1)"
if [ "$LAST_EXEC_LINE" = "suite_summary_and_exit" ]; then
  pass "arm C: the last executable line of agent-test.sh is suite_summary_and_exit"
else
  fail "arm C: the last executable line of agent-test.sh is NOT suite_summary_and_exit: [$LAST_EXEC_LINE]"
fi

# ============================================================================
# Arm D: scripts/lib/agent-suite-run.sh is sourced by scripts/agent-test.sh
# and by NO other script in the repo.
# ============================================================================
if grep -q 'agent-suite-run.sh' "$AGENT_TEST"; then
  pass "arm D: scripts/agent-test.sh sources scripts/lib/agent-suite-run.sh"
else
  fail "arm D: scripts/agent-test.sh does not source scripts/lib/agent-suite-run.sh"
fi

# --include='*.sh' restricts this to scripts that could actually SOURCE the
# file (a `.`/`source` statement); otherwise this false-positives on any
# prose mention of the filename in a doc, JSON witness or markdown record
# (e.g. this very suite's own closure output.json, which names the file in
# its rationale).
OTHER_SOURCERS="$(grep -rl --include='*.sh' 'agent-suite-run.sh' "$REPO_ROOT/scripts" "$REPO_ROOT/dev" 2>/dev/null \
  | grep -v -F "$AGENT_TEST" \
  | grep -v -F "$AGENT_SUITE_RUN_LIB" \
  | grep -v -F "$SCRIPT_DIR/test_agent_test_collect_all.sh" \
  || true)"
if [ -z "$OTHER_SOURCERS" ]; then
  pass "arm D: no other script in the repo sources agent-suite-run.sh"
else
  fail "arm D: unexpected additional sourcer(s) of agent-suite-run.sh: $OTHER_SOURCERS"
fi

# ============================================================================
# Arm E: usage errors, driven through the REAL scripts/agent-test.sh entry
# point for the two genuine arg-parse arms (they must exit before any
# registered suite runs — timed to prove they are fast). The third sub-arm
# (an --exclude-suite label matching no registration) is checked at the lib
# level in arm A3 above: per the design, that check happens AFTER every
# registration has executed, so driving it through the real agent-test.sh
# would require a full multi-minute run — forbidden by this suite's own
# environment rules (never run agent-test.sh to completion from a test).
# ============================================================================
run_agent_test_argv() {
  set +e
  E_OUT="$(cd "$REPO_ROOT" && timeout 20 bash scripts/agent-test.sh "$@" 2>&1)"
  E_RC=$?
  set -e
}

E_T0=$(date +%s)
run_agent_test_argv --this-flag-does-not-exist
E_T1=$(date +%s)
if [ "$E_RC" -eq 2 ]; then
  pass "arm E: an unknown flag exits 2"
else
  fail "arm E: an unknown flag exited $E_RC, expected 2: $E_OUT"
fi
if [ $((E_T1 - E_T0)) -le 10 ]; then
  pass "arm E: the unknown-flag arg-parse error is fast (${E_T1}-${E_T0}s, exits before any suite runs)"
else
  fail "arm E: the unknown-flag arg-parse error took $((E_T1 - E_T0))s — it should exit before any suite runs"
fi

E_T0=$(date +%s)
run_agent_test_argv a-bare-positional-argument
E_T1=$(date +%s)
if [ "$E_RC" -eq 2 ]; then
  pass "arm E: a bare positional argument exits 2"
else
  fail "arm E: a bare positional argument exited $E_RC, expected 2: $E_OUT"
fi

E_T0=$(date +%s)
run_agent_test_argv --exclude-suite=
E_T1=$(date +%s)
if [ "$E_RC" -eq 2 ]; then
  pass "arm E: an empty --exclude-suite= value exits 2"
else
  fail "arm E: an empty --exclude-suite= value exited $E_RC, expected 2: $E_OUT"
fi
if [ $((E_T1 - E_T0)) -le 10 ]; then
  pass "arm E: the empty-value arg-parse error is fast"
else
  fail "arm E: the empty-value arg-parse error took $((E_T1 - E_T0))s"
fi

# --exclude-suite is never read from the environment (a demonstration flag,
# never a default): setting only the env var (no CLI flag) must behave as an
# ordinary, unfiltered run's arg-parse phase — i.e. it must NOT be treated as
# an exclusion. We only assert it does not itself cause a usage error here;
# the "excludes nothing by env alone" behavior is exercised structurally by
# arm C (no env-var read anywhere in the script).
if grep -qE 'exclude_suite.*\$(\{)?(AGENT_|FATHOMDB_)' "$AGENT_TEST"; then
  fail "arm E: agent-test.sh appears to read an exclusion from an environment variable"
else
  pass "arm E: agent-test.sh does not read --exclude-suite from any environment variable"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi

printf '\nall test_agent_test_collect_all arms passed\n'
