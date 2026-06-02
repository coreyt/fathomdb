#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = [
#   "datasets>=3.0,<4.0",
#   "pyarrow>=17",
# ]
# ///
"""Acquire EnronQA email docs and eval-only QA rows.

Source:  HuggingFace MichaelR207/enron_qa_0922 (Ryan et al. 2025).
Pinned:  dataset revision c0b3a919..221e (2024-09-22).
License: not declared on the HF card; derived from the Enron base.
         Treated as cache-only per corpus-card.md until clarified.

The corpus document output remains the existing 200 selected emails.
The upstream question/answer fields are now exported separately to
`data/corpus-data/eval/enronqa_qa.jsonl`. Eval rows are marked grounded
only when their email maps to one of the emitted corpus docs.

Determinism: HF parquet order is fixed for a pinned revision.
Take the first TARGET_COUNT rows from the train split.
"""

from __future__ import annotations

import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _corpus_lib import (  # noqa: E402
    CorpusDoc,
    EvalQaRow,
    EvidenceSpan,
    corpus_data_dir,
    doc_id,
    qa_id,
    write_eval_jsonl,
    write_jsonl,
)

DATASET_ID = "MichaelR207/enron_qa_0922"
DATASET_REVISION = "c0b3a9190fd970e83cfbe7d399a08860e43e221e"
TARGET_COUNT = 200
PROVENANCE = f"hf:{DATASET_ID}@{DATASET_REVISION[:8]}"
LICENSE_SPDX = "LicenseRef-EnronQA-Undeclared"


def native_id(row: dict[str, Any]) -> str:
    user = row.get("user") or "unknown"
    path = row.get("path") or "unknown"
    return f"{user}/{path}"


def build_doc(row: dict[str, Any]) -> CorpusDoc | None:
    body = (row.get("email") or "").strip()
    if not body:
        return None
    user = row.get("user") or "unknown"
    nid = native_id(row)
    return CorpusDoc(
        doc_id=doc_id(PROVENANCE, nid),
        source_type="email",
        title=None,
        body=body,
        created_at="2002-01-01T00:00:00+00:00",
        modified_at=None,
        author_or_sender=user,
        recipients=[],
        people_mentions=[],
        project_mentions=[],
        tags=["enronqa-user:" + user],
        url_or_external_id=f"enronqa:{nid}",
        thread_id=None,
        parent_doc_id=None,
        license=LICENSE_SPDX,
        provenance=PROVENANCE,
    )


def _list_value(row: dict[str, Any], key: str) -> list[Any]:
    value = row.get(key) or []
    if isinstance(value, list):
        return value
    return [value]


def _answer_texts(row: dict[str, Any], index: int) -> list[str]:
    out: list[str] = []
    for key in ("gold_answers", "alternate_answers"):
        values = _list_value(row, key)
        if index < len(values):
            value = values[index]
            if isinstance(value, list):
                out.extend(str(v).strip() for v in value if str(v).strip())
            elif str(value).strip():
                out.append(str(value).strip())
    return sorted(set(out))


def _evidence_spans(row: dict[str, Any], index: int, doc: CorpusDoc) -> list[EvidenceSpan]:
    spans: list[EvidenceSpan] = []
    for key in ("gold_rationales", "alternate_rationales"):
        values = _list_value(row, key)
        if index >= len(values):
            continue
        rationales = values[index] if isinstance(values[index], list) else [values[index]]
        for rationale in rationales:
            text = str(rationale).strip()
            start = doc.body.find(text)
            if text and start >= 0:
                spans.append(EvidenceSpan(doc_id=doc.doc_id, start=start, end=start + len(text), text=text))
    return spans


def build_qa_rows(row: dict[str, Any], doc: CorpusDoc) -> list[EvalQaRow]:
    questions = [str(q).strip() for q in _list_value(row, "questions") if str(q).strip()]
    rephrased = [str(q).strip() for q in _list_value(row, "rephrased_questions") if str(q).strip()]
    out: list[EvalQaRow] = []
    for index, question in enumerate(questions):
        answers = _answer_texts(row, index)
        if not answers:
            continue
        upstream = f"{native_id(row)}:{index}"
        out.append(EvalQaRow(
            qa_id=qa_id("enronqa", native_id(row), str(index)),
            source="enronqa",
            source_type="email",
            question=question,
            answers=answers,
            answer_type="span",
            evidence_doc_ids=[doc.doc_id],
            evidence_spans=_evidence_spans(row, index, doc),
            negative_doc_ids=[],
            relation_type="mentions",
            metadata={
                "split": "train",
                "user_id": row.get("user"),
                "upstream_id": upstream,
                "path": row.get("path"),
                "rephrased_question": rephrased[index] if index < len(rephrased) else None,
                "incorrect_answers": _list_value(row, "incorrect_answers"),
                "include_email": row.get("include_email"),
                "questions_count": row.get("questions_count"),
            },
            license=LICENSE_SPDX,
            provenance=PROVENANCE,
        ))
    return out


def main() -> int:
    from datasets import load_dataset  # type: ignore[import-not-found]

    raw_path = corpus_data_dir() / "raw" / "enronqa.jsonl"
    eval_path = corpus_data_dir() / "eval" / "enronqa_qa.jsonl"

    print(f"loading {DATASET_ID} (revision={DATASET_REVISION})")
    ds = load_dataset(DATASET_ID, split="train", revision=DATASET_REVISION)

    docs: list[CorpusDoc] = []
    qa_rows: list[EvalQaRow] = []
    seen_ids: set[str] = set()
    for row in ds:
        if len(docs) >= TARGET_COUNT:
            break
        d = build_doc(row)
        if d is None or d.doc_id in seen_ids:
            continue
        seen_ids.add(d.doc_id)
        docs.append(d)
        qa_rows.extend(build_qa_rows(row, d))
        if len(docs) % 50 == 0:
            print(f"  {len(docs)}/{TARGET_COUNT}", flush=True)

    count, sha = write_jsonl(raw_path, docs)
    qa_count, qa_sha = write_eval_jsonl(eval_path, qa_rows)
    print(f"wrote {count} docs to {raw_path}")
    print(f"sha256 = {sha}")
    print(f"wrote {qa_count} eval QA rows to {eval_path}")
    print(f"eval sha256 = {qa_sha}")
    return 0 if count == TARGET_COUNT else 1


if __name__ == "__main__":
    raise SystemExit(main())
