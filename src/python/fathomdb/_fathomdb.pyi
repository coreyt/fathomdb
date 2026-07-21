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

# C-2 (0.8.19 / TC-8) — typed id-space carrier for SearchHit.id.
class IdSpace:
    # "logical" | "content" | "passage"
    space: str
    # bare id value (id-space prefix stripped)
    value: str

class SearchHit:
    # C-2 (0.8.19 / TC-8) — typed, non-null, id-space-total hit id (subsumes the
    # pre-0.8.19 int write_cursor id AND the additive stable_id).
    id: IdSpace
    kind: str
    body: str
    score: float
    branch: str
    source_id: str | None
    # 0.8.5 (EXP-0) — CE score (sigmoid of the cross-encoder logit) for in-pool
    # reranked hits; None otherwise (out-of-pool, identity path, or no CE model).
    ce_score: float | None

class QueryTrace:
    # 0.8.8 EXP-OBS (Slice 10) — query-level retrieval trace.
    query_chars: int
    k: int
    rerank_depth: int
    pool_n: int
    alpha: float
    use_graph_arm: bool
    recency: bool
    embedder_id: str
    ce_active: bool
    vector_hits: int
    text_hits: int
    graph_hits: int

class PerHitExplain:
    # 0.8.8 EXP-OBS (Slice 10) — per-hit provenance + score breakdown.
    id: int
    arm: str
    vector_rank: int | None
    text_rank: int | None
    graph_rank: int | None
    fused_score: float
    ce_score: float | None
    blended: float
    # 0.8.16 Slice 5 / F9 — node importance / edge confidence applied to this
    # hit's contribution (None = graceful-absent / neutral).
    importance: float | None
    confidence: float | None

class Explanation:
    # 0.8.8 EXP-OBS (Slice 10) — opt-in explanation sidecar.
    trace: QueryTrace
    per_hit: list[PerHitExplain]

class SearchResult:
    projection_cursor: int
    soft_fallback: SoftFallback | None
    results: list[SearchHit]
    # 0.8.8 EXP-OBS (Slice 10) — opt-in; None unless search(..., explain=True).
    explanation: Explanation | None

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

class ReadView:
    """0.8.20 Slice 10b (R-20-RV / R-20-NV) — read-mode / validity selector.

    Every field defaults to the STRICT view, so ``ReadView()`` is the shipped
    read behaviour. World-time only — no ``history_as_of``.
    """

    include_superseded: bool
    include_inactive: bool
    include_out_of_window: bool
    valid_as_of: int | None
    def __init__(
        self,
        include_superseded: bool = False,
        include_inactive: bool = False,
        include_out_of_window: bool = False,
        valid_as_of: int | None = None,
    ) -> None: ...

class BoundaryCrossing:
    node: NodeRecord
    became_valid_at: int | None
    became_invalid_at: int | None

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

class ConsolidateReceipt:
    """0.8.12 Slice 15 (OPP-2) — consolidation / recency provider receipt."""
    clusters_processed: int
    edges_examined: int
    edges_kept: int
    edges_invalidated: int
    edges_superseded: int

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
    # Write items are loose mappings, not typed structs, so the stub cannot
    # express their shape. A NODE item accepts:
    #   kind: str (required)        body: str
    #   source_id: str (required, 0.8.20 R-20-E3)
    #   logical_id: str | None      state: "pending" | "active" | None
    #   reason: str | None
    #   valid_from: int | None      valid_until: int | None
    # 0.8.20 Slice 15b (TC-34): `valid_from`/`valid_until` are the world-time
    # validity window in INTEGER epoch SECONDS, half-open
    # [valid_from, valid_until). Absent/None = unbounded on that side, which is
    # the back-compat default (valid at every instant). `valid_from >=
    # valid_until` raises InvalidArgumentError; a non-integer (including bool)
    # raises WriteValidationError. See `fathomdb.engine.Engine.write`.
    def write(self, batch: Iterable[Any]) -> WriteReceipt: ...
    def embed(self, text: str) -> list[float]: ...
    def search(
        self,
        query: str,
        source_type: str | None = ...,
        kind: str | None = ...,
        created_after: int | None = ...,
        status: str | None = ...,
        rerank_depth: int = ...,
        use_graph_arm: bool = ...,
        alpha: float | None = ...,
        pool_n: int | None = ...,
        explain: bool = ...,
        # 0.8.20 Slice 15b fix-2 (R-20-NV / R-20-RV) — optional VALIDITY view.
        # `None` (default) is the strict view: active-only, non-superseded, and
        # valid AT QUERY TIME. The EXISTENCE flags (`include_superseded` /
        # `include_inactive`) raise `InvalidArgumentError` on this path rather
        # than being silently ignored. See `fathomdb.engine.Engine.search`.
        view: ReadView | None = ...,
    ) -> SearchResult: ...
    # 0.8.18 Slice 5 (R-VEQ-4) — text-only / FTS-only path; takes the same
    # optional validity view as `search` (0.8.20 Slice 15b fix-2).
    def search_text_only(
        self, query: str, view: ReadView | None = ...
    ) -> SearchResult: ...
    # 0.8.8 Slice 15 (OPP-9) — opt-in local telemetry capture.
    def enable_telemetry(self, sink_path: str) -> None: ...
    def last_telemetry_query_id(self) -> str | None: ...
    def record_feedback(
        self,
        query_id: str,
        relevant_ids: list[int],
        irrelevant_ids: list[int],
        label_source: str,
    ) -> None: ...
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
    def consolidate_with_provider(
        self,
        cmd: list[str],
        axes: list[dict[str, str]],
    ) -> ConsolidateReceipt:
        """0.8.12 Slice 15 (OPP-2) — consolidation / recency via a BYO-LLM
        harness speaking the fathomdb.consolidate.v1 protocol.

        ``cmd`` is argv (first element = program, rest = args).
        ``axes`` is a list of dicts with ``subject_logical_id`` and ``relation``
        keys (a `ConsolidateAxis`); each names one (subject, relation) cluster to
        consolidate.
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
def transition(
    engine: Engine,
    logical_id: str,
    to_state: str,
    reason: str | None = ...,
) -> None: ...
def purge(engine: Engine, logical_id: str) -> None: ...

# 0.8.20 Slice 5d (R-20-E4) — the `erase_source` lifecycle verb + its report.
class EraseReport:
    source_ref: str
    nodes_excised: int
    edges_excised: int
    projections_invalidated: int

def erase_source(engine: Engine, source_id: str) -> EraseReport: ...

# 0.8.20 Slice 15d (R-20-PR) — projection registry verbs + their types.
class ProjectionSpec:
    name: str
    roles: list[str]
    fts: bool
    fts_tokenizer: str | None
    vector: bool
    vector_embedder: str | None
    def __init__(
        self,
        name: str,
        roles: list[str],
        fts: bool = ...,
        fts_tokenizer: str | None = ...,
        vector: bool = ...,
        vector_embedder: str | None = ...,
    ) -> None: ...

class ProjectionDelta:
    built: list[str]
    dropped: list[str]
    deferred: list[str]
    unchanged: bool

def configure_projections(
    engine: Engine, specs: list[ProjectionSpec], drop: list[str] | None = ...
) -> ProjectionDelta: ...
def read_projections(engine: Engine) -> list[ProjectionSpec]: ...
def read_get(
    engine: Engine, logical_id: str, view: ReadView | None = None
) -> NodeRecord | None: ...
def read_get_many(
    engine: Engine, logical_ids: list[str], view: ReadView | None = None
) -> list[NodeRecord | None]: ...
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
    view: ReadView | None = ...,
) -> list[NodeRecord]: ...
def read_list_filter(
    engine: Engine,
    kind: str,
    terms: list[dict[str, Any]] | None = ...,
    limit: int = ...,
    view: ReadView | None = ...,
) -> list[NodeRecord]: ...
def crossed_boundary_since(
    engine: Engine,
    since: int,
    view: ReadView | None = ...,
) -> list[BoundaryCrossing]: ...
def graph_neighbors(
    engine: Engine,
    logical_id: str,
    depth: int,
    direction: str,
    view: ReadView | None = ...,
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
    alpha: float | None = ...,
    pool_n: int | None = ...,
) -> list[dict[str, Any]]:
    """0.8.2 Slice E2 — standalone CE rerank over an arbitrary passage list.

    Each ``passages`` entry is ``{"id": int, "body": str, "score": float}``
    (``score`` = the caller's fused/RRF score). Returns the reranked order as
    ``[{"id": int, "score": float, "ce_score": float | None}]`` where ``score`` is
    the CE-blended score and ``ce_score`` is the per-candidate ``sigmoid(ce_logit)``
    (None outside the reranked pool). ``rerank_depth == 0`` OR an empty list returns
    the input order with input scores, byte-identical (no model load, no network).

    0.8.5 (EXP-0): ``alpha`` (default 0.3, clamped to [0,1]) is the CE-blend weight
    and ``pool_n`` (default = ``rerank_depth``) is the reranked-pool size. Omitting
    both reproduces the pre-slice α=0.3 blend; ``alpha=1.0, pool_n=10`` is the
    measured-parity config. Unlike ``Engine.search(rerank_depth=...)`` — which
    reranks the engine's own capped text pool — this reranks the caller-supplied
    pool with the identical cross-encoder.
    """
    ...


def embed_batch_cls(texts: list[str]) -> list[list[float]]:
    """V-3 dense-encoder path — CLS-pooled batch embed of ``texts``.

    Embeds each string with the pinned default bge-small weights using **CLS
    pooling** + L2-normalization (distinct from ``Engine.embed()``, which uses
    the engine's default Mean pooling), honoring ``FATHOMDB_EMBED_DEVICE``.
    Returns one ``list[float]`` per input, in input order; an empty input list
    returns ``[]``. Requires a wheel built with the ``default-embedder`` (or
    ``embed-cuda``) feature; otherwise raises ``EmbedderNotConfiguredError``.
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

# 0.8.12 Slice 15 (OPP-2) — consolidation harness protocol error.
class ConsolidatorError(EngineError): ...

# G4 (Slice 35) — filter predicate construction error (non-allowlisted path).
class InvalidFilterError(EngineError): ...

# Slice 20 — depth > 3 or other argument validation failure.
class InvalidArgumentError(EngineError): ...

# 0.8.18 Slice 5 (#5 vector-equivalence probe) — query-time dense-refusal leaf.
class VectorEquivalenceMismatchError(EngineError):
    reason: str

# OPP-12 Phase-1 (0.8.19 Slice 10) — lifecycle-verb typed errors. Parity-safe
# field names (S7): `from_state`/`to_state` (never `from`, a Python keyword).
class IllegalTransitionError(EngineError):
    from_state: str
    to_state: str
    legal: list[str]

class NotLifecycleAddressableError(EngineError):
    id_space: str

# 0.8.20 Slice 5b (R-20-E5) — an erasure verb deleted its rows but could not
# complete the erasure at rest (typically a WAL checkpoint blocked by a
# concurrent reader). `stage` names the uncompleted step.
class ErasureIncompleteError(EngineError):
    stage: str
    detail: str

# 0.8.20 Slice 15d (R-20-PR) — `configure_projections` refused a DESTRUCTIVE
# change to a live projection `name` (a role removal or tokenizer/embedder swap)
# that was NOT accompanied by an explicit `drop`. `delta` names WHAT changed; the
# caller re-issues with `drop: [name]` to consciously rebuild.
class ProjectionDestructiveError(EngineError):
    name: str
    delta: str
