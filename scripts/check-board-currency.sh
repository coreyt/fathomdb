#!/usr/bin/env bash
# check-board-currency.sh — board/git-ancestry drift detector.
#
# Shared by two callers (status-board-currency-enforcement design, items 2+3):
#   * scripts/preflight.sh --landing (PREVENT, land-time gate)
#   * .github/workflows/ci.yml board-currency job (DETECT, non-bypassable CI
#     backstop on `main` for whatever slips preflight or predates it)
# Reuse, not reimplementation: both callers invoke THIS script so the
# staleness predicate cannot diverge between the two homes.
#
# STALENESS PREDICATE (evidence-based; no network; O(commits) + O(boards), cheap):
#   For every dev/plans/runs/STATUS-0.8.*.md file that is NOT already banner-
#   marked "CLOSED — historical record" in its first 5 lines (i.e. the
#   currently-live release board(s) — closed boards are frozen, nothing lands
#   into them again, so they are out of scope and never scanned):
#     1. Parse the release version from the filename (STATUS-0.8.20.md -> 0.8.20).
#     2. Walk `git log <tip>` (default tip = HEAD; newest-first, git's default
#        order) for commits whose subject matches this repo's own landing-merge
#        convention for that release: `merge(<version>): Slice[- ]<N> ...`.
#     3. Because the walk is newest-first, the FIRST commit seen for a given
#        slice number N is that slice's CURRENT (most recent) land — an
#        intermediate/superseded partial merge for the same slice (there can
#        be more than one across a slice's history) is intentionally NOT
#        re-checked once a newer one for the same N has been seen; only the
#        live state must be reflected, not every past partial land.
#     4. For each slice's current landing commit, require its short SHA
#        (first 8 hex chars) to appear as a literal substring somewhere in
#        the board file. If it does not, that slice landed (by git ancestry —
#        the merge commit is reachable from the tip) while its board never
#        mentions the commit that landed it: a demonstrable contradiction,
#        not a wording heuristic. HARD fail.
#
# This is a floor, not a full semantic reader of the board's prose — it does
# not parse "LANDED" / "IN FLIGHT" wording, which the current boards' own
# SUPERSEDED-banner convention shows is unreliable to regex over (historical
# close-record sections deliberately retain stale wording as history). Requiring
# the literal SHA is something a board legitimately cannot satisfy without
# actually being touched at land time, which is exactly the failure this gate
# closes (the board sat untouched for 4 days after two real merges).
#
# Known scope limit: only recognizes the `merge(0.8.z): Slice[- ]N ...` subject
# convention (0.8.20's, the only currently-live board). Older releases used other
# merge-subject shapes (e.g. `merge(0.8.4/Slice 5)`) but their boards are already
# banner-marked CLOSED and are skipped, so this does not affect them. If a future
# release adopts a different merge-subject convention, extend the regex below
# rather than adding a second predicate implementation.
#
# Usage:
#   scripts/check-board-currency.sh [--tip <ref>] [--boards-dir <dir>]
#
# Exit codes: 0 = every live board's most-recent per-slice land is referenced
#             by SHA; 1 = at least one demonstrable board/git contradiction.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

TIP="HEAD"
BOARDS_DIR="dev/plans/runs"

while [ $# -gt 0 ]; do
  case "$1" in
    --tip)         TIP="${2:?--tip needs a ref}"; shift 2 ;;
    --boards-dir)  BOARDS_DIR="${2:?--boards-dir needs a dir}"; shift 2 ;;
    *) printf 'check-board-currency: unknown arg %q\n' "$1" >&2; exit 2 ;;
  esac
done

if ! git rev-parse --verify -q "${TIP}^{commit}" >/dev/null; then
  printf 'check-board-currency: --tip %q does not resolve to a commit\n' "$TIP" >&2
  exit 2
fi

STALE=0

shopt -s nullglob
for board in "$BOARDS_DIR"/STATUS-0.8.*.md; do
  # Closed boards are self-labelled and frozen -- never scanned (see predicate
  # above). Header window is 15 lines, not 5: a board with T3 YAML frontmatter
  # (--- ... ---) pushes the banner line further down (measured: line 10 on
  # STATUS-0.8.9.1.md, vs line 3 on every frontmatter-less closed board) --
  # still a "header region" scan, not a whole-file grep that could incidentally
  # match a live board's own prose describing a DIFFERENT closed board.
  if head -n 15 "$board" | grep -qiE 'CLOSED — historical record'; then
    continue
  fi

  ver="$(basename "$board" .md | sed -n 's/^STATUS-\(0\.8\.[0-9.]*\)$/\1/p')"
  if [ -z "$ver" ]; then
    continue # non-standard board name (e.g. STATUS-phase12.md) -- not this gate's shape
  fi
  ver_escaped="$(printf '%s' "$ver" | sed 's/\./\\./g')"

  declare -A SEEN_SLICE=()
  while IFS= read -r line; do
    [ -n "$line" ] || continue
    sha="${line%% *}"
    subject="${line#* }"
    if [[ "$subject" =~ ^merge\(${ver_escaped}\):[[:space:]]Slice[-[:space:]]([0-9]+) ]]; then
      slice_n="${BASH_REMATCH[1]}"
      if [ -n "${SEEN_SLICE[$slice_n]+x}" ]; then
        continue # superseded intermediate merge for a slice already seen at a newer commit
      fi
      SEEN_SLICE[$slice_n]=1
      short="${sha:0:8}"
      if ! grep -qF "$short" "$board"; then
        printf 'STALE  %s Slice %s: landing commit %s ("%s") is an ancestor of %s but its SHA is not referenced anywhere in %s\n' \
          "$ver" "$slice_n" "$short" "$subject" "$TIP" "$board" >&2
        STALE=1
      fi
    fi
  # NOTE: deliberately no pathspec here. `git log -- <path>` applies default
  # history simplification, which PRUNES merge commits whose tree is TREESAME to
  # a parent (i.e. exactly the --no-ff landing merges this predicate targets) —
  # measured: a merge commit is silently invisible with `-- .` appended. A full,
  # unfiltered `git log` does not simplify and always includes merge commits.
  done < <(git log "$TIP" --format='%H %s')
  unset SEEN_SLICE
done

if [ "$STALE" -eq 0 ]; then
  printf 'ok    board-currency: no live STATUS-0.8.z.md board contradicts git ancestry\n' >&2
fi

exit "$STALE"
