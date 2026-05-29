"""EU-6 FIX-1 — release-surface introspection tests (RED).

Builds a release-equivalent ``fathomdb`` wheel (i.e. ``maturin build
--release --features pyo3/extension-module,default-embedder``, with NO
``test-hooks``), installs it into a throwaway venv, and asserts the
*published-artifact-equivalent* surface:

- AC-FIX1-1: ``_write_vector_for_test`` and
  ``_configure_vector_kind_for_test`` are NOT present on
  ``fathomdb.Engine`` after installing the release-shaped wheel.
- AC-FIX1-3: ``Engine.open(path, use_default_embedder=True)`` succeeds
  (network-gated via ``FATHOMDB_SKIP_NETWORK_TESTS`` symmetrically with
  EU-5c).

Gated on ``RELEASE_SURFACE_TESTS=1`` because the build is slow
(~30-60s cold). Default ``pytest`` invocations skip with a clear log
line; CI sets the env var in a dedicated job.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import textwrap
import venv
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[3]
PYTHON_SRC_DIR = REPO_ROOT / "src" / "python"


def _skip_if_release_surface_not_enabled() -> None:
    if os.environ.get("RELEASE_SURFACE_TESTS") != "1":
        pytest.skip(
            "RELEASE_SURFACE_TESTS!=1; set RELEASE_SURFACE_TESTS=1 to enable "
            "release-equivalent wheel build + introspection."
        )


def _skip_if_no_network() -> None:
    if os.environ.get("FATHOMDB_SKIP_NETWORK_TESTS"):
        pytest.skip("FATHOMDB_SKIP_NETWORK_TESTS set; skipping network-hitting test")


@pytest.fixture(scope="module")
def release_venv_python(tmp_path_factory: pytest.TempPathFactory) -> Path:
    """Build a release-equivalent wheel and install it into a fresh
    venv. Returns the venv's Python executable.

    The maturin invocation mirrors ``release.yml::jobs.build-python``
    EXACTLY (per the FIX-1 design §5.1) — no ``test-hooks``, with
    ``default-embedder``.
    """

    _skip_if_release_surface_not_enabled()

    work = tmp_path_factory.mktemp("eu6_fix1_release")
    wheel_dir = work / "wheels"
    wheel_dir.mkdir()

    # Build the release-equivalent wheel.
    subprocess.check_call(
        [
            sys.executable,
            "-m",
            "maturin",
            "build",
            "--release",
            "--out",
            str(wheel_dir),
            "-i",
            sys.executable,
            "--features",
            "pyo3/extension-module,default-embedder",
        ],
        cwd=str(PYTHON_SRC_DIR),
    )

    wheels = sorted(wheel_dir.glob("*.whl"))
    assert wheels, f"maturin produced no wheels in {wheel_dir}"

    # Create a throwaway venv and install the wheel.
    venv_dir = work / "venv"
    venv.create(str(venv_dir), with_pip=True, clear=True)
    if os.name == "nt":
        venv_python = venv_dir / "Scripts" / "python.exe"
    else:
        venv_python = venv_dir / "bin" / "python"
    subprocess.check_call(
        [str(venv_python), "-m", "pip", "install", "--quiet", str(wheels[0])]
    )
    return venv_python


def _run_in_venv(venv_python: Path, script: str) -> str:
    return subprocess.check_output(
        [str(venv_python), "-c", textwrap.dedent(script)],
        text=True,
    )


def test_release_wheel_hides_test_hooks_methods(release_venv_python: Path) -> None:
    """AC-FIX1-1: the release-equivalent wheel must NOT expose
    ``_write_vector_for_test`` or ``_configure_vector_kind_for_test``
    on the native ``Engine`` PyO3 class."""

    script = """
        import json
        from fathomdb._fathomdb import Engine
        members = dir(Engine)
        leaked = [
            m for m in (
                "_write_vector_for_test",
                "_configure_vector_kind_for_test",
                "_force_panic_for_test",
            )
            if m in members
        ]
        print(json.dumps({"leaked": leaked, "members_sample": members[:20]}))
    """
    output = _run_in_venv(release_venv_python, script)
    result = json.loads(output.strip().splitlines()[-1])
    assert result["leaked"] == [], (
        f"Release-equivalent wheel still exposes dev-only hooks: "
        f"{result['leaked']!r}. AC-FIX1-1 requires these to be absent."
    )


def test_release_wheel_opens_with_default_embedder(
    release_venv_python: Path, tmp_path: Path
) -> None:
    """AC-FIX1-3: release-equivalent wheel must successfully open an
    engine with ``use_default_embedder=True`` (proves
    ``default-embedder`` feature is compiled in)."""

    _skip_if_no_network()

    db_path = tmp_path / "release_smoke.sqlite"
    script = f"""
        import json
        from fathomdb import Engine
        engine = Engine.open({str(db_path)!r}, use_default_embedder=True)
        try:
            report = engine.open_report()
            print(json.dumps({{
                "name": report.default_embedder.name,
                "dimension": report.default_embedder.dimension,
            }}))
        finally:
            engine.close()
    """
    output = _run_in_venv(release_venv_python, script)
    result = json.loads(output.strip().splitlines()[-1])
    assert result["name"] == "fathomdb-bge-small-en-v1.5", result
    assert result["dimension"] == 384, result
