#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = [
#   "datasets>=3.0",
#   "pyarrow>=17",
# ]
# ///
"""Acquire ArguAna from BEIR for FathomDB.

Source:   BeIR/arguana on Hugging Face Hub (corpus + queries).
          BeIR/arguana-qrels on Hugging Face Hub (relevance judgments).
Paper:    Wachsmuth et al. 2018, "Retrieval of the Best Counterargument without
          Prior Topic Knowledge" (ACL 2018).
License:  CC BY 4.0 (Wachsmuth et al. 2018, idebate.org scrape).
          NOTE: HF BeIR wrapper displays "cc-by-sa-4.0" as a BLANKET LABEL —
          the canonical per-dataset license is CC BY 4.0.

Role:     ANTI-EXAMPLE / low-priority.  Dense BEATS BM25 here (TAS-B
          nDCG@10=0.429 vs BM25=0.315) because counter-argument retrieval
          rewards semantic opposition over lexical match — the INVERSE of
          FathomDB's exploratory failure mode.  Acquired for completeness and
          to support potential semantic-opposition task research, NOT as an
          exploratory proxy.  Use Touché-2020 for exploratory retrieval proxy.

Corpus:   8,674 idebate.org arguments; queries ARE the corpus (each argument
          is both query and potential answer for a different query).  Evaluation
          split: test (each query has exactly 1 relevant document).

Output layout (BEIR standard):
  data/corpus-data/raw/beir/arguana/
    corpus.jsonl          — one doc per line: {"_id", "title", "text"}
    queries.jsonl         — one query per line: {"_id", "text"}
    qrels/test.tsv        — tab-sep: query-id \\t corpus-id \\t score
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
CORPUS_ID = "BeIR/arguana"
QRELS_ID = "BeIR/arguana-qrels"

DATASET_REVISION = None  # pin to a commit SHA after first run

LICENSE_SPDX = "CC-BY-4.0"
LICENSE_NOTES = (
    "CC BY 4.0 (Wachsmuth et al. 2018, idebate.org).  "
    "HF BeIR wrapper label (cc-by-sa-4.0) is a blanket tag — not authoritative.  "
    "Research/eval + redistribution: permitted with attribution."
)

# ── paths ─────────────────────────────────────────────────────────────────────
OUT_DIR = corpus_data_dir() / "raw" / "beir" / "arguana"
CORPUS_PATH = OUT_DIR / "corpus.jsonl"
QUERIES_PATH = OUT_DIR / "queries.jsonl"
MANIFEST_PATH = Path(__file__).resolve().parent / "manifest.json"

EXPECT_CORPUS = 8_674
EXPECT_QUERIES = 1_406  # test queries (subset of corpus used as queries)


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

    print("[acquire_beir_arguana] loading corpus …")
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
    print(f"[acquire_beir_arguana] corpus: {n_corpus} documents")
    if n_corpus != EXPECT_CORPUS:
        print(f"[acquire_beir_arguana] WARN: expected {EXPECT_CORPUS}, got {n_corpus}")

    print("[acquire_beir_arguana] loading queries …")
    queries_ds = load_dataset(
        CORPUS_ID, "queries",
        split="queries",
        revision=DATASET_REVISION,
        trust_remote_code=False,
    )
    query_rows = [{"_id": r["_id"], "text": r["text"]} for r in queries_ds]
    n_queries = len(query_rows)
    print(f"[acquire_beir_arguana] queries: {n_queries}")
    if n_queries != EXPECT_QUERIES:
        print(f"[acquire_beir_arguana] WARN: expected {EXPECT_QUERIES}, got {n_queries}")

    print("[acquire_beir_arguana] loading qrels/test …")
    qrels_ds = load_dataset(
        QRELS_ID,
        split="test",
        revision=DATASET_REVISION,
        trust_remote_code=False,
    )
    qrel_rows = [
        {"query-id": str(r["query-id"]), "corpus-id": str(r["corpus-id"]), "score": int(r["score"])}
        for r in qrels_ds
    ]
    n_qrels = len(qrel_rows)
    print(f"[acquire_beir_arguana] qrels (test): {n_qrels}")

    # ── write ──────────────────────────────────────────────────────────────────
    corpus_sha = write_jsonl(CORPUS_PATH, corpus_rows)
    queries_sha = write_jsonl(QUERIES_PATH, query_rows)
    qrels_sha = write_qrels_tsv(OUT_DIR / "qrels" / "test.tsv", qrel_rows)

    print(f"[acquire_beir_arguana] wrote corpus  → {CORPUS_PATH}  (sha256={corpus_sha[:16]}…)")
    print(f"[acquire_beir_arguana] wrote queries → {QUERIES_PATH}  (sha256={queries_sha[:16]}…)")
    print(f"[acquire_beir_arguana] wrote qrels   → {OUT_DIR}/qrels/test.tsv  (sha256={qrels_sha[:16]}…)")

    # ── update manifest ────────────────────────────────────────────────────────
    manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
    existing = manifest.get("sources", {}).get("beir_arguana", {})
    acquired_at = existing.get("acquired_at", "")
    if not acquired_at:
        import datetime
        acquired_at = datetime.date.today().isoformat()
    manifest.setdefault("sources", {})["beir_arguana"] = {
        "script": "acquire_beir_arguana.py",
        "upstream": {
            "kind": "huggingface_dataset",
            "corpus_id": CORPUS_ID,
            "qrels_id": QRELS_ID,
            "revision": DATASET_REVISION,
        },
        "license": LICENSE_SPDX,
        "license_notes": LICENSE_NOTES,
        "distribution": "cache",
        "output_dir": "data/corpus-data/raw/beir/arguana/",
        "corpus_doc_count": n_corpus,
        "query_count": n_queries,
        "qrel_count": n_qrels,
        "corpus_sha256": corpus_sha,
        "queries_sha256": queries_sha,
        "qrels_sha256": qrels_sha,
        "acquired_at": acquired_at,
        "role_note": (
            "ANTI-EXAMPLE: dense beats BM25 here (inverted vs FathomDB exploratory "
            "failure mode).  Low-priority.  Use Touché-2020 for the exploratory proxy."
        ),
    }
    MANIFEST_PATH.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=True) + "\n", encoding="utf-8"
    )
    print("[acquire_beir_arguana] updated manifest.json")
    print()
    print("[acquire_beir_arguana] SUMMARY")
    print(f"  corpus docs : {n_corpus}")
    print(f"  queries     : {n_queries}")
    print(f"  qrels(test) : {n_qrels}")
    print(f"  output dir  : {OUT_DIR}")
    print()
    print("[acquire_beir_arguana] DONE — payload is gitignored (EVAL-ONLY)")
    print("[acquire_beir_arguana] NOTE: this is an ANTI-EXAMPLE (dense beats BM25);")
    print("  use Touché-2020 (acquire_beir_touche2020.py) for the exploratory proxy.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
