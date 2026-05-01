from __future__ import annotations

import os
import threading
import time
from collections import defaultdict
from pathlib import Path

from fathomdb import (
    ChunkInsert,
    ChunkPolicy,
    FeedbackConfig,
    NodeInsert,
    ResponseCyclePhase,
    WriteRequest,
    new_row_id,
)


def _stress_duration_seconds() -> float:
    return float(os.environ.get("FATHOM_PY_STRESS_DURATION_SECONDS", "5"))


def emit_success_summary(name: str, **metrics: object) -> None:
    rendered = ", ".join(f"{key}={value}" for key, value in metrics.items())
    print(f"{name}: {rendered}")


def make_harness_write(label: str) -> WriteRequest:
    logical_id = f"doc:{label}"
    return WriteRequest(
        label=label,
        nodes=[
            NodeInsert(
                row_id=new_row_id(),
                logical_id=logical_id,
                kind="Document",
                properties={"title": label},
                source_ref=f"source:{label}",
                upsert=True,
                chunk_policy=ChunkPolicy.REPLACE,
            )
        ],
        chunks=[
            ChunkInsert(
                id=f"chunk:{logical_id}:0",
                node_logical_id=logical_id,
                text_content=f"harness stress content for {label}",
            )
        ],
    )


def spawn_harness_telemetry_sampler(
    engine: object,
    stop: threading.Event,
    snapshots: list[object],
    errors: list[str],
    error_lock: threading.Lock,
) -> threading.Thread:
    def sampler() -> None:
        while not stop.is_set():
            snapshots.append(engine.telemetry_snapshot())
            time.sleep(0.025)
        final_snapshot = engine.telemetry_snapshot()
        if final_snapshot.errors_total > 0:
            with error_lock:
                errors.append(f"telemetry errors_total was {final_snapshot.errors_total}")
        snapshots.append(final_snapshot)

    thread = threading.Thread(target=sampler)
    thread.start()
    return thread


def assert_harness_monotonic_snapshots(snapshots: list[object]) -> None:
    for first, second in zip(snapshots, snapshots[1:]):
        assert second.queries_total >= first.queries_total
        assert second.writes_total >= first.writes_total
        assert second.write_rows_total >= first.write_rows_total
        assert second.errors_total >= first.errors_total
        assert second.admin_ops_total >= first.admin_ops_total
        assert second.cache_hits >= 0
        assert second.cache_misses >= 0
        assert second.cache_writes >= 0
        assert second.cache_spills >= 0


def record_harness_feedback_event(
    events_by_operation: dict[str, list[object]],
    event_lock: threading.Lock,
    event: object,
) -> None:
    with event_lock:
        events_by_operation[event.operation_id].append(event)


def assert_harness_feedback_lifecycle(events_by_operation: dict[str, list[object]]) -> None:
    assert events_by_operation, "expected harness feedback events"
    completed_operations = 0
    suppressed_operations = 0
    for operation_id, events in events_by_operation.items():
        phases = [event.phase for event in events]
        assert phases[0] is ResponseCyclePhase.STARTED, operation_id
        if phases[-1] in {ResponseCyclePhase.FINISHED, ResponseCyclePhase.FAILED}:
            completed_operations += 1
        else:
            assert phases == [ResponseCyclePhase.STARTED], operation_id
            suppressed_operations += 1
        elapsed_ms = [event.elapsed_ms for event in events]
        assert elapsed_ms == sorted(elapsed_ms), operation_id
    assert completed_operations > 0
    assert suppressed_operations <= 1


def test_harness_telemetry_snapshot_is_monotonic_under_load(tmp_path: Path) -> None:
    from examples.harness.engine_factory import open_engine

    duration_seconds = _stress_duration_seconds()
    engine = open_engine(tmp_path / "harness-telemetry.db", mode="baseline")
    errors: list[str] = []
    stop = threading.Event()
    error_lock = threading.Lock()
    snapshots: list[object] = []

    def writer(thread_id: int) -> None:
        iteration = 0
        while not stop.is_set():
            try:
                engine.write(make_harness_write(f"harness-w{thread_id}-{iteration}"))
                iteration += 1
            except Exception as exc:  # pragma: no cover
                with error_lock:
                    errors.append(f"writer[{thread_id}]: {exc!r}")
                stop.set()

    def reader(thread_id: int) -> None:
        while not stop.is_set():
            try:
                rows = engine.nodes("Document").limit(10).execute()
                assert rows.was_degraded is False
            except Exception as exc:  # pragma: no cover
                with error_lock:
                    errors.append(f"reader[{thread_id}]: {exc!r}")
                stop.set()

    writers = [threading.Thread(target=writer, args=(i,)) for i in range(3)]
    readers = [threading.Thread(target=reader, args=(i,)) for i in range(6)]
    sampler = spawn_harness_telemetry_sampler(engine, stop, snapshots, errors, error_lock)

    for thread in writers + readers:
        thread.start()

    time.sleep(duration_seconds)
    stop.set()

    for thread in writers + readers:
        thread.join(timeout=15)
        assert not thread.is_alive(), f"thread {thread.name} hung"
    sampler.join(timeout=15)
    assert not sampler.is_alive(), "harness telemetry sampler hung"

    assert errors == [], f"errors during harness telemetry stress test: {errors}"
    assert len(snapshots) >= 2
    assert_harness_monotonic_snapshots(snapshots)

    last = snapshots[-1]
    assert last.queries_total > 0
    assert last.writes_total > 0
    assert last.errors_total == 0

    report = engine.admin.check_integrity()
    assert report.physical_ok is True
    assert report.foreign_keys_ok is True

    emit_success_summary(
        "python_harness_stress_telemetry",
        duration_seconds=duration_seconds,
        telemetry_samples=len(snapshots),
        queries_total=last.queries_total,
        writes_total=last.writes_total,
        write_rows_total=last.write_rows_total,
        errors_total=last.errors_total,
        admin_ops_total=last.admin_ops_total,
        cache_hits=last.cache_hits,
        cache_misses=last.cache_misses,
        cache_writes=last.cache_writes,
        cache_spills=last.cache_spills,
    )
    engine.close()


def test_harness_feedback_remains_live_under_load(tmp_path: Path) -> None:
    from examples.harness.engine_factory import open_engine

    duration_seconds = _stress_duration_seconds()
    events_by_operation: dict[str, list[object]] = defaultdict(list)
    event_lock = threading.Lock()
    callback_state = {"throw_once": False}

    def callback(event: object) -> None:
        record_harness_feedback_event(events_by_operation, event_lock, event)
        if not callback_state["throw_once"] and event.phase is ResponseCyclePhase.STARTED:
            callback_state["throw_once"] = True
            raise RuntimeError("intentional harness callback failure")

    engine = open_engine(
        tmp_path / "harness-feedback.db",
        mode="baseline",
        progress_callback=callback,
        feedback_config=FeedbackConfig(slow_threshold_ms=1, heartbeat_interval_ms=1),
    )

    errors: list[str] = []
    stop = threading.Event()
    error_lock = threading.Lock()

    def writer(thread_id: int) -> None:
        iteration = 0
        while not stop.is_set():
            try:
                engine.write(make_harness_write(f"harness-feedback-w{thread_id}-{iteration}"))
                iteration += 1
            except Exception as exc:  # pragma: no cover
                with error_lock:
                    errors.append(f"writer[{thread_id}]: {exc!r}")
                stop.set()

    def reader(thread_id: int) -> None:
        while not stop.is_set():
            try:
                rows = engine.nodes("Document").limit(10).execute()
                assert rows.was_degraded is False
            except Exception as exc:  # pragma: no cover
                with error_lock:
                    errors.append(f"reader[{thread_id}]: {exc!r}")
                stop.set()

    writers = [threading.Thread(target=writer, args=(i,)) for i in range(2)]
    readers = [threading.Thread(target=reader, args=(i,)) for i in range(4)]

    for thread in writers + readers:
        thread.start()

    time.sleep(duration_seconds)
    stop.set()

    for thread in writers + readers:
        thread.join(timeout=15)
        assert not thread.is_alive(), f"thread {thread.name} hung"

    assert errors == [], f"errors during harness feedback stress test: {errors}"
    assert callback_state["throw_once"] is True
    assert_harness_feedback_lifecycle(events_by_operation)

    phases = {
        event.phase
        for operation_events in events_by_operation.values()
        for event in operation_events
    }
    assert ResponseCyclePhase.STARTED in phases
    assert ResponseCyclePhase.FINISHED in phases
    assert ResponseCyclePhase.SLOW in phases or ResponseCyclePhase.HEARTBEAT in phases
    completed_operations = sum(
        1
        for operation_events in events_by_operation.values()
        if operation_events[-1].phase
        in {ResponseCyclePhase.FINISHED, ResponseCyclePhase.FAILED}
    )
    suppressed_operations = sum(
        1
        for operation_events in events_by_operation.values()
        if operation_events[-1].phase
        not in {ResponseCyclePhase.FINISHED, ResponseCyclePhase.FAILED}
    )

    report = engine.admin.check_integrity()
    assert report.physical_ok is True
    assert report.foreign_keys_ok is True

    emit_success_summary(
        "python_harness_stress_feedback",
        duration_seconds=duration_seconds,
        operations=len(events_by_operation),
        completed_operations=completed_operations,
        suppressed_operations=suppressed_operations,
        phases_seen="|".join(sorted(phase.name.lower() for phase in phases)),
    )
    engine.close()
