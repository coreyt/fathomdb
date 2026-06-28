#!/usr/bin/env bash
# Aggregate security-fixture gate. Runs the AC-036, AC-037, AC-038,
# AC-050a, and AC-050c gates that live under scripts/security/.
#
# Exit semantics:
#   0  — every gate green.
#   1  — at least one gate found a real violation.
#   2  — toolchain blocker (e.g. strace missing for AC-036/AC-037);
#        gates that BLOCKER do not fail the overall script unless
#        STRICT=1 is set.
#
# Wiring: scripts/agent-verify.sh invokes this with STRICT=1, so any
# blocker fails the agent loop on hosts where bootstrap.sh has run.
# Local dev hosts without strace must either run scripts/bootstrap.sh
# (which apt-installs strace on Debian) or install strace manually.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SEC="$SCRIPT_DIR/security"

violations=0
blockers=0

run_gate() {
    local label="$1"
    shift
    echo "==> $label"
    set +e
    "$@"
    local rc=$?
    set -e
    if [ "$rc" -eq 0 ]; then
        echo "    PASS"
    elif [ "$rc" -eq 2 ]; then
        echo "    BLOCKER (toolchain missing)" >&2
        blockers=$((blockers + 1))
    else
        echo "    FAIL (rc=$rc)" >&2
        violations=$((violations + 1))
    fi
}

run_gate "AC-036 no-listen-syscall"     bash "$SEC/check-no-listen.sh"
run_gate "AC-037 netns-deny-egress"     bash "$SEC/check-netns-deny-egress.sh"
run_gate "AC-037 catch (demonstrate)"   bash "$SEC/check-netns-deny-egress-catch.sh"
run_gate "AC-038 FTS5-injection-safe"   cargo test -p fathomdb-engine --test fts5_injection_safety --quiet
run_gate "AC-050a ast-scan rust"        python3 "$SEC/ast_scan.py" --language rust
run_gate "AC-050a ast-scan python"      python3 "$SEC/ast_scan.py" --language python
run_gate "AC-050a ast-scan ts"          python3 "$SEC/ast_scan.py" --language ts
run_gate "AC-050c removal-detect"       bash "$SEC/check-removal-changelog.sh"

echo
echo "agent-security: $violations violation(s), $blockers blocker(s)"

if [ "$violations" -gt 0 ]; then
    exit 1
fi
if [ "$blockers" -gt 0 ] && [ "${STRICT:-0}" = "1" ]; then
    exit 2
fi
exit 0
