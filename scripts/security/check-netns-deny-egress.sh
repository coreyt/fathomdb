#!/usr/bin/env bash
# AC-037 netns-deny-egress fixture. Runs the security_cycle binary
# inside an unprivileged user + network namespace with loopback as
# the only interface, traces `connect()` syscalls, and asserts every
# observed connect targets AF_UNIX or 127.0.0.1 / ::1.
#
# Exits:
#   0 — clean trace (no off-loopback connect attempts).
#   1 — at least one off-loopback connect observed.
#   2 — toolchain blocker an operator must FIX (strace / unshare(1) / the
#       example binary missing). STRICT-fatal in every job.
#   3 — ENVIRONMENTAL: unprivileged userns is unavailable on this host (e.g.
#       ubuntu-latest/24.04 AppArmor blocks it), so the live netns layer
#       cannot run here. Distinct from rc=2 so the caller can narrow the
#       STRICT exception to exactly this proven-environmental case (see
#       scripts/security/lib-gate-policy.sh). The dedicated ubuntu-22.04
#       `security` CI job, where userns IS available, remains the
#       authoritative AC-037-live gate.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Shared egress classifier (EGRESS_ALLOW_RE + egress_violations) — the SAME
# logic the demonstrate-the-catch test (check-netns-deny-egress-catch.sh)
# exercises, so the gate and its catch-proof cannot drift.
# shellcheck source=scripts/security/lib-egress-allowlist.sh
. "$SCRIPT_DIR/lib-egress-allowlist.sh"

if ! command -v strace >/dev/null 2>&1; then
    echo "AC-037 BLOCKER: strace not installed. Add to scripts/bootstrap.sh" >&2
    exit 2
fi
if ! command -v unshare >/dev/null 2>&1; then
    echo "AC-037 BLOCKER: unshare(1) not available" >&2
    exit 2
fi

# Environmental capability checks come BEFORE the cargo build: a host that
# can't create a rootless userns (e.g. ubuntu-latest/24.04 AppArmor) should
# surface the environmental downgrade fast, without paying for a build it can
# never use. These exit 3 (environmental), distinct from the rc=2 toolchain
# blockers above/below, so the caller can narrow the STRICT exception to
# exactly this case (lib-gate-policy.sh).
if [ -r /proc/sys/kernel/unprivileged_userns_clone ]; then
    if [ "$(cat /proc/sys/kernel/unprivileged_userns_clone)" != "1" ]; then
        echo "AC-037 ENVIRONMENTAL: unprivileged user namespaces disabled by kernel" >&2
        echo "  (sysctl kernel.unprivileged_userns_clone=1 would enable the live layer)" >&2
        exit 3
    fi
fi
# Probe rootless userns+netns up-front. On ubuntu-latest/24.04 AppArmor blocks
# this even though unprivileged_userns_clone is absent/1 — the probe is the
# authoritative environmental gate.
if ! unshare -rUn true >/dev/null 2>&1; then
    echo "AC-037 ENVIRONMENTAL: unshare -rUn failed; rootless userns+netns unavailable on this host" >&2
    echo "  (the live netns layer runs on the dedicated ubuntu-22.04 security CI job)" >&2
    exit 3
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

# Any connect() outside loopback is a violation. The allowlist + classifier
# live in lib-egress-allowlist.sh (egress_violations / EGRESS_ALLOW_RE), shared
# with the demonstrate-the-catch test. AF_UNIX paths show as
# connect(fd, {sa_family=AF_UNIX, sun_path="..."}); AF_INET shows as
# sin_addr=inet_addr("...") or sin_addr=htonl(0x7f000001).
VIOLATIONS="$(egress_violations "$TRACE")"

if [ -n "$VIOLATIONS" ]; then
    echo "AC-037 VIOLATION: off-loopback connect() observed:" >&2
    echo "$VIOLATIONS" >&2
    exit 1
fi

echo "AC-037 OK: all connect() syscalls were loopback / AF_UNIX / AF_NETLINK."
