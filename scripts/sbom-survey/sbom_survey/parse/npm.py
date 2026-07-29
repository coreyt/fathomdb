"""`package-lock.json` (v3) and `package.json` parsing (design §5.5)."""

from __future__ import annotations

import json
from typing import Any, Mapping

from . import Declaration, LockPackage, ManifestParseError

__all__ = ["parse_lock", "parse_manifest"]

_DEPENDENCY_TABLES = (
    ("dependencies", "normal"),
    ("devDependencies", "dev"),
    ("optionalDependencies", "optional"),
)

_NODE_MODULES = "node_modules/"


def _load(path: str, text: str) -> dict[str, Any]:
    try:
        return json.loads(text)
    except Exception as exc:  # noqa: BLE001 - any parse failure is exit 3
        raise ManifestParseError(path, exc) from exc


def _name_of(key: str, entry: Mapping[str, Any]) -> str:
    name = entry.get("name")
    if isinstance(name, str) and name:
        return name
    # `node_modules/@scope/pkg` and the nested
    # `node_modules/a/node_modules/@scope/pkg` both end in the real name.
    index = key.rfind(_NODE_MODULES)
    return key[index + len(_NODE_MODULES) :] if index >= 0 else key


def _resolve(packages: Mapping[str, Any], from_key: str, dep_name: str) -> str | None:
    """npm's own resolution: nearest `node_modules` walking up from `from_key`.

    A v3 lock can carry `node_modules/a/node_modules/b` for a conflicting
    version, so `b` must resolve to the nested entry when reached through `a`
    and to the hoisted one otherwise — otherwise the two distinct components
    would share edges they do not have.
    """
    base = from_key
    while True:
        candidate = f"{base}/{_NODE_MODULES}{dep_name}" if base else f"{_NODE_MODULES}{dep_name}"
        if candidate in packages:
            return candidate
        if not base:
            return None
        index = base.rfind(f"/{_NODE_MODULES}")
        base = base[:index] if index >= 0 else ""


def parse_lock(path: str, text: str) -> list[LockPackage]:
    """Every real entry in a `package-lock.json` v3 `packages` map."""
    data = _load(path, text)
    packages = data.get("packages")
    if not isinstance(packages, Mapping):
        return []

    resolved: list[LockPackage] = []
    for key, entry in packages.items():
        if key == "" or not isinstance(entry, Mapping):
            continue  # the root project itself is not one of its own dependencies
        if entry.get("link") is True:
            continue  # a workspace symlink, not a resolved third-party package
        version = entry.get("version")
        if not isinstance(version, str) or not version:
            continue
        name = _name_of(key, entry)

        depends_on: list[str] = []
        for table, _kind in _DEPENDENCY_TABLES:
            deps = entry.get(table)
            if not isinstance(deps, Mapping):
                continue
            for dep_name in deps:
                target = _resolve(packages, key, dep_name)
                if target is not None:
                    depends_on.append(target)

        resolved.append(
            LockPackage(
                ecosystem="npm",
                name=name,
                version=version,
                key=key,
                depends_on=depends_on,
            )
        )
    return resolved


def parse_manifest(path: str, text: str) -> list[Declaration]:
    """`dependencies` / `devDependencies` / `optionalDependencies` declarations."""
    data = _load(path, text)
    declarations: list[Declaration] = []
    for table, kind in _DEPENDENCY_TABLES:
        entries = data.get(table)
        if not isinstance(entries, Mapping):
            continue
        for name, constraint in entries.items():
            declarations.append(
                Declaration(
                    ecosystem="npm",
                    name=name,
                    constraint=constraint if isinstance(constraint, str) else "*",
                    kind=kind,
                    manifest_path=path,
                )
            )
    return declarations
