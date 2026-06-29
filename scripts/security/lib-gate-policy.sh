#!/usr/bin/env bash
# Shared exit-code policy for the agent-security battery (AC-036/037/038/050a/050c).
#
# Sourced by BOTH scripts/agent-security.sh and its demonstrate-the-catch test
# (scripts/security/lib-gate-policy.test.sh) so the classification can never
# silently drift — the same trap the egress allowlist lib guards against
# (conformance-rewrite-vacuous-green-trap).
#
# This file is sourced, not executed; it defines two pure functions and has no
# side effects.
#
# Per-gate exit-code convention (every scripts/security/check-*.sh follows it):
#   0  PASS       — gate is green.
#   1  VIOLATION  — a real security defect. ALWAYS fatal (STRICT or not).
#   2  BLOCKER    — a toolchain prerequisite is missing (strace, the example
#                   binary, unshare(1)). Promoted to a hard failure under
#                   STRICT=1; a real regression an operator must fix.
#   3  DOWNGRADE  — the gate's AUTHORITATIVE layer is *environmentally*
#                   unavailable on THIS host AND a cheaper independent proof
#                   already ran. The one case: AC-037's live netns layer needs
#                   unprivileged userns, which ubuntu-latest/24.04 AppArmor
#                   blocks; the offline catch (lib-egress-allowlist.sh) still
#                   proves the classifier, and the dedicated ubuntu-22.04
#                   `security` CI job runs the live layer authoritatively.
#                   rc=3 is honored as a non-fatal downgrade ONLY where the
#                   caller opts in PER GATE (optional=1). Without opt-in it is
#                   treated as a BLOCKER, so the authoritative runner cannot go
#                   vacuously green if its userns ever breaks.

# gate_outcome <rc> <optional>  ->  PASS | VIOLATION | BLOCKER | DOWNGRADE
#
# `optional` (default 0) is set to 1 only by callers that delegate a gate's
# authoritative layer to another runner (the verify job delegates AC-037-live
# to the dedicated 22.04 security job). It NEVER masks a real violation (rc=1)
# or a real toolchain blocker (rc=2) — only the rc=3 environmental signal.
gate_outcome() {
    local rc="${1:-0}" optional="${2:-0}"
    case "$rc" in
        0) echo PASS ;;
        1) echo VIOLATION ;;
        2) echo BLOCKER ;;
        3) if [ "$optional" = "1" ]; then echo DOWNGRADE; else echo BLOCKER; fi ;;
        *) echo VIOLATION ;;  # unknown rc: fail closed, never silently pass
    esac
}

# battery_exit_code <violations> <blockers> <strict>  ->  0 | 1 | 2
#
# Downgrades are intentionally NOT a parameter: they are informational and do
# not affect the exit code (the caller increments a separate counter). A real
# violation dominates; a real blocker fails only under STRICT=1.
battery_exit_code() {
    local violations="${1:-0}" blockers="${2:-0}" strict="${3:-0}"
    if [ "$violations" -gt 0 ]; then echo 1; return; fi
    if [ "$blockers" -gt 0 ] && [ "$strict" = "1" ]; then echo 2; return; fi
    echo 0
}
