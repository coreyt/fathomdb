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
from eval.earp.gold import GoldQuery, verify_gold
from eval.earp.metrics import KResult, aggregate, resolve_ndcg, validate_methodology
from eval.earp.schema.models import (
    SCHEMA_VERSION_PER_QUERY,
    SCHEMA_VERSION_RESULT,
    Blocker,
    BlockerCode,
    MetricValue,
    RunVerdict,
)
from eval.earp.writer import write_run

#: The production rerank floor, recorded with every number per IR-B (c).
DEFAULT_FANOUT = 10


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
    fanout_used: int = DEFAULT_FANOUT
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
    from fathomdb import Engine  # noqa: PLC0415

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

    def _blocked_result(blocker: Blocker) -> CharacterizationResult:
        outcome = _write(
            config_doc, experiments_root, experiment, ts, RunVerdict.BLOCKED,
            {}, [], (blocker,), blocker.message, blank_provenance,
        )
        return CharacterizationResult(
            verdict=RunVerdict.BLOCKED,
            run_id=outcome.run_id,
            run_dir=outcome.run_dir,
            blockers=(blocker,),
            config_doc=config_doc,
        )

    if not Path(data_root).is_dir():
        return _blocked_result(
            _blocked(
                BlockerCode.CORPUS_ROOT_ABSENT,
                f"configured corpus data_root does not exist: {data_root}",
                "characterize.ingest",
            )
        )

    items, ingest_blocker = _load_snapshot_shards(Path(data_root), Path(snapshot_path))
    if ingest_blocker is not None:
        return _blocked_result(ingest_blocker)

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
        return _blocked_result(blocker)

    gold_set = verification.gold_set
    assert gold_set is not None

    issues = validate_methodology(gold_set.queries)
    if issues:
        return _blocked_result(
            _blocked(
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
        return _blocked_result(
            _blocked(
                BlockerCode.GOLD_CORPUS_MISMATCH,
                f"{len(missing)} required gold doc_id(s) are absent from the ingested "
                f"corpus, e.g. {missing[:3]}; scoring would report a silent 0.0",
                "characterize.join",
                missing=len(missing),
            )
        )

    db_dir = tempfile.mkdtemp(prefix="earp-characterization-")
    engine = None
    cache: dict[str, list[str]] = {}
    errors: dict[str, str] = {}
    retrievals = 0
    ladder = tuple(sorted(set(evidence_recall_k)))
    deepest = max(ladder)

    try:
        engine = Engine.open(str(Path(db_dir) / "corpus.db"))
        engine.write(items)

        # Retrieve ONCE per query, truncated to the deepest rung. Calling the
        # engine per K rung would re-execute every search -- ~55 hours at
        # corpus scale instead of ~28.
        for query in gold_set.queries:
            retrievals += 1
            try:
                if retrieve_override is not None:
                    result = retrieve_override(query.query)
                else:
                    result = engine.search_text_only(query.query)
                cache[query.query_id] = [
                    hit.id.value
                    for hit in result.results[:deepest]
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

    per_k = {k: aggregate(gold_set.queries, _cached, k=k) for k in ladder}
    rows = _per_query_rows(gold_set.queries, cache, errors, ladder)

    outcome = _write(
        config_doc, experiments_root, experiment, ts, RunVerdict.COMPLETE,
        _metrics_document(per_k), rows, (),
        f"characterization over {len(items)} docs, {len(gold_set.queries)} queries",
        blank_provenance,
    )
    return CharacterizationResult(
        verdict=RunVerdict.COMPLETE,
        run_id=outcome.run_id,
        run_dir=outcome.run_dir,
        ingested=len(items),
        retrievals=retrievals,
        fanout_used=DEFAULT_FANOUT,
        per_k=per_k,
        document_metrics={"ndcg": resolve_ndcg(has_graded_relevance=False)},
        per_query_rows=rows,
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
            "fanout_used": DEFAULT_FANOUT,
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
    "DEFAULT_FANOUT",
    "CharacterizationResult",
    "Drift",
    "DriftAxis",
    "ReplayReport",
    "load_corpus",
    "replay",
    "run_characterization",
]
