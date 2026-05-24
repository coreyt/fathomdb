"""Caller-visible result shapes for the FathomDB Python SDK.

Field names owned by `dev/interfaces/python.md` § Caller-visible data shapes.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Literal

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
    """

    schema_version_before: int
    schema_version_after: int
    migration_steps: list[MigrationStepReport]
    embedder_warmup_ms: int
    query_backend: str
    default_embedder: EmbedderIdentity


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
    "EmbedderIdentity",
    "MigrationStepReport",
    "OpenReport",
    "SearchResult",
    "SoftFallback",
    "SoftFallbackBranch",
    "WriteReceipt",
]
