"""Manifest discovery — `git ls-files`-derived, never a filesystem walk (REQ-1, REQ-2).

Design §5.1. A filesystem walk sees `target/`, `node_modules/`, `.venv/`, `site/`
and every gitignored scratch tree — tens of thousands of vendored manifests the
project does not own — plus the gitignored `/python/` tree, whose pinned `0.1.0`
shim would be silently pulled into the SBOM on a developer machine that has it.
`git ls-files` makes all of that structurally unreachable.
"""

from __future__ import annotations

import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Iterable

__all__ = [
    "MANIFEST_TABLE",
    "ManifestRef",
    "discover_manifests",
    "git_ls_files",
]

#: The recognized-basename table (design §5.1). ⚠ MIRRORED 1:1 BY
#: `MANIFEST_BASENAMES` in `tests/conftest.py` AND BY THE DESIGN TABLE; the
#: three must be kept in lockstep. A name present in the design but missing here
#: is a manifest this tool silently skips.
#:
#: Two groups. The first matches tracked paths today. The second currently
#: matches nothing and exists as a FORWARD GUARD: adding one of those files to
#: the repository must make it DISCOVERED (and then fail REQ-4 tiering loudly)
#: rather than silently ignored.
MANIFEST_TABLE: dict[str, tuple[str, str]] = {
    # --- matches tracked paths today ---
    "Cargo.toml": ("cargo", "manifest"),
    "Cargo.lock": ("cargo", "lockfile"),
    "package.json": ("npm", "manifest"),
    "package-lock.json": ("npm", "lockfile"),
    "pyproject.toml": ("pypi", "manifest"),
    "uv.lock": ("pypi", "lockfile"),
    "setup.py": ("pypi", "manifest"),
    # --- recognized, forward-looking: nothing tracked matches these today ---
    "yarn.lock": ("npm", "lockfile"),
    "pnpm-lock.yaml": ("npm", "lockfile"),
    "poetry.lock": ("pypi", "lockfile"),
    "Pipfile": ("pypi", "manifest"),
    "setup.cfg": ("pypi", "manifest"),
}

#: `requirements*.txt` is a glob rather than an exact basename, so it is matched
#: separately. It is a pypi MANIFEST (no lock exists for it — §5.5).
_REQUIREMENTS_ECOSYSTEM_KIND = ("pypi", "manifest")


@dataclass(frozen=True, order=True)
class ManifestRef:
    """A tracked dependency manifest or lockfile.

    `path` is repo-relative and POSIX-separated, exactly as `git ls-files`
    reports it.
    """

    path: str
    ecosystem: str
    kind: str


def _classify_basename(path: str) -> tuple[str, str] | None:
    name = path.rsplit("/", 1)[-1]
    if name in MANIFEST_TABLE:
        return MANIFEST_TABLE[name]
    if name.startswith("requirements") and name.endswith(".txt"):
        return _REQUIREMENTS_ECOSYSTEM_KIND
    return None


def git_ls_files(repo_root: Path) -> list[str]:
    """Every path tracked at `repo_root`, straight from git.

    `-z` because repository paths may contain anything but NUL. This is the
    tool's entire git surface — no `GitPython`, no second subprocess (§5.7).
    """
    completed = subprocess.run(  # noqa: S603 - fixed argv, no shell
        ["git", "-C", str(repo_root), "ls-files", "-z"],
        capture_output=True,
        text=True,
        check=True,
    )
    return [p for p in completed.stdout.split("\0") if p]


def discover_manifests(
    repo_root: Path | str,
    *,
    ls_files: Callable[[Path], Iterable[str]] | None = None,
) -> list[ManifestRef]:
    """The tracked manifests/lockfiles at `repo_root`, sorted by path.

    The candidate set comes from `ls_files` — `git_ls_files` by default,
    injectable for tests — and from nowhere else. There is no `os.walk`, no
    `glob` and no `Path.rglob` in this package's discovery path, so a manifest
    that exists on disk but is not tracked can never reach the survey.
    """
    root = Path(repo_root)
    runner = ls_files if ls_files is not None else git_ls_files

    refs: list[ManifestRef] = []
    for path in runner(root):
        classified = _classify_basename(path)
        if classified is None:
            continue
        ecosystem, kind = classified
        refs.append(ManifestRef(path=path, ecosystem=ecosystem, kind=kind))
    return sorted(refs, key=lambda ref: ref.path)
