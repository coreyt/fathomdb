#!/usr/bin/env bash
# enumerate-corpora.sh — READ-ONLY corpus inventory for FathomDB.
#
# Purpose: re-orient a corpus-search cycle FAST. Scans the on-disk corpus +
# eval artifacts, lists what is present (sizes, line counts, license markers,
# gitignore status), and reports MAP-EXPECTS vs ACTUALLY-ON-DISK so a new
# cycle can see at a glance what is already acquired and what is still a gap.
#
# Guarantees: no downloads, no network, no mutations. Safe to run repeatedly.
#
# Usage:
#   dev/corpus-survey/enumerate-corpora.sh            # uses repo-relative data dir
#   FATHOMDB_DATA=/path/to/corpus-data enumerate-corpora.sh
#
# The corpus payloads live under data/corpus-data/ which is .gitignored, so the
# data may be physically present only in the primary checkout (NOT in a fresh
# worktree). This script probes a few well-known locations; override with
# FATHOMDB_DATA if your data lives elsewhere.

set -uo pipefail

# --- locate repo root + data dir ------------------------------------------
REPO_ROOT="$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel 2>/dev/null || (cd "$(dirname "${BASH_SOURCE[0]}")" && pwd))"

probe_data_dir() {
  local candidates=(
    "${FATHOMDB_DATA:-}"
    "${REPO_ROOT}/data/corpus-data"
    "/home/coreyt/projects/fathomdb/data/corpus-data"
  )
  local c
  for c in "${candidates[@]}"; do
    [ -n "$c" ] && [ -d "$c" ] && { printf '%s\n' "$c"; return 0; }
  done
  return 1
}

DATA_DIR="$(probe_data_dir || true)"
CORPUS_DIR="${REPO_ROOT}/tests/corpus"

echo "============================================================"
echo " FathomDB corpus inventory (READ-ONLY)"
echo " repo root : ${REPO_ROOT}"
echo " data dir  : ${DATA_DIR:-<NOT FOUND — set FATHOMDB_DATA>}"
echo " date      : $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "============================================================"

human() { du -sh "$1" 2>/dev/null | cut -f1; }
lines() { [ -f "$1" ] && wc -l < "$1" 2>/dev/null | tr -d ' ' || echo '-'; }

# --- 1. committed corpus scaffolding (tests/corpus) -----------------------
echo
echo "## 1. Committed corpus scaffolding (tests/corpus/ — in git)"
if [ -d "$CORPUS_DIR" ]; then
  for f in corpus-card.md README.md snapshot.json scripts/manifest.json; do
    p="${CORPUS_DIR}/${f}"
    [ -f "$p" ] && printf '  [present] %-26s %s\n' "$f" "$(human "$p")"
  done
  n_chains=$(find "${CORPUS_DIR}/chains" -name '*.json' 2>/dev/null | wc -l | tr -d ' ')
  [ -d "${CORPUS_DIR}/chains" ] \
    && echo "  [present] chains/                    ${n_chains} chain definitions" \
    || echo "  [MISS]    chains/                    (directory absent)"
  n_acq=$(find "${CORPUS_DIR}/scripts" -name 'acquire_*.py' 2>/dev/null | wc -l | tr -d ' ')
  [ -d "${CORPUS_DIR}/scripts" ] \
    && echo "  [present] scripts/acquire_*.py       ${n_acq} acquisition scripts:" \
    || echo "  [MISS]    scripts/acquire_*.py       (directory absent)"
  find "${CORPUS_DIR}/scripts" -name 'acquire_*.py' 2>/dev/null | sort | sed 's|.*/|      |'
else
  echo "  <tests/corpus not found>"
fi

# --- 2. on-disk corpus payloads (data/corpus-data — gitignored) -----------
echo
echo "## 2. On-disk corpus payloads (data/corpus-data/ — GITIGNORED, never committed)"
if [ -n "${DATA_DIR}" ]; then
  for sub in raw eval eval/ir_gold external downloads; do
    d="${DATA_DIR}/${sub}"
    [ -d "$d" ] || continue
    echo
    echo "  ### ${sub}/"
    # list files (depth 1) with size + line count for text payloads
    find "$d" -maxdepth 1 -mindepth 1 \( -type f -o -type d \) 2>/dev/null | sort | while read -r p; do
      base="$(basename "$p")"
      if [ -d "$p" ]; then
        printf '    [dir ] %-34s %s\n' "$base/" "$(human "$p")"
      else
        case "$base" in
          *.jsonl|*.json) printf '    [file] %-34s %8s  (%s lines)\n' "$base" "$(human "$p")" "$(lines "$p")" ;;
          *)              printf '    [file] %-34s %8s\n' "$base" "$(human "$p")" ;;
        esac
      fi
    done
  done
  # license markers
  echo
  echo "  ### license markers found under data/corpus-data/"
  find "${DATA_DIR}" -maxdepth 3 \( -iname '*LICENSE*' -o -iname '*COPYING*' \) 2>/dev/null | sort | sed "s|$(printf '%s' "${DATA_DIR}" | sed 's|[\\&|]|\\&|g')/|    |"
else
  echo "  <data dir not found — corpus payloads may live only in the primary checkout>"
  echo "  Rebuild reproducibly via tests/corpus/scripts/acquire_*.py (see corpus-card.md)."
fi

# --- 3. gitignore confirmation --------------------------------------------
echo
echo "## 3. Gitignore posture (payloads MUST be ignored)"
if git -C "${REPO_ROOT}" check-ignore -q "data/corpus-data/raw/x" 2>/dev/null; then
  echo "  [OK] data/corpus-data/ is gitignored (payloads never committed)."
else
  echo "  [WARN] data/corpus-data/ does NOT appear gitignored — investigate .gitignore."
fi

# --- 4. map-expects vs on-disk reconciliation -----------------------------
# Keep this list in sync with dev/corpus-survey/corpus-map.md (ON-DISK rows).
echo
echo "## 4. MAP-EXPECTS vs ON-DISK (reconcile against corpus-map.md)"
echo "  legend: [HAVE] present  ·  [MISS] expected by map but not found  ·  [HF] HuggingFace-streamed (cached, not a static file)"
check() { # $1 = label, $2 = relative path under DATA_DIR ("" if HF-streamed)
  local label="$1" rel="${2:-}"
  if [ -z "$rel" ]; then printf '  [HF]   %s\n' "$label"; return; fi
  if [ -n "${DATA_DIR}" ] && [ -e "${DATA_DIR}/${rel}" ]; then
    printf '  [HAVE] %-26s %s\n' "$label" "$rel"
  else
    printf '  [MISS] %-26s %s\n' "$label" "$rel"
  fi
}
check "LOCOMO (agentic mem)"      "raw/locomo10.json"
check "AP-News BenchmarkQED"      "raw/apnews_benchmarkqed/raw_data.zip"
check "MuSiQue (multi-hop)"       "raw/musique_dev.jsonl"
check "Enron (email)"             "raw/enron.jsonl"
check "EnronQA"                   "raw/enronqa.jsonl"
check "QMSum (meeting)"           "raw/qmsum.jsonl"
check "QAConv"                    "raw/qaconv.jsonl"
check "QASPER (paper)"            "raw/qasper.jsonl"
check "CNN/DailyMail (article)"   "raw/cnn_dailymail.jsonl"
check "Landes todos"              "raw/landes_todos.jsonl"
check "bahmutov daily-logs"       "raw/bahmutov_dailylogs.jsonl"
check "synthetic notes"           "raw/synthetic_notes.jsonl"
check "chain connectives"         "raw/chain_connectives.jsonl"
check "IR gold (eu8 relevance)"   "eval/ir_gold/all.gold.json"
check "LOCOMO memory gold"        "eval/0.8.3-locomo-memory-gold.json"
check "memex-ELPS golden"         "external/memex-elps/memex_elps_golden.jsonl"
check "LongMemEval (xiaowu0162/longmemeval-cleaned)" ""
check "BEIR Touché-2020 (exploratory)" "raw/beir/touche2020/corpus.jsonl"
check "BEIR FiQA-2018 (dense wins)"    "raw/beir/fiqa/corpus.jsonl"
check "BEIR NFCorpus (BM25 wins)"      "raw/beir/nfcorpus/corpus.jsonl"
check "BEIR ArguAna (anti-example)"    "raw/beir/arguana/corpus.jsonl"

echo
echo "Done. For candidate-NEW corpora (MS MARCO, NQ, HotpotQA, 2Wiki, etc.)"
echo "see dev/corpus-survey/corpus-map.md and corpus-search-ledger.md."
