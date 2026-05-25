#!/usr/bin/env bash
# Cross-check a proposed lever ID against the do-not-retry ledger
# (dev/notes/performance-whitepaper-notes.md § 5).
# Exits 0 with informational output if the lever is not on the ledger;
# exits 1 (with the matching ledger excerpts) if it IS on the ledger
# and the caller needs to record an honest-retry argument in
# dev/plans/0.7.0-perf-experiments.md § Lever taxonomy.
set -euo pipefail

if [ "$#" -lt 1 ]; then
  echo "usage: ledger-check.sh <LEVER_ID>" >&2
  exit 2
fi
LEVER_ID="$1"

LEDGER_FILE="dev/notes/performance-whitepaper-notes.md"
TAXONOMY="dev/plans/0.7.0-perf-experiments.md"

if [ ! -f "$LEDGER_FILE" ]; then
  echo "ledger file not found: $LEDGER_FILE" >&2
  exit 2
fi

# Find the §5 (reverted) block.
SECTION="$(awk '/^## 5\. Experiments tried — reverted/,/^## 6\./' "$LEDGER_FILE" | head -n -1)"

if [ -z "$SECTION" ]; then
  echo "could not extract §5 ledger section from $LEDGER_FILE" >&2
  exit 2
fi

case "$LEVER_ID" in
  L-B1) MATCH="cache_size";;
  L-B2) MATCH="mmap_size";;
  L-B3) MATCH="temp_store";;
  L-B4) MATCH="synchronous";;
  L-B5) MATCH="cache_size|mmap_size|temp_store";;
  L-C1|L-C2) MATCH="LIMIT";;
  L-D1) MATCH="PCACHE2|SQLITE_CONFIG_PCACHE2";;
  L-D2) MATCH="WAL2";;
  L-D3) MATCH="reader.*writer.*split|reader/writer.*split|R-W.*split";;
  L-D4) MATCH="libSQL|sqlite3mc|vendor.SQLite";;
  L-E1) MATCH="bulk-vec|batched.*vec0";;
  L-A0|L-A1) MATCH="";;
  *)
    echo "unknown LEVER_ID: $LEVER_ID" >&2
    exit 2
    ;;
esac

if [ -z "$MATCH" ]; then
  echo "OK: $LEVER_ID has no ledger pattern to match (baseline/diagnostic)."
  exit 0
fi

HITS="$(printf '%s\n' "$SECTION" | grep -nE "$MATCH" || true)"

if [ -z "$HITS" ]; then
  echo "OK: $LEVER_ID not on the do-not-retry ledger ($MATCH)."
  exit 0
fi

echo "WARN: $LEVER_ID matches do-not-retry ledger entries (pattern: $MATCH):"
echo "$HITS"
echo
echo "Honest-retry argument MUST be recorded in $TAXONOMY § Lever taxonomy"
echo "before running this experiment."
if grep -q "^| $LEVER_ID " "$TAXONOMY" 2>/dev/null; then
  echo
  echo "Taxonomy row found for $LEVER_ID — verify the honest-retry argument is present."
  exit 0
fi
echo
echo "No taxonomy row found for $LEVER_ID. Add it before running."
exit 1
