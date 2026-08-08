"""S5 — the diagnostic runner. The first slice that opens a real engine.

Proves the machinery end to end WITHOUT making a retrieval-quality claim.
Everything it measures is a property of the system -- did the write land, did
the search return, what did open report -- never of relevance. A green
diagnostic says the harness works; it says nothing about whether FathomDB
retrieves well.

Design of record: `dev/design/earp-slice-5-design.md`.
"""

from __future__ import annotations

import json
import shutil
import tempfile
import time
from dataclasses import dataclass, field, replace
from datetime import datetime
from pathlib import Path
from typing import Any, Callable, Mapping, Sequence

from eval.earp.config import ResolvedScenario
from eval.earp.schema.models import (
    SCHEMA_VERSION_RESULT,
    Blocker,
    BlockerCode,
    DeclaredProjection,
    ProjectionStatusWitness,
    ProjectionWitnesses,
    RunVerdict,
    Witness,
    WitnessSource,
    WitnessStatus,
)
from eval.earp.writer import WriteOutcome, write_run

#: Config knob name -> real SDK parameter name. They are NOT always the same:
#: `Engine.search_projected_text` takes `name`, while the config calls it
#: `projection_name` because a bare `name` would be meaningless in a config.
PARAM_RENAMES: Mapping[str, str] = {"projection_name": "name"}


@dataclass(frozen=True)
class DiagnosticResult:
    verdict: RunVerdict
    run_id: str | None = None
    run_dir: Path | None = None
    witnesses: tuple[Witness, ...] = ()
    blockers: tuple[Blocker, ...] = ()
    hit_doc_ids: list[str] = field(default_factory=list)
    failure: str | None = None
    db_dir: str | None = None


def load_fixture(path: Path) -> list[dict[str, Any]]:
    """Parse and validate a fixture file.

    Every precondition here exists because the engine will NOT catch it:

    * a null or absent `body` is accepted and stored as `'{}'`, invisible to
      FTS, behind a receipt that looks perfectly healthy;
    * a non-string body raises `WriteValidationError: ... lone surrogate`,
      which names a UTF-8 problem rather than the type error it is;
    * a duplicate `logical_id` silently supersedes the earlier row, leaving one
      active document with no error and no signal in the receipt.
    """
    items: list[dict[str, Any]] = []
    seen: set[str] = set()
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        line = line.strip()
        if not line:
            continue
        item = json.loads(line)
        body = item.get("body")
        if not isinstance(body, str) or not body:
            raise ValueError(
                f"fixture line {number}: `body` must be a non-empty string; the engine "
                f"stores a null body as '{{}}' invisibly and reports a misleading "
                f"UTF-8 error for other types"
            )
        if not item.get("source_id"):
            raise ValueError(f"fixture line {number}: `source_id` is mandatory")
        logical_id = item.get("logical_id")
        if not isinstance(logical_id, str) or not logical_id:
            raise ValueError(
                f"fixture line {number}: `logical_id` is required -- it is the doc id "
                f"the search hit maps back to"
            )
        if logical_id in seen:
            raise ValueError(
                f"fixture line {number}: duplicate logical_id `{logical_id}`; the second "
                f"write would silently supersede the first"
            )
        seen.add(logical_id)
        items.append(item)
    if not items:
        raise ValueError("fixture is empty")
    return items


def classify_open(
    report: Mapping[str, Any], *, dense_required: bool = False
) -> tuple[tuple[Witness, ...], tuple[Blocker, ...]]:
    """Turn an open report into witnesses and blockers.

    A pure function over a mapping, deliberately: a real embedder fetch is
    forbidden by the default-deny network policy, so if this decision lived
    inline in the run path its blocker branch could never be exercised.
    (`vector_equivalence_refusal_count()` is an Engine METHOD, so the runner
    reads it at capture time and carries it in the projection witnesses --
    it never enters this function.)

    S7 amendment: `dense_disabled` is the typed blocker only when the scenario
    declared a `vector: true` projection (`dense_required`) -- per `earp.md`,
    "a typed blocker when dense retrieval was REQUIRED". Otherwise the
    condition is witness-recorded, not blocking: the open-report witness below
    carries the full report either way.
    """
    witnesses: list[Witness] = []
    blockers: list[Blocker] = []

    witnesses.append(
        Witness(
            name="open_report",
            source=WitnessSource.OPEN_REPORT,
            call_path="Engine.open_report",
            status=WitnessStatus.OBSERVED,
            value=dict(report),
        )
    )

    download_ms = report.get("embedder_download_ms")
    if download_ms is not None:
        blockers.append(
            Blocker(
                code=BlockerCode.EMBEDDER_FETCHED,
                message=(
                    f"the open fetched embedder weights ({download_ms} ms) rather than "
                    f"using a local cache; network is denied by default"
                ),
                stage="runner.open",
                detail={"embedder_download_ms": download_ms},
            )
        )
    if report.get("dense_disabled") and dense_required:
        blockers.append(
            Blocker(
                code=BlockerCode.DENSE_DISABLED,
                message=(
                    "the engine opened degraded: the vector-equivalence self-check "
                    "found a divergence, so vector-dependent arms refuse at query "
                    "time -- and this scenario declared a dense projection"
                ),
                stage="runner.open",
                detail={"reason": report.get("dense_disabled_reason")},
            )
        )
    return tuple(witnesses), tuple(blockers)


def classify_delta(delta_value: Mapping[str, Any]) -> tuple[Witness, Blocker | None]:
    """Turn a `ProjectionDelta` mapping into its witness and (maybe) blocker.

    Pure over the delta mapping for the same reason `classify_open` is pure
    over the report: on the S7 order (configure BEFORE ingest, fresh database)
    the corpus has no kinds yet, so `vector_unsupported_kinds` is empty on
    every honest run and the blocker branch is only exercisable synthetically.

    The delta is recorded VERBATIM either way -- including the non-disjoint
    built/deferred lists: "in `built`" must never be read as "fully built";
    the dense portion keys on `deferred`.
    """
    witness = Witness(
        name="projection_delta",
        source=WitnessSource.PROJECTION_DELTA,
        call_path="Engine.configure_projections",
        status=WitnessStatus.OBSERVED,
        value=dict(delta_value),
    )
    kinds = list(delta_value.get("vector_unsupported_kinds") or ())
    if not kinds:
        return witness, None
    return witness, Blocker(
        code=BlockerCode.VECTOR_UNSUPPORTED_KINDS,
        message=(
            f"the vector writer can never embed kinds {kinds}; this is PERMANENT, "
            f"not a deferred build -- such rows stay FTS-searchable only"
        ),
        stage="runner.configure_projections",
        detail={"vector_unsupported_kinds": kinds},
    )


#: Real-poll cadence. The `poll_override` seam replaces BOTH the
#: `read.projections` call and the clock, so tests never sleep.
_POLL_INTERVAL_S = 0.5


def _readiness_view(engine: Any) -> Callable[[], tuple[Sequence[Any], float]]:
    """The real (specs, elapsed) view: sleeps between polls, never before the
    first, so a ready-at-once projection costs zero wall time."""
    from fathomdb import read as fathom_read  # noqa: PLC0415 -- native import

    start = time.monotonic()
    polled = False

    def view() -> tuple[Sequence[Any], float]:
        nonlocal polled
        if polled:
            time.sleep(_POLL_INTERVAL_S)
        polled = True
        return fathom_read.projections(engine), time.monotonic() - start

    return view


def _poll_readiness(
    view: Callable[[], tuple[Sequence[Any], float]],
    declared: Sequence[DeclaredProjection],
    timeout_s: float,
) -> tuple[dict[str, str], list[str]]:
    """Poll until every `vector: true` spec reads a SETTLED state, or
    `timeout_s` elapses. Returns the readiness map and the still-`embedding`
    names (empty on success). `vector: false` specs are recorded
    `not_declared` -- meaning no VECTOR SUB-TARGET is declared; the projection
    itself is -- and are never polled-for.

    Settled states (S10, stated rather than an accident of the comparison):
    `ready` AND `unavailable` -- the Slice-21 absent-or-equivalence-refused
    runtime never resolves by waiting, so the poll exits immediately for it
    and never emits DENSE_READINESS_TIMEOUT; only `embedding` spins. Blocked-
    verdict coverage for the degraded open comes from DENSE_DISABLED instead.
    """
    while True:
        specs, elapsed = view()
        by_name = {spec.name: spec for spec in specs}
        readiness: dict[str, str] = {}
        stuck: list[str] = []
        for projection in declared:
            spec = by_name[projection.name]
            state = ProjectionWitnesses.readiness_state(
                vector=spec.vector, vector_dense_readiness=spec.vector_dense_readiness
            )
            readiness[projection.name] = state
            if state == "embedding":
                stuck.append(projection.name)
        if not stuck or elapsed >= timeout_s:
            return readiness, stuck


def _doc_ids(results: Sequence[Any]) -> tuple[list[str], list[str]]:
    """Map hits to doc ids. A hit in the `content` space means the fixture
    omitted a `logical_id` -- a fixture defect, so it is surfaced separately
    rather than counted as a retrieval outcome."""
    mapped: list[str] = []
    unmapped: list[str] = []
    for hit in results:
        if getattr(hit.id, "space", None) == "logical":
            mapped.append(hit.id.value)
        else:
            unmapped.append(f"{hit.id.space}:{hit.id.value}")
    return mapped, unmapped


def run_diagnostic(
    *,
    scenario: ResolvedScenario,
    config_doc: Mapping[str, Any],
    experiments_root: Path,
    experiment: str,
    ts: datetime,
    query_override: Callable[..., Any] | None = None,
    poll_override: Callable[[], tuple[Sequence[Any], float]] | None = None,
) -> DiagnosticResult:
    """Run one diagnostic scenario against a real engine.

    S7 order: open -> open-report witness (+refusal count) ->
    `configure_projections(declared)` BEFORE ingest -> delta witness -> ingest
    -> poll readiness -> query. `poll_override` supplies the (specs, elapsed)
    view per iteration, replacing both the `read.projections` call and the
    clock (the S5 `query_override` precedent) -- the timeout path is testable
    with zero real waiting.
    """
    from fathomdb import Engine  # noqa: PLC0415 -- native import, S5 only
    from fathomdb import read as fathom_read  # noqa: PLC0415
    from fathomdb.types import ProjectionSpec  # noqa: PLC0415

    fixture_path = Path(str(config_doc["scenario"]["fixture"]))
    query_text = str(config_doc["scenario"]["query"].get("text", ""))

    if not fixture_path.is_file():
        blocker = Blocker(
            code=BlockerCode.FIXTURE_MISSING,
            message=f"declared fixture does not exist: {fixture_path}",
            stage="runner.fixture",
            detail={"path": str(fixture_path)},
        )
        outcome = _write(
            scenario, config_doc, experiments_root, experiment, ts,
            RunVerdict.BLOCKED, (), (blocker,), "fixture missing",
        )
        return DiagnosticResult(
            verdict=RunVerdict.BLOCKED,
            run_id=outcome.run_id,
            run_dir=outcome.run_dir,
            blockers=(blocker,),
        )

    items = load_fixture(fixture_path)

    # One fresh temp DIRECTORY, not just a file: close() checkpoints away
    # -wal/-shm but leaves a .lock sidecar, so per-file deletion is wrong.
    db_dir = tempfile.mkdtemp(prefix="earp-diagnostic-")
    witnesses: list[Witness] = []
    blockers: list[Blocker] = []
    failure: str | None = None
    verdict = RunVerdict.COMPLETE
    hit_doc_ids: list[str] = []
    engine = None
    declared = scenario.projections
    dense_required = any(projection.vector for projection in declared)
    projection_witnesses: ProjectionWitnesses | None = None

    try:
        engine = Engine.open(
            str(Path(db_dir) / "diagnostic.db"),
            use_default_embedder=scenario.use_default_embedder,
        )
        report_value = _report_mapping(engine.open_report())
        open_witnesses, open_blockers = classify_open(
            report_value, dense_required=dense_required
        )
        witnesses.extend(open_witnesses)
        blockers.extend(open_blockers)
        # The refusal count is an Engine method, not an open-report field, so
        # it is read here at capture time and carried in the witness -- which
        # keeps classify_open a pure function over the report mapping.
        projection_witnesses = ProjectionWitnesses(
            open_report={
                "dense_disabled": bool(report_value["dense_disabled"]),
                "dense_disabled_reason": report_value["dense_disabled_reason"],
                "query_backend": report_value["query_backend"],
                "refusal_count": engine.vector_equivalence_refusal_count(),
            }
        )

        if declared:
            # BEFORE ingest: the engine backfills same-transaction FTS builds,
            # and the delta on a fresh, empty database is the declaration's
            # honest diff.
            delta = engine.configure_projections(
                [
                    ProjectionSpec(
                        name=projection.name,
                        roles=frozenset(projection.roles),
                        fts=projection.fts,
                        vector=projection.vector,
                    )
                    for projection in declared
                ]
            )
            delta_value = {
                "built": list(delta.built),
                "dropped": list(delta.dropped),
                "deferred": list(delta.deferred),
                "unchanged": delta.unchanged,
                "vector_unsupported_kinds": list(delta.vector_unsupported_kinds),
            }
            delta_witness, delta_blocker = classify_delta(delta_value)
            witnesses.append(delta_witness)
            if delta_blocker is not None:
                blockers.append(delta_blocker)
            projection_witnesses = replace(
                projection_witnesses, configure_delta=delta_value
            )

        receipt = engine.write(list(items))
        witnesses.append(
            Witness(
                name="write_receipt",
                source=WitnessSource.WRITE_RECEIPT,
                call_path="Engine.write",
                status=WitnessStatus.OBSERVED,
                value={"cursor": receipt.cursor, "rows": len(receipt.row_cursors)},
            )
        )

        # The receipt is counters only, so it cannot distinguish a landed
        # fixture from a silently-empty one. Read the documents back.
        expected = [item["logical_id"] for item in items]
        # `get_many` returns None for an id it cannot find, so a filtered
        # comprehension is the difference between "two landed" and a crash on
        # exactly the silent-write case this witness exists to catch.
        landed = [record for record in fathom_read.get_many(engine, expected) if record]
        witnesses.append(
            Witness(
                name="fixture_landed",
                source=WitnessSource.STORE_QUERY,
                call_path="fathomdb.read.get_many",
                status=WitnessStatus.OBSERVED,
                value={
                    "expected": len(expected),
                    "found": len(landed),
                    "logical_ids": sorted(record.logical_id for record in landed),
                },
            )
        )

        witnesses.append(
            Witness(
                name="projection_coverage",
                source=WitnessSource.READ_PROJECTIONS,
                call_path="fathomdb.read.projections",
                status=WitnessStatus.OBSERVED,
                value={"count": len(fathom_read.projections(engine))},
            )
        )

        if declared:
            readiness, stuck = _poll_readiness(
                poll_override or _readiness_view(engine),
                declared,
                scenario.readiness_timeout_s,
            )
            witnesses.append(
                Witness(
                    name="projection_readiness",
                    source=WitnessSource.READ_PROJECTIONS,
                    call_path="fathomdb.read.projections",
                    status=WitnessStatus.OBSERVED,
                    value=dict(readiness),
                )
            )
            projection_witnesses = replace(projection_witnesses, readiness=readiness)
            if stuck:
                blockers.append(
                    Blocker(
                        code=BlockerCode.DENSE_READINESS_TIMEOUT,
                        message=(
                            f"vector_dense_readiness stayed `embedding` past the "
                            f"declared timeout ({scenario.readiness_timeout_s}s) "
                            f"for {stuck}"
                        ),
                        stage="runner.readiness",
                        detail={
                            "stuck": stuck,
                            "readiness_timeout_s": scenario.readiness_timeout_s,
                        },
                    )
                )

        if declared:
            # S10: capture `read.projection_status` ONCE -- after the poll
            # settles (including on the DENSE_READINESS_TIMEOUT / degraded-open
            # blocked paths, the S7 delta-witness precedent) and BEFORE the
            # query, so a query-time FAILED run still carries it while a
            # poll-raise skips capture. Projection-less runs never capture
            # (absent, not empty). Supplementary: the three true sources above
            # are untouched, and on any disagreement both are recorded as-is --
            # capture is not atomic with the poll, so transient disagreement is
            # legitimate; EARP records, it does not reconcile.
            status = fathom_read.projection_status(engine)
            status_value = ProjectionStatusWitness(
                runtime_embedder_available=status.runtime_embedder_available,
                runtime_unavailability_reason=status.runtime_unavailability_reason,
                readiness={
                    entry.name: entry.dense_readiness for entry in status.projections
                },
                vector_unsupported_kinds=tuple(status.vector_unsupported_kinds),
            )
            witnesses.append(
                Witness(
                    name="projection_status",
                    source=WitnessSource.PROJECTION_STATUS,
                    call_path="fathomdb.read.projection_status",
                    status=WitnessStatus.OBSERVED,
                    value=status_value.as_value(),
                )
            )
            projection_witnesses = replace(
                projection_witnesses, projection_status=status_value
            )

        call = query_override or resolve_call(engine, scenario.query_call)
        params = {
            PARAM_RENAMES.get(key, key): value
            for key, value in scenario.query_params.items()
            if key != "text"
        }
        result = call(query_text, **params)
        hit_doc_ids, unmapped = _doc_ids(result.results)
        witnesses.append(
            Witness(
                name="search_returned",
                source=WitnessSource.SEARCH_RESULT,
                call_path=scenario.query_call,
                status=WitnessStatus.OBSERVED,
                value={"n": len(result.results), "doc_ids": hit_doc_ids},
            )
        )
        if unmapped:
            failure = f"hits outside the logical id space: {unmapped}"
            verdict = RunVerdict.FAILED
        elif blockers:
            verdict = RunVerdict.BLOCKED
    except Exception as exc:  # noqa: BLE001 -- surfaced as a typed failure
        failure = f"{type(exc).__name__}: {exc}"
        verdict = RunVerdict.FAILED
    finally:
        if engine is not None:
            try:
                engine.close()
            except Exception:  # noqa: BLE001, S110 -- teardown must not mask
                pass
        shutil.rmtree(db_dir, ignore_errors=True)

    outcome = _write(
        scenario, config_doc, experiments_root, experiment, ts,
        verdict, tuple(witnesses), tuple(blockers),
        failure or f"diagnostic run: {len(hit_doc_ids)} hit(s)",
        projection_witnesses=projection_witnesses,
    )
    return DiagnosticResult(
        verdict=verdict,
        run_id=outcome.run_id,
        run_dir=outcome.run_dir,
        witnesses=tuple(witnesses),
        blockers=tuple(blockers),
        hit_doc_ids=hit_doc_ids,
        failure=failure,
        db_dir=db_dir,
    )


def resolve_call(engine: Any, name: str) -> Callable[..., Any]:
    """Map a config `Engine.<method>` name to the bound engine method. Public
    since S8: the characterization/comparison arm executor threads
    `query_call`/`query_params` through the same seam as the diagnostic
    runner, rather than growing a second rename table."""
    attribute = name.split(".", 1)[1]
    return getattr(engine, attribute)


def _report_mapping(report: Any) -> dict[str, Any]:
    return {
        "schema_version_before": report.schema_version_before,
        "schema_version_after": report.schema_version_after,
        "query_backend": report.query_backend,
        "embedder_download_ms": report.embedder_download_ms,
        "dense_disabled": report.dense_disabled,
        "dense_disabled_reason": report.dense_disabled_reason,
        "embedder_events": [dict(event) for event in report.embedder_events],
    }


def _write(
    scenario: ResolvedScenario,
    config_doc: Mapping[str, Any],
    experiments_root: Path,
    experiment: str,
    ts: datetime,
    verdict: RunVerdict,
    witnesses: tuple[Witness, ...],
    blockers: tuple[Blocker, ...],
    read: str,
    projection_witnesses: ProjectionWitnesses | None = None,
) -> WriteOutcome:
    """Hand the run to S4. `metrics` is structurally `{}` -- a diagnostic makes
    no relevance claim, so there is no code path by which one could appear."""
    sidecar: dict[str, Any] = {
        "schema_version": SCHEMA_VERSION_RESULT,
        "run_id": "",
        "campaign": scenario.campaign.value,
        "verdict": verdict.value,
        "scenario": {
            "config_sha256": scenario.config_sha256,
            "query_call": scenario.query_call,
            "retrieval_mode": scenario.retrieval_mode.value,
            # The public result limit in effect (S6a). The resolver injected it
            # into query_params, so the engine call above genuinely used it.
            "fanout_used": scenario.max_measurable_k,
        },
        "metrics": {},
        "witnesses": [
            {
                "name": w.name,
                "source": w.source.value,
                "call_path": w.call_path,
                "status": w.status.value,
                "value": w.value,
            }
            for w in witnesses
        ],
        "blockers": [
            {"code": b.code.value, "message": b.message, "stage": b.stage, "detail": b.detail}
            for b in blockers
        ],
    }
    if projection_witnesses is not None:
        # Absent signals stay ABSENT (never empty): as_value omits what was
        # not captured, so a reader can tell "not declared" from "not captured".
        sidecar["scenario"]["projection_witnesses"] = projection_witnesses.as_value()
    run_id = _lib_run_id(experiment, ts, scenario.config_sha256)
    sidecar["run_id"] = run_id
    return write_run(
        experiment=experiment,
        ts=ts,
        config_doc=config_doc,
        experiments_root=experiments_root,
        verdict=verdict,
        read=read,
        metrics={},
        sidecar=sidecar,
        code={"git_sha": "", "dirty": False, "branch": "", "baseline_commit": None},
        env={"python": "", "lockfile_sha256": None, "gpu": None, "key_deps": {}},
        corpus={"source": None, "manifest_sha256": None, "datasets": []},
        seeds={},
        cost_usd=0.0,
    )


def _lib_run_id(experiment: str, ts: datetime, sha: str) -> str:
    from eval.earp._experiments import lib as _lib  # noqa: PLC0415

    return _lib.make_run_id(experiment, ts, sha)


__all__ = [
    "PARAM_RENAMES",
    "DiagnosticResult",
    "classify_delta",
    "classify_open",
    "load_fixture",
    "resolve_call",
    "run_diagnostic",
]
