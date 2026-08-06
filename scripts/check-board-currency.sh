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
#   marked "CLOSED — historical record" in its first 15 lines (the shared
#   predicate in scripts/lib/board-closed.sh; see there for why 15) (i.e. the
#   currently-live release board(s) — closed boards are frozen, nothing lands
#   into them again, so they are out of scope and never scanned):
#     1. Parse the release version from the filename (STATUS-0.8.20.md -> 0.8.20).
#     2. Walk `git log <tip>` (default tip = HEAD; newest-first, git's default
#        order) for commits whose subject matches this repo's own landing-merge
#        convention for that release: `merge(<version>): Slice[- ]<N> ...`.
#        <N> is a DOTTED slice id, not an integer: `39` and `39.5` are two
#        DIFFERENT slices and must never collapse onto one key (SLICE-ID-
#        HARDENING site 1 — see the capture below for the measured consequence).
#     3. Because the walk is newest-first, the FIRST commit seen for a given
#        slice id N is that slice's CURRENT (most recent) land — an
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
#     5. Vacuous-pass guard (fix-1): if step 2 matches ZERO commits for a live
#        board's release, that board contributed zero checks in steps 3-4 and
#        the loop would otherwise pass it by default. HARD fail instead, except
#        for a state-backed newly activated release whose `landed` is empty,
#        whose `next_slice` is its earliest ladder slice, and whose complete
#        ladder remains with no recorded landing evidence — see the dedicated
#        section below.
#     6. BOARD/STATE CROSS-READ (TC-133): steps 2-4 ask git a question. This
#        step asks dev/plans/release-state-<version>.json — the single writer
#        for landed slices, their SHAs and the remaining ladder — whether the
#        board's HAND-WRITTEN rows still agree with it. See the dedicated
#        section below; this is a separate defect from step 2's slice-id
#        capture and its fix is independent.
#
# Steps 1-5 are a floor, not a full semantic reader of the board's prose. Step 6
# is deliberately NARROW rather than a prose reader: it looks only at the
# board's ladder TABLE (a structural region) and at `Ladder remaining:` claims,
# never at free prose, because the current boards' own SUPERSEDED-banner
# convention shows free prose is unreliable to regex over (historical
# close-record sections deliberately retain stale "NOT STARTED" wording as
# history — STATUS-0.8.20.md:1002/1011 do exactly that, and must not be
# flagged). Requiring the literal SHA is something a board legitimately cannot
# satisfy without actually being touched at land time, which is exactly the
# failure this gate closes (the board sat untouched for 4 days after two real
# merges).
#
# ------------------------------------------------------------------------------
# TC-133 — BOARD/STATE CROSS-READ (step 6). Added by HITL ruling 2026-07-30
# (steward seq-212 option (b); todos-ledger seq-197), as §3a of the
# SLICE-ID-HARDENING commission brief.
#
# THE DEFECT, MEASURED. Steps 2-4 require a landing merge's short SHA to appear
# SOMEWHERE in the board file. Throughout a long period
# dev/plans/runs/STATUS-0.8.20.md said Slice 39 was NOT_STARTED and `91db34d8`
# WAS present in the file — but only inside the two `<!-- BEGIN GENERATED -->`
# cells that scripts/check-release-state-views.sh renders from the state file.
# THE GENERATED HALF WAS CARRYING THE CHECK FOR THE HAND-WRITTEN HALF. This
# script exited 0 and the always-on CI job passed too.
#
# ⚠ THIS IS NOT SITE 1 AND THE TWO FIXES ARE INDEPENDENT. At site 1 (the capture
# at the [[ =~ ]] below) the slice-number CAPTURE is wrong. Here the capture is
# correct, the landing merge IS found, its SHA IS present — and the gate still
# certifies a board that contradicts the state file on every hand-written row.
# PRESENCE OF A SHA IS NOT CURRENCY OF THE ROW IT BELONGS TO.
#
# WHAT STEP 6 CHECKS, against dev/plans/release-state-<version>.json:
#   (a) every id in `landed` has a row in the board's ladder table;
#   (b) that row does not describe the slice with a not-started / in-flight /
#       next marker;
#   (c) that row cites the slice's own `ladder[].sha` — the SHA must be in the
#       ROW, not merely in the file. Its PRECONDITION is checked too (fix-2):
#       a slice in `landed` whose `ladder` entry is missing or carries no `sha`
#       is itself a HARD fail, because otherwise (c) would silently not run for
#       that slice and (b), the fragile half, would be the only thing left;
#   (d) every `Ladder remaining:` / `Remaining ladder:` claim names exactly the
#       ids in `remaining_ladder`.
# (c) is the load-bearing half and (b) is the fragile half — see the
# [DETERMINE] block on the marker vocabulary further down.
#
# STATE FILE ABSENT OR UNPARSEABLE — a STATED, VISIBLE policy, because this gate
# loops over EVERY live STATUS-0.8.z.md board and only 0.8.20 currently has a
# release-state file (the single writer arrived with DOC-HYGIENE-2 T2a):
#   * ABSENT -> the cross-read is SKIPPED and SAYS SO on stderr ("cross-read
#     (TC-133) DID NOT RUN"). A hard fail would redden the gate for boards of
#     releases that legitimately predate the single writer; a SILENT skip would
#     rebuild the exact blind spot this step closes, so it is announced.
#   * PRESENT BUT UNPARSEABLE -> HARD fail. That release DOES declare a single
#     writer and the gate cannot vouch for the board without reading it. Same
#     failure family as the fix-1 vacuous-pass guard: never report ok by
#     default. "Unparseable" means bad JSON, a top level that is not an object,
#     a missing `landed`/`ladder`/`remaining_ladder`, one of those three not
#     being a JSON array, or a `ladder` element that is not a JSON object —
#     TYPE, not merely key presence (fix-2 finding B; see the validation below
#     for what a key-only check let through and why it fails OPEN in preflight).
# Both policies are covered by arms in scripts/tests/test_check_board_currency.sh.
# ------------------------------------------------------------------------------
#
# Known scope limit: only recognizes the `merge(0.8.z): Slice[- ]N ...` subject
# convention (0.8.20's, the only currently-live board). Older releases used other
# merge-subject shapes (e.g. `merge(0.8.4/Slice 5)`) but their boards are already
# banner-marked CLOSED and are skipped, so this does not affect them. If a future
# release adopts a different merge-subject convention, extend the regex below
# rather than adding a second predicate implementation.
#
# VACUOUS-PASS GUARD (fix-1, same failure class as TC-37's markdownlint-cli2
# hole): the predicate above only fires when a landing-merge subject is FOUND.
# If a LIVE (non-CLOSED, version-parseable) board's release has ZERO commits
# matching the convention reachable from <tip> -- a squash-land, a reworded
# merge, a historical slash-style subject, or simply nothing has landed yet --
# the per-slice loop body never runs, STALE never flips, and the board passes
# green while the gate has actually vouched for nothing. That is a silent
# vacuous pass, not a real currency check, so it is itself a HARD failure:
# every LIVE board must show at least one recognized land, or the gate must say
# loudly that it could not vouch for it (see the "no landing merge matched"
# message below), never report ok by default. The only exception is a newly
# activated state whose empty `landed`, first `next_slice`, and complete
# `remaining_ladder` prove no merge can exist yet. Its earliest (`next`) entry
# may be `IN_PROGRESS` or `NOT_STARTED`; every later entry must be
# `NOT_STARTED`, and none may carry a landing SHA. Its STATUS ladder is
# structurally validated by the unconditional TC-133 cross-read. This can only
# convert a silent pass into a loud failure -- it never fires for a board with
# >=1 matched slice.
#
# Usage:
#   scripts/check-board-currency.sh [--tip <ref>] [--boards-dir <dir>]
#
# Exit codes: 0 = every live board's most-recent per-slice land is referenced
#             by SHA (or is a state-backed newly activated release), and every
#             live board with a readable release-state file agrees with it; 1 = at
#             least one demonstrable board/git contradiction, OR a live board
#             matched zero landing merges (vacuous-pass guard, fix-1), OR a
#             board/state contradiction or unreadable state file (TC-133).
set -euo pipefail

# The legacy CLOSED-banner predicate remains for pre-release-state fixtures and
# historical boards. A modern checkout delegates current-release selection to
# release-current.py below, which additionally recognizes a published state as
# closure even if its retained board predates the CLOSED banner.
_CBC_SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_CBC_LIB_DIR="$_CBC_SCRIPT_DIR/lib"
# shellcheck source=lib/board-closed.sh
. "$_CBC_LIB_DIR/board-closed.sh"

# `git rev-parse` failing here used to degrade to `cd ""` — a bash no-op that
# leaves the script running in an arbitrary cwd. Bind and check it instead.
_repo_toplevel="$(git rev-parse --show-toplevel)" || exit 1
cd "$_repo_toplevel" || exit 1

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

# ---------------------------------------------------------------------------
# TC-133 board/state cross-read (step 6). Returns 0 = agrees (or the state file
# is absent, which is announced); 1 = contradiction / unreadable state file.
# Python because the input is JSON and bash cannot read JSON without inventing
# a parser; python3 embedded in a bash gate is this repo's house style
# (commission-manifest.sh, check-release-state-views.sh).
# ---------------------------------------------------------------------------
cbc_cross_read() {
  local ver="$1" board="$2" state="$3" allow_concise="$4"
  if ! command -v python3 >/dev/null 2>&1; then
    printf 'STALE  %s: python3 is not on PATH, so the board/state cross-read (TC-133) could not run — the gate cannot vouch for %s\n' \
      "$ver" "$board" >&2
    return 1
  fi
  python3 - "$ver" "$board" "$state" "$allow_concise" <<'CROSS_READ_PY'
import json
import re
import sys

ver, board_path, state_path, allow_concise = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4] == "1"


def err(msg):
    sys.stderr.write(msg + "\n")


# --- state file: absent -> announced skip; unreadable -> HARD fail -----------
try:
    with open(state_path, encoding="utf-8") as fh:
        raw = fh.read()
except FileNotFoundError:
    err("note   %s: no %s — the board/state cross-read (TC-133) DID NOT RUN for "
        "this board; only the git-ancestry SHA check vouched for it. Releases "
        "that predate the single-writer state file (DOC-HYGIENE-2 T2a) have no "
        "such file, so this is not a failure — but it is not a full check "
        "either, which is why it is said out loud." % (ver, state_path))
    sys.exit(0)
except OSError as exc:
    err("STALE  %s: %s could not be read as release state (%s) — the board/state "
        "cross-read (TC-133) cannot vouch for %s" % (ver, state_path, exc, board_path))
    sys.exit(1)

# VALIDATE THE TYPE, NOT JUST THE KEY (fix-2 finding B). The first draft asked
# only `if key not in st`, which is a shape check that stops one step short: a
# file carrying `"landed": 5` PASSED it and then detonated in the loop below as
# a bare `TypeError: 'int' object is not iterable` — an UNPREFIXED Python
# traceback. That mattered far beyond tidiness, because the traceback carries no
# `STALE` prefix and preflight.sh --landing used to promote ONLY `STALE*` lines
# to HARD (see the anti-fail-open guard now in preflight.sh section 7): the
# cross-read's failure was DOWNGRADED TO INFO and the land was certified. Every
# diagnostic this validation raises is re-emitted with the `STALE` prefix by the
# handler below, which is what makes it a land-blocking failure.
#
# `ladder` is validated as a list of OBJECTS, not merely as a list, because the
# `by_slice` builder below guarded itself with `isinstance(entry, dict)` and so
# SILENTLY DROPPED every non-object entry. Measured: with `"ladder": [5, 40]`
# the map came out EMPTY, check (c) then went no-op for every landed slice via
# the (by_slice.get(key) or {}) construct that finding A closes, and the gate
# CERTIFIED the malformed file with rc=0 — no exception, no diagnostic. Two
# independent fail-opens composing into one green.
try:
    st = json.loads(raw)
    if not isinstance(st, dict):
        raise ValueError("top level is %s, not a JSON object" % type(st).__name__)
    for key in ("landed", "ladder", "remaining_ladder"):
        if key not in st:
            raise ValueError("required key %r is absent" % key)
        if not isinstance(st[key], list):
            raise ValueError("required key %r is a %s, not a JSON array"
                             % (key, type(st[key]).__name__))
    for idx, entry in enumerate(st["ladder"]):
        if not isinstance(entry, dict):
            raise ValueError("ladder[%d] is a %s, not a JSON object"
                             % (idx, type(entry).__name__))
except Exception as exc:  # noqa: BLE001 - any malformed state is the same verdict
    err("STALE  %s: %s exists but could not be read as release state (%s) — the "
        "board/state cross-read (TC-133) cannot vouch for %s. A release that "
        "declares a single writer must have a readable one; falling back to the "
        "SHA-only predicate here would be the silent vacuous pass this step "
        "closes." % (ver, state_path, exc, board_path))
    sys.exit(1)


def slice_str(n):
    """Render a slice id: 40 -> "40", 39.5 -> "39.5". NEVER %d — that would
    truncate a fractional slice onto its integer neighbour, which is the
    SLICE-ID-HARDENING defect class this whole unit exists to end. Same
    rendering as check-release-state-views.sh's `_slice_str`."""
    return "%g" % n if isinstance(n, float) else str(n)


def norm(cell):
    """Strip markdown emphasis/code fencing so a marker cannot hide behind it."""
    return cell.replace("*", "").replace("`", "").strip()


try:
    board_lines = open(board_path, encoding="utf-8").read().split("\n")
except OSError as exc:
    err("STALE  %s: %s could not be read (%s) — the board/state cross-read "
        "(TC-133) cannot vouch for it" % (ver, board_path, exc))
    sys.exit(1)


def cells(line):
    """Split a markdown table row into cells, or None if it is not one."""
    s = line.strip()
    if not s.startswith("|"):
        return None
    s = s[1:]
    if s.endswith("|"):
        s = s[:-1]
    return s.split("|")


# --- locate the ladder TABLE, structurally --------------------------------
# The region is identified by its own header row (`| Slice | ... | Status |`)
# plus the markdown delimiter row beneath it — NOT by a section heading and NOT
# by scanning the whole file. Anchoring on the table shape is what keeps
# historical close-record prose (which deliberately retains stale "NOT STARTED"
# wording) out of scope, and it is also what keeps OTHER `| 22 | ... |` tables
# out of scope: STATUS-0.8.20.md:316 is an HITL-DECISION row numbered 22 whose
# text contains "not started", and reading it as Slice 22's status would be a
# false positive that hard-blocks a land.
DELIM = re.compile(r":?-{2,}:?$")
rows = {}
dup = []
tables = 0
i = 0
while i < len(board_lines):
    hdr = cells(board_lines[i])
    if (hdr and len(hdr) >= 2
            and norm(hdr[0]).lower() == "slice"
            and norm(hdr[-1]).lower() == "status"):
        delim = cells(board_lines[i + 1]) if i + 1 < len(board_lines) else None
        if delim and all(DELIM.match(c.strip()) for c in delim):
            tables += 1
            j = i + 2
            while j < len(board_lines):
                rc = cells(board_lines[j])
                if not rc:
                    break
                key = norm(rc[0])
                if re.fullmatch(r"[0-9]+(?:\.[0-9]+)*", key):
                    if key in rows:
                        dup.append((key, rows[key][0], j + 1))
                    else:
                        rows[key] = (j + 1, rc[-1])
                j += 1
            i = j
            continue
    i += 1

bad = False

if tables == 0:
    if allow_concise:
        err("note   %s: %s intentionally carries no ladder table; canonical "
            "release-state SHA ancestry facts were checked instead."
            % (ver, board_path))
        sys.exit(0)
    err("STALE  %s: %s carries no ladder table (no `| Slice | ... | Status |` "
        "header row followed by a delimiter row), so the board/state cross-read "
        "(TC-133) would vouch for nothing. A live board with a release-state "
        "file must expose its per-slice status in a table the gate can read."
        % (ver, board_path))
    sys.exit(1)

for key, first, second in dup:
    # Mirrors check-release-state-views.sh `_by_slice`, which refuses a
    # collision rather than taking the last writer.
    err("STALE  %s: the ladder table lists Slice %s TWICE (%s:%d and %s:%d). One "
        "row's status would silently answer for the other's; give each slice "
        "exactly one row (TC-133)" % (ver, key, board_path, first, board_path, second))
    bad = True

# --- [DETERMINE] marker detection: FIXED VOCABULARY, not derived -------------
# DETERMINATION: a fixed, explicitly-listed vocabulary. Reasoning:
#   * There is nothing to derive it FROM. The state file only ever authors
#     POSITIVE status ("LANDED"); it carries no field enumerating words that
#     mean "not done". Deriving the negative set from the complement of the
#     positive one means deriving it from open-ended English — unbounded and
#     unfalsifiable.
#   * The one plausible derivation — "use the `ladder[].status` values the
#     state file currently assigns to non-landed slices" — is SELF-WEAKENING:
#     it shrinks to the empty set exactly when every slice is landed, i.e. at
#     the end of a release, which is precisely when a board is most likely to
#     be stale. And it could only ever contain the STATE FILE's vocabulary,
#     never the BOARD's prose vocabulary — but the board's prose is the thing
#     under test.
#   * A fixed list is auditable: its gaps are visible in one place, here.
#
# ⚠ WHAT THIS REGEX CAN AND CANNOT CATCH — do not over-trust it.
#   CAN:    a landed slice's ladder row that spells out one of the listed
#           markers, in any of the punctuation variants (`NOT_STARTED`,
#           `not-started`, `not started`, `IN FLIGHT`, `in-flight`), behind any
#           markdown emphasis, anywhere in the status cell.
#   CANNOT: any wording not on this list. "pending", "TBD", "queued", "awaiting
#           review", "blocked on X", "0/5 arms green", "—", an empty cell, or a
#           row that is simply months out of date while using upbeat words —
#           ALL PASS THIS CHECK. It is a prose regex; it detects the phrasings
#           someone thought of, and every board that goes stale in a new
#           phrasing goes stale silently as far as (b) is concerned.
#   FALSE-POSITIVES IT WILL PRODUCE (measured against a case matrix, not one
#           example): the state markers match ANYWHERE in the status cell, so a
#           landed slice whose close prose says "closed while in-flight work was
#           parked", or "R-20-PR/R-20-EAV/TC-33 not started; now shipped" (a
#           real phrasing on STATUS-0.8.20.md:316, in another table), IS
#           flagged. That is the intended strictness — a landed slice's own
#           status cell should not contain those words — but it costs a
#           land-blocking hard failure, so it is stated rather than discovered.
#   THEREFORE (b) IS THE FRAGILE HALF AND IS NOT THE LOAD-BEARING ONE. Check
#   (c) below — "the row must cite the slice's own recorded landing SHA" — is
#   derived from the state file, has NO vocabulary at all, and is what would
#   actually have caught the real Slice 39 incident. If you are tempted to
#   trust this gate, trust (c). Widening the vocabulary below is a deliberate
#   act with a false-positive cost: a false positive here HARD-BLOCKS A LAND.
#
# The `next` family is deliberately NOT a bare `\bnext\b`. In a status cell,
# "next" is overwhelmingly a POINTER word ("carried to the next slice",
# "closed; next is Slice 40") rather than a STATE word, and a bare match would
# false-positive on legitimate close prose and block a land. It counts only
# when it is the ENTIRE status cell. Measured cost of that narrowing: a board
# writing "next in queue" for a landed slice is not caught by (b) — it is
# caught by (c).
STATE_MARKERS = (
    ("not yet started", r"not[-_ ]+yet[-_ ]+started"),
    ("not started", r"not[-_ ]+started"),
    ("unstarted", r"\bunstarted\b"),
    ("in flight", r"in[-_ ]+flight"),
)
WHOLE_CELL_MARKERS = ("next", "next up", "up next")

# AND IT REFUSES A COLLISION RATHER THAN TAKING THE LAST WRITER. `30` and
# `30.0` are distinct JSON numbers but `slice_str` renders both as "30" (and in
# Python `30 == 30.0` and `hash(30) == hash(30.0)`, so this is not a
# hypothetical shape). A bare `by_slice[key] = entry` kept whichever entry came
# SECOND in the file — dict-insertion order, i.e. LINE ORDER — and the survivor
# then answered for the loser in check (c) below: a landed row could be
# reconciled against the WRONG entry's SHA, or, when the two agree, a malformed
# state file could be CERTIFIED. That is the same "SILENTLY OVERWRITE" collapse
# check-release-state-views.sh's `_by_slice` was written to forbid, and it
# already refuses it; two readers of one file must not disagree about what that
# file means, so this one refuses it too.
#
# It is a HARD fail, not a `bad = True`, because a duplicate is a defect in the
# STATE FILE, and Leg 3's policy for a present-but-malformed state file is a
# hard fail (only an ABSENT one is the announced skip above). Exiting also stops
# the checks below from running against a map that is known to be wrong. Every
# duplicate is reported first, so one run names them all.
#
# The entries are known to be OBJECTS by the type validation above — the old
# `isinstance(entry, dict)` guard here was silently dropping malformed ones
# instead of reporting them (fix-2 finding B). An entry with no `slice` key is
# still skipped, because it cannot be keyed at all; a landed slice whose entry
# went missing that way is caught by the state-completeness check below.
by_slice = {}
state_dup = []
for entry in st["ladder"]:
    if "slice" in entry:
        key = slice_str(entry["slice"])
        if key in by_slice:
            state_dup.append((key, by_slice[key]["slice"], entry["slice"]))
            continue
        by_slice[key] = entry

for key, first, second in state_dup:
    err("STALE  %s: %s carries TWO ladder entries that resolve to the same slice "
        "id %s (%r and %r). One would SILENTLY OVERWRITE the other — whichever "
        "came second in the file — so a landed row could be reconciled against "
        "the WRONG entry's SHA, and every reader of this file would disagree "
        "with the next about which entry is the slice. Give each slice exactly "
        "one ladder entry (TC-133)."
        % (ver, state_path, key, first, second))
if state_dup:
    sys.exit(1)

# --- (c-pre) STATE COMPLETENESS: a landed slice MUST carry its landing SHA ----
# fix-2 finding A. The original construct was
#     sha = (by_slice.get(key) or {}).get("sha")
#     if sha and sha not in status:
# and both halves fail OPEN. When a slice is listed in `landed` but its `ladder`
# entry is missing, or carries no `sha`, `sha` is None and check (c) — the
# LOAD-BEARING half of this cross-read, the one that would actually have caught
# the real Slice 39 incident — silently DOES NOT RUN for that slice. The gate
# then certifies the hand-written row on the strength of the marker regex (b)
# alone, which its own comment above calls the fragile half. That is precisely
# the state-file INCOMPLETENESS this step was added to catch, so it is reported
# rather than tolerated: `landed` and `ladder[].sha` are two halves of one fact
# and a release whose single writer has only one of them cannot be vouched for.
#
# MEASURED AGAINST THE REAL FILE BEFORE MAKING IT A HARD FAIL: all 14 ids in
# `landed` in dev/plans/release-state-0.8.20.json (0, 5, 10, 15, 20, 21, 22, 23,
# 25, 30, 31, 32, 33, 39) have a `ladder` entry AND a non-null `sha`, zero
# exceptions — so this cannot redden the live gate. Had a real landed slice
# legitimately lacked one, this would have gone back to the Steward instead.
#
# The `sha` must be a NON-EMPTY STRING, not merely truthy-or-present: `sha` is
# used as `sha not in status`, and a non-string there is either a TypeError
# (e.g. a number) or a silently wrong answer.
reconciled = 0
for n in st["landed"]:
    key = slice_str(n)
    entry = by_slice.get(key)
    if entry is None:
        why = "its `ladder` carries no ladder entry for that id"
    elif "sha" not in entry:
        why = "its `ladder` entry carries no `sha` field"
    elif not isinstance(entry["sha"], str) or not entry["sha"].strip():
        why = ("its `ladder` entry records sha=%r, which is not a non-empty string"
               % (entry["sha"],))
    else:
        why = None
    if why is None:
        sha = entry["sha"]
    else:
        sha = None
        err("STALE  %s Slice %s: %s records it in `landed`, but %s. The SHA-row "
            "check would then be a silent NO-OP for this slice and the board's "
            "hand-written row would be certified on the marker regex alone — a "
            "landed slice and its landing SHA are two halves of one fact, and "
            "the single writer must carry both (TC-133)"
            % (ver, key, state_path, why))
        bad = True

    row = rows.get(key)
    if row is None:
        err("STALE  %s Slice %s: %s records it in `landed` but %s's ladder table "
            "has NO row for it — a board cannot be current for a slice it does "
            "not list (TC-133)" % (ver, key, state_path, board_path))
        bad = True
        continue
    lineno, status = row
    reconciled += 1
    flat = norm(status)
    hit = None
    for label, pat in STATE_MARKERS:
        if re.search(pat, flat, re.I):
            hit = label
            break
    if hit is None and flat.lower() in WHOLE_CELL_MARKERS:
        hit = flat
    if hit is not None:
        err("STALE  %s Slice %s: %s records it LANDED in `landed`, but the "
            "hand-written ladder row at %s:%d still describes it as \"%s\". "
            "Presence of a SHA elsewhere in the file is NOT currency of THIS "
            "row (TC-133)" % (ver, key, state_path, board_path, lineno, hit))
        bad = True
    # `sha` was resolved (and its absence already reported) above — deliberately
    # NOT re-derived here as `(by_slice.get(key) or {}).get("sha")`, which was
    # the fix-2 finding-A construct: it made this check evaporate whenever the
    # state file was incomplete. It is None here only when that incompleteness
    # has ALREADY been reported as a STALE line, so the gate is red either way.
    if sha is not None and sha not in status:
        err("STALE  %s Slice %s: %s records landing SHA %s, but the hand-written "
            "ladder row at %s:%d does not cite it (TC-133). A SHA that reaches "
            "the board only through a `<!-- BEGIN GENERATED -->` cell is the "
            "generated half carrying the check for the hand-written half — the "
            "exact shape that kept this board wrong while the gate stayed green."
            % (ver, key, state_path, sha, board_path, lineno))
        bad = True

# --- (d) `Ladder remaining:` prose vs remaining_ladder ----------------------
# Both word orders are matched: the boards use "Ladder remaining: 40 alone" and
# "Remaining ladder: 40 alone" interchangeably for the same claim.
#
# The claim's span is read as an ID LIST — the maximal run of slice ids and
# separators immediately after the marker — and NOT by cutting the line at the
# first punctuation mark. The obvious cut-at-punctuation version was written
# first and was measured wrong on a matrix rather than on one example:
#   "Ladder remaining: 40 alone."                -> {40}      both agree
#   "Ladder remaining: 40 and 41 alone, then ..."-> {40, 41}   both agree
#   "Ladder remaining: 39.5, 40"                 -> cut-at-`,` yields {39.5}
# The third is a COMMA-SEPARATED LADDER, the most natural way to write a
# two-slice remainder, and cutting at the comma silently drops every id after
# the first — a false negative when the board UNDER-claims and a land-blocking
# false positive when it does not. (Cutting at `.` had the same shape against a
# fractional id: `40.5` would have been truncated to `40`, this unit's own
# defect class, which is why the id token below is `[0-9]+(\.[0-9]+)*` and not
# `[0-9]+`.) Reading a list stops naturally at the first non-id word —
# "alone", "then", "NONE" — so all three cases above come out right.
CLAIM = re.compile(r"(?:ladder\s+remaining|remaining\s+ladder)\s*[:—]", re.I)
SEP = re.compile(r"(?:\s|[*`,;&+]|→|->|and\b)+")
ID = re.compile(r"[0-9]+(?:\.[0-9]+)*")


def claim_ids(rest):
    """Read the leading slice-id list out of a `Ladder remaining:` clause.
    Returns (set-of-id-strings, the-span-consumed)."""
    ids = []
    pos = 0
    end = 0
    while True:
        sep = SEP.match(rest, pos)
        if sep:
            pos = sep.end()
        tok = ID.match(rest, pos)
        if not tok:
            break
        ids.append(tok.group(0))
        pos = end = tok.end()
    return set(ids), rest[:end]


expected = set(slice_str(n) for n in st["remaining_ladder"])
claims = 0
for idx, line in enumerate(board_lines, start=1):
    m = CLAIM.search(line)
    if not m:
        continue
    claims += 1
    rest = line[m.end():]
    found, span = claim_ids(rest)
    if found != expected:
        err("STALE  %s: the board claims \"%s\" at %s:%d, naming slice id(s) "
            "{%s}, but `remaining_ladder` in %s is {%s} (TC-133)"
            % (ver, (m.group(0) + span).strip(), board_path, idx,
               ", ".join(sorted(found)) or "-", state_path,
               ", ".join(sorted(expected)) or "-"))
        bad = True

if claims == 0:
    err("note   %s: %s carries no `Ladder remaining:` claim, so that half of the "
        "board/state cross-read (TC-133) vouched for nothing on this board"
        % (ver, board_path))

if bad:
    sys.exit(1)

err("ok    %s: board/state cross-read (TC-133): %d landed slice(s) reconciled "
    "against %s, %d `Ladder remaining` claim(s) agree with `remaining_ladder`"
    % (ver, reconciled, state_path, claims))
sys.exit(0)
CROSS_READ_PY
}

# Emit `slice<TAB>sha` for a canonical current state with authoritative landed
# facts. Empty output means the caller must retain the legacy merge-subject
# predicate (e.g. a pre-first-land foundation or an old fixture). Any malformed
# non-empty landed set is a hard error: machine facts must never be partial.
cbc_machine_lands() {
  local state="$1"
  python3 - "$state" <<'MACHINE_LANDS_PY'
import json
import sys

path = sys.argv[1]
try:
    with open(path, encoding="utf-8") as fh:
        state = json.load(fh)
    landed = state.get("landed")
    if not isinstance(landed, list):
        raise ValueError("`landed` is not a JSON array")
    if not landed:
        raise SystemExit(0)
    ladder = state.get("ladder")
    if not isinstance(ladder, list):
        raise ValueError("`ladder` is not a JSON array")
    by_slice = {}
    for entry in ladder:
        if not isinstance(entry, dict) or "slice" not in entry:
            raise ValueError("`ladder` contains a non-object or entry without `slice`")
        key = str(entry["slice"])
        if key in by_slice:
            raise ValueError("`ladder` contains duplicate slice %s" % key)
        by_slice[key] = entry
    for slice_id in landed:
        key = str(slice_id)
        entry = by_slice.get(key)
        if entry is None:
            raise ValueError("landed slice %s has no ladder entry" % key)
        sha = entry.get("sha")
        if not isinstance(sha, str) or not sha:
            raise ValueError("landed slice %s has no non-empty ladder sha" % key)
        print("%s\t%s" % (key, sha))
except SystemExit:
    raise
except Exception as exc:
    print("STALE  machine release-state facts in %s are invalid: %s" % (path, exc), file=sys.stderr)
    raise SystemExit(1)
MACHINE_LANDS_PY
}

# Return the earliest next slice only for the exact state-backed shape in which
# a release has just been activated and cannot yet have a landing merge. This is
# deliberately stricter than TC-133's general structural validation: anything
# malformed, partially landed, or progressed past the first slice must retain
# the normal zero-merge hard failure.
cbc_initial_release_next() {
  local ver="$1" state="$2"
  python3 - "$ver" "$state" <<'INITIAL_RELEASE_PY'
import json
import math
import re
import sys

ver, path = sys.argv[1:]


def slice_key(value):
    if isinstance(value, bool) or not isinstance(value, (int, float, str)):
        raise ValueError("slice id has an unsupported type")
    if isinstance(value, float):
        if not math.isfinite(value):
            raise ValueError("slice id is not finite")
        text = "%g" % value
    else:
        text = str(value)
    if not re.fullmatch(r"[0-9]+(?:\.[0-9]+)*", text):
        raise ValueError("slice id is not a dotted numeric id")
    return text


try:
    with open(path, encoding="utf-8") as fh:
        state = json.load(fh)
    if not isinstance(state, dict):
        raise ValueError("state is not an object")
    if state.get("release") != ver:
        raise ValueError("release does not match state filename")
    if state.get("board") != "dev/plans/runs/STATUS-%s.md" % ver:
        raise ValueError("board does not match state filename")
    if state.get("landed") != []:
        raise ValueError("release has landed slices")

    ladder = state.get("ladder")
    remaining = state.get("remaining_ladder")
    if not isinstance(ladder, list) or not ladder or not isinstance(remaining, list):
        raise ValueError("ladder or remaining_ladder is not a non-empty array")
    ladder_slices = []
    for index, entry in enumerate(ladder):
        if not isinstance(entry, dict) or "slice" not in entry:
            raise ValueError("ladder entry has no slice")
        allowed_statuses = (
            ("NOT_STARTED", "IN_PROGRESS") if index == 0 else ("NOT_STARTED",)
        )
        if entry.get("status") not in allowed_statuses:
            raise ValueError("ladder records a non-initial status")
        if entry.get("sha") not in (None, ""):
            raise ValueError("ladder records a landing SHA")
        ladder_slices.append(slice_key(entry["slice"]))
    if len(set(ladder_slices)) != len(ladder_slices):
        raise ValueError("ladder has duplicate slices")
    if [slice_key(value) for value in remaining] != ladder_slices:
        raise ValueError("remaining_ladder is not the complete ladder")
    if slice_key(state.get("next_slice")) != ladder_slices[0]:
        raise ValueError("next_slice is not the first ladder slice")
except Exception:
    raise SystemExit(1)

print(ladder_slices[0])
INITIAL_RELEASE_PY
}

STALE=0

# Inputs are tracked paths, never a physical directory walk: stale nested
# worktrees and scratch boards cannot affect this checkout's currency verdict.
mapfile -d '' -t TRACKED_BOARDS < <(git ls-files -z -- "$BOARDS_DIR")
FILTERED_BOARDS=()
for board in "${TRACKED_BOARDS[@]}"; do
  case "$(basename "$board")" in STATUS-0.8.*.md) FILTERED_BOARDS+=("$board") ;; esac
done

# For the canonical board directory, use the same authority as steward-orient.
# Legacy fixtures and pre-writer historical releases intentionally fall back to
# the banner predicate when no conforming resolver tuple exists.
CURRENT_BOARD=""
CURRENT_STATE=""
if [ "$BOARDS_DIR" = "dev/plans/runs" ] && CURRENT="$("$_CBC_SCRIPT_DIR/release-current.py" 2>/dev/null)"; then
  IFS=$'\t' read -r _CURRENT_VER CURRENT_BOARD CURRENT_STATE <<<"$CURRENT"
  # An empty, successful resolver result is the valid terminal state: every
  # tracked release is published, so no retained historical board is live.
  FILTERED_BOARDS=()
  if [ -n "$CURRENT_BOARD" ]; then
    FILTERED_BOARDS=("$CURRENT_BOARD")
  fi
fi

for board in "${FILTERED_BOARDS[@]}"; do
  # Closed boards are self-labelled and frozen -- never scanned (see predicate
  # above). The 15-line header window (not 5) and its rationale live in
  # scripts/lib/board-closed.sh, sourced above and shared with steward-orient.sh.
  if board_is_closed "$board"; then
    continue
  fi

  ver="$(basename "$board" .md | sed -n 's/^STATUS-\(0\.8\.[0-9.]*\)$/\1/p')"
  if [ -z "$ver" ]; then
    continue # non-standard board name (e.g. STATUS-phase12.md) -- not this gate's shape
  fi
  ver_escaped="$(printf '%s' "$ver" | sed 's/\./\\./g')"

  MACHINE_MODE=0
  if [ "$board" = "$CURRENT_BOARD" ] && [ -n "$CURRENT_STATE" ]; then
    if MACHINE_LANDS="$(cbc_machine_lands "$CURRENT_STATE")"; then
      if [ -n "$MACHINE_LANDS" ]; then
        MACHINE_MODE=1
        while IFS=$'\t' read -r slice_n state_sha; do
          if ! git rev-parse --verify -q "${state_sha}^{commit}" >/dev/null; then
            printf 'STALE  %s Slice %s: release-state SHA %s does not resolve to a commit\n' \
              "$ver" "$slice_n" "$state_sha" >&2
            STALE=1
          elif ! git merge-base --is-ancestor "$state_sha" "$TIP"; then
            printf 'STALE  %s Slice %s: release-state SHA %s is not reachable from %s\n' \
              "$ver" "$slice_n" "$state_sha" "$TIP" >&2
            STALE=1
          fi
        done <<<"$MACHINE_LANDS"
      fi
    else
      STALE=1
    fi
  fi

  declare -A SEEN_SLICE=()
  if [ "$MACHINE_MODE" -eq 0 ]; then
  while IFS= read -r line; do
    [ -n "$line" ] || continue
    sha="${line%% *}"
    subject="${line#* }"
    # Slice id capture: `([0-9]+(\.[0-9]+)*)` followed by a non-digit-or-EOL, NOT
    # a bare `([0-9]+)`. SLICE-ID-HARDENING site 1: the integer-only capture read
    # `merge(0.8.20): Slice 39.5 ...` as slice **39** (confirmed by executing the
    # real bash [[ =~ ]]). Because this walk is newest-first, the NEWER fractional
    # merge set SEEN_SLICE[39] first and the genuinely-distinct `Slice 39` merge
    # was then discarded by the superseded-intermediate branch below, so ITS SHA
    # CHECK NEVER RAN -- a silent wrong answer, exit 0. It also made the STALE
    # diagnostic print `Slice 39` while quoting 39.5's SHA: a FABRICATED POINTER,
    # the gate misdirecting the reader to a unit that is not the one at fault.
    #
    # The trailing `([^0-9]|$)` (rather than `([^0-9.]|$)`) is deliberate: it
    # guarantees every subject matching the merge convention still yields SOME
    # id, so no landing merge can become INVISIBLE to the walk. Invisibility
    # would be worse than truncation here, because the vacuous-pass guard below
    # only fires when a board matches ZERO lands -- one silently-dropped merge
    # among several would restore exactly the silent hole this closes. The
    # greedy `(\.[0-9]+)*` means a dotted id is always captured WHOLE (measured:
    # 39 -> `39`, 39.5 -> `39.5`, 39.5.1 -> `39.5.1`, 395 -> `395`), so the two
    # never collide on one key.
    if [[ "$subject" =~ ^merge\(${ver_escaped}\):[[:space:]]Slice[-[:space:]]([0-9]+(\.[0-9]+)*)([^0-9]|$) ]]; then
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

  if [ "${#SEEN_SLICE[@]}" -eq 0 ]; then
    if INITIAL_NEXT="$(cbc_initial_release_next "$ver" "dev/plans/release-state-${ver}.json")"; then
      printf 'note   %s: no landing merge matched the convention, but release state records a newly activated initial release (landed: [], next_slice %s is the earliest ladder slice); the TC-133 cross-read will verify its STATUS ladder.\n' \
        "$ver" "$INITIAL_NEXT" >&2
    else
      printf 'STALE  %s: no landing merge matched the convention '\''merge(%s): Slice[- ]N'\'' reachable from %s — either nothing has landed for this live board, or the merge-subject convention drifted; the board-currency gate cannot vouch for it.\n' \
        "$ver" "$ver" "$TIP" >&2
      STALE=1
    fi
  fi
  unset SEEN_SLICE
  fi

  # Step 6 — TC-133 board/state cross-read. Runs UNCONDITIONALLY for every live
  # board, including one that already failed steps 3-5: the two questions are
  # independent (git ancestry vs the single-writer state file) and reporting
  # only the first would hide the second, which is how this defect survived.
  if ! cbc_cross_read "$ver" "$board" "dev/plans/release-state-${ver}.json" "$MACHINE_MODE"; then
    STALE=1
  fi
done

if [ "$STALE" -eq 0 ]; then
  printf 'ok    board-currency: no live STATUS-0.8.z.md board contradicts git ancestry\n' >&2
fi

exit "$STALE"
