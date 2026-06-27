#!/usr/bin/env bash
# AC-037 demonstrate-the-catch (0.8.9 R-037-2).
#
# The AC-037 gate (check-netns-deny-egress.sh) asserts the engine made no
# off-loopback connect(). A label-flip or an over-broad allowlist could turn it
# vacuous — passing even when egress happens — and nobody would notice, because
# the production binary never egresses (conformance-rewrite-vacuous-green-trap).
# This test proves the gate's classifier ACTUALLY trips on egress, using the
# SAME shared classifier the gate ships (lib-egress-allowlist.sh).
#
# Two layers:
#   1. Offline classifier proof — ALWAYS runs (no userns needed): feed canned
#      strace fixtures through egress_violations() and assert a known
#      off-loopback connect() IS flagged and a loopback/AF_UNIX/netlink trace
#      is NOT. This is the per-push demonstrate-the-catch.
#   2. Live netns proof — best-effort, only where rootless userns is available
#      (the ubuntu-22.04 CI security job): run a deliberately-egressing command
#      inside `unshare -rUn` under strace and assert the classifier flags it.
#      Skipped (NOT failed) where userns is unavailable — layer 1 already
#      proved the catch.
#
# Exits:
#   0 — catch proven.
#   1 — the classifier FAILED to catch a known egress (the gate is vacuous) or
#       wrongly flagged a clean loopback trace.
#   2 — toolchain blocker for the offline layer (grep missing — should not
#       happen on any supported host).
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/security/lib-egress-allowlist.sh
. "$SCRIPT_DIR/lib-egress-allowlist.sh"

if ! command -v grep >/dev/null 2>&1; then
    echo "AC-037-catch BLOCKER: grep not available" >&2
    exit 2
fi

WORK="$(mktemp -d -t fdb-ac037-catch-XXXXXX)"
trap 'rm -rf "$WORK"' EXIT

# --- Layer 1: offline classifier proof (deterministic, always runs) ---------
BAD="$WORK/egress.trace"
GOOD="$WORK/loopback.trace"

# Off-loopback connect()s as strace renders them (IPv4 8.8.8.8:53, IPv6 public
# address). The classifier MUST flag both. The `= -1 ENETUNREACH` tail mirrors
# a connect attempt inside an egress-less netns (the syscall is recorded even
# though it fails).
cat >"$BAD" <<'EOF'
connect(3, {sa_family=AF_INET, sin_port=htons(53), sin_addr=inet_addr("8.8.8.8")}, 16) = -1 ENETUNREACH (Network is unreachable)
connect(4, {sa_family=AF_INET6, sin6_port=htons(443), inet_pton(AF_INET6, "2606:4700:4700::1111", &sin6_addr), sin6_scope_id=0}, 28) = -1 ENETUNREACH (Network is unreachable)
EOF

# Loopback / AF_UNIX / netlink connect()s — all allowlisted. The classifier
# MUST NOT flag any of these.
cat >"$GOOD" <<'EOF'
connect(3, {sa_family=AF_UNIX, sun_path="/run/foo.sock"}, 110) = 0
connect(4, {sa_family=AF_INET, sin_port=htons(8080), sin_addr=htonl(0x7f000001)}, 16) = 0
connect(5, {sa_family=AF_INET, sin_port=htons(9000), sin_addr=inet_addr("127.0.0.53")}, 16) = 0
connect(6, {sa_family=AF_NETLINK, nl_pid=0, nl_groups=00000000}, 12) = 0
connect(7, {sa_family=AF_INET6, sin6_port=htons(631), inet_pton(AF_INET6, "::1", &sin6_addr), sin6_scope_id=0}, 28) = 0
EOF

rc=0

bad_hits="$(egress_violations "$BAD")"
if [ -z "$bad_hits" ]; then
    echo "AC-037 CATCH FAILED: off-loopback connect() was NOT flagged — the gate is vacuous." >&2
    rc=1
else
    bad_count="$(printf '%s\n' "$bad_hits" | grep -c 'connect(')"
    echo "AC-037 catch OK (offline): $bad_count off-loopback connect() flagged:"
    printf '%s\n' "$bad_hits" | sed 's/^/    /'
fi

good_hits="$(egress_violations "$GOOD")"
if [ -n "$good_hits" ]; then
    echo "AC-037 CATCH FAILED: clean loopback/AF_UNIX trace wrongly flagged as egress:" >&2
    printf '%s\n' "$good_hits" | sed 's/^/    /' >&2
    rc=1
fi

if [ "$rc" -ne 0 ]; then
    exit "$rc"
fi

# --- Layer 2: live netns proof (best-effort; needs rootless userns) ----------
if command -v unshare >/dev/null 2>&1 && command -v strace >/dev/null 2>&1 \
   && unshare -rUn true >/dev/null 2>&1; then
    LIVE="$WORK/live.trace"
    # bash's /dev/tcp pseudo-device issues a real connect() to a literal public
    # IP (no DNS). Inside the empty netns it fails (ENETUNREACH) but strace
    # records the off-loopback attempt — exactly what the gate must catch.
    unshare -rUn -- bash -c '
        ip link set lo up 2>/dev/null || true
        strace -f -e trace=connect -o "$1" \
            bash -c "exec 3<>/dev/tcp/8.8.8.8/53" >/dev/null 2>&1 || true
    ' bash "$LIVE" 2>/dev/null || true

    if [ ! -s "$LIVE" ]; then
        echo "AC-037 catch: live netns produced no trace (strace/connect unsupported here); offline layer proved the catch."
        exit 0
    fi
    live_hits="$(egress_violations "$LIVE")"
    if [ -z "$live_hits" ]; then
        echo "AC-037 CATCH FAILED (live): deliberate egress NOT flagged in netns." >&2
        echo "  trace follows:" >&2
        cat "$LIVE" >&2 || true
        exit 1
    fi
    echo "AC-037 catch OK (live netns): deliberate egress flagged:"
    printf '%s\n' "$live_hits" | sed 's/^/    /'
else
    echo "AC-037 catch: live netns layer skipped (rootless userns unavailable); offline layer proved the catch."
fi

exit 0
