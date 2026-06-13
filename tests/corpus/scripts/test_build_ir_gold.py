"""Unit tests for build_ir_gold.transform_row (WI-2 tracers + WI-3a spans).

Pure-function tests — no corpus, no network. Run: `pytest tests/corpus/scripts/`.
"""

import os
import sys

sys.path.insert(0, os.path.dirname(__file__))

import build_ir_gold as b  # noqa: E402


def test_qrels_version_bumped_for_schema_change():
    assert b.QRELS_VERSION == "ir-c-reused-v2"


def test_positive_row_emits_promoted_tracers_and_spans():
    row = {
        "qa_id": "1",
        "question": "what was decided?",
        "answer_type": "span",
        "evidence_doc_ids": ["D"],
        "evidence_spans": [{"doc_id": "D", "start": 5, "end": 20}],
    }
    gq, reason = b.transform_row("qmsum", row, {"D"})
    assert reason == ""
    # Promoted (non-underscore) tracers; legacy keys gone.
    assert gq["source"] == "qmsum"
    assert gq["answer_type"] == "span"
    assert gq["query_origin"] == "human_dataset"
    assert "_source" not in gq and "_answer_type" not in gq
    # Span carried into the locator.
    loc = gq["required_evidence"][0]["locator"]
    assert loc["kind"] == "span"
    assert loc["spans"] == [{"doc_id": "D", "start": 5, "end": 20}]


def test_whole_body_locator_when_no_spans():
    row = {
        "qa_id": "2",
        "question": "who attended?",
        "answer_type": "free_form",
        "evidence_doc_ids": ["D"],
        "evidence_spans": [],
    }
    gq, reason = b.transform_row("enronqa", row, {"D"})
    assert reason == ""
    loc = gq["required_evidence"][0]["locator"]
    assert loc["kind"] == "whole_body"
    assert "spans" not in loc


def test_spans_are_filtered_per_doc():
    # A multi-doc evidence row: each unit only carries its own doc's spans.
    row = {
        "qa_id": "3",
        "question": "q",
        "answer_type": "span",
        "evidence_doc_ids": ["D1", "D2"],
        "evidence_spans": [
            {"doc_id": "D1", "start": 0, "end": 4},
            {"doc_id": "D2", "start": 7, "end": 9},
        ],
    }
    gq, reason = b.transform_row("qaconv", row, {"D1", "D2"})
    assert reason == ""
    by_doc = {u["doc_id"]: u["locator"] for u in gq["required_evidence"]}
    assert by_doc["D1"]["spans"] == [{"doc_id": "D1", "start": 0, "end": 4}]
    assert by_doc["D2"]["spans"] == [{"doc_id": "D2", "start": 7, "end": 9}]


def test_negative_row_keeps_tracers_and_empty_denominator():
    row = {
        "qa_id": "4",
        "question": "is X mentioned?",
        "answer_type": "abstain",
        "evidence_doc_ids": ["D"],
        "evidence_spans": [],
    }
    gq, reason = b.transform_row("qaconv", row, {"D"})
    assert reason == ""
    assert gq["query_class"] == "negative"
    assert gq["required_evidence"] == []
    assert gq["source"] == "qaconv"
    assert gq["query_origin"] == "human_dataset"
    assert "_source" not in gq
