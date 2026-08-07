"""S5 diagnostic-runner tests — written RED, before `eval.earp.runner` exists.

These open a REAL engine. They skip visibly when the native binding is absent,
never silently.
"""

from __future__ import annotations

import json
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import pytest

from eval.earp.config import resolve_config
from eval.earp.schema.models import (
    BlockerCode,
    RunVerdict,
    WitnessSource,
    WitnessStatus,
)

pytest.importorskip("fathomdb._fathomdb", reason="native binding not built")

from eval.earp.runner import (  # noqa: E402
    classify_open,
    load_fixture,
    run_diagnostic,
)

TS = datetime(2026, 8, 6, 12, 0, tzinfo=timezone.utc)

FIXTURE = [
    {"kind": "doc", "body": "the deal sheet is missing for March", "source_id": "s1", "logical_id": "doc-a"},
    {"kind": "doc", "body": "unrelated meeting notes about parking", "source_id": "s1", "logical_id": "doc-b"},
]


def _fixture_file(tmp_path: Path, items: list[dict[str, Any]] | None = None) -> Path:
    path = tmp_path / "fixture.jsonl"
    path.write_text(
        "".join(json.dumps(item) + "\n" for item in (FIXTURE if items is None else items)),
        encoding="utf-8",
    )
    return path


def _config(fixture: Path, **query: Any) -> dict[str, Any]:
    q = {"call": "Engine.search_text_only", "text": "deal sheet"}
    q.update(query)
    return {
        "schema_version": "earp.v1",
        "campaign": "diagnostic",
        "scenario": {"fixture": str(fixture), "query": q},
    }


def _run(tmp_path: Path, doc: dict[str, Any], **over: Any) -> Any:
    resolution = resolve_config(doc)
    assert resolution.blockers == (), resolution.blockers
    kwargs: dict[str, Any] = {
        "scenario": resolution.scenario,
        "config_doc": doc,
        "experiments_root": tmp_path / "experiments",
        "experiment": "earp-diagnostic",
        "ts": TS,
    }
    kwargs.update(over)
    return run_diagnostic(**kwargs)


# --- AC-1: end to end against a real engine --------------------------------


def test_diagnostic_run_completes(tmp_path: Path) -> None:
    result = _run(tmp_path, _config(_fixture_file(tmp_path)))
    assert result.verdict is RunVerdict.COMPLETE
    assert result.blockers == ()
    assert result.hit_doc_ids == ["doc-a"]
    sidecar = json.loads((result.run_dir / "earp.result.v1.json").read_text())
    assert sidecar["campaign"] == "diagnostic"
    assert sidecar["verdict"] == "complete"


def test_witnesses_carry_real_sources(tmp_path: Path) -> None:
    result = _run(tmp_path, _config(_fixture_file(tmp_path)))
    by_name = {w.name: w for w in result.witnesses}
    assert by_name["open_report"].source is WitnessSource.OPEN_REPORT
    assert by_name["open_report"].call_path == "Engine.open_report"
    assert by_name["write_receipt"].source is WitnessSource.WRITE_RECEIPT
    assert by_name["fixture_landed"].source is WitnessSource.STORE_QUERY
    assert by_name["projection_coverage"].source is WitnessSource.READ_PROJECTIONS
    assert by_name["search_returned"].source is WitnessSource.SEARCH_RESULT
    for witness in result.witnesses:
        assert witness.status is WitnessStatus.OBSERVED
        assert not isinstance(witness.value, bool), witness.name


def test_open_report_witness_records_the_schema_migration(tmp_path: Path) -> None:
    result = _run(tmp_path, _config(_fixture_file(tmp_path)))
    value = {w.name: w.value for w in result.witnesses}["open_report"]
    assert value["schema_version_before"] == 0
    assert value["schema_version_after"] > 0
    assert value["query_backend"]


# --- AC-2/M-3: fixture preconditions ---------------------------------------


def test_missing_fixture_is_a_typed_blocker(tmp_path: Path) -> None:
    result = _run(tmp_path, _config(tmp_path / "absent.jsonl"))
    assert result.verdict is RunVerdict.BLOCKED
    assert result.blockers[0].code is BlockerCode.FIXTURE_MISSING


def test_item_without_logical_id_is_refused_before_the_write(tmp_path: Path) -> None:
    bad = [{"kind": "doc", "body": "x", "source_id": "s1"}]
    with pytest.raises(ValueError, match="logical_id"):
        load_fixture(_fixture_file(tmp_path, bad))


def test_null_body_is_refused_before_the_write(tmp_path: Path) -> None:
    """The engine ACCEPTS a null body, storing it as '{}' and invisible to FTS,
    behind a healthy-looking receipt. Only a precondition catches it."""
    bad = [{"kind": "doc", "body": None, "source_id": "s1", "logical_id": "doc-a"}]
    with pytest.raises(ValueError, match="body"):
        load_fixture(_fixture_file(tmp_path, bad))


def test_non_string_body_is_refused_before_the_write(tmp_path: Path) -> None:
    """A dict body raises a misleading lone-surrogate UTF-8 error from the
    engine, so the precondition names the real problem instead."""
    bad = [{"kind": "doc", "body": {"text": "x"}, "source_id": "s1", "logical_id": "doc-a"}]
    with pytest.raises(ValueError, match="body"):
        load_fixture(_fixture_file(tmp_path, bad))


def test_duplicate_logical_id_is_refused(tmp_path: Path) -> None:
    """A duplicate silently SUPERSEDES the first write -- one active row, no
    error, no signal in the receipt. For S6 that would be an unattributable
    recall loss."""
    dup = [
        {"kind": "doc", "body": "first", "source_id": "s1", "logical_id": "doc-a"},
        {"kind": "doc", "body": "second", "source_id": "s1", "logical_id": "doc-a"},
    ]
    with pytest.raises(ValueError, match="duplicate"):
        load_fixture(_fixture_file(tmp_path, dup))


def test_missing_source_id_is_refused(tmp_path: Path) -> None:
    bad = [{"kind": "doc", "body": "x", "logical_id": "doc-a"}]
    with pytest.raises(ValueError, match="source_id"):
        load_fixture(_fixture_file(tmp_path, bad))


def test_fixture_landing_is_verified_by_round_trip(tmp_path: Path) -> None:
    """WriteReceipt carries only counters, so it cannot tell a landed fixture
    from a silently-empty one. The witness is a real read-back."""
    result = _run(tmp_path, _config(_fixture_file(tmp_path)))
    landed = {w.name: w.value for w in result.witnesses}["fixture_landed"]
    assert sorted(landed["logical_ids"]) == ["doc-a", "doc-b"]
    assert landed["expected"] == 2


# --- AC-4: failures are typed, never misses --------------------------------


def test_sdk_exception_yields_a_failed_verdict(tmp_path: Path) -> None:
    doc = _config(_fixture_file(tmp_path))
    result = _run(tmp_path, doc, query_override=_boom)
    assert result.verdict is RunVerdict.FAILED
    assert result.failure is not None
    assert "RuntimeError" in result.failure


def _boom(*_args: Any, **_kwargs: Any) -> Any:
    raise RuntimeError("search exploded")


# --- AC-5: no metric can appear --------------------------------------------


def test_diagnostic_result_carries_no_metrics(tmp_path: Path) -> None:
    result = _run(tmp_path, _config(_fixture_file(tmp_path)))
    sidecar = json.loads((result.run_dir / "earp.result.v1.json").read_text())
    assert sidecar.get("metrics", {}) == {}


@pytest.mark.parametrize(
    "metrics",
    [
        {"evidence_recall_k": [5, 10]},
        {"document_metrics": ["mrr"]},
        {"integrity": ["provenance"]},
    ],
)
def test_diagnostic_config_refuses_every_metric_key(
    tmp_path: Path, metrics: dict[str, Any]
) -> None:
    """The guarantee is enforced at config time, not merely asserted: refusing
    only evidence_recall_k left document_metrics and integrity flowing through."""
    doc = _config(_fixture_file(tmp_path))
    doc["metrics"] = metrics
    resolution = resolve_config(doc)
    assert any(
        b.code is BlockerCode.CONFIG_INAPPLICABLE_KNOB for b in resolution.blockers
    ), resolution.blockers


# --- AC-6: the open classification is a pure function ----------------------


def test_fetched_embedder_is_blocked_without_a_network_fetch() -> None:
    """A real fetch is forbidden by policy, so the decision is factored out and
    driven by a synthetic report -- otherwise this path is untestable."""
    witnesses, blockers = classify_open(
        {"embedder_download_ms": 1234, "dense_disabled": False, "embedder_events": []}
    )
    assert any(b.code is BlockerCode.EMBEDDER_FETCHED for b in blockers)
    assert witnesses


def test_cache_hit_is_not_blocked() -> None:
    _witnesses, blockers = classify_open(
        {
            "embedder_download_ms": None,
            "dense_disabled": False,
            "embedder_events": [{"kind": "DefaultEmbedderCacheHit"}],
        }
    )
    assert blockers == ()


def test_dense_disabled_is_blocked() -> None:
    """S7 amendment: `dense_disabled` blocks only when the scenario DECLARED a
    dense projection (matching earp.md's "typed blocker when dense retrieval
    was required"); otherwise the degraded open is witness-recorded, not
    blocking -- the witness value carries the full report either way."""
    report = {"embedder_download_ms": None, "dense_disabled": True, "embedder_events": []}
    _witnesses, blockers = classify_open(report, dense_required=True)
    assert any(b.code is BlockerCode.DENSE_DISABLED for b in blockers)

    witnesses, blockers = classify_open(report, dense_required=False)
    assert not any(b.code is BlockerCode.DENSE_DISABLED for b in blockers)
    assert {w.name: w.value for w in witnesses}["open_report"]["dense_disabled"] is True


# --- AC-8: lifecycle --------------------------------------------------------


def test_database_directory_is_removed(tmp_path: Path) -> None:
    """close() leaves a .lock sidecar behind, so per-file deletion is wrong --
    the whole temp directory goes."""
    result = _run(tmp_path, _config(_fixture_file(tmp_path)))
    assert result.db_dir is not None
    assert not Path(result.db_dir).exists()


def test_database_directory_is_removed_on_failure(tmp_path: Path) -> None:
    result = _run(tmp_path, _config(_fixture_file(tmp_path)), query_override=_boom)
    assert not Path(result.db_dir).exists()


# --- AC-9 / N-12: zero hits is complete, and distinguishable ----------------


def test_zero_hits_is_complete_not_failed(tmp_path: Path) -> None:
    """A diagnostic makes no relevance claim, so zero hits is not a failure --
    but it is also the signature of a silently-broken fixture, which is why the
    landing witness exists alongside it."""
    doc = _config(_fixture_file(tmp_path), text="zzzznomatchzzzz")
    result = _run(tmp_path, doc)
    assert result.verdict is RunVerdict.COMPLETE
    assert result.hit_doc_ids == []
    landed = {w.name: w.value for w in result.witnesses}["fixture_landed"]
    assert landed["found"] == 2


def test_blocked_run_is_written_with_a_blocked_verdict(tmp_path: Path) -> None:
    result = _run(tmp_path, _config(tmp_path / "absent.jsonl"))
    sidecar = json.loads((result.run_dir / "earp.result.v1.json").read_text())
    assert sidecar["verdict"] == "blocked"
    assert sidecar["blockers"][0]["code"] == "fixture_missing"
