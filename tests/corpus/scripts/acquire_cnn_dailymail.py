#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = [
#   "datasets>=3.0,<4.0",
#   "pyarrow>=17",
# ]
# ///
"""Acquire CNN/DailyMail articles into the canonical corpus JSONL.

Source:  HuggingFace abisee/cnn_dailymail, config 3.0.0.
License: Apache-2.0 (the prepared dataset). Commit OK per corpus-card.md.

The HF distribution strips per-article dates and URLs, so we synthesize
created_at deterministically across the publication window
(2007-04-01 .. 2015-04-01) and mark provenance accordingly.

Determinism:
  - Dataset revision pinned via DATASET_REVISION below.
  - First TARGET_COUNT rows from the train split, in HF stream order.
  - created_at is a deterministic function of the row's HF id.

Usage:
    uv run tests/corpus/scripts/acquire_cnn_dailymail.py
"""

from __future__ import annotations

import hashlib
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

# Make _corpus_lib importable when running via `uv run --script`.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from _corpus_lib import CorpusDoc, corpus_dir, doc_id, write_jsonl  # noqa: E402

DATASET_ID = "abisee/cnn_dailymail"
DATASET_CONFIG = "3.0.0"
# Pin to a specific dataset revision (HF commit SHA) for reproducibility.
# Resolved + recorded by acquire run; first-run output prints the resolved SHA.
DATASET_REVISION = "96df5e686bee6baa90b8bee7c28b81fa3fa6223d"
TARGET_COUNT = 2500
PROVENANCE = f"hf:{DATASET_ID}@{DATASET_CONFIG}"
LICENSE_SPDX = "Apache-2.0"

# Publication window for the original CNN/DM scrape — used to synthesize
# created_at values uniformly. Documented in corpus-card.md §"Provenance".
WINDOW_START = datetime(2007, 4, 1, tzinfo=timezone.utc)
WINDOW_END = datetime(2015, 4, 1, tzinfo=timezone.utc)
WINDOW_SECONDS = int((WINDOW_END - WINDOW_START).total_seconds())


def synthesize_created_at(hf_id: str) -> str:
    """Deterministic created_at from HF id (hash → seconds offset)."""
    h = hashlib.sha256(hf_id.encode("utf-8")).digest()
    offset = int.from_bytes(h[:8], "big") % WINDOW_SECONDS
    return (WINDOW_START + timedelta(seconds=offset)).isoformat()


def build_doc(row: dict) -> CorpusDoc:
    hf_id = row["id"]
    article = row["article"]
    highlights = row["highlights"]
    return CorpusDoc(
        doc_id=doc_id(PROVENANCE, hf_id),
        source_type="article",
        title=None,                       # HF distribution has no title field
        body=article,
        created_at=synthesize_created_at(hf_id),
        modified_at=None,
        author_or_sender=None,
        recipients=[],
        people_mentions=[],               # filled by a later NER pass
        project_mentions=[],
        tags=["news", "highlights:" + str(len(highlights.split("\n")))],
        url_or_external_id=hf_id,
        thread_id=None,
        parent_doc_id=None,
        license=LICENSE_SPDX,
        provenance=PROVENANCE + "+synthetic-date",
    )


def main() -> int:
    from datasets import load_dataset  # type: ignore[import-not-found]

    out_path = corpus_dir() / "raw" / "cnn_dailymail.jsonl"

    print(f"streaming {DATASET_ID} ({DATASET_CONFIG}, revision={DATASET_REVISION})")
    ds = load_dataset(
        DATASET_ID,
        DATASET_CONFIG,
        split="train",
        streaming=True,
        revision=DATASET_REVISION,
    )

    def gen():
        taken = 0
        for row in ds:
            if taken >= TARGET_COUNT:
                break
            yield build_doc(row)
            taken += 1
            if taken % 250 == 0:
                print(f"  {taken}/{TARGET_COUNT}", flush=True)

    count, sha = write_jsonl(out_path, gen())
    if count != TARGET_COUNT:
        print(f"ERROR: wanted {TARGET_COUNT} docs, got {count}", file=sys.stderr)
        return 1
    print(f"wrote {count} docs to {out_path}")
    print(f"sha256 = {sha}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
