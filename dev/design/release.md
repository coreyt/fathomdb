---
title: Release Subsystem Design
date: 2026-05-12
target_release: 0.6.0
desc: Version axes, tiered publish order, multi-registry publish discipline, and post-publish verification
blast_radius: release workflows; REQ-047..REQ-052; scripts/set-version.sh; .github/workflows/release.yml; dev/plans/ci-deferred.md
status: locked
---

# Release Design

This file owns release-gate mechanics: version axes, sibling-package
co-tagging, tiered publish order, registry-installed smoke verification, and
atomic completion of the multi-registry publish flow.

## Version axes (2026-05-12)

0.6.0 ships **two independent version axes**, not one global workspace
version:

- **Axis W (workspace).** Lockstep version for the runtime/binding/CLI
  surface: `fathomdb`, `fathomdb-cli`, `fathomdb-engine`,
  `fathomdb-query`, `fathomdb-schema`, `fathomdb-embedder`, plus the
  Python (`src/python/`) and TypeScript (`src/ts/`) binding packages.
- **Axis E (embedder-api).** Independent semver for
  `fathomdb-embedder-api` only. Tagged and published on every workspace
  release per REQ-048, but the embedder-api version bumps only when its
  trait surface changes.

Rationale: `fathomdb-embedder-api` is the semver-stable trait surface
per ADR-0.6.0-crate-topology (sibling-crate amendment) and
ADR-0.6.0-embedder-protocol. Lockstep-bumping it on every workspace
release would create false version churn for downstream embedder
consumers pinning the trait, and would break the version-skew-resolution
intent of REQ-047. Two axes preserve `fathomdb-embedder` /
`fathomdb-embedder-api` consumer ergonomics without giving up workspace
lockstep elsewhere.

Both axes share a single tag prefix (`v<axis-W-version>`) per release;
the embedder-api crate carries its independent `version` field in its
own `Cargo.toml`. Version-consistency checks (`set-version.sh
--check-files`) verify Axis W lockstep across all Axis-W manifests and
verify Axis E sits at its independently declared version.

## Tiered publish order

The publish order is a strict topological sort of the workspace crate
dependency graph plus the two binding packages. Index-propagation sleeps
sit between tiers (pattern from pre-0.6.0 `.github/workflows/release.yml`).

| Tier | Targets                                                      | Why                                                                                                                                                                 |
| ---- | ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| T1   | `fathomdb-embedder-api`                                      | Leaf. Axis E. Must publish first so every later crate can resolve it on crates.io.                                                                                  |
| T2   | `fathomdb-schema`                                            | Leaf (no in-workspace deps beyond external crates).                                                                                                                 |
| T3   | `fathomdb-query`                                             | Depends on `fathomdb-schema` (per ADR-0.6.0-crate-topology consequences) and `fathomdb-embedder-api` indirectly via retrieval glue.                                 |
| T4   | `fathomdb-engine`                                            | Depends on T1+T2+T3. Largest crate; publishes only after its deps are resolvable on crates.io.                                                                      |
| T5   | `fathomdb-embedder`                                          | Depends on `fathomdb-embedder-api` (Axis E). Independent of `fathomdb-engine`.                                                                                      |
| T6   | `fathomdb`                                                   | Facade. Depends on `fathomdb-engine`.                                                                                                                               |
| T7   | `fathomdb-cli`                                               | Depends on `fathomdb` facade per ADR-0.6.0-crate-topology amendment 2026-05-11.                                                                                     |
| T8   | Python wheel (`src/python/`); TypeScript package (`src/ts/`) | Both wrap `fathomdb-engine` directly (PyO3 / napi-rs); they can publish in parallel after T4 resolves on crates.io. PyPI / npm publishing is independent of T5..T7. |

Cross-ecosystem gate: every tier must succeed before the next tier
starts. `all-builds-passed` gates Tier T1 from starting until every
matrix build for the release has produced an artifact (no "publish T1
while the napi-rs prebuild is still red"). Cross-registry atomicity is
covered by the post-publish smoke step below.

Tier inter-step sleep: keep the pre-0.6.0 60-second
crates.io-index-propagation sleep between tiers. Without the sleep,
later tiers race the index and fail dependency resolution.

## Sibling-package co-tagging

Per REQ-048, `fathomdb-embedder-api` and `fathomdb-embedder` carry the
same git tag as the workspace release even though `fathomdb-embedder-api`
runs on its own version axis. The tag prefix is workspace (Axis W);
`fathomdb-embedder-api` records its independent version inside the tag
commit via its `Cargo.toml`.

## Post-publish smoke

Per `feedback_release_verification`, "green CI + published wheel" is not
done. Release-evidence sweep installs the published wheel from PyPI and
runs an end-to-end open + close + exit smoke before the release is
declared signed. Equivalent npm smoke applies for `@fathomdb/...`
publishes. Crate publishes are smoked via `cargo install fathomdb-cli`
followed by `fathomdb doctor check-integrity --json` against a fresh
fixture database.

## Sources

- `dev/plans/ci-deferred.md` enumerates the pre-0.6.0 workflows being
  restored.
- `ADR-0.6.0-crate-topology` (incl. 2026-04-27 sibling amendment + 2026-05-11
  CLI-via-facade amendment) owns the crate set.
- `ADR-0.6.0-embedder-protocol` owns the `fathomdb-embedder-api` trait
  stability posture.
- REQ-047, REQ-048, REQ-049, REQ-050, REQ-052.
