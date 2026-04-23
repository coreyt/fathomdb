# TypeScript napi wrappers for configure_embedding / configure_vec_kind

**Date:** 2026-04-22
**Status:** Design — ready to implement
**Parent release:** managed vector projection (branch `design-db-wide-embedding-per-kind-vec`)
**Targets pack:** H.2

## Context

Pack H already added `capabilities`, `currentConfig`, `describeKind`, and `configureVecKinds`
(batch) to both `crates/fathomdb/src/node.rs` (lines 688-714) and
`typescript/packages/fathomdb/src/admin.ts` (lines 703-731). Two sibling admin methods from
Pack C/B are still missing on the TS side:

- `configure_embedding` (single-call — sets the DB-wide embedding identity).
- `configure_vec_kind` (single-kind — the non-batch form).

Both exist as Rust FFI helpers: `crates/fathomdb/src/admin_ffi.rs::configure_embedding_json`
(line 371) and `configure_vec_kind_json` (line 338). Python exposes them; TS does not.

Consequence: Pack H's TS tests for the vector pipeline asserted only wire shapes and could
not drive an end-to-end `configureEmbedding → configureVecKind → write → drain →
semanticSearch` path from TS.

The `node.rs` dispatch style is uniform: every `#[napi]` method takes `String` args and
returns `Result<String>` (raw JSON on both sides). `admin.ts` uses
`parseNativeJson(callNative(() => this.#core.xxx(...)))` to deserialize.

## Design

### Rust napi additions (`crates/fathomdb/src/node.rs`)

Two new `#[napi]` methods on `EngineCore`, next to the existing `configure_vec_kinds`
(line 708):

```rust
#[napi]
pub fn configure_embedding(&self, request_json: String) -> Result<String> {
    self.with_engine(|engine| {
        crate::admin_ffi::configure_embedding_json(engine, &request_json)
            .map_err(map_admin_ffi_error)
    })
}

#[napi]
pub fn configure_vec_kind(&self, request_json: String) -> Result<String> {
    self.with_engine(|engine| {
        crate::admin_ffi::configure_vec_kind_json(engine, &request_json)
            .map_err(map_admin_ffi_error)
    })
}
```

No new Rust logic — pure delegation. The existing `admin_ffi::IdentityOnlyEmbedder` shim
at line 307-326 already preserves the identity invariant: the FFI request carries identity
fields only, and the Rust side wraps them in a `QueryEmbedder` whose `embed_query` returns
`EmbedderError::Unavailable`. This preserves the "identity belongs to the embedder"
invariant because `AdminService::configure_embedding` still reads identity off a
`QueryEmbedder` trait object.

### TS `Admin` additions (`typescript/packages/fathomdb/src/admin.ts`)

```typescript
configureEmbedding(request: ConfigureEmbeddingRequest): ConfigureEmbeddingOutcome {
  return parseNativeJson(callNative(() => this.#core.configureEmbedding(JSON.stringify(request))))
    as ConfigureEmbeddingOutcome;
}

configureVecKind(request: { kind: string; source: "chunks" }): ConfigureVecOutcome {
  return parseNativeJson(callNative(() => this.#core.configureVecKind(JSON.stringify(request))))
    as ConfigureVecOutcome;
}
```

Add the request/outcome types to `admin.ts` (or to a shared types file if the Pack H
types already live there — `ConfigureVecOutcome` already exists for `configureVecKinds`,
confirmed by the batch signature at line 727-731).

`ConfigureEmbeddingRequest` mirrors `admin_ffi::ConfigureEmbeddingRequest` (line 290):

```typescript
interface ConfigureEmbeddingRequest {
  modelIdentity: string;
  modelVersion?: string;
  dimensions: number;
  normalizationPolicy?: string;
  maxTokens?: number;            // default 512
  acknowledgeRebuildImpact?: boolean;
}
```

Note: Rust uses snake_case in the JSON envelope; convert to snake_case on `JSON.stringify` —
or add `#[serde(rename_all = "snake_case")]` confirmation on the Rust struct (already present
by default serde behavior — verify). If the TS types use camelCase, wrap in a
`configureEmbeddingRequestToWire` helper mirroring the pattern in
`operationalRegisterRequestToWire` (see admin.ts:334).

### Embedder representation on the TS side

The Rust `configure_embedding_json` request takes **identity fields only** (no embed fn).
This is intentional and aligns with the `IdentityOnlyEmbedder` shim. For this pack, TS
callers provide only the identity path:

- Write-time embedding from TS is **out of scope for this pack**. The database will accept
  vector writes that already contain vectors (caller-embedded), or the engine will drain
  with a builtin embedder if one was provided at engine-open time.
- A Node-side embedder callback (parallel to Design 3's Python shim) is deferred to a later
  pack. Tradeoff: without it, TS end-to-end tests must either (a) run against a builtin
  embedder (requires `default-embedder` feature in the napi build), or (b) provide
  pre-embedded vectors directly, or (c) accept that drain is a no-op in TS tests and assert
  only on the configuration wire path.

### Test plan

Two test tiers in `typescript/packages/fathomdb/test/` (or wherever Pack H TS tests live):

1. **Wire-shape tests (always on):** invoke `configureEmbedding` and `configureVecKind`
   with valid/invalid payloads; assert outcome shape and error propagation. No embedder
   needed.

2. **End-to-end (feature-gated):** if the napi build includes `default-embedder`, run
   `configureEmbedding → configureVecKind("chunk", "chunks") → submitWrite(chunk with
   text) → (drain or auto_drain_vector) → semanticSearch`. To make this deterministic:
   expose `auto_drain_vector` in the TS `Engine.open` surface (if not already — check
   `node.rs` `open` signature; Pack H may or may not have plumbed it. If missing, add as
   part of this pack: it's a one-line bool field on `EngineOptions` mirroring the Python
   path at python.rs:80).

## Scope guardrails

- Do NOT add a Node-callable embedder trait in this pack. That is a larger design (GIL-free
  equivalent of Design 3's pyo3 callback) and belongs in its own pack.
- Do NOT change `admin_ffi.rs`; the Rust side is already correct.
- Do NOT rename existing TS types.
- Do NOT add a `configureVecKind` variant that accepts anything other than `source: "chunks"` —
  the Rust FFI rejects other values (admin_ffi.rs:344-351).

## Followups / open questions

- Node-side embedder callback for in-TS write-time embedding — later pack.
- Should `auto_drain_vector` be TS-surface plumbed here, or in its own mini-pack? Recommend
  plumbing it here as a prerequisite for the E2E test tier.

### Critical files for implementation

- crates/fathomdb/src/node.rs
- crates/fathomdb/src/admin_ffi.rs
- typescript/packages/fathomdb/src/admin.ts
- typescript/packages/fathomdb/src/types.ts
