#!/usr/bin/env bash
# Shared loopback/AF_UNIX allowlist for the AC-037 netns-deny-egress gate.
#
# Sourced by BOTH the live gate (check-netns-deny-egress.sh) and its
# demonstrate-the-catch test (check-netns-deny-egress-catch.sh) so the two can
# never silently drift. A duplicated regex would let the catch test prove
# nothing about the real gate (conformance-rewrite-vacuous-green-trap): the
# catch must exercise the SAME classifier the gate ships.
#
# This file is sourced, not executed; it defines a regex + a function and has
# no side effects.

# A connect() line is ALLOWED (not an egress violation) iff it matches one of:
#   AF_UNIX / AF_UNSPEC / AF_NETLINK — local IPC + kernel RTNL chatter (ip link)
#   127. / htonl(0x7f000001)         — IPv4 loopback
#   ::1                              — IPv6 loopback
EGRESS_ALLOW_RE='AF_UNIX|AF_UNSPEC|AF_NETLINK|127\.|sin_addr=htonl\(0x7f000001\)|inet_pton\(AF_INET6,\s*"::1"|inet6_addr.*::1'

# egress_violations <tracefile>
# Echo every connect() line in the strace log that targets a NON-loopback,
# non-local address. Empty output ⇒ clean (no off-loopback egress observed).
egress_violations() {
    local trace="$1"
    grep -E 'connect\(' "$trace" 2>/dev/null | grep -vE "$EGRESS_ALLOW_RE" || true
}
