#!/usr/bin/env bash
# Demonstrate-the-catch for the agent-security exit-code policy (0.8.9.2 AC-037
# CI fix). Proves the SHARED classifier (lib-gate-policy.sh) narrows the STRICT
# exception to EXACTLY the proven-environmental case and nothing wider — so the
# verify job can tolerate AC-037's live netns layer being unavailable on
# ubuntu-latest/24.04 (userns blocked by AppArmor) WITHOUT going vacuously green
# on a real egress violation or a real toolchain blocker.
#
# This is the RED-confirmable guardrail: flip any cell of the truth table (e.g.
# make rc=3 always DOWNGRADE regardless of opt-in, or let optional mask a real
# VIOLATION/BLOCKER) and this test goes red, failing the battery it runs inside.
#
# Exits:
#   0 — policy is honest (every truth-table cell holds).
#   1 — the policy drifted (a cell is wrong) — the gate would be vacuous.
#   2 — toolchain blocker (the lib is missing — should never happen in-tree).
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB="$SCRIPT_DIR/lib-gate-policy.sh"
if [ ! -r "$LIB" ]; then
    echo "policy self-test BLOCKER: $LIB missing" >&2
    exit 2
fi
# shellcheck source=scripts/security/lib-gate-policy.sh
. "$LIB"

rc=0
fail() { echo "POLICY DRIFT: $*" >&2; rc=1; }

expect_outcome() {
    local got want
    got="$(gate_outcome "$1" "$2")"
    want="$3"
    if [ "$got" != "$want" ]; then
        fail "gate_outcome rc=$1 optional=$2 => $got (want $want)"
    fi
}

expect_exit() {
    local got want
    got="$(battery_exit_code "$1" "$2" "$3")"
    want="$4"
    if [ "$got" != "$want" ]; then
        fail "battery_exit_code v=$1 b=$2 strict=$3 => $got (want $want)"
    fi
}

# --- gate_outcome truth table ------------------------------------------------
# rc=0 PASS regardless of opt-in.
expect_outcome 0 0 PASS
expect_outcome 0 1 PASS

# rc=1 is a REAL violation — opt-in must NEVER mask it.
expect_outcome 1 0 VIOLATION
expect_outcome 1 1 VIOLATION

# rc=2 is a REAL toolchain blocker (strace/binary/unshare missing) — opt-in must
# NOT downgrade it; only the rc=3 *environmental* code is ever downgradable.
expect_outcome 2 0 BLOCKER
expect_outcome 2 1 BLOCKER

# rc=3 is the environmental (userns-unavailable) signal:
#   - WITHOUT opt-in it stays a BLOCKER, so the authoritative ubuntu-22.04
#     security job (which does NOT opt in) still hard-fails if its userns ever
#     breaks — no vacuous green on the gate's home runner.
#   - WITH opt-in (the verify job on ubuntu-latest) it is an accepted DOWNGRADE.
expect_outcome 3 0 BLOCKER
expect_outcome 3 1 DOWNGRADE

# Unknown / unexpected rc must never silently pass — treat as a violation.
expect_outcome 7 1 VIOLATION

# --- battery_exit_code -------------------------------------------------------
# Clean battery → 0.
expect_exit 0 0 1 0
expect_exit 0 0 0 0
# A real violation is fatal regardless of STRICT and dominates blockers.
expect_exit 1 0 0 1
expect_exit 1 0 1 1
expect_exit 3 2 0 1
# A real (non-environmental) blocker fails only under STRICT.
expect_exit 0 1 1 2
expect_exit 0 1 0 0
# KEY honesty property: an environmental DOWNGRADE is counted separately and
# never lands in `blockers`, so STRICT=1 with only downgrades present → PASS.
# (run_gate increments `downgrades`, not `blockers`, for a DOWNGRADE outcome —
# this asserts the aggregate decision that follows.)
expect_exit 0 0 1 0

if [ "$rc" -eq 0 ]; then
    echo "policy self-test OK: exit-code classification is honest (STRICT narrowed to the environmental case)."
fi
exit "$rc"
