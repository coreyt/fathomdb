#!/usr/bin/env bash
# Aggregate security-fixture gate. Runs the AC-036, AC-037, AC-038,
# AC-050a, and AC-050c gates that live under scripts/security/.
#
# Exit semantics:
#   0  — every gate green (downgrades, if any, are informational).
#   1  — at least one gate found a real violation (always fatal).
#   2  — toolchain blocker (e.g. strace missing for AC-036/AC-037);
#        gates that BLOCKER do not fail the overall script unless
#        STRICT=1 is set.
#
# Per-gate exit codes and the STRICT policy live in the shared classifier
# scripts/security/lib-gate-policy.sh (gate_outcome / battery_exit_code), so
# the aggregation can't drift from its demonstrate-the-catch test.
#
# DOWNGRADE (gate rc=3): a gate's authoritative layer is *environmentally*
# unavailable on this host AND a cheaper proof already ran. The only case is
# AC-037's live netns layer, which needs unprivileged userns — blocked on
# ubuntu-latest/24.04 by AppArmor. It is honored as a non-fatal downgrade ONLY
# when the caller opts in for that gate via AC037_LIVE_OPTIONAL=1 (the verify
# job does; the authoritative ubuntu-22.04 `security` CI job does NOT, so a
# userns regression there still hard-fails). Without opt-in, rc=3 is a BLOCKER.
#
# Wiring: scripts/agent-verify.sh invokes this with STRICT=1 (and
# AC037_LIVE_OPTIONAL=1), so a real blocker fails the agent loop on hosts where
# bootstrap.sh has run. Local dev hosts without strace must either run
# scripts/bootstrap.sh (which apt-installs strace on Debian) or install it.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SEC="$SCRIPT_DIR/security"
# shellcheck source=scripts/security/lib-gate-policy.sh
. "$SEC/lib-gate-policy.sh"

violations=0
blockers=0
downgrades=0

# run_gate <label> <optional> <cmd...>
#   optional=1 → a gate's rc=3 (environmental) is an accepted downgrade here
#   (the authoritative layer runs on another runner). optional NEVER masks a
#   real violation (rc=1) or a real toolchain blocker (rc=2).
run_gate() {
    local label="$1" optional="$2"
    shift 2
    echo "==> $label"
    set +e
    "$@"
    local rc=$?
    set -e
    case "$(gate_outcome "$rc" "$optional")" in
        PASS)
            echo "    PASS" ;;
        DOWNGRADE)
            echo "    DOWNGRADE (environmental: live layer unavailable here; the offline catch ran and the dedicated ubuntu-22.04 security job is authoritative)" >&2
            downgrades=$((downgrades + 1)) ;;
        BLOCKER)
            echo "    BLOCKER (toolchain missing)" >&2
            blockers=$((blockers + 1)) ;;
        *)  # VIOLATION
            echo "    FAIL (rc=$rc)" >&2
            violations=$((violations + 1)) ;;
    esac
}

# AC-037's LIVE netns layer is the one gate whose authoritative layer can be
# delegated to the dedicated ubuntu-22.04 `security` job; opt-in is gated on
# AC037_LIVE_OPTIONAL. Every other gate passes optional=0 (no exception).
AC037_OPT="${AC037_LIVE_OPTIONAL:-0}"

run_gate "AC-036 no-listen-syscall"   0           bash "$SEC/check-no-listen.sh"
run_gate "AC-037 netns-deny-egress"   "$AC037_OPT" bash "$SEC/check-netns-deny-egress.sh"
run_gate "AC-037 catch (demonstrate)" 0           bash "$SEC/check-netns-deny-egress-catch.sh"
run_gate "AC-037 policy self-test"    0           bash "$SEC/lib-gate-policy.test.sh"
run_gate "AC-038 FTS5-injection-safe" 0           cargo test -p fathomdb-engine --test fts5_injection_safety --quiet
run_gate "AC-050a ast-scan rust"      0           python3 "$SEC/ast_scan.py" --language rust
run_gate "AC-050a ast-scan python"    0           python3 "$SEC/ast_scan.py" --language python
run_gate "AC-050a ast-scan ts"        0           python3 "$SEC/ast_scan.py" --language ts
run_gate "AC-050c removal-detect"     0           bash "$SEC/check-removal-changelog.sh"

echo
echo "agent-security: $violations violation(s), $blockers blocker(s), $downgrades downgrade(s)"

exit "$(battery_exit_code "$violations" "$blockers" "${STRICT:-0}")"
