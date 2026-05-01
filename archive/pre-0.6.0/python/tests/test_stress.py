"""Stress tests for sustained concurrent Python engine workloads."""

from __future__ import annotations

import os
import threading
import time
from collections import defaultdict
from pathlib import Path

from fathomdb import (
    ChunkInsert,
    ChunkPolicy,
    Engine,
    FeedbackConfig,
    NodeInsert,
    ResponseCyclePhase,
    WriteRequest,
    new_row_id,
)


def _make_write(label: str) -> WriteRequest:
    return _make_write_with_content(label)


def _make_write_with_content(
    label: str,
    content_ref: str | None = None,
    content_hash: str | None = None,
) -> WriteRequest:
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
                content_ref=content_ref,
            )
        ],
        chunks=[
            ChunkInsert(
                id=f"chunk:{logical_id}:0",
                node_logical_id=logical_id,
                text_content=f"stress test content for {label}",
                content_hash=content_hash,
            )
        ],
    )


def _stress_duration_seconds() -> float:
    return float(os.environ.get("FATHOM_PY_STRESS_DURATION_SECONDS", "5"))


def emit_success_summary(name: str, **metrics: object) -> None:
    rendered = ", ".join(f"{key}={value}" for key, value in metrics.items())
    print(f"{name}: {rendered}")


def spawn_telemetry_sampler(
    engine: Engine,
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


def assert_monotonic_snapshots(snapshots: list[object]) -> None:
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


def record_feedback_event(
    events_by_operation: dict[str, list[object]],
    event_lock: threading.Lock,
    event: object,
) -> None:
    with event_lock:
        events_by_operation[event.operation_id].append(event)


def assert_feedback_lifecycle(events_by_operation: dict[str, list[object]]) -> None:
    assert events_by_operation, "expected feedback events"
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

        started_seen = False
        for event in events:
            if event.phase is ResponseCyclePhase.STARTED:
                started_seen = True
            if event.phase is ResponseCyclePhase.HEARTBEAT:
                assert started_seen, operation_id
    assert completed_operations > 0, "expected completed feedback operations"
    assert suppressed_operations <= 1, "expected at most one callback-suppressed operation"


def test_sustained_concurrent_reads_under_write_load(tmp_path: Path) -> None:
    duration_seconds = _stress_duration_seconds()
    engine = Engine.open(tmp_path / "stress.db")
    engine.write(_make_write("seed-0"))

    errors: list[str] = []
    stop = threading.Event()
    counts_lock = threading.Lock()
    error_lock = threading.Lock()
    write_count = 0
    read_count = 0

    def writer(thread_id: int) -> None:
        nonlocal write_count
        iteration = 0
        while not stop.is_set():
            try:
                engine.write(_make_write(f"w{thread_id}-{iteration}"))
                with counts_lock:
                    write_count += 1
                iteration += 1
            except Exception as exc:  # pragma: no cover - assertion carries the details
                with error_lock:
                    errors.append(f"writer[{thread_id}]: {exc!r}")
                stop.set()

    def reader(thread_id: int) -> None:
        nonlocal read_count
        while not stop.is_set():
            try:
                rows = engine.nodes("Document").limit(10).execute()
                assert rows.was_degraded is False
                with counts_lock:
                    read_count += 1
            except Exception as exc:  # pragma: no cover - assertion carries the details
                with error_lock:
                    errors.append(f"reader[{thread_id}]: {exc!r}")
                stop.set()

    writers = [threading.Thread(target=writer, args=(i,)) for i in range(5)]
    readers = [threading.Thread(target=reader, args=(i,)) for i in range(20)]

    for thread in writers + readers:
        thread.start()

    time.sleep(duration_seconds)
    stop.set()

    for thread in writers + readers:
        thread.join(timeout=15)
        assert not thread.is_alive(), f"thread {thread.name} hung"

    assert errors == [], f"errors during stress test: {errors}"
    assert write_count > 0, "no writes completed"
    assert read_count > 0, "no reads completed"

    report = engine.admin.check_integrity()
    assert report.physical_ok is True
    assert report.foreign_keys_ok is True
    assert report.missing_fts_rows == 0
    assert report.duplicate_active_logical_ids == 0

    emit_success_summary(
        "python_stress_reads_under_write_load",
        duration_seconds=duration_seconds,
        writes=write_count,
        reads=read_count,
    )

    engine.close()


def test_telemetry_snapshot_is_monotonic_under_load(tmp_path: Path) -> None:
    duration_seconds = _stress_duration_seconds()
    engine = Engine.open(tmp_path / "telemetry-stress.db", telemetry_level="counters")
    engine.write(_make_write("seed-0"))

    errors: list[str] = []
    stop = threading.Event()
    counts_lock = threading.Lock()
    error_lock = threading.Lock()
    snapshots: list[object] = []
    write_count = 0
    read_count = 0

    def writer(thread_id: int) -> None:
        nonlocal write_count
        iteration = 0
        while not stop.is_set():
            try:
                engine.write(_make_write(f"telemetry-w{thread_id}-{iteration}"))
                with counts_lock:
                    write_count += 1
                iteration += 1
            except Exception as exc:  # pragma: no cover - assertion carries the details
                with error_lock:
                    errors.append(f"writer[{thread_id}]: {exc!r}")
                stop.set()

    def reader(thread_id: int) -> None:
        nonlocal read_count
        while not stop.is_set():
            try:
                rows = engine.nodes("Document").limit(10).execute()
                assert rows.was_degraded is False
                with counts_lock:
                    read_count += 1
            except Exception as exc:  # pragma: no cover - assertion carries the details
                with error_lock:
                    errors.append(f"reader[{thread_id}]: {exc!r}")
                stop.set()

    writers = [threading.Thread(target=writer, args=(i,)) for i in range(5)]
    readers = [threading.Thread(target=reader, args=(i,)) for i in range(20)]
    sampler = spawn_telemetry_sampler(engine, stop, snapshots, errors, error_lock)

    for thread in writers + readers:
        thread.start()

    time.sleep(duration_seconds)
    stop.set()

    for thread in writers + readers:
        thread.join(timeout=15)
        assert not thread.is_alive(), f"thread {thread.name} hung"
    sampler.join(timeout=15)
    assert not sampler.is_alive(), "telemetry sampler hung"

    assert errors == [], f"errors during telemetry stress test: {errors}"
    assert write_count > 0, "no writes completed"
    assert read_count > 0, "no reads completed"
    assert len(snapshots) >= 2, "expected multiple telemetry samples"

    assert_monotonic_snapshots(snapshots)
    last = snapshots[-1]
    assert last.queries_total > 0
    assert last.writes_total > 0
    assert last.write_rows_total >= last.writes_total
    assert last.errors_total == 0
    assert last.cache_hits + last.cache_misses > 0

    report = engine.admin.check_integrity()
    assert report.physical_ok is True
    assert report.foreign_keys_ok is True

    emit_success_summary(
        "python_stress_telemetry",
        duration_seconds=duration_seconds,
        writes=write_count,
        reads=read_count,
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


def test_observability_feedback_remains_live_under_load(tmp_path: Path) -> None:
    duration_seconds = _stress_duration_seconds()
    engine = Engine.open(tmp_path / "feedback-stress.db")
    engine.write(_make_write("seed-0"))

    errors: list[str] = []
    stop = threading.Event()
    event_lock = threading.Lock()
    error_lock = threading.Lock()
    callback_state = {"throw_once": False}
    events_by_operation: dict[str, list[object]] = defaultdict(list)

    def callback(event: object) -> None:
        record_feedback_event(events_by_operation, event_lock, event)
        if not callback_state["throw_once"] and event.phase is ResponseCyclePhase.STARTED:
            callback_state["throw_once"] = True
            raise RuntimeError("intentional callback failure")

    feedback = FeedbackConfig(slow_threshold_ms=1, heartbeat_interval_ms=1)

    def writer(thread_id: int) -> None:
        iteration = 0
        while not stop.is_set():
            try:
                engine.write(
                    _make_write(f"feedback-w{thread_id}-{iteration}"),
                    progress_callback=callback,
                    feedback_config=feedback,
                )
                iteration += 1
            except Exception as exc:  # pragma: no cover - assertion carries the details
                with error_lock:
                    errors.append(f"writer[{thread_id}]: {exc!r}")
                stop.set()

    def reader(thread_id: int) -> None:
        while not stop.is_set():
            try:
                rows = engine.nodes("Document").limit(10).execute(
                    progress_callback=callback,
                    feedback_config=feedback,
                )
                assert rows.was_degraded is False
            except Exception as exc:  # pragma: no cover - assertion carries the details
                with error_lock:
                    errors.append(f"reader[{thread_id}]: {exc!r}")
                stop.set()

    def admin_worker() -> None:
        while not stop.is_set():
            try:
                engine.admin.check_integrity(
                    progress_callback=callback,
                    feedback_config=feedback,
                )
                engine.admin.trace_source(
                    "source:seed-0",
                    progress_callback=callback,
                    feedback_config=feedback,
                )
            except Exception as exc:  # pragma: no cover - assertion carries the details
                with error_lock:
                    errors.append(f"admin: {exc!r}")
                stop.set()

    writers = [threading.Thread(target=writer, args=(i,)) for i in range(3)]
    readers = [threading.Thread(target=reader, args=(i,)) for i in range(6)]
    admin_thread = threading.Thread(target=admin_worker)

    for thread in writers + readers + [admin_thread]:
        thread.start()

    time.sleep(duration_seconds)
    stop.set()

    for thread in writers + readers + [admin_thread]:
        thread.join(timeout=15)
        assert not thread.is_alive(), f"thread {thread.name} hung"

    assert errors == [], f"errors during feedback stress test: {errors}"
    assert callback_state["throw_once"] is True
    assert_feedback_lifecycle(events_by_operation)
    all_phases = {
        event.phase
        for operation_events in events_by_operation.values()
        for event in operation_events
    }
    assert ResponseCyclePhase.STARTED in all_phases
    assert ResponseCyclePhase.FINISHED in all_phases
    assert (
        ResponseCyclePhase.SLOW in all_phases
        or ResponseCyclePhase.HEARTBEAT in all_phases
    )
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
        "python_stress_feedback",
        duration_seconds=duration_seconds,
        operations=len(events_by_operation),
        completed_operations=completed_operations,
        suppressed_operations=suppressed_operations,
        phases_seen="|".join(sorted(phase.name.lower() for phase in all_phases)),
    )

    engine.close()


def test_concurrent_external_content_writes_and_filtered_reads(tmp_path: Path) -> None:
    """Stress test for external content objects: mixed writes (some with
    ``content_ref`` / ``content_hash``, some without) alongside concurrent
    reads that filter on ``content_ref``.  Exercises the partial index,
    nullable column handling, and new query predicates under sustained
    concurrent load."""
    duration_seconds = _stress_duration_seconds()
    engine = Engine.open(tmp_path / "ext-content-stress.db")

    # Seed a mix of content and non-content nodes.
    for index in range(50):
        content_ref = f"s3://docs/seed-{index}.pdf" if index % 2 == 0 else None
        content_hash = f"sha256:seed{index}" if content_ref else None
        engine.write(_make_write_with_content(f"seed-{index}", content_ref, content_hash))

    errors: list[str] = []
    stop = threading.Event()
    counts_lock = threading.Lock()
    error_lock = threading.Lock()
    content_write_count = 0
    plain_write_count = 0
    filtered_read_count = 0
    unfiltered_read_count = 0

    def content_writer(thread_id: int) -> None:
        nonlocal content_write_count
        iteration = 0
        while not stop.is_set():
            label = f"ext-{thread_id}-{iteration}"
            try:
                engine.write(
                    _make_write_with_content(
                        label,
                        content_ref=f"s3://docs/{label}.pdf",
                        content_hash=f"sha256:{label}",
                    )
                )
                with counts_lock:
                    content_write_count += 1
                iteration += 1
            except Exception as exc:
                with error_lock:
                    errors.append(f"content-writer[{thread_id}]: {exc!r}")
                stop.set()

    def plain_writer(thread_id: int) -> None:
        nonlocal plain_write_count
        iteration = 0
        while not stop.is_set():
            try:
                engine.write(_make_write(f"plain-{thread_id}-{iteration}"))
                with counts_lock:
                    plain_write_count += 1
                iteration += 1
            except Exception as exc:
                with error_lock:
                    errors.append(f"plain-writer[{thread_id}]: {exc!r}")
                stop.set()

    def filtered_reader(thread_id: int) -> None:
        nonlocal filtered_read_count
        while not stop.is_set():
            try:
                rows = (
                    engine.nodes("Document")
                    .filter_content_ref_not_null()
                    .limit(10)
                    .execute()
                )
                for node in rows.nodes:
                    assert node.content_ref is not None, (
                        f"filtered read returned node without content_ref: {node.logical_id}"
                    )
                with counts_lock:
                    filtered_read_count += 1
            except Exception as exc:
                with error_lock:
                    errors.append(f"filtered-reader[{thread_id}]: {exc!r}")
                stop.set()

    def unfiltered_reader(thread_id: int) -> None:
        nonlocal unfiltered_read_count
        while not stop.is_set():
            try:
                engine.nodes("Document").limit(10).execute()
                with counts_lock:
                    unfiltered_read_count += 1
            except Exception as exc:
                with error_lock:
                    errors.append(f"unfiltered-reader[{thread_id}]: {exc!r}")
                stop.set()

    content_writers = [threading.Thread(target=content_writer, args=(i,)) for i in range(3)]
    plain_writers = [threading.Thread(target=plain_writer, args=(i,)) for i in range(2)]
    filtered_readers = [threading.Thread(target=filtered_reader, args=(i,)) for i in range(10)]
    unfiltered_readers = [threading.Thread(target=unfiltered_reader, args=(i,)) for i in range(10)]

    all_threads = content_writers + plain_writers + filtered_readers + unfiltered_readers
    for thread in all_threads:
        thread.start()

    time.sleep(duration_seconds)
    stop.set()

    for thread in all_threads:
        thread.join(timeout=15)
        assert not thread.is_alive(), f"thread {thread.name} hung"

    assert errors == [], f"errors during external content stress test: {errors}"
    assert content_write_count > 0, "no content writes completed"
    assert plain_write_count > 0, "no plain writes completed"
    assert filtered_read_count > 0, "no filtered reads completed"
    assert unfiltered_read_count > 0, "no unfiltered reads completed"

    report = engine.admin.check_integrity()
    assert report.physical_ok is True
    assert report.foreign_keys_ok is True
    assert report.missing_fts_rows == 0
    assert report.duplicate_active_logical_ids == 0

    emit_success_summary(
        "python_stress_external_content",
        duration_seconds=duration_seconds,
        content_writes=content_write_count,
        plain_writes=plain_write_count,
        filtered_reads=filtered_read_count,
        unfiltered_reads=unfiltered_read_count,
    )

    engine.close()
