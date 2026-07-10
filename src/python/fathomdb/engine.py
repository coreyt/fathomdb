"""Python wrapper around the native PyO3 engine handle.

`Engine` mirrors the public five-verb surface owned by
`dev/interfaces/python.md`. The native PyO3 class
(`fathomdb._fathomdb.Engine`) holds the `Arc<fathomdb_engine::Engine>`
and runs every blocking call under `py.allow_threads`; this Python
wrapper converts native return values into the dataclasses in
`fathomdb.types` and rejects unknown `open()` kwargs.
"""

from __future__ import annotations

import logging
import math
from typing import Any, cast

from fathomdb._fathomdb import ConsolidateReceipt
from fathomdb._fathomdb import Engine as _NativeEngine
from fathomdb._fathomdb import IngestWithExtractorReceipt
from fathomdb.config import EngineConfig
from fathomdb.types import (
    CounterSnapshot,
    EmbedderIdentity,
    Explanation,
    IdSpace,
    MigrationStepReport,
    OpenReport,
    PerHitExplain,
    QueryTrace,
    SearchFilter,
    SearchHit,
    SearchResult,
    SoftFallback,
    SoftFallbackBranch,
    WriteReceipt,
)
from fathomdb.filter import Filter

_KWARG_FIELDS = {
    "embedder_pool_size",
    "scheduler_runtime_threads",
    "provenance_row_cap",
    "embedder_call_timeout_ms",
    "slow_threshold_ms",
}


def _validate_id_list(name: str, value: object) -> list[int]:
    """0.8.8 Slice 15 — validate a relevance-label id list before the native
    call (mirrors the TS ``validateIdArray`` guard for cross-SDK parity). Ids
    are non-negative ints — the telemetry ``result_ids`` / ``write_cursor`` key
    space (the pre-0.8.19 ``SearchHit.id``), NOT the post-C-2 typed
    ``SearchHit.id``. ``bool`` is rejected explicitly (it is an int subclass that
    PyO3 would otherwise coerce silently)."""
    if not isinstance(value, list):
        raise TypeError(
            f"{name} must be a list of non-negative ints, got {type(value).__name__!r}"
        )
    for item in value:
        if not isinstance(item, int) or isinstance(item, bool):
            raise TypeError(
                f"{name} must contain only non-negative ints, got {type(item).__name__!r}"
            )
        if item < 0:
            raise ValueError(f"{name} must contain only non-negative ints, got {item!r}")
    return value


def _map_per_hit_explain(p: Any) -> PerHitExplain:
    """Map one native per-hit explain object into the public
    :class:`fathomdb.types.PerHitExplain` dataclass.

    Factored out of :meth:`Engine.search` so the mapping is unit-testable
    against a fake native per-hit object without the compiled ``_fathomdb``
    extension (0.8.16 Slice 5 / F9, codex §9 fix-2). ``importance``/``confidence``
    are the additive F9 fields (node importance / edge confidence applied to this
    hit's contribution; ``None`` = graceful-absent / neutral), symmetric with the
    TypeScript ``perHit`` mapping.
    """
    return PerHitExplain(
        id=p.id,
        arm=cast(SoftFallbackBranch, p.arm),
        vector_rank=p.vector_rank,
        text_rank=p.text_rank,
        graph_rank=p.graph_rank,
        fused_score=p.fused_score,
        ce_score=p.ce_score,
        blended=p.blended,
        importance=p.importance,
        confidence=p.confidence,
    )


class Engine:
    """Python handle that wraps the native PyO3 engine."""

    __slots__ = ("_native", "_path", "_config")

    def __init__(
        self,
        native: _NativeEngine,
        *,
        path: str,
        config: EngineConfig,
    ) -> None:
        self._native = native
        self._path = path
        self._config = config

    @classmethod
    def open(
        cls,
        path: str,
        *,
        config: EngineConfig | None = None,
        use_default_embedder: bool = False,
        **engine_config: Any,
    ) -> "Engine":
        """Open the database at `path`.

        Either `config` or per-knob keyword arguments may be supplied,
        but not both. Unknown keyword arguments are rejected.

        EU-6: ``use_default_embedder`` opts into the engine's pinned
        default embedder (``fathomdb-bge-small-en-v1.5``). On first use,
        weights are downloaded from HuggingFace and cached under
        ``~/.cache/fathomdb/embedders/``. The default (``False``) opens
        without an embedder; subsequent vector writes fail with
        ``EmbedderNotConfiguredError``. Caller-supplied custom embedders
        are deferred to a later release (see ``dev/interfaces/python.md``).
        """

        if config is not None and engine_config:
            raise ValueError(
                "Engine.open accepts either config= or per-knob keyword arguments, not both",
            )

        unknown = set(engine_config) - _KWARG_FIELDS
        if unknown:
            raise TypeError(
                f"Engine.open got unexpected keyword arguments: {sorted(unknown)!r}",
            )

        resolved = config if config is not None else EngineConfig(**engine_config)
        native = _NativeEngine.open(path, use_default_embedder=use_default_embedder)
        return cls(native, path=path, config=resolved)

    @property
    def path(self) -> str:
        return self._path

    @property
    def config(self) -> EngineConfig:
        return self._config

    def write(self, batch: list[Any] | None = None) -> WriteReceipt:
        receipt = self._native.write(batch or [])
        return WriteReceipt(
            cursor=receipt.cursor,
            row_cursors=tuple(receipt.row_cursors),
            dangling_edge_endpoints=receipt.dangling_edge_endpoints,
        )

    def embed(self, text: str) -> list[float]:
        """Embed ``text`` with the engine's pinned default embedder
        (``fathomdb-bge-small-en-v1.5``) and return the raw vector.

        Read-path primitive for callers that need vectors under the engine's
        own embedder identity (e.g. coverage-index clustering) rather than a
        parallel, possibly-divergent embedder. Raises
        ``EmbedderNotConfiguredError`` if the engine was opened without an
        embedder (``use_default_embedder=False``)."""
        return list(self._native.embed(text))

    def search(
        self,
        query: str,
        filter: SearchFilter | Filter | None = None,
        *,
        rerank_depth: int = 0,
        use_graph_arm: bool = False,
        alpha: float | None = None,
        pool_n: int | None = None,
        explain: bool = False,
    ) -> SearchResult:
        """Hybrid search with optional CE reranking and optional graph-BFS arm.

        Args:
            query: Free-text search query.
            filter: Optional closed metadata filter (``SearchFilter``).
            rerank_depth: 0 (default) = soft-fallback / identity (no CE).
                N > 0 = rerank the top-N fused hits with the cross-encoder.
                Must be a non-negative integer. Negative values raise
                ``ValueError``.
            use_graph_arm: When ``True``, seed a BFS over temporal fact-edges
                from the top-10 fused hits and fuse reachable nodes as a third
                RRF arm (Slice 30 R3). Default ``False`` → byte-identical to
                the pre-Slice-30 two-arm pipeline.
            alpha: 0.8.5 (EXP-0) CE-blend weight, clamped to ``[0, 1]`` in the
                engine. ``None`` (default) ⇒ 0.3, the C6 factoid-guard default;
                ``1.0`` is the measured Mem0-parity config. Opt-in for the
                agentic-answer/memory path — the default protects naive lookups.
            pool_n: 0.8.5 (EXP-0) reranked-pool size. ``None`` (default) ⇒
                ``rerank_depth`` (preserves today's pool == depth semantics).

        Returns:
            ``SearchResult`` with RRF-fused (and optionally CE-reranked) hits.
            Each hit carries ``ce_score`` (the CE score for in-pool reranked
            hits, ``None`` otherwise).
        """
        # FIX-3: reject bool and non-int before the negative check.
        # bool is a subclass of int in Python so it passes isinstance(x, int);
        # we reject it explicitly for X1 parity with TypeScript.
        if not isinstance(rerank_depth, int) or isinstance(rerank_depth, bool):
            raise TypeError(
                f"rerank_depth must be a non-negative integer, got {type(rerank_depth).__name__!r}"
            )
        if rerank_depth < 0:
            raise ValueError(
                f"rerank_depth must be >= 0, got {rerank_depth!r}"
            )
        if not isinstance(use_graph_arm, bool):
            raise TypeError(
                f"use_graph_arm must be a bool, got {type(use_graph_arm).__name__!r}"
            )
        # 0.8.8 EXP-OBS (Slice 10) — validate `explain` before the native call,
        # mirroring use_graph_arm + the TS `search` guard (cross-SDK parity).
        if not isinstance(explain, bool):
            raise TypeError(
                f"explain must be a bool, got {type(explain).__name__!r}"
            )
        # 0.8.5 (codex §9 P2-2) — validate the new α/pool_n knobs before the native
        # call, mirroring the rerank_depth guard and the TS `search` validation
        # (cross-SDK parity). bool is rejected explicitly (it is an int/float
        # subclass that PyO3 would otherwise coerce silently).
        if alpha is not None:
            if isinstance(alpha, bool) or not isinstance(alpha, (int, float)):
                raise TypeError(
                    f"alpha must be a finite number, got {type(alpha).__name__!r}"
                )
            if not math.isfinite(alpha):
                raise ValueError(f"alpha must be a finite number, got {alpha!r}")
        if pool_n is not None:
            if not isinstance(pool_n, int) or isinstance(pool_n, bool):
                raise TypeError(
                    f"pool_n must be a non-negative integer, got {type(pool_n).__name__!r}"
                )
            if pool_n < 0:
                raise ValueError(f"pool_n must be >= 0, got {pool_n!r}")
        # 0.8.11 Slice 40 (#17) — accept the unified Filter on the vec0 search
        # path; lower to the SearchFilter sugar (typed-rejects a Json term, D3).
        if isinstance(filter, Filter):
            filter = filter.to_search_filter()
        if filter is None:
            result = self._native.search(
                query,
                rerank_depth=rerank_depth,
                use_graph_arm=use_graph_arm,
                alpha=alpha,
                pool_n=pool_n,
                explain=explain,
            )
        else:
            result = self._native.search(
                query,
                source_type=filter.source_type,
                kind=filter.kind,
                created_after=filter.created_after,
                status=filter.status,
                rerank_depth=rerank_depth,
                use_graph_arm=use_graph_arm,
                alpha=alpha,
                pool_n=pool_n,
                explain=explain,
            )
        fallback = result.soft_fallback
        soft = (
            SoftFallback(branch=cast(SoftFallbackBranch, fallback.branch))
            if fallback is not None
            else None
        )
        # 0.8.8 EXP-OBS (Slice 10) — convert the opt-in native explanation sidecar
        # into dataclasses; `None` (default explain=False) stays `None`.
        native_exp = result.explanation
        explanation = (
            Explanation(
                trace=QueryTrace(
                    query_chars=native_exp.trace.query_chars,
                    k=native_exp.trace.k,
                    rerank_depth=native_exp.trace.rerank_depth,
                    pool_n=native_exp.trace.pool_n,
                    alpha=native_exp.trace.alpha,
                    use_graph_arm=native_exp.trace.use_graph_arm,
                    recency=native_exp.trace.recency,
                    embedder_id=native_exp.trace.embedder_id,
                    ce_active=native_exp.trace.ce_active,
                    vector_hits=native_exp.trace.vector_hits,
                    text_hits=native_exp.trace.text_hits,
                    graph_hits=native_exp.trace.graph_hits,
                ),
                per_hit=[_map_per_hit_explain(p) for p in native_exp.per_hit],
            )
            if native_exp is not None
            else None
        )
        return SearchResult(
            projection_cursor=result.projection_cursor,
            soft_fallback=soft,
            results=[
                SearchHit(
                    id=IdSpace(space=hit.id.space, value=hit.id.value),
                    kind=hit.kind,
                    body=hit.body,
                    score=hit.score,
                    branch=cast(SoftFallbackBranch, hit.branch),
                    source_id=hit.source_id,
                    ce_score=hit.ce_score,
                )
                for hit in result.results
            ],
            explanation=explanation,
        )

    def search_text_only(self, query: str) -> SearchResult:
        """0.8.18 Slice 5 (#5 vector-equivalence probe) — text-only / FTS-only search.

        Does NOT embed the query and NEVER raises
        ``VectorEquivalenceMismatchError``, so it stays serviceable when the engine
        opened in the degraded ``dense_disabled`` state (the D2 "keep FTS servable"
        contract). Returns node-body FTS hits only — no vector recall, no CE
        rerank, no graph arm.
        """
        result = self._native.search_text_only(query)
        fallback = result.soft_fallback
        soft = (
            SoftFallback(branch=cast(SoftFallbackBranch, fallback.branch))
            if fallback is not None
            else None
        )
        return SearchResult(
            projection_cursor=result.projection_cursor,
            soft_fallback=soft,
            results=[
                SearchHit(
                    id=IdSpace(space=hit.id.space, value=hit.id.value),
                    kind=hit.kind,
                    body=hit.body,
                    score=hit.score,
                    branch=cast(SoftFallbackBranch, hit.branch),
                    source_id=hit.source_id,
                    ce_score=hit.ce_score,
                )
                for hit in result.results
            ],
            explanation=None,
        )

    def dense_disabled(self) -> bool:
        """0.8.18 Slice 5 (R-VEQ-6) — ``True`` iff the engine opened degraded.

        The open-time #5 self-check found a vector-equivalence divergence and every
        vector-dependent arm now refuses at query time with
        ``VectorEquivalenceMismatchError``. Mirrors ``OpenReport.dense_disabled``.
        """
        return self._native.dense_disabled()

    def dense_disabled_reason(self) -> str | None:
        """0.8.18 Slice 5 (R-VEQ-6) — reason for the degraded state, or ``None``."""
        return self._native.dense_disabled_reason()

    def vector_equivalence_refusal_count(self) -> int:
        """0.8.18 Slice 5 (R-VEQ-6) — count of query-time dense-arm refusals."""
        return self._native.vector_equivalence_refusal_count()

    def enable_telemetry(self, sink_path: str) -> None:
        """0.8.8 Slice 15 (OPP-9) — enable opt-in local telemetry capture to a
        JSONL ``sink_path``. Off by default; local file only (no egress). Once
        enabled, each ``search`` records a query→result event keyed on the
        stable id, and ``record_feedback`` appends correlated agent labels.
        The query text and ``source_id`` are NEVER written (privacy, ADR §C)."""
        if not isinstance(sink_path, str):
            raise TypeError(
                f"sink_path must be a str, got {type(sink_path).__name__!r}"
            )
        self._native.enable_telemetry(sink_path)

    def last_telemetry_query_id(self) -> str | None:
        """0.8.8 Slice 15 — the most-recent captured ``query_id`` (for
        ``record_feedback``), or ``None`` when telemetry is off / no query has
        been captured yet."""
        return self._native.last_telemetry_query_id()

    def record_feedback(
        self,
        query_id: str,
        relevant_ids: list[int],
        irrelevant_ids: list[int],
        label_source: str,
    ) -> None:
        """0.8.8 Slice 15 — attach agent relevance labels for a previously
        captured ``query_id``. ``relevant_ids`` / ``irrelevant_ids`` are the
        telemetry ``result_ids`` / ``write_cursor`` keys (the pre-0.8.19
        ``SearchHit.id`` space), NOT the post-C-2 typed ``SearchHit.id``;
        ``label_source`` is the caller-declared label origin (e.g.
        ``"agent:hermes"``). Raises when telemetry is off."""
        if not isinstance(query_id, str):
            raise TypeError(
                f"query_id must be a str, got {type(query_id).__name__!r}"
            )
        if not isinstance(label_source, str):
            raise TypeError(
                f"label_source must be a str, got {type(label_source).__name__!r}"
            )
        relevant = _validate_id_list("relevant_ids", relevant_ids)
        irrelevant = _validate_id_list("irrelevant_ids", irrelevant_ids)
        self._native.record_feedback(query_id, relevant, irrelevant, label_source)

    def close(self) -> None:
        self._native.close()

    def drain(self, *, timeout_s: float | int = 0) -> None:
        """Block until in-flight writes drain or `timeout_s` elapses."""

        self._native.drain(timeout_s=float(timeout_s))

    def ingest_with_extractor(
        self,
        cmd: list[str],
        documents: list[dict[str, str]],
    ) -> IngestWithExtractorReceipt:
        """G11 (Slice 15) — BYO-LLM ingest via the fathomdb.extract.v1 protocol.

        ``cmd`` is argv (first element = program, rest = args).
        ``documents`` is a list of dicts with ``source_doc_id`` and ``body`` keys.
        """

        return self._native.ingest_with_extractor(cmd, documents)

    def consolidate_with_provider(
        self,
        cmd: list[str],
        axes: list[dict[str, str]],
    ) -> ConsolidateReceipt:
        """0.8.12 Slice 15 (OPP-2) — consolidation / recency via a BYO-LLM
        harness speaking the ``fathomdb.consolidate.v1`` protocol.

        ``cmd`` is argv (first element = program, rest = args).
        ``axes`` is a list of dicts with ``subject_logical_id`` and ``relation``
        keys; each names one (subject, relation) cluster to consolidate.
        """

        return self._native.consolidate_with_provider(cmd, axes)

    def open_report(self) -> OpenReport:
        """Return the structured open-time report captured at `Engine.open`.

        Shape D (locked HITL 2026-05-24): the report is exposed as an
        engine-attached accessor, not a return-shape change on
        `Engine.open`. Idempotent — repeat calls return the same data;
        the report is a snapshot from open time, not live state.
        """

        native = self._native.open_report()
        return OpenReport(
            schema_version_before=native.schema_version_before,
            schema_version_after=native.schema_version_after,
            migration_steps=[
                MigrationStepReport(
                    step_id=step.step_id,
                    duration_ms=step.duration_ms,
                    failed=step.failed,
                )
                for step in native.migration_steps
            ],
            embedder_warmup_ms=native.embedder_warmup_ms,
            query_backend=native.query_backend,
            default_embedder=EmbedderIdentity(
                name=native.default_embedder.name,
                revision=native.default_embedder.revision,
                dimension=native.default_embedder.dimension,
            ),
            embedder_download_ms=native.embedder_download_ms,
            embedder_events=list(native.embedder_events),
            embedder_mean_centering_required=native.embedder_mean_centering_required,
            embedder_mean_vec_pinned=native.embedder_mean_vec_pinned,
            dense_disabled=native.dense_disabled,
            dense_disabled_reason=native.dense_disabled_reason,
        )

    def counters(self) -> CounterSnapshot:
        snap = self._native.counters()
        return CounterSnapshot(
            queries=snap.queries,
            writes=snap.writes,
            write_rows=snap.write_rows,
            admin_ops=snap.admin_ops,
            cache_hit=snap.cache_hit,
            cache_miss=snap.cache_miss,
        )

    def set_profiling(self, *, enabled: bool) -> None:
        self._native.set_profiling(enabled)

    def set_slow_threshold_ms(self, *, value: int) -> None:
        self._native.set_slow_threshold_ms(value)

    def attach_logging_subscriber(
        self,
        logger: logging.Logger,
        *,
        heartbeat_interval_ms: int | None = None,
    ) -> None:
        """Bind engine events into the supplied `logging.Logger`.

        Subscriber wiring lands in a later 0.6.x slice; the native call
        accepts the parameters so callers can wire a logger against the
        public surface.
        """

        self._native.attach_logging_subscriber(
            logger,
            heartbeat_interval_ms=heartbeat_interval_ms,
        )


__all__ = ["Engine"]
