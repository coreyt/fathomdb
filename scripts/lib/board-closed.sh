#!/usr/bin/env bash
# scripts/lib/board-closed.sh — the ONE board-CLOSED predicate.
#
# A `dev/plans/runs/STATUS-<version>.md` board is either LIVE (things still land
# into it) or CLOSED (frozen historical record). Two callers need that split and
# must never disagree about it:
#
#   * scripts/check-board-currency.sh — skips CLOSED boards, because a frozen
#     board legitimately no longer tracks git ancestry.
#   * scripts/steward-orient.sh       — selects the LIVE board, and derives the
#     release number from its filename.
#
# Shared, not reimplemented: if the two ever drifted, one of them would be
# quietly wrong about which release is current — check-board-currency would
# either scan a frozen board (noise) or skip the live one (a silent vacuous
# pass), and the briefing would report a closed release as current. So the
# predicate lives here exactly once and both callers source this file.
#
# WINDOW = 15 LINES, NOT 5. The banner is a header-region marker, but YAML
# frontmatter (`--- ... ---`) pushes it down: measured at line 10 on
# STATUS-0.8.9.1.md, versus line 3 on every frontmatter-less closed board. 15
# lines still bounds it to the header region — deliberately NOT a whole-file
# grep, which could incidentally match a LIVE board's own prose describing some
# OTHER closed board. Do not narrow this window without moving both callers.
#
# Sourced, never executed: `. "<dir>/lib/board-closed.sh"`.

# Usage: board_is_closed <path-to-STATUS-board.md>
# Exit 0 = CLOSED (frozen historical record); non-zero = LIVE.
# The head output is fed to grep by HERE-STRING, not a pipe: both callers run
# under `set -o pipefail`, where `grep -q` exiting early on a match can leave
# `head` with SIGPIPE and turn a CLOSED board into a 141 (i.e. "LIVE"). No pipe,
# no SIGPIPE, same predicate.
board_is_closed() {
  local head_window
  head_window="$(head -n 15 "$1")"
  grep -qiE 'CLOSED — historical record' <<<"$head_window"
}
