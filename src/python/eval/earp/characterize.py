"""S6 — corpus-scale characterization and replay.

The first slice that makes a retrieval-quality claim. Everything before it
measured the harness; this measures FathomDB.

Design of record: `dev/design/earp-slice-6-design.md`.
"""

from __future__ import annotations

import hashlib
import json
import platform
import shutil
import tempfile
from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum
from pathlib import Path
from typing import Any, Callable, Mapping, Sequence

from eval.earp._experiments import lib as _lib
from eval.earp.config import ResolvedScenario
from eval.earp.depth import check_depth
from eval.earp.gold import GoldQuery, verify_gold
from eval.earp.metrics import KResult, aggregate, resolve_ndcg, validate_methodology
from eval.earp.runner import PARAM_RENAMES, resolve_call
from eval.earp.schema.models import (
    ENGINE_DEFAULT_RESULT_LIMIT,
    ENGINE_MAX_RESULT_LIMIT,
    SCHEMA_VERSION_PER_QUERY,
    SCHEMA_VERSION_RESULT,
    Blocker,
    BlockerCode,
    CampaignKind,
    MetricValue,
    RetrievalMode,
    RunVerdict,
)
from eval.earp.writer import write_run


class DriftAxis(str, Enum):
    CONFIG = "config"
    CODE = "code"
    ENVIRONMENT = "environment"


@dataclass(frozen=True)
class Drift:
    axis: DriftAxis
    before: str
    after: str
    #: True when the prior run recorded nothing on this axis. Drift from an
    #: empty value is a MISSING RECORD, not a change, and reporting it as
    #: change would be a false positive.
    unrecoverable: bool = False


@dataclass(frozen=True)
class ReplayReport:
    """Deliberately carries no verdict: S6 measures drift, it does not rule."""

    run_id: str
    drift: tuple[Drift, ...] = ()


@dataclass(frozen=True)
class CharacterizationResult:
    verdict: RunVerdict
    run_id: str | None = None
    run_dir: Path | None = None
    ingested: int = 0
    retrievals: int = 0
    #: The public result limit the run actually passed to the engine (S6a):
    #: max(ladder), recorded with every number per IR-B (c).
    fanout_used: int = ENGINE_DEFAULT_RESULT_LIMIT
    per_k: Mapping[int, KResult] = field(default_factory=dict)
    document_metrics: Mapping[str, MetricValue] = field(default_factory=dict)
    per_query_rows: list[dict[str, Any]] = field(default_factory=list)
    blockers: tuple[Blocker, ...] = ()
    config_doc: Mapping[str, Any] = field(default_factory=dict)
    code_sha: str = ""
    python_version: str = ""


def load_corpus(shard: Path, *, source: str) -> list[dict[str, Any]]:
    """Parse one snapshot shard into write items.

    Preconditions mirror S5's fixture loader, for the same reasons: the engine
    stores a null body as `'{}'` invisibly, and a duplicate `logical_id`
    silently supersedes -- at corpus scale that is a document deleted from the
    index while the gold still requires it.
    """
    items: list[dict[str, Any]] = []
    seen: set[str] = set()
    for number, line in enumerate(shard.read_text(encoding="utf-8").splitlines(), start=1):
        line = line.strip()
        if not line:
            continue
        row = json.loads(line)
        doc_id = row.get("doc_id")
        body = row.get("body")
        if not isinstance(doc_id, str) or not doc_id:
            raise ValueError(f"{shard.name} line {number}: missing `doc_id`")
        if not isinstance(body, str) or not body:
            raise ValueError(f"{shard.name} line {number}: `body` must be a non-empty string")
        if doc_id in seen:
            raise ValueError(
                f"{shard.name} line {number}: duplicate doc_id `{doc_id}`; the second "
                f"write would silently supersede the first, deleting a document the "
                f"gold still requires"
            )
        seen.add(doc_id)
        items.append(
            {
                "kind": row.get("source_type") or "doc",
                "body": body,
                "source_id": source,
                "logical_id": doc_id,
            }
        )
    return items


def _blocked(code: BlockerCode, message: str, stage: str, **detail: Any) -> Blocker:
    return Blocker(code=code, message=message, stage=stage, detail=dict(detail))


def _load_snapshot_shards(
    data_root: Path, snapshot_path: Path
) -> tuple[list[dict[str, Any]], Blocker | None]:
    """Ingest is driven by `snapshot.per_source_sha256`, never by a glob.

    A glob over `raw/*.jsonl` picks up shards that are NOT in the frozen
    snapshot, so the run would pin a corpus identity to an index that does not
    match it -- and Recall@K would be depressed by documents the corpus does
    not contain.
    """
    snapshot = json.loads(snapshot_path.read_text(encoding="utf-8"))
    items: list[dict[str, Any]] = []
    for entry in snapshot.get("per_source_sha256", []):
        source = entry["source"]
        shard = data_root / "raw" / f"{source}.jsonl"
        if not shard.is_file():
            return [], _blocked(
                BlockerCode.CORPUS_ROOT_ABSENT,
                f"snapshot declares source `{source}` but {shard} is absent",
                "characterize.ingest",
                source=source,
            )
        digest = hashlib.sha256(shard.read_bytes()).hexdigest()
        if digest != entry["sha256"]:
            return [], _blocked(
                BlockerCode.CORPUS_ROOT_ABSENT,
                f"shard `{source}` does not match the snapshot pin: {digest} != "
                f"{entry['sha256']}; the corpus identity would certify an index it "
                f"does not describe",
                "characterize.ingest",
                source=source,
            )
        shard_items = load_corpus(shard, source=source)
        if len(shard_items) != entry["doc_count"]:
            return [], _blocked(
                BlockerCode.CORPUS_ROOT_ABSENT,
                f"shard `{source}` has {len(shard_items)} rows, snapshot declares "
                f"{entry['doc_count']}",
                "characterize.ingest",
                source=source,
            )
        items.extend(shard_items)
    return items, None


@dataclass(frozen=True)
class ArmExecution:
    """One arm's execution: ingest + gold verification + retrieve loop +
    scoring, and NOTHING durable -- no run directory, no sidecar, no index
    line. Extracted from `run_characterization` for S8 so a comparison never
    writes standalone characterization records per arm; ALL arms-campaign
    writing lives in `eval.earp.comparison`.

    Either `blocker` is set (nothing was measured) or the measurement fields
    are populated.
    """

    blocker: Blocker | None = None
    ingested: int = 0
    retrievals: int = 0
    cache: Mapping[str, list[str]] = field(default_factory=dict)
    errors: Mapping[str, str] = field(default_factory=dict)
    per_k: Mapping[int, KResult] = field(default_factory=dict)
    rows: list[dict[str, Any]] = field(default_factory=list)
    queries: tuple[GoldQuery, ...] = ()


def execute_arm(
    *,
    scenario: ResolvedScenario,
    data_root: Path,
    snapshot_path: Path,
    gold_path: Path,
    gold_sha256: str,
    corpus_hash: str,
    qrels_version: str,
    manifest_path: Path | None = None,
    retrieve_override: Callable[[str], Any] | None = None,
) -> ArmExecution:
    """Run one resolved scenario over a FRESH database and score it.

    Honors the arm's resolved public `limit` (never `max(ladder)` -- that
    remains `run_characterization`'s own wrapper behaviour), embedder flag,
    projections (configured BEFORE ingest, the S7 order), and query call:
    `query_call`/`query_params` thread through the runner's `resolve_call` +
    `PARAM_RENAMES` seam. `retrieve_override` takes the query text and stands
    in for the whole engine call (the S6 seam, unchanged).
    """
    from fathomdb import Engine  # noqa: PLC0415 -- native import

    if not Path(data_root).is_dir():
        return ArmExecution(
            blocker=_blocked(
                BlockerCode.CORPUS_ROOT_ABSENT,
                f"configured corpus data_root does not exist: {data_root}",
                "characterize.ingest",
            )
        )

    items, ingest_blocker = _load_snapshot_shards(Path(data_root), Path(snapshot_path))
    if ingest_blocker is not None:
        return ArmExecution(blocker=ingest_blocker)

    verification = verify_gold(
        gold_path=Path(gold_path),
        snapshot_path=Path(snapshot_path),
        manifest_path=manifest_path,
        expected_sha256=gold_sha256,
        expected_corpus_hash=corpus_hash,
        expected_qrels_version=qrels_version,
        data_root=Path(data_root),
    )
    if verification.blocker is not None:
        blocker = verification.blocker
        if blocker.code is BlockerCode.GOLD_STALE_QRELS_VERSION:
            blocker = _blocked(
                blocker.code,
                blocker.message + " (tests/corpus/scripts/build_ir_gold.py)",
                blocker.stage,
                **blocker.detail,
            )
        return ArmExecution(blocker=blocker)

    gold_set = verification.gold_set
    assert gold_set is not None

    issues = validate_methodology(gold_set.queries)
    if issues:
        return ArmExecution(
            blocker=_blocked(
                BlockerCode.GOLD_MALFORMED,
                f"gold violates a methodology invariant: {issues[:3]}",
                "characterize.gold",
            )
        )

    ingested_ids = {item["logical_id"] for item in items}
    required = {
        unit.doc_id
        for query in gold_set.queries
        for unit in query.required_evidence
        if unit.necessity == "required"
    }
    missing = sorted(required - ingested_ids)
    if missing:
        return ArmExecution(
            blocker=_blocked(
                BlockerCode.GOLD_CORPUS_MISMATCH,
                f"{len(missing)} required gold doc_id(s) are absent from the ingested "
                f"corpus, e.g. {missing[:3]}; scoring would report a silent 0.0",
                "characterize.join",
                missing=len(missing),
            )
        )

    limit = scenario.max_measurable_k
    db_dir = tempfile.mkdtemp(prefix="earp-arm-")
    engine = None
    cache: dict[str, list[str]] = {}
    errors: dict[str, str] = {}
    retrievals = 0

    try:
        engine = Engine.open(
            str(Path(db_dir) / "corpus.db"),
            use_default_embedder=scenario.use_default_embedder,
        )
        if scenario.projections:
            # BEFORE ingest (the S7 order): the engine backfills
            # same-transaction FTS builds on a fresh, empty database.
            from fathomdb.types import ProjectionSpec  # noqa: PLC0415

            engine.configure_projections(
                [
                    ProjectionSpec(
                        name=projection.name,
                        roles=frozenset(projection.roles),
                        fts=projection.fts,
                        vector=projection.vector,
                    )
                    for projection in scenario.projections
                ]
            )
        engine.write(items)

        params = {
            PARAM_RENAMES.get(key, key): value
            for key, value in scenario.query_params.items()
            if key != "text"
        }
        call = (
            retrieve_override
            if retrieve_override is not None
            else resolve_call(engine, scenario.query_call)
        )
        # Retrieve ONCE per query with the arm's resolved public limit.
        for query in gold_set.queries:
            retrievals += 1
            try:
                if retrieve_override is not None:
                    result = call(query.query)
                else:
                    result = call(query.query, **params)
                cache[query.query_id] = [
                    hit.id.value
                    for hit in result.results[:limit]
                    if getattr(hit.id, "space", None) == "logical"
                ]
            except Exception as exc:  # noqa: BLE001 -- typed per-query failure
                errors[query.query_id] = f"{type(exc).__name__}: {exc}"
    finally:
        if engine is not None:
            try:
                engine.close()
            except Exception:  # noqa: BLE001, S110
                pass
        shutil.rmtree(db_dir, ignore_errors=True)

    def _cached(query: GoldQuery) -> list[str]:
        if query.query_id in errors:
            raise RuntimeError(errors[query.query_id])
        return cache.get(query.query_id, [])

    ladder = tuple(sorted(set(scenario.evidence_recall_k)))
    per_k = {k: aggregate(gold_set.queries, _cached, k=k) for k in ladder}
    rows = _per_query_rows(gold_set.queries, cache, errors, ladder)
    return ArmExecution(
        ingested=len(items),
        retrievals=retrievals,
        cache=cache,
        errors=errors,
        per_k=per_k,
        rows=rows,
        queries=gold_set.queries,
    )


def run_characterization(
    *,
    data_root: Path,
    snapshot_path: Path,
    gold_path: Path,
    gold_sha256: str,
    corpus_hash: str,
    qrels_version: str,
    experiments_root: Path,
    experiment: str,
    ts: datetime,
    evidence_recall_k: Sequence[int] = (5, 10),
    manifest_path: Path | None = None,
    retrieve_override: Callable[[str], Any] | None = None,
    blank_provenance: bool = False,
) -> CharacterizationResult:
    """Ingest, verify gold, score, and write. Retrieval happens ONCE per query."""
    config_doc = {
        "schema_version": "earp.v1",
        "campaign": "characterization",
        "corpus": {"snapshot": str(snapshot_path), "data_root": str(data_root)},
        "gold": {
            "path": str(gold_path),
            "sha256": gold_sha256,
            "corpus_hash": corpus_hash,
            "qrels_version": qrels_version,
        },
        "scenario": {
            "engine": {"use_default_embedder": False},
            "query": {"call": "Engine.search_text_only"},
        },
        "metrics": {"evidence_recall_k": list(evidence_recall_k)},
    }

    # characterize() never resolves a config, so it cannot take "the resolved
    # limit": the deepest rung IS the limit it passes -- explicitly -- at the
    # search call below. Before S6a it truncated to max(ladder) while calling
    # with the engine default, so a (5, 10, 50) ladder silently scored @50
    # over 10 hits.
    ladder = tuple(sorted(set(evidence_recall_k)))
    deepest = max(ladder)

    def _blocked_result(blocker: Blocker) -> CharacterizationResult:
        outcome = _write(
            config_doc, experiments_root, experiment, ts, RunVerdict.BLOCKED,
            {}, [], (blocker,), blocker.message, blank_provenance, deepest,
        )
        return CharacterizationResult(
            verdict=RunVerdict.BLOCKED,
            run_id=outcome.run_id,
            run_dir=outcome.run_dir,
            fanout_used=deepest,
            blockers=(blocker,),
            config_doc=config_doc,
        )

    depth_blocker = check_depth(RetrievalMode.FTS_ONLY, deepest, ENGINE_MAX_RESULT_LIMIT)
    if depth_blocker is not None:
        return _blocked_result(depth_blocker)

    # The thin single-arm wrapper (S8): the arm executor does the work; this
    # function keeps its historical `limit = max(ladder)` behaviour by
    # synthesizing the scenario itself, and keeps writing its own records.
    execution = execute_arm(
        scenario=ResolvedScenario(
            campaign=CampaignKind.CHARACTERIZATION,
            config_sha256=_lib.config_sha256(dict(config_doc)),
            query_call="Engine.search_text_only",
            retrieval_mode=RetrievalMode.FTS_ONLY,
            max_measurable_k=deepest,
            use_default_embedder=False,
            query_params={"limit": deepest},
            evidence_recall_k=ladder,
            document_metrics=(),
            corpus=config_doc["corpus"],
            gold=config_doc["gold"],
            decision_rule=None,
            consumed_paths=frozenset(),
            carried_paths=frozenset(),
        ),
        data_root=Path(data_root),
        snapshot_path=Path(snapshot_path),
        gold_path=Path(gold_path),
        gold_sha256=gold_sha256,
        corpus_hash=corpus_hash,
        qrels_version=qrels_version,
        manifest_path=manifest_path,
        retrieve_override=retrieve_override,
    )
    if execution.blocker is not None:
        return _blocked_result(execution.blocker)

    outcome = _write(
        config_doc, experiments_root, experiment, ts, RunVerdict.COMPLETE,
        _metrics_document(execution.per_k), execution.rows, (),
        f"characterization over {execution.ingested} docs, "
        f"{len(execution.queries)} queries",
        blank_provenance, deepest,
    )
    return CharacterizationResult(
        verdict=RunVerdict.COMPLETE,
        run_id=outcome.run_id,
        run_dir=outcome.run_dir,
        ingested=execution.ingested,
        retrievals=execution.retrievals,
        fanout_used=deepest,
        per_k=execution.per_k,
        document_metrics={"ndcg": resolve_ndcg(has_graded_relevance=False)},
        per_query_rows=execution.rows,
        config_doc=config_doc,
        code_sha=_git_sha(blank_provenance),
        python_version="" if blank_provenance else platform.python_version(),
    )


def _per_query_rows(
    queries: Sequence[GoldQuery],
    cache: Mapping[str, list[str]],
    errors: Mapping[str, str],
    ladder: Sequence[int],
) -> list[dict[str, Any]]:
    from eval.earp.metrics import evidence_recall_at_k, negative_abstained  # noqa: PLC0415

    rows: list[dict[str, Any]] = []
    for query in queries:
        for k in ladder:
            row: dict[str, Any] = {
                "schema_version": SCHEMA_VERSION_PER_QUERY,
                "query_id": query.query_id,
                "query_class": query.query_class,
                "k": k,
            }
            if query.query_id in errors:
                row.update({"outcome": "error", "reason": errors[query.query_id]})
                rows.append(row)
                continue
            retrieved = cache.get(query.query_id, [])
            #: Truncated to k -- an untruncated corpus-scale result is ~5,000
            #: ids per row, which across 9,194 rows is a ~1.5 GB sidecar.
            row["retrieved_doc_ids"] = retrieved[:k]
            row["retrieved_n"] = len(retrieved)
            if query.query_class == "negative":
                row.update({"outcome": "scored", "abstained": negative_abstained(retrieved, k)})
                row.update({"strict": None, "graded": None, "required_n": 0, "required_hits": 0})
            else:
                recall = evidence_recall_at_k(query, retrieved, k)
                row.update(
                    {
                        "outcome": "scored",
                        "strict": recall.strict,
                        "graded": recall.graded,
                        "required_n": recall.required_n,
                        "required_hits": recall.required_hits,
                        "supporting_coverage": recall.supporting_coverage,
                    }
                )
            rows.append(row)
    return rows


def _metrics_document(per_k: Mapping[int, KResult]) -> dict[str, Any]:
    return {
        "per_k": {
            str(k): {
                "n": result.overall.n,
                "strict_evidence_recall": {
                    "status": "emitted",
                    "value": result.overall.strict(),
                },
                "graded_evidence_recall": {
                    "status": "emitted",
                    "value": result.overall.graded(),
                },
                "supporting_coverage": {
                    "status": "not_applicable",
                    "value": None,
                    "reason": "no gold in this repo carries supporting units",
                },
                "supporting_query_n": result.overall.supporting_query_n,
            }
            for k, result in per_k.items()
        },
        "document_metrics": {
            "ndcg": {
                "status": "not_applicable",
                "value": None,
                "reason": "nDCG requires graded relevance; no gold set carries it",
            }
        },
        **_negative_class_document(per_k),
    }


def _negative_class_document(per_k: Mapping[int, KResult]) -> dict[str, Any]:
    """The k-free `negative_class` aggregate (S0 declared the slot; nothing
    wrote it until 2026-08-08 — Campaign 1 had to derive it by hand).

    Abstention is K-independent — a non-empty ranked list is non-empty at
    every K >= 1 — so every rung carries the identical `NegativeAgg`. That
    invariant is asserted rather than assumed: divergence would mean the
    rungs scored different query sets, a scoring bug, not a state to record
    silently.
    """
    if not per_k:
        return {}
    aggs = {(r.negative.n, r.negative.abstained) for r in per_k.values()}
    if len(aggs) != 1:
        raise AssertionError(f"negative aggregates diverge across K rungs: {sorted(aggs)}")
    n, abstained = next(iter(aggs))
    rate: dict[str, Any] = (
        {"status": "emitted", "value": abstained / n}
        if n
        else {
            "status": "not_applicable",
            "value": None,
            "reason": "gold has no negative queries",
        }
    )
    return {
        "negative_class": {
            "n": n,
            "abstention_correct": abstained,
            "abstention_rate": rate,
        }
    }


def _git_sha(blank: bool) -> str:
    if blank:
        return ""
    try:
        return _lib.git_info()["git_sha"]
    except Exception:  # noqa: BLE001 -- outside a repo, provenance is absent
        return ""


def _write(
    config_doc: Mapping[str, Any],
    experiments_root: Path,
    experiment: str,
    ts: datetime,
    verdict: RunVerdict,
    metrics: Mapping[str, Any],
    rows: Sequence[Mapping[str, Any]],
    blockers: tuple[Blocker, ...],
    read: str,
    blank_provenance: bool,
    fanout_used: int,
) -> Any:
    sha = _lib.config_sha256(dict(config_doc))
    run_id = _lib.make_run_id(experiment, ts, sha)
    sidecar = {
        "schema_version": SCHEMA_VERSION_RESULT,
        "run_id": run_id,
        "campaign": "characterization",
        "verdict": verdict.value,
        "scenario": {
            "config_sha256": sha,
            "query_call": "Engine.search_text_only",
            "retrieval_mode": "fts_only",
            "fanout_used": fanout_used,
        },
        "metrics": dict(metrics),
        "witnesses": [],
        "blockers": [
            {"code": b.code.value, "message": b.message, "stage": b.stage, "detail": b.detail}
            for b in blockers
        ],
    }
    # `_lib.git_info`/`env_info` exist and S5 simply did not call them, which
    # made the code and env drift axes unrecoverable for every record it wrote.
    code = {"git_sha": _git_sha(blank_provenance), "dirty": False, "branch": "", "baseline_commit": None}
    env = {
        "python": "" if blank_provenance else platform.python_version(),
        "lockfile_sha256": None,
        "gpu": None,
        "key_deps": {},
    }
    return write_run(
        experiment=experiment,
        ts=ts,
        config_doc=config_doc,
        experiments_root=experiments_root,
        verdict=verdict,
        read=read,
        metrics=dict(metrics),
        per_query=list(rows),
        sidecar=sidecar,
        code=code,
        env=env,
        corpus={"source": None, "manifest_sha256": None, "datasets": []},
        seeds={},
        cost_usd=0.0,
    )


def replay(
    *,
    run_id: str,
    experiments_root: Path,
    config_doc: Mapping[str, Any],
    code: Mapping[str, Any],
    env: Mapping[str, Any],
) -> ReplayReport:
    """Re-resolve a stored run and report drift, without ruling on it."""
    record = json.loads(
        (Path(experiments_root) / "runs" / run_id / "record.json").read_text(encoding="utf-8")
    )
    drift: list[Drift] = []

    prior_sha = record["config"]["sha256"]
    now_sha = _lib.config_sha256(dict(config_doc))
    if prior_sha != now_sha:
        drift.append(Drift(DriftAxis.CONFIG, prior_sha, now_sha))

    prior_code = record["code"].get("git_sha") or ""
    now_code = str(code.get("git_sha") or "")
    if prior_code != now_code:
        drift.append(
            Drift(DriftAxis.CODE, prior_code, now_code, unrecoverable=not prior_code)
        )

    prior_env = record["env"].get("python") or ""
    now_env = str(env.get("python") or "")
    if prior_env != now_env:
        drift.append(
            Drift(DriftAxis.ENVIRONMENT, prior_env, now_env, unrecoverable=not prior_env)
        )

    return ReplayReport(run_id=run_id, drift=tuple(drift))


__all__ = [
    "ArmExecution",
    "CharacterizationResult",
    "Drift",
    "DriftAxis",
    "ReplayReport",
    "execute_arm",
    "load_corpus",
    "replay",
    "run_characterization",
]
