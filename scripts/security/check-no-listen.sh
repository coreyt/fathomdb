#!/usr/bin/env bash
# AC-036 no-listen-syscall capture. Wraps the `security_cycle` example
# binary under strace, scoped to socket() + listen(), and asserts the
# captured trace contains no successful `listen(` call.
#
# Exits:
#   0 — clean trace (no listen() observed) and the cycle ran green.
#   1 — at least one successful listen() recorded (AC-036 violation).
#   2 — toolchain blocker (strace missing, binary missing, etc).
#
# Toolchain: strace is portable and unprivileged. bpftrace was the
# alternative cited in the AC; we chose strace because CI runners are
# heterogeneous and strace ships in apt-get default repos at <1MB.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

if ! command -v strace >/dev/null 2>&1; then
    echo "AC-036 BLOCKER: strace not installed. Add to scripts/bootstrap.sh" >&2
    echo "  (apt install strace, ~50KB, unprivileged)." >&2
    exit 2
fi

BIN="$REPO_ROOT/target/debug/examples/security_cycle"
if [ ! -x "$BIN" ]; then
    echo "AC-036: building security_cycle example binary..." >&2
    (cd "$REPO_ROOT" && cargo build -p fathomdb-engine --example security_cycle --quiet)
fi
if [ ! -x "$BIN" ]; then
    echo "AC-036 BLOCKER: security_cycle binary missing after build: $BIN" >&2
    exit 2
fi

TMPDIR="$(mktemp -d -t fdb-ac036-XXXXXX)"
trap 'rm -rf "$TMPDIR"' EXIT
TRACE="$TMPDIR/strace.log"
DB="$TMPDIR/cycle.sqlite"

# -f follow forks (engine spawns reader workers as threads, not procs,
# but -f costs nothing and protects against future fork-based code).
# -e trace=socket,listen narrows the kernel surface we record.
if ! strace -f -e trace=socket,listen -o "$TRACE" "$BIN" "$DB" >/dev/null; then
    echo "AC-036: security_cycle exited non-zero under strace" >&2
    cat "$TRACE" >&2 || true
    exit 1
fi

# A successful listen() returns 0; the strace text form is
# `listen(fd, backlog) = 0` (or `= -1 EXXX (...)`). Anything matching
# `listen(...) = 0` is a public-port bind we forbid.
if grep -E 'listen\([^)]*\)\s*=\s*0' "$TRACE" >/dev/null; then
    echo "AC-036 VIOLATION: successful listen() syscall observed:" >&2
    grep -E 'listen\(' "$TRACE" >&2 || true
    exit 1
fi

echo "AC-036 OK: no successful listen() captured across security_cycle."
