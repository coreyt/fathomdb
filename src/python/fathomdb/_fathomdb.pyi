"""Type stubs for the PyO3 extension `fathomdb._fathomdb`.

Mirrors the surface emitted by `src/rust/crates/fathomdb-py/src/lib.rs`.
Hand-maintained — keep in sync with the binding's `#[pyclass]` /
`create_exception!` / `#[pyfunction]` exports.
"""

from typing import Any, Iterable

from fathomdb.types import EmbedderEvent

class WriteReceipt:
    cursor: int
    row_cursors: list[int]
    dangling_edge_endpoints: int

class SoftFallback:
    branch: str

class SearchHit:
    id: int
    kind: str
    body: str
    score: float
    branch: str
    source_id: str | None

class SearchResult:
    projection_cursor: int
    soft_fallback: SoftFallback | None
    results: list[SearchHit]

class CounterSnapshot:
    queries: int
    writes: int
    write_rows: int
    admin_ops: int
    cache_hit: int
    cache_miss: int

class NodeRecord:
    logical_id: str
    kind: str
    body: str
    write_cursor: int

class OpStoreRow:
    id: int
    collection: str
    record_key: str
    op_kind: str
    payload: str
    schema_id: str | None
    write_cursor: int

class IngestWithExtractorReceipt:
    """G11 (Slice 15) — BYO-LLM ingest receipt."""
    nodes_written: int
    edges_written: int
    docs_processed: int

class MigrationStepReport:
    step_id: int
    duration_ms: int | None
    failed: bool

class EmbedderIdentity:
    name: str
    revision: str
    dimension: int

class OpenReport:
    schema_version_before: int
    schema_version_after: int
    migration_steps: list[MigrationStepReport]
    embedder_warmup_ms: int
    query_backend: str
    default_embedder: EmbedderIdentity
    # EU-5a1/5a2/5b — surfaced by EU-6.
    embedder_download_ms: int | None
    embedder_events: list[EmbedderEvent]
    embedder_mean_centering_required: bool
    embedder_mean_vec_pinned: bool

class Engine:
    @staticmethod
    def open(path: str, use_default_embedder: bool = ...) -> "Engine": ...
    # NOTE: `_configure_vector_kind_for_test` and `_write_vector_for_test`
    # are intentionally NOT declared here. They only exist on the binary
    # when the `test-hooks` Cargo feature is enabled (see
    # `src/rust/crates/fathomdb-py/src/lib.rs::#[cfg(any(test, feature =
    # "test-hooks"))]`), and that feature is dev-only — it is not part of
    # the shipped wheel's feature axis (`pyproject.toml [tool.maturin]
    # features` and `release.yml` build-python's `args:`). Advertising
    # them on the public stub would imply they are callable by end users,
    # which is false.
    def open_report(self) -> OpenReport: ...
    def write(self, batch: Iterable[Any]) -> WriteReceipt: ...
    def search(
        self,
        query: str,
        source_type: str | None = ...,
        kind: str | None = ...,
        created_after: int | None = ...,
        status: str | None = ...,
        rerank_depth: int = ...,
        use_graph_arm: bool = ...,
    ) -> SearchResult: ...
    def close(self) -> None: ...
    def drain(self, timeout_s: float = ...) -> None: ...
    def ingest_with_extractor(
        self,
        cmd: list[str],
        documents: list[dict[str, str]],
    ) -> IngestWithExtractorReceipt:
        """G11 (Slice 15) — BYO-LLM ingest: spawn an external extraction harness
        speaking the fathomdb.extract.v1 protocol, extract entities + edges from
        documents, and write them to the store.

        ``cmd`` is argv (first element = program, rest = args).
        ``documents`` is a list of dicts with ``source_doc_id`` and ``body`` keys.
        """
        ...
    def counters(self) -> CounterSnapshot: ...
    def set_profiling(self, enabled: bool) -> None: ...
    def set_slow_threshold_ms(self, value: int) -> None: ...
    def attach_logging_subscriber(
        self,
        logger: Any,
        heartbeat_interval_ms: int | None = ...,
    ) -> None: ...

# Slice 20 (G5/G6) — graph traversal result types.

class ExpandedNode:
    """One node reached by BFS traversal that is NOT in the search hit set."""

    node: NodeRecord
    hop_count: int

class SearchExpandResult:
    """G6 result: original search hits + graph-expanded neighbors.

    Deduplication rule: a node in ``search_hits`` will NOT appear in
    ``expanded`` (search score takes priority).
    """

    search_hits: list[SearchHit]
    expanded: list[ExpandedNode]
    all_logical_ids: list[str]

def admin_configure(engine: Engine, name: str, body: str) -> WriteReceipt: ...
def read_get(engine: Engine, logical_id: str) -> NodeRecord | None: ...
def read_get_many(engine: Engine, logical_ids: list[str]) -> list[NodeRecord | None]: ...
def read_collection(
    engine: Engine,
    collection: str,
    after_id: int | None = ...,
    limit: int = ...,
) -> list[OpStoreRow]: ...
def read_mutations(
    engine: Engine,
    collection: str,
    after_id: int | None = ...,
    limit: int = ...,
) -> list[OpStoreRow]: ...
def read_list(
    engine: Engine,
    kind: str,
    predicates: list[dict[str, Any]] | None = ...,
    limit: int = ...,
) -> list[NodeRecord]: ...
def graph_neighbors(
    engine: Engine,
    logical_id: str,
    depth: int,
    direction: str,
) -> list[NodeRecord]:
    """G5 — bounded BFS from ``logical_id`` over ``canonical_edges``.

    ``depth`` must be 1–3; raises ``InvalidArgumentError`` for depth > 3.
    ``direction`` is one of ``"outgoing"``, ``"incoming"``, or ``"both"``.
    Returns nodes reachable within ``depth`` hops, hard-capped at 50.
    Valid-time filter: edges with ``t_invalid`` in the past are not traversed.
    """
    ...

def search_expand(
    engine: Engine,
    query: str,
    depth: int,
    source_type: str | None = ...,
    kind: str | None = ...,
    created_after: int | None = ...,
    status: str | None = ...,
) -> SearchExpandResult:
    """G6 — FTS/vector search followed by bounded BFS expansion.

    Runs ``search(query, ...)`` (G1), then expands each hit node via
    ``graph_neighbors(depth, both)``. Nodes that are both search hits
    and traversal neighbors appear only in ``search_hits`` (deduplicated).
    """
    ...


def rerank(
    query: str,
    passages: list[dict[str, Any]],
    rerank_depth: int,
) -> list[dict[str, Any]]:
    """0.8.2 Slice E2 — standalone CE rerank over an arbitrary passage list.

    Each ``passages`` entry is ``{"id": int, "body": str, "score": float}``
    (``score`` = the caller's fused/RRF score). Returns the reranked order as
    ``[{"id": int, "score": float}]`` where ``score`` is the CE-blended score.
    ``rerank_depth == 0`` OR an empty list returns the input order with input
    scores, byte-identical (no model load, no network). Unlike
    ``Engine.search(rerank_depth=...)`` — which reranks the engine's own capped
    text pool — this reranks the caller-supplied pool with the identical
    cross-encoder.
    """
    ...


def force_panic_for_test() -> None: ...

class EngineError(Exception): ...
class StorageError(EngineError): ...
class ProjectionError(EngineError): ...
class VectorError(EngineError): ...
class KindNotVectorIndexedError(VectorError): ...
class EmbedderError(EngineError): ...
class EmbedderNotConfiguredError(EmbedderError): ...
class SchedulerError(EngineError): ...
class OpStoreError(EngineError): ...
class WriteValidationError(EngineError): ...
class SchemaValidationError(EngineError): ...
class OverloadedError(EngineError): ...
class ClosingError(EngineError): ...

class DatabaseLockedError(EngineError):
    holder_pid: int | None
    def __init__(self, *args: Any, holder_pid: int | None = ...) -> None: ...

class CorruptionError(EngineError):
    kind: str
    stage: str
    recovery_hint_code: str
    doc_anchor: str
    def __init__(
        self,
        *args: Any,
        kind: str = ...,
        stage: str = ...,
        recovery_hint_code: str = ...,
        doc_anchor: str = ...,
    ) -> None: ...

class IncompatibleSchemaVersionError(EngineError): ...
class MigrationError(EngineError): ...

class EmbedderIdentityMismatchError(EngineError):
    stored_name: str
    stored_revision: str
    supplied_name: str
    supplied_revision: str
    def __init__(
        self,
        *args: Any,
        stored_name: str = ...,
        stored_revision: str = ...,
        supplied_name: str = ...,
        supplied_revision: str = ...,
    ) -> None: ...

class EmbedderDimensionMismatchError(EngineError):
    stored: int
    supplied: int
    def __init__(self, *args: Any, stored: int = ..., supplied: int = ...) -> None: ...

# G11 (Slice 15) — BYO-LLM extraction harness protocol error.
class ExtractorError(EngineError): ...

# G4 (Slice 35) — filter predicate construction error (non-allowlisted path).
class InvalidFilterError(EngineError): ...

# Slice 20 — depth > 3 or other argument validation failure.
class InvalidArgumentError(EngineError): ...
