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
PRUNE_EXPR='-path */.git -o -path */.venv -o -path */hf_cache -o -path */node_modules -o -path */__pycache__ -o -path */.pytest_cache -o -path */.ruff_cache -o -path */target'

# --- helpers -------------------------------------------------------------
# bytes of a set of files passed on stdin (one path per line); 0 if none.
bytes_of() { local t=0 b; while IFS= read -r f; do [ -f "$f" ] || continue; b=$(wc -c <"$f"); t=$((t+b)); done; echo "$t"; }
count_of() { grep -c . || true; }                      # count nonempty stdin lines
est_tokens() { echo $(( ($1 + 3) / 4 )); }             # ceil(bytes/4)

# find md files under a dir, excluding caches (prune dirs FIRST, then match files)
md_under() { find "$1" \( $PRUNE_EXPR \) -prune -o -type f -name '*.md' -print 2>/dev/null; }
# all files under a dir, excluding caches
all_under() { find "$1" \( $PRUNE_EXPR \) -prune -o -type f -print 2>/dev/null; }

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
if [ -f "$MEM_DIR/MEMORY.md" ]; then
  MEM_IDX_B=$(wc -c <"$MEM_DIR/MEMORY.md"); MEM_IDX_T=$(est_tokens "$MEM_IDX_B")
  MEM_IDX_ENTRIES=$(grep -c '^- \[' "$MEM_DIR/MEMORY.md" || true)
  MEM_FILES=$(find "$MEM_DIR" -maxdepth 1 -name '*.md' | grep -c . || true)
  MEM_ALL_B=$(find "$MEM_DIR" -maxdepth 1 -name '*.md' -print | bytes_of)
else
  MEM_IDX_B=0; MEM_IDX_T=0; MEM_IDX_ENTRIES=0; MEM_FILES=0; MEM_ALL_B=0
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
  echo "| memory MEMORY.md index | $MEM_IDX_B bytes, ~$MEM_IDX_T tok, $MEM_IDX_ENTRIES entries |"
  echo "| memory/ dir (all .md) | $MEM_FILES files, $MEM_ALL_B bytes |"
  echo
  echo "## Search signal-to-noise (live-path .md files matching; ledger = runs/+prompts/)"
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
