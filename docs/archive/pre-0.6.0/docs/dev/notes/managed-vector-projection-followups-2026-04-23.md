# Managed vector projection — remaining followups

**Date:** 2026-04-23
**Released:** v0.5.4 (tag pushed, commit `e992243`)
**Parent release:** branch `design-db-wide-embedding-per-kind-vec` merged to `main`
**Audience:** next orchestrator / implementer picking up after 0.5.4

## Shipped in 0.5.4

Packs A → G + F1.5 + F1.75 + H + H.1 + merge/test/version fix commits. Integration merged to main and pushed as `v0.5.4`. Summary: schema v24 (plus main's concurrent v25 renumber for projection-table-registry hash naming), `configure_embedding`, `configure_vec_kind`, `configure_vec_kinds` batch, `drain_vector_projection`, `auto_drain_vector`, `VectorProjectionActor`, write-path enqueue, `semantic_search` / `raw_vector_search` across Rust + Python + TS, introspection (`capabilities`, `current_config`, `describe_kind`), compile-time mutual-exclusion guard, typed `DrainReport`, deprecations (public `VecInsert`, ambiguous `vector_search(text)`), auto-drain tracing warn, TS napi `configureEmbedding` / `configureVecKind` wrappers.

## Open followups (tracked as GitHub issues)

### #41 — Python callable embedder (`PyCallableEmbedder` via `EmbedderChoice::InProcess`)

**URL:** https://github.com/coreyt/fathomdb/issues/41
**Status:** Filed, not started.
**Priority:** Unblocks any Python workflow that can't rely on the builtin-wheel Candle embedder (test stubs, OpenAI / Cohere / local Ollama backends, custom identity).

**Scope summary:**
- New pyo3 `PyCallableEmbedder` implementing Rust `QueryEmbedder` via `Python::with_gil` callbacks.
- Python protocol: `identity()`, `embed(text)`, `max_tokens()`.
- Identity cached once at registration — preserves `project_vector_identity_invariant`.
- Reuses existing `EmbedderChoice::InProcess(Arc<dyn QueryEmbedder>)` (no new enum variant).
- New `EngineCore::open` kwarg `embedder_handle: Option<&PyAny>`; `make_embedder_choice_from_callable` factory.
- Batch via existing sequential `QueryEmbedderBatchAdapter` pattern.

**Test coverage required:** happy-path end-to-end, identity mismatch, dimension mismatch at embed time, Python exception propagation as `EmbedderError::Backend`, works on build without `default-embedder` feature.

**GIL safety:** drain actor re-acquires GIL per call (`Python::with_gil`); sequential; acceptable v1 perf.

**Why this didn't ship in 0.5.4:** out of scope. Pack H narrowly added introspection + `auto_drain_vector`; callable embedders are a separate capability.

### #42 — TypeScript callable embedder (`NodeCallableEmbedder` via `EmbedderChoice::InProcess`)

**URL:** https://github.com/coreyt/fathomdb/issues/42
**Status:** Filed, not started.
**Priority:** TS parallel of #41. Unblocks Node-side alternative embedders.

**Scope summary:**
- napi-side `NodeCallableEmbedder` using `ThreadsafeFunction` (blocking call mode) to marshal the JS callback onto the Node event loop.
- TS `Embedder` interface: `identity()` (camelCase keys), `embed(text)` (sync or async — await inside the threadsafe-function tick), `maxTokens?`.
- Identity cached at registration (same invariant as #41).
- New napi `makeEmbedderChoiceFromCallable(embedder)` + `embedderHandle` open option.
- Reentrancy audit required: if the JS callback calls back into fathomdb via napi, must not deadlock.
- **Open design question in issue:** `auto_drain_vector=true` from Node event loop + JS callback blocking-wait → deadlock. Recommend for v1: explicit error at engine open when both are combined. Document in TSDoc.

**Test coverage required:** sync `embed`, async `embed` (Promise), identity mismatch, dimension mismatch, JS exception propagation, `auto_drain_vector` + callable combination (documented error or deadlock-free), works on build without `default-embedder`.

**Why this didn't ship in 0.5.4:** out of scope; napi threadsafe-function + reentrancy design larger than Pack H's appetite. Pack H.1 filled the simpler napi gap (configure_embedding / configure_vec_kind wrappers) but left the callable path to #42.

## Non-issue followups (minor, not tracked on GitHub)

Surfaced during reviews. Low priority — wait until someone touches the adjacent code.

1. **Auto-drain warn message wording** — `auto_drain_vector_work` warn text includes `"(test-mode)"`. Misleading if someone sets the flag outside tests. Suggest rephrasing to `"best-effort drain failed"`. Reviewer nit on Pack H.1. File: `crates/fathomdb/src/lib.rs:418` (approx).
2. **`current_config` N+1 per-kind FTS schema check** — `property_schema_present` subquery inside the row-iteration loop. Harmless at typical kind counts; could be folded into the aggregation SQL if someone profiles the admin surface. File: `crates/fathomdb-engine/src/admin/introspection.rs` (approx lines 216-224).
3. **Introspection wire-parse pass-through** — TS `capabilities()`, `currentConfig()`, `describeKind()` cast JSON directly to typed shapes without `*fromWire` converters. Silent risk if the Rust side renames a snake_case field. File: `typescript/packages/fathomdb/src/admin.ts` (approx lines 691, 704, 713).
4. **`DrainReport` field-drop doc comment** — `search_rows_to_query_rows` intentionally drops `vector_distance`, `score`, `modality`, `snippet` when projecting semantic results into `QueryRows`. Worth a one-line comment stating this is by design. File: `crates/fathomdb-engine/src/coordinator.rs::search_rows_to_query_rows`.
5. **Non-partial unique index on `vector_embedding_profiles`** — Pack A shipped `idx_vep_identity ON (model_identity, model_version, dimensions)` non-partial, so demoted rows still occupy the unique key. Pack B used in-place UPDATE as a workaround. If someone revisits identity migration, convert the index to `WHERE active = 1` partial and simplify to demote+insert.
6. **TS prebuild staleness in CI** — Pack G fixed the vitest local-dev lookup order; a proper CI refresh story (automated prebuild rebuild or `FATHOMDB_NATIVE_BINDING` in CI) is still open. File: `typescript/packages/fathomdb/vitest.config.ts`, `README.md`.
7. **Background-tick activation for idle engines** — `VectorProjectionActor`'s `Wakeup` signal is `#[allow(dead_code)]`; drains only happen via explicit `drain_vector_projection` (or `auto_drain_vector` on writes). Idle engines between writes do not auto-drain. Documented in Pack D review + Pack G handoff. Activation touches the actor loop + embedder-resolver wiring.

## Referenced design docs

- `dev/notes/design-db-wide-embedding-per-kind-vector-indexing-2026-04-22.md` — parent design.
- `dev/notes/implementation-plan-db-wide-embedding-per-kind-vector-2026-04-22.md` — pack breakdown.
- `dev/notes/managed-vector-projection-handoff-2026-04-22.md` — Pack G handoff.
- `dev/notes/memex-vector-integration-example-2026-04-22.md` — Memex-facing canonical flow.
- `dev/notes/2026-04-22-auto-drain-error-tracing.md` — H.1 Part A design (shipped).
- `dev/notes/2026-04-22-typescript-configure-embedding-napi.md` — H.1 Part B design (shipped).
- Design 3 (Python callable embedder) spec lives in issue #41 body — no markdown copy committed in 0.5.4.
- TS callable embedder spec lives in issue #42 body — no markdown copy committed in 0.5.4.
