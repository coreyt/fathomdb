"""EU-6 FIX-1 — workflow + manifest YAML/JSON content assertions (RED).

These tests assert the FIX-1 packaging invariants directly against the
checked-in workflow + manifest files (no build required). They are NOT
gated on any env var; they run on every ``pytest`` invocation. See
``dev/design/0.7.1-EU-6-FIX-1-design.md`` §6 for the design.

Covers:
- AC-FIX1-5: ``ci.yml`` wheel-size-gate matrix covers all 4 release-target
  platforms (Linux x86_64, Linux aarch64, macOS, Windows).
- AC-FIX1-6: ``pyproject.toml [tool.maturin] features`` does NOT list
  ``test-hooks``.
- AC-FIX1-7: ``package.json`` ``scripts.build:native`` carries
  ``--features default-embedder``.
- AC-FIX1-8: ``release.yml`` build-python's maturin-action ``args:``
  carries an explicit ``--features pyo3/extension-module,default-embedder``
  list (not pyproject discovery), and does NOT carry ``test-hooks``.
- AC-FIX1-9: collapsed transitively with AC-FIX1-7 per design §3.1 —
  asserted by the ``build:native`` script check above.

The assertions are content-only sanity checks; ``actionlint`` (run by
the ``verify-release`` job) covers workflow schema validity.
"""

from __future__ import annotations

import json
import tomllib
from pathlib import Path

import pytest
import yaml

REPO_ROOT = Path(__file__).resolve().parents[3]
RELEASE_YML = REPO_ROOT / ".github" / "workflows" / "release.yml"
CI_YML = REPO_ROOT / ".github" / "workflows" / "ci.yml"
PYPROJECT_TOML = REPO_ROOT / "src" / "python" / "pyproject.toml"
PACKAGE_JSON = REPO_ROOT / "src" / "ts" / "package.json"


def _load_yaml(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as fh:
        return yaml.safe_load(fh)


def test_pyproject_excludes_test_hooks() -> None:
    """AC-FIX1-6: pyproject.toml [tool.maturin] features must not list
    ``test-hooks``. Test-hooks is dev-only and must be reintroduced via
    pytest fixture, not via pyproject discovery."""

    with PYPROJECT_TOML.open("rb") as fh:
        data = tomllib.load(fh)
    features = data.get("tool", {}).get("maturin", {}).get("features", [])
    assert "test-hooks" not in features, (
        f"pyproject.toml still lists 'test-hooks' in [tool.maturin] features "
        f"({features!r}); release wheels will leak _write_vector_for_test "
        f"and friends to PyPI consumers."
    )


def test_package_json_build_native_has_default_embedder_feature() -> None:
    """AC-FIX1-7 (and transitively AC-FIX1-9): ``scripts.build:native``
    must include ``--features default-embedder`` so the shipped .node
    honours ``useDefaultEmbedder: true``."""

    data = json.loads(PACKAGE_JSON.read_text(encoding="utf-8"))
    build_native = data.get("scripts", {}).get("build:native", "")
    assert "--features default-embedder" in build_native, (
        f"package.json scripts.build:native ({build_native!r}) is missing "
        f"'--features default-embedder'; published .node will raise "
        f"EmbedderNotConfigured on useDefaultEmbedder: true."
    )


def test_release_workflow_build_python_has_explicit_features() -> None:
    """AC-FIX1-8: release.yml's build-python job must pass an explicit
    ``--features pyo3/extension-module,default-embedder`` list in the
    maturin-action ``args:`` — NOT rely on pyproject feature discovery,
    and must NOT carry ``test-hooks``."""

    data = _load_yaml(RELEASE_YML)
    build_python = data["jobs"]["build-python"]
    steps = build_python["steps"]
    maturin_step = next(
        (s for s in steps if isinstance(s.get("uses"), str) and "PyO3/maturin-action" in s["uses"]),
        None,
    )
    assert maturin_step is not None, "build-python has no PyO3/maturin-action step"
    args = maturin_step.get("with", {}).get("args", "")
    assert "--features" in args, (
        f"build-python maturin-action args ({args!r}) has no explicit --features; "
        f"relying on pyproject discovery is dev/prod skew."
    )
    assert "default-embedder" in args, (
        f"build-python maturin-action args ({args!r}) does not name "
        f"'default-embedder' — shipped wheel will not compile the BGE loader."
    )
    assert "pyo3/extension-module" in args, (
        f"build-python maturin-action args ({args!r}) does not name "
        f"'pyo3/extension-module' — required for the extension build."
    )
    assert "test-hooks" not in args, (
        f"build-python maturin-action args ({args!r}) contains 'test-hooks'; "
        f"dev-only hooks must never appear on the release-build feature axis."
    )


def test_ci_wheel_size_matrix_covers_all_release_platforms() -> None:
    """AC-FIX1-5: wheel-size-gate matrix must cover Linux x86_64,
    Linux aarch64, macOS (x86_64 or arm64), and Windows x86_64 — the
    four release-target platform families. Each entry must carry a
    ``baseline_bytes`` integer for regression gating."""

    data = _load_yaml(CI_YML)
    job = data["jobs"]["wheel-size-gate"]
    matrix_include = job["strategy"]["matrix"]["include"]

    required_targets = {
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
        "x86_64-pc-windows-msvc",
    }
    macos_targets = {"x86_64-apple-darwin", "aarch64-apple-darwin"}

    actual_targets = {entry.get("target") for entry in matrix_include}
    missing = required_targets - actual_targets
    assert not missing, (
        f"wheel-size-gate matrix is missing required platforms: {missing!r}; "
        f"current entries cover {actual_targets!r}. AC-FIX1-5 requires all "
        f"4 release-target families to be gated."
    )
    assert actual_targets & macos_targets, (
        f"wheel-size-gate matrix is missing a macOS entry; current entries "
        f"cover {actual_targets!r}."
    )

    for entry in matrix_include:
        baseline = entry.get("baseline_bytes")
        assert isinstance(baseline, int) and baseline > 0, (
            f"wheel-size-gate matrix entry {entry!r} is missing a positive "
            f"integer baseline_bytes."
        )


@pytest.mark.parametrize(
    "release_napi_step_match",
    ["build:native"],
)
def test_release_workflow_build_napi_uses_build_native(release_napi_step_match: str) -> None:
    """AC-FIX1-9 (companion to AC-FIX1-7): release.yml's build-napi job
    must invoke ``npm run build:native`` (relying on the package.json
    script edited under AC-FIX1-7 to carry ``--features
    default-embedder``). This passes on current main already; the FIX-1
    invariant is the package.json script content (see
    ``test_package_json_build_native_has_default_embedder_feature``)."""

    data = _load_yaml(RELEASE_YML)
    build_napi = data["jobs"]["build-napi"]
    steps = build_napi["steps"]
    run_strings = [s.get("run", "") for s in steps if isinstance(s.get("run"), str)]
    assert any(release_napi_step_match in r for r in run_strings), (
        f"build-napi has no step running 'npm run build:native'; current run "
        f"steps: {run_strings!r}"
    )
