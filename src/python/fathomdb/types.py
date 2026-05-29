"""Caller-visible result shapes for the FathomDB Python SDK.

Field names owned by `dev/interfaces/python.md` § Caller-visible data shapes.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Literal, TypedDict, Union

#: Typed soft-fallback branch values per `dev/design/retrieval.md`.
SoftFallbackBranch = Literal["vector", "text"]


@dataclass(frozen=True)
class WriteReceipt:
    """Receipt returned by `engine.write` and `admin.configure`."""

    cursor: int


@dataclass(frozen=True)
class SoftFallback:
    """Hybrid-search soft-fallback signal.

    `branch` indicates which non-essential branch could not contribute. Total
    request failure is not expressed via this carrier (see
    `dev/design/retrieval.md`).
    """

    branch: SoftFallbackBranch


@dataclass(frozen=True)
class SearchResult:
    """Result returned by `engine.search`."""

    projection_cursor: int
    soft_fallback: SoftFallback | None = None
    results: list[str] = field(default_factory=list)


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


class UnknownEmbedderEvent(TypedDict, total=False):
    """Forward-compat fallback. Any `kind` not recognised by this build
    surfaces at runtime under this shape. NOT included in the
    `EmbedderEvent` discriminated union — including an open-`kind: str`
    member would defeat literal narrowing on the known variants. User
    code that does not match a known `kind` in its `if/elif` chain
    can pattern-match on `event["kind"]` further in the final `else`
    branch."""

    kind: str


EmbedderEvent = Union[
    DefaultEmbedderDownloadEvent,
    DefaultEmbedderCacheHitEvent,
    MeanVecPinnedEvent,
]
"""Discriminated union surfaced by `OpenReport.embedder_events`. Pyright
narrows the payload keys inside `if event["kind"] == "..."` branches."""


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
    "MeanVecPinnedEvent",
    "MigrationStepReport",
    "OpenReport",
    "SearchResult",
    "SoftFallback",
    "SoftFallbackBranch",
    "UnknownEmbedderEvent",
    "WriteReceipt",
]
