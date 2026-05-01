"""Telemetry wrappers that inject feedback callbacks into engine operations."""

from __future__ import annotations

from collections.abc import Callable
from typing import Any

from fathomdb import AdminClient, Engine, FeedbackConfig, Query, ResponseCycleEvent


class TelemetryQuery:
    """Wrap a Query to forward feedback config on each terminal operation."""

    def __init__(
        self,
        query: Query,
        *,
        progress_callback: Callable[[ResponseCycleEvent], None] | None,
        feedback_config: FeedbackConfig | None,
    ) -> None:
        """Store the wrapped query and feedback settings."""
        self._query = query
        self._progress_callback = progress_callback
        self._feedback_config = feedback_config

    def __getattr__(self, name: str) -> Any:
        """Proxy attribute access, wrapping returned Query objects with telemetry."""
        attr = getattr(self._query, name)
        if not callable(attr):
            return attr

        def wrapper(*args: Any, **kwargs: Any) -> Any:
            result = attr(*args, **kwargs)
            if isinstance(result, Query):
                return TelemetryQuery(
                    result,
                    progress_callback=self._progress_callback,
                    feedback_config=self._feedback_config,
                )
            return result

        return wrapper

    def compile(self) -> Any:
        """Compile the query plan with feedback."""
        return self._query.compile(
            progress_callback=self._progress_callback,
            feedback_config=self._feedback_config,
        )

    def explain(self) -> Any:
        """Explain the query plan with feedback."""
        return self._query.explain(
            progress_callback=self._progress_callback,
            feedback_config=self._feedback_config,
        )

    def execute(self) -> Any:
        """Execute the query with feedback."""
        return self._query.execute(
            progress_callback=self._progress_callback,
            feedback_config=self._feedback_config,
        )


class TelemetryAdminClient:
    """Wrap an AdminClient to forward feedback config on each admin call."""

    def __init__(
        self,
        admin: AdminClient,
        *,
        progress_callback: Callable[[ResponseCycleEvent], None] | None,
        feedback_config: FeedbackConfig | None,
    ) -> None:
        self._admin = admin
        self._progress_callback = progress_callback
        self._feedback_config = feedback_config

    def __getattr__(self, name: str) -> Any:
        """Proxy attribute access, injecting feedback config into callable methods."""
        attr = getattr(self._admin, name)
        if not callable(attr):
            return attr

        def wrapper(*args: Any, **kwargs: Any) -> Any:
            kwargs.setdefault("progress_callback", self._progress_callback)
            kwargs.setdefault("feedback_config", self._feedback_config)
            return attr(*args, **kwargs)

        return wrapper


class TelemetryEngine:
    """Wrap an Engine to inject feedback config into every operation."""

    def __init__(
        self,
        engine: Engine,
        *,
        progress_callback: Callable[[ResponseCycleEvent], None] | None,
        feedback_config: FeedbackConfig | None,
    ) -> None:
        self._engine = engine
        self._progress_callback = progress_callback
        self._feedback_config = feedback_config
        self.admin = TelemetryAdminClient(
            engine.admin,
            progress_callback=progress_callback,
            feedback_config=feedback_config,
        )

    def __getattr__(self, name: str) -> Any:
        """Forward attribute access to the underlying engine."""
        return getattr(self._engine, name)

    def nodes(self, kind: str) -> TelemetryQuery:
        """Start a node query wrapped with feedback config."""
        return TelemetryQuery(
            self._engine.nodes(kind),
            progress_callback=self._progress_callback,
            feedback_config=self._feedback_config,
        )

    def query(self, kind: str) -> TelemetryQuery:
        """Alias for :meth:`nodes` for backward compatibility."""
        return self.nodes(kind)

    def write(self, request: Any) -> Any:
        """Submit a synchronous write request with feedback."""
        return self._engine.write(
            request,
            progress_callback=self._progress_callback,
            feedback_config=self._feedback_config,
        )

    def submit(self, request: Any) -> Any:
        """Submit an asynchronous write request with feedback."""
        return self._engine.submit(
            request,
            progress_callback=self._progress_callback,
            feedback_config=self._feedback_config,
        )


def wrap_engine(
    engine: Engine,
    *,
    progress_callback: Callable[[ResponseCycleEvent], None] | None,
    feedback_config: FeedbackConfig | None,
) -> TelemetryEngine:
    """Create a TelemetryEngine wrapping the given engine with feedback config."""
    return TelemetryEngine(
        engine,
        progress_callback=progress_callback,
        feedback_config=feedback_config,
    )
