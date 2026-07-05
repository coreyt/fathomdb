"""TDD tests for the CompMix kb-corpus materializer (cross-source acquire2).

These tests exercise the CONVERSION logic on a tiny inline fixture — a single
CompMix QA record carrying native Wikidata QIDs in ``entities``/``answers`` —
with NO network I/O. The live download + zip-member read happen in
acquire_compmix.main(); the pure conversion helpers are what we pin here.

Assertions:
  (a) a CompMix record -> a valid CorpusDoc with source_type="kb";
  (b) the question's + answer's native QIDs land as well-formed
      EntityRef(kind="qid") entries in entity_ids, carrying the label surfaces;
  (c) QIDs are deduped by id (first occurrence's surface wins, entities first);
  (d) a record with no valid QIDs yields entity_ids=[] (graceful, additive);
  (e) body is the question alone when answer_text is empty, else
      "<question> <answer_text>";
  (f) created_at is the fixed provenance constant — deterministic, no wall-clock;
  (g) sample_records is deterministic + bounded for a given (sample_size, seed);
  (h) the writer round-trips: entity_ids survive write_jsonl -> re-read.

Run: FATHOMDB_TESTS_NO_REBUILD=1 python -m pytest \
     tests/corpus/scripts/test_acquire_compmix.py -q
"""

from __future__ import annotations

import argparse
import json
import sys
import warnings
from dataclasses import asdict
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _config import add_config_cli, config_from_dict, resolve_config  # noqa: E402
from _corpus_lib import _QID_RE, write_jsonl  # noqa: E402
from acquire_compmix import (  # noqa: E402
    CREATED_AT,
    DEFAULT_SAMPLE_SIZE,
    DEFAULT_SEED,
    CompMixConfig,
    build_doc,
    doc_body,
    entity_refs,
    sample_records,
)

CONFIGS_DIR = Path(__file__).resolve().parent / "configs"

# ── inline fixture: one CompMix QA record (fields per the HF data card) ────────
FIXTURE_RECORD = {
    "question_id": "6470",
    "question": "What was Johann Morgenstern's profession?",
    "domain": "books",
    "entities": [{"id": "Q67149", "label": "Johann Karl Simon Morgenstern"}],
    "answers": [{"id": "Q40634", "label": "philology"}],
    "answer_src": "text",
    "answer_text": "Philologist",
    "convmix_question_id": "2180-4",
}


def test_record_becomes_kb_doc():
    doc = build_doc(FIXTURE_RECORD)
    assert doc.source_type == "kb"
    assert doc.title == "What was Johann Morgenstern's profession?"
    assert doc.body == doc_body(FIXTURE_RECORD)
    assert doc.body.strip(), "kb body must be non-empty"


def test_entities_and_answers_populate_entity_refs():
    doc = build_doc(FIXTURE_RECORD)
    assert len(doc.entity_ids) == 2
    ids = [ref.id for ref in doc.entity_ids]
    # entities before answers
    assert ids == ["Q67149", "Q40634"]
    assert all(ref.kind == "qid" for ref in doc.entity_ids)
    assert doc.entity_ids[0].surface == "Johann Karl Simon Morgenstern"
    assert doc.entity_ids[1].surface == "philology"


def test_qids_are_well_formed():
    doc = build_doc(FIXTURE_RECORD)
    for ref in doc.entity_ids:
        assert _QID_RE.fullmatch(ref.id), f"QID {ref.id!r} is not well-formed (Q\\d+)"
    # Well-formed QIDs must not trip the _corpus_lib validation warning.
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        build_doc(FIXTURE_RECORD)


def test_qids_deduped_first_surface_wins():
    record = {
        "question_id": "42",
        "question": "Who?",
        "domain": "movies",
        "entities": [{"id": "Q5", "label": "human"}],
        "answers": [
            {"id": "Q5", "label": "person"},
            {"id": "Q7", "label": "other"},
        ],
        "answer_src": "kb",
        "answer_text": "",
    }
    refs = entity_refs(record)
    assert [r.id for r in refs] == ["Q5", "Q7"]
    # First occurrence (from entities) keeps its surface.
    assert refs[0].surface == "human"
    assert refs[1].surface == "other"


def test_no_valid_qids_yields_empty_entity_ids():
    record = {
        "question_id": "99",
        "question": "Untethered question?",
        "domain": "music",
        "entities": [{"id": "not-a-qid", "label": "junk"}],
        "answers": [],
        "answer_src": "",
        "answer_text": "",
    }
    doc = build_doc(record)
    assert doc.entity_ids == []
    assert doc.source_type == "kb"


def test_body_is_question_when_answer_text_empty():
    record = dict(FIXTURE_RECORD, answer_text="")
    assert doc_body(record) == record["question"]
    record2 = dict(FIXTURE_RECORD, answer_text="Philologist")
    assert doc_body(record2) == "What was Johann Morgenstern's profession? Philologist"


def test_tags_preserve_domain_and_answer_src():
    doc = build_doc(FIXTURE_RECORD)
    assert "compmix-domain:books" in doc.tags
    assert "answer-src:text" in doc.tags


def test_created_at_is_deterministic_provenance_constant():
    a = build_doc(FIXTURE_RECORD)
    b = build_doc(FIXTURE_RECORD)
    assert a.created_at == CREATED_AT == b.created_at


def test_sample_records_deterministic_and_bounded():
    records = [dict(FIXTURE_RECORD, question_id=str(i)) for i in range(50)]
    a = sample_records(records, sample_size=10, seed=7)
    b = sample_records(records, sample_size=10, seed=7)
    assert len(a) == 10
    assert [r["question_id"] for r in a] == [r["question_id"] for r in b]
    # A different seed selects a different subset (overwhelming probability).
    c = sample_records(records, sample_size=10, seed=8)
    assert [r["question_id"] for r in a] != [r["question_id"] for r in c]
    # sample_size >= population returns everything.
    full = sample_records(records, sample_size=1000, seed=7)
    assert len(full) == 50


def test_writer_round_trips_entity_ids(tmp_path):
    docs = [
        build_doc(FIXTURE_RECORD),
        build_doc(
            {
                "question_id": "77",
                "question": "No entities here?",
                "domain": "soccer",
                "entities": [],
                "answers": [],
                "answer_src": "",
                "answer_text": "",
            }
        ),
    ]
    out = tmp_path / "compmix.jsonl"
    count, sha = write_jsonl(out, docs)
    assert count == 2
    assert sha
    rows = [json.loads(line) for line in out.read_text(encoding="utf-8").splitlines()]
    assert rows[0]["source_type"] == "kb"
    assert rows[0]["entity_ids"] == [
        {"id": "Q67149", "kind": "qid", "surface": "Johann Karl Simon Morgenstern"},
        {"id": "Q40634", "kind": "qid", "surface": "philology"},
    ]
    assert rows[1]["entity_ids"] == []


# ── typed-config conversion (behavior-identical to the wec_eng exemplar) ───────


def test_config_defaults_match_baked_defaults():
    cfg = CompMixConfig()
    assert cfg.split == "train"
    assert cfg.sample_size == DEFAULT_SAMPLE_SIZE == 5000
    assert cfg.seed == DEFAULT_SEED == 20260702


def test_config_round_trips_through_dict():
    cfg = CompMixConfig(split="dev", sample_size=50, seed=7)
    assert config_from_dict(CompMixConfig, asdict(cfg)) == cfg


def test_config_rejects_unknown_key():
    with pytest.raises(ValueError, match="unknown config keys"):
        config_from_dict(CompMixConfig, {"splitt": "train"})


def test_config_validate_rejects_bad_split():
    with pytest.raises(ValueError, match="split"):
        CompMixConfig(split="nonsense").validate()


def test_config_validate_rejects_nonpositive_sample_size():
    with pytest.raises(ValueError, match="sample_size"):
        CompMixConfig(sample_size=0).validate()


def test_resolve_config_override_matches_argparse_semantics():
    parser = argparse.ArgumentParser()
    add_config_cli(parser)
    args = parser.parse_args(
        ["--override", "split=dev", "--override", "sample_size=100"]
    )
    cfg = resolve_config(CompMixConfig, args, CompMixConfig())
    assert cfg.split == "dev"
    assert cfg.sample_size == 100
    assert cfg.seed == DEFAULT_SEED  # untouched fields keep defaults


def test_baked_yaml_config_matches_defaults():
    from _config import load_config

    cfg = load_config(CompMixConfig, CONFIGS_DIR / "acquire-compmix.yaml")
    assert cfg == CompMixConfig()
