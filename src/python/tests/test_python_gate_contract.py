"""Reproducibility and native-stub contracts for the Python quality gate."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any

import pytest

_PYTHON_ROOT = Path(__file__).resolve().parents[1]
_PYPROJECT = _PYTHON_ROOT / "pyproject.toml"
_FIXTURE = _PYTHON_ROOT / "_typecheck_fixtures" / "_python_gate_contract.py"


def _load_pyproject() -> dict[str, Any]:
    if sys.version_info >= (3, 11):
        import tomllib
    else:
        import tomli as tomllib  # pyright: ignore[reportMissingImports]
    with _PYPROJECT.open("rb") as file:
        return tomllib.load(file)


def test_ruff_version_is_pinned_for_the_clean_clone_gate() -> None:
    """Lint and dev extras resolve the reviewed Ruff behavior identically."""
    extras = _load_pyproject()["project"]["optional-dependencies"]
    assert extras["lint"] == ["ruff==0.6.9"]
    assert "ruff==0.6.9" in extras["dev"]


def test_pyright_accepts_the_native_wrapper_contract() -> None:
    """The hand-maintained native stub covers every wrapper member it exposes."""
    pyright = shutil.which("pyright")
    if pyright is None:
        pytest.skip("pyright not installed; install the typecheck extra")

    proc = subprocess.run(
        [pyright, "--project", str(_PYPROJECT), "--outputjson", str(_FIXTURE)],
        capture_output=True,
        text=True,
        check=False,
    )
    assert proc.stdout, f"pyright emitted no JSON (stderr: {proc.stderr})"
    report = json.loads(proc.stdout)
    assert proc.returncode == 0 and report["summary"]["errorCount"] == 0, proc.stdout
