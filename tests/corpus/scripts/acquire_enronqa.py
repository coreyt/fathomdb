#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = [
#   "datasets>=3.0,<4.0",
#   "pyarrow>=17",
# ]
# ///
"""Acquire EnronQA email corpus.

Source:  HuggingFace MichaelR207/enron_qa_0922 (Ryan et al. 2025).
Pinned:  dataset revision c0b3a919..221e (2024-09-22).
License: not declared on the HF card; derived from the Enron base.
         Treated as cache-only per corpus-card.md until clarified.

Augments the Enron sent-folder corpus (acquire_enron.py) with full-
mailbox emails — EnronQA carries inbox + sent across multiple users
and ships per-email QA pairs as supervision. For Pack 1 we only need
the email *bodies*; QA pairs are deferred (they'll feed Corpus-Pack 2
ground-truth queries, not the doc corpus).

Determinism: HF parquet stream order is fixed for a pinned revision.
Take the first TARGET_COUNT rows from the train split.
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _corpus_lib import CorpusDoc, corpus_data_dir, doc_id, write_jsonl  # noqa: E402

DATASET_ID = "MichaelR207/enron_qa_0922"
DATASET_REVISION = "c0b3a9190fd970e83cfbe7d399a08860e43e221e"
TARGET_COUNT = 200
PROVENANCE = f"hf:{DATASET_ID}@{DATASET_REVISION[:8]}"
LICENSE_SPDX = "LicenseRef-EnronQA-Undeclared"


def build_doc(row: dict) -> CorpusDoc | None:
    body = (row.get("email") or "").strip()
    if not body:
        return None
    user = row.get("user") or "unknown"
    path = row.get("path") or "unknown"
    native_id = f"{user}/{path}"
    return CorpusDoc(
        doc_id=doc_id(PROVENANCE, native_id),
        source_type="email",
        title=None,
        body=body,
        created_at="2002-01-01T00:00:00+00:00",  # EnronQA strips per-message dates
        modified_at=None,
        author_or_sender=user,
        recipients=[],
        people_mentions=[],
        project_mentions=[],
        tags=["enronqa-user:" + user],
        url_or_external_id=f"enronqa:{native_id}",
        thread_id=None,
        parent_doc_id=None,
        license=LICENSE_SPDX,
        provenance=PROVENANCE,
    )


def main() -> int:
    from datasets import load_dataset  # type: ignore[import-not-found]

    out_path = corpus_data_dir() / "raw" / "enronqa.jsonl"

    print(f"streaming {DATASET_ID} (revision={DATASET_REVISION})")
    ds = load_dataset(
        DATASET_ID,
        split="train",
        streaming=True,
        revision=DATASET_REVISION,
    )

    def gen():
        emitted = 0
        seen_ids: set[str] = set()
        for row in ds:
            if emitted >= TARGET_COUNT:
                break
            d = build_doc(row)
            if d is None or d.doc_id in seen_ids:
                continue
            seen_ids.add(d.doc_id)
            emitted += 1
            yield d
            if emitted % 50 == 0:
                print(f"  {emitted}/{TARGET_COUNT}", flush=True)

    count, sha = write_jsonl(out_path, gen())
    print(f"wrote {count} docs to {out_path}")
    print(f"sha256 = {sha}")
    return 0 if count == TARGET_COUNT else 1


if __name__ == "__main__":
    raise SystemExit(main())
