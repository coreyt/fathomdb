"""Shared support for the `sbom-survey` acceptance suite (0.8.20 Slice 31).

This suite is **RED by construction**: Slice 31 ships requirements, acceptance
criteria, design and tests; Slice 32 ships the code that turns them GREEN.

The one structural rule this file exists to enforce: **nothing imports
`sbom_survey` at module level.** A top-level import of a package that does not
exist yet raises at collection time, which aborts the whole module and hides
every criterion after the first. Tests call `require()` *inside the test body*
instead, so each acceptance criterion produces its own attributable FAILED with
the required behaviour restated in the message.

A `pytest.skip` would be a vacuous green and is used nowhere in this suite.
"""

from __future__ import annotations

import importlib
import json
import os
import subprocess
import sys
from pathlib import Path

import pytest

# <repo>/scripts/sbom-survey/tests/conftest.py
TESTS_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = TESTS_DIR.parent
REPO_ROOT = PROJECT_ROOT.parents[1]

# The mini-project is deliberately isolated (its own pyproject.toml, no
# workspace membership), so the suite must be runnable without installing it.
if str(PROJECT_ROOT) not in sys.path:
    sys.path.insert(0, str(PROJECT_ROOT))

# Basenames the survey recognizes as dependency manifests / lockfiles.
# Kept here as well as in the design doc so the suite is self-contained.
MANIFEST_BASENAMES = (
    "Cargo.toml",
    "Cargo.lock",
    "package.json",
    "package-lock.json",
    "pyproject.toml",
    "uv.lock",
    "setup.py",
    "yarn.lock",
    "pnpm-lock.yaml",
    "poetry.lock",
    "Pipfile",
)

TIER_VOCABULARY = ("shipped", "dev-tooling", "eval-only")

FIXTURE_PREFIX = "dev/release/fixtures/"


def require(module: str, criterion: str, behaviour: str):
    """Import `module`, or FAIL naming the criterion and the missing behaviour.

    Never skips. A missing implementation is a failed acceptance criterion,
    not an absent test.
    """
    try:
        return importlib.import_module(module)
    except Exception as exc:  # noqa: BLE001 - any import-time failure is a RED result
        pytest.fail(
            f"{criterion} is NOT SATISFIED: `{module}` is not implemented yet"
            " (it lands in 0.8.20 Slice 32).\n"
            f"  REQUIRED BEHAVIOUR: {behaviour}\n"
            f"  import {module!r} raised: {exc!r}"
        )


def run_cli(args: list[str], criterion: str, behaviour: str) -> subprocess.CompletedProcess:
    """Drive the CLI as a subprocess, or FAIL naming the criterion.

    Driving the CLI out-of-process is the second half of the no-module-level-
    import rule: an absent entry point becomes a normal process result rather
    than a collection error.
    """
    env = dict(os.environ)
    env["PYTHONPATH"] = os.pathsep.join(
        [str(PROJECT_ROOT), env.get("PYTHONPATH", "")]
    ).rstrip(os.pathsep)
    proc = subprocess.run(  # noqa: S603 - fixed argv, no shell
        [sys.executable, "-m", "sbom_survey", *args],
        capture_output=True,
        text=True,
        env=env,
        cwd=str(REPO_ROOT),
    )
    if "No module named" in proc.stderr and "sbom_survey" in proc.stderr:
        pytest.fail(
            f"{criterion} is NOT SATISFIED: the `sbom-survey` CLI does not exist yet"
            " (it lands in 0.8.20 Slice 32).\n"
            f"  REQUIRED BEHAVIOUR: {behaviour}\n"
            f"  `python -m sbom_survey {' '.join(args)}` exited {proc.returncode}"
            f" with: {proc.stderr.strip().splitlines()[-1] if proc.stderr.strip() else '<no stderr>'}"
        )
    return proc


def tracked_paths() -> list[str]:
    """Every path tracked in this repository, straight from git."""
    out = subprocess.run(  # noqa: S603 - fixed argv, no shell
        ["git", "-C", str(REPO_ROOT), "ls-files", "-z"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    return [p for p in out.split("\0") if p]


def tracked_manifest_paths() -> list[str]:
    """The tracked paths whose basename is a recognized manifest name."""
    return sorted(
        p
        for p in tracked_paths()
        if Path(p).name in MANIFEST_BASENAMES
        or (Path(p).name.startswith("requirements") and p.endswith(".txt"))
    )


def is_gitignored(path: str) -> bool:
    proc = subprocess.run(  # noqa: S603 - fixed argv, no shell
        ["git", "-C", str(REPO_ROOT), "check-ignore", "-q", "--", path],
        capture_output=True,
        text=True,
    )
    return proc.returncode == 0


@pytest.fixture()
def repo_root() -> Path:
    return REPO_ROOT


@pytest.fixture()
def project_root() -> Path:
    return PROJECT_ROOT


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def load_json(path: Path):
    return json.loads(read_text(path))
