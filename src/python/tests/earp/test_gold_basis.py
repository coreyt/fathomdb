"""S1 gold-basis tests — written RED, before `eval.earp.gold` exists.

Pure tests: no SDK, no database, no network, no real corpus data. Every fixture
is synthesised in a tmp_path, so these run in the default suite and never
depend on the gitignored `data/corpus-data` tree.

Acceptance criteria under test are numbered per
`dev/design/earp-slice-1-design.md` § Acceptance criteria.
"""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Any

import pytest

from eval.earp.gold import verify_gold
from eval.earp.schema.models import BlockerCode

SNAPSHOT_HASH = "a" * 64
OTHER_HASH = "b" * 64
QRELS = "ir-c-reused-v2"


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _gold_doc(**over: Any) -> dict[str, Any]:
    doc: dict[str, Any] = {
        "corpus_hash": SNAPSHOT_HASH,
        "qrels_version": QRELS,
        "note": "IR-C reuse tier — synthetic fixture",
        "queries": [
            {
                "query_id": "src:q1",
                "query": "who owns the deal sheet?",
                "query_class": "exact_fact",
                "required_evidence": [
                    {
                        "evidence_id": "src:q1#e0",
                        "doc_id": "doc-a",
                        "necessity": "required",
                        "locator": {"kind": "whole_body"},
                    }
                ],
                "expected_top_k_doc_ids": ["doc-a"],
                "source": "enronqa",
                "answer_type": "span",
                "relation_type": "mentions",
                "query_origin": "human_dataset",
            },
            {
                "query_id": "src:q2",
                "query": "what did nobody ever say?",
                "query_class": "negative",
                "required_evidence": [],
                "expected_top_k_doc_ids": [],
                "source": "enronqa",
                "query_origin": "human_dataset",
            },
        ],
    }
    doc.update(over)
    return doc


def _write(path: Path, doc: Any) -> Path:
    path.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return path


@pytest.fixture
def bed(tmp_path: Path) -> dict[str, Any]:
    """A valid, self-consistent bed: snapshot + gold + matching pins."""
    root = tmp_path / "corpus-data"
    (root / "eval" / "ir_gold").mkdir(parents=True)
    gold_path = _write(root / "eval" / "ir_gold" / "all.gold.json", _gold_doc())
    snapshot = _write(
        tmp_path / "snapshot.json",
        {"snapshot_id": "syn", "corpus_hash": SNAPSHOT_HASH, "total_docs": 2},
    )
    manifest = _write(tmp_path / "manifest.json", {"schema_version": 1, "sources": []})
    return {
        "data_root": root,
        "gold_path": gold_path,
        "snapshot": snapshot,
        "manifest": manifest,
        "sha256": _sha256(gold_path),
        "corpus_hash": SNAPSHOT_HASH,
        "qrels_version": QRELS,
    }


def _verify(bed: dict[str, Any], **over: Any) -> Any:
    kwargs = {
        "data_root": bed["data_root"],
        "gold_path": bed["gold_path"],
        "snapshot_path": bed["snapshot"],
        "manifest_path": bed["manifest"],
        "expected_sha256": bed["sha256"],
        "expected_corpus_hash": bed["corpus_hash"],
        "expected_qrels_version": bed["qrels_version"],
    }
    kwargs.update(over)
    return verify_gold(**kwargs)


# --- AC-2: the success path ------------------------------------------------


def test_valid_gold_returns_all_three_identities(bed: dict[str, Any]) -> None:
    result = _verify(bed)
    assert result.blocker is None
    assert result.gold_identity.sha256 == bed["sha256"]
    assert result.gold_identity.corpus_hash == SNAPSHOT_HASH
    assert result.gold_identity.qrels_version == QRELS
    #: AC-8 -- total, including the negative.
    assert result.gold_identity.query_count == 2
    assert result.gold_set.note is not None
    assert len(result.gold_set.queries) == 2


def test_corpus_identity_separates_snapshot_from_manifest(bed: dict[str, Any]) -> None:
    """AC-3. The snapshot's file hash, the manifest's file hash, and the
    corpus_hash FIELD read out of the snapshot body are three different
    values and must never be merged."""
    result = _verify(bed)
    corpus = result.corpus_identity
    assert corpus.snapshot_sha256 == _sha256(bed["snapshot"])
    assert corpus.manifest_sha256 == _sha256(bed["manifest"])
    assert corpus.snapshot_sha256 != corpus.manifest_sha256
    #: The body field is the gold's pin, NOT the snapshot file's hash.
    assert corpus.snapshot_sha256 != result.gold_identity.corpus_hash


# --- AC-1: each refusal is its own code ------------------------------------


def test_absent_data_root_is_corpus_root_absent(bed: dict[str, Any], tmp_path: Path) -> None:
    result = _verify(bed, data_root=tmp_path / "not-there")
    assert result.blocker.code is BlockerCode.CORPUS_ROOT_ABSENT


def test_absent_gold_file_is_gold_missing(bed: dict[str, Any]) -> None:
    result = _verify(bed, gold_path=bed["data_root"] / "eval" / "ir_gold" / "nope.json")
    assert result.blocker.code is BlockerCode.GOLD_MISSING


def test_hash_mismatch_is_gold_hash_mismatch(bed: dict[str, Any]) -> None:
    result = _verify(bed, expected_sha256="f" * 64)
    assert result.blocker.code is BlockerCode.GOLD_HASH_MISMATCH


def test_unparseable_gold_is_gold_malformed(bed: dict[str, Any]) -> None:
    bed["gold_path"].write_text("{not json at all", encoding="utf-8")
    result = _verify(bed, expected_sha256=_sha256(bed["gold_path"]))
    assert result.blocker.code is BlockerCode.GOLD_MALFORMED


def test_missing_queries_array_is_gold_malformed(bed: dict[str, Any]) -> None:
    doc = _gold_doc()
    del doc["queries"]
    _write(bed["gold_path"], doc)
    result = _verify(bed, expected_sha256=_sha256(bed["gold_path"]))
    assert result.blocker.code is BlockerCode.GOLD_MALFORMED


def test_absent_snapshot_is_snapshot_unreadable(bed: dict[str, Any], tmp_path: Path) -> None:
    result = _verify(bed, snapshot_path=tmp_path / "gone.json")
    assert result.blocker.code is BlockerCode.SNAPSHOT_UNREADABLE


def test_snapshot_without_corpus_hash_is_snapshot_unreadable(bed: dict[str, Any]) -> None:
    _write(bed["snapshot"], {"snapshot_id": "syn", "total_docs": 2})
    result = _verify(bed)
    assert result.blocker.code is BlockerCode.SNAPSHOT_UNREADABLE


def test_corpus_hash_mismatch_is_gold_corpus_mismatch(bed: dict[str, Any]) -> None:
    _write(bed["snapshot"], {"snapshot_id": "syn", "corpus_hash": OTHER_HASH})
    result = _verify(bed)
    assert result.blocker.code is BlockerCode.GOLD_CORPUS_MISMATCH


def test_declared_corpus_hash_must_also_agree(bed: dict[str, Any]) -> None:
    """The check is THREE-way: config pin, gold body, snapshot body."""
    result = _verify(bed, expected_corpus_hash=OTHER_HASH)
    assert result.blocker.code is BlockerCode.GOLD_CORPUS_MISMATCH


# --- AC-1 / version hygiene ------------------------------------------------


def test_stale_v1_gold_is_refused(bed: dict[str, Any]) -> None:
    _write(bed["gold_path"], _gold_doc(qrels_version="ir-c-reused-v1"))
    result = _verify(bed, expected_sha256=_sha256(bed["gold_path"]))
    assert result.blocker.code is BlockerCode.GOLD_STALE_QRELS_VERSION


def test_version_rule_is_exact_match_not_a_denylist(bed: dict[str, Any]) -> None:
    """A denylist fails open: when the generator moves to v3, a stale v2 cache
    would pass silently. Exact match against the declared version fails closed."""
    _write(bed["gold_path"], _gold_doc(qrels_version="ir-c-reused-v2"))
    result = _verify(
        bed,
        expected_sha256=_sha256(bed["gold_path"]),
        expected_qrels_version="ir-c-reused-v3",
    )
    assert result.blocker.code is BlockerCode.GOLD_STALE_QRELS_VERSION


def test_stale_version_message_does_not_claim_a_metric_change(bed: dict[str, Any]) -> None:
    """The refusal is provenance hygiene. `evidence_spans` is non-empty on zero
    of 4,597 source rows, so v2 is semantically identical to v1 -- an operator
    must never be told to regenerate because metrics would move."""
    _write(bed["gold_path"], _gold_doc(qrels_version="ir-c-reused-v1"))
    result = _verify(bed, expected_sha256=_sha256(bed["gold_path"]))
    message = result.blocker.message.lower()
    assert "span" not in message
    assert "metric" not in message


# --- AC-4: the ordering is observable --------------------------------------


def test_hash_pin_wins_over_corpus_mismatch(bed: dict[str, Any]) -> None:
    """A file that fails BOTH the pin and the cross-check reports the pin:
    fields of an unverified file cannot be trusted enough to name a defect."""
    _write(bed["gold_path"], _gold_doc(corpus_hash=OTHER_HASH))
    result = _verify(bed)  # bed's sha256 is now stale AND corpus_hash differs
    assert result.blocker.code is BlockerCode.GOLD_HASH_MISMATCH


def test_hash_pin_wins_over_malformed(bed: dict[str, Any]) -> None:
    bed["gold_path"].write_text("{not json", encoding="utf-8")
    result = _verify(bed)  # stale pin AND unparseable
    assert result.blocker.code is BlockerCode.GOLD_HASH_MISMATCH


# --- AC-5: closed vocabularies are validated, not retained ------------------


@pytest.mark.parametrize(
    ("field", "bad"),
    [
        ("query_class", "exact-fact"),
        ("query_origin", "templeted"),
    ],
)
def test_unknown_vocabulary_value_is_refused(bed: dict[str, Any], field: str, bad: str) -> None:
    doc = _gold_doc()
    doc["queries"][0][field] = bad
    _write(bed["gold_path"], doc)
    result = _verify(bed, expected_sha256=_sha256(bed["gold_path"]))
    assert result.blocker.code is BlockerCode.GOLD_MALFORMED


def test_unknown_necessity_is_refused(bed: dict[str, Any]) -> None:
    doc = _gold_doc()
    doc["queries"][0]["required_evidence"][0]["necessity"] = "sort-of"
    _write(bed["gold_path"], doc)
    result = _verify(bed, expected_sha256=_sha256(bed["gold_path"]))
    assert result.blocker.code is BlockerCode.GOLD_MALFORMED


def test_missing_query_id_is_refused(bed: dict[str, Any]) -> None:
    """Optional in the reference, but the pairing key for comparisons -- so a
    gold set lacking one is refused rather than letting S8 degrade silently."""
    doc = _gold_doc()
    del doc["queries"][0]["query_id"]
    _write(bed["gold_path"], doc)
    result = _verify(bed, expected_sha256=_sha256(bed["gold_path"]))
    assert result.blocker.code is BlockerCode.GOLD_MALFORMED


# --- permissive-superset handling ------------------------------------------


def test_unknown_keys_are_retained_not_rejected(bed: dict[str, Any]) -> None:
    """IR-B defines the gold schema as an additive superset owned upstream, so
    an unrecognised key is kept rather than refused. EARP's own config stays
    strict; that asymmetry is deliberate."""
    doc = _gold_doc()
    doc["queries"][0]["some_future_field"] = {"k": 1}
    _write(bed["gold_path"], doc)
    result = _verify(bed, expected_sha256=_sha256(bed["gold_path"]))
    assert result.blocker is None
    assert result.gold_set.queries[0].extra["some_future_field"] == {"k": 1}


def test_legacy_underscore_tracers_are_accepted(bed: dict[str, Any]) -> None:
    """v1 names `_source` / `_answer_type` fall back to the v2 fields, matching
    the reference's own legacy handling."""
    doc = _gold_doc()
    q = doc["queries"][0]
    q["_source"] = q.pop("source")
    q["_answer_type"] = q.pop("answer_type")
    _write(bed["gold_path"], doc)
    result = _verify(bed, expected_sha256=_sha256(bed["gold_path"]))
    assert result.blocker is None
    assert result.gold_set.queries[0].source == "enronqa"
    assert result.gold_set.queries[0].answer_type == "span"


def test_absent_query_origin_defaults_to_human_dataset(bed: dict[str, Any]) -> None:
    doc = _gold_doc()
    del doc["queries"][0]["query_origin"]
    _write(bed["gold_path"], doc)
    result = _verify(bed, expected_sha256=_sha256(bed["gold_path"]))
    assert result.blocker is None
    assert result.gold_set.queries[0].query_origin == "human_dataset"


def test_missing_locator_is_allowed(bed: dict[str, Any]) -> None:
    doc = _gold_doc()
    del doc["queries"][0]["required_evidence"][0]["locator"]
    _write(bed["gold_path"], doc)
    result = _verify(bed, expected_sha256=_sha256(bed["gold_path"]))
    assert result.blocker is None
    assert result.gold_set.queries[0].required_evidence[0].locator is None
