---
title: Errors Subsystem Design
date: 2026-04-30
target_release: 0.6.0
desc: Canonical error-type ownership, binding mapping inputs, and corruption detail host file
blast_radius: fathomdb-engine errors; bindings; acceptance AC-035* and AC-060*
status: draft
---

# Errors Design

This file is the design owner named by `architecture.md` for the cross-cutting
error surface.

## Top-level types

- `EngineError` owns post-open runtime failures returned by `write`, `search`,
  `close`, scheduler callbacks, and op-store validation on accepted 0.6.0
  write paths.
- `EngineOpenError` owns `Engine.open` failures, including lock contention,
  incompatible schema, embedder-identity mismatch, and corruption-on-open.

Bindings map these roots into language-idiomatic class hierarchies per
`design/bindings.md`.

Top-level ownership boundary:

- `design/engine.md` owns when open-path stages produce `EngineOpenError`.
- Subsystem design docs own the semantics of their module errors.
- This file owns which module errors stay distinct, which top-level root they
  route through, and the stable machine-readable payload fields bindings may
  depend on.

## Module taxonomy

Per ADR-0.6.0-error-taxonomy, per-module errors stay distinct when they carry
different remediation or cross-doc ownership.

| Error type | Produced by | Routed through | Semantics owner | Why distinct |
|---|---|---|---|---|
| `StorageError` | canonical SQLite read/write path | `EngineError` | `design/engine.md` | physical storage / transaction failures are not projection or op-store failures |
| `ProjectionError` | projection-row commit / terminal-state accounting | `EngineError` | `design/projections.md` | projection freshness and failure-state rules are distinct from canonical writes |
| `VectorError` | `sqlite-vec` encode/load/query path | `EngineError` | `design/vector.md` | vector capability / encoding failures have vector-specific recovery |
| `EmbedderError` | embedder dispatch, timeout, invalid vector return | `EngineError` | `design/embedder.md` | caller remediation is "fix or replace embedder," not "retry generic write" |
| `SchedulerError` | scheduler startup/shutdown / queue orchestration | `EngineError` | `design/scheduler.md` | queue and shutdown failures are not vector math or write-shape failures |
| `OpStoreError` | unknown collection, kind mismatch, registry misuse | `EngineError` | `design/op-store.md` | op-store contract failures are separate from primary graph writes |
| `WriteValidationError` | malformed typed write shape | `EngineError` | `design/engine.md` | fix caller-submitted field shape / variant construction |
| `SchemaValidationError` | JSON Schema rejection for op-store payloads | `EngineError` | `design/op-store.md` | fix payload contents against registered `schema_id` |
| `EmbedderIdentityMismatchError` | open-time stored-vs-supplied identity comparison | `EngineOpenError` | `design/embedder.md` | open-time incompatibility, not runtime write/query failure |
| `MigrationError` | schema migration execution | `EngineOpenError` | `design/migrations.md` | open-time schema transition failure with per-step reporting |

`Overloaded` and `Closing` remain direct `EngineError` variants rather than
module errors because they are cross-cutting runtime states:

- `OverloadedError`
- `ClosingError`

This file is the canonical home for the variant-to-binding mapping inputs and
the reason the named error modules exist at all.

## Validation boundary

The validation split is load-bearing and must not be collapsed in owner docs or
bindings:

- `WriteValidationError` means the submitted typed write is malformed before
  schema-sensitive payload checks run.
- `SchemaValidationError` means the op-store payload is structurally valid JSON
  but fails the registered `schema_id` JSON Schema at save time.
- `EmbedderIdentityMismatchError` is not a write-time validation at all; it is
  an open-time compatibility failure.

`design/engine.md`, `design/op-store.md`, `design/bindings.md`, and
`acceptance.md` must preserve this split.

## Binding mapping ownership

This file owns the stable inputs bindings map from:

- top-level root (`EngineError` vs `EngineOpenError`)
- module / direct variant identity
- stable machine payload fields

`design/bindings.md` owns the mapping protocol:

- one class per variant
- single rooted hierarchy per binding
- typed attributes rather than message parsing

`interfaces/{python,ts,cli}.md` own idiomatic casing and concrete class names.

## Corruption detail owner

This file is the canonical host for the `CorruptionDetail` payload contract
carried by `EngineOpenError::Corruption`:

- `CorruptionKind`
- `OpenStage`
- `CorruptionLocator`
- `RecoveryHint.code`
- `RecoveryHint.doc_anchor`

`design/engine.md` and `design/recovery.md` cite these rows by stable code and
must not redeclare the same join in parallel.

### Surface split

Two machine-readable surfaces exist and are intentionally not the same thing:

- `CorruptionKind` + `OpenStage` are the structured `Engine.open` error surface.
- `code` is the stable report / dispatch surface used by bindings and doctor
  output.

Doctor finding codes are not required to equal the `Engine.open`
`CorruptionKind` set. In particular, `E_CORRUPT_INTEGRITY_CHECK` is a
doctor-report code for `doctor check-integrity --full`, not an `Engine.open`
corruption kind.

### `Engine.open` corruption table

`Engine.open` in 0.6.0 exposes exactly four corruption-emitting stages and four
open-path corruption kinds.

| `OpenStage` | `CorruptionKind` | Typical `CorruptionLocator` | `RecoveryHint.code` | `RecoveryHint.doc_anchor` |
|---|---|---|---|---|
| `WalReplay` | `WalReplayFailure` | `PageId { page: u32 }`, `FileOffset { offset: u64 }`, `OpaqueSqliteError { sqlite_extended_code: i32 }` | `E_CORRUPT_WAL_REPLAY` | `design/recovery.md#wal-replay-failures` |
| `HeaderProbe` | `HeaderMalformed` | `FileOffset { offset: u64 }`, `OpaqueSqliteError { sqlite_extended_code: i32 }` | `E_CORRUPT_HEADER` | `design/recovery.md#header-malformed` |
| `SchemaProbe` | `SchemaInconsistent` | `TableRow { table: &'static str, rowid: i64 }`, `MigrationStep { from: u32, to: u32 }`, `OpaqueSqliteError { sqlite_extended_code: i32 }` | `E_CORRUPT_SCHEMA` | `design/recovery.md#schema-inconsistent` |
| `EmbedderIdentity` | `EmbedderIdentityDrift` | `TableRow { table: &'static str, rowid: i64 }`, `OpaqueSqliteError { sqlite_extended_code: i32 }` | `E_CORRUPT_EMBEDDER_IDENTITY` | `design/recovery.md#embedder-identity-drift` |

The table above is the only canonical materialized join for the open-path
corruption contract in 0.6.0.

### `CorruptionLocator` ownership

`CorruptionLocator` keeps the broader locator enum even though `Engine.open`
uses only a subset of variants today. Every variant remains justified:

| `CorruptionLocator` | Why it exists in 0.6.0 |
|---|---|
| `FileOffset { offset: u64 }` | Header/page-byte diagnosis needs a byte-position locator that survives even when no logical row can be decoded. |
| `PageId { page: u32 }` | WAL replay and page-level diagnosis still produce page ids; this remains justified even after integrity-check removal from the open path. |
| `TableRow { table: &'static str, rowid: i64 }` | Schema and embedder-profile failures may be row-addressable even when the file is otherwise readable. |
| `Vec0ShadowRow { partition: &'static str, rowid: i64 }` | Doctor / recovery diagnostics may need to point at sqlite-vec shadow rows, which are not user-named tables. |
| `MigrationStep { from: u32, to: u32 }` | Some failures are best localized to a migration edge rather than a page or row. |
| `OpaqueSqliteError { sqlite_extended_code: i32 }` | Required fallback when SQLite surfaces corruption without a usable structured locator; replaces any forbidden `Unspecified` escape hatch. |

### Doctor-only finding codes

Doctor/report codes share the same stable `code` dispatch surface, but they are
not constrained to map 1:1 to open-path enums.

| `code` | Surface | Meaning | `doc_anchor` |
|---|---|---|---|
| `E_CORRUPT_INTEGRITY_CHECK` | `doctor check-integrity --full` finding | Page-damage finding emitted by dedicated full-integrity diagnosis; not returned from `Engine.open` | `design/recovery.md#integrity-check-full-findings` |

## Foreign-cause sanitization

When a module wraps a foreign cause (`rusqlite::Error`, `io::Error`,
`serde_json::Error`), the module doc owns the semantic category while this file
owns the shared sanitization rule:

- `Display` is safe for callers and operators: no raw SQL text, absolute host
  paths, or parser byte offsets as the primary message.
- Full foreign cause chains remain available to engine-internal logging and
  debug builds.
- Bindings do not flatten the sanitized type back into a generic string error.

This keeps module attribution visible without turning foreign dependency message
formats into public contract.
