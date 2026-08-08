"""S7 — store/projection/query matrix + readiness witnesses. Written RED.

Three projection-state signals from three different APIs, never conflated and
never converted into an empty retrieval result:

* `vector_dense_readiness` — polled from `read.projections()`;
* `vector_unsupported_kinds` — only on the `ProjectionDelta` that
  `configure_projections` returns;
* `dense_disabled` — only on `open_report`, with the refusal count read
  separately from `Engine.vector_equivalence_refusal_count()`.

Design of record: `dev/design/earp-slice-7-design.md`.
"""

from __future__ import annotations

import dataclasses
import json
import os
import typing
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import pytest

from eval.earp.config import CONSUMER_REGISTRY, resolve_config
from eval.earp.knobs import CATALOG_BY_NAME
from eval.earp.schema.models import (
    BlockerCode,
    DeclaredProjection,
    KnobClass,
    ProjectionWitnesses,
    RunVerdict,
    WitnessSource,
)

TS = datetime(2026, 8, 7, 12, 0, tzinfo=timezone.utc)

#: Projected-text search reads a top-level object member of the canonical body
#: (`ProjectionSpec.source is None` = legacy direct lookup by name), so the
#: fixture bodies are JSON OBJECTS carrying a `title` member -- a plain string
#: body projects nothing (verified by execution, 2026-08-07).
PROJECTED_FIXTURE = [
    {
        "kind": "doc",
        "body": json.dumps({"title": "the deal sheet is missing for March"}),
        "source_id": "s1",
        "logical_id": "doc-a",
    },
    {
        "kind": "doc",
        "body": json.dumps({"title": "unrelated meeting notes about parking"}),
        "source_id": "s1",
        "logical_id": "doc-b",
    },
]


def _fixture_file(tmp_path: Path) -> Path:
    path = tmp_path / "fixture.jsonl"
    path.write_text(
        "".join(json.dumps(item) + "\n" for item in PROJECTED_FIXTURE), encoding="utf-8"
    )
    return path


def _declare(**over: Any) -> dict[str, Any]:
    spec: dict[str, Any] = {"name": "title", "roles": ["searchable"], "fts": True}
    spec.update(over)
    return spec


def _config(fixture: Path | str = "fixture.jsonl", **over: Any) -> dict[str, Any]:
    doc: dict[str, Any] = {
        "schema_version": "earp.v1",
        "campaign": "diagnostic",
        "scenario": {
            "fixture": str(fixture),
            "projections": {"declare": [_declare()]},
            "query": {
                "call": "Engine.search_projected_text",
                "projection_name": "title",
                "text": "deal sheet",
            },
        },
    }
    for key, value in over.items():
        if value is None:
            doc["scenario"].pop(key, None)
        else:
            doc["scenario"][key] = value
    return doc


def _codes(result: Any) -> set[BlockerCode]:
    return {b.code for b in result.blockers}


# --- 1. schema: the projections block ---------------------------------------


def test_projections_block_resolves_and_is_carried_into_the_scenario() -> None:
    result = resolve_config(_config())
    assert result.blockers == ()
    assert result.scenario is not None
    assert result.scenario.projections == (
        DeclaredProjection(name="title", roles=("searchable",), fts=True, vector=False),
    )
    assert result.scenario.readiness_timeout_s == 30.0


def test_declared_readiness_timeout_is_resolved() -> None:
    doc = _config()
    doc["scenario"]["projections"]["readiness_timeout_s"] = 5
    result = resolve_config(doc)
    assert result.blockers == ()
    assert result.scenario is not None
    assert result.scenario.readiness_timeout_s == 5.0


def test_unknown_key_inside_the_projections_block_is_refused() -> None:
    doc = _config()
    doc["scenario"]["projections"]["bogus"] = 1
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_UNKNOWN_KEY in _codes(result)


def test_unknown_key_inside_a_declared_spec_is_refused() -> None:
    doc = _config()
    doc["scenario"]["projections"]["declare"] = [_declare(tokenizer="porter")]
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_UNKNOWN_KEY in _codes(result)


def test_roles_enum_is_enforced() -> None:
    doc = _config()
    doc["scenario"]["projections"]["declare"] = [_declare(roles=["searchable", "sortable"])]
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)


def test_empty_declare_array_is_refused() -> None:
    doc = _config()
    doc["scenario"]["projections"]["declare"] = []
    result = resolve_config(doc)
    assert result.blockers != ()


@pytest.mark.parametrize("timeout", [0.05, 301])
def test_readiness_timeout_window_is_enforced(timeout: float) -> None:
    """`minimum: 0.1` rather than `exclusiveMinimum: 0`: the stdlib walker does
    not interpret exclusiveMinimum, so the window is closed at 0.1."""
    doc = _config()
    doc["scenario"]["projections"]["readiness_timeout_s"] = timeout
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)


# --- 2. resolver: matrix coherence -------------------------------------------


def test_undeclared_projection_name_is_a_config_error_naming_the_declared_set() -> None:
    doc = _config()
    doc["scenario"]["query"]["projection_name"] = "body"
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)
    offending = [b for b in result.blockers if b.code is BlockerCode.CONFIG_INVALID_VALUE]
    assert any("title" in b.message for b in offending), offending


def test_projected_text_without_a_projections_block_is_refused() -> None:
    """Owned behaviour change: today such a config resolves and dies at run
    time with InvalidFilterError on EVERY run (fresh DB per scenario), so the
    error moves to resolution rather than twenty minutes into a run."""
    doc = _config(projections=None)
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)


def test_vector_without_the_default_embedder_is_refused_at_config_time() -> None:
    """With no embedder the engine reports the dense sub-target `unavailable`
    (0.8.22 Slice 21), so the config declares a dense arm the run can never
    exercise -- resolution refuses it up front (S10 rewrote the pre-Slice-21
    vacuous-ready story; the refusal semantics are unchanged)."""
    doc = _config()
    doc["scenario"]["projections"]["declare"] = [_declare(vector=True)]
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)


def test_vector_with_the_default_embedder_resolves() -> None:
    doc = _config(engine={"use_default_embedder": True})
    doc["scenario"]["projections"]["declare"] = [_declare(vector=True)]
    result = resolve_config(doc)
    assert result.blockers == ()
    assert result.scenario is not None
    assert result.scenario.projections[0].vector is True


def test_fts_without_searchable_role_is_refused() -> None:
    doc = _config()
    doc["scenario"]["projections"]["declare"] = [
        {"name": "title", "roles": ["filterable"], "fts": True}
    ]
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)


def test_vector_without_searchable_role_is_refused() -> None:
    doc = _config(engine={"use_default_embedder": True})
    doc["scenario"]["projections"]["declare"] = [
        _declare(),
        {"name": "extra", "roles": ["rankable"], "vector": True},
    ]
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)


def test_empty_projection_name_is_a_collected_resolver_error() -> None:
    """The walker cannot express minLength, so the resolver owns this."""
    doc = _config()
    doc["scenario"]["projections"]["declare"] = [_declare(), _declare(name="")]
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)


def test_new_projection_paths_are_registered_to_s7() -> None:
    for path in (
        "scenario.projections",
        "scenario.projections.declare",
        "scenario.projections.readiness_timeout_s",
    ):
        assert path in CONSUMER_REGISTRY, path
        assert CONSUMER_REGISTRY[path].slice_id == "S7", path


def test_catalog_owns_the_new_and_refused_knobs() -> None:
    """One call path, one entry: the pre-S7 INDEXING `configure_projections`
    entry is REPLACED by the config-facing name, and each deliberate exclusion
    is a self-documenting UNSUPPORTED entry."""
    assert "configure_projections" not in CATALOG_BY_NAME
    declare = CATALOG_BY_NAME["projections.declare"]
    assert declare.classification is KnobClass.SEMANTIC
    assert declare.call_path == "Engine.configure_projections(specs=)"
    timeout = CATALOG_BY_NAME["projections.readiness_timeout_s"]
    assert timeout.classification is KnobClass.RUNTIME
    for name in ("fts_tokenizer", "vector_embedder", "projections.drop", "projections.source"):
        entry = CATALOG_BY_NAME[name]
        assert entry.classification is KnobClass.UNSUPPORTED, name
        assert entry.reason, name


# --- 3. witness dataclass: readiness derivation ------------------------------


def test_not_declared_is_derived_from_the_vector_flag() -> None:
    """`None` means "no vector sub-target on this spec", and is never reported
    bare -- the disambiguator is the spec's own round-tripping vector flag."""
    assert ProjectionWitnesses.readiness_state(vector=False, vector_dense_readiness=None) == (
        "not_declared"
    )


def test_ready_and_embedding_pass_through() -> None:
    assert (
        ProjectionWitnesses.readiness_state(vector=True, vector_dense_readiness="ready")
        == "ready"
    )
    assert (
        ProjectionWitnesses.readiness_state(vector=True, vector_dense_readiness="embedding")
        == "embedding"
    )


def test_none_readiness_on_a_vector_spec_is_a_contract_violation() -> None:
    """read.py binding-enforces unavailable|embedding|ready|None, and None with
    vector=True is outside the engine's contract -- assert, never silently
    reported."""
    with pytest.raises(AssertionError):
        ProjectionWitnesses.readiness_state(vector=True, vector_dense_readiness=None)


def test_absent_signals_are_omitted_not_empty() -> None:
    """A sidecar reader distinguishes "not declared" from "not captured" by
    ABSENCE of configure_delta/readiness, never by an empty mapping."""
    value = ProjectionWitnesses(open_report={"dense_disabled": False}).as_value()
    assert set(value) == {"open_report"}


# --- 4. runner: end to end against a real engine -----------------------------


def _run(tmp_path: Path, doc: dict[str, Any], **over: Any) -> Any:
    pytest.importorskip("fathomdb._fathomdb", reason="native binding not built")
    from eval.earp.runner import run_diagnostic  # noqa: PLC0415

    resolution = resolve_config(doc)
    assert resolution.blockers == (), resolution.blockers
    kwargs: dict[str, Any] = {
        "scenario": resolution.scenario,
        "config_doc": doc,
        "experiments_root": tmp_path / "experiments",
        "experiment": "earp-s7",
        "ts": TS,
    }
    kwargs.update(over)
    return run_diagnostic(**kwargs)


def _sidecar(result: Any) -> dict[str, Any]:
    return json.loads((result.run_dir / "earp.result.v1.json").read_text(encoding="utf-8"))


def test_declared_projection_queries_end_to_end(tmp_path: Path) -> None:
    result = _run(tmp_path, _config(_fixture_file(tmp_path)))
    assert result.verdict is RunVerdict.COMPLETE, (result.failure, result.blockers)
    assert result.blockers == ()
    assert result.hit_doc_ids == ["doc-a"]


def test_delta_witness_is_recorded_verbatim_from_its_true_source(tmp_path: Path) -> None:
    """The delta appears exactly as configure_projections returned it --
    including the non-disjoint built/deferred lists (engine fact 1)."""
    result = _run(tmp_path, _config(_fixture_file(tmp_path)))
    by_name = {w.name: w for w in result.witnesses}
    delta = by_name["projection_delta"]
    assert delta.source is WitnessSource.PROJECTION_DELTA
    assert delta.call_path == "Engine.configure_projections"
    assert delta.value == {
        "built": ["title"],
        "dropped": [],
        "deferred": [],
        "unchanged": False,
        "vector_unsupported_kinds": [],
    }
    readiness = by_name["projection_readiness"]
    assert readiness.source is WitnessSource.READ_PROJECTIONS
    assert readiness.call_path == "fathomdb.read.projections"
    assert readiness.value == {"title": "not_declared"}


def test_sidecar_carries_the_three_signals_under_their_source_names(tmp_path: Path) -> None:
    result = _run(tmp_path, _config(_fixture_file(tmp_path)))
    witnesses = _sidecar(result)["scenario"]["projection_witnesses"]
    assert witnesses["open_report"]["dense_disabled"] is False
    assert witnesses["open_report"]["query_backend"]
    assert witnesses["open_report"]["refusal_count"] == 0
    assert witnesses["configure_delta"]["built"] == ["title"]
    assert witnesses["readiness"] == {"title": "not_declared"}


def test_projection_less_sidecar_omits_delta_and_readiness(tmp_path: Path) -> None:
    """AC-7: the open-report witness is free and always true; the other two are
    ABSENT (not empty) when nothing was declared."""
    doc = _config(_fixture_file(tmp_path), projections=None)
    doc["scenario"]["query"] = {"call": "Engine.search_text_only", "text": "deal sheet"}
    result = _run(tmp_path, doc)
    assert result.verdict is RunVerdict.COMPLETE
    witnesses = _sidecar(result)["scenario"]["projection_witnesses"]
    assert "open_report" in witnesses
    assert "configure_delta" not in witnesses
    assert "readiness" not in witnesses
    assert not any(w.name == "projection_delta" for w in result.witnesses)


def test_readiness_timeout_is_typed_and_performs_zero_real_waiting(tmp_path: Path) -> None:
    """The poll_override seam replaces BOTH the read.projections call and the
    clock (S5's query_override precedent), so no wall time passes.

    The scenario is built by dataclasses.replace: the resolver deliberately
    refuses `vector: true` without the embedder (the engine would report the
    dense sub-target `unavailable`), so the embedder-on shape is simulated the
    same way classify_open is driven by a synthetic report -- the readiness
    view is synthetic either way.
    """
    pytest.importorskip("fathomdb._fathomdb", reason="native binding not built")
    from eval.earp.runner import run_diagnostic  # noqa: PLC0415
    from fathomdb.types import ProjectionSpec  # noqa: PLC0415

    doc = _config(_fixture_file(tmp_path))
    resolution = resolve_config(doc)
    assert resolution.blockers == ()
    assert resolution.scenario is not None
    scenario = dataclasses.replace(
        resolution.scenario,
        projections=(
            DeclaredProjection(name="title", roles=("searchable",), fts=True, vector=True),
        ),
    )
    calls = 0

    def poll() -> tuple[list[ProjectionSpec], float]:
        nonlocal calls
        calls += 1
        specs = [
            ProjectionSpec(
                name="title",
                roles=frozenset({"searchable"}),
                fts=True,
                vector=True,
                vector_dense_readiness="embedding",
            )
        ]
        return specs, 0.0 if calls == 1 else scenario.readiness_timeout_s + 1.0

    result = run_diagnostic(
        scenario=scenario,
        config_doc=doc,
        experiments_root=tmp_path / "experiments",
        experiment="earp-s7",
        ts=TS,
        poll_override=poll,
    )
    assert calls == 2
    assert result.verdict is RunVerdict.BLOCKED, result.failure
    blocker = next(b for b in result.blockers if b.code is BlockerCode.DENSE_READINESS_TIMEOUT)
    assert blocker.detail["stuck"] == ["title"]
    assert _sidecar(result)["scenario"]["projection_witnesses"]["readiness"] == {
        "title": "embedding"
    }


def test_non_empty_unsupported_kinds_is_the_typed_blocker() -> None:
    """Pure over the delta mapping, exactly like classify_open over the open
    report: on the S7 order (configure BEFORE ingest, fresh DB) the corpus has
    no kinds yet, so the branch is only exercisable synthetically."""
    pytest.importorskip("fathomdb._fathomdb", reason="native binding not built")
    from eval.earp.runner import classify_delta  # noqa: PLC0415

    witness, blocker = classify_delta(
        {
            "built": ["title"],
            "dropped": [],
            "deferred": ["title"],
            "unchanged": False,
            "vector_unsupported_kinds": ["event", "task"],
        }
    )
    assert witness.source is WitnessSource.PROJECTION_DELTA
    assert blocker is not None
    assert blocker.code is BlockerCode.VECTOR_UNSUPPORTED_KINDS
    assert blocker.detail["vector_unsupported_kinds"] == ["event", "task"]

    _witness, none_blocker = classify_delta(
        {"built": [], "dropped": [], "deferred": [], "unchanged": True,
         "vector_unsupported_kinds": []}
    )
    assert none_blocker is None


# --- 5. S10: engine adoption -- `unavailable` + the status witness -----------
#
# Design of record: `dev/design/earp-slice-10-design.md`. Slice 21 gave the
# engine a THIRD readiness spelling (`unavailable`) and Slice 22 the pure
# `read.projection_status` verb; S10 adopts both and fixes the reachable
# degraded-open crash (fact 3 / AC-7).


def test_unavailable_passes_through_on_a_vector_spec() -> None:
    """Slice 21: `unavailable` is the ENGINE's value for an absent or
    equivalence-refused runtime -- passed through, never derived by EARP."""
    assert (
        ProjectionWitnesses.readiness_state(vector=True, vector_dense_readiness="unavailable")
        == "unavailable"
    )


@pytest.mark.parametrize("bad", [None, "pending", "built"])
def test_unknown_readiness_still_raises_naming_the_binding_vocabulary(bad: Any) -> None:
    """The assertion narrows to its REAL contract -- `None` or an unknown
    spelling with vector=True -- and its message names all three binding
    values, so the next vocabulary change announces itself in the failure."""
    with pytest.raises(AssertionError) as excinfo:
        ProjectionWitnesses.readiness_state(vector=True, vector_dense_readiness=bad)
    message = str(excinfo.value)
    for member in ("unavailable", "embedding", "ready"):
        assert member in message, (member, message)


def test_drift_alarm_pins_the_binding_readiness_vocabulary() -> None:
    """R4/AC-3: EARP's suite stayed green through Slice 21 because nothing
    pinned the binding set. The alarm pins EARP's HANDLING at the binding
    seam: the Literal set is exactly the three spellings, and readiness_state
    accepts EVERY member by iteration (not literals) -- a fourth value breaks
    the set equality and, until handled, the acceptance loop."""
    pytest.importorskip("fathomdb._fathomdb", reason="native binding not built")
    from fathomdb.types import DenseReadiness  # noqa: PLC0415

    members = typing.get_args(DenseReadiness)
    assert set(members) == {"unavailable", "embedding", "ready"}
    for member in members:
        assert (
            ProjectionWitnesses.readiness_state(vector=True, vector_dense_readiness=member)
            == member
        ), member


def test_projection_status_witness_value_is_json_ready() -> None:
    """Witness.value flows RAW into json.dumps and a raw dataclass raises
    TypeError there (review-executed), so as_value() is the required seam."""
    from eval.earp.schema.models import ProjectionStatusWitness  # noqa: PLC0415

    status = ProjectionStatusWitness(
        runtime_embedder_available=False,
        runtime_unavailability_reason="no_runtime",
        readiness={"title": "unavailable"},
        vector_unsupported_kinds=("event",),
    )
    assert json.loads(json.dumps(status.as_value())) == {
        "runtime_embedder_available": False,
        "runtime_unavailability_reason": "no_runtime",
        "readiness": {"title": "unavailable"},
        "vector_unsupported_kinds": ["event"],
    }


def _vector_scenario(doc: dict[str, Any]) -> Any:
    """The S7 synthetic-shape precedent: the resolver deliberately refuses
    `vector: true` without the embedder, so the embedder-declared shape is
    built by dataclasses.replace and driven through poll_override."""
    resolution = resolve_config(doc)
    assert resolution.blockers == ()
    assert resolution.scenario is not None
    return dataclasses.replace(
        resolution.scenario,
        projections=(
            DeclaredProjection(name="title", roles=("searchable",), fts=True, vector=True),
        ),
    )


def test_unavailable_is_a_settled_poll_state(tmp_path: Path) -> None:
    """`unavailable` never resolves by waiting, so the poll exits on the FIRST
    read -- stated semantics now, no longer an accident of the `== "embedding"`
    comparison -- and never becomes DENSE_READINESS_TIMEOUT."""
    pytest.importorskip("fathomdb._fathomdb", reason="native binding not built")
    from eval.earp.runner import run_diagnostic  # noqa: PLC0415
    from fathomdb.types import ProjectionSpec  # noqa: PLC0415

    doc = _config(_fixture_file(tmp_path))
    scenario = _vector_scenario(doc)
    calls = 0

    def poll() -> tuple[list[ProjectionSpec], float]:
        nonlocal calls
        calls += 1
        specs = [
            ProjectionSpec(
                name="title",
                roles=frozenset({"searchable"}),
                fts=True,
                vector=True,
                vector_dense_readiness="unavailable",
            )
        ]
        return specs, 0.0 if calls == 1 else scenario.readiness_timeout_s + 1.0

    result = run_diagnostic(
        scenario=scenario,
        config_doc=doc,
        experiments_root=tmp_path / "experiments",
        experiment="earp-s10",
        ts=TS,
        poll_override=poll,
    )
    assert calls == 1
    assert result.failure is None, result.failure
    assert BlockerCode.DENSE_READINESS_TIMEOUT not in {b.code for b in result.blockers}
    assert _sidecar(result)["scenario"]["projection_witnesses"]["readiness"] == {
        "title": "unavailable"
    }


def test_degraded_open_with_a_dense_declaration_is_blocked_not_failed(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """AC-7 -- the fact-3 defect, fixed: classify_open only ACCUMULATES the
    DENSE_DISABLED blocker and the run proceeds to the poll, which pre-S10
    crashed the run to verdict=FAILED (bare AssertionError, empty blockers,
    no readiness in the sidecar). There is no open-report seam, so the
    degraded report is synthesized by patching `_report_mapping` -- the same
    synthetic-shape rationale as poll_override."""
    pytest.importorskip("fathomdb._fathomdb", reason="native binding not built")
    from eval.earp import runner as runner_module  # noqa: PLC0415
    from fathomdb.types import ProjectionSpec  # noqa: PLC0415

    real_mapping = runner_module._report_mapping

    def degraded(report: Any) -> dict[str, Any]:
        value = real_mapping(report)
        value["dense_disabled"] = True
        value["dense_disabled_reason"] = "vector_equivalence_disabled"
        return value

    monkeypatch.setattr(runner_module, "_report_mapping", degraded)

    doc = _config(_fixture_file(tmp_path))
    scenario = _vector_scenario(doc)

    def poll() -> tuple[list[ProjectionSpec], float]:
        specs = [
            ProjectionSpec(
                name="title",
                roles=frozenset({"searchable"}),
                fts=True,
                vector=True,
                vector_dense_readiness="unavailable",
            )
        ]
        return specs, 0.0

    result = runner_module.run_diagnostic(
        scenario=scenario,
        config_doc=doc,
        experiments_root=tmp_path / "experiments",
        experiment="earp-s10",
        ts=TS,
        poll_override=poll,
    )
    assert result.verdict is RunVerdict.BLOCKED, (result.verdict, result.failure)
    assert result.failure is None, result.failure
    codes = {b.code for b in result.blockers}
    assert BlockerCode.DENSE_DISABLED in codes
    sidecar = _sidecar(result)
    assert sidecar["verdict"] == "blocked"
    assert sidecar["scenario"]["projection_witnesses"]["readiness"] == {
        "title": "unavailable"
    }


def test_projection_status_is_captured_as_the_fourth_witness(tmp_path: Path) -> None:
    """R3: SUPPLEMENTARY -- the three true sources stay untouched and the
    status snapshot rides under its own source name. On this no-embedder,
    FTS-only run the engine reports `no_runtime` and an ENGINE-produced
    `not_declared` (fact 6: same spelling as the poll map's EARP-derived
    value, different producer)."""
    result = _run(tmp_path, _config(_fixture_file(tmp_path)))
    assert result.verdict is RunVerdict.COMPLETE, (result.failure, result.blockers)
    by_name = {w.name: w for w in result.witnesses}
    status = by_name["projection_status"]
    assert status.source is WitnessSource.PROJECTION_STATUS
    assert status.call_path == "fathomdb.read.projection_status"
    assert status.value == {
        "runtime_embedder_available": False,
        "runtime_unavailability_reason": "no_runtime",
        "readiness": {"title": "not_declared"},
        "vector_unsupported_kinds": [],
    }
    for name in ("open_report", "projection_delta", "projection_readiness"):
        assert name in by_name, name
    sidecar_block = _sidecar(result)["scenario"]["projection_witnesses"]
    assert sidecar_block["projection_status"] == status.value


def test_projection_less_run_captures_no_status(tmp_path: Path) -> None:
    """Absent, never empty (S7's rule): a projection-less run records neither
    the witness nor the object -- the always-captured open-report witness
    already carries runtime availability."""
    doc = _config(_fixture_file(tmp_path), projections=None)
    doc["scenario"]["query"] = {"call": "Engine.search_text_only", "text": "deal sheet"}
    result = _run(tmp_path, doc)
    assert result.verdict is RunVerdict.COMPLETE
    assert not any(w.name == "projection_status" for w in result.witnesses)
    assert "projection_status" not in _sidecar(result)["scenario"]["projection_witnesses"]


def test_the_refusal_message_cites_the_engines_unavailable_state() -> None:
    """R2/AC-4 -- a NEW pin (review-verified that none existed): the refusal
    stays, and its message names the engine's honest `unavailable` state
    instead of the pre-Slice-21 vacuous-ready story."""
    doc = _config()
    doc["scenario"]["projections"]["declare"] = [_declare(vector=True)]
    result = resolve_config(doc)
    blocker = next(
        b
        for b in result.blockers
        if b.code is BlockerCode.CONFIG_INVALID_VALUE
        and str(b.detail.get("path", "")).endswith(".vector")
    )
    assert "unavailable" in blocker.message, blocker.message
    assert "use_default_embedder" in blocker.message
    assert "vacuous" not in blocker.message


# --- opt-in: real embedder, real readiness -----------------------------------

_EARP_INTEGRATION = os.environ.get("FDB_EARP_INTEGRATION") == "1"


@pytest.mark.integration
@pytest.mark.skipif(
    not _EARP_INTEGRATION,
    reason=(
        "opt-in integration test: set FDB_EARP_INTEGRATION=1 (loads the real "
        "default embedder from the local cache; the default gate skips -- no "
        "model load, no network)"
    ),
)
def test_embedder_on_readiness_reaches_ready(tmp_path: Path) -> None:
    """The control for the no-embedder refusal: WITH the embedder the dense
    sub-target defers at configure time and the poll reads embedding -> ready,
    never `unavailable`."""
    doc = _config(_fixture_file(tmp_path), engine={"use_default_embedder": True})
    doc["scenario"]["projections"]["declare"] = [_declare(vector=True)]
    result = _run(tmp_path, doc)
    codes = {b.code for b in result.blockers}
    assert BlockerCode.DENSE_READINESS_TIMEOUT not in codes, result.blockers
    by_name = {w.name: w for w in result.witnesses}
    assert "title" in by_name["projection_delta"].value["deferred"]
    assert by_name["projection_readiness"].value == {"title": "ready"}
