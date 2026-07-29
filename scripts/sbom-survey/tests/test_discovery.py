"""AC-SBOM-01 .. AC-SBOM-04 — manifest discovery.

REQ-1 (discovery is `git ls-files`-derived) and REQ-2 (completeness).
Design: dev/design/0.8.20-slice-31-sbom-survey-tool.md §5.1.
"""

from __future__ import annotations

from pathlib import Path

from conftest import (
    REPO_ROOT,
    require,
    tracked_manifest_paths,
    tracked_paths,
)


def test_discovery_uses_git_ls_files_not_a_filesystem_walk(tmp_path: Path) -> None:
    """AC-SBOM-01.

    Discovery takes its candidate set from an injected `git ls-files` runner.
    A manifest that exists on disk but is NOT reported by that runner must be
    absent from the result — that is what makes `python/pyproject.toml`,
    `target/**` and `node_modules/**` structurally unreachable.
    """
    discovery = require(
        "sbom_survey.discovery",
        "AC-SBOM-01",
        "discover_manifests(root, *, ls_files=...) must derive its candidate set"
        " from the injected `git ls-files` runner ONLY; a manifest present on"
        " disk but untracked must never appear in the result.",
    )

    # Tracked, per the injected runner.
    (tmp_path / "Cargo.toml").write_text("[package]\nname='x'\n", encoding="utf-8")
    # On disk but NOT reported by the runner -> must be invisible.
    untracked = tmp_path / "vendored"
    untracked.mkdir()
    (untracked / "package.json").write_text("{}", encoding="utf-8")

    calls: list[Path] = []

    def fake_ls_files(root: Path) -> list[str]:
        calls.append(root)
        return ["Cargo.toml"]

    refs = discovery.discover_manifests(tmp_path, ls_files=fake_ls_files)

    assert calls, "the injected ls_files runner was never called"
    found = {r.path for r in refs}
    assert found == {"Cargo.toml"}, (
        "discovery must return exactly what git reports; got "
        f"{sorted(found)} — an untracked on-disk manifest leaked in"
    )


def test_gitignored_top_level_python_dir_is_out_of_scope() -> None:
    """AC-SBOM-02.

    `.gitignore` ignores `/python/` in its entirety (the sourceless recovered
    binding kept on disk for Memex). `python/pyproject.toml` is therefore
    UNTRACKED and out of BOM scope, even on a developer machine where the
    directory exists. This is the LBS charter §2 "noise against gitignored
    dev/eval lockfiles" trap.
    """
    assert not any(p.startswith("python/") for p in tracked_paths()), (
        "precondition changed: `python/` is now tracked — the design's scope"
        " ruling (§5.1) must be revisited before this criterion is re-graded"
    )

    discovery = require(
        "sbom_survey.discovery",
        "AC-SBOM-02",
        "discover_manifests(REPO_ROOT) must return no path under `python/`,"
        " because `.gitignore` ignores the whole `/python/` tree and the SBOM"
        " scope is `everything tracked on main`.",
    )

    refs = discovery.discover_manifests(REPO_ROOT)
    leaked = sorted(r.path for r in refs if r.path.startswith("python/"))
    assert not leaked, f"gitignored `/python/` leaked into the BOM scope: {leaked}"


def test_every_tracked_manifest_is_discovered() -> None:
    """AC-SBOM-03.

    Rule-based, not a frozen list: every tracked path whose basename is a
    recognized manifest name must be discovered. Nothing tracked is silently
    missed (REQ-2).
    """
    expected = tracked_manifest_paths()
    assert expected, "vacuous-pass guard: git reported zero tracked manifests"

    discovery = require(
        "sbom_survey.discovery",
        "AC-SBOM-03",
        "discover_manifests(REPO_ROOT) must return EVERY tracked path whose"
        " basename is a recognized manifest/lockfile name — computed from"
        " `git ls-files` at run time, not a frozen list.",
    )

    found = {r.path for r in discovery.discover_manifests(REPO_ROOT)}
    missing = sorted(set(expected) - found)
    assert not missing, f"tracked manifests not discovered: {missing}"


def test_ecosystem_and_kind_classification() -> None:
    """AC-SBOM-04.

    Each ref carries ecosystem in {cargo, npm, pypi} and kind in
    {manifest, lockfile}. Checked against four real tracked paths.
    """
    discovery = require(
        "sbom_survey.discovery",
        "AC-SBOM-04",
        "each ManifestRef must carry ecosystem in {cargo, npm, pypi} and kind"
        " in {manifest, lockfile}; e.g. Cargo.lock -> (cargo, lockfile),"
        " src/ts/package.json -> (npm, manifest), src/python/uv.lock ->"
        " (pypi, lockfile), tools/docs/requirements.txt -> (pypi, manifest).",
    )

    by_path = {r.path: r for r in discovery.discover_manifests(REPO_ROOT)}
    expected = {
        "Cargo.lock": ("cargo", "lockfile"),
        "src/ts/package.json": ("npm", "manifest"),
        "src/python/uv.lock": ("pypi", "lockfile"),
        "tools/docs/requirements.txt": ("pypi", "manifest"),
    }
    for path, (ecosystem, kind) in expected.items():
        ref = by_path.get(path)
        assert ref is not None, f"{path} was not discovered"
        assert (ref.ecosystem, ref.kind) == (ecosystem, kind), (
            f"{path}: expected ({ecosystem}, {kind}), got "
            f"({ref.ecosystem}, {ref.kind})"
        )
