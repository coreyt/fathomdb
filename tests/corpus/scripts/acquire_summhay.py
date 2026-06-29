#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = [
#   "huggingface_hub>=0.23",
#   "pyarrow>=17",
# ]
# ///
"""Acquire SummHay (Summary of a Haystack) from Hugging Face for FathomDB.

Source:   Salesforce/summary-of-a-haystack on Hugging Face Hub
          (single file: summhay.parquet).
          GH mirror: salesforce/summary-of-a-haystack.
Paper:    Laban et al. 2024, "Summary of a Haystack: A Challenge to Long-Context
          LLMs and RAG Systems", EMNLP 2024 (arXiv:2407.01370).
License:  Apache-2.0 (redistributable with attribution).  Project default is
          still "scripts in git, data out of git" — the payload is written
          under data/corpus-data/ (gitignored).

Role:     The closest public analog to FathomDB's global-sensemaking / "what's
          been happening across everything" shape (the GraphRAG global-QFS
          shape).  Each topic is a ~100-document haystack (~100k tokens); the
          task is a query-focused bulleted summary that covers predefined
          "insights" with per-bullet source citations, scored on Coverage +
          Citation -> Joint Score.

Gold:     RICH.  Each of the 10 haystacks ships:
            - subtopics[].insights        reference insights (the coverage gold)
            - subtopics[].query           the focusing query
            - subtopics[].eval_summaries  100s of human + model candidate
                                          summaries with per-bullet coverage
                                          labels (NO/PARTIAL/FULL) + insight_id
                                          associations and citation mappings
            - documents[]                 the ~100-doc haystack with the
                                          insight_id each document supports
                                          (the citation gold)

Output layout:
  data/corpus-data/raw/summhay/
    summhay.jsonl   — one haystack (topic) per line, full nested structure
                      preserved (topic, topic_metadata, subtopics, documents).
"""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _corpus_lib import corpus_data_dir  # noqa: E402

# ── dataset coordinates ────────────────────────────────────────────────────────
REPO_ID = "Salesforce/summary-of-a-haystack"
PARQUET_FILE = "summhay.parquet"
DATASET_REVISION = None  # pin to a commit SHA after first run

LICENSE_SPDX = "Apache-2.0"
LICENSE_NOTES = (
    "Apache-2.0 (Laban et al. 2024, SummHay, EMNLP 2024, arXiv:2407.01370).  "
    "Redistribution permitted with attribution.  Project default keeps the "
    "payload out of git (gitignored)."
)

# ── paths ─────────────────────────────────────────────────────────────────────
OUT_DIR = corpus_data_dir() / "raw" / "summhay"
OUT_PATH = OUT_DIR / "summhay.jsonl"
MANIFEST_PATH = Path(__file__).resolve().parent / "manifest.json"

EXPECT_TOPICS = 10


def write_jsonl(path: Path, rows: list[dict]) -> str:
    path.parent.mkdir(parents=True, exist_ok=True)
    hasher = hashlib.sha256()
    with path.open("w", encoding="utf-8") as f:
        for row in rows:
            line = json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n"
            f.write(line)
            hasher.update(line.encode("utf-8"))
    return hasher.hexdigest()


def main() -> int:
    from huggingface_hub import hf_hub_download  # type: ignore[import-not-found]
    import pyarrow.parquet as pq  # type: ignore[import-not-found]

    print(f"[acquire_summhay] downloading {REPO_ID}/{PARQUET_FILE} …")
    local = hf_hub_download(
        REPO_ID, PARQUET_FILE, repo_type="dataset", revision=DATASET_REVISION
    )
    table = pq.read_table(local)
    rows = table.to_pylist()
    n_topics = len(rows)
    print(f"[acquire_summhay] haystacks (topics): {n_topics}")
    if n_topics != EXPECT_TOPICS:
        print(f"[acquire_summhay] WARN: expected {EXPECT_TOPICS}, got {n_topics}")

    n_subtopics = sum(len(r.get("subtopics") or []) for r in rows)
    n_documents = sum(len(r.get("documents") or []) for r in rows)
    print(f"[acquire_summhay] subtopics (queries): {n_subtopics}")
    print(f"[acquire_summhay] documents (haystack docs): {n_documents}")

    sha = write_jsonl(OUT_PATH, rows)
    print(f"[acquire_summhay] wrote {OUT_PATH}  (sha256={sha[:16]}…)")

    # ── update manifest ────────────────────────────────────────────────────────
    manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
    existing = manifest.get("sources", {}).get("summhay", {})
    acquired_at = existing.get("acquired_at", "")
    if not acquired_at:
        import datetime
        acquired_at = datetime.date.today().isoformat()
    manifest.setdefault("sources", {})["summhay"] = {
        "script": "acquire_summhay.py",
        "upstream": {
            "kind": "huggingface_dataset_parquet_file",
            "id": REPO_ID,
            "file": PARQUET_FILE,
            "revision": DATASET_REVISION,
        },
        "license": LICENSE_SPDX,
        "license_notes": LICENSE_NOTES,
        "distribution": "cache",
        "output": "data/corpus-data/raw/summhay/summhay.jsonl",
        "topic_count": n_topics,
        "subtopic_count": n_subtopics,
        "document_count": n_documents,
        "sha256": sha,
        "acquired_at": acquired_at,
        "role_note": (
            "Global-sensemaking / query-focused-summarization (QFS) proxy — "
            "the closest public analog to FathomDB's across-everything shape.  "
            "Coverage + Citation joint-score gold."
        ),
    }
    MANIFEST_PATH.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=True) + "\n", encoding="utf-8"
    )
    print("[acquire_summhay] updated manifest.json")
    print()
    print("[acquire_summhay] SUMMARY")
    print(f"  haystacks  : {n_topics}")
    print(f"  subtopics  : {n_subtopics}")
    print(f"  documents  : {n_documents}")
    print(f"  output     : {OUT_PATH}")
    print("[acquire_summhay] DONE — payload is gitignored")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
