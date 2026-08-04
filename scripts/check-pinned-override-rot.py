#!/usr/bin/env python3
"""Offline gate for npm overrides that can outlive their security purpose."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any


class Unverified(Exception):
    """The checked-in evidence cannot support a trustworthy verdict."""


VERSION = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?$")
COMPARATOR = re.compile(r"^(<=|>=|<|>|=)?\s*(\d+\.\d+\.\d+)$")


def read_json(path: Path, label: str) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except OSError as exc:
        raise Unverified(f"cannot read {label} {path}: {exc}") from exc
    except json.JSONDecodeError as exc:
        raise Unverified(f"{label} {path} is malformed JSON: {exc}") from exc
    if not isinstance(value, dict):
        raise Unverified(f"{label} {path} must be a JSON object")
    return value


def version_tuple(version: str) -> tuple[int, int, int]:
    match = VERSION.fullmatch(version)
    if not match:
        raise Unverified(f"unsupported non-exact semver version {version!r}")
    return tuple(int(component) for component in version.split("-", 1)[0].split("."))  # type: ignore[return-value]


def satisfies(version: str, range_text: str) -> bool:
    """Evaluate the deliberately small comparator grammar used by the snapshot."""
    value = version_tuple(version)
    comparators = [item.strip() for item in range_text.split(",") if item.strip()]
    if not comparators:
        raise Unverified(f"empty version range for {version}")
    for item in comparators:
        match = COMPARATOR.fullmatch(item)
        if not match:
            raise Unverified(f"unsupported range {range_text!r}; use comma-separated exact semver comparators")
        op, required_text = match.groups()
        required = version_tuple(required_text)
        if op in (None, "=") and value != required:
            return False
        if op == ">=" and value < required:
            return False
        if op == ">" and value <= required:
            return False
        if op == "<=" and value > required:
            return False
        if op == "<" and value >= required:
            return False
    return True


def nonempty_string(value: Any, name: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise Unverified(f"{name} must be a non-empty string")
    return value


def records_by_package(metadata: dict[str, Any]) -> dict[str, dict[str, Any]]:
    records = metadata.get("npm_overrides")
    if not isinstance(records, list):
        raise Unverified("metadata has no npm_overrides list")
    indexed: dict[str, dict[str, Any]] = {}
    for index, record in enumerate(records):
        if not isinstance(record, dict):
            raise Unverified(f"npm_overrides[{index}] must be an object")
        package = nonempty_string(record.get("package"), f"npm_overrides[{index}].package")
        if package in indexed:
            raise Unverified(f"npm_overrides records {package!r} more than once")
        indexed[package] = record
    return indexed


def validate_metadata(metadata: dict[str, Any]) -> None:
    if metadata.get("schema_version") != 1:
        raise Unverified("metadata schema_version must be 1")
    snapshot = metadata.get("advisory_snapshot")
    if not isinstance(snapshot, dict):
        raise Unverified("metadata has no advisory_snapshot object")
    for key in ("source", "retrieved_at", "provenance"):
        nonempty_string(snapshot.get(key), f"advisory_snapshot.{key}")
    scope = metadata.get("scope")
    if not isinstance(scope, dict):
        raise Unverified("metadata has no scope object")
    for key in ("npm", "cargo", "governed_commit_pins"):
        nonempty_string(scope.get(key), f"scope.{key}")
    advisories = metadata.get("advisories")
    if not isinstance(advisories, list):
        raise Unverified("metadata has no advisories list")
    seen_ids: set[str] = set()
    for index, advisory in enumerate(advisories):
        if not isinstance(advisory, dict):
            raise Unverified(f"advisories[{index}] must be an object")
        advisory_id = nonempty_string(advisory.get("id"), f"advisories[{index}].id")
        if advisory_id in seen_ids:
            raise Unverified(f"advisory {advisory_id!r} appears more than once")
        seen_ids.add(advisory_id)
        nonempty_string(advisory.get("package"), f"advisories[{index}].package")
        nonempty_string(advisory.get("vulnerable_range"), f"advisories[{index}].vulnerable_range")
        nonempty_string(advisory.get("url"), f"advisories[{index}].url")
        # Parse now, even when there is no matching pin, so a malformed snapshot
        # cannot turn into a clean pass merely because today has zero overrides.
        satisfies("0.0.0", advisory["vulnerable_range"])
    records_by_package(metadata)


def advisory_index(metadata: dict[str, Any]) -> dict[str, list[dict[str, Any]]]:
    indexed: dict[str, list[dict[str, Any]]] = {}
    for advisory in metadata["advisories"]:
        indexed.setdefault(advisory["package"], []).append(advisory)
    return indexed


def cargo_governed_pins(root: Path) -> list[str]:
    """Identify Cargo's actual override mechanisms without calling Cargo/network."""
    try:
        import tomllib
    except ImportError as exc:  # pragma: no cover - supported CI Python has it
        raise Unverified("python3.11+ tomllib is required to inspect Cargo override scope") from exc
    found: list[str] = []
    for manifest in root.glob("**/Cargo.toml"):
        if any(part in {"target", ".git"} for part in manifest.parts):
            continue
        try:
            parsed = tomllib.loads(manifest.read_text(encoding="utf-8"))
        except (OSError, tomllib.TOMLDecodeError) as exc:
            raise Unverified(f"cannot parse Cargo manifest {manifest}: {exc}") from exc
        if parsed.get("patch") or parsed.get("replace"):
            found.append(str(manifest.relative_to(root)))
        for section in ("dependencies", "dev-dependencies", "build-dependencies"):
            for name, spec in parsed.get(section, {}).items():
                if isinstance(spec, dict) and "git" in spec:
                    found.append(f"{manifest.relative_to(root)}:{section}.{name}")
    return found


def lockfile_dependent_ranges(lockfile: dict[str, Any], package: str) -> list[str]:
    """Return every dependency constraint the checked-in lock records for a pin."""
    packages = lockfile.get("packages")
    if not isinstance(packages, dict):
        raise Unverified("npm lockfile has no packages object")
    ranges: list[str] = []
    for location, entry in packages.items():
        if not isinstance(location, str) or not isinstance(entry, dict):
            raise Unverified("npm lockfile packages must map string paths to objects")
        dependencies = entry.get("dependencies", {})
        if not isinstance(dependencies, dict):
            raise Unverified(f"npm lockfile {location or '<root>'} has non-object dependencies")
        required_range = dependencies.get(package)
        if required_range is not None:
            if not isinstance(required_range, str):
                raise Unverified(f"npm lockfile dependency range for {package!r} is not a string")
            ranges.append(required_range)
    return sorted(set(ranges))


def check(root: Path, manifest_path: Path, lockfile_path: Path, metadata_path: Path) -> list[str]:
    manifest = read_json(manifest_path, "npm manifest")
    lockfile = read_json(lockfile_path, "npm lockfile")
    metadata = read_json(metadata_path, "pinned-override metadata")
    validate_metadata(metadata)

    overrides = manifest.get("overrides", {})
    if not isinstance(overrides, dict):
        raise Unverified("package.json overrides must be an object")
    records = records_by_package(metadata)
    failures: list[str] = []
    for package, override in overrides.items():
        if not isinstance(override, str):
            raise Unverified(f"npm override {package!r} is not an exact string version")
        version_tuple(override)
        record = records.pop(package, None)
        if record is None:
            failures.append(f"R3 npm override {package}@{override} has no recorded rationale")
            continue
        recorded_version = nonempty_string(record.get("version"), f"npm override {package}.version")
        if recorded_version != override:
            failures.append(f"npm override {package}@{override} disagrees with metadata version {recorded_version}")
        rationale = record.get("rationale")
        if not isinstance(rationale, str) or not rationale.strip():
            failures.append(f"R3 npm override {package}@{override} has no recorded rationale")
        for advisory in advisory_index(metadata).get(package, []):
            if satisfies(override, advisory["vulnerable_range"]):
                failures.append(
                    f"R1 npm override {package}@{override} is vulnerable to {advisory['id']} "
                    f"({advisory['vulnerable_range']})"
                )
        evidence = record.get("unpin_evidence")
        if not isinstance(evidence, dict):
            raise Unverified(f"npm override {package} has no unpin_evidence object")
        natural = nonempty_string(evidence.get("resolved_version"), f"npm override {package}.unpin_evidence.resolved_version")
        nonempty_string(evidence.get("provenance"), f"npm override {package}.unpin_evidence.provenance")
        ranges = evidence.get("dependent_ranges")
        if not isinstance(ranges, list) or not all(isinstance(item, str) and item for item in ranges):
            raise Unverified(f"npm override {package}.unpin_evidence.dependent_ranges must be a non-empty string list")
        lockfile_ranges = lockfile_dependent_ranges(lockfile, package)
        if sorted(set(ranges)) != lockfile_ranges:
            raise Unverified(
                f"npm override {package}.unpin_evidence.dependent_ranges {sorted(set(ranges))} does not match "
                f"the checked-in lockfile constraints {lockfile_ranges}"
            )
        safe = all(not satisfies(natural, advisory["vulnerable_range"]) for advisory in advisory_index(metadata).get(package, []))
        if safe and all(satisfies(natural, required_range) for required_range in ranges):
            failures.append(
                f"R2 npm override {package}@{override} is obsolete: recorded no-override resolution "
                f"{natural} satisfies every dependent range and known advisory"
            )
    for extra in sorted(records):
        failures.append(f"metadata records npm override {extra!r}, but package.json has no such override")

    cargo_pins = cargo_governed_pins(root)
    if cargo_pins:
        failures.append(
            "Cargo override/git pin(s) require an explicit governed record before this gate can verify them: "
            + ", ".join(cargo_pins)
        )
    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--lockfile", type=Path)
    parser.add_argument("--metadata", type=Path)
    args = parser.parse_args()
    root = args.root.resolve()
    manifest = args.manifest or root / "package.json"
    lockfile = args.lockfile or root / "package-lock.json"
    metadata = args.metadata or root / "scripts/pinned-override-rot.json"
    try:
        failures = check(root, manifest, lockfile, metadata)
    except Unverified as exc:
        print(f"UNVERIFIED pinned-override-rot: {exc}; refusing to report a clean result", file=sys.stderr)
        return 2
    if failures:
        for failure in failures:
            print(f"FAIL  pinned-override-rot: {failure}", file=sys.stderr)
        return 1
    print("ok pinned-override-rot: every governed npm override has an offline rationale and no recorded rot")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
