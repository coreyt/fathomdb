"""S6 corpus-scale characterization + replay tests — written RED.

These run OFFLINE against a small corpus-shaped fixture: a synthetic snapshot
with real per-shard hashes, a matching shard, and a matching gold set. The real
10,506-document campaign is exercised only when the gitignored data is present,
and skips visibly otherwise.
"""

from __future__ import annotations

import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import pytest

from eval.earp.schema.models import BlockerCode, RunVerdict

pytest.importorskip("fathomdb._fathomdb", reason="native binding not built")

from eval.earp.characterize import (  # noqa: E402
    DriftAxis,
    load_corpus,
    replay,
    run_characterization,
)

TS = datetime(2026, 8, 6, 12, 0, tzinfo=timezone.utc)

DOCS = [
    {"doc_id": "d1", "body": "the deal sheet is missing for March", "source_type": "email"},
    {"doc_id": "d2", "body": "parking arrangements for the annual meeting", "source_type": "note"},
    {"doc_id": "d3", "body": "quarterly revenue rose after the deal closed", "source_type": "article"},
]


def _shard(tmp_path: Path, docs: list[dict[str, Any]] | None = None) -> Path:
    raw = tmp_path / "raw"
    raw.mkdir(exist_ok=True)
    path = raw / "synthetic_notes.jsonl"
    path.write_text(
        "".join(json.dumps(d) + "\n" for d in (DOCS if docs is None else docs)),
        encoding="utf-8",
    )
    return path


def _snapshot(tmp_path: Path, shard: Path, *, doc_count: int | None = None) -> Path:
    digest = hashlib.sha256(shard.read_bytes()).hexdigest()
    path = tmp_path / "snapshot.json"
    path.write_text(
        json.dumps(
            {
                "corpus_hash": "c" * 64,
                "total_docs": doc_count if doc_count is not None else len(DOCS),
                "per_source_sha256": [
                    {
                        "source": "synthetic_notes",
                        "sha256": digest,
                        "doc_count": doc_count if doc_count is not None else len(DOCS),
                    }
                ],
            }
        ),
        encoding="utf-8",
    )
    return path


def _gold(tmp_path: Path) -> Path:
    path = tmp_path / "gold.json"
    path.write_text(
        json.dumps(
            {
                "corpus_hash": "c" * 64,
                "qrels_version": "ir-c-reused-v2",
                "queries": [
                    {
                        "query_id": "q1",
                        "query": "deal sheet",
                        "query_class": "exact_fact",
                        "required_evidence": [
                            {"evidence_id": "q1#e0", "doc_id": "d1", "necessity": "required"}
                        ],
                        "expected_top_k_doc_ids": ["d1"],
                    },
                    {
                        "query_id": "q2",
                        "query": "zzzznothingmatcheszzz",
                        "query_class": "negative",
                        "required_evidence": [],
                        "expected_top_k_doc_ids": [],
                    },
                ],
            }
        ),
        encoding="utf-8",
    )
    return path


def _bed(tmp_path: Path, **over: Any) -> dict[str, Any]:
    shard = _shard(tmp_path, over.pop("docs", None))
    snapshot = _snapshot(tmp_path, shard, doc_count=over.pop("doc_count", None))
    gold = _gold(tmp_path)
    bed = {
        "data_root": tmp_path,
        "snapshot_path": snapshot,
        "gold_path": gold,
        "gold_sha256": hashlib.sha256(gold.read_bytes()).hexdigest(),
        "corpus_hash": "c" * 64,
        "qrels_version": "ir-c-reused-v2",
        "experiments_root": tmp_path / "experiments",
        "experiment": "earp-characterization",
        "ts": TS,
        "evidence_recall_k": (5, 10),
    }
    bed.update(over)
    return bed


# --- AC-1: end to end -------------------------------------------------------


def test_characterization_scores_against_real_retrieval(tmp_path: Path) -> None:
    result = run_characterization(**_bed(tmp_path))
    assert result.verdict is RunVerdict.COMPLETE
    assert result.ingested == 3
    #: q1's required doc is d1 and FTS finds it; q2 is a negative.
    assert result.per_k[10].overall.strict() == 1.0
    assert result.per_k[10].negative.n == 1


def test_negatives_are_scored_by_abstention_not_recall(tmp_path: Path) -> None:
    result = run_characterization(**_bed(tmp_path))
    #: The negative is held OUT of the recall mean entirely.
    assert result.per_k[10].overall.n == 1
    assert result.per_k[10].negative.n == 1


def test_ndcg_and_supporting_are_not_applicable(tmp_path: Path) -> None:
    result = run_characterization(**_bed(tmp_path))
    assert result.document_metrics["ndcg"].value is None
    assert result.per_k[10].overall.supporting() is None


def test_fanout_is_recorded(tmp_path: Path) -> None:
    result = run_characterization(**_bed(tmp_path))
    assert result.fanout_used > 0


# --- AC-2/3: ingest is snapshot-driven, never a glob ------------------------


def test_ingest_uses_only_the_snapshot_shards(tmp_path: Path) -> None:
    """A glob over raw/*.jsonl would pick up 12,800 documents that are not in
    the frozen snapshot, pinning a 10,506-doc identity to a 2.2x index."""
    stray = tmp_path / "raw" / "musique_dev.jsonl"
    _shard(tmp_path)
    stray.write_text(json.dumps({"question": "no doc_id here"}) + "\n", encoding="utf-8")
    result = run_characterization(**_bed(tmp_path))
    assert result.ingested == 3


def test_shard_hash_mismatch_is_refused(tmp_path: Path) -> None:
    bed = _bed(tmp_path)
    shard = tmp_path / "raw" / "synthetic_notes.jsonl"
    shard.write_text(shard.read_text() + json.dumps({"doc_id": "x", "body": "y"}) + "\n")
    result = run_characterization(**bed)
    assert result.verdict is RunVerdict.BLOCKED
    assert result.blockers[0].code is BlockerCode.CORPUS_ROOT_ABSENT


def test_duplicate_doc_id_is_refused_before_the_write(tmp_path: Path) -> None:
    dupes = [*DOCS, {"doc_id": "d1", "body": "a duplicate", "source_type": "note"}]
    with pytest.raises(ValueError, match="duplicate"):
        load_corpus(_shard(tmp_path, dupes), source="synthetic_notes")


def test_absent_data_root_is_a_typed_blocker(tmp_path: Path) -> None:
    bed = _bed(tmp_path)
    bed["data_root"] = tmp_path / "nope"
    result = run_characterization(**bed)
    assert result.verdict is RunVerdict.BLOCKED
    assert result.blockers[0].code is BlockerCode.CORPUS_ROOT_ABSENT


def test_stale_v1_gold_is_refused(tmp_path: Path) -> None:
    bed = _bed(tmp_path)
    gold = json.loads(bed["gold_path"].read_text())
    gold["qrels_version"] = "ir-c-reused-v1"
    bed["gold_path"].write_text(json.dumps(gold), encoding="utf-8")
    bed["gold_sha256"] = hashlib.sha256(bed["gold_path"].read_bytes()).hexdigest()
    result = run_characterization(**bed)
    assert result.verdict is RunVerdict.BLOCKED
    assert result.blockers[0].code is BlockerCode.GOLD_STALE_QRELS_VERSION
    assert "build_ir_gold" in result.blockers[0].message


def test_gold_doc_id_absent_from_corpus_is_refused(tmp_path: Path) -> None:
    """Free to assert, and it prevents a silent 0.0 if the shard list drifts."""
    bed = _bed(tmp_path)
    gold = json.loads(bed["gold_path"].read_text())
    gold["queries"][0]["required_evidence"][0]["doc_id"] = "not-in-corpus"
    bed["gold_path"].write_text(json.dumps(gold), encoding="utf-8")
    bed["gold_sha256"] = hashlib.sha256(bed["gold_path"].read_bytes()).hexdigest()
    result = run_characterization(**bed)
    assert result.verdict is RunVerdict.BLOCKED
    assert result.blockers[0].code is BlockerCode.GOLD_CORPUS_MISMATCH


# --- the runtime fix: retrieve once ----------------------------------------


def test_each_query_is_retrieved_exactly_once(tmp_path: Path) -> None:
    """metrics.aggregate calls retrieve inside its own per-K loop, so driving
    the ladder by calling it per rung would re-run every search -- ~55 hours at
    corpus scale instead of ~28."""
    result = run_characterization(**_bed(tmp_path))
    assert result.retrievals == 2  # two gold queries, two K rungs
    assert set(result.per_k) == {5, 10}


def test_retrieved_ids_are_truncated_to_the_deepest_k(tmp_path: Path) -> None:
    """An untruncated result at corpus scale is ~5,000 ids per row; across
    9,194 rows that is a ~1.5 GB sidecar."""
    result = run_characterization(**_bed(tmp_path))
    for row in result.per_query_rows:
        ids = row.get("retrieved_doc_ids") or []
        assert len(ids) <= 10


# --- per-query artifacts ----------------------------------------------------


def test_per_query_rows_are_written_per_k(tmp_path: Path) -> None:
    result = run_characterization(**_bed(tmp_path))
    assert result.run_dir is not None
    lines = (result.run_dir / "earp.per-query.v1.jsonl").read_text().strip().splitlines()
    assert len(lines) == 4  # 2 queries x 2 K rungs
    rows = [json.loads(line) for line in lines]
    assert {r["k"] for r in rows} == {5, 10}
    # Negatives are SCORED, but by abstention -- they carry `abstained` and
    # null recall numbers, because recall is not the thing being measured.
    for row in rows:
        if row["outcome"] != "scored":
            continue
        if row["query_class"] == "negative":
            assert row["abstained"] is not None
            assert row["strict"] is None
        else:
            assert row["strict"] is not None
            assert row["graded"] is not None


def test_retrieval_error_is_a_typed_per_query_failure(tmp_path: Path) -> None:
    def boom(_query: str) -> Any:
        raise RuntimeError("retrieval exploded")

    result = run_characterization(**_bed(tmp_path), retrieve_override=boom)
    rows = [r for r in result.per_query_rows if r["outcome"] == "error"]
    assert rows
    assert result.per_k[10].overall.n == 0
    #: Never folded into an empty result set and scored as a miss.
    assert all(r.get("strict") is None for r in rows)


# --- AC-9: replay -----------------------------------------------------------


def test_replay_reports_no_drift_for_an_identical_run(tmp_path: Path) -> None:
    bed = _bed(tmp_path)
    first = run_characterization(**bed)
    assert first.run_id is not None
    report = replay(
        run_id=first.run_id,
        experiments_root=bed["experiments_root"],
        config_doc=first.config_doc,
        code={"git_sha": first.code_sha, "dirty": False},
        env={"python": first.python_version},
    )
    assert report.drift == ()


def test_replay_reports_code_drift_separately(tmp_path: Path) -> None:
    """The interesting case: same declared experiment, different engine."""
    bed = _bed(tmp_path)
    first = run_characterization(**bed)
    assert first.run_id is not None
    report = replay(
        run_id=first.run_id,
        experiments_root=bed["experiments_root"],
        config_doc=first.config_doc,
        code={"git_sha": "f" * 40, "dirty": False},
        env={"python": first.python_version},
    )
    assert DriftAxis.CODE in {d.axis for d in report.drift}
    assert DriftAxis.CONFIG not in {d.axis for d in report.drift}


def test_replay_marks_an_unrecorded_axis_unrecoverable(tmp_path: Path) -> None:
    """S5 wrote empty code/env dicts, so drift from "" is not drift -- it is a
    missing record, and reporting it as drift would be a false positive."""
    bed = _bed(tmp_path)
    first = run_characterization(**bed, blank_provenance=True)
    assert first.run_id is not None
    report = replay(
        run_id=first.run_id,
        experiments_root=bed["experiments_root"],
        config_doc=first.config_doc,
        code={"git_sha": "f" * 40, "dirty": False},
        env={"python": "3.12.3"},
    )
    axes = {d.axis: d for d in report.drift}
    assert axes[DriftAxis.CODE].unrecoverable is True


def test_replay_does_not_rule_on_drift(tmp_path: Path) -> None:
    bed = _bed(tmp_path)
    first = run_characterization(**bed)
    assert first.run_id is not None
    report = replay(
        run_id=first.run_id,
        experiments_root=bed["experiments_root"],
        config_doc=first.config_doc,
        code={"git_sha": "f" * 40, "dirty": False},
        env={"python": first.python_version},
    )
    assert not hasattr(report, "passed")
    assert not hasattr(report, "verdict")


# --- negative_class aggregate (2026-08-08 fix: Campaign 1 had to derive it
# --- by hand from per-query rows; the S0 schema block existed, unwritten) ---


def test_metrics_document_carries_the_negative_class_aggregate(tmp_path: Path) -> None:
    """The sidecar's metrics block must carry the k-free negative_class
    aggregate: abstention is K-independent (a non-empty list is non-empty at
    every K >= 1), so one block, not a per-K entry."""
    result = run_characterization(**_bed(tmp_path))
    assert result.verdict is RunVerdict.COMPLETE
    assert result.run_id is not None
    sidecar = json.loads(
        (tmp_path / "experiments" / "runs" / result.run_id / "earp.result.v1.json").read_text()
    )
    nc = sidecar["metrics"]["negative_class"]
    agg = result.per_k[10].negative
    assert nc["n"] == agg.n == 1
    assert nc["abstention_correct"] == agg.abstained
    assert nc["abstention_rate"]["status"] == "emitted"
    assert nc["abstention_rate"]["value"] == agg.abstained / agg.n


def test_negative_class_is_not_applicable_without_negatives(tmp_path: Path) -> None:
    """A gold set with zero negatives reports not_applicable, never 0.0 --
    an inapplicable diagnostic is not a failed one."""
    bed = _bed(tmp_path)  # writes the standard gold; filter it AFTERWARD
    gold_path = tmp_path / "gold.json"
    doc = json.loads(gold_path.read_text())
    doc["queries"] = [q for q in doc["queries"] if q["query_class"] != "negative"]
    gold_path.write_text(json.dumps(doc))
    bed["gold_sha256"] = hashlib.sha256(gold_path.read_bytes()).hexdigest()
    result = run_characterization(**bed)
    assert result.verdict is RunVerdict.COMPLETE
    assert result.run_id is not None
    sidecar = json.loads(
        (tmp_path / "experiments" / "runs" / result.run_id / "earp.result.v1.json").read_text()
    )
    nc = sidecar["metrics"]["negative_class"]
    assert nc["n"] == 0
    assert nc["abstention_rate"]["status"] == "not_applicable"
    assert nc["abstention_rate"]["value"] is None
