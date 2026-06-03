from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path
from types import ModuleType


def load_corpus_lib() -> ModuleType:
    repo = Path(__file__).resolve().parents[3]
    path = repo / "tests" / "corpus" / "scripts" / "_corpus_lib.py"
    spec = importlib.util.spec_from_file_location("_corpus_lib_for_test", path)
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def test_eval_qa_row_serializes_required_contract() -> None:
    corpus_lib = load_corpus_lib()

    row = corpus_lib.EvalQaRow(
        qa_id=corpus_lib.qa_id("qasper", "paper-1", "q-1", "a-0"),
        source="qasper",
        source_type="paper",
        question="What does the model compare?",
        answers=["baseline systems"],
        answer_type="free_form",
        evidence_doc_ids=["doc-1"],
        evidence_spans=[
            corpus_lib.EvidenceSpan(doc_id="doc-1", start=10, end=26, text="baseline systems")
        ],
        negative_doc_ids=[],
        relation_type="summarizes",
        metadata={"split": "train", "upstream_id": "paper-1:q-1"},
        license="CC-BY-4.0",
        provenance="hf:allenai/qasper@test",
    )

    payload = json.loads(row.to_jsonl())
    assert payload["qa_id"] == corpus_lib.qa_id("qasper", "paper-1", "q-1", "a-0")
    assert payload["source"] == "qasper"
    assert payload["source_type"] == "paper"
    assert payload["answer_type"] == "free_form"
    assert payload["evidence_doc_ids"] == ["doc-1"]
    assert payload["evidence_spans"][0]["start"] == 10


def test_eval_qa_id_is_deterministic_and_source_scoped() -> None:
    corpus_lib = load_corpus_lib()

    first = corpus_lib.qa_id("qmsum", "meeting-1", "general", "0")
    second = corpus_lib.qa_id("qmsum", "meeting-1", "general", "0")
    other_source = corpus_lib.qa_id("enronqa", "meeting-1", "general", "0")

    assert first == second
    assert first != other_source
    assert len(first) == 24


def test_eval_qa_row_rejects_ungrounded_evidence_doc_ids() -> None:
    corpus_lib = load_corpus_lib()

    row = corpus_lib.EvalQaRow(
        qa_id=corpus_lib.qa_id("enronqa", "row-1", "q-1"),
        source="enronqa",
        source_type="email",
        question="Who sent the message?",
        answers=["Phillip Allen"],
        answer_type="span",
        evidence_doc_ids=["missing-doc"],
        evidence_spans=[],
        negative_doc_ids=[],
        relation_type=None,
        metadata={"split": "train"},
        license="LicenseRef-EnronQA-Undeclared",
        provenance="hf:MichaelR207/enron_qa_0922@test",
    )

    try:
        corpus_lib.assert_grounded_evidence(row, {"known-doc"})
    except ValueError as exc:
        assert "missing-doc" in str(exc)
    else:
        raise AssertionError("expected ungrounded evidence to fail")


def test_eval_jsonl_writer_hashes_rows_without_doc_output_collision(tmp_path: Path) -> None:
    corpus_lib = load_corpus_lib()
    out = tmp_path / "eval" / "qmsum_qa.jsonl"
    row = corpus_lib.EvalQaRow(
        qa_id=corpus_lib.qa_id("qmsum", "meeting-1", "specific", "0"),
        source="qmsum",
        source_type="meeting",
        question="What action was discussed?",
        answers=["draft the plan"],
        answer_type="summary",
        evidence_doc_ids=["doc-1"],
        evidence_spans=[],
        negative_doc_ids=[],
        relation_type="summarizes",
        metadata={"split": "train", "thread_id": "meeting-1"},
        license="LicenseRef-QMSum-MIT-with-upstream-chain",
        provenance="github:Yale-LILY/QMSum@test",
    )

    count, digest = corpus_lib.write_eval_jsonl(out, [row])

    assert count == 1
    assert len(digest) == 64
    assert out.parent.name == "eval"
    assert json.loads(out.read_text(encoding="utf-8"))["qa_id"] == row.qa_id
