"""Caller-visible result shapes for the FathomDB Python SDK.

Field names owned by `dev/interfaces/python.md` § Caller-visible data shapes.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Literal, TypedDict, TypeGuard, Union

#: Typed soft-fallback branch values per `dev/design/retrieval.md`.
SoftFallbackBranch = Literal["vector", "text"]


@dataclass(frozen=True)
class WriteReceipt:
    """Receipt returned by `engine.write` and `admin.configure`."""

    cursor: int
    #: G0 (Slice 15) — per-row ``write_cursor``s, 1:1 with the input batch
    #: order. The ``write_cursor``-as-row-id identity carrier; for an N-row
    #: batch this is ``[cursor - N + 1, …, cursor]``.
    row_cursors: tuple[int, ...] = ()
    #: G8 (Slice 20 / F10) — count of edge endpoints in this batch that point at
    #: a non-existent or superseded canonical node (an active node carrying that
    #: ``logical_id``). ``from_id``/``to_id`` are probed independently, so one
    #: edge contributes 0, 1, or 2. Informational only: the batch commits
    #: regardless (flag-and-count). ``0`` when the batch committed no active edges.
    dangling_edge_endpoints: int = 0


@dataclass(frozen=True)
class SoftFallback:
    """Hybrid-search soft-fallback signal.

    `branch` indicates which non-essential branch could not contribute. Total
    request failure is not expressed via this carrier (see
    `dev/design/retrieval.md`).
    """

    branch: SoftFallbackBranch


@dataclass(frozen=True)
class SearchHit:
    """One structured hit in a `SearchResult` (G1 / AC-057a-clean).

    `id` is the canonical row's `write_cursor` — the interim identity carrier
    per `dev/adr/ADR-0.8.0-canonical-identity-substrate.md`. `score` is the raw
    per-branch relevance (`vec_distance_l2` for the vector branch, `bm25()` for
    the text branch); the two are not comparable raw. `branch` tags which
    retrieval branch produced the hit.
    """

    id: int
    kind: str
    body: str
    score: float
    branch: SoftFallbackBranch


@dataclass(frozen=True)
class NodeRecord:
    """Slice 30 (G2) — an ACTIVE canonical node returned by `read.get` /
    `read.get_many`.

    `logical_id` is the queried stable identity (echoed). `write_cursor` is the
    interim id carrier (the same column `SearchHit.id` carries). Only active rows
    (`superseded_at IS NULL`) are ever materialised into this shape. Mirrors the
    TypeScript `NodeRecord` (cross-binding parity).
    """

    logical_id: str
    kind: str
    body: str
    write_cursor: int


@dataclass(frozen=True)
class OpStoreRow:
    """Slice 30 (G3) — one `operational_mutations` row returned by
    `read.collection` / `read.mutations`.

    `id` is the autoincrement PK and the after-id cursor key. `payload` is the
    stored `payload_json`. Mirrors the TypeScript `OpStoreRow` (cross-binding
    parity).
    """

    id: int
    collection: str
    record_key: str
    op_kind: str
    payload: str
    schema_id: str | None
    write_cursor: int


@dataclass(frozen=True)
class SearchFilter:
    """G10 — closed metadata filter for `engine.search(query, filter=...)`.

    All fields optional; an all-`None` filter (or no filter) is the unfiltered
    path. A **closed struct**, not an open filter DSL. `created_after` is a
    `created_at >= bound` lower bound in unix seconds. `status` filters the vec0
    `status` metadata column, which ships an empty-string sentinel only (no real
    population source yet — vec0 TEXT metadata is not NULL-able), so a
    `status="open"`-style filter prunes every row until a population slice lands.
    Mirrors the TypeScript `SearchFilter` (cross-binding parity).
    """

    source_type: str | None = None
    kind: str | None = None
    created_after: int | None = None
    status: str | None = None


@dataclass(frozen=True)
class SearchResult:
    """Result returned by `engine.search`."""

    projection_cursor: int
    soft_fallback: SoftFallback | None = None
    results: list[SearchHit] = field(default_factory=list)


@dataclass(frozen=True)
class MigrationStepReport:
    """One row in `OpenReport.migration_steps`.

    Mirrors the native `fathomdb_schema::MigrationStepReport` per
    `src/rust/crates/fathomdb-engine/src/lib.rs:541-548`.
    """

    step_id: int
    duration_ms: int | None
    failed: bool


class DefaultEmbedderDownloadEvent(TypedDict):
    """`embedder_events` entry emitted when the loader downloads a weight
    file from HuggingFace. Per `dev/design/0.7.1-EU-6-FIX-2-design.md`
    §2.1. Mirrors the Rust emitter at
    `src/rust/crates/fathomdb-py/src/lib.rs:417-432`."""

    kind: Literal["DefaultEmbedderDownload"]
    file: str
    url: str
    bytes: int
    sha256: str
    cache_path: str
    duration_ms: int


class DefaultEmbedderCacheHitEvent(TypedDict):
    """`embedder_events` entry emitted on a cache hit for a weight file.
    Per `dev/design/0.7.1-EU-6-FIX-2-design.md` §2.1."""

    kind: Literal["DefaultEmbedderCacheHit"]
    file: str
    sha256: str
    cache_path: str


class MeanVecPinnedEvent(TypedDict):
    """`embedder_events` entry emitted after the 256-doc threshold pins
    the workspace mean vector. Per
    `dev/design/0.7.1-EU-6-FIX-2-design.md` §2.1."""

    kind: Literal["MeanVecPinned"]
    dim: int
    doc_count: int


class UnknownEmbedderEvent(TypedDict):
    """Forward-compat fallback. Any `kind` not recognised by this build
    surfaces at runtime under this shape. Part of the public
    `EmbedderEvent` union for soundness (a future or replaced native
    extension may emit kinds this build does not know about). Because
    its `kind` field is the open type ``str``, pyright cannot exclude
    this member purely from a literal ``event["kind"] == "..."`` check
    — wrap such checks in :func:`is_known_embedder_event` first to
    recover precise narrowing on the three known variants.

    ``kind`` is **required** (the TypedDict is total): every event the
    native extension emits carries a ``kind`` discriminant, so accessing
    ``event["kind"]`` on the bare union is sound. Without totality pyright
    flags the access under ``reportTypedDictNotRequiredAccess``."""

    kind: str


EmbedderEvent = Union[
    DefaultEmbedderDownloadEvent,
    DefaultEmbedderCacheHitEvent,
    MeanVecPinnedEvent,
    UnknownEmbedderEvent,
]
"""Discriminated union surfaced by `OpenReport.embedder_events`. Includes
`UnknownEmbedderEvent` for forward-compat soundness. For precise literal
narrowing on the three known variants, gate the `if event["kind"] == "..."`
chain on :func:`is_known_embedder_event` first."""


def is_known_embedder_event(
    event: EmbedderEvent,
) -> TypeGuard[
    Union[
        DefaultEmbedderDownloadEvent,
        DefaultEmbedderCacheHitEvent,
        MeanVecPinnedEvent,
    ]
]:
    """Narrow an :data:`EmbedderEvent` to the three known variants.

    Used as a guard before discriminating on ``event["kind"]``. Pyright
    cannot exclude :class:`UnknownEmbedderEvent` (whose ``kind`` is the
    open type ``str``) from a literal ``kind == "..."`` check on the
    bare union — so the two-step pattern is::

        if is_known_embedder_event(event):
            if event["kind"] == "DefaultEmbedderDownload":
                bytes_: int = event["bytes"]  # narrowed precisely

    See ``dev/interfaces/python.md`` and
    ``dev/design/0.7.1-EU-6-FIX-2-design.md`` §6.3.
    """
    return event["kind"] in (
        "DefaultEmbedderDownload",
        "DefaultEmbedderCacheHit",
        "MeanVecPinned",
    )


@dataclass(frozen=True)
class EmbedderIdentity:
    """Embedder identity payload carried on `OpenReport.default_embedder`.

    Mirrors `fathomdb_embedder_api::EmbedderIdentity`.
    """

    name: str
    revision: str
    dimension: int


@dataclass(frozen=True)
class OpenReport:
    """Structured open-time report owned by `dev/design/engine.md`.

    Captured at `Engine.open` time and surfaced via the engine-attached
    accessor `engine.open_report()` (Shape D, locked HITL 2026-05-24).
    The accessor is idempotent — the report is a snapshot, not live state.

    EU-5a1/5a2/5b added four embedder-related fields, surfaced by EU-6:

    - ``embedder_download_ms``: wall-time milliseconds the EU-3 loader
      spent fetching default-embedder weights, or ``None`` on full cache
      hit / caller-supplied embedder.
    - ``embedder_events``: list of structured loader event ``dict``s.
      Each carries a ``"kind"`` discriminant (``"DefaultEmbedderDownload"``,
      ``"DefaultEmbedderCacheHit"``, ``"MeanVecPinned"``) and a
      variant-specific payload in snake_case.
    - ``embedder_mean_centering_required``: static identity capability —
      ``True`` for the bge-small default identity, ``False`` otherwise.
    - ``embedder_mean_vec_pinned``: dynamic workspace state — ``True``
      iff ``_fathomdb_embedder_profiles.mean_vec IS NOT NULL`` after the
      256-doc threshold crossing.
    """

    schema_version_before: int
    schema_version_after: int
    migration_steps: list[MigrationStepReport]
    embedder_warmup_ms: int
    query_backend: str
    default_embedder: EmbedderIdentity
    embedder_download_ms: int | None = None
    embedder_events: list[EmbedderEvent] = field(default_factory=list)
    embedder_mean_centering_required: bool = False
    embedder_mean_vec_pinned: bool = False


@dataclass(frozen=True)
class ExpandedNode:
    """Slice 20 (G6) — one node reached by BFS traversal in `search_expand`.

    Carries the reachable `NodeRecord` and the hop distance from the nearest
    search-hit root.  Only nodes NOT already in the search-hit set appear here
    (deduplication: search-score takes priority).
    """

    node: NodeRecord
    hop_count: int


@dataclass(frozen=True)
class SearchExpandResult:
    """Slice 20 (G6) — result of `graph.search_expand`.

    `search_hits` — original RRF-scored results (same shape as `engine.search`).
    `expanded`    — nodes reachable from any search hit within `depth` hops
                    that are NOT in `search_hits`.
    `all_logical_ids` — deduplicated union of both sets (search hit `logical_id`s
                        resolved via `write_cursor` look-up + expanded `logical_id`s).
    """

    search_hits: list[SearchHit]
    expanded: list[ExpandedNode]
    all_logical_ids: list[str]


@dataclass(frozen=True)
class CounterSnapshot:
    """Snapshot of engine-internal counters returned by `engine.counters`.

    Field set mirrors the napi-rs `CounterSnapshot` in idiomatic snake_case
    per `dev/interfaces/python.md` § Engine-attached instrumentation and the
    cross-binding data-shape parity claim in `dev/design/bindings.md` § 1.
    """

    queries: int = 0
    writes: int = 0
    write_rows: int = 0
    admin_ops: int = 0
    cache_hit: int = 0
    cache_miss: int = 0


__all__ = [
    "CounterSnapshot",
    "DefaultEmbedderCacheHitEvent",
    "DefaultEmbedderDownloadEvent",
    "EmbedderEvent",
    "EmbedderIdentity",
    "ExpandedNode",
    "MeanVecPinnedEvent",
    "MigrationStepReport",
    "NodeRecord",
    "OpStoreRow",
    "OpenReport",
    "SearchExpandResult",
    "SearchFilter",
    "SearchHit",
    "SearchResult",
    "SoftFallback",
    "SoftFallbackBranch",
    "UnknownEmbedderEvent",
    "WriteReceipt",
    "is_known_embedder_event",
]
