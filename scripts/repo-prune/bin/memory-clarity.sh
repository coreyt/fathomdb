#!/usr/bin/env bash
# memory-clarity.sh — reproducible "agent-memory clarity" snapshot.
#
# Companion to context-clarity.sh, but for the persistent agent memory
# (~/.claude/.../memory/). MEMORY.md loads into context EVERY session, so its size
# is a per-turn token tax and its staleness is an act-on-wrong-fact risk. Measures
# not just bytes but redirects (pointers vs duplicated knowledge), staleness
# (dead repo refs, superseded markers), link health, index integrity, and dup
# clusters. Run BEFORE and AFTER a memory prune (scripts/repo-prune/prompts/prune-memory.md) and diff JSON.
#
# Usage: scripts/repo-prune/bin/memory-clarity.sh [LABEL]   (LABEL default "baseline")
#   Writes scripts/repo-prune/measurements/memory-clarity/<LABEL>.{json,md}
# Read-only. Token est = ceil(bytes/4) (no tokenizer dep; constant method).
set -u  # not -e/pipefail: grep|wc legitimately exits nonzero on zero matches
REPO_ROOT="$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)"
cd "$REPO_ROOT"
LABEL="${1:-baseline}"
MEM="${CLAUDE_MEMORY_DIR:-$HOME/.claude/projects/-home-coreyt-projects-fathomdb/memory}"
OUT="scripts/repo-prune/measurements/memory-clarity"; mkdir -p "$OUT"
JSON="$OUT/${LABEL}.json"; MD="$OUT/${LABEL}.md"
est() { echo $(( ($1 + 3) / 4 )); }

[ -f "$MEM/MEMORY.md" ] || { echo "no MEMORY.md at $MEM" >&2; exit 1; }
mapfile -t FILES < <(find "$MEM" -maxdepth 1 -name '*.md' ! -name 'MEMORY.md' | sort)
NFILES=${#FILES[@]}
ALL_BYTES=$(cat "$MEM"/*.md | wc -c)
IDX_BYTES=$(wc -c <"$MEM/MEMORY.md"); IDX_TOK=$(est "$IDX_BYTES")
IDX_ROWS=$(grep -c '^- \[' "$MEM/MEMORY.md" || true)

# --- index integrity ---
UNINDEXED=0; UNINDEXED_LIST=""
for f in "${FILES[@]}"; do b=$(basename "$f" .md)
  grep -q "($b.md)" "$MEM/MEMORY.md" || { UNINDEXED=$((UNINDEXED+1)); UNINDEXED_LIST="$UNINDEXED_LIST $b"; }
done
DANGLING_ROWS=0; DANGLING_LIST=""
while read -r n; do [ -f "$MEM/$n" ] || { DANGLING_ROWS=$((DANGLING_ROWS+1)); DANGLING_LIST="$DANGLING_LIST $n"; }
done < <(grep -oE '\(([a-z0-9-]+\.md)\)' "$MEM/MEMORY.md" | tr -d '()')

# --- frontmatter validity (required fields present) + name/filename mismatch (informational) ---
BAD_FM=0; BAD_FM_LIST=""; NAME_MISMATCH=0; NAME_MISMATCH_LIST=""
for f in "${FILES[@]}"; do b=$(basename "$f" .md); ok=1
  grep -q '^name:' "$f" || ok=0
  grep -q '^description:' "$f" || ok=0
  grep -qE '^\s*type:' "$f" || ok=0
  [ "$ok" = 1 ] || { BAD_FM=$((BAD_FM+1)); BAD_FM_LIST="$BAD_FM_LIST $b"; }
  nm=$(grep -m1 '^name:' "$f" | sed 's/name: *//;s/[[:space:]]*$//')
  # convention: filename may use dots where name uses dashes -> normalize before compare
  [ "${nm//-/.}" = "${b//-/.}" ] || { NAME_MISMATCH=$((NAME_MISMATCH+1)); NAME_MISMATCH_LIST="$NAME_MISMATCH_LIST $b"; }
done

# --- link health (wikilinks; dotted filename-style OR dashed name-style) ---
grep -h '^name:' "$MEM"/*.md | sed 's/name: *//;s/[[:space:]]*$//' | sort -u > /tmp/_mem_names.txt
WL_TOTAL=$(grep -rhoE '\[\[[a-z0-9._-]+\]\]' "$MEM"/*.md | wc -l)
WL_DISTINCT=$(grep -rhoE '\[\[[a-z0-9._-]+\]\]' "$MEM"/*.md | sort -u | wc -l)
WL_BROKEN=0; WL_BROKEN_LIST=""
while read -r t; do [ -z "$t" ] && continue
  { [ -f "$MEM/$t.md" ] || grep -qx "${t//./-}" /tmp/_mem_names.txt; } || { WL_BROKEN=$((WL_BROKEN+1)); WL_BROKEN_LIST="$WL_BROKEN_LIST $t"; }
done < <(grep -rhoE '\[\[[a-z0-9._-]+\]\]' "$MEM"/*.md | sed 's/\[\[//;s/\]\]//' | sort -u)

# --- redirects (pointer-ness): files that defer to a durable source ---
FILES_WITH_WIKILINK=$(grep -lE '\[\[[a-z0-9-]+\]\]' "${FILES[@]}" 2>/dev/null | wc -l)
FILES_CITING_REPO=$(grep -lE 'dev/[A-Za-z0-9._/-]+\.(md|rs|py|json)' "${FILES[@]}" 2>/dev/null | wc -l)
FILES_CITING_LEDGER=$(grep -lF 'experiments-ledger.md' "${FILES[@]}" 2>/dev/null | wc -l)

# --- staleness ---
# dead repo refs: cited dev/ path that no longer exists in the repo tree
STALE_REFS=0; STALE_LIST=""
while read -r p; do [ -e "$p" ] || { STALE_REFS=$((STALE_REFS+1)); STALE_LIST="$STALE_LIST $p"; }
done < <(grep -rhoE 'dev/[A-Za-z0-9._/-]+\.(md|rs|py|json)' "$MEM"/*.md | sort -u)
# superseded/aging markers in index descriptions (consolidation candidates)
SUPERSEDED_MARKERS=$(grep -ciE 'supersed|overturn|reframe|demoted|CLOSED|RESOLVED|no-go|withdrawn|artifact' "$MEM/MEMORY.md" || true)

# --- type breakdown ---
T_USER=$(grep -rhE '^\s*type:\s*user' "$MEM"/*.md | wc -l)
T_FEEDBACK=$(grep -rhE '^\s*type:\s*feedback' "$MEM"/*.md | wc -l)
T_PROJECT=$(grep -rhE '^\s*type:\s*project' "$MEM"/*.md | wc -l)
T_REFERENCE=$(grep -rhE '^\s*type:\s*reference' "$MEM"/*.md | wc -l)

# --- dup/overlap clusters by release token in index descriptions ---
cluster_count() { grep -c "$1" "$MEM/MEMORY.md" || true; }
C_083=$(cluster_count '0\.8\.3'); C_084=$(cluster_count '0\.8\.4'); C_080=$(cluster_count '0\.8\.0')
C_072=$(cluster_count '0\.7\.2'); C_eu7=$(cluster_count 'eu7')

# --- write JSON ---
STAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ); SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
cat >"$JSON" <<EOF
{
  "label": "$LABEL", "captured_utc": "$STAMP", "repo_sha": "$SHA",
  "memory_dir": "$MEM", "git_tracked": false, "token_estimate": "ceil(bytes/4)",
  "size": { "files": $NFILES, "all_bytes": $ALL_BYTES,
            "index_bytes": $IDX_BYTES, "index_tokens_est": $IDX_TOK, "index_rows": $IDX_ROWS },
  "integrity": { "unindexed_files": $UNINDEXED, "dangling_index_rows": $DANGLING_ROWS,
                 "invalid_frontmatter": $BAD_FM, "name_filename_mismatch": $NAME_MISMATCH },
  "link_health": { "wikilinks_total": $WL_TOTAL, "wikilinks_distinct": $WL_DISTINCT, "wikilinks_broken": $WL_BROKEN },
  "redirects": { "files_with_wikilink": $FILES_WITH_WIKILINK, "files_citing_repo": $FILES_CITING_REPO,
                 "files_citing_experiments_ledger": $FILES_CITING_LEDGER },
  "staleness": { "dead_repo_refs": $STALE_REFS, "superseded_aging_markers_in_index": $SUPERSEDED_MARKERS },
  "types": { "user": ${T_USER:-0}, "feedback": $T_FEEDBACK, "project": $T_PROJECT, "reference": $T_REFERENCE },
  "overlap_clusters_in_index": { "0.8.0": $C_080, "0.7.2": $C_072, "0.8.3": $C_083, "0.8.4": $C_084, "eu7": $C_eu7 }
}
EOF

# --- human summary ---
{
  echo "# Memory-clarity snapshot — \`$LABEL\` ($STAMP, repo $SHA)"
  echo; echo "Memory dir: \`$MEM\` (NOT git-tracked → deletions irreversible; snapshot before pruning)."
  echo "Tokens est ceil(bytes/4). MEMORY.md loads every session = per-turn cost."
  echo
  echo "| Metric | Value |"
  echo "|---|---|"
  echo "| memory files | $NFILES (+ MEMORY.md) |"
  echo "| total bytes | $ALL_BYTES |"
  echo "| MEMORY.md index | $IDX_BYTES bytes, ~$IDX_TOK tok/session, $IDX_ROWS rows |"
  echo "| **integrity:** unindexed files | $UNINDEXED ($UNINDEXED_LIST ) |"
  echo "| integrity: dangling index rows | $DANGLING_ROWS ($DANGLING_LIST ) |"
  echo "| integrity: invalid frontmatter (missing field) | $BAD_FM ($BAD_FM_LIST ) |"
  echo "| integrity: name≠filename (informational) | $NAME_MISMATCH |"
  echo "| **link health:** wikilinks total / distinct | $WL_TOTAL / $WL_DISTINCT |"
  echo "| link health: BROKEN wikilinks | $WL_BROKEN ($WL_BROKEN_LIST ) |"
  echo "| **redirects:** files w/ wikilink | $FILES_WITH_WIKILINK / $NFILES |"
  echo "| redirects: files citing repo paths | $FILES_CITING_REPO |"
  echo "| redirects: files citing experiments-ledger | $FILES_CITING_LEDGER |"
  echo "| **staleness:** dead repo refs | $STALE_REFS ($STALE_LIST ) |"
  echo "| staleness: superseded/aging markers in index | $SUPERSEDED_MARKERS |"
  echo "| types (user/feedback/project/reference) | ${T_USER:-0}/$T_FEEDBACK/$T_PROJECT/$T_REFERENCE |"
  echo "| overlap clusters in index (0.8.0/0.7.2/0.8.3/0.8.4/eu7) | $C_080/$C_072/$C_083/$C_084/$C_eu7 |"
} >"$MD"
echo "wrote $JSON and $MD"; cat "$MD"
