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
class CounterSnapshot:
    """Snapshot of engine-internal counters returned by `engine.counters`.

    Field set is owned by `dev/design/lifecycle.md`; the 0.6.0 stub publishes
    the carrier type so callers can dispatch on it.
    """


__all__ = [
    "CounterSnapshot",
    "SearchResult",
    "SoftFallback",
    "SoftFallbackBranch",
    "WriteReceipt",
]
