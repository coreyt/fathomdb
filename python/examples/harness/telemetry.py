from __future__ import annotations

from collections.abc import Callable
from typing import Any

from fathomdb import AdminClient, Engine, FeedbackConfig, Query, ResponseCycleEvent


class TelemetryQuery:
    def __init__(
        self,
        query: Query,
        *,
        progress_callback: Callable[[ResponseCycleEvent], None] | None,
        feedback_config: FeedbackConfig | None,
    ) -> None:
        self._query = query
        self._progress_callback = progress_callback
        self._feedback_config = feedback_config

    def __getattr__(self, name: str) -> Any:
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
        return self._query.compile(
            progress_callback=self._progress_callback,
            feedback_config=self._feedback_config,
        )

    def explain(self) -> Any:
        return self._query.explain(
            progress_callback=self._progress_callback,
            feedback_config=self._feedback_config,
        )

    def execute(self) -> Any:
        return self._query.execute(
            progress_callback=self._progress_callback,
            feedback_config=self._feedback_config,
        )


class TelemetryAdminClient:
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

    def check_integrity(self) -> Any:
        return self._admin.check_integrity(
            progress_callback=self._progress_callback,
            feedback_config=self._feedback_config,
        )

    def check_semantics(self) -> Any:
        return self._admin.check_semantics(
            progress_callback=self._progress_callback,
            feedback_config=self._feedback_config,
        )

    def rebuild(self, target: Any = "all") -> Any:
        return self._admin.rebuild(
            target=target,
            progress_callback=self._progress_callback,
            feedback_config=self._feedback_config,
        )

    def rebuild_missing(self) -> Any:
        return self._admin.rebuild_missing(
            progress_callback=self._progress_callback,
            feedback_config=self._feedback_config,
        )

    def trace_source(self, source_ref: str) -> Any:
        return self._admin.trace_source(
            source_ref,
            progress_callback=self._progress_callback,
            feedback_config=self._feedback_config,
        )

    def excise_source(self, source_ref: str) -> Any:
        return self._admin.excise_source(
            source_ref,
            progress_callback=self._progress_callback,
            feedback_config=self._feedback_config,
        )

    def safe_export(self, destination_path: str, *, force_checkpoint: bool = True) -> Any:
        return self._admin.safe_export(
            destination_path,
            force_checkpoint=force_checkpoint,
            progress_callback=self._progress_callback,
            feedback_config=self._feedback_config,
        )


class TelemetryEngine:
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
        return getattr(self._engine, name)

    def nodes(self, kind: str) -> TelemetryQuery:
        return TelemetryQuery(
            self._engine.nodes(kind),
            progress_callback=self._progress_callback,
            feedback_config=self._feedback_config,
        )

    def query(self, kind: str) -> TelemetryQuery:
        return self.nodes(kind)

    def write(self, request: Any) -> Any:
        return self._engine.write(
            request,
            progress_callback=self._progress_callback,
            feedback_config=self._feedback_config,
        )

    def submit(self, request: Any) -> Any:
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
    return TelemetryEngine(
        engine,
        progress_callback=progress_callback,
        feedback_config=feedback_config,
    )
