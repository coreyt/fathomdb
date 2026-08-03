#!/usr/bin/env bash
# scripts/steward-orient.sh — the stateless cold-start briefing (DOC-HYGIENE-2 T3a).
#
# WHY THIS EXISTS. Every Steward cold start re-paid for the same orientation:
# which branch/HEAD, what is landed, what SCHEMA, what is next, what the ledgers
# say, what is in flight. That state was narrated across a 5-12 file fan-out and
# the live board alone is ~79 KB, so orientation cost either a large context bill
# or a guess. This prints the whole picture in ONE read, from the writers of
# record, in under 4 KB.
#
# ============================ THE TWO HARD LIMITS ============================
# 1. <= 4096 bytes on stdout. Self-checked: over budget is a HARD failure, not a
#    silent overrun, because the cap is a promise to the reader's context budget.
#    Override for experiments only via $STEWARD_ORIENT_BUDGET.
#    Collections are cardinality-bounded: worktrees retain actionable identity
#    for the current checkout and one locked and detached exception (each
#    path/branch/SHA), then state exact omitted-row counts plus `git worktree
#    list` for full follow-up; ledger preview is five headline-first entries; and
#    todo IDs are losslessly run-folded. THE STANDING EXPOSURE is the board's
#    next-action cell: it is verbatim by contract and its length is set by its
#    writer. Keep §1's next action short; do not raise the cap or truncate the
#    cell — the suite asserts both the cap and verbatim reproduction independently.
# 2. WRITES NOTHING. No repo file, no ledger, no cursor, no commit. The only
#    filesystem touch is one mktemp -d sandbox, removed by an EXIT trap, used as
#    an ephemeral ledgerwatch --state-dir so the Steward's REAL cursor is never
#    read or advanced. Verified mechanically by scripts/tests/test_steward_orient.sh
#    (fixture tree byte-identical before/after; the run's whole TMPDIR empty after).
#    ".git/index" IS A FILE THIS SCRIPT WRITES TOO, and it is the one the working
#    tree's own contract lives in. `git status` REFRESHES a stale stat cache and
#    then rewrites the index under .git/index.lock — measured on git 2.43:
#    touching tracked files and running `git status --porcelain` changes the
#    index's bytes. That is a repo-metadata write in a script whose whole promise
#    is that it makes none, it can race a concurrent git command for the index
#    lock, and it would fail in a read-only checkout. So EVERY git probe here runs
#    with git's OPTIONAL locks disabled (GIT_OPTIONAL_LOCKS=0 plus an explicit
#    --no-optional-locks at each call site): git then does all the same reads and
#    simply declines to persist the refreshed cache. The env var covers git run by
#    CHILD processes (gh, python3); the per-call flag keeps the intent legible and
#    survives someone unsetting the environment. Asserted by the suite's
#    ".git/index is byte-unchanged across a run with a deliberately staled stat
#    cache" arm, plus a structural arm that no bare `git` call creeps back in.
#
# ========================== WHERE THE FACTS COME FROM ========================
# LIVE repo facts are read live (branch, HEAD, dirty count, bounded actionable
# worktrees plus explicit full-list follow-up, orphan checkout dirs, open-PR
# count). RELEASE facts are NOT re-scraped from git: they
# are read from `dev/plans/release-state-<version>.json`, the single writer that
# DOC-HYGIENE-2 T2a landed for landed-slices+SHAs, SCHEMA, and the next slice
# (kept honest against its rendered views by scripts/check-release-state-views.sh).
# Re-scraping them here would recreate the exact fan-out this effort removes.
#
# ======================= FOUR THINGS THAT ARE LOAD-BEARING ==================
# (1) THE RELEASE COMES FROM scripts/release-current.py. It validates the tracked
#     release-state/board pairs and returns exactly one non-CLOSED board. A
#     hardcoded version, a highest-version tie-break, or an untracked nested
#     worktree copy would all recreate the ambiguity this briefing exists to end.
# (2) `<repo-root>-worktrees/` IS A SIBLING OF THE REPO ROOT, NOT A CHILD. It is
#     resolved from the MAIN worktree's root (via --git-common-dir, so running
#     this from inside a linked worktree still resolves the same sibling) plus a
#     `-worktrees` suffix. Resolving it as a child yields a silently empty
#     section, which is precisely the failure mode this briefing exists to end.
# (3) ANY ZERO-RESULT SECTION IS A HARD FAILURE. A briefing that silently omits a
#     section is worse than no briefing: the reader cannot tell "nothing to
#     report" from "I could not read it". The payload still prints (it is useful
#     even when partial), then the run names every empty section on stderr and
#     exits 1.
#     DOCUMENTED EXEMPTIONS — sections with a legitimate empty state, each an
#     explicit decision on the record rather than an accident:
#       * open-PR count: zero open PRs is a real, healthy state. `gh` being
#         absent or unauthenticated prints an explicit `unavailable (<reason>)`
#         marker instead — a cold start must not fail because the operator is
#         offline, but it must also never quietly show a wrong number.
#       * orphan checkout dirs: zero orphans is the HEALTHY state. Prints
#         `(none)` alongside the resolved path, so a mis-resolved directory is
#         still visible to the reader.
#       * dirty-file count: zero is the healthy state on a clean tree.
#       * the todos fold's `unfoldable (no id)` bucket, printed `no-id`: zero is
#         fine (and the
#         count is always printed, so it can never be confused with "no data").
# (4) The resolver uses the same header CLOSED marker as board-currency. Its
#     result, rather than a local reimplementation, is the release authority for
#     this briefing.
#
# ========================== LEDGERS GO THROUGH ledgerwatch ==================
# Never a raw `tail` of the .jsonl. Invocations:
#   steward ledger: `--strategy tail --dry-run --no-status --state-dir <sandbox>`
#   todos ledger:   `--project --json`
# CURSOR SEMANTICS = PEEK, DELIBERATELY. A cold-start briefing must not disturb
# the Steward's real cursor: consuming a delta here would silently drop it from
# the next real `ledgerwatch` poll — a monitor that loses an event is the one
# failure mode that tool is built to prevent. Two independent guarantees:
# `--dry-run` computes the delta without advancing the cursor, AND `--state-dir`
# points at a throwaway sandbox, so the real cursor is not even opened. (The
# sandbox also means the tail run is always a `cold` baseline, which is what we
# want: we need the last 5 entries, not "what changed since some other reader
# last looked".) `--project` is read-only by construction — it never creates,
# reads or advances a cursor at all.
#
# Exit codes: 0 = complete briefing within budget; 1 = at least one empty
# section, and/or over budget (payload still printed); 2 = environment error.
#
# Usage: scripts/steward-orient.sh
set -euo pipefail

# Hard limit 2, mechanically. Belongs BEFORE the first git call: with optional
# locks disabled git never takes .git/index.lock, so a stale stat cache is
# refreshed in memory and never written back, and a lock held by a concurrent
# git command (or a read-only .git) cannot perturb this run. Exported so git
# invoked by CHILD processes (gh, python3, ledgerwatch) inherits it.
export GIT_OPTIONAL_LOCKS=0

BUDGET_BYTES="${STEWARD_ORIENT_BUDGET:-4096}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if ! REPO_ROOT="$(git --no-optional-locks rev-parse --show-toplevel 2>/dev/null)"; then
  printf 'steward-orient: not inside a git worktree\n' >&2
  exit 2
fi
cd "$REPO_ROOT"

# The one filesystem touch, removed on EXIT. Guarded like the repo's test
# harnesses so a surprising value can never turn cleanup into a destructive rm.
SANDBOX="$(mktemp -d)"
cleanup() {
  case "$SANDBOX" in
    "${TMPDIR:-/tmp}"/*|/tmp/*) rm -rf "$SANDBOX" ;;
    *) printf 'steward-orient: refusing to remove unexpected temp path: %s\n' "$SANDBOX" >&2 ;;
  esac
}
trap cleanup EXIT

LEDGERWATCH="$REPO_ROOT/dev/agent-tools/ledgerwatch/ledgerwatch.py"
STEWARD_LEDGER="dev/steward/steward-ledger.jsonl"
TODOS_LEDGER="dev/todos-and-considerations-ledger.jsonl"

OUT=""
EMPTY=()
add() { OUT+="$1"$'\n'; }
note_empty() { EMPTY+=("$1"); }
# Renders a helper's stdout, routing its `#EMPTY:<section>` markers to the
# zero-result guard instead of the payload. Fed by here-string, NOT a pipe, so
# note_empty mutates this shell's array rather than a subshell's copy.
emit() {
  local line
  while IFS= read -r line; do
    case "$line" in
      '#EMPTY:'*) note_empty "${line#\#EMPTY:}" ;;
      *)          add "$line" ;;
    esac
  done <<<"$1"
}
tilde() { printf '%s' "${1/#$HOME/\~}"; }

# ------------------------------------------------------------------- REPO ---
BRANCH="$(git --no-optional-locks rev-parse --abbrev-ref HEAD 2>/dev/null || printf '?')"
HEAD_SHA="$(git --no-optional-locks rev-parse --short=8 HEAD 2>/dev/null || printf '?')"
DIRTY="$(git --no-optional-locks status --porcelain --untracked-files=all | wc -l | tr -d ' ')"

add "ORIENT $(date -u +%Y-%m-%dT%H:%MZ) RO"
add "REPO branch=$BRANCH head=$HEAD_SHA dirty=$DIRTY"

# -------------------------------------------------------------- WORKTREES ---
# The bounded briefing cannot emit every clean attached checkout: that output
# grows without limit and evicts the board's verbatim next action. It must not
# replace the list with an opaque count, though. Preserve the identities a cold
# start can act on immediately — current, one locked, one detached exception —
# and state exactly how many rows are omitted with the one on-demand source.
#
# Porcelain makes paths with spaces unambiguous. Each record is parsed once; the
# presentation below is deliberately bounded to three identities regardless of
# how many ordinary or same-category exception worktrees exist.
WT_PATHS=()
WT_HEADS=()
WT_BRANCHES=()
WT_LOCKS=()
wt_path=""
wt_head=""
wt_branch=""
wt_locked=0
wt_flush() {
  [ -n "$wt_path" ] || return 0
  WT_PATHS+=("$wt_path")
  WT_HEADS+=("$wt_head")
  WT_BRANCHES+=("${wt_branch:-DETACHED}")
  WT_LOCKS+=("$wt_locked")
  wt_path=""
  wt_head=""
  wt_branch=""
  wt_locked=0
}
while IFS= read -r wt_line || [ -n "$wt_line" ]; do
  case "$wt_line" in
    'worktree '*) wt_path="${wt_line#worktree }" ;;
    'HEAD '*)     wt_head="${wt_line#HEAD }" ;;
    'branch refs/heads/'*) wt_branch="${wt_line#branch refs/heads/}" ;;
    locked*)      wt_locked=1 ;;
    '')           wt_flush ;;
  esac
done < <(git --no-optional-locks worktree list --porcelain 2>/dev/null || true)
wt_flush

if [ "${#WT_PATHS[@]}" -eq 0 ]; then
  note_empty "worktrees (git worktree list returned nothing)"
fi
WT_DETACHED=0
WT_LOCKED=0
WT_CURRENT=-1
for i in "${!WT_PATHS[@]}"; do
  [ "${WT_BRANCHES[$i]}" = "DETACHED" ] && WT_DETACHED=$((WT_DETACHED + 1))
  [ "${WT_LOCKS[$i]}" -eq 1 ] && WT_LOCKED=$((WT_LOCKED + 1))
  [ "${WT_PATHS[$i]}" = "$REPO_ROOT" ] && WT_CURRENT="$i"
done
if [ "$WT_CURRENT" -lt 0 ]; then
  note_empty "current worktree ($REPO_ROOT was absent from git worktree list)"
fi

WT_LABELS=()
WT_SHOWN=0
wt_select() {
  local idx="$1" label="$2"
  [ -z "${WT_LABELS[$idx]:-}" ] || return 0
  WT_LABELS[$idx]="$label"
  WT_SHOWN=$((WT_SHOWN + 1))
}
if [ "$WT_CURRENT" -ge 0 ]; then wt_select "$WT_CURRENT" current; fi
# One representative per exception category: exact category totals and the
# omitted-row count below make any unrendered peers explicit rather than silent.
for i in "${!WT_PATHS[@]}"; do
  [ "${WT_LOCKS[$i]}" -eq 1 ] || continue
  wt_select "$i" locked
  break
done
for i in "${!WT_PATHS[@]}"; do
  [ "${WT_BRANCHES[$i]}" = "DETACHED" ] || continue
  wt_select "$i" detached
  break
done
WT_OMITTED=$((${#WT_PATHS[@]} - WT_SHOWN))
add "WORKTREES ${#WT_PATHS[@]} registered; locked=$WT_LOCKED detached=$WT_DETACHED shown=$WT_SHOWN omitted=$WT_OMITTED; details: git worktree list"
for i in "${!WT_PATHS[@]}"; do
  [ -n "${WT_LABELS[$i]:-}" ] || continue
  add "  WT ${WT_LABELS[$i]} path=$(tilde "${WT_PATHS[$i]}") branch=${WT_BRANCHES[$i]} sha=${WT_HEADS[$i]:0:8}"
done

# Correction 2: the checkout pool is a SIBLING of the MAIN worktree root.
# --git-common-dir points at the main .git even from inside a linked worktree,
# so this resolves the same pool no matter where the briefing is run from.
GIT_COMMON="$(cd "$(git --no-optional-locks rev-parse --git-common-dir)" && pwd)"
MAIN_ROOT="$(dirname "$GIT_COMMON")"
WT_DIR="${MAIN_ROOT}-worktrees"
REGISTERED="$(git --no-optional-locks worktree list --porcelain 2>/dev/null | sed -n 's/^worktree //p' || true)"
ORPHANS=""
if [ -d "$WT_DIR" ]; then
  for d in "$WT_DIR"/*/; do
    [ -d "$d" ] || continue
    p="${d%/}"
    # Here-string, not a pipe: under pipefail an early `grep -q` exit would
    # SIGPIPE the producer and misreport a REGISTERED worktree as an orphan.
    if ! grep -qxF -- "$p" <<<"$REGISTERED"; then
      ORPHANS+=" $(basename "$p")"
    fi
  done
  # Exempt from the zero-result guard: no orphans is the HEALTHY state.
  add "ORPHANS $(tilde "$WT_DIR"):${ORPHANS:- (none)}"
else
  add "ORPHAN dirs: $(tilde "$WT_DIR") does not exist"
fi

# ---------------------------------------------------------------- RELEASE ---
# The resolver is the only current-release selector. Its input is `git ls-files`,
# so a stale nested worktree cannot become a second authority.
RESOLVER="$REPO_ROOT/scripts/release-current.py"
BOARD=""
VER=""
STATE=""
if [ ! -x "$RESOLVER" ]; then
  note_empty "release resolver ($RESOLVER is missing or not executable)"
  add "RELEASE (resolver unavailable)"
elif ! CURRENT="$($RESOLVER)"; then
  note_empty "current release (release-current resolver rejected tracked release metadata)"
  add "RELEASE (no authoritative current release)"
else
  IFS=$'\t' read -r VER BOARD STATE <<<"$CURRENT"
  if [ -z "$VER" ] || [ -z "$BOARD" ] || [ -z "$STATE" ]; then
    note_empty "current release (release-current resolver returned an incomplete tuple)"
    add "RELEASE (resolver returned incomplete metadata)"
  else
    if STATE_OUT="$(python3 - "$STATE" "$VER" "$BOARD" <<'PY'
import json, sys

path, ver, board = sys.argv[1], sys.argv[2], sys.argv[3]
try:
    d = json.load(open(path, encoding="utf-8"))
except Exception as exc:  # a corrupt single writer must be loud, never empty
    print(f"RELEASE {ver}  state file UNREADABLE: {exc}")
    print(f"#EMPTY:release state ({path} is not parseable)")
    raise SystemExit(0)

ladder = d.get("ladder") or []
landed = [e for e in ladder if e.get("status") == "LANDED"]
schema = d.get("schema_version", "?")
nxt = d.get("next_slice")
rem = d.get("remaining_ladder") or []

# The state file's name is `release-state-<ver>.json` by construction, so printing
# it spends 32 bytes restating `ver`. The board's name is NOT derivable (it is the
# thing the release was derived FROM), so it stays.
print(f"RELEASE {ver}  SCHEMA {schema}  board={board.split('/')[-1]}")

if landed:
    cells = " ".join(f"{e.get('slice')}({e.get('sha') or '?'})" for e in landed)
    print(f"  LANDED {cells}")
else:
    print("  LANDED (none)")
    # A foundation release has a legitimate pre-first-land state. Its explicit
    # release kind distinguishes that from an accidentally-empty normal ladder.
    if "foundation" not in str(d.get("release_kind", "")).casefold():
        print(f"#EMPTY:landed slices (no ladder entry in {path} has status LANDED)")

# The state file also carries a flat `landed` list; if its own two fields
# disagree, the single writer is internally inconsistent and must say so.
flat = d.get("landed")
if flat is not None and sorted(flat) != sorted(e.get("slice") for e in landed):
    print(f"  !! state file self-inconsistent: landed={flat} vs ladder LANDED")

nxt_entry = next((e for e in ladder if e.get("slice") == nxt), None)
title = ""
if nxt_entry:
    title = f" {nxt_entry.get('short') or ''} — {nxt_entry.get('title') or ''}"[:88]
print(f"  NEXT   slice {nxt}{title}")
print(f"  REMAIN {', '.join(str(s) for s in rem) or '(none)'}")
PY
    )"; then
      emit "$STATE_OUT"
    else
      note_empty "release state ($STATE could not be rendered)"
      add "RELEASE $VER  state file could not be rendered"
    fi
  fi
fi

# ------------------------------------------------------------ NEXT ACTION ---
# Verbatim, markup and all: the board is the writer of record for this sentence,
# and a paraphrase here would be exactly the second narration T2a removed.
if [ -n "$BOARD" ]; then
  ROW="$(grep -m1 -E '^\|[[:space:]]*\*\*Immediate next (action|step)\*\*[[:space:]]*\|' "$BOARD" || true)"
  if [ -z "$ROW" ]; then
    # Foundation boards are intentionally concise: their state writer owns the
    # current slice, so use that fact rather than inventing a prose row. Older
    # boards retain the strict verbatim-row contract below.
    FOUNDATION_NEXT="$(python3 - "$STATE" <<'PY'
import json, sys
try:
    d = json.load(open(sys.argv[1], encoding="utf-8"))
    if "foundation" in str(d.get("release_kind", "")).casefold() and d.get("next_slice") is not None:
        print(d["next_slice"])
except Exception:
    pass
PY
)"
    if [ -n "$FOUNDATION_NEXT" ]; then
      add "NEXT ACTION (from release state)"
      add "  Slice $FOUNDATION_NEXT is current."
    else
      note_empty "board next action (no '| **Immediate next action** |' row in $BOARD)"
      add "NEXT ACTION (row not found in $(basename "$BOARD"))"
    fi
  else
    CELL="${ROW#*|}"; CELL="${CELL#*|}"; CELL="${CELL%|}"
    CELL="${CELL#"${CELL%%[![:space:]]*}"}"; CELL="${CELL%"${CELL##*[![:space:]]}"}"
    if [ -z "$CELL" ]; then
      note_empty "board next action (the row exists but its cell is empty)"
      add "NEXT ACTION (empty cell)"
    else
      add "NEXT ACTION (verbatim)"
      add "  $CELL"
    fi
  fi
fi

# ----------------------------------------------------------------- LEDGER ---
# Peek, never consume — see the cursor-semantics note in the header.
LW_STATE="$SANDBOX/ledgerwatch-state"
mkdir -p "$LW_STATE"
if [ ! -f "$STEWARD_LEDGER" ]; then
  note_empty "steward ledger ($STEWARD_LEDGER does not exist)"
  add "LEDGER (missing: $STEWARD_LEDGER)"
elif LEDGER_RAW="$(python3 "$LEDGERWATCH" "$STEWARD_LEDGER" \
      --strategy tail --dry-run --no-status --state-dir "$LW_STATE" 2>/dev/null)"; then
  # The reader takes its program from `-c` and its DATA from stdin. A heredoc
  # cannot carry the program here: it would occupy stdin, the reader would see
  # no data, and the section would report "no entries" for a full ledger.
  LEDGER_PY="$(cat <<'PY'
import json, sys

rows = [l for l in sys.stdin.read().split("\n") if l.strip()]
if not rows:
    print("LEDGER (no entries)")
    print(f"#EMPTY:steward ledger ({sys.argv[1]} has no entries)")
    raise SystemExit(0)

last = rows[-5:]
print(f"LEDGER last {len(last)} of {len(rows)}")
for line in last:
    try:
        e = json.loads(line)
    except Exception:
        print("  ?? unparseable entry")
        continue
    # 48, not 118. This preview was ALWAYS a truncation (the entry itself is one
    # line of dev/steward/steward-ledger.jsonl, which is where the reader goes for
    # the full text); the only question is where it cuts. Steward summaries are
    # written headline-first, so 48 preserves the decision's lead while five
    # actionable worktree identities and the verbatim next action stay in budget.
    summary = " ".join(str(e.get("summary", "")).split())[:48]
    print(f"  {e.get('seq', '?')} {e.get('kind', '?')}: {summary}")
PY
  )"
  LEDGER_OUT="$(python3 -c "$LEDGER_PY" "$STEWARD_LEDGER" <<<"$LEDGER_RAW")"
  emit "$LEDGER_OUT"
else
  note_empty "steward ledger (ledgerwatch could not read $STEWARD_LEDGER)"
  add "LEDGER (unreadable)"
fi

# ------------------------------------------------------------------ TODOS ---
# The fold is ledgerwatch's own --project (latest entry per id, with an explicit
# `unfoldable (no id)` bucket). Not hand-rolled: a second fold implementation is
# a second place for the ledger's semantics to be wrong.
if [ ! -f "$TODOS_LEDGER" ]; then
  note_empty "todos ledger ($TODOS_LEDGER does not exist)"
  add "TODOS (missing: $TODOS_LEDGER)"
elif TODOS_RAW="$(python3 "$LEDGERWATCH" "$TODOS_LEDGER" --project --json 2>/dev/null)"; then
  TODOS_PY="$(cat <<'PY'
import json, re, sys

try:
    env = json.loads(sys.stdin.read())
except Exception:
    print("TODOS (projection unreadable)")
    print("#EMPTY:todos ledger (ledgerwatch --project produced no envelope)")
    raise SystemExit(0)

rows = env.get("latest") or []
print(
    f"TODOS  {env.get('folded_ids', 0)} ids folded from {env.get('entries', 0)} entries · "
    f"no-id {env.get('unfoldable_no_id', 0)} · malformed {env.get('malformed', 0)}"
)
if not rows:
    print("#EMPTY:todos ledger (folded to zero ids)")
    raise SystemExit(0)


def split_id(i):
    m = re.search(r"(\d+)$", i)
    return (i, None) if not m else (i[: m.start()], int(m.group(1)))


def natural(i):
    prefix, num = split_id(i)
    return (prefix, -1 if num is None else num)


# LOSSLESS COMPRESSION, not a truncation. Spelled out in full, one bucket of 91
# ids costs 577 bytes, 273 of them the string "TC-" repeated. So: emit the shared
# stem once per run and collapse consecutive numbers into `a..b`. Every id is
# still exactly reconstructable and the counts are unchanged; the section costs
# about a quarter of what it did. An id with no trailing number, or with a
# non-numeric tail (TC-99-VEC0), is passed through verbatim rather than guessed
# at — it simply forms its own single-member run.
def compress(ids):
    groups = []
    for i in sorted(ids, key=natural):
        prefix, num = split_id(i)
        if num is None:
            groups.append([None, [i]])
        elif groups and groups[-1][0] == prefix:
            groups[-1][1].append(num)
        else:
            groups.append([prefix, [num]])

    out = []
    for prefix, items in groups:
        if prefix is None:
            out.append(items[0])
            continue
        runs, start, end = [], items[0], items[0]
        for num in items[1:]:
            if num == end + 1:
                end = num
            else:
                runs.append((start, end))
                start = end = num
        runs.append((start, end))
        # A two-member run is written `a,b`: `a..b` would be a byte longer.
        out.append(prefix + ",".join(
            str(a) if a == b else (f"{a},{b}" if b == a + 1 else f"{a}..{b}")
            for a, b in runs
        ))
    return " ".join(out)


buckets = {}
for r in rows:
    buckets.setdefault(str(r.get("status", "?")), []).append(str(r.get("id")))
for status in sorted(buckets, key=lambda s: (-len(buckets[s]), s)):
    print(f"  {status}({len(buckets[status])}) {compress(buckets[status])}")
PY
  )"
  TODOS_OUT="$(python3 -c "$TODOS_PY" <<<"$TODOS_RAW")"
  emit "$TODOS_OUT"
else
  note_empty "todos ledger (ledgerwatch --project failed on $TODOS_LEDGER)"
  add "TODOS (unreadable)"
fi

# -------------------------------------------------------------------- PRS ---
# Exempt from the zero-result guard: zero open PRs is a real state, and an
# offline/unauthenticated host must degrade to an explicit marker, never to a
# wrong number.
if ! command -v gh >/dev/null 2>&1; then
  PRS="unavailable (gh not on PATH)"
elif PR_N="$(timeout 20 gh pr list --state open --limit 300 --json number --jq 'length' 2>/dev/null)" \
     && [ -n "$PR_N" ]; then
  PRS="$PR_N open"
else
  PRS="unavailable (gh unauthenticated, offline, or no remote)"
fi

# ---------------------------------------------------------------- HANDOFF ---
# Newest by the dated filename, which is the naming convention of record
# (STEWARD-SESSION-HANDOFF-YYYY-MM-DD-<A|B|...>.md), so lexicographic == newest.
HANDOFF="$(ls -1 dev/plans/runs/STEWARD-SESSION-HANDOFF-*.md 2>/dev/null | sort | tail -1 || true)"
if [ -z "$HANDOFF" ]; then
  note_empty "steward hand-off (no dev/plans/runs/STEWARD-SESSION-HANDOFF-*.md found)"
  add "PRs $PRS · HANDOFF (none found)"
else
  add "PRs $PRS · HANDOFF $HANDOFF"
fi

# ------------------------------------------------------------------ EMIT ----
printf '%s' "$OUT"

RC=0
BYTES="$(printf '%s' "$OUT" | wc -c | tr -d ' ')"

if [ "${#EMPTY[@]}" -gt 0 ]; then
  printf 'steward-orient: INCOMPLETE BRIEFING — %d section(s) came back empty:\n' "${#EMPTY[@]}" >&2
  for e in "${EMPTY[@]}"; do printf '  EMPTY: %s\n' "$e" >&2; done
  printf 'A briefing that silently omits a section is worse than no briefing; fix the input or the reader above.\n' >&2
  RC=1
fi

if [ "$BYTES" -gt "$BUDGET_BYTES" ]; then
  printf 'steward-orient: BUDGET EXCEEDED — %s bytes on stdout, cap is %s. The cap is a promise to the reader'\''s context budget.\n' \
    "$BYTES" "$BUDGET_BYTES" >&2
  RC=1
fi

exit "$RC"
