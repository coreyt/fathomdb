#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = [
#   "datasets>=3.0",
#   "pyarrow>=17",
# ]
# ///
"""Acquire Touché-2020 (Webis-Touché-2020 Task 1) from BEIR for FathomDB.

Source:   BeIR/webis-touche2020 on Hugging Face Hub (corpus + queries).
          BeIR/webis-touche2020-qrels on Hugging Face Hub (relevance judgments).
Paper:    Bondarenko et al. 2020, "Overview of Touché 2020: Argument Retrieval"
          (CEUR Workshop Proceedings 2696, arXiv:2009.13211).
License:  CC BY 4.0 (Zenodo record 6862281 — the canonical dataset release).
          NOTE: The HF BeIR wrapper displays "cc-by-sa-4.0" as a BLANKET LABEL
          for all BeIR subsets; the per-dataset license is CC BY 4.0 from the
          Zenodo source.  Research/eval + redistribution: permitted with attribution.

Role:     Confirmed proxy for FathomDB's "discovery-in-k / exploratory" failure
          mode (dense median gold rank ~99).  BM25 nDCG@10=0.367 vs dense ≤0.27
          through 2024-era models (E5-large, BGE-large); gap confirmed structural
          (LNC2-axiom violation, not training-distribution artifact; SIGIR 2024
          arXiv:2407.07790).  Dense retrieves short non-argumentative fragments
          (avg <350w); BM25 retrieves full arguments (avg >600w) — the same
          length-mismatch mechanism FathomDB observes on exploratory discourse
          queries.  Do NOT use ArguAna as an exploratory proxy: dense BEATS BM25
          there (inverted pattern).

Corpus:   49 test queries (debate questions: "Should X?") over 382,545 arguments
          scraped from args.me.  Evaluation split: test (49 queries, qrels).

Output layout (BEIR standard):
  data/corpus-data/raw/beir/touche2020/
    corpus.jsonl          — one doc per line: {"_id", "title", "text"}
    queries.jsonl         — one query per line: {"_id", "text"}
    qrels/test.tsv        — tab-sep header: query-id \\t corpus-id \\t score
"""

from __future__ import annotations

import csv
import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _corpus_lib import corpus_data_dir  # noqa: E402

# ── dataset coordinates ────────────────────────────────────────────────────────
CORPUS_ID = "BeIR/webis-touche2020"
CORPUS_CONFIG = "corpus"
CORPUS_SPLIT = "corpus"

QUERIES_ID = "BeIR/webis-touche2020"
QUERIES_CONFIG = "queries"
QUERIES_SPLIT = "queries"

QRELS_ID = "BeIR/webis-touche2020-qrels"
QRELS_SPLIT = "test"

DATASET_REVISION = None  # pin to a commit SHA after first run (printed below)

LICENSE_SPDX = "CC-BY-4.0"
LICENSE_NOTES = (
    "CC BY 4.0 (Zenodo 6862281, Bondarenko et al. 2020).  "
    "HF BeIR wrapper label (cc-by-sa-4.0) is a blanket tag — not authoritative.  "
    "Research/eval + redistribution: permitted with attribution."
)

# ── paths ─────────────────────────────────────────────────────────────────────
OUT_DIR = corpus_data_dir() / "raw" / "beir" / "touche2020"
CORPUS_PATH = OUT_DIR / "corpus.jsonl"
QUERIES_PATH = OUT_DIR / "queries.jsonl"
QRELS_PATH = OUT_DIR / "qrels" / "test.tsv"
MANIFEST_PATH = Path(__file__).resolve().parent / "manifest.json"

# ── expected sizes (warn if upstream changes) ──────────────────────────────────
EXPECT_CORPUS = 382_545
EXPECT_QUERIES = 49
EXPECT_QRELS = 2_858   # total relevance judgments in test split


# ── helpers ───────────────────────────────────────────────────────────────────

def write_jsonl(path: Path, rows: list[dict]) -> str:
    """Write rows as JSONL (sort_keys for determinism); return sha256."""
    path.parent.mkdir(parents=True, exist_ok=True)
    hasher = hashlib.sha256()
    with path.open("w", encoding="utf-8") as f:
        for row in rows:
            line = json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n"
            f.write(line)
            hasher.update(line.encode("utf-8"))
    return hasher.hexdigest()


def write_qrels_tsv(path: Path, rows: list[dict]) -> str:
    """Write qrels as TSV (BEIR standard: query-id, corpus-id, score); return sha256."""
    path.parent.mkdir(parents=True, exist_ok=True)
    hasher = hashlib.sha256()
    with path.open("w", encoding="utf-8", newline="") as f:
        writer = csv.writer(f, delimiter="\t")
        writer.writerow(["query-id", "corpus-id", "score"])
        for row in rows:
            line_str = f"{row['query-id']}\t{row['corpus-id']}\t{row['score']}\n"
            hasher.update(line_str.encode("utf-8"))
            writer.writerow([row["query-id"], row["corpus-id"], row["score"]])
    return hasher.hexdigest()


# ── main ──────────────────────────────────────────────────────────────────────

def main() -> int:
    from datasets import load_dataset  # type: ignore[import-not-found]

    print("[acquire_beir_touche2020] loading corpus …")
    corpus_ds = load_dataset(
        CORPUS_ID, CORPUS_CONFIG,
        split=CORPUS_SPLIT,
        revision=DATASET_REVISION,
        trust_remote_code=False,
    )
    corpus_rows = [
        {"_id": r["_id"], "title": r.get("title") or "", "text": r["text"]}
        for r in corpus_ds
    ]
    n_corpus = len(corpus_rows)
    print(f"[acquire_beir_touche2020] corpus: {n_corpus} documents")
    if n_corpus != EXPECT_CORPUS:
        print(f"[acquire_beir_touche2020] WARN: expected {EXPECT_CORPUS}, got {n_corpus} — upstream may have changed")

    print("[acquire_beir_touche2020] loading queries …")
    queries_ds = load_dataset(
        QUERIES_ID, QUERIES_CONFIG,
        split=QUERIES_SPLIT,
        revision=DATASET_REVISION,
        trust_remote_code=False,
    )
    query_rows = [{"_id": r["_id"], "text": r["text"]} for r in queries_ds]
    n_queries = len(query_rows)
    print(f"[acquire_beir_touche2020] queries: {n_queries}")
    if n_queries != EXPECT_QUERIES:
        print(f"[acquire_beir_touche2020] WARN: expected {EXPECT_QUERIES}, got {n_queries}")

    print("[acquire_beir_touche2020] loading qrels …")
    qrels_ds = load_dataset(
        QRELS_ID,
        split=QRELS_SPLIT,
        revision=DATASET_REVISION,
        trust_remote_code=False,
    )
    qrel_rows = [
        {"query-id": str(r["query-id"]), "corpus-id": str(r["corpus-id"]), "score": int(r["score"])}
        for r in qrels_ds
    ]
    n_qrels = len(qrel_rows)
    print(f"[acquire_beir_touche2020] qrels (test): {n_qrels}")
    if n_qrels != EXPECT_QRELS:
        print(f"[acquire_beir_touche2020] WARN: expected {EXPECT_QRELS}, got {n_qrels}")

    # ── write ──────────────────────────────────────────────────────────────────
    corpus_sha = write_jsonl(CORPUS_PATH, corpus_rows)
    queries_sha = write_jsonl(QUERIES_PATH, query_rows)
    qrels_sha = write_qrels_tsv(QRELS_PATH, qrel_rows)

    print(f"[acquire_beir_touche2020] wrote corpus  → {CORPUS_PATH}  (sha256={corpus_sha[:16]}…)")
    print(f"[acquire_beir_touche2020] wrote queries → {QUERIES_PATH}  (sha256={queries_sha[:16]}…)")
    print(f"[acquire_beir_touche2020] wrote qrels   → {QRELS_PATH}   (sha256={qrels_sha[:16]}…)")

    # ── update manifest ────────────────────────────────────────────────────────
    manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
    existing = manifest.get("sources", {}).get("beir_touche2020", {})
    acquired_at = existing.get("acquired_at", "")
    if not acquired_at:
        import datetime
        acquired_at = datetime.date.today().isoformat()
    manifest.setdefault("sources", {})["beir_touche2020"] = {
        "script": "acquire_beir_touche2020.py",
        "upstream": {
            "kind": "huggingface_dataset",
            "corpus_id": CORPUS_ID,
            "qrels_id": QRELS_ID,
            "revision": DATASET_REVISION,
        },
        "license": LICENSE_SPDX,
        "license_notes": LICENSE_NOTES,
        "distribution": "cache",
        "output_dir": "data/corpus-data/raw/beir/touche2020/",
        "corpus_doc_count": n_corpus,
        "query_count": n_queries,
        "qrel_count": n_qrels,
        "corpus_sha256": corpus_sha,
        "queries_sha256": queries_sha,
        "qrels_sha256": qrels_sha,
        "acquired_at": acquired_at,
    }
    MANIFEST_PATH.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=True) + "\n", encoding="utf-8"
    )
    print("[acquire_beir_touche2020] updated manifest.json")
    print()
    print("[acquire_beir_touche2020] SUMMARY")
    print(f"  corpus docs : {n_corpus}")
    print(f"  queries     : {n_queries}")
    print(f"  qrels(test) : {n_qrels}")
    print(f"  output dir  : {OUT_DIR}")
    print()
    print("[acquire_beir_touche2020] DONE — payload is gitignored (EVAL-ONLY)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
