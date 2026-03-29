# Release Policy

## Purpose

Define the public release contract for `fathomdb`.

## Version Source Of Truth

`fathomdb` uses a unified version across the public Rust and Python surfaces.

- Rust source of truth: `Cargo.toml` `workspace.package.version`
- Python source of truth: `python/pyproject.toml` `project.version`
- Release tag format: `vX.Y.Z`

These values must remain aligned. `scripts/check-version-consistency.py`
enforces:

- Cargo version matches Python version
- release tag matches both versions

## Public Release Artifacts

A production release consists of:

1. a GitHub Release for the version tag
2. a PyPI publish of the `fathomdb` Python package
3. a crates.io publish of the `fathomdb` Rust crate

## Release Gates

Before publishing a release tag:

1. the release verification workflow must pass
2. baseline CI must be green for the tagged commit
3. Python CI must be green for the tagged commit
4. `Benchmark And Robustness` must have a successful run on `main` within the
   last 10 days
5. `scripts/check-version-consistency.py --tag vX.Y.Z` must pass

## Release Workflow Shape

- baseline CI remains separate from release publishing
- release publishing is tag-driven
- `verify-release` enforces the CI, Python, benchmark freshness, and version
  consistency gates before any artifact publish job starts
- GitHub Release is created only after PyPI and crates.io publication succeed

## Manual Fallback

If automated publishing is unavailable, do not publish partial artifacts.
Either restore the automation path or postpone the release. GitHub-only source
releases do not satisfy the public release contract defined here.
