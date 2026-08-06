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

This release also prepares the next scale-bound release without making a
scale claim: it records the current documentation authority/debt inventory
and pre-registers the future measurement protocol. Neither preparatory slice
executes a scale run, changes a supported-scale statement, nor authorizes a
publication.

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
- A two-phase `repo-prune` classification records current authorities and
  document debt without deleting, moving, or silently rewriting historical
  records.
- The 0.8.23 scale-bound measurement protocol is pre-registered with fixture
  identity, dependency/toolchain/hardware capture, repetitions, metrics, and
  result-artifact schema; it explicitly records that no scale measurement or
  supported-scale claim has been made.

## Slice ladder

| Slice | Work | Depends on |
| ---: | --- | --- |
| 0 | Contract, acceptance, and npm OIDC bootstrap | — |
| 5 | `rusqlite` 0.40 + `sqlite-vec` 0.1.9 migration | 0 |
| 10 | Five-target platform package topology | 5 |
| 12 | Current-authority and document-debt inventory | 10 |
| 17 | Pre-registered 0.8.23 scale-measurement protocol (no run) | 5, 12 |
| 15 | Native build, validation, and wheel-size coverage | 10 |
| 20 | Ordered publish and real registry smokes | 15 |
| 25 | `next` → `latest` promotion and release truth | 20 |

### Slice 12 — DOC-BASELINE

Run the existing two-phase `repo-prune` classifier against the candidate and
commit a bounded inventory of current authority, historical records, and
document debt. The inventory must identify each proposed follow-up's owner and
release home. Historical records stay in place; this slice neither deletes nor
moves documents, rewrites their content, or broadens Markdown/link validation.

Acceptance is a reproducible classifier command, a versioned inventory with
source revision/provenance, and a reviewable distinction between current
authority, historical record, and unresolved debt. It must be useful to the
0.8.23 architecture/contract-baseline work without pre-deciding that work.

### Slice 17 — SCALE-PROTOCOL

Pre-register, but do not execute, the 0.8.23 supported-scale characterization.
The protocol must pin the candidate revision and corpus/fixture identity;
record dependency-lock, schema, toolchain, CPU/GPU, and host-capacity evidence;
specify repetitions, warm/cold treatment, metrics and percentile summaries;
and define the result-artifact schema and interpretation rules.

It must state that the protocol has no measured result, creates no supported
scale limit, and cannot satisfy either the 0.8.23 advisory or 0.8.24 firm
scale-bound outcome. Execution remains after the complete 0.8.22 dependency
stack, including the coupled vector migration.

## Landed release state

<!-- BEGIN GENERATED release-state:0.8.22:plan-landed-roll-up -->
**LANDED on `origin/main`, in full:** Slices 0 (`55792858b2adce00d3d87193d02b23a5d8d52dd7`) · 5 (`55792858b2adce00d3d87193d02b23a5d8d52dd7`) · 10 (`4c7bb26b`) · 12 (`72a83049`). SCHEMA is 25; remaining ladder = 17 → 15 → 20 → 25.<!-- END GENERATED release-state:0.8.22:plan-landed-roll-up -->

## Reserved-gap policy

Slices 12 and 17 are authorized reserved-gap preparatory work: they establish
the current documentation baseline and the future scale-measurement protocol.
They do not widen the supported target matrix, imply future package promises,
or authorize a scale measurement or publication. Unsupported targets remain
explicitly unsupported.

## Cross-cutting DoD

Every implemented slice must retain the five-target capability truth across
the manifest, native artifact, package metadata, loader, actual-runner smoke,
and public documentation. Changes to public API or error taxonomy require their
own governing decision.

## Publish authority

Tagging and publication require normal explicit HITL authorization. This plan
prepares and verifies the release path; it does not authorize a registry write.

## Immediate next slice

<!-- BEGIN GENERATED release-state:0.8.22:plan-immediate-next -->
**IMMEDIATE NEXT: Slice 17** (`SCALE-PROTOCOL`) — pre-register the 0.8.23 scale-bound measurement protocol without running it

**Remaining ladder:** 17 → 15 → 20 → 25.<!-- END GENERATED release-state:0.8.22:plan-immediate-next -->
