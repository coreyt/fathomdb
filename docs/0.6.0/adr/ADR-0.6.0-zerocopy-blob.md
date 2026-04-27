---
title: ADR-0.6.0-zerocopy-blob
date: 2026-04-27
target_release: 0.6.0
desc: Vector BLOB on-disk invariants — endianness, alignment, byte-length, type affinity
blast_radius: crates/fathomdb-engine vector dispatch + sqlite-vec integration; design/vector.md; design/embedder.md
status: accepted
---

# ADR-0.6.0 — Zerocopy BLOB invariants

**Status:** accepted (HITL 2026-04-27, decision-recording).

Promoted from critic-3 M-1. The zerocopy `Vec<f32>` → BLOB transfer is
mentioned in passing in ADR-0.6.0-default-embedder.md. M-1 flagged that
the on-disk shape is load-bearing for vector correctness across the
entire engine and deserves explicit pinning.

## Context

`sqlite-vec` `vec0` virtual tables expect vectors as BLOBs. The
default-embedder writes `Vec<f32>` slices directly to the BLOB column
via a zerocopy cast (e.g. `bytemuck::cast_slice` or `zerocopy`'s
`AsBytes`). No serde, no JSON, no msgpack. Speed comes at the cost of
making the binary layout part of the on-disk schema contract.

Critic noted four sub-claims that are implicit but not pinned:
endianness, alignment, byte-length validation, SQLite type affinity.

## Decision

**Vector BLOBs on disk are little-endian f32, aligned, exact byte
length, written to columns of `BLOB` affinity.**

### Five invariants

**Z-1 — Element type.** Every vector element is IEEE 754 `f32`.
NaN / +Inf / −Inf are forbidden in stored vectors (engine asserts in
debug builds; rejects in release with `EngineError::InvalidVector`).

**Z-2 — Endianness.** Vector BLOBs are little-endian. Workspace targets
all-LE today (Linux x86_64, Linux aarch64, macOS x86_64/Apple Silicon,
Windows x86_64). If a big-endian target is ever added, the engine
performs an explicit byte-swap before the cast; on disk remains LE.
Documented as a target-platform invariant; CI matrix asserts.

**Z-3 — Alignment.** Cast uses an alignment-safe path (`bytemuck::
cast_slice` for `&[f32]` → `&[u8]`, or `zerocopy::AsBytes`). No
unaligned `transmute`. Test: cast a `Vec<f32>` whose backing allocation
is at an arbitrary address; zero copies, zero unaligned reads.

**Z-4 — Byte-length validation.** Every BLOB read or write is
byte-length-checked: `bytes.len() == 4 * dimension` exactly. Mismatch
fails with `EngineError::VectorDimensionMismatch { expected, actual }`.
No silent truncation, no padding tolerance.

**Z-5 — SQLite type affinity.** Storage column declared `BLOB`
(equivalent to `vec0`-managed virtual-table column type). Inserts
that bind a non-BLOB value type fail at `prepare`/`step` time, not at
query time. Schema migration guards against accidental `TEXT` /
`NUMERIC` affinity changes.

## Options considered

**A — Pin all 5 invariants now (chosen).** Pros: makes the contract
unambiguous; survives implementer-level rewrites; future readers of
`design/vector.md` see the contract not the implementation. Cons:
none — this is documenting what every reasonable impl already does.

**B — Pin only endianness; treat the rest as implementation detail.**
Cons: byte-length validation has bitten storage engines silently
before; alignment + NaN are non-obvious failure modes worth elevating.

**C — Use a richer encoding (length prefix + magic byte).** Cons:
loses zerocopy benefit (~+8 bytes per vector × N vectors is non-trivial
at scale); needs versioned reader; adds parsing surface.

## Consequences

- `design/vector.md`: invariant section quotes Z-1..Z-5.
- `design/embedder.md`: `Embedder` impl returns `Vec<f32>`; engine
  validates Z-1 (NaN/Inf) before cast.
- `crates/fathomdb-engine` vector insert path uses
  `bytemuck::cast_slice` (or `zerocopy::AsBytes`); byte-length checked
  on both write and read.
- BE-target acceptance: not a 0.6.0 target; if added later, ADR is
  re-opened and Z-2 byte-swap step is implemented.
- Schema migration: `BLOB` affinity for any vector column is
  immutable; affinity changes require ADR amendment.

## Citations

- Critic-3 EMB-6 + M-1.
- ADR-0.6.0-default-embedder.md (zerocopy cast site).
- ADR-0.6.0-embedder-protocol.md (unit-norm invariant complements
  these byte-layout invariants).
