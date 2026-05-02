"""Pure-Python engine surface stub.

Pins the five-verb shape and engine-attached instrumentation methods owned by
`dev/interfaces/python.md`. PyO3 wiring lands in a follow-up slice; the 0.6.0
surface-stub keeps every body inert beyond a synthetic cursor counter that
lets parser tests assert call sequencing.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass, field
from typing import Any

from fathomdb.config import EngineConfig
from fathomdb.types import CounterSnapshot, SearchResult, WriteReceipt

_KWARG_FIELDS = {
    "embedder_pool_size",
    "scheduler_runtime_threads",
    "provenance_row_cap",
    "embedder_call_timeout_ms",
    "slow_threshold_ms",
}


@dataclass
class Engine:
    """Pure-Python placeholder for the future PyO3-backed engine handle."""

    path: str
    config: EngineConfig = field(default_factory=EngineConfig)
    _cursor: int = 0
    _closed: bool = False

    @classmethod
    def open(
        cls,
        path: str,
        *,
        config: EngineConfig | None = None,
        **engine_config: Any,
    ) -> "Engine":
        """Open (or scaffold-open) a database at `path`.

        Either `config` or per-knob `**engine_config` keyword arguments may be
        supplied, but not both. Unknown keyword arguments are rejected.
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
        return cls(path=path, config=resolved)

    def write(self, batch: list[Any] | None = None) -> WriteReceipt:
        self._ensure_open()
        self._cursor += max(len(batch or []), 1)
        return WriteReceipt(cursor=self._cursor)

    def search(self, query: str) -> SearchResult:
        self._ensure_open()
        if not query.strip():
            from fathomdb.errors import WriteValidationError

            raise WriteValidationError("query must not be empty")
        return SearchResult(
            projection_cursor=self._cursor,
            soft_fallback=None,
            results=[f"rewrite scaffold query: {query.strip()}"],
        )

    def close(self) -> None:
        self._closed = True

    def drain(self, *, timeout_s: float | int = 0) -> None:
        """Block until in-flight writes drain or `timeout_s` elapses.

        Semantics owned by `dev/design/lifecycle.md`; the 0.6.0 stub returns
        immediately without blocking.
        """

        del timeout_s

    def counters(self) -> CounterSnapshot:
        return CounterSnapshot()

    def set_profiling(self, *, enabled: bool) -> None:
        del enabled

    def set_slow_threshold_ms(self, *, value: int) -> None:
        del value

    def attach_logging_subscriber(
        self,
        logger: logging.Logger,
        *,
        heartbeat_interval_ms: int | None = None,
    ) -> None:
        """Bind engine events into the supplied `logging.Logger`.

        The 0.6.0 stub does not emit any events; the helper exists so callers
        can wire a logger up against the public surface in advance of
        subscriber wiring.
        """

        del logger, heartbeat_interval_ms

    def _record_admin_configure(self, *, name: str, body: str) -> WriteReceipt:
        del name, body
        self._ensure_open()
        self._cursor += 1
        return WriteReceipt(cursor=self._cursor)

    def _ensure_open(self) -> None:
        if self._closed:
            from fathomdb.errors import ClosingError

            raise ClosingError("engine is closed")


__all__ = ["Engine"]
