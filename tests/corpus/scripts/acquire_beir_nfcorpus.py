#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = [
#   "datasets>=3.0",
#   "pyarrow>=17",
# ]
# ///
"""Acquire NFCorpus (NutritionFacts.org corpus) from BEIR for FathomDB.

Source:   BeIR/nfcorpus on Hugging Face Hub (corpus + queries).
          BeIR/nfcorpus-qrels on Hugging Face Hub (relevance judgments).
Paper:    Boteva et al. 2016, "A Full-Text Learning to Rank Dataset for Medical
          Information Retrieval" (ECIR 2016).
License:  Free for academic research (informal; NutritionFacts.org /
          Heidelberg NLP data terms).  Non-academic commercial use requires
          contacting the data owner.  NOTE: HF BeIR wrapper displays
          "cc-by-sa-4.0" as a BLANKET LABEL — not authoritative per-dataset.

Role:     BM25 vs dense DISCRIMINATOR — "BM25 wins" leg.  BM25 nDCG@10 ≈32.5
          vs DPR ≈17.7, ANCE ≈23.7.  Specialist biomedical vocabulary causes
          dense models trained on general text (MSMARCO) to miss domain-specific
          terminology.  Pairs with FiQA ("dense wins" leg) as the recommended
          discriminator pair for FathomDB's external nDCG@10 reporting.
          Highest Rel D/Q (≈38.2) among small BEIR subsets — statistically
          stable estimator.

Corpus:   3,633 documents; 323 test queries; high D/Q ratio (38.2).
          Splits: train/dev/test queries + qrels.

Output layout (BEIR standard):
  data/corpus-data/raw/beir/nfcorpus/
    corpus.jsonl          — one doc per line: {"_id", "title", "text"}
    queries.jsonl         — one query per line: {"_id", "text"}
    qrels/
      train.tsv           — tab-sep: query-id \\t corpus-id \\t score
      validation.tsv
      test.tsv
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
CORPUS_ID = "BeIR/nfcorpus"
QRELS_ID = "BeIR/nfcorpus-qrels"

DATASET_REVISION = None  # pin to a commit SHA after first run

LICENSE_SPDX = "LicenseRef-NFCorpus-AcademicUse"
LICENSE_NOTES = (
    "Free for academic research (NutritionFacts.org / Heidelberg NLP terms, "
    "Boteva et al. 2016).  Non-academic commercial use: contact data owner.  "
    "HF BeIR wrapper label (cc-by-sa-4.0) is a blanket tag — not authoritative.  "
    "EVAL-ONLY, gitignored."
)

# ── paths ─────────────────────────────────────────────────────────────────────
OUT_DIR = corpus_data_dir() / "raw" / "beir" / "nfcorpus"
CORPUS_PATH = OUT_DIR / "corpus.jsonl"
QUERIES_PATH = OUT_DIR / "queries.jsonl"
MANIFEST_PATH = Path(__file__).resolve().parent / "manifest.json"

EXPECT_CORPUS = 3_633
EXPECT_QUERIES = 3_237   # total queries across all splits (train+validation+test)
QREL_SPLITS = ["train", "validation", "test"]


# ── helpers ───────────────────────────────────────────────────────────────────

def write_jsonl(path: Path, rows: list[dict]) -> str:
    path.parent.mkdir(parents=True, exist_ok=True)
    hasher = hashlib.sha256()
    with path.open("w", encoding="utf-8") as f:
        for row in rows:
            line = json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n"
            f.write(line)
            hasher.update(line.encode("utf-8"))
    return hasher.hexdigest()


def write_qrels_tsv(path: Path, rows: list[dict]) -> str:
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

    print("[acquire_beir_nfcorpus] loading corpus …")
    corpus_ds = load_dataset(
        CORPUS_ID, "corpus",
        split="corpus",
        revision=DATASET_REVISION,
        trust_remote_code=False,
    )
    corpus_rows = [
        {"_id": r["_id"], "title": r.get("title") or "", "text": r["text"]}
        for r in corpus_ds
    ]
    n_corpus = len(corpus_rows)
    print(f"[acquire_beir_nfcorpus] corpus: {n_corpus} documents")
    if n_corpus != EXPECT_CORPUS:
        print(f"[acquire_beir_nfcorpus] WARN: expected {EXPECT_CORPUS}, got {n_corpus}")

    print("[acquire_beir_nfcorpus] loading queries …")
    queries_ds = load_dataset(
        CORPUS_ID, "queries",
        split="queries",
        revision=DATASET_REVISION,
        trust_remote_code=False,
    )
    query_rows = [{"_id": r["_id"], "text": r["text"]} for r in queries_ds]
    n_queries = len(query_rows)
    print(f"[acquire_beir_nfcorpus] queries: {n_queries}")
    if n_queries != EXPECT_QUERIES:
        print(f"[acquire_beir_nfcorpus] WARN: expected {EXPECT_QUERIES}, got {n_queries}")

    # ── qrels ─────────────────────────────────────────────────────────────────
    qrel_counts: dict[str, int] = {}
    qrel_shas: dict[str, str] = {}
    for split in QREL_SPLITS:
        print(f"[acquire_beir_nfcorpus] loading qrels/{split} …")
        try:
            ds = load_dataset(
                QRELS_ID,
                split=split,
                revision=DATASET_REVISION,
                trust_remote_code=False,
            )
            rows = [
                {"query-id": str(r["query-id"]), "corpus-id": str(r["corpus-id"]), "score": int(r["score"])}
                for r in ds
            ]
        except Exception as exc:
            print(f"[acquire_beir_nfcorpus] WARN: qrels/{split} unavailable — {exc}")
            rows = []
        qrel_counts[split] = len(rows)
        if rows:
            sha = write_qrels_tsv(OUT_DIR / "qrels" / f"{split}.tsv", rows)
            qrel_shas[split] = sha
            print(f"[acquire_beir_nfcorpus] qrels/{split}: {len(rows)} judgments")

    # ── write corpus + queries ─────────────────────────────────────────────────
    corpus_sha = write_jsonl(CORPUS_PATH, corpus_rows)
    queries_sha = write_jsonl(QUERIES_PATH, query_rows)
    print(f"[acquire_beir_nfcorpus] wrote corpus  → {CORPUS_PATH}  (sha256={corpus_sha[:16]}…)")
    print(f"[acquire_beir_nfcorpus] wrote queries → {QUERIES_PATH}  (sha256={queries_sha[:16]}…)")

    # ── update manifest ────────────────────────────────────────────────────────
    manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
    existing = manifest.get("sources", {}).get("beir_nfcorpus", {})
    acquired_at = existing.get("acquired_at", "")
    if not acquired_at:
        import datetime
        acquired_at = datetime.date.today().isoformat()
    manifest.setdefault("sources", {})["beir_nfcorpus"] = {
        "script": "acquire_beir_nfcorpus.py",
        "upstream": {
            "kind": "huggingface_dataset",
            "corpus_id": CORPUS_ID,
            "qrels_id": QRELS_ID,
            "revision": DATASET_REVISION,
        },
        "license": LICENSE_SPDX,
        "license_notes": LICENSE_NOTES,
        "distribution": "cache",
        "output_dir": "data/corpus-data/raw/beir/nfcorpus/",
        "corpus_doc_count": n_corpus,
        "query_count": n_queries,
        "qrel_counts": qrel_counts,
        "corpus_sha256": corpus_sha,
        "queries_sha256": queries_sha,
        "qrels_sha256": qrel_shas,
        "acquired_at": acquired_at,
    }
    MANIFEST_PATH.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=True) + "\n", encoding="utf-8"
    )
    print("[acquire_beir_nfcorpus] updated manifest.json")
    print()
    print("[acquire_beir_nfcorpus] SUMMARY")
    print(f"  corpus docs : {n_corpus}")
    print(f"  queries     : {n_queries}")
    for split in QREL_SPLITS:
        print(f"  qrels/{split:5s} : {qrel_counts.get(split, 0)}")
    print(f"  output dir  : {OUT_DIR}")
    print()
    print("[acquire_beir_nfcorpus] DONE — payload is gitignored (EVAL-ONLY, academic use)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
