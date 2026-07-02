"""TDD tests for the WEC-Eng event-corpus materializer (cross-source Slice B1).

These tests exercise the CONVERSION logic on a tiny inline fixture — a single
WEC-Eng gold-mention record plus a resolved Wikipedia->Wikidata QID — with NO
network I/O. The live download + Wikipedia QID resolution happen in
acquire_wec_eng.main(); the pure conversion helpers are what we pin here.

Assertions:
  (a) a WEC mention -> a valid CorpusDoc with source_type="event";
  (b) a resolved QID lands as a well-formed EntityRef(kind="qid") in entity_ids,
      carrying the mention surface;
  (c) an unresolved (None) QID yields entity_ids=[] (graceful, additive);
  (d) created_at is the fixed provenance constant — deterministic, no wall-clock;
  (e) sample_records is deterministic + bounded for a given (sample_size, seed);
  (f) the writer round-trips: entity_ids survive write_jsonl -> re-read.

Run: FATHOMDB_TESTS_NO_REBUILD=1 python -m pytest \
     tests/corpus/scripts/test_acquire_wec_eng.py -q
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
from acquire_wec_eng import (  # noqa: E402
    CREATED_AT,
    DEFAULT_SAMPLE_SIZE,
    DEFAULT_SEED,
    WecEngConfig,
    build_doc,
    mention_body,
    sample_records,
)

CONFIGS_DIR = Path(__file__).resolve().parent / "configs"

# ── inline fixture: one WEC-Eng gold mention (fields per the HF data card) ─────
FIXTURE_MENTION = {
    "coref_chain": 152075,
    "coref_link": "2011 G20 Cannes summit",
    "doc_id": "Nicolas Sarkozy",
    "mention_context": ["The", "leaders", "met", "at", "the", "summit", "in", "Cannes", "."],
    "mention_head": "summit",
    "mention_head_lemma": "summit",
    "mention_head_pos": "NOUN",
    "mention_id": 900001,
    "mention_index": 3,
    "mention_ner": "EVENT",
    "tokens_number": [5],
    "tokens_str": "summit",
}
FIXTURE_QID = "Q152075"


def test_mention_becomes_event_doc():
    doc = build_doc(FIXTURE_MENTION, FIXTURE_QID)
    assert doc.source_type == "event"
    assert doc.title == "2011 G20 Cannes summit"
    assert doc.body == mention_body(FIXTURE_MENTION)
    assert doc.body.strip(), "event-context body must be non-empty"


def test_resolved_qid_populates_entity_ref():
    doc = build_doc(FIXTURE_MENTION, FIXTURE_QID)
    assert len(doc.entity_ids) == 1
    ref = doc.entity_ids[0]
    assert ref.kind == "qid"
    assert ref.id == FIXTURE_QID
    assert ref.surface == "summit"


def test_qid_is_well_formed():
    doc = build_doc(FIXTURE_MENTION, FIXTURE_QID)
    ref = doc.entity_ids[0]
    assert _QID_RE.fullmatch(ref.id), f"QID {ref.id!r} is not well-formed (Q\\d+)"
    # A well-formed QID must not trip the _corpus_lib validation warning.
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        build_doc(FIXTURE_MENTION, FIXTURE_QID)


def test_unresolved_qid_yields_empty_entity_ids():
    doc = build_doc(FIXTURE_MENTION, None)
    assert doc.entity_ids == []
    assert doc.source_type == "event"


def test_created_at_is_deterministic_provenance_constant():
    a = build_doc(FIXTURE_MENTION, FIXTURE_QID)
    b = build_doc(FIXTURE_MENTION, FIXTURE_QID)
    assert a.created_at == CREATED_AT == b.created_at


def test_sample_records_deterministic_and_bounded():
    records = [dict(FIXTURE_MENTION, mention_id=i) for i in range(50)]
    a = sample_records(records, sample_size=10, seed=7)
    b = sample_records(records, sample_size=10, seed=7)
    assert len(a) == 10
    assert [r["mention_id"] for r in a] == [r["mention_id"] for r in b]
    # A different seed selects a different subset (with overwhelming probability).
    c = sample_records(records, sample_size=10, seed=8)
    assert [r["mention_id"] for r in a] != [r["mention_id"] for r in c]
    # sample_size >= population returns everything.
    full = sample_records(records, sample_size=1000, seed=7)
    assert len(full) == 50


def test_writer_round_trips_entity_ids(tmp_path):
    docs = [
        build_doc(FIXTURE_MENTION, FIXTURE_QID),
        build_doc(dict(FIXTURE_MENTION, mention_id=900002), None),
    ]
    out = tmp_path / "wec_eng.jsonl"
    count, sha = write_jsonl(out, docs)
    assert count == 2
    assert sha
    rows = [json.loads(line) for line in out.read_text(encoding="utf-8").splitlines()]
    assert rows[0]["source_type"] == "event"
    assert rows[0]["entity_ids"] == [
        {"id": "Q152075", "kind": "qid", "surface": "summit"}
    ]
    assert rows[1]["entity_ids"] == []


# ── typed-config conversion (behavior-identical to the former argparse) ────────


def test_config_defaults_match_legacy_argparse_defaults():
    cfg = WecEngConfig()
    assert cfg.split == "train"
    assert cfg.sample_size == DEFAULT_SAMPLE_SIZE == 3000
    assert cfg.seed == DEFAULT_SEED == 20260702


def test_config_round_trips_through_dict():
    cfg = WecEngConfig(split="dev", sample_size=50, seed=7)
    assert config_from_dict(WecEngConfig, asdict(cfg)) == cfg


def test_config_rejects_unknown_key():
    with pytest.raises(ValueError, match="unknown config keys"):
        config_from_dict(WecEngConfig, {"splitt": "train"})


def test_config_validate_rejects_bad_split():
    with pytest.raises(ValueError, match="split"):
        WecEngConfig(split="nonsense").validate()


def test_config_validate_rejects_nonpositive_sample_size():
    with pytest.raises(ValueError, match="sample_size"):
        WecEngConfig(sample_size=0).validate()


def test_resolve_config_override_matches_argparse_semantics():
    parser = argparse.ArgumentParser()
    add_config_cli(parser)
    args = parser.parse_args(
        ["--override", "split=dev", "--override", "sample_size=100"]
    )
    cfg = resolve_config(WecEngConfig, args, WecEngConfig())
    assert cfg.split == "dev"
    assert cfg.sample_size == 100
    assert cfg.seed == DEFAULT_SEED  # untouched fields keep defaults


def test_baked_yaml_config_matches_defaults():
    from _config import load_config

    cfg = load_config(WecEngConfig, CONFIGS_DIR / "acquire-wec-eng.yaml")
    assert cfg == WecEngConfig()
