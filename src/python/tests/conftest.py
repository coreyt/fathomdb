"""Shared pytest fixtures for the FathomDB Python SDK test suite.

Phase 11a wires the PyO3 binding to a real `fathomdb-engine`; each
test now needs a unique database path (the engine acquires an
exclusive file lock, so reusing one path across tests in the same
process would surface as `DatabaseLockedError`). `db_path` yields a
fresh per-test `Path` under pytest's `tmp_path` fixture.
"""

from __future__ import annotations

from pathlib import Path

import pytest


@pytest.fixture
def db_path(tmp_path: Path) -> str:
    return str(tmp_path / "rewrite.sqlite")
