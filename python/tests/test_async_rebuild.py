from __future__ import annotations

import time
from pathlib import Path

import pytest


def test_register_async_returns_fast(tmp_path: Path) -> None:
    from fathomdb import Engine, FtsPropertySchemaRecord

    db = Engine.open(tmp_path / "test.db")
    start = time.monotonic()
    record = db.admin.register_fts_property_schema_async("Kind", ["$.name"])
    elapsed = time.monotonic() - start

    assert isinstance(record, FtsPropertySchemaRecord)
    assert record.kind == "Kind"
    assert elapsed < 2.0


def test_get_rebuild_progress_returns_state(tmp_path: Path) -> None:
    from fathomdb import Engine, RebuildProgress

    db = Engine.open(tmp_path / "test.db")
    db.admin.register_fts_property_schema_async("Thing", ["$.title"])

    progress = db.admin.get_rebuild_progress("Thing")
    assert progress is not None
    assert isinstance(progress, RebuildProgress)
    assert progress.state in ("BUILDING", "COMPLETE", "FAILED")


def test_rebuild_eventually_completes(tmp_path: Path) -> None:
    from fathomdb import Engine

    db = Engine.open(tmp_path / "test.db")
    db.admin.register_fts_property_schema_async("Item", ["$.body"])

    deadline = time.monotonic() + 10.0
    while time.monotonic() < deadline:
        progress = db.admin.get_rebuild_progress("Item")
        if progress is not None and progress.state == "COMPLETE":
            break
        time.sleep(0.1)
    else:
        pytest.fail("Rebuild did not reach COMPLETE within 10 seconds")

    assert progress is not None
    assert progress.state == "COMPLETE"
