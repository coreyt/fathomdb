"""S6a public result-limit adoption tests — written RED, before the adoption.

0.8.22 Slice 18 ("bound ranked retrieval results") gave every search verb a
public `limit` (default 10, refused outside 1..=100 with a typed engine
error). One rule replaces the D-5 mode table: @K is measurable exactly when
`K <= limit`, for every retrieval mode, with the limit recorded with every
number. Design of record: `dev/design/earp-slice-6a-design.md`.

Pure tests (resolver, depth, identity) run everywhere; engine-backed tests
skip visibly when the native binding is absent, never silently.
"""

from __future__ import annotations

import hashlib
import inspect
import json
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import pytest

from eval.earp.config import resolve_config
from eval.earp.depth import check_depth
from eval.earp.schema import CONFIG_SCHEMA_PATH
from eval.earp.schema.models import (
    ENGINE_DEFAULT_RESULT_LIMIT,
    ENGINE_MAX_RESULT_LIMIT,
    BlockerCode,
    RetrievalMode,
    RunVerdict,
)

SHA = "a" * 64
TS = datetime(2026, 8, 7, 12, 0, tzinfo=timezone.utc)

#: config_sha256 of `_config()` below, computed from the PRE-S6a resolver and
#: hard-coded. The hash covers the RAW document (`_lib.canonical_json`), so
#: absence-means-10 must reproduce it byte-identically: if this constant ever
#: moves, every existing record's config identity silently breaks.
LIMITLESS_CONFIG_SHA256 = "3d526890d8c0b4db1cd8b35a4a87d2fd9a571cd8300b2f5788bae0725f870d6b"

RETIRED_WORDS = ("D-5.2", "set_search_limit_for_test", "SEARCH_RERANK_LIMIT")


def _config(**over: Any) -> dict[str, Any]:
    doc: dict[str, Any] = {
        "schema_version": "earp.v1",
        "campaign": "characterization",
        "corpus": {"snapshot": "tests/corpus/snapshot.json", "data_root": "data/corpus-data"},
        "gold": {
            "path": "d/all.gold.json",
            "sha256": SHA,
            "corpus_hash": SHA,
            "qrels_version": "ir-c-reused-v2",
        },
        "scenario": {
            "engine": {"use_default_embedder": True},
            "query": {"call": "Engine.search"},
        },
        "metrics": {"evidence_recall_k": [5, 10]},
    }
    doc.update(over)
    return doc


def _codes(result: Any) -> set[BlockerCode]:
    return {b.code for b in result.blockers}


# --- AC-6: the engine-mirror constants are guarded --------------------------


def test_engine_search_limit_default_mirrors_the_constant() -> None:
    """The S2 drift detector guards ir_eval.rs, NOT lib.rs, so the mirrored
    constants need their own binding-present guard."""
    pytest.importorskip("fathomdb._fathomdb", reason="native binding not built")
    from fathomdb import engine  # noqa: PLC0415

    for func in (
        engine.Engine.search,
        engine.Engine.search_text_only,
        engine.Engine.search_projected_text,
    ):
        default = inspect.signature(func).parameters["limit"].default
        assert default == ENGINE_DEFAULT_RESULT_LIMIT, func.__name__


def test_engine_window_is_pinned_empirically(tmp_path: Path) -> None:
    """limit=100 accepted, limit=101 refused with the engine's typed error --
    refusal, not clamp."""
    pytest.importorskip("fathomdb._fathomdb", reason="native binding not built")
    from fathomdb import Engine  # noqa: PLC0415
    from fathomdb.errors import InvalidArgumentError  # noqa: PLC0415

    engine = Engine.open(str(tmp_path / "mirror.db"))
    try:
        engine.write(
            [{"kind": "doc", "body": "deal sheet", "source_id": "s1", "logical_id": "doc-a"}]
        )
        accepted = engine.search_text_only("deal", limit=ENGINE_MAX_RESULT_LIMIT)
        assert len(accepted.results) == 1
        with pytest.raises(InvalidArgumentError):
            engine.search_text_only("deal", limit=ENGINE_MAX_RESULT_LIMIT + 1)
    finally:
        engine.close()


# --- AC-2: the schema owns the range window ----------------------------------


@pytest.mark.parametrize(
    "bad", [0, 101, 2.5, True], ids=["zero", "above-max", "non-integer", "bool"]
)
def test_out_of_window_limit_is_refused_by_the_schema(bad: Any) -> None:
    doc = _config()
    doc["scenario"]["query"]["limit"] = bad
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)


def test_limit_defect_is_collected_not_first_failure() -> None:
    doc = _config()
    doc["scenario"]["query"]["limit"] = 0
    doc["nope"] = 1
    result = resolve_config(doc)
    assert {
        BlockerCode.CONFIG_INVALID_VALUE,
        BlockerCode.CONFIG_UNKNOWN_KEY,
    } <= _codes(result)


@pytest.mark.parametrize(("limit", "ladder"), [(1, [1]), (100, [5, 10, 100])])
def test_in_window_limit_is_accepted(limit: int, ladder: list[int]) -> None:
    doc = _config()
    doc["scenario"]["query"]["limit"] = limit
    doc["metrics"]["evidence_recall_k"] = ladder
    result = resolve_config(doc)
    assert result.blockers == ()
    assert result.scenario is not None
    assert result.scenario.max_measurable_k == limit


def test_schema_window_mirrors_the_engine_and_names_it() -> None:
    """The schema owns the window (no bespoke resolver check), so its bounds
    must equal the mirrored engine constants and its description must name the
    engine's own validator, making the walker's refusal self-explaining."""
    schema = json.loads(CONFIG_SCHEMA_PATH.read_text(encoding="utf-8"))
    limit_schema = schema["properties"]["scenario"]["properties"]["query"]["properties"]["limit"]
    assert limit_schema["type"] == "integer"
    assert limit_schema["minimum"] == 1
    assert limit_schema["maximum"] == ENGINE_MAX_RESULT_LIMIT
    assert "validate_search_result_limit" in limit_schema["description"]


# --- AC-3: the successor depth rule ------------------------------------------


def test_k_at_the_limit_is_measurable_for_hybrid() -> None:
    assert check_depth(RetrievalMode.HYBRID, 50, 50) is None


def test_k_beyond_the_limit_is_refused_even_for_fts_only() -> None:
    """fts_only deep-K without a declared limit was ACCEPTED under the old
    unbounded-FTS doctrine; the rebuilt engine really returns at most 10 by
    default, so it is now refused -- and the message names the Slice 18
    `limit` lever, not the retired commissioning or the hidden seam."""
    blocker = check_depth(RetrievalMode.FTS_ONLY, 20, 10)
    assert blocker is not None
    assert blocker.code is BlockerCode.METRIC_NOT_MEASURABLE
    assert "Slice 18" in blocker.message
    assert "`limit`" in blocker.message
    assert str(ENGINE_MAX_RESULT_LIMIT) in blocker.message
    for retired in RETIRED_WORDS:
        assert retired not in blocker.message


def test_hybrid_deep_k_resolves_when_the_limit_covers_it() -> None:
    """Pinned per AC-3: limit=50, k=50, hybrid resolves -- the depth the old
    doctrine refused unconditionally."""
    doc = _config()
    doc["scenario"]["query"]["limit"] = 50
    doc["metrics"]["evidence_recall_k"] = [5, 50]
    result = resolve_config(doc)
    assert result.blockers == ()
    assert result.scenario is not None
    assert result.scenario.retrieval_mode is RetrievalMode.HYBRID


# --- AC-4: identity -----------------------------------------------------------


def test_limitless_config_resolves_to_the_engine_default() -> None:
    result = resolve_config(_config())
    assert result.blockers == ()
    assert result.scenario is not None
    assert result.scenario.max_measurable_k == ENGINE_DEFAULT_RESULT_LIMIT


def test_limitless_config_hash_does_not_move() -> None:
    result = resolve_config(_config())
    assert result.scenario is not None
    assert result.scenario.config_sha256 == LIMITLESS_CONFIG_SHA256


def test_resolved_limit_is_injected_into_query_params_exactly_once() -> None:
    """The resolver injects the resolved limit into query_params -- the single
    source the runner passes through -- whether or not the config declared it,
    so no duplicate-kwarg path exists."""
    absent = resolve_config(_config())
    assert absent.scenario is not None
    assert absent.scenario.query_params["limit"] == ENGINE_DEFAULT_RESULT_LIMIT

    doc = _config()
    doc["scenario"]["query"]["limit"] = 50
    doc["metrics"]["evidence_recall_k"] = [5, 50]
    declared = resolve_config(doc)
    assert declared.scenario is not None
    assert declared.scenario.query_params["limit"] == 50


# --- AC-5: the runner demonstrably passes the limit ---------------------------


def test_runner_passes_the_limit_witnessed_by_exact_cardinality(tmp_path: Path) -> None:
    """A fixture with more than 3 matching documents searched with limit=3
    returns EXACTLY 3 hits, and the sidecar records fanout_used == 3."""
    pytest.importorskip("fathomdb._fathomdb", reason="native binding not built")
    from eval.earp.runner import run_diagnostic  # noqa: PLC0415

    fixture = tmp_path / "fixture.jsonl"
    items = [
        {
            "kind": "doc",
            "body": f"deal memo number {n}",
            "source_id": "s1",
            "logical_id": f"doc-{n}",
        }
        for n in range(5)
    ]
    fixture.write_text(
        "".join(json.dumps(item) + "\n" for item in items), encoding="utf-8"
    )
    doc = {
        "schema_version": "earp.v1",
        "campaign": "diagnostic",
        "scenario": {
            "fixture": str(fixture),
            "query": {"call": "Engine.search_text_only", "text": "deal", "limit": 3},
        },
    }
    resolution = resolve_config(doc)
    assert resolution.blockers == (), resolution.blockers
    assert resolution.scenario is not None
    result = run_diagnostic(
        scenario=resolution.scenario,
        config_doc=doc,
        experiments_root=tmp_path / "experiments",
        experiment="earp-limit-adoption",
        ts=TS,
    )
    assert result.verdict is RunVerdict.COMPLETE
    assert len(result.hit_doc_ids) == 3
    assert result.run_dir is not None
    sidecar = json.loads((result.run_dir / "earp.result.v1.json").read_text(encoding="utf-8"))
    assert sidecar["scenario"]["fanout_used"] == 3


# --- characterize: explicit limit, refused deep ladders -----------------------


def _characterization_bed(tmp_path: Path, ladder: tuple[int, ...]) -> dict[str, Any]:
    docs = [
        {"doc_id": "d1", "body": "the deal sheet is missing for March", "source_type": "email"},
        {"doc_id": "d2", "body": "parking arrangements for the meeting", "source_type": "note"},
        {"doc_id": "d3", "body": "revenue rose after the deal closed", "source_type": "article"},
    ]
    raw = tmp_path / "raw"
    raw.mkdir(exist_ok=True)
    shard = raw / "synthetic_notes.jsonl"
    shard.write_text("".join(json.dumps(d) + "\n" for d in docs), encoding="utf-8")
    snapshot = tmp_path / "snapshot.json"
    snapshot.write_text(
        json.dumps(
            {
                "corpus_hash": "c" * 64,
                "total_docs": len(docs),
                "per_source_sha256": [
                    {
                        "source": "synthetic_notes",
                        "sha256": hashlib.sha256(shard.read_bytes()).hexdigest(),
                        "doc_count": len(docs),
                    }
                ],
            }
        ),
        encoding="utf-8",
    )
    gold = tmp_path / "gold.json"
    gold.write_text(
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
                    }
                ],
            }
        ),
        encoding="utf-8",
    )
    return {
        "data_root": tmp_path,
        "snapshot_path": snapshot,
        "gold_path": gold,
        "gold_sha256": hashlib.sha256(gold.read_bytes()).hexdigest(),
        "corpus_hash": "c" * 64,
        "qrels_version": "ir-c-reused-v2",
        "experiments_root": tmp_path / "experiments",
        "experiment": "earp-characterization",
        "ts": TS,
        "evidence_recall_k": ladder,
    }


def test_characterize_refuses_a_ladder_beyond_the_engine_maximum(tmp_path: Path) -> None:
    """Today characterize() truncates to max(ladder) while calling with the
    engine default -- a (5, 150) ladder would silently score @150 over 10
    hits. Now it is a typed refusal: K > 100 is permanently unmeasurable."""
    pytest.importorskip("fathomdb._fathomdb", reason="native binding not built")
    from eval.earp.characterize import run_characterization  # noqa: PLC0415

    result = run_characterization(**_characterization_bed(tmp_path, (5, 150)))
    assert result.verdict is RunVerdict.BLOCKED
    assert result.blockers[0].code is BlockerCode.METRIC_NOT_MEASURABLE
    for retired in RETIRED_WORDS:
        assert retired not in result.blockers[0].message


def test_characterize_records_the_default_ladder_fanout(tmp_path: Path) -> None:
    pytest.importorskip("fathomdb._fathomdb", reason="native binding not built")
    from eval.earp.characterize import run_characterization  # noqa: PLC0415

    result = run_characterization(**_characterization_bed(tmp_path, (5, 10)))
    assert result.verdict is RunVerdict.COMPLETE
    assert result.fanout_used == 10
    assert result.run_dir is not None
    sidecar = json.loads((result.run_dir / "earp.result.v1.json").read_text(encoding="utf-8"))
    assert sidecar["scenario"]["fanout_used"] == 10


def test_characterize_deep_ladder_records_its_max(tmp_path: Path) -> None:
    """A (5, 50) ladder now really calls the engine with limit=50 and records
    50 -- not the old silent truncation to a 10-hit page."""
    pytest.importorskip("fathomdb._fathomdb", reason="native binding not built")
    from eval.earp.characterize import run_characterization  # noqa: PLC0415

    result = run_characterization(**_characterization_bed(tmp_path, (5, 50)))
    assert result.verdict is RunVerdict.COMPLETE
    assert result.fanout_used == 50
    assert result.per_k[50].overall.strict() == 1.0
    assert result.run_dir is not None
    sidecar = json.loads((result.run_dir / "earp.result.v1.json").read_text(encoding="utf-8"))
    assert sidecar["scenario"]["fanout_used"] == 50
