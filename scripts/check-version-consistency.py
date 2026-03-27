#!/usr/bin/env python3

from __future__ import annotations

import argparse
import pathlib
import sys
import tomllib


def read_toml(path: pathlib.Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Verify that Cargo workspace and Python package versions stay aligned."
    )
    parser.add_argument(
        "--tag",
        help="Optional release tag to validate against, e.g. v0.1.0",
    )
    args = parser.parse_args()

    repo_root = pathlib.Path(__file__).resolve().parent.parent
    cargo = read_toml(repo_root / "Cargo.toml")
    pyproject = read_toml(repo_root / "python" / "pyproject.toml")

    cargo_version = cargo["workspace"]["package"]["version"]
    python_version = pyproject["project"]["version"]

    if cargo_version != python_version:
        print(
            f"version mismatch: Cargo.toml={cargo_version} python/pyproject.toml={python_version}",
            file=sys.stderr,
        )
        return 1

    if args.tag:
        expected_tag = f"v{cargo_version}"
        if args.tag != expected_tag:
            print(
                f"tag/version mismatch: tag={args.tag} expected={expected_tag}",
                file=sys.stderr,
            )
            return 1

    print(f"version check passed: {cargo_version}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
