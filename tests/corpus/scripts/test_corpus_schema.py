"""Schema tests for _corpus_lib cross-source extensions (Slice B1).

Pure-dataclass tests — no corpus, no network. Run:
`python -m pytest tests/corpus/scripts/test_corpus_schema.py -q`.

Covers the HITL-approved (2026-07-02, coreyt) additive schema changes:
  - `source_type` vocabulary extended 6 -> 8 with `event` + `kb`;
  - additive `entity_ids` field on CorpusDoc (list[EntityRef]) for
    cross-corpus QID/DOI join keys, defaulting to empty;
  - EntityRef.kind constrained to {"qid","doi"} with non-empty id.
"""

import json
import os
import sys

import pytest

sys.path.insert(0, os.path.dirname(__file__))

from _corpus_lib import (  # noqa: E402
    SOURCE_TYPES,
    CorpusDoc,
    EntityRef,
    EvalQaRow,
)


def _base_doc(**overrides) -> CorpusDoc:
    kwargs = dict(
        doc_id="deadbeef00000000",
        source_type="note",
        title=None,
        body="hello world",
        created_at="2026-07-02T00:00:00+00:00",
        modified_at=None,
        author_or_sender=None,
        recipients=[],
        people_mentions=[],
        project_mentions=[],
        tags=[],
        url_or_external_id=None,
        thread_id=None,
        parent_doc_id=None,
        license="MIT",
        provenance="test",
    )
    kwargs.update(overrides)
    return CorpusDoc(**kwargs)


def _base_qa_row(**overrides) -> EvalQaRow:
    kwargs = dict(
        qa_id="qa1",
        source="qaconv",
        source_type="event",
        question="what happened?",
        answers=["something"],
        answer_type="span",
        evidence_doc_ids=[],
        evidence_spans=[],
        negative_doc_ids=[],
        relation_type=None,
        metadata={},
        license="MIT",
        provenance="test",
    )
    kwargs.update(overrides)
    return EvalQaRow(**kwargs)


def test_source_types_extended_to_eight():
    assert set(SOURCE_TYPES) == {
        "email",
        "meeting",
        "paper",
        "article",
        "note",
        "todo",
        "event",
        "kb",
    }
    assert len(SOURCE_TYPES) == 8


@pytest.mark.parametrize("st", ["event", "kb"])
def test_new_source_types_construct_and_serialize_on_doc(st):
    doc = _base_doc(source_type=st)
    payload = json.loads(doc.to_jsonl())
    assert payload["source_type"] == st


@pytest.mark.parametrize("st", ["event", "kb"])
def test_new_source_types_validate_on_qa_row(st):
    row = _base_qa_row(source_type=st)
    # validate() is invoked by to_jsonl(); must not raise for the new types.
    row.validate()
    assert json.loads(row.to_jsonl())["source_type"] == st


def test_entity_ids_defaults_empty_and_serializes():
    doc = _base_doc()
    assert doc.entity_ids == []
    payload = json.loads(doc.to_jsonl())
    assert payload["entity_ids"] == []


def test_entity_ids_round_trip_qid_and_doi():
    doc = _base_doc(
        source_type="kb",
        entity_ids=[
            EntityRef(id="Q42", kind="qid", surface="Douglas Adams"),
            EntityRef(id="10.1000/xyz", kind="doi"),
        ],
    )
    payload = json.loads(doc.to_jsonl())
    assert payload["entity_ids"] == [
        {"id": "Q42", "kind": "qid", "surface": "Douglas Adams"},
        {"id": "10.1000/xyz", "kind": "doi", "surface": None},
    ]


def test_bad_entity_kind_raises():
    with pytest.raises(ValueError):
        EntityRef(id="Q42", kind="wikidata")


def test_empty_entity_id_raises():
    with pytest.raises(ValueError):
        EntityRef(id="", kind="qid")


def test_backward_compat_doc_without_entity_ids_serializes_empty():
    # Existing-style construction (no entity_ids kwarg) still works and
    # serializes entity_ids as [].
    doc = _base_doc(source_type="email", title="Re: budget")
    line = doc.to_jsonl()
    payload = json.loads(line)
    assert payload["entity_ids"] == []
    # sort_keys stays intact (keys emitted in ascending order).
    assert list(payload.keys()) == sorted(payload.keys())


def test_non_qid_pattern_warns_but_constructs():
    with pytest.warns(UserWarning):
        ref = EntityRef(id="notaqid", kind="qid")
    assert ref.id == "notaqid"
