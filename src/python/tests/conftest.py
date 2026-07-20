"""Shared pytest fixtures for the FathomDB Python SDK test suite.

Phase 11a wires the PyO3 binding to a real `fathomdb-engine`; each
test now needs a unique database path (the engine acquires an
exclusive file lock, so reusing one path across tests in the same
process would surface as `DatabaseLockedError`). `db_path` yields a
fresh per-test `Path` under pytest's `tmp_path` fixture.

EU-6 FIX-1 / 0.8.9.2: rebuild the editable binding with the dev-only
`test-hooks` Cargo feature before any test module is imported —
`pyproject.toml [tool.maturin] features` ships only `default-embedder`
(parity with the released wheel), so without this `Engine._write_vector_for_test`
and friends are missing and tests like `test_use_default_embedder.py`,
`test_ffi_safety.py`, the AC-067 force-panic probe, etc. break.

This rebuild runs at conftest IMPORT time (module level below), NOT as a
session-scoped fixture. A fixture runs during test *setup*, which is too late
for a test module that imports a test-hooks symbol at MODULE level
(`test_ffi_safety.py: from fathomdb._fathomdb import force_panic_for_test`):
that import is evaluated during *collection*, before any fixture runs, so the
binding must already carry the symbol. conftest.py top-level code is the
earliest hook that still runs before sibling test modules are collected. The
rebuild short-circuits if the test-hooks symbol is already present (developer
pre-warmed with `maturin develop --features ...,test-hooks,...`, or a previous
session in the same venv built it), so the `maturin develop` cost (~5-60s) is
paid at most once per venv.
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


_PROBE_SRC = (
    "import sys\n"
    "try:\n"
    "    from fathomdb import _fathomdb\n"
    "except Exception:\n"
    "    sys.exit(1)\n"
    "engine = getattr(_fathomdb, 'Engine', None)\n"
    "sys.exit(0 if engine is not None and hasattr(engine, '_write_vector_for_test') else 1)\n"
)


def _binding_has_test_hooks() -> bool:
    """Return True if the installed `Engine` already exposes the test-hooks
    surface (`_write_vector_for_test`).

    The probe runs in a CHILD process and must NOT import
    `fathomdb._fathomdb` into this (the pytest) process. Importing the
    hook-less extension here dlopen()s it; a subsequent `maturin develop`
    that overwrites the `.so` on disk cannot be re-imported in the same
    process — the dynamic loader returns the already-mapped library, so the
    rebuilt symbols never become visible and collection still fails on the
    missing `force_panic_for_test`. Probing out-of-process keeps this
    interpreter pristine so pytest's first import sees the FRESH `.so`."""

    return subprocess.run(
        [sys.executable, "-c", _PROBE_SRC],
        cwd=str(_PYTHON_SRC_DIR),
    ).returncode == 0


_REBUILD_OPT_IN = "FATHOMDB_TESTS_ALLOW_REBUILD"


class TestHooksBindingMissing(RuntimeError):
    """The binding lacks `test-hooks` and this environment did not opt in to a rebuild."""


def _ensure_test_hooks_binding() -> None:
    """Ensure the editable binding has `test-hooks` before sibling test modules
    are collected — rebuilding ONLY if this environment explicitly opted in.

    Skipped when not invoked from the source tree (e.g. running the
    test files against a pip-installed wheel for release-surface
    verification) — detected by the absence of `pyproject.toml` next
    to the tests directory.

    **TC-27 (0.8.20 Slice 5 fix-4) — the rebuild is OPT-IN, not opt-out.**

    This function used to shell out to `maturin develop` autonomously, and did
    so during an agent session that never issued a build command. It failed
    only benignly, and only by luck: the interpreter was not an activated
    virtualenv. Had it been, the rebuild would have rebound the SHARED `.venv`
    to whatever source tree pytest happened to be launched from — including an
    agent worktree, silently repointing every other consumer of that venv at
    unreviewed code.

    Per this repo's standing "fix the tooling, not the actor" rule, the guard
    lives here rather than in a note asking people to be careful: an
    unattended `pytest` can no longer mutate a shared environment as a
    side effect of test collection. Opt in with
    `FATHOMDB_TESTS_ALLOW_REBUILD=1` when a rebuild is genuinely intended.

    `FATHOMDB_TESTS_NO_REBUILD=1` is retained (it now merely reasserts the
    default) so existing invocations that set it keep working.

    :raises TestHooksBindingMissing: when a rebuild is required but not
        authorized. Loud and actionable beats either silently rebuilding or
        letting sibling modules fail on a confusing missing-attribute import.
    """

    pyproject = _PYTHON_SRC_DIR / "pyproject.toml"
    if not pyproject.exists():
        # Not an editable-install context (release-surface tests run
        # against a pip-installed wheel under a throwaway venv).
        return
    if _binding_has_test_hooks():
        # Developer already built with test-hooks (or a previous
        # pytest session in the same venv did). Nothing to do — this is the
        # normal path, and it never consults the opt-in.
        return

    if os.environ.get(_REBUILD_OPT_IN) != "1":
        raise TestHooksBindingMissing(
            "the installed `fathomdb` binding was built WITHOUT the `test-hooks` feature, "
            "so these tests cannot run.\n"
            "\n"
            "This conftest will NOT rebuild it for you: `maturin develop` rebinds the "
            "active virtualenv to this source tree, which silently repoints every other "
            "consumer of a SHARED venv (TC-27).\n"
            "\n"
            "Choose one:\n"
            f"  * authorize the rebuild here:  {_REBUILD_OPT_IN}=1 pytest ...\n"
            "  * or build it yourself, from an environment you intend to rebind:\n"
            f"      cd {_PYTHON_SRC_DIR} && python -m maturin develop --features "
            "pyo3/extension-module,test-hooks,default-embedder,default-reranker\n"
            "\n"
            "NEVER run either from a git worktree against a shared .venv."
        )

    if os.environ.get("FATHOMDB_TESTS_NO_REBUILD") == "1":
        raise TestHooksBindingMissing(
            f"contradictory configuration: {_REBUILD_OPT_IN}=1 asks for a rebuild while "
            "FATHOMDB_TESTS_NO_REBUILD=1 forbids one. Unset one of them."
        )

    print(
        "\n[conftest] EU-6 FIX-1: rebuilding fathomdb editable binding "
        "with test-hooks feature (one-time per venv; ~5-60s) ...",
        flush=True,
    )
    # `maturin develop` requires an activated virtualenv: it looks for
    # $VIRTUAL_ENV / $CONDA_PREFIX / a `.venv` in cwd-or-parents. A bare
    # subprocess inherits neither when pytest was launched via an absolute
    # interpreter path (e.g. `.venv/bin/python -m pytest`, or any venv not
    # named `.venv`), so it fails with "Couldn't find a virtualenv". Point it
    # at THIS interpreter's environment root (`sys.prefix` is the venv dir for
    # a venv interpreter) so the rebuilt `.so` lands in the venv we are running.
    env = dict(os.environ)
    env["VIRTUAL_ENV"] = sys.prefix
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
        env=env,
    )
    # Belt-and-suspenders: drop any cached `fathomdb*` modules so the first
    # post-rebuild import re-reads from disk. (The out-of-process probe above
    # means this interpreter has NOT yet dlopen()ed the extension, so the fresh
    # `.so` is what pytest collection loads.)
    for mod_name in list(sys.modules):
        if mod_name == "fathomdb" or mod_name.startswith("fathomdb."):
            del sys.modules[mod_name]


# Run the rebuild at conftest IMPORT time (before sibling test modules are
# collected), not as a fixture — see the module docstring for why a fixture is
# too late for module-level test-hooks imports (e.g. test_ffi_safety.py).
_ensure_test_hooks_binding()
