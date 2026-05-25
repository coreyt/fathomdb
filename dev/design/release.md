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

## RC1 bootstrap publish (2026-05-18)

`cargo publish --dry-run` resolves every in-workspace sibling dep against
crates.io (path stripping happens before resolution), so the dry-run
gate cannot succeed for T2-T7 until the registry holds the prior tier's
crate at the matching version. Pre-0.6.0 the registry was empty for
all seven crates, so the first dry-run cascade had nowhere to resolve
from.

The 0.6.0 RC slot is split: `0.6.0-rc.1` is a one-time, operator-run
bootstrap publish that seeds crates.io with registry presence for
every axis-W crate plus axis-E `fathomdb-embedder-api`. The first
"real" RC tag is `0.6.0-rc.2`; rc.1 is consumed by the bootstrap.

- Bootstrap script: `scripts/release/publish-rc1-bootstrap.sh`
  (sequential `cargo publish` of T1-T7 at `0.6.0-rc.1` with 60s
  inter-tier sleeps; idempotent — skips crates already on the
  registry at that version).
- After bootstrap, every axis-W dispatch (`0.6.0-rc.2`, `…`, GA) uses
  `cargo publish --dry-run -p <crate>` as the dry-run gate; sibling
  resolution succeeds against the rc.1 state on crates.io.
- Axis-E (`fathomdb-embedder-api`) is held on axis-W lockstep through
  the RC1 bootstrap so a single sentinel-publish establishes
  registry presence for all eight crates at once. Axis-E
  independence resumes at/after 0.6.0 GA — embedder-api may then
  bump on its own cadence when the trait surface changes.

## Pre-tag procedure (2026-05-25)

`scripts/verify-release-gates.sh:74` enforces strict equality between
the tag's bare version and `Cargo.toml [workspace.package].version`.
Every release tag (RC or GA) requires a preceding **workspace-version
bump** on `main`. The canonical sequence for either kind of tag:

1. `bash scripts/set-version.sh --workspace <target>` — bumps
   `[workspace.package].version`, every Axis-W entry in
   `[workspace.dependencies]`, `pyproject.toml`, and `package.json`
   in lockstep. Skips Axis-E (`fathomdb-embedder-api/Cargo.toml`).
   Targets: `0.6.1-rc.1`, `0.6.1-rc.2`, …, `0.6.1` (GA).
2. `cargo update --workspace` — refresh `Cargo.lock` so the in-
   workspace path-dep entries pin to the new versions. The build
   step in `release.yml` does not pass `--locked` (L179), so a
   stale lockfile would silently regenerate during CI; refreshing
   here keeps the diff atomic with the bump commit.
3. `CHANGELOG.md` — add or rename the section heading to match the
   new workspace version (e.g. `## 0.6.1-rc.1 - 2026-05-25` or
   `## 0.6.1 - 2026-05-NN`). `verify-release-gates.sh:123` matches
   on this heading.
4. `bash scripts/release/local-dry-run.sh` — runs the validatable
   subset of `release.yml` on the developer host: gates preflight,
   workspace build, leaf-crate package + dry-run publish. This is
   the primary debug loop; CI dry-run dispatch becomes a final
   confirmation.
5. Single commit on `main`: `chore(release): bump to <version>`.
6. Annotated tag at the bump commit: `git tag -a v<version>` and
   `git push origin v<version>`.

### Dependent-crate dry-run limitation (WF-FIX-2, 2026-05-25)

`cargo publish --dry-run --no-verify` for dependent crates
(`fathomdb-engine`, `fathomdb-embedder`, `fathomdb`, `fathomdb-cli`)
cannot succeed in either CI or local rehearsal. `--no-verify` skips
the verify (compile) step, **not** the package step. `cargo package`
rewrites path dependencies to versioned dependencies and resolves
them against the registry; the just-"published" sibling versions
aren't actually on crates.io during a dry-run, so the resolve fails
with `failed to select a version for the requirement
fathomdb-query = "^X.Y.Z"`.

This is the same limitation the `build-rust` job acknowledges by
packaging only leaf crates. `scripts/release/cargo-publish-if-new.sh`
short-circuits dependent crates in `--dry-run` mode with an "skipped"
diagnostic + exit 0; the real publish path is exercised by the actual
tag push. Manifest correctness for dependent crates is enforced at
real publish time inside `cargo publish`.

### Idempotency contract (WF-FIX-1, 2026-05-25)

`scripts/release/cargo-publish-if-new.sh` queries crates.io before
each tier's `cargo publish` and exits 0 if the target version is
already on the registry. This covers two scenarios:

- **Axis-W-only patch releases** (e.g. `0.6.1` while Axis-E
  `fathomdb-embedder-api` stays at `0.6.0`): T1
  short-circuits because `fathomdb-embedder-api@0.6.0` is already
  on the registry from the prior GA train.
- **Partial-republish retries**: if RC1 published T1-T3 cleanly but
  failed at T4, a follow-up RC2 with the same tags would re-attempt
  T1-T3 and now no-op cleanly.

The helper reads the local crate version from
`src/rust/crates/<crate>/Cargo.toml` (resolving `version.workspace =
true` to the root `[workspace.package]`), so it stays correct across
both axis-W bumps and axis-E independent bumps.

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
declared signed. Equivalent npm smoke applies for `fathomdb` publishes.
The npm package is published as bare `fathomdb` (not a `@fathomdb/`
scope) per 11d Blocker 7 — single brand across crates, wheel, and npm.
Crate publishes are smoked via `cargo install fathomdb-cli`
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
