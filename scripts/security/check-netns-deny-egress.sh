#!/usr/bin/env bash
# AC-037 netns-deny-egress fixture. Runs the security_cycle binary
# inside an unprivileged user + network namespace with loopback as
# the only interface, traces `connect()` syscalls, and asserts every
# observed connect targets AF_UNIX or 127.0.0.1 / ::1.
#
# Exits:
#   0 — clean trace (no off-loopback connect attempts).
#   1 — at least one off-loopback connect observed.
#   2 — toolchain blocker (strace missing, unprivileged userns
#       disabled, binary missing).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

if ! command -v strace >/dev/null 2>&1; then
    echo "AC-037 BLOCKER: strace not installed. Add to scripts/bootstrap.sh" >&2
    exit 2
fi
if ! command -v unshare >/dev/null 2>&1; then
    echo "AC-037 BLOCKER: unshare(1) not available" >&2
    exit 2
fi
if [ -r /proc/sys/kernel/unprivileged_userns_clone ]; then
    if [ "$(cat /proc/sys/kernel/unprivileged_userns_clone)" != "1" ]; then
        echo "AC-037 BLOCKER: unprivileged user namespaces disabled by kernel" >&2
        echo "  sysctl kernel.unprivileged_userns_clone=1 required" >&2
        exit 2
    fi
fi

BIN="$REPO_ROOT/target/debug/examples/security_cycle"
if [ ! -x "$BIN" ]; then
    (cd "$REPO_ROOT" && cargo build -p fathomdb-engine --example security_cycle --quiet)
fi
if [ ! -x "$BIN" ]; then
    echo "AC-037 BLOCKER: security_cycle binary missing: $BIN" >&2
    exit 2
fi

TMPDIR="$(mktemp -d -t fdb-ac037-XXXXXX)"
trap 'rm -rf "$TMPDIR"' EXIT
TRACE="$TMPDIR/strace.log"
DB="$TMPDIR/cycle.sqlite"

# Probe unprivileged userns+netns up-front. If unshare fails the
# kernel-level capability is missing — surface as a toolchain blocker
# rather than letting the wrapper raise a confusing strace error.
if ! unshare -rUn true >/dev/null 2>&1; then
    echo "AC-037 BLOCKER: unshare -rUn failed; rootless userns+netns unavailable" >&2
    exit 2
fi

# unshare -rUn: rootless user namespace + fresh network namespace.
# Inside the netns we bring loopback up so the engine's intra-process
# UNIX/loopback bookkeeping (none expected) can still proceed.
# strace -f follows the engine's reader-pool threads.
if ! unshare -rUn -- bash -c '
    set -euo pipefail
    ip link set lo up || true
    exec strace -f -e trace=connect -o "$1" "$2" "$3" >/dev/null
' bash "$TRACE" "$BIN" "$DB"; then
    echo "AC-037: security_cycle exited non-zero in netns under strace" >&2
    cat "$TRACE" >&2 || true
    exit 1
fi

# Any connect() outside loopback is a violation. AF_UNIX paths show
# as connect(fd, {sa_family=AF_UNIX, sun_path="..."}). AF_INET shows
# as sin_addr=inet_addr("...") or sin_addr=htonl(0x7f000001).
# Allowlisted patterns: AF_UNIX, 127.x.x.x, ::1, AF_NETLINK (kernel
# RTNL chatter for ip link), AF_UNSPEC.
VIOLATIONS="$(grep -E 'connect\(' "$TRACE" | \
    grep -vE 'AF_UNIX|AF_UNSPEC|AF_NETLINK|127\.|sin_addr=htonl\(0x7f000001\)|inet_pton\(AF_INET6,\s*"::1"|inet6_addr.*::1' || true)"

if [ -n "$VIOLATIONS" ]; then
    echo "AC-037 VIOLATION: off-loopback connect() observed:" >&2
    echo "$VIOLATIONS" >&2
    exit 1
fi

echo "AC-037 OK: all connect() syscalls were loopback / AF_UNIX / AF_NETLINK."
