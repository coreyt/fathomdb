---
title: FathomDB 0.8.22 — cross-platform stable release
status: ACTIVE
target_release: 0.8.22
---

# FathomDB 0.8.22 — cross-platform stable release

## Goal and scope

0.8.22 makes native Python and npm delivery stable on Linux glibc x64/ARM64,
macOS x64/ARM64, and Windows x64. Linux musl, Windows ARM/32-bit, and all
other target triples remain unsupported.

The main `fathomdb` npm package publishes under `next` first. It is promoted
to `latest` only after every platform's actual-runner registry smoke and the
co-tagging check pass. Platform packages stay on `next`.

## Requirements and acceptance criteria

- The manifest, loader, npm metadata, and publish job agree on exactly five
  supported triples.
- The main package injects exactly one version-locked optional dependency for
  each supported platform package; unsupported hosts retain a clear error.
- Python wheels and napi binaries build and registry-smoke on actual target
  runners for every triple.
- A trusted-publishing bootstrap exists for each new unscoped npm package and
  relies on GitHub OIDC, never a long-lived npm token.
- The immutable candidate passes dry-run and full CI. A failed platform smoke
  leaves `latest` untouched and the version recoverable from its tag.

## Slice ladder

| Slice | Work | Depends on |
| ---: | --- | --- |
| 0 | Contract, acceptance, and npm OIDC bootstrap | — |
| 5 | `rusqlite` 0.40 + `sqlite-vec` 0.1.9 migration | 0 |
| 10 | Five-target platform package topology | 5 |
| 15 | Native build, validation, and wheel-size coverage | 10 |
| 20 | Ordered publish and real registry smokes | 15 |
| 25 | `next` → `latest` promotion and release truth | 20 |

## Landed release state

<!-- BEGIN GENERATED release-state:0.8.22:plan-landed-roll-up -->
**LANDED on `origin/main`, in full:** no slices. SCHEMA is 25; remaining ladder = 0 → 5 → 10 → 15 → 20 → 25.<!-- END GENERATED release-state:0.8.22:plan-landed-roll-up -->

## Reserved-gap policy

No reserved-gap work is authorized by this release plan. Unsupported targets
remain explicitly unsupported rather than implied future package promises.

## Cross-cutting DoD

Every implemented slice must retain the five-target capability truth across
the manifest, native artifact, package metadata, loader, actual-runner smoke,
and public documentation. Changes to public API or error taxonomy require their
own governing decision.

## Publish authority

Tagging and publication require normal explicit HITL authorization. This plan
prepares and verifies the release path; it does not authorize a registry write.

## Immediate next slice

Land and review the prerequisite candidate work against `origin/main`, then
run the non-publishing immutable-SHA release dry-run.
