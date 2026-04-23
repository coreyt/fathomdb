# Managed vector projection — pack-by-pack handoff (2026-04-22)

This note captures the implementation state of the db-wide embedding profile
+ per-kind vector indexing work stream. Design-of-record lives in
`dev/notes/design-db-wide-embedding-per-kind-vector-indexing-2026-04-22.md`.

## Packs landed

### Pack A — schema v24
`vector_embedding_profiles` (active-singleton), `vector_index_schemas`
(per-kind enablement + state), `vector_projection_work` (durable work
queue). Migration file:
`crates/fathomdb-schema/migrations/024_vector_embedding_profiles.sql`.
Tests: `crates/fathomdb-schema/tests/migrations.rs`.

### Pack B — `AdminService::configure_embedding`
Rust admin surface; enforces identity-switch acknowledgement via
`EmbeddingChangeRequiresAck`. Callers never supply identity; the
embedder owns it.  Tests:
`crates/fathomdb-engine/tests/configure_embedding.rs`.

### Pack C — `AdminService::configure_vec_kind`
Creates `vec_<kind>` and enqueues backfill rows for every existing
chunk. Tests:
`crates/fathomdb-engine/tests/configure_vec_per_kind.rs`.

### Pack D — `VectorProjectionActor` + `drain_vector_projection`
Background actor that claims work rows, validates canonical hash +
active profile, embeds via `BatchEmbedder`, and atomically applies
results. Tests:
`crates/fathomdb-engine/tests/vector_projection_actor.rs`. Pack G adds
the profile-change discard test.

### Pack E — writer auto-enqueue
Canonical chunk writes for vector-enabled kinds + an active profile
automatically enqueue incremental (priority=1000) work rows. Tests:
`crates/fathomdb-engine/tests/write_path_vector_enqueue.rs`.

### Pack F1 — Rust `semantic_search` / `raw_vector_search`
`QueryBuilder::semantic_search(text, limit)` and
`::raw_vector_search(vec, limit)` in `fathomdb-query`. Execution in
`fathomdb-engine`'s coordinator routes via the semantic / raw vector
executors; error surface per design doc. Tests:
`crates/fathomdb/tests/semantic_search_surface.rs` (memex tripwire is
`test_semantic_search_end_to_end_memex_tripwire`).

### Pack F1.5 — admin drain uses engine's embedder
`drain_vector_projection` no longer takes an embedder argument at the
admin layer: the engine's coordinator is the sole source of truth for
identity. Preserves the vector-identity invariant.

### Pack F1.75 — compile + dispatch
`compile_query` emits optional `CompiledSemanticSearch` /
`CompiledRawVectorSearch` sidecars; the coordinator dispatcher routes
to the dedicated executors. Pack G adds the mutual-exclusion compile
guard (`CompileError::SemanticAndRawVectorSearchBothPresent`).

### Pack F2 — Python surface
`fathomdb.Query.semantic_search` / `.raw_vector_search`;
`fathomdb.Admin.configure_embedding` / `.configure_vec` /
`.drain_vector_projection` / `.get_vec_index_status`. Deprecation shim
on `vector_search(text)`. Tests: `python/tests/test_semantic_search.py`,
`test_configure_embedding.py`, `test_configure_vec_per_kind.py`.

### Pack F3 — TypeScript surface
Mirrors Python surface on `Engine`, `Query`, and `AdminClient`. Pack G
replaces the `drainVectorProjection` return type from
`Record<string, unknown>` with a typed `DrainReport` interface and
exports it from `index.ts`. Tests under
`typescript/packages/fathomdb/test/` including
`semantic_search.test.ts` and `drain_report_type.test.ts`.

### Pack G — migration, deprecation, docs (this pack)
- Compile guard:
  `compile_query` rejects an AST that carries both a
  `SemanticSearch` and a `RawVectorSearch` step with
  `CompileError::SemanticAndRawVectorSearchBothPresent`. Test at
  `crates/fathomdb-query/src/compile.rs`
  `semantic_search_and_raw_vector_search_together_rejected`.
- Profile-change discard:
  `vector_projection_actor.rs` already checks
  `claim.embedding_profile_id != active_profile_id`; new test
  `test_profile_change_discards_pending_work_under_old_profile` pins
  the invariant (`crates/fathomdb-engine/tests/vector_projection_actor.rs`).
- `VecInsert` deprecation across Rust
  (`#[deprecated]` on the engine struct; re-exports at `fathomdb-engine`
  and `fathomdb` wrap with `#[allow(deprecated)]` so external callers
  still see the warning), Python
  (`DeprecationWarning` in `VecInsert.__post_init__`; test at
  `python/tests/test_vec_insert_deprecation.py`), and TypeScript
  (JSDoc `@deprecated` on `WriteRequestBuilder.addVecInsert`).
- DrainReport typed shape + wire adapter in TypeScript
  (`typescript/packages/fathomdb/src/types.ts`; test
  `test/drain_report_type.test.ts`).
- vitest config change: a freshly-built local cdylib now takes
  priority over a main-worktree stale prebuild. Fixes the symptom
  where pack-local tests would fail under a pre-feature binary.
- CHANGELOG `[Unreleased]` entry covering the full stream.
- Memex-facing usage example at
  `dev/notes/memex-vector-integration-example-2026-04-22.md`.

## Known followups

- **Background tick activation.** `VectorProjectionActor` currently
  ticks on explicit `drain_vector_projection` calls + via an
  engine-internal `Wakeup` signal on canonical chunk writes. A
  periodic wake-up that covers idle engines between writes
  (heartbeat / interval tick) is deferred.
- **`FATHOMDB_NATIVE_BINDING` CI story.** Prebuilt `.node` binaries
  are not committed. CI jobs must either build `-p fathomdb
  --features node` or set `FATHOMDB_NATIVE_BINDING`. The pack-g
  vitest config fix makes linked-worktree runs use a freshly built
  cdylib first, so local dev works without env overrides.
- **Pack G1/G2 (design doc, optional hardening)** — stress tests for
  >10k backfill rows, observability dashboards for drain rate /
  failure distribution, and a dedicated CLI progress surface for
  `configure_vec` + drain. Not blocking release.
- **Writer docstrings.** The internal `VecInsert` path is allowed
  deprecated; once external call sites have migrated we can make the
  struct crate-private.

## Test pointers

Rust unit/integration (search by file):
- `crates/fathomdb-engine/tests/configure_embedding.rs`
- `crates/fathomdb-engine/tests/configure_vec_per_kind.rs`
- `crates/fathomdb-engine/tests/vector_projection_actor.rs`
- `crates/fathomdb-engine/tests/write_path_vector_enqueue.rs`
- `crates/fathomdb/tests/semantic_search_surface.rs`
- `crates/fathomdb/tests/semantic_search_ffi.rs`
- `crates/fathomdb-query/src/compile.rs` (tests module)

Python: `python/tests/test_configure_embedding.py`,
`test_configure_vec_per_kind.py`, `test_semantic_search.py`,
`test_vec_insert_deprecation.py`.

TypeScript: `typescript/packages/fathomdb/test/semantic_search.test.ts`,
`drain_report_type.test.ts`.

## Release readiness

Additive across the board. No canonical data change; no migration
path beyond migrating deprecated `vector_search` /
`VecInsert` usage, both of which continue to work with deprecation
warnings for one minor version.
