"""S4 writer tests — written RED, before `eval.earp.writer` exists.

Every test threads an `experiments_root` into tmp_path. `_lib.write_record`
defaults `base_dir` to the real `experiments/` directory and `regen_index_md`
defaults `md_path` to the committed `INDEX.md`, so a forgotten parameter would
write into the repo — and a half-passed one would overwrite `INDEX.md` from a
tmp index.
"""

from __future__ import annotations

import json
import subprocess
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any

import pytest

from eval.earp._experiments import lib as _lib
from eval.earp.config import resolve_config
from eval.earp.schema.models import BlockerCode, RunVerdict
from eval.earp.schema.validate import UnsupportedSchema, assert_supported
from eval.earp.writer import WriteOutcome, write_run

TS = datetime(2026, 8, 6, 12, 0, tzinfo=timezone.utc)
SHA = "a" * 64


def _metrics(n: int) -> dict[str, Any]:
    """A schema-valid per-K aggregate. The `n` differs between runs, which is
    what makes two same-config runs byte-different sidecars."""
    return {
        "per_k": {
            "10": {
                "n": n,
                "strict_evidence_recall": {"status": "emitted", "value": 0.5},
                "graded_evidence_recall": {"status": "emitted", "value": 0.5},
            }
        }
    }


def _config(**over: Any) -> dict[str, Any]:
    doc: dict[str, Any] = {
        "schema_version": "earp.v1",
        "campaign": "diagnostic",
        "scenario": {"query": {"call": "Engine.search_text_only"}},
    }
    doc.update(over)
    return doc


def _sidecar(run_id: str, **over: Any) -> dict[str, Any]:
    doc: dict[str, Any] = {
        "schema_version": "earp.result.v1",
        "run_id": run_id,
        "campaign": "diagnostic",
        "verdict": "complete",
        "scenario": {"config_sha256": SHA, "query_call": "Engine.search_text_only"},
        "witnesses": [],
        "blockers": [],
    }
    doc.update(over)
    return doc


def _write(root: Path, doc: dict[str, Any], **over: Any) -> WriteOutcome:
    kwargs: dict[str, Any] = {
        "experiment": "earp-diagnostic",
        "ts": TS,
        "config_doc": doc,
        "experiments_root": root,
        "verdict": RunVerdict.COMPLETE,
        "read": "a synthetic run",
        "metrics": {"per_k": {}},
        "per_query": [],
        "code": {"git_sha": "0" * 40, "dirty": False, "branch": "t", "baseline_commit": None},
        "env": {"python": "3.12.3", "lockfile_sha256": None, "gpu": None, "key_deps": {}},
        "corpus": {"source": None, "manifest_sha256": None, "datasets": []},
        "seeds": {},
        "cost_usd": 0.0,
    }
    kwargs.update(over)
    sidecar = kwargs.pop("sidecar", None)
    if sidecar is not None:
        kwargs["sidecar"] = sidecar
    return write_run(**kwargs)


# --- AC-3: the identities agree --------------------------------------------


def test_pre_derived_run_id_matches_write_record(tmp_path: Path) -> None:
    doc = _config()
    outcome = _write(tmp_path, doc)
    expected = _lib.make_run_id("earp-diagnostic", TS, _lib.config_sha256(dict(doc)))
    assert outcome.run_id == expected
    assert outcome.run_dir == tmp_path / "runs" / expected
    assert outcome.run_dir.is_dir()


# --- AC-2: one canonicalisation, pinned by non-ASCII ------------------------


def test_resolver_hash_matches_lib_on_non_ascii() -> None:
    """All-ASCII configs agree even with two implementations, so only a
    non-ASCII config can tell them apart."""
    doc = _config()
    doc["scenario"]["query"] = {
        "call": "Engine.search_projected_text",
        "projection_name": "café",
    }
    resolution = resolve_config(doc)
    assert resolution.scenario is not None
    assert resolution.scenario.config_sha256 == _lib.config_sha256(dict(doc))


def test_resolve_config_accepts_a_non_dict_mapping() -> None:
    """_lib._resolved_dict raises TypeError on a Mapping that is not a dict,
    and resolve_config documents that it returns rather than raises."""
    from types import MappingProxyType  # noqa: PLC0415

    result = resolve_config(MappingProxyType(_config()))
    assert result.scenario is not None


# --- AC-1: the sidecar precedes the index line ------------------------------


def test_sidecar_exists_before_the_index_line(tmp_path: Path, monkeypatch: Any) -> None:
    """write_record calls append_index by module-global lookup as its last
    statement, so spying on it observes the exact moment between materialize
    and append."""
    observed: dict[str, Any] = {}
    original = _lib.append_index

    def spy(record: dict[str, Any], *, index_path: Any = None) -> None:
        run_dir = tmp_path / "runs" / record["run_id"]
        observed["sidecar_exists"] = (run_dir / "earp.result.v1.json").is_file()
        index = Path(index_path) if index_path else None
        observed["run_id_indexed"] = bool(
            index and index.is_file() and record["run_id"] in index.read_text()
        )
        original(record, index_path=index_path)

    monkeypatch.setattr(_lib, "append_index", spy)
    _write(tmp_path, _config())
    assert observed["sidecar_exists"] is True
    assert observed["run_id_indexed"] is False


# --- AC-4: the collision that actually happens ------------------------------


def test_same_config_remeasured_in_one_minute_is_a_collision(tmp_path: Path) -> None:
    """The hashes are equal by construction, so keying on config_sha256 would
    miss this entirely. The sidecars differ because the measurements differ."""
    doc = _config()
    first = _write(tmp_path, doc, metrics=_metrics(1))
    assert first.blocker is None
    second = _write(tmp_path, doc, metrics=_metrics(2))
    assert second.blocker is not None
    assert second.blocker.code is BlockerCode.RUN_ID_COLLISION


def test_byte_identical_rewrite_is_idempotent(tmp_path: Path) -> None:
    doc = _config()
    first = _write(tmp_path, doc)
    second = _write(tmp_path, doc)
    assert second.blocker is None
    assert second.run_id == first.run_id
    index_lines = (tmp_path / "index.jsonl").read_text().strip().splitlines()
    assert len(index_lines) == 1


def test_collision_writes_nothing_and_is_not_indexed(tmp_path: Path) -> None:
    """A collision-blocked run is the one blocked run that cannot be indexed:
    its run_id is already taken."""
    doc = _config()
    _write(tmp_path, doc, metrics=_metrics(1))
    before = (tmp_path / "index.jsonl").read_text()
    outcome = _write(tmp_path, doc, metrics=_metrics(2))
    assert outcome.run_dir is None
    assert (tmp_path / "index.jsonl").read_text() == before


def test_a_different_minute_does_not_collide(tmp_path: Path) -> None:
    doc = _config()
    _write(tmp_path, doc)
    outcome = _write(tmp_path, doc, ts=TS + timedelta(minutes=1))
    assert outcome.blocker is None


# --- AC-5/6: verdicts -------------------------------------------------------


def test_blocked_run_is_indexed_with_a_blocked_verdict(tmp_path: Path) -> None:
    outcome = _write(
        tmp_path,
        _config(),
        verdict=RunVerdict.BLOCKED,
        sidecar_blockers=[
            {"code": "gold_missing", "message": "no gold", "stage": "gold.resolve"}
        ],
    )
    assert outcome.blocker is None
    row = json.loads((tmp_path / "index.jsonl").read_text().strip())
    assert row["verdict"] == "blocked"
    sidecar = json.loads((outcome.run_dir / "earp.result.v1.json").read_text())
    assert sidecar["verdict"] == "blocked"
    assert sidecar["blockers"]


def test_unpinned_verdict_is_refused_before_any_write(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="verdict"):
        _write(tmp_path, _config(), verdict="done")
    assert not (tmp_path / "runs").exists()
    assert not (tmp_path / "index.jsonl").exists()


# --- AC-7/12/13: refusals that leave nothing behind -------------------------


def test_invalid_sidecar_prevents_directory_creation(tmp_path: Path) -> None:
    with pytest.raises(ValueError):
        _write(tmp_path, _config(), sidecar={"schema_version": "earp.result.v1"})
    assert not (tmp_path / "runs").exists()


def test_resolved_scenario_as_config_is_refused(tmp_path: Path) -> None:
    """Passing ResolvedScenario hashes differently AND fails destructively:
    record.json writes, then yaml.safe_dump raises on the enum, leaving a
    half-materialized directory."""
    resolution = resolve_config(_config())
    with pytest.raises(TypeError, match="raw config"):
        _write(tmp_path, _config(), config_doc=resolution.scenario)
    assert not (tmp_path / "runs").exists()


def test_naive_timestamp_is_refused(tmp_path: Path) -> None:
    """_ts_compact stamps a literal Z without converting, so a naive ts yields
    a run_id that lies about its timezone."""
    with pytest.raises(ValueError, match="UTC"):
        _write(tmp_path, _config(), ts=datetime(2026, 8, 6, 12, 0))
    assert not (tmp_path / "runs").exists()


def test_config_mutation_after_derivation_cannot_move_the_directory(
    tmp_path: Path,
) -> None:
    """_resolved_dict returns the same object rather than a copy."""
    doc = _config()
    outcome = _write(tmp_path, doc)
    doc["campaign"] = "characterization"
    assert outcome.run_dir.is_dir()


# --- AC-8/9: the walker covers all three schemas ---------------------------


def test_assert_supported_passes_on_every_schema() -> None:
    from eval.earp.schema import (  # noqa: PLC0415
        CONFIG_SCHEMA_PATH,
        PER_QUERY_SCHEMA_PATH,
        RESULT_SCHEMA_PATH,
    )

    for path in (CONFIG_SCHEMA_PATH, RESULT_SCHEMA_PATH, PER_QUERY_SCHEMA_PATH):
        assert_supported(json.loads(path.read_text(encoding="utf-8")))


def test_unknown_keyword_still_raises() -> None:
    with pytest.raises(UnsupportedSchema):
        assert_supported({"type": "object", "allOff": []})


def test_union_typed_slot_rejects_a_wrong_type(tmp_path: Path) -> None:
    """`["string","null"]`-style unions appear 20 times across the two sidecar
    schemas and currently accept anything at all."""
    from eval.earp.schema.validate import validate  # noqa: PLC0415

    schema = {"type": "object", "properties": {"x": {"type": ["string", "null"]}}}
    assert validate({"x": "ok"}, schema) == []
    assert validate({"x": None}, schema) == []
    assert validate({"x": {"a": 1}}, schema) != []


def test_per_query_conditional_requirements_are_enforced(tmp_path: Path) -> None:
    """if/then is what makes `outcome: scored` carry numbers; without it the
    per-query file is silently under-validated."""
    from eval.earp.schema import PER_QUERY_SCHEMA_PATH  # noqa: PLC0415
    from eval.earp.schema.validate import validate  # noqa: PLC0415

    schema = json.loads(PER_QUERY_SCHEMA_PATH.read_text(encoding="utf-8"))
    scored_without_numbers = {
        "schema_version": "earp.per-query.v1",
        "query_id": "q",
        "k": 10,
        "outcome": "scored",
    }
    assert validate(scored_without_numbers, schema) != []
    blocked_without_reason = {
        "schema_version": "earp.per-query.v1",
        "query_id": "q",
        "k": 10,
        "outcome": "blocked",
    }
    assert validate(blocked_without_reason, schema) != []


def test_per_k_aggregates_are_validated(tmp_path: Path) -> None:
    """additionalProperties-as-schema: without it every per-K aggregate — the
    highest-value part of the sidecar — is unvalidated."""
    from eval.earp.schema import RESULT_SCHEMA_PATH  # noqa: PLC0415
    from eval.earp.schema.validate import validate  # noqa: PLC0415

    schema = json.loads(RESULT_SCHEMA_PATH.read_text(encoding="utf-8"))
    doc = _sidecar("r")
    doc["metrics"] = {"per_k": {"10": {"n": "not-an-integer"}}}
    assert validate(doc, schema) != []


# --- AC-10/11: the boundaries -----------------------------------------------


def test_nothing_is_written_into_the_real_experiments_tree(tmp_path: Path) -> None:
    real_index = Path(_lib.INDEX_PATH)
    before = real_index.read_text(encoding="utf-8") if real_index.is_file() else None
    _write(tmp_path, _config())
    after = real_index.read_text(encoding="utf-8") if real_index.is_file() else None
    assert after == before
    assert (tmp_path / "index.jsonl").is_file()


def test_index_md_is_regenerated_into_the_given_root(tmp_path: Path) -> None:
    _write(tmp_path, _config())
    assert (tmp_path / "INDEX.md").is_file()


def test_lib_imports_from_a_foreign_cwd(tmp_path: Path) -> None:
    """pytest's pythonpath adds only src/python, and the repo root resolves
    today only by accident of running with `python -m` from the root."""
    source_root = Path(__file__).parents[2]
    proc = subprocess.run(
        [sys.executable, "-c", "from eval.earp._experiments import lib; print(lib.__name__)"],
        cwd=tmp_path,
        env={"PYTHONPATH": str(source_root), "PATH": "/usr/bin:/bin"},
        capture_output=True,
        text=True,
        timeout=60,
        check=False,
    )
    assert proc.returncode == 0, proc.stderr
    assert "_lib" in proc.stdout
