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

That rebuild is gated: see `_test_hooks_gate.py` for the policy (TC-27) and
`_ensure_test_hooks_binding` below for how the observations are gathered. When
the gate returns DEGRADED the suite still collects and runs; only the tests
marked `@pytest.mark.requires_test_hooks` are skipped, with the gate's reason
attached, plus a banner in the pytest report header.

When a rebuild IS authorized it runs at conftest IMPORT time (module level
below), NOT as a session-scoped fixture. A fixture runs during test *setup*,
which is too late: a test module that imports the extension during *collection*
dlopen()s the stale `.so`, and a later `maturin develop` cannot be re-imported
in the same process. conftest.py top-level code is the earliest hook that still
runs before sibling test modules are collected. The rebuild short-circuits if
the test-hooks symbol is already present (developer pre-warmed with
`maturin develop --features ...,test-hooks,...`, or a previous session in the
same venv built it), so the `maturin develop` cost (~5-60s) is paid at most
once per venv.
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

import pytest

from _test_hooks_gate import (
    CONTRADICTORY,
    DEGRADED,
    REBUILD,
    REBUILD_OPT_IN,
    REBUILD_OPT_OUT,
    REQUIRES_HOOKS_MARKER,
    Decision,
    decide,
    skip_reason,
    venv_belongs_to_source_tree,
)


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


class TestHooksBindingMisconfigured(RuntimeError):
    """The environment both requests and forbids a `test-hooks` rebuild."""


def _ensure_test_hooks_binding() -> Decision:
    """Gather the observations `_test_hooks_gate.decide` needs, then act on its
    verdict — before sibling test modules are collected.

    **TC-27 (0.8.20 Slice 5 fix-4/fix-6) — the rebuild is OPT-IN and
    ownership-checked; a missing binding DEGRADES the run, it does not abort it.**

    This function used to shell out to `maturin develop` autonomously, and did
    so during an agent session that never issued a build command. It failed
    only benignly, and only by luck: the interpreter was not an activated
    virtualenv. Had it been, the rebuild would have rebound the SHARED `.venv`
    to whatever source tree pytest happened to be launched from — including an
    agent worktree, silently repointing every other consumer of that venv at
    unreviewed code.

    fix-4 closed that by raising at import time when the rebuild was not
    authorized — which made the *documented* default path
    (`pip install -e 'src/python[dev]'` then `scripts/agent-test.sh`) fail
    before collection, i.e. permanently red. fix-6 keeps the hazard closed but
    returns DEGRADED instead: the suite runs, and only the tests carrying
    ``@pytest.mark.requires_test_hooks`` skip (visibly, with the reason). The
    sanctioned dev loop authorizes the rebuild explicitly from
    ``scripts/agent-test.sh``.

    :raises TestHooksBindingMisconfigured: only for a contradictory
        opt-in/opt-out pair — a configuration error the caller must fix.
    """

    # Not an editable-install context when `pyproject.toml` is absent (the
    # release-surface tests run against a pip-installed wheel under a throwaway
    # venv). Probing the binding there would be a pointless subprocess, so keep
    # the short-circuit here rather than inside the pure policy.
    is_source_tree = (_PYTHON_SRC_DIR / "pyproject.toml").exists()

    decision = decide(
        is_source_tree=is_source_tree,
        # Developer already built with test-hooks (or a previous pytest session
        # in the same venv did) — the normal path, which never consults the opt-in.
        hooks_present=is_source_tree and _binding_has_test_hooks(),
        allow_rebuild=os.environ.get(REBUILD_OPT_IN) == "1",
        forbid_rebuild=os.environ.get(REBUILD_OPT_OUT) == "1",
        # `sys.prefix` is the environment root for a venv interpreter.
        venv_owned_by_source_tree=venv_belongs_to_source_tree(sys.prefix, _PYTHON_SRC_DIR),
    )

    if decision.action == CONTRADICTORY:
        raise TestHooksBindingMisconfigured(decision.reason)
    if decision.action != REBUILD:
        return decision

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

    # Re-probe rather than trusting a zero exit code. If `maturin develop`
    # somehow lands a binding that still lacks the hooks, DEGRADED (visible
    # skips) is the honest outcome — never let the marked tests run and
    # "pass" against a surface that is not there.
    if not _binding_has_test_hooks():
        return Decision(
            DEGRADED,
            "`maturin develop` completed but the rebuilt binding still does not expose "
            "the test-hooks surface. Check that the `test-hooks` Cargo feature still "
            "gates the expected methods.",
        )
    return decision


# Run the rebuild at conftest IMPORT time (before sibling test modules are
# collected), not as a fixture — see the module docstring for why a fixture is
# too late once a sibling module has dlopen()ed the stale extension.
_TEST_HOOKS_DECISION = _ensure_test_hooks_binding()


def pytest_report_header() -> list[str] | None:
    """Announce a degraded run in the pytest header, at ANY verbosity.

    Requirement: never a silent green. If the hook surface is missing, the
    reason is on screen before the first test runs, not buried in `-rs`.
    """

    if not _TEST_HOOKS_DECISION.is_degraded:
        return None
    return [
        "test-hooks: UNAVAILABLE — tests marked "
        f"`{REQUIRES_HOOKS_MARKER}` will be SKIPPED (not run).",
        *(f"  {line}" for line in _TEST_HOOKS_DECISION.reason.splitlines()),
    ]


def pytest_collection_modifyitems(items: list[pytest.Item]) -> None:
    """Skip — visibly, with the gate's reason — every test that needs the
    `test-hooks` surface when the gate came back DEGRADED.

    A real `pytest.mark.skip`, so the run reports `s` and `-rs` prints the
    reason. Tests that do not carry the marker are untouched: a missing dev-only
    feature must not silently shrink the rest of the suite either.
    """

    if not _TEST_HOOKS_DECISION.is_degraded:
        return
    mark = pytest.mark.skip(reason=skip_reason(_TEST_HOOKS_DECISION.reason))
    for item in items:
        if item.get_closest_marker(REQUIRES_HOOKS_MARKER) is not None:
            item.add_marker(mark)
