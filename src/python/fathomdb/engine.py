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
from typing import Any, cast

from fathomdb._fathomdb import Engine as _NativeEngine
from fathomdb._fathomdb import IngestWithExtractorReceipt
from fathomdb.config import EngineConfig
from fathomdb.types import (
    CounterSnapshot,
    EmbedderIdentity,
    MigrationStepReport,
    OpenReport,
    SearchFilter,
    SearchHit,
    SearchResult,
    SoftFallback,
    SoftFallbackBranch,
    WriteReceipt,
)

_KWARG_FIELDS = {
    "embedder_pool_size",
    "scheduler_runtime_threads",
    "provenance_row_cap",
    "embedder_call_timeout_ms",
    "slow_threshold_ms",
}


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

    def search(
        self,
        query: str,
        filter: SearchFilter | None = None,
        *,
        rerank_depth: int = 0,
        use_graph_arm: bool = False,
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

        Returns:
            ``SearchResult`` with RRF-fused (and optionally CE-reranked) hits.
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
        if filter is None:
            result = self._native.search(
                query, rerank_depth=rerank_depth, use_graph_arm=use_graph_arm
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
            )
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
                    id=hit.id,
                    kind=hit.kind,
                    body=hit.body,
                    score=hit.score,
                    branch=cast(SoftFallbackBranch, hit.branch),
                )
                for hit in result.results
            ],
        )

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
