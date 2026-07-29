"""`Cargo.lock` and `Cargo.toml` parsing (design §5.5)."""

from __future__ import annotations

import tomllib
from typing import Any, Mapping

from . import Declaration, LockPackage, ManifestParseError

__all__ = ["parse_lock", "parse_manifest", "workspace_dependencies"]

_DEPENDENCY_TABLES = (
    ("dependencies", "normal"),
    ("dev-dependencies", "dev"),
    ("build-dependencies", "build"),
)


def _load(path: str, text: str) -> dict[str, Any]:
    try:
        return tomllib.loads(text)
    except Exception as exc:  # noqa: BLE001 - any parse failure is exit 3
        raise ManifestParseError(path, exc) from exc


def parse_lock(path: str, text: str) -> list[LockPackage]:
    """Every `[[package]]` in a `Cargo.lock`, with its resolved edges.

    A lock `dependencies` entry is either `"name"` or `"name version"` — cargo
    only writes the version when the name is ambiguous — so edges are resolved
    against a name→versions index built from the same file.
    """
    data = _load(path, text)
    entries = data.get("package", [])

    by_name: dict[str, list[str]] = {}
    for entry in entries:
        name = entry.get("name")
        version = entry.get("version")
        if isinstance(name, str) and isinstance(version, str):
            by_name.setdefault(name, []).append(version)

    packages: list[LockPackage] = []
    for entry in entries:
        name = entry.get("name")
        version = entry.get("version")
        if not isinstance(name, str) or not isinstance(version, str):
            continue
        depends_on: list[str] = []
        for raw in entry.get("dependencies", []) or []:
            if not isinstance(raw, str):
                continue
            parts = raw.split()
            dep_name = parts[0]
            if len(parts) >= 2:
                dep_version: str | None = parts[1]
            else:
                candidates = by_name.get(dep_name, [])
                # Ambiguous and unqualified cannot happen in a well-formed lock
                # (cargo qualifies exactly then), so an unresolvable edge is
                # dropped rather than guessed — a wrong edge is worse than a
                # missing one.
                dep_version = candidates[0] if len(candidates) == 1 else None
            if dep_version is None:
                continue
            depends_on.append(f"{dep_name} {dep_version}")
        packages.append(
            LockPackage(
                ecosystem="cargo",
                name=name,
                version=version,
                key=f"{name} {version}",
                depends_on=depends_on,
            )
        )
    return packages


def workspace_dependencies(path: str, text: str) -> dict[str, str]:
    """`[workspace.dependencies]` version pins from a workspace root manifest.

    Member crates write `foo.workspace = true`, so the member is the declaring
    manifest (that is where a bump's edit site is) while the constraint string
    lives here.
    """
    data = _load(path, text)
    table = data.get("workspace", {}).get("dependencies", {})
    pins: dict[str, str] = {}
    if isinstance(table, Mapping):
        for name, spec in table.items():
            pins[name] = _constraint_of(spec, {}) or "*"
    return pins


def _constraint_of(spec: Any, workspace_pins: Mapping[str, str]) -> str | None:
    if isinstance(spec, str):
        return spec
    if not isinstance(spec, Mapping):
        return None
    if spec.get("workspace") is True:
        return None  # resolved by the caller, which knows the crate name
    version = spec.get("version")
    if isinstance(version, str):
        return version
    if "path" in spec:
        return "path"
    if "git" in spec:
        return "git"
    return "*"


def parse_manifest(
    path: str,
    text: str,
    *,
    workspace_pins: Mapping[str, str] | None = None,
) -> list[Declaration]:
    """`[dependencies]` / `[dev-dependencies]` / `[build-dependencies]` declarations.

    `[workspace.dependencies]` in a virtual workspace root is deliberately NOT a
    declaration site: the crate that writes `foo.workspace = true` is the one a
    bump has to touch, and the root's pin is folded in as that declaration's
    constraint instead.
    """
    data = _load(path, text)
    pins = dict(workspace_pins or {})

    declarations: list[Declaration] = []
    for table, kind in _DEPENDENCY_TABLES:
        entries = data.get(table, {})
        if not isinstance(entries, Mapping):
            continue
        for key, spec in entries.items():
            # `foo = { package = "real-crate", version = "1" }` renames.
            name = key
            if isinstance(spec, Mapping) and isinstance(spec.get("package"), str):
                name = spec["package"]
            constraint = _constraint_of(spec, pins)
            if constraint is None:
                constraint = pins.get(name, "workspace")
            declarations.append(
                Declaration(
                    ecosystem="cargo",
                    name=name,
                    constraint=constraint,
                    kind=kind,
                    manifest_path=path,
                )
            )
    return declarations
