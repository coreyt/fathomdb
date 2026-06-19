"""Shared pytest fixtures for the FathomDB Python SDK test suite.

Phase 11a wires the PyO3 binding to a real `fathomdb-engine`; each
test now needs a unique database path (the engine acquires an
exclusive file lock, so reusing one path across tests in the same
process would surface as `DatabaseLockedError`). `db_path` yields a
fresh per-test `Path` under pytest's `tmp_path` fixture.

EU-6 FIX-1: a session-scoped autouse fixture rebuilds the editable
binding with the dev-only `test-hooks` Cargo feature before any test
imports `fathomdb._fathomdb` — `pyproject.toml [tool.maturin] features`
ships only `default-embedder` (parity with the released wheel), so
without this fixture `Engine._write_vector_for_test` and friends are
missing and tests like `test_use_default_embedder.py`,
`test_ffi_safety.py`, the AC-067 force-panic probe, etc. break. The
fixture short-circuits if the test-hooks symbol is already present
(developer pre-warmed the build with `maturin develop --features
...,test-hooks,...`), so the latency cost (~5-60s for the
`maturin develop` invocation) is paid at most once per pytest session.
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

import pytest


@pytest.fixture
def db_path(tmp_path: Path) -> str:
    return str(tmp_path / "rewrite.sqlite")


_PYTHON_SRC_DIR = Path(__file__).resolve().parent.parent  # src/python


def _binding_has_test_hooks() -> bool:
    """Return True if the imported `Engine` already exposes the
    test-hooks surface (`_write_vector_for_test`). Used to skip the
    fixture's `maturin develop` when the developer pre-built locally."""

    try:
        from fathomdb import _fathomdb  # noqa: PLC0415 — import-on-demand
    except ImportError:
        # Binding not installed yet — definitely need to build.
        return False
    engine = getattr(_fathomdb, "Engine", None)
    if engine is None:
        return False
    return hasattr(engine, "_write_vector_for_test")


@pytest.fixture(scope="session", autouse=True)
def _ensure_test_hooks_binding() -> None:
    """Rebuild the editable binding with `test-hooks` enabled before
    the test session runs, unless it is already present.

    Skipped when not invoked from the source tree (e.g. running the
    test files against a pip-installed wheel for release-surface
    verification) — detected by the absence of `pyproject.toml` next
    to the tests directory. Skipped when `FATHOMDB_TESTS_NO_REBUILD=1`
    is set (developer escape hatch: prebuild once, iterate without
    paying the fixture cost)."""

    if os.environ.get("FATHOMDB_TESTS_NO_REBUILD") == "1":
        return
    pyproject = _PYTHON_SRC_DIR / "pyproject.toml"
    if not pyproject.exists():
        # Not an editable-install context (release-surface tests run
        # against a pip-installed wheel under a throwaway venv).
        return
    if _binding_has_test_hooks():
        # Developer already built with test-hooks (or a previous
        # pytest session in the same venv did). Skip the rebuild.
        return

    print(
        "\n[conftest] EU-6 FIX-1: rebuilding fathomdb editable binding "
        "with test-hooks feature (one-time per session; ~5-60s) ...",
        flush=True,
    )
    subprocess.check_call(
        [
            sys.executable,
            "-m",
            "maturin",
            "develop",
            "--features",
            "pyo3/extension-module,test-hooks,default-embedder,default-reranker",
        ],
        cwd=str(_PYTHON_SRC_DIR),
    )
    # Force the re-import on next access — pytest collection may have
    # already imported the old module pre-fixture.
    for mod_name in list(sys.modules):
        if mod_name == "fathomdb" or mod_name.startswith("fathomdb."):
            del sys.modules[mod_name]
