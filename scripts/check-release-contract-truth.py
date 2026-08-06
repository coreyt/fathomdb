#!/usr/bin/env python3
"""Fail closed when the 0.8.22 release-ready native contract drifts.

Usage: ``python3 scripts/check-release-contract-truth.py``.

This intentionally parses only the small, known-shaped release workflow
blocks.  GitHub's workflow syntax remains actionlint's responsibility.  Public
documentation is intentionally excluded: it describes only published
artifacts and is checked by ``check-public-doc-truth.py``.
"""

from __future__ import annotations

import json
import os
from pathlib import Path
import re
import sys


READY_STATUSES = {"published", "release-ready"}
EXPECTED_READY_TRIPLES = {
    "linux-x64-gnu",
    "linux-arm64-gnu",
    "darwin-x64",
    "darwin-arm64",
    "win32-x64-msvc",
}
JOB_HEADER = re.compile(r"^  ([A-Za-z][A-Za-z0-9_-]*):\s*$")
MATRIX_RUNNER = re.compile(r"^          - runner: ([^\s#]+)\s*$")
MATRIX_VALUE = re.compile(r"^            ([a-z_]+): ([^\s#]+)\s*$")
RUNNER = re.compile(r"^    runs-on: ([^\s#]+)\s*$", re.MULTILINE)
INLINE_NEEDS = re.compile(r"^    needs: \[([^]]*)\]\s*$", re.MULTILINE)
SCALAR_NEEDS = re.compile(r"^    needs: ([A-Za-z0-9_-]+)\s*$", re.MULTILINE)


def root() -> Path:
    return Path(os.environ.get("REPO_ROOT", Path(__file__).parent.parent)).resolve()


def fail(message: str) -> None:
    print(f"FAIL release-contract-truth: {message}", file=sys.stderr)
    raise SystemExit(1)


def read_json(path: Path) -> dict:
    try:
        value = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"cannot read {path}: {exc}")
    if not isinstance(value, dict):
        fail(f"{path} must contain a JSON object")
    return value


def workflow_jobs(path: Path) -> dict[str, str]:
    try:
        lines = path.read_text().splitlines()
    except OSError as exc:
        fail(f"cannot read {path}: {exc}")

    starts: list[tuple[str, int]] = []
    for index, line in enumerate(lines):
        match = JOB_HEADER.match(line)
        if match:
            starts.append((match.group(1), index))
    if not starts:
        fail("release workflow has no recognized jobs")

    jobs: dict[str, str] = {}
    for offset, (name, start) in enumerate(starts):
        if name in jobs:
            fail(f"release workflow declares job {name!r} more than once")
        end = starts[offset + 1][1] if offset + 1 < len(starts) else len(lines)
        jobs[name] = "\n".join(lines[start:end]) + "\n"
    return jobs


def matrix_rows(job_name: str, block: str, label_required: bool) -> list[tuple[str, str, str | None]]:
    lines = block.splitlines()
    rows: list[tuple[str, str, str | None]] = []
    for start, line in enumerate(lines):
        match = MATRIX_RUNNER.match(line)
        if not match:
            continue
        values: dict[str, str] = {}
        for candidate in lines[start + 1 :]:
            if MATRIX_RUNNER.match(candidate) or (candidate and len(candidate) - len(candidate.lstrip()) < 12):
                break
            value = MATRIX_VALUE.match(candidate)
            if value:
                values[value.group(1)] = value.group(2)
        target = values.get("target")
        if target is None:
            fail(f"{job_name} matrix row for runner {match.group(1)!r} has no target")
        label = values.get("label")
        if label_required and label is None:
            fail(f"{job_name} matrix row for target {target!r} has no N-API label")
        rows.append((match.group(1), target, label))
    if not rows:
        fail(f"{job_name} has no recognized matrix rows")
    return rows


def needs(job_name: str, block: str) -> list[str]:
    inline = INLINE_NEEDS.search(block)
    if inline:
        values = [value.strip() for value in inline.group(1).split(",") if value.strip()]
    else:
        scalar = SCALAR_NEEDS.search(block)
        if scalar:
            values = [scalar.group(1)]
        else:
            match = re.search(r"^    needs:\s*$\n((?:      - [A-Za-z0-9_-]+\s*$\n?)+)", block, re.MULTILINE)
            if not match:
                fail(f"{job_name} has no recognized needs block")
            values = re.findall(r"^      - ([A-Za-z0-9_-]+)\s*$", match.group(1), re.MULTILINE)
    if len(values) != len(set(values)):
        fail(f"{job_name} repeats a dependency")
    return values


def expected_package_fields(triple: str) -> tuple[list[str], list[str], list[str] | None]:
    if triple.startswith("linux-"):
        _, cpu, libc = triple.split("-", maxsplit=2)
        if libc != "gnu":
            fail(f"release-ready Linux triple {triple!r} is not glibc")
        return ["linux"], [cpu], ["glibc"]
    if triple.startswith("darwin-"):
        return ["darwin"], [triple.removeprefix("darwin-")], None
    if triple.startswith("win32-"):
        _, cpu, toolchain = triple.split("-", maxsplit=2)
        if toolchain != "msvc":
            fail(f"release-ready Windows triple {triple!r} is not MSVC")
        return ["win32"], [cpu], None
    fail(f"release-ready triple {triple!r} is not a supported platform family")


def require_runner(job_name: str, block: str, expected: str) -> None:
    match = RUNNER.search(block)
    if not match:
        fail(f"{job_name} has no recognized runs-on")
    if match.group(1) != expected:
        fail(f"{job_name} runs on {match.group(1)!r}, expected {expected!r}")


def main() -> None:
    repo = root()
    manifest = read_json(repo / "dev/platform-capabilities.json")
    release = manifest.get("release")
    platforms = manifest.get("platforms")
    if not isinstance(release, str) or not isinstance(platforms, list):
        fail("platform manifest must declare a string release and platforms list")
    if not all(isinstance(entry, dict) for entry in platforms):
        fail("platform manifest contains a non-object platform")

    ready = [entry for entry in platforms if entry.get("status") in READY_STATUSES]
    triples = [entry.get("triple") for entry in ready]
    if any(not isinstance(triple, str) for triple in triples):
        fail("release-ready platform has no string triple")
    if len(triples) != len(set(triples)):
        fail("release-ready platform matrix has a duplicate triple")

    forbidden = [
        triple for triple in triples
        if "musl" in triple or triple.startswith("win32-arm") or triple.startswith("win32-ia32")
    ]
    if forbidden:
        fail(f"unsupported triplets became release-ready: {sorted(forbidden)}")
    if set(triples) != EXPECTED_READY_TRIPLES:
        fail(
            "release-ready platform matrix must be exactly "
            f"{sorted(EXPECTED_READY_TRIPLES)}, got {sorted(triples)}"
        )

    for entry in ready:
        for field in ("triple", "rust_target", "runner", "smoke_job", "npm_package", "package_dir"):
            if not isinstance(entry.get(field), str) or not entry[field]:
                fail(f"release-ready platform {entry.get('triple')!r} lacks {field}")

    jobs = workflow_jobs(repo / ".github/workflows/release.yml")
    expected_build = {(entry["runner"], entry["rust_target"]) for entry in ready}
    for job_name, label_required in (("build-python", False), ("build-napi", True)):
        block = jobs.get(job_name)
        if block is None:
            fail(f"release workflow lacks {job_name}")
        rows = matrix_rows(job_name, block, label_required)
        actual = {(runner, target) for runner, target, _ in rows}
        if len(rows) != len(actual):
            fail(f"{job_name} matrix repeats a runner/target row")
        if actual != expected_build:
            fail(f"{job_name} runner/target coverage is {sorted(actual)}, expected {sorted(expected_build)}")
        if label_required:
            expected_napi = {(entry["runner"], entry["rust_target"], entry["triple"]) for entry in ready}
            actual_napi = set(rows)
            if len(rows) != len(actual_napi) or actual_napi != expected_napi:
                fail(f"build-napi runner/target/label coverage is {sorted(actual_napi)}, expected {sorted(expected_napi)}")

    publish_jobs: list[str] = []
    smoke_jobs: list[str] = []
    for entry in ready:
        triple = entry["triple"]
        package_dir = repo / entry["package_dir"]
        package = read_json(package_dir / "package.json")
        os_values, cpu_values, libc_values = expected_package_fields(triple)
        expected_package = {
            "name": entry["npm_package"],
            "version": release,
            "os": os_values,
            "cpu": cpu_values,
            "main": f"fathomdb.{triple}.node",
            "files": [f"fathomdb.{triple}.node"],
        }
        for field, expected in expected_package.items():
            if package.get(field) != expected:
                fail(f"{entry['package_dir']}/package.json {field!r} is {package.get(field)!r}, expected {expected!r}")
        if libc_values is None:
            if "libc" in package:
                fail(f"{entry['package_dir']}/package.json must not declare libc")
        elif package.get("libc") != libc_values:
            fail(f"{entry['package_dir']}/package.json libc is {package.get('libc')!r}, expected {libc_values!r}")

        publish_job = f"publish-npm-platform-{triple}"
        publish_block = jobs.get(publish_job)
        if publish_block is None:
            fail(f"release workflow lacks {publish_job}")
        require_runner(publish_job, publish_block, entry["runner"])
        if "all-builds-passed" not in needs(publish_job, publish_block):
            fail(f"{publish_job} must depend on all-builds-passed")
        if f"name: napi-{triple}" not in publish_block:
            fail(f"{publish_job} does not stage napi-{triple}")
        if f"working-directory: {entry['package_dir']}" not in publish_block:
            fail(f"{publish_job} does not publish {entry['package_dir']}")
        publish_jobs.append(publish_job)

        smoke_job = entry["smoke_job"]
        smoke_block = jobs.get(smoke_job)
        if smoke_block is None:
            fail(f"release workflow lacks {smoke_job}")
        require_runner(smoke_job, smoke_block, entry["runner"])
        if "publish-npm" not in needs(smoke_job, smoke_block):
            fail(f"{smoke_job} must depend on publish-npm")
        has_python_smoke = "smoke-pypi-wheel" in smoke_block or "pypi-wheel" in smoke_block
        has_npm_smoke = "smoke-npm-package" in smoke_block or "npm-package" in smoke_block
        if not (has_python_smoke and has_npm_smoke):
            fail(f"{smoke_job} must run Python-wheel and npm-package smokes")
        smoke_jobs.append(smoke_job)

    publish_main = jobs.get("publish-npm")
    if publish_main is None:
        fail("release workflow lacks publish-npm")
    if not set(publish_jobs) <= set(needs("publish-npm", publish_main)):
        fail("publish-npm must depend on every release-ready platform publish job")

    if len(smoke_jobs) != len(set(smoke_jobs)):
        fail("release-ready platforms share a smoke job")
    promotion = jobs.get("promote-npm-latest")
    if promotion is None:
        fail("release workflow lacks promote-npm-latest")
    promotion_needs = set(needs("promote-npm-latest", promotion))
    if not set(smoke_jobs) <= promotion_needs:
        fail("promote-npm-latest must depend on every release-ready platform smoke")
    if "co-tagging-assert" not in promotion_needs:
        fail("promote-npm-latest must depend on co-tagging-assert")

    print(f"ok    release-contract-truth: {release} has {len(ready)} release-ready native triples")


if __name__ == "__main__":
    main()
