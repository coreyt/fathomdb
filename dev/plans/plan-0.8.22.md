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

It also makes ranked retrieval cardinality explicit before publication. The
default result count is 10 and the validated maximum is 100 for the direct
ranked-search families. This closes the existing unbounded FTS result paths and
makes EARP's `K = 20` and `K = 50` measurements accessible through public SDKs.

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
- Ranked `search`, `search_text_only`, and `search_projected_text` default to
  10 results, accept a caller-selected result limit through 100, and reject an
  out-of-range request rather than silently clamping it.
- `search_expand.search_hits` uses the same `search_limit` contract; graph
  expansion retains its separate 50-per-root traversal cap.

## Slice ladder

| Slice | Work | Depends on |
| ---: | --- | --- |
| 0 | Contract, acceptance, and npm OIDC bootstrap | — |
| 5 | `rusqlite` 0.40 + `sqlite-vec` 0.1.9 migration | 0 |
| 10 | Five-target platform package topology | 5 |
| 12 | Current-authority and document-debt inventory | 10 |
| 17 | Pre-registered 0.8.23 scale-measurement protocol (no run) | 5, 12 |
| 15 | Native build, validation, and wheel-size coverage | 10 |
| 18 | Ranked retrieval result limits and SDK parity | 15 |
| 20 | Ordered publish and real registry smokes | 15, 18 |
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

### Slice 18 — RETRIEVAL-LIMITS

Implement the accepted ranked-result contract in
`dev/design/retrieval-result-limits.md`: default to 10 hits, accept a requested
limit through 100, and reject invalid requests rather than silently clamping
them. The scope is hybrid search, text-only search, projected-text search, and
the initial `search_hits` result of `search_expand`, across Rust, Python, and
TypeScript.

The slice must prove the default, `K = 5/20/50`, the maximum, and invalid
boundary behavior on all three layers. It must also prove that vector rerank
depth follows the requested K, and that FTS/filter ordering cannot return fewer
valid hits because filtered candidates consumed the limit. Traversal expansion
and enumerative read limits are expressly out of scope: `graph_neighbors` keeps
its current 50-per-root cap, and `read_list`/operational-log pagination retain
their own policies.

## Landed release state

<!-- BEGIN GENERATED release-state:0.8.22:plan-landed-roll-up -->
**LANDED on `origin/main`, in full:** Slices 0 (`55792858b2adce00d3d87193d02b23a5d8d52dd7`) · 5 (`55792858b2adce00d3d87193d02b23a5d8d52dd7`) · 10 (`4c7bb26b`) · 12 (`72a83049`) · 15 (`13341688fca3d02d11c10bb10eb26232156f8032`) · 17 (`5a7f2484`). SCHEMA is 25; remaining ladder = 18 → 20 → 25.<!-- END GENERATED release-state:0.8.22:plan-landed-roll-up -->

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
**IMMEDIATE NEXT: Slice 18** (`RETRIEVAL-LIMITS`) — ranked retrieval result limits and SDK parity

**Remaining ladder:** 18 → 20 → 25.<!-- END GENERATED release-state:0.8.22:plan-immediate-next -->
