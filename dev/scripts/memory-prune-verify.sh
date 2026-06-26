#!/usr/bin/env bash
# memory-prune-verify.sh — invariant checker / test mechanism for a memory prune.
#
# The memory dir (~/.claude/.../memory) is NOT a git repo: deletions are irreversible.
# This is the gate a memory prune (dev/prune-memory.md) must pass — run it BEFORE (audit
# pre-existing debt) and AFTER (no regressions). Encodes the doc-prune learnings:
#   * index 1:1 integrity  (gate-m analog: MEMORY.md must be a true map)
#   * [[wikilink]] integrity (cross-ref hazard analog of the by-path ADR reference)
#   * snapshot-before-irreversible-delete (memory has no git to recover from)
#   * verify-before-assert: required frontmatter present
#
# Usage:
#   dev/scripts/memory-prune-verify.sh                 # audit current state (exit = #HARD fails)
#   MEMORY_PRUNE_ACTIVE=1 MEMORY_SNAPSHOT=/path ...    # also enforce snapshot-exists (HARD)
# Checks: HARD (counted in exit code) + WARN (reported, not fatal).
set -u
MEM="${CLAUDE_MEMORY_DIR:-$HOME/.claude/projects/-home-coreyt-projects-fathomdb/memory}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"; cd "$REPO_ROOT"
[ -f "$MEM/MEMORY.md" ] || { echo "FAIL: no MEMORY.md at $MEM"; exit 1; }
FAILS=0; WARNS=0
hard() { if [ "$1" -eq 0 ]; then echo "  PASS  $2"; else echo "  FAIL  $2 :: $3"; FAILS=$((FAILS+1)); fi; }
warn() { if [ "$1" -eq 0 ]; then echo "  ok    $2"; else echo "  WARN  $2 :: $3"; WARNS=$((WARNS+1)); fi; }

mapfile -t FILES < <(find "$MEM" -maxdepth 1 -name '*.md' ! -name 'MEMORY.md' | sort)

echo "== memory-prune-verify ($MEM) =="
echo "[integrity]"
# INV-1 dangling index rows (row -> missing file)
DANG=""; while read -r n; do [ -f "$MEM/$n" ] || DANG="$DANG $n"; done < <(grep -oE '\(([a-z0-9.-]+\.md)\)' "$MEM/MEMORY.md" | tr -d '()')
hard "$([ -z "$DANG" ]; echo $?)" "INV-1 no dangling index rows" "$DANG"
# INV-2 unindexed files (file -> no row)
UNIDX=""; for f in "${FILES[@]}"; do b=$(basename "$f" .md); grep -q "($b.md)" "$MEM/MEMORY.md" || UNIDX="$UNIDX $b"; done
hard "$([ -z "$UNIDX" ]; echo $?)" "INV-2 every file has a MEMORY.md row" "$UNIDX"
# INV-3 required frontmatter fields present
BADFM=""; for f in "${FILES[@]}"; do
  { grep -q '^name:' "$f" && grep -q '^description:' "$f" && grep -qE '^\s*type:' "$f"; } || BADFM="$BADFM $(basename "$f" .md)"
done
hard "$([ -z "$BADFM" ]; echo $?)" "INV-3 frontmatter has name+description+type" "$BADFM"

echo "[links]"
# INV-4 every [[wikilink]] resolves to a defined name:
grep -h '^name:' "$MEM"/*.md | sed 's/name: *//;s/[[:space:]]*$//' | sort -u > /tmp/_vfy_names.txt
BROKEN=""; while read -r t; do [ -z "$t" ] && continue; grep -qx "$t" /tmp/_vfy_names.txt || BROKEN="$BROKEN $t"; done \
  < <(grep -rhoE '\[\[[a-z0-9-]+\]\]' "$MEM"/*.md | sed 's/\[\[//;s/\]\]//' | sort -u)
hard "$([ -z "$BROKEN" ]; echo $?)" "INV-4 all [[wikilinks]] resolve" "$BROKEN"

echo "[irreversibility]"
# INV-5 snapshot must exist before a destructive prune (memory has no git)
if [ "${MEMORY_PRUNE_ACTIVE:-0}" = "1" ]; then
  snap="${MEMORY_SNAPSHOT:-}"
  if [ -n "$snap" ] && [ -d "$snap" ]; then
    sc=$(find "$snap" -maxdepth 1 -name '*.md' | wc -l); cc=$(( ${#FILES[@]} + 1 ))
    hard "$([ "$sc" -ge "$cc" ]; echo $?)" "INV-5 snapshot present & complete ($sc >= $cc files)" "snapshot $snap has $sc md, need >= $cc"
  else
    hard 1 "INV-5 snapshot present (MEMORY_SNAPSHOT set to an existing dir)" "MEMORY_SNAPSHOT='$snap'"
  fi
else
  echo "  skip  INV-5 snapshot check (set MEMORY_PRUNE_ACTIVE=1 + MEMORY_SNAPSHOT during a prune)"
fi

echo "[soft / advisory]"
# name != filename (convention allows dots->dashes; flag true mismatches)
NM=""; for f in "${FILES[@]}"; do b=$(basename "$f" .md); nm=$(grep -m1 '^name:' "$f" | sed 's/name: *//;s/[[:space:]]*$//')
  [ "${nm//-/.}" = "${b//-/.}" ] || NM="$NM $b"; done
warn "$([ -z "$NM" ]; echo $?)" "name matches filename (normalized)" "$NM"
# dead repo refs (cited dev/ path that no longer exists)
DEAD=""; while read -r p; do [ -e "$p" ] || DEAD="$DEAD $p"; done < <(grep -rhoE 'dev/[A-Za-z0-9._/-]+\.(md|rs|py|json)' "$MEM"/*.md | sort -u)
warn "$([ -z "$DEAD" ]; echo $?)" "no dead repo-path references" "$DEAD"

echo "== summary: $FAILS HARD fail(s), $WARNS warn(s) =="
exit "$FAILS"
