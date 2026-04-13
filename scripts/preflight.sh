#!/usr/bin/env bash
# Pre-flight checks for agent harness launches on fathomdb.
#
# Usage:
#   ./scripts/preflight.sh                # standard agent-harness checks
#   ./scripts/preflight.sh --baseline     # include cargo check baseline (slow)
#   ./scripts/preflight.sh --release      # include feature-gated clippy checks
#                                         # that CI runs but agent preflight
#                                         # normally skips. Use this before
#                                         # release prep if you don't want to
#                                         # run the full preflight-CI.sh.
#
# Exit codes:
#   0 = all gates pass
#   1 = one or more gates failed (fix before launching agents)
#
# For release preparation, prefer ./scripts/preflight-CI.sh — it runs every
# gate CI runs (including Python/TypeScript/Go). This script is optimized
# for agent-harness speed and skips feature-gated clippy / test invocations
# unless --release is passed.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

INCLUDE_BASELINE=false
INCLUDE_RELEASE=false
for arg in "$@"; do
    case "$arg" in
        --baseline) INCLUDE_BASELINE=true ;;
        --release)  INCLUDE_RELEASE=true ;;
    esac
done

FAILED=0
WARN=0

pass()  { echo "  ✓ $1"; }
fail()  { echo "  ✗ $1"; FAILED=$((FAILED + 1)); }
warn()  { echo "  ~ $1"; WARN=$((WARN + 1)); }

echo "── Pre-flight checks ──"
echo ""

# 1. Branch
BRANCH=$(git branch --show-current)
echo "Branch: $BRANCH"
if [ "$BRANCH" = "main" ]; then
    pass "On main"
else
    warn "Not on main (on $BRANCH) — verify this is expected"
fi

# 2. HEAD
HEAD=$(git log --oneline -1)
echo "HEAD:   $HEAD"
pass "HEAD recorded"

# 3. Clean working tree (tracked files only)
DIRTY=$(git status --short | grep "^ M" || true)
if [ -z "$DIRTY" ]; then
    pass "No modified tracked files"
else
    fail "Dirty tracked files — commit or stash before launching agents:"
    echo "$DIRTY" | sed 's/^/        /'
fi

# 4. Worktrees
git worktree prune 2>/dev/null
WORKTREE_COUNT=$(git worktree list | wc -l)
ACTIVE_WORKTREES=$((WORKTREE_COUNT - 1))
if [ "$ACTIVE_WORKTREES" -lt 3 ]; then
    pass "Worktrees: $ACTIVE_WORKTREES active (max 3)"
else
    fail "Too many worktrees: $ACTIVE_WORKTREES active (max 3). Remove stale ones:"
    git worktree list | tail -n +2 | sed 's/^/        /'
fi

# 5. Disk space
check_disk() {
    local mount=$1
    local avail_kb
    avail_kb=$(df --output=avail "$mount" 2>/dev/null | tail -1 | tr -d ' ')
    local avail_gb=$((avail_kb / 1048576))
    if [ "$avail_gb" -ge 10 ]; then
        pass "$mount: ${avail_gb}GB free"
    else
        fail "$mount: only ${avail_gb}GB free (need >10GB)"
    fi
}
check_disk /

# 6. Toolchain
if command -v cargo >/dev/null 2>&1; then
    CARGO_VER=$(cargo --version 2>/dev/null || echo "FAILED")
    pass "Cargo: $CARGO_VER"
else
    fail "Cargo not on PATH"
fi

if command -v rustc >/dev/null 2>&1; then
    RUSTC_VER=$(rustc --version 2>/dev/null || echo "FAILED")
    pass "Rustc: $RUSTC_VER"
else
    fail "Rustc not on PATH"
fi

# 7. Python venv (optional — only if fathomdb-python work is planned)
if [ -d "python/.venv" ] && [ -f "python/.venv/bin/python" ]; then
    pass "Python venv exists at python/.venv"
else
    warn "No python/.venv — run 'pip install -e python/' if you need Python bindings"
fi

# 8. Baseline (optional, expensive)
if [ "$INCLUDE_BASELINE" = true ]; then
    echo ""
    echo "── Baseline cargo check ──"
    cargo check --workspace --quiet 2>&1 | tail -5
fi

# 9. Release gates (optional, catches CI-only failures)
if [ "$INCLUDE_RELEASE" = true ]; then
    echo ""
    echo "── Release gates (feature-gated clippy) ──"
    # --features tracing catches used_underscore_binding on `error = %err`
    # spans in trace_* macros, which the default clippy run misses because
    # trace_* expand to no-ops without the feature.
    if cargo clippy --workspace --all-targets --features tracing \
        -- -D warnings -A missing-docs >/dev/null 2>&1; then
        pass "clippy --features tracing"
    else
        fail "clippy --features tracing (run manually for details)"
    fi
    # --features python mirrors the pyo3 binding build CI runs.
    if cargo clippy --workspace --all-targets --features python \
        -- -D warnings -A missing-docs >/dev/null 2>&1; then
        pass "clippy --features python"
    else
        fail "clippy --features python (run manually for details)"
    fi
    echo ""
    echo "  Note: preflight.sh --release does NOT run feature-gated tests,"
    echo "  Python / TypeScript / Go gates, or docs build. Use"
    echo "  ./scripts/preflight-CI.sh for the full CI-equivalent set."
fi

# 10. Summary
echo ""
echo "── Result ──"
if [ "$FAILED" -gt 0 ]; then
    echo "BLOCKED: $FAILED gate(s) failed. Fix before launching agents."
    exit 1
elif [ "$WARN" -gt 0 ]; then
    echo "READY with $WARN warning(s). Review above before proceeding."
    exit 0
else
    echo "READY. All gates passed."
    exit 0
fi
