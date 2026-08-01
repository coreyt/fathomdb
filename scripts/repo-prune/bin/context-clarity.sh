#!/usr/bin/env bash
# context-clarity.sh — reproducible "documentation context clarity" snapshot.
#
# Measures the proxies for how much an agent must wade through to find CURRENT
# state in dev/ (and the per-session memory surface). Run BEFORE and AFTER a
# ledger prune (scripts/repo-prune/prompts/prune-docs.md) and diff the JSON snapshots for the delta.
#
# Usage:
#   scripts/repo-prune/bin/context-clarity.sh [LABEL]
#     LABEL defaults to "baseline". Writes:
#       scripts/repo-prune/measurements/context-clarity/<LABEL>.json   (machine-diffable)
#       scripts/repo-prune/measurements/context-clarity/<LABEL>.md      (human summary)
#
# Notes:
# - Tokens are ESTIMATED as ceil(bytes/4) (no tokenizer dependency); the method
#   is constant across runs so before/after deltas are valid.
# - Operates on ON-DISK files (what grep/glob actually hit), excluding heavy
#   caches; this includes untracked/git-ignored trees like dev/research/.
# - Read-only: never edits/moves repo docs. Safe to run any time.
set -euo pipefail

REPO_ROOT="$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)"
cd "$REPO_ROOT"
LABEL="${1:-baseline}"
OUT_DIR="scripts/repo-prune/measurements/context-clarity"
mkdir -p "$OUT_DIR"
JSON="$OUT_DIR/${LABEL}.json"
MD="$OUT_DIR/${LABEL}.md"

# Caches/binaries to exclude from "what an agent sees".
PRUNE=(
  -path '*/.git' -o -path '*/.venv' -o -path '*/hf_cache'
  -o -path '*/node_modules' -o -path '*/__pycache__'
  -o -path '*/.pytest_cache' -o -path '*/.ruff_cache' -o -path '*/target'
)

# --- helpers -------------------------------------------------------------
# bytes of a set of files passed on stdin (one path per line); 0 if none.
bytes_of() { local t=0 b; while IFS= read -r f; do [ -f "$f" ] || continue; b=$(wc -c <"$f"); t=$((t+b)); done; echo "$t"; }
count_of() { grep -c . || true; }                      # count nonempty stdin lines
est_tokens() { echo $(( ($1 + 3) / 4 )); }             # ceil(bytes/4)

# find md files under a dir, excluding caches (prune dirs FIRST, then match files)
md_under() { find "$1" \( "${PRUNE[@]}" \) -prune -o -type f -name '*.md' -print 2>/dev/null; }
# all files under a dir, excluding caches
all_under() { find "$1" \( "${PRUNE[@]}" \) -prune -o -type f -print 2>/dev/null; }

metric() { # name list-producing-command -> sets _CNT/_BYTES/_TOK
  local list; list="$(eval "$2")"
  _CNT=$(printf '%s\n' "$list" | grep -c . || true)
  _BYTES=$(printf '%s\n' "$list" | bytes_of)
  _TOK=$(est_tokens "$_BYTES")
}

# --- 1. dev/ whole-tree (on disk, ex-caches) -----------------------------
metric dev_all "all_under dev";                 DEV_ALL_C=$_CNT;  DEV_ALL_B=$_BYTES
metric dev_md  "md_under dev";                   DEV_MD_C=$_CNT;   DEV_MD_B=$_BYTES;  DEV_MD_T=$_TOK

# --- 2. live paths vs archive -------------------------------------------
metric live_md "md_under dev | grep -v '^dev/archive/'";  LIVE_MD_C=$_CNT;  LIVE_MD_B=$_BYTES;  LIVE_MD_T=$_TOK
metric arch_md "md_under dev | grep '^dev/archive/'";      ARCH_MD_C=$_CNT;  ARCH_MD_B=$_BYTES

# --- 3. runs/ ledger zone by type ---------------------------------------
RUNS=dev/plans/runs
runs_count_ext() { find "$RUNS" -maxdepth 1 -type f -name "*.$1" 2>/dev/null | grep -c . || true; }
RUNS_ALL=$(all_under "$RUNS" | grep -c . || true)
RUNS_B=$(all_under "$RUNS" | bytes_of)
RUNS_MD=$(runs_count_ext md); RUNS_JSON=$(runs_count_ext json); RUNS_LOG=$(runs_count_ext log); RUNS_TXT=$(runs_count_ext txt)

# --- 4. DOC-INDEX (the cold-start map) ----------------------------------
DI=dev/DOC-INDEX.md
DI_B=$(wc -c <"$DI"); DI_T=$(est_tokens "$DI_B")
DI_ROWS=$(grep -cE '^\| ' "$DI" || true)

# --- 5. cold-start orient set (what an agent reads to get current) -------
# DOC-INDEX + every dev README + root contracts + interface contracts.
orient_list() {
  { echo dev/DOC-INDEX.md
    echo dev/README.md dev/architecture.md dev/requirements.md dev/acceptance.md \
         dev/test-plan.md dev/traceability.md | tr ' ' '\n'
    md_under dev/interfaces
    md_under dev | grep '/README.md$'
  } | sort -u | while IFS= read -r f; do [ -f "$f" ] && echo "$f"; done
}
metric orient "orient_list";  ORIENT_C=$_CNT;  ORIENT_B=$_BYTES;  ORIENT_T=$_TOK

# --- 6. per-session memory surface --------------------------------------
MEM_DIR="${CLAUDE_MEMORY_DIR:-$HOME/.claude/projects/-home-coreyt-projects-fathomdb/memory}"
# (defined here because §5b's resolver needs it; §6 below consumes it too.)
if [ -f "$MEM_DIR/MEMORY.md" ]; then
  MEM_IDX_B=$(wc -c <"$MEM_DIR/MEMORY.md"); MEM_IDX_T=$(est_tokens "$MEM_IDX_B")
  MEM_IDX_ENTRIES=$(grep -c '^- \[' "$MEM_DIR/MEMORY.md" || true)
  MEM_FILES=$(find "$MEM_DIR" -maxdepth 1 -name '*.md' | grep -c . || true)
  MEM_ALL_B=$(find "$MEM_DIR" -maxdepth 1 -name '*.md' -print | bytes_of)
else
  MEM_IDX_B=0; MEM_IDX_T=0; MEM_IDX_ENTRIES=0; MEM_FILES=0; MEM_ALL_B=0
fi

# --- 5b. STEWARD cold-start set (a SECOND, disjoint orient axis) ---------
# §5's orient_list() is the "understand the codebase" set: DOC-INDEX + READMEs +
# root/interface contracts. A /steward cold start reads something else entirely —
# the boards, ladders, master and hand-offs named by §3 of the steward hand-off.
# Measured 2026-07-31: the two sets share ZERO files, and the 2026-06-26 prune
# cut dev/ md tokens 46% while cold_start_orient_set rose 1.8%. Tree size and
# reading-list cost are different variables; this metric is the second one.
#
# DERIVED FROM §3, NOT HARDCODED. orient_list() above is a hardcoded list, which
# is fine for a campaign instrument but would be a vacuous gate: it would keep
# passing while §3 grew. This resolver reads the hand-off itself, so adding a
# file to §3 shows up here.
#
# Partial is allowed; SILENTLY partial is not (the steward-orient.sh contract).
# Any §3 item that resolves to nothing, and is not declared file-less, is a hard
# failure naming the item.
STEWARD_HANDOFF=dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md
# RATCHET, not the destination. 180,000 is just above the post-Phase-1 measurement
# (~174,800). The end state is 60,000, reachable only after 0.8.20 publishes and
# Phase 3 may split the live board (steward ledger seq-226). A ceiling set to the
# destination on day one is red on day one, and a gate that is red on day one gets
# switched off — which is how repo-prune's own metrics went unratcheted. Tighten
# this as each phase lands; never loosen it without a ruling.
STEWARD_CEILING="${STEWARD_COLDSTART_CEILING:-180000}"

# Body of a numbered §3 item: from "<n>. " to the next numbered item or "## ".
steward_item_body() { # $1 = item number
  awk -v want="$1" '
    /^## *3\./      { in3=1; next }
    in3 && /^## /   { exit }
    !in3            { next }
    /^[0-9]+\. /    { cur = $1+0 }
    { if (cur == want) print }
  ' "$STEWARD_HANDOFF"
}

# Resolve one backticked token from §3 into existing file paths.
#
# NOTE for whoever fixes TC-139 (Slice 40 item B6): this resolver REQUIRES
# pathname expansion for `plan-0.8.*.md` and `STATUS-0.8.*.md`. If the TC-139 fix
# is `set -f` at file scope rather than wrapped tightly around the `find`, this
# function breaks. It therefore enables globbing locally and restores the caller's
# state, so it is correct under either fix shape.
steward_resolve() { # $1 = raw token
  local t="$1" p
  local _noglob=0; case $- in *f*) _noglob=1;; esac
  set +f
  case "$t" in
    # `…/memory/MEMORY.md` — an elided absolute path.
    *…*|*...*)      t="$MEM_DIR/MEMORY.md" ;;
  esac
  case "$t" in
    # `plan-0.8.z.md` is a PATTERN written in prose, not a literal filename.
    plan-0.8.z.md)  t="dev/plans/plan-0.8.*.md" ;;
  esac
  # A bare filename (item 5's orchestrator hand-off) is SEARCHED across the doc
  # dirs, not assumed to live in one. Assuming `dev/plans/prompts/` silently
  # mis-resolved every bare name that lives elsewhere.
  case "$t" in
    */*) : ;;
    *)   local d found=""
         for d in dev/plans/prompts dev/plans dev/design dev/plans/runs; do
           [ -f "$d/$t" ] && { found="$d/$t"; break; }
         done
         t="${found:-$t}" ;;
  esac
  for p in $t; do [ -f "$p" ] && printf '%s\n' "$p"; done   # deliberate glob
  [ "$_noglob" -eq 1 ] && set -f
  return 0
}

# The item numbers §3 actually contains — NOT a hardcoded 1..7.
# A first draft hardcoded `for _i in 1 2 3 4 5 6 7` while its comment claimed the
# metric was derived. Adding an item 8 to §3 then changed nothing and failed
# nothing: the silent under-report this whole section exists to prevent.
steward_item_numbers() {
  awk '
    /^## *3\./    { in3=1; next }
    in3 && /^## / { exit }
    in3 && /^[0-9]+\. / { n=$1+0; if (!(n in seen)) { seen[n]=1; print n } }
  ' "$STEWARD_HANDOFF"
}

# Path-like backticked tokens in item N.
# A token counts as a path if it contains "/" or ends in a document extension.
# That admits `dev/plans/...md`, `plan-0.8.z.md` and `…/memory/MEMORY.md` while
# rejecting item 2's `git log --oneline -30` / `git status`. Matching only
# `\.md$` (the first draft) silently dropped `.json`, extensionless names and
# `file.md#anchor`.
# EXECUTABLES ARE EXCLUDED. §3 names `steward-orient.sh` as something to RUN;
# what enters context is its ~955-token output, not the 8 KB script, and that
# output is bounded and reported separately. Counting the file would overstate
# the read and — because §3 names it bare — would also make the resolver guess a
# directory. Reading cost is documents.
steward_item_tokens() { # $1 = item number
  steward_item_body "$1" \
    | grep -oE '`[^`]+`' | tr -d '`' \
    | sed 's/#.*$//' \
    | grep -vE '^git( |$)' \
    | grep -vE '\.sh$' \
    | grep -E '/|\.(md|json|ya?ml|toml)$' \
    | sort -u
}

# Mirror §3's LIVENESS RULE, and only when §3 actually states it.
#
# §3 names globs (`STATUS-0.8.*.md`, `plan-0.8.z.md`) and then narrows them in
# prose — "the LIVE board only", "COMPLETE ladders are historical". Expanding the
# glob literally measures 19 boards and 20 ladders and reports ~374k forever,
# so the ratchet would stay red no matter how much reading the instruction
# actually removed: a gate blind to the fix it exists to reward.
#
# The filter is applied ONLY IF the item's own text cites the predicate. If §3
# reverts to an unfiltered glob, the text stops citing it, the filter switches
# off, and the number climbs back — the metric tracks the INSTRUCTION, which is
# the thing that determines what gets read.
# shellcheck source=scripts/lib/board-closed.sh
. scripts/lib/board-closed.sh 2>/dev/null || true

steward_live_filter() { # $1 = item number; stdin paths -> stdout the ones §3 says to read
  local n="$1" body f
  body="$(steward_item_body "$n")"
  while IFS= read -r f; do
    case "$f" in
      dev/plans/runs/STATUS-0.8.*.md)
        if printf '%s' "$body" | grep -q 'board_is_closed'; then
          board_is_closed "$f" 2>/dev/null && continue
        fi ;;
      dev/plans/plan-0.8.*.md)
        # ALLOW-LIST, not a deny-list. Excluding only COMPLETE let SUPERSEDED
        # ladders (plan-0.8.15, plan-0.8.17) through, so the metric counted 4
        # where §3 names 2. §3 says ACTIVE or PROPOSED; match that exactly.
        if printf '%s' "$body" | grep -qE 'status:'; then
          head -6 "$f" | grep -qiE '^status: *(ACTIVE|PROPOSED)' || continue
        fi ;;
    esac
    printf '%s\n' "$f"
  done
}

# Files named by item N of §3, one path per line.
# Item bodies that name no path but still imply a read are matched on CONTENT,
# never on item number — renumbering §3 must not silently drop the rule.
steward_item_files() { # $1 = item number
  local tok
  steward_item_tokens "$1" | while IFS= read -r tok; do steward_resolve "$tok"; done \
    | steward_live_filter "$1"
  if steward_item_body "$1" | grep -qiE 'last Steward report|your own last'; then
    ls dev/plans/runs/STEWARD-SESSION-HANDOFF-*.md 2>/dev/null | sort | tail -1
  fi
}

# Tokens in item N that resolved to NOTHING. The guard must be per-REFERENCE,
# not per-item: item 5 names two files, so an item-level "did it produce any
# file?" test stays green when one of the two moves, and undercounts in silence.
steward_item_dead_tokens() { # $1 = item number
  local tok
  steward_item_tokens "$1" | while IFS= read -r tok; do
    [ -z "$(steward_resolve "$tok")" ] && printf '3.%s:%s ' "$1" "$tok"
  done
  return 0
}

# The entry point itself: /steward loads these before §3 is even reached.
steward_entry_files() {
  { echo "$STEWARD_HANDOFF"
    echo .claude/commands/steward.md
    echo .claude/agents/steward.md
  } | while IFS= read -r f; do [ -f "$f" ] && echo "$f"; done
}

STEWARD_ROWS=""; STEWARD_ALL=""; STEWARD_UNRESOLVED=""; _SC_LAST=0
# Sets globals directly and reports the count in _SC_LAST. It must NOT be called
# via $(...): a command substitution runs in a subshell, so the STEWARD_ROWS /
# STEWARD_ALL accumulation would be silently discarded — which is exactly how the
# first draft of this metric reported 0 files while its own guard stayed quiet.
_sc_add() { # $1 = item label, $2 = newline-separated paths
  local list="$2" c b t
  c=$(printf '%s\n' "$list" | grep -c . || true)
  b=$(printf '%s\n' "$list" | bytes_of)
  t=$(est_tokens "$b")
  STEWARD_ROWS="${STEWARD_ROWS}    {\"item\": \"$1\", \"files\": $c, \"bytes\": $b, \"tokens_est\": $t},\n"
  if [ "$c" -gt 0 ]; then STEWARD_ALL="${STEWARD_ALL}${list}"$'\n'; fi
  _SC_LAST="$c"
}

_sc_add entry "$(steward_entry_files)"
[ "$_SC_LAST" -gt 0 ] || STEWARD_UNRESOLVED="${STEWARD_UNRESOLVED}entry "

STEWARD_ITEMS="$(steward_item_numbers)"
[ -n "$STEWARD_ITEMS" ] || STEWARD_UNRESOLVED="${STEWARD_UNRESOLVED}§3-has-no-numbered-items "
for _i in $STEWARD_ITEMS; do
  _sc_add "3.$_i" "$(steward_item_files "$_i")"
  # An item with NO path-like token (item 2 = git commands) legitimately names
  # no file — derived from the body, not from a hardcoded exemption list.
  STEWARD_UNRESOLVED="${STEWARD_UNRESOLVED}$(steward_item_dead_tokens "$_i")"
done
STEWARD_ROWS="${STEWARD_ROWS%,\\n}"

# De-duplicate: a file named by two items is read once, so count it once.
STEWARD_UNIQ=$(printf '%s\n' "$STEWARD_ALL" | grep -v '^$' | sort -u || true)
STEW_C=$(printf '%s\n' "$STEWARD_UNIQ" | grep -c . || true)
STEW_B=$(printf '%s\n' "$STEWARD_UNIQ" | bytes_of)
STEW_T=$(est_tokens "$STEW_B")
STEW_OVER=$( [ "$STEW_T" -gt "$STEWARD_CEILING" ] && echo true || echo false )

if [ -n "$STEWARD_UNRESOLVED" ]; then
  echo "context-clarity: FAIL — steward §3 item(s) resolved to no files: $STEWARD_UNRESOLVED" >&2
  echo "  (§3 of $STEWARD_HANDOFF changed shape, or a named file moved.)" >&2
  echo "  A silently-partial cold-start measurement is worse than none — refusing to write." >&2
  exit 3
fi

# --- 7. search signal-to-noise ------------------------------------------
# For a fixed query set: total live-path md files matching, and how many are in
# ledger zones (runs/ or prompts/) vs core (design/adr/interfaces/root/notes).
QUERIES=("CE-rerank" "rerank" "graphrag" "recall floor" "logical_id" "mem0" "RRF" "GraphRAG")
SN_ROWS=""
for q in "${QUERIES[@]}"; do
  hits=$(grep -rilF "$q" --include='*.md' dev 2>/dev/null | grep -v '^dev/archive/' || true)
  tot=$(printf '%s\n' "$hits" | grep -c . || true)
  ledger=$(printf '%s\n' "$hits" | grep -E '^dev/plans/(runs|prompts)/' | grep -c . || true)
  core=$((tot-ledger))
  SN_ROWS="${SN_ROWS}    {\"query\": \"$q\", \"total_files\": $tot, \"ledger_zone\": $ledger, \"core\": $core},\n"
done
SN_ROWS="${SN_ROWS%,\\n}"

# --- write JSON ----------------------------------------------------------
GIT_SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ)
cat >"$JSON" <<EOF
{
  "label": "$LABEL",
  "git_sha": "$GIT_SHA",
  "captured_utc": "$STAMP",
  "token_estimate": "ceil(bytes/4)",
  "dev_tree": { "all_files": $DEV_ALL_C, "all_bytes": $DEV_ALL_B,
                "md_files": $DEV_MD_C, "md_bytes": $DEV_MD_B, "md_tokens_est": $DEV_MD_T },
  "live_vs_archive": { "live_md_files": $LIVE_MD_C, "live_md_bytes": $LIVE_MD_B, "live_md_tokens_est": $LIVE_MD_T,
                       "archive_md_files": $ARCH_MD_C, "archive_md_bytes": $ARCH_MD_B },
  "runs_zone": { "all_files": $RUNS_ALL, "all_bytes": $RUNS_B,
                 "md": $RUNS_MD, "json": $RUNS_JSON, "log": $RUNS_LOG, "txt": $RUNS_TXT },
  "doc_index": { "bytes": $DI_B, "tokens_est": $DI_T, "table_rows": $DI_ROWS },
  "cold_start_orient_set": { "files": $ORIENT_C, "bytes": $ORIENT_B, "tokens_est": $ORIENT_T },
  "steward_cold_start_set": { "files": $STEW_C, "bytes": $STEW_B, "tokens_est": $STEW_T,
                              "ceiling_tokens": $STEWARD_CEILING, "over_ceiling": $STEW_OVER,
                              "source": "$STEWARD_HANDOFF §3 (derived, not hardcoded)",
    "items": [
$(printf "$STEWARD_ROWS")
    ] },
  "memory_surface": { "index_bytes": $MEM_IDX_B, "index_tokens_est": $MEM_IDX_T,
                      "index_entries": $MEM_IDX_ENTRIES, "memory_files": $MEM_FILES, "memory_all_bytes": $MEM_ALL_B },
  "search_signal_to_noise": [
$(printf "$SN_ROWS")
  ]
}
EOF

# --- write human summary -------------------------------------------------
{
  echo "# Context-clarity snapshot — \`$LABEL\` ($GIT_SHA, $STAMP)"
  echo
  echo "Token counts are estimates: ceil(bytes/4). Re-run \`scripts/repo-prune/bin/context-clarity.sh <label>\` and diff JSON for deltas."
  echo
  echo "| Metric | Value |"
  echo "|---|---|"
  echo "| dev/ files (ex-caches) | $DEV_ALL_C ($DEV_ALL_B bytes) |"
  echo "| dev/ .md files | $DEV_MD_C ($DEV_MD_B bytes, ~$DEV_MD_T tok) |"
  echo "| live-path .md (ex archive/) | $LIVE_MD_C ($LIVE_MD_B bytes, ~$LIVE_MD_T tok) |"
  echo "| archive/ .md | $ARCH_MD_C ($ARCH_MD_B bytes) |"
  echo "| runs/ zone files | $RUNS_ALL ($RUNS_B bytes); md=$RUNS_MD json=$RUNS_JSON log=$RUNS_LOG txt=$RUNS_TXT |"
  echo "| DOC-INDEX.md | $DI_B bytes, ~$DI_T tok, $DI_ROWS table rows |"
  echo "| cold-start orient set | $ORIENT_C files, $ORIENT_B bytes, ~$ORIENT_T tok |"
  echo "| **steward cold-start set** | **$STEW_C files, $STEW_B bytes, ~$STEW_T tok** (ceiling $STEWARD_CEILING, over=$STEW_OVER) |"
  echo "| memory MEMORY.md index | $MEM_IDX_B bytes, ~$MEM_IDX_T tok, $MEM_IDX_ENTRIES entries |"
  echo "| memory/ dir (all .md) | $MEM_FILES files, $MEM_ALL_B bytes |"
  echo
  echo "## Steward cold-start set by §3 item (derived from the hand-off, not hardcoded)"
  echo
  echo "| §3 item | Files | ~Tokens |"
  echo "|---|---|---|"
  printf "$STEWARD_ROWS\n" | sed -n 's/.*"item": "\([^"]*\)", "files": \([0-9]*\), "bytes": [0-9]*, "tokens_est": \([0-9]*\).*/| \1 | \2 | \3 |/p'
  echo
  echo "## Search signal-to-noise (live-path .md files matching; ledger = runs/+prompts/)"
  echo  # MD022/MD058: blank line between the heading and the table below
  echo "| Query | Total files | Ledger-zone | Core |"
  echo "|---|---|---|---|"
  for q in "${QUERIES[@]}"; do
    hits=$(grep -rilF "$q" --include='*.md' dev 2>/dev/null | grep -v '^dev/archive/' || true)
    tot=$(printf '%s\n' "$hits" | grep -c . || true)
    ledger=$(printf '%s\n' "$hits" | grep -E '^dev/plans/(runs|prompts)/' | grep -c . || true)
    echo "| $q | $tot | $ledger | $((tot-ledger)) |"
  done
} >"$MD"

echo "wrote $JSON and $MD"
cat "$MD"
