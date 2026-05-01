"""Regression tests for clean interpreter shutdown after Engine.open.

Memex (consumer) reported on 0.5.5 that a Python process opening an engine
and exiting without an explicit `engine.close()` hangs at interpreter
shutdown. Root cause: CPython does not reliably run destructors on
module-level refs at exit, so the writer / vector-projection worker threads
keep the process alive.

We assert that a subprocess exercising the affected code paths exits within
a bounded time. The fix is an ``atexit`` hook that closes live engines.
"""

from __future__ import annotations

import os
import subprocess
import sys
import textwrap
from pathlib import Path

import fathomdb


def _python_with_fathomdb() -> str:
    """Return a Python interpreter that can ``import fathomdb``.

    ``sys.executable`` under ``uv run pytest`` can be the system interpreter
    (which lacks the compiled ``_fathomdb`` extension). Prefer the venv
    python colocated with the loaded fathomdb module when present.
    """
    # First try sys.executable.
    candidates: list[str] = [sys.executable]
    search_roots = [
        Path(fathomdb.__file__).resolve().parent,
        Path(__file__).resolve().parent,
    ]
    for root in search_roots:
        for parent in [root, *root.parents]:
            venv_py = parent / ".venv" / "bin" / "python3"
            if venv_py.exists():
                candidates.append(str(venv_py))
                break
    for py in candidates:
        probe = subprocess.run(
            [py, "-c", "import fathomdb"],
            capture_output=True,
            text=True,
            timeout=10,
        )
        if probe.returncode == 0:
            return py
    raise RuntimeError(f"no interpreter can import fathomdb; tried {candidates}")


def _run_subprocess(script: str, tmp_path: Path) -> subprocess.CompletedProcess:
    db_path = tmp_path / "shutdown.db"
    full = textwrap.dedent(
        f"""
        import os, sys
        DB = {str(db_path)!r}
        {script}
        print("__main_returning__", flush=True)
        """
    )
    py = _python_with_fathomdb()
    return subprocess.run(
        [py, "-c", full],
        capture_output=True,
        text=True,
        timeout=15,
        env={**os.environ},
    )


def test_engine_open_does_not_block_interpreter_shutdown(tmp_path: Path) -> None:
    """A bare Engine.open without close() must allow the interpreter to exit."""
    script = """
        from fathomdb import Engine
        engine = Engine.open(DB)
        # Intentionally do NOT call engine.close() — model-level refs on a
        # real consumer app would never run __del__ reliably at shutdown.
    """
    # Wrap in a wall-clock assertion distinct from the subprocess timeout so
    # we get a clear diagnostic if the process wedges.
    import time

    start = time.monotonic()
    result = _run_subprocess(script, tmp_path)
    elapsed = time.monotonic() - start
    assert result.returncode == 0, (
        f"exit={result.returncode} stdout={result.stdout!r} stderr={result.stderr!r}"
    )
    assert "__main_returning__" in result.stdout
    assert elapsed < 5.0, f"interpreter took {elapsed:.1f}s to exit after Engine.open"


def test_engine_open_with_admin_ops_does_not_block_shutdown(tmp_path: Path) -> None:
    """configure_embedding + configure_vec should not block shutdown either."""
    script = """
        from fathomdb import Engine

        class Identity:
            model_identity = "test/model"
            model_version = "v1"
            dimensions = 384
            normalization_policy = "l2"

        class DummyEmbedder:
            def identity(self):
                return Identity()
            def max_tokens(self):
                return 512

        engine = Engine.open(DB)
        engine.admin.configure_embedding(DummyEmbedder())
        engine.admin.configure_vec("KnowledgeItem", source="chunks")
    """
    import time

    start = time.monotonic()
    result = _run_subprocess(script, tmp_path)
    elapsed = time.monotonic() - start
    assert result.returncode == 0, (
        f"exit={result.returncode} stdout={result.stdout!r} stderr={result.stderr!r}"
    )
    assert "__main_returning__" in result.stdout
    assert elapsed < 5.0, f"interpreter took {elapsed:.1f}s to exit after admin ops"
