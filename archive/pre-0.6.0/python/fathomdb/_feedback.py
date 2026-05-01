from __future__ import annotations

import threading
import time
import uuid
from collections.abc import Callable, Mapping
from typing import TypeVar

from ._types import FeedbackConfig, ResponseCycleEvent, ResponseCyclePhase

T = TypeVar("T")


def run_with_feedback(
    *,
    surface: str,
    operation_kind: str,
    metadata: Mapping[str, str] | None,
    progress_callback: Callable[[ResponseCycleEvent], None] | None,
    feedback_config: FeedbackConfig | None,
    operation: Callable[[], T],
) -> T:
    if progress_callback is None:
        return operation()

    config = feedback_config or FeedbackConfig()
    metadata_dict = dict(metadata or {})
    operation_id = uuid.uuid4().hex
    started_at = time.monotonic()
    stop_event = threading.Event()
    callback_disabled = False
    callback_lock = threading.Lock()

    def emit(
        phase: ResponseCyclePhase,
        *,
        error_code: str | None = None,
        error_message: str | None = None,
    ) -> None:
        nonlocal callback_disabled
        with callback_lock:
            if callback_disabled:
                return
            event = ResponseCycleEvent(
                operation_id=operation_id,
                operation_kind=operation_kind,
                surface=surface,
                phase=phase,
                elapsed_ms=int((time.monotonic() - started_at) * 1000),
                slow_threshold_ms=config.slow_threshold_ms,
                metadata=dict(metadata_dict),
                error_code=error_code,
                error_message=error_message,
            )
            try:
                progress_callback(event)
            except Exception:
                callback_disabled = True

    def heartbeat_loop() -> None:
        if stop_event.wait(config.slow_threshold_ms / 1000):
            return
        emit(ResponseCyclePhase.SLOW)
        while not stop_event.wait(config.heartbeat_interval_ms / 1000):
            emit(ResponseCyclePhase.HEARTBEAT)

    emit(ResponseCyclePhase.STARTED)
    timer_thread = threading.Thread(target=heartbeat_loop, daemon=True)
    timer_thread.start()

    try:
        result = operation()
    except Exception as error:
        stop_event.set()
        timer_thread.join()
        emit(
            ResponseCyclePhase.FAILED,
            error_code=type(error).__name__,
            error_message=str(error),
        )
        raise

    stop_event.set()
    timer_thread.join()
    emit(ResponseCyclePhase.FINISHED)
    return result
