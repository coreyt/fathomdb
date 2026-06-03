#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Acquire QASPER papers and eval-only QA rows.

Source:  HuggingFace allenai/qasper.
Pinned:  dataset revision fdc9d8214fbab5dd782958601db4d678e6934a54.
License: CC-BY-4.0 per the Hugging Face dataset card.

QASPER fills the corpus's previously-empty `paper` source_type. Each
paper is emitted as one canonical document containing title, abstract,
and section text. The original question/answer/evidence annotations are
written separately to data/corpus-data/eval/qasper_qa.jsonl and are not
ingested as corpus documents.

Determinism: split order is train, validation, test; rows are sorted by
paper id within each split. Include all papers unless the upstream grows
past TARGET_DOCS, in which case the sorted stream is capped.
"""

from __future__ import annotations

import io
import json
import sys
import tarfile
import urllib.request
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

DATASET_ID = "allenai/qasper"
DATASET_REVISION = "fdc9d8214fbab5dd782958601db4d678e6934a54"
TRAIN_DEV_URL = "https://qasper-dataset.s3.us-west-2.amazonaws.com/qasper-train-dev-v0.3.tgz"
TEST_URL = "https://qasper-dataset.s3.us-west-2.amazonaws.com/qasper-test-and-evaluator-v0.3.tgz"
PROVENANCE = f"hf:{DATASET_ID}@{DATASET_REVISION[:8]}"
LICENSE_SPDX = "CC-BY-4.0"
TARGET_DOCS = 1585
SPLITS = ("train", "validation", "test")



def _fetch(url: str) -> bytes:
    print(f"fetching {url}", flush=True)
    with urllib.request.urlopen(url) as resp:
        return resp.read()


def _read_json_from_tgz(archive: bytes, filename: str) -> dict[str, Any]:
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz") as tf:
        for member in tf:
            if not member.isfile() or not member.name.endswith(filename):
                continue
            f = tf.extractfile(member)
            if f is None:
                break
            return json.loads(f.read().decode("utf-8"))
    raise FileNotFoundError(filename)


def iter_rows() -> list[tuple[str, dict[str, Any]]]:
    train_dev = _fetch(TRAIN_DEV_URL)
    test = _fetch(TEST_URL)
    files = {
        "train": _read_json_from_tgz(train_dev, "qasper-train-v0.3.json"),
        "validation": _read_json_from_tgz(train_dev, "qasper-dev-v0.3.json"),
        "test": _read_json_from_tgz(test, "qasper-test-v0.3.json"),
    }
    rows: list[tuple[str, dict[str, Any]]] = []
    for split in SPLITS:
        for pid in sorted(files[split]):
            row = dict(files[split][pid])
            row["id"] = pid
            rows.append((split, row))
    return rows

def _first(row: dict[str, Any], *keys: str, default: str = "") -> str:
    for key in keys:
        value = row.get(key)
        if value is not None:
            return str(value)
    return default


def _paper_id(row: dict[str, Any]) -> str:
    return _first(row, "id", "paper_id", "arxiv_id", default=_first(row, "title", default="unknown"))


def _section_pairs(full_text: Any) -> list[tuple[str, str]]:
    if not isinstance(full_text, dict):
        return []
    names = full_text.get("section_name") or []
    paragraphs = full_text.get("paragraphs") or []
    pairs: list[tuple[str, str]] = []
    for name, paras in zip(names, paragraphs):
        if isinstance(paras, list):
            text = "\n".join(str(p).strip() for p in paras if str(p).strip())
        else:
            text = str(paras).strip()
        if text:
            pairs.append((str(name).strip() or "Section", text))
    return pairs


def build_doc(row: dict[str, Any], split: str) -> CorpusDoc | None:
    pid = _paper_id(row)
    title = _first(row, "title", default="Untitled paper").strip()
    abstract = _first(row, "abstract", default="").strip()
    body_parts = [f"# {title}"]
    if abstract:
        body_parts.extend(["", "## Abstract", abstract])
    for section, text in _section_pairs(row.get("full_text")):
        body_parts.extend(["", f"## {section}", text])
    body = "\n".join(body_parts).strip()
    if not body:
        return None
    authors = row.get("authors") or []
    if isinstance(authors, str):
        people = [authors]
    else:
        people = [str(a) for a in authors if str(a).strip()]
    return CorpusDoc(
        doc_id=doc_id(PROVENANCE, pid),
        source_type="paper",
        title=title,
        body=body,
        created_at="2021-05-07T00:00:00+00:00",
        modified_at=None,
        author_or_sender=", ".join(people[:3]) if people else None,
        recipients=[],
        people_mentions=people,
        project_mentions=["nlp"],
        tags=["qasper", "qasper-split:" + split],
        url_or_external_id=f"qasper:{pid}",
        thread_id=pid,
        parent_doc_id=None,
        license=LICENSE_SPDX,
        provenance=PROVENANCE,
    )


def _qa_items(row: dict[str, Any]) -> list[dict[str, Any]]:
    qas = row.get("qas") or row.get("qa") or row.get("questions") or []
    if isinstance(qas, list):
        return [q for q in qas if isinstance(q, dict)]
    if not isinstance(qas, dict):
        return []
    questions = qas.get("question") or []
    question_ids = qas.get("question_id") or qas.get("id") or list(range(len(questions)))
    answers = qas.get("answers") or []
    out: list[dict[str, Any]] = []
    for i, question in enumerate(questions):
        out.append({
            "question": question,
            "question_id": question_ids[i] if i < len(question_ids) else str(i),
            "answers": answers[i] if i < len(answers) else [],
        })
    return out


def _answer_records(value: Any) -> list[dict[str, Any]]:
    if isinstance(value, list):
        return [v for v in value if isinstance(v, dict)]
    if isinstance(value, dict):
        nested = value.get("answer") or value.get("answers")
        if isinstance(nested, list):
            return [v for v in nested if isinstance(v, dict)]
        return [value]
    return []


def _answer_text_and_type(answer: dict[str, Any]) -> tuple[list[str], str]:
    if answer.get("unanswerable") is True:
        return [], "abstain"
    spans = [str(s) for s in (answer.get("extractive_spans") or []) if str(s).strip()]
    if spans:
        return spans, "span"
    free = str(answer.get("free_form_answer") or "").strip()
    if free:
        return [free], "free_form"
    if answer.get("yes_no") is not None:
        return ["yes" if answer.get("yes_no") else "no"], "yes_no_maybe"
    return [], "abstain"


def _evidence_spans(doc: CorpusDoc, evidence: list[str]) -> list[EvidenceSpan]:
    spans: list[EvidenceSpan] = []
    for text in evidence:
        if not text:
            continue
        start = doc.body.find(text)
        if start >= 0:
            spans.append(EvidenceSpan(doc_id=doc.doc_id, start=start, end=start + len(text), text=text))
    return spans


def build_qa_rows(row: dict[str, Any], split: str, doc: CorpusDoc) -> list[EvalQaRow]:
    pid = _paper_id(row)
    out: list[EvalQaRow] = []
    for q_index, qa in enumerate(_qa_items(row)):
        question = str(qa.get("question") or "").strip()
        if not question:
            continue
        qid = str(qa.get("question_id") or q_index)
        for a_index, answer in enumerate(_answer_records(qa.get("answers"))):
            texts, answer_type = _answer_text_and_type(answer)
            evidence = [str(e) for e in (answer.get("evidence") or []) if str(e).strip()]
            highlighted = [str(e) for e in (answer.get("highlighted_evidence") or []) if str(e).strip()]
            out.append(EvalQaRow(
                qa_id=qa_id("qasper", pid, qid, str(a_index)),
                source="qasper",
                source_type="paper",
                question=question,
                answers=texts,
                answer_type=answer_type,  # type: ignore[arg-type]
                evidence_doc_ids=[doc.doc_id] if evidence or highlighted else [],
                evidence_spans=_evidence_spans(doc, highlighted or evidence),
                negative_doc_ids=[],
                relation_type="summarizes",
                metadata={"split": split, "thread_id": pid, "upstream_id": f"{pid}:{qid}"},
                license=LICENSE_SPDX,
                provenance=PROVENANCE,
            ))
    return out


def main() -> int:
    docs: list[CorpusDoc] = []
    qa_rows: list[EvalQaRow] = []
    seen: set[str] = set()
    for split, row in iter_rows():
        if len(docs) >= TARGET_DOCS:
            break
        d = build_doc(row, split)
        if d is None or d.doc_id in seen:
            continue
        seen.add(d.doc_id)
        docs.append(d)
        qa_rows.extend(build_qa_rows(row, split, d))

    raw_path = corpus_data_dir() / "raw" / "qasper.jsonl"
    eval_path = corpus_data_dir() / "eval" / "qasper_qa.jsonl"
    doc_count, doc_sha = write_jsonl(raw_path, docs)
    qa_count, qa_sha = write_eval_jsonl(eval_path, qa_rows)
    print(f"wrote {doc_count} docs to {raw_path}")
    print(f"sha256 = {doc_sha}")
    print(f"wrote {qa_count} eval QA rows to {eval_path}")
    print(f"eval sha256 = {qa_sha}")
    return 0 if doc_count > 0 and qa_count > 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
