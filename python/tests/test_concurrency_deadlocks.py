"""Concurrency and GIL deadlock regression tests for fathomdb Python bindings.

These tests verify that pyo3-log GIL interactions do not deadlock under
concurrent engine usage. Each test has a timeout to catch hangs.
"""

from __future__ import annotations

import logging
import threading
from pathlib import Path

import pytest

from fathomdb import (
    ChunkInsert,
    ChunkPolicy,
    Engine,
    NodeInsert,
    ProvenanceMode,
    WriteRequest,
    new_row_id,
)


@pytest.fixture()
def _debug_logging():
    """Enable DEBUG logging for the duration of a test."""
    root = logging.getLogger()
    original_level = root.level
    root.setLevel(logging.DEBUG)
    handler = logging.StreamHandler()
    handler.setLevel(logging.DEBUG)
    root.addHandler(handler)
    yield
    root.removeHandler(handler)
    root.setLevel(original_level)


def _make_write(label: str = "test") -> WriteRequest:
    """Build a minimal write request with one node and one chunk."""
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
                text_content=f"content for {label}",
            )
        ],
    )


@pytest.mark.timeout(15)
@pytest.mark.usefixtures("_debug_logging")
def test_two_engines_with_debug_logging_no_deadlock(tmp_path: Path) -> None:
    """Regression: opening two engines with DEBUG logging caused GIL deadlock.

    The deadlock occurred because Engine.open() held the GIL while schema
    bootstrap emitted tracing events through pyo3-log, which tried to
    acquire the GIL from the writer thread.  Fixed in d09deb4.
    """
    db1 = tmp_path / "engine1.db"
    db2 = tmp_path / "engine2.db"

    engine1 = Engine.open(db1)
    engine2 = Engine.open(db2)
    engine2.close()
    engine1.close()


@pytest.mark.timeout(15)
@pytest.mark.usefixtures("_debug_logging")
def test_concurrent_writes_with_debug_logging(tmp_path: Path) -> None:
    """Verify concurrent writes from threads don't deadlock with pyo3-log."""
    db_path = tmp_path / "concurrent.db"
    engine = Engine.open(db_path)

    errors: list[Exception] = []

    def writer(thread_id: int) -> None:
        try:
            for i in range(5):
                engine.write(_make_write(f"t{thread_id}-w{i}"))
        except Exception as e:
            errors.append(e)

    threads = [threading.Thread(target=writer, args=(i,)) for i in range(4)]
    for t in threads:
        t.start()
    for t in threads:
        t.join(timeout=10)
        assert not t.is_alive(), f"thread {t.name} hung"

    assert errors == [], f"write errors: {errors}"
    engine.close()


@pytest.mark.timeout(15)
@pytest.mark.usefixtures("_debug_logging")
def test_concurrent_reads_and_writes_with_logging(tmp_path: Path) -> None:
    """Verify mixed read/write concurrency doesn't deadlock with pyo3-log."""
    db_path = tmp_path / "mixed.db"
    engine = Engine.open(db_path)

    # Seed some data
    engine.write(_make_write("seed"))

    errors: list[Exception] = []
    stop = threading.Event()

    def reader() -> None:
        try:
            while not stop.is_set():
                engine.nodes("Document").execute()
        except Exception as e:
            errors.append(e)

    def writer() -> None:
        try:
            for i in range(10):
                engine.write(_make_write(f"write-{i}"))
        except Exception as e:
            errors.append(e)

    reader_threads = [threading.Thread(target=reader) for _ in range(3)]
    writer_thread = threading.Thread(target=writer)

    for t in reader_threads:
        t.start()
    writer_thread.start()

    writer_thread.join(timeout=10)
    stop.set()
    for t in reader_threads:
        t.join(timeout=5)

    assert not writer_thread.is_alive(), "writer thread hung"
    for t in reader_threads:
        assert not t.is_alive(), f"reader thread {t.name} hung"
    assert errors == [], f"errors: {errors}"
    engine.close()


@pytest.mark.timeout(15)
@pytest.mark.usefixtures("_debug_logging")
def test_close_during_concurrent_reads(tmp_path: Path) -> None:
    """Verify close() doesn't deadlock when readers are active."""
    db_path = tmp_path / "close-race.db"
    engine = Engine.open(db_path)
    engine.write(_make_write("seed"))

    errors: list[Exception] = []
    started = threading.Event()

    def reader() -> None:
        started.set()
        try:
            for _ in range(50):
                engine.nodes("Document").execute()
        except Exception as e:
            # FathomError("engine is closed") is expected
            errors.append(e)

    reader_thread = threading.Thread(target=reader)
    reader_thread.start()
    started.wait(timeout=5)

    engine.close()
    reader_thread.join(timeout=10)
    assert not reader_thread.is_alive(), "reader thread hung after close"


@pytest.mark.timeout(15)
@pytest.mark.usefixtures("_debug_logging")
def test_open_close_cycle_with_logging(tmp_path: Path) -> None:
    """Verify repeated open/close cycles don't leak or deadlock."""
    db_path = tmp_path / "cycle.db"
    for i in range(10):
        engine = Engine.open(db_path)
        engine.write(_make_write(f"cycle-{i}"))
        engine.close()


@pytest.mark.timeout(15)
@pytest.mark.usefixtures("_debug_logging")
def test_admin_ops_with_debug_logging(tmp_path: Path) -> None:
    """Verify admin operations don't deadlock with pyo3-log."""
    db_path = tmp_path / "admin.db"
    engine = Engine.open(db_path)
    engine.write(_make_write("admin-test"))

    engine.admin.check_integrity()
    engine.admin.check_semantics()
    engine.admin.rebuild()
    engine.admin.rebuild_missing()
    engine.admin.trace_source("source:admin-test")

    engine.close()


@pytest.mark.timeout(15)
@pytest.mark.usefixtures("_debug_logging")
def test_context_manager_with_debug_logging(tmp_path: Path) -> None:
    """Verify context manager close doesn't deadlock with pyo3-log."""
    db_path = tmp_path / "ctx.db"
    with Engine.open(db_path) as engine:
        engine.write(_make_write("ctx-test"))
    # Engine closed by context manager — must not deadlock
