"""`uv.lock`, `pyproject.toml` and `requirements*.txt` parsing (design §5.5).

**No `setup.py` is ever executed or AST-parsed.** The only tracked `setup.py`
files are the four excluded pip-skew fixtures, so the single most fragile parser
in the Python packaging space is never written (§5.2). If a real `setup.py` is
ever tracked it is DISCOVERED, and it contributes no declarations rather than
being mis-parsed.
"""

from __future__ import annotations

import tomllib
from typing import Any, Mapping

from packaging.requirements import InvalidRequirement, Requirement
from packaging.utils import canonicalize_name

from . import Declaration, LockPackage, ManifestParseError

__all__ = ["parse_pyproject", "parse_requirements", "parse_uv_lock"]

#: A lock entry whose source is one of these IS the project being locked, not a
#: dependency of it — `src/python/uv.lock` locks `fathomdb` itself.
_SELF_SOURCES = ("editable", "virtual", "directory")


def _load_toml(path: str, text: str) -> dict[str, Any]:
    try:
        return tomllib.loads(text)
    except Exception as exc:  # noqa: BLE001 - any parse failure is exit 3
        raise ManifestParseError(path, exc) from exc


def _dep_names(block: Any) -> list[str]:
    names: list[str] = []
    if isinstance(block, list):
        for item in block:
            if isinstance(item, Mapping) and isinstance(item.get("name"), str):
                names.append(canonicalize_name(item["name"]))
    elif isinstance(block, Mapping):
        for group in block.values():
            names.extend(_dep_names(group))
    return names


def parse_uv_lock(path: str, text: str) -> list[LockPackage]:
    """Every `[[package]]` in a `uv.lock`, with its resolved edges."""
    data = _load_toml(path, text)
    entries = data.get("package", [])

    present = {
        canonicalize_name(entry["name"])
        for entry in entries
        if isinstance(entry, Mapping) and isinstance(entry.get("name"), str)
    }

    packages: list[LockPackage] = []
    for entry in entries:
        if not isinstance(entry, Mapping):
            continue
        name = entry.get("name")
        version = entry.get("version")
        if not isinstance(name, str) or not isinstance(version, str):
            continue
        source = entry.get("source")
        if isinstance(source, Mapping) and any(k in source for k in _SELF_SOURCES):
            continue
        key = canonicalize_name(name)
        depends_on = [
            dep
            for dep in (
                *_dep_names(entry.get("dependencies")),
                *_dep_names(entry.get("optional-dependencies")),
                *_dep_names(entry.get("dev-dependencies")),
            )
            if dep in present and dep != key
        ]
        packages.append(
            LockPackage(
                ecosystem="pypi",
                name=name,
                version=version,
                key=key,
                depends_on=depends_on,
            )
        )
    return packages


def _declaration(spec: str, kind: str, path: str) -> Declaration | None:
    try:
        requirement = Requirement(spec)
    except InvalidRequirement:
        return None
    constraint = str(requirement.specifier) or "*"
    return Declaration(
        ecosystem="pypi",
        name=requirement.name,
        constraint=constraint,
        kind=kind,
        manifest_path=path,
    )


def parse_pyproject(path: str, text: str) -> list[Declaration]:
    """`project.dependencies` + `project.optional-dependencies` (PEP 621)."""
    data = _load_toml(path, text)
    project = data.get("project")
    if not isinstance(project, Mapping):
        return []

    declarations: list[Declaration] = []
    for spec in project.get("dependencies", []) or []:
        if isinstance(spec, str):
            declaration = _declaration(spec, "normal", path)
            if declaration is not None:
                declarations.append(declaration)

    optional = project.get("optional-dependencies")
    if isinstance(optional, Mapping):
        for group, specs in optional.items():
            kind = "dev" if group in ("dev", "test", "lint", "typecheck") else "optional"
            for spec in specs or []:
                if isinstance(spec, str):
                    declaration = _declaration(spec, kind, path)
                    if declaration is not None:
                        declarations.append(declaration)
    return declarations


def parse_requirements(path: str, text: str) -> list[tuple[Declaration, str | None]]:
    """`requirements*.txt` declarations, each with its pinned version if any.

    No lock exists for a `requirements.txt` (§5.5), so an `==` pin IS the locked
    version and anything looser resolves to nothing — which becomes
    `locked_version=None` and therefore `status="unknown"`, never `current`.
    """
    parsed: list[tuple[Declaration, str | None]] = []
    for raw in text.splitlines():
        line = raw.split("#", 1)[0].strip()
        if not line or line.startswith("-"):
            continue
        declaration = _declaration(line, "normal", path)
        if declaration is None:
            continue
        pinned: str | None = None
        try:
            requirement = Requirement(line)
        except InvalidRequirement:  # pragma: no cover - _declaration already returned
            requirement = None
        if requirement is not None:
            exact = [s for s in requirement.specifier if s.operator == "=="]
            if len(exact) == 1 and "*" not in exact[0].version:
                pinned = exact[0].version
        parsed.append((declaration, pinned))
    return parsed
