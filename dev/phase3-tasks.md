# Phase 3 Task List

## Item 1: sqlite-vec Feature Flag + Extension Loading

- [x] Add `sqlite-vec = { version = "0.1", optional = true }` to workspace `Cargo.toml`
- [x] Add `[features] sqlite-vec = ["dep:sqlite-vec"]` to `crates/fathomdb-engine/Cargo.toml`
- [x] Add `"load_extension"` to rusqlite features in workspace `Cargo.toml`
- [x] Add `open_connection_with_vec()` to `crates/fathomdb-engine/src/sqlite.rs` (cfg-gated)
- [x] Add `vector_dimension: Option<usize>` to `EngineOptions` in `crates/fathomdb/src/lib.rs`
- [x] Add `vector_enabled: bool` to `ExecutionCoordinator`; expose via `vector_enabled()`
- [x] Thread vector dimension through `Engine::open` → `EngineRuntime::open` → coordinator
- [x] Tests: `capability_gate_reports_false_without_feature`, `capability_gate_reports_true_when_feature_enabled`

## Item 2: Virtual Table Schema + `ensure_vector_profile()`

- [x] Replace stub in `crates/fathomdb-schema/src/bootstrap.rs`: implement `ensure_vector_profile()` with `#[cfg(feature = "sqlite-vec")]` guard
- [x] Update `BootstrapReport.vector_profile_enabled`: query `vector_profiles WHERE enabled = 1` after bootstrap
- [x] Call `ensure_vector_profile()` in `EngineRuntime::open()` when `vector_dimension` is `Some`
- [x] Add `sqlite-vec` feature to `crates/fathomdb-schema/Cargo.toml`
- [x] Tests: `vector_profile_not_enabled_without_feature`, `vector_profile_created_when_feature_enabled`, `vector_profile_skipped_when_dimension_absent`, `bootstrap_report_reflects_actual_vector_state`

## Item 3: `VecInsert` Write Path

- [x] Add `VecInsert { chunk_id: String, embedding: Vec<f32> }` to `crates/fathomdb-engine/src/writer.rs`
- [x] Add `vec_inserts: Vec<VecInsert>` to `WriteRequest` and `PreparedWrite`
- [x] Wire `apply_write()` to INSERT into `vec_nodes_active` when feature enabled (cfg-gated block)
- [x] Add validation in `prepare_write()`: reject empty `chunk_id` or empty `embedding`
- [x] Re-export `VecInsert` from engine lib and public facade
- [x] Tests: `vec_insert_noop_without_feature`, `vec_insert_empty_chunk_id_is_rejected`, `vec_insert_empty_embedding_is_rejected`
- [x] Test (cfg-gated): `vec_insert_is_persisted_when_feature_enabled`

## Item 4: `QueryRows` Runtime Table Extension

- [x] Add `RunRow`, `StepRow`, `ActionRow` structs to `crates/fathomdb-engine/src/coordinator.rs`
- [x] Extend `QueryRows` with `runs: Vec<RunRow>`, `steps: Vec<StepRow>`, `actions: Vec<ActionRow>`
- [x] Add `read_run(&str)`, `read_step(&str)`, `read_action(&str)`, `read_active_runs()` to `ExecutionCoordinator`
- [x] Re-export new types from engine lib and public facade
- [x] Update `QueryRows::default()` to include empty vecs for new fields
- [x] Tests: `read_run_returns_inserted_run`, `read_step_returns_inserted_step`, `read_action_returns_inserted_action`, `read_active_runs_excludes_superseded`

## Item 5: Client Workload Tests (22 + 1)

- [x] Add `run_count()`, `step_count()`, `action_count()` to `crates/fathomdb/tests/helpers.rs`
- [x] M-1: Meeting transcript ingestion
- [x] M-2: Meeting note correction via upsert
- [x] M-3: FTS search on transcripts
- [x] M-4: History preservation after upsert
- [x] M-5: Excise by source_ref
- [x] M-6: FTS rebuild after deletion
- [x] OC-1: Persist and retrieve agent context
- [x] OC-2: Context versioning via supersession
- [x] OC-3: Write provenance run/step/action records
- [x] OC-4: Traverse task dependency graph
- [x] OC-5: Edge retire removes from traversal
- [x] OC-6: Check semantics clean after workload
- [x] HC-1: Self-evaluation node round trip
- [x] HC-2: Evaluation update with supersession chain
- [x] HC-3: Excise flagged evaluation
- [x] HC-4: Projection rebuild after data loss
- [x] HC-5: FTS search after rebuild
- [x] NC-1: Bulk-ingest documents (50 nodes + chunks)
- [x] NC-2: FTS search on bulk documents
- [x] NC-3: Excise by source_ref (10 nodes)
- [x] NC-4: Safe export manifest completeness
- [x] NC-5: Check integrity after enterprise workload
- [x] V-1: Vector search round trip (`#[cfg(feature = "sqlite-vec")]`)

## Item 6: Checklist + Doc Updates

- [x] Mark Phase 3 items done in `dev/phase3-tasks.md`
- [ ] Mark Phase 3 items done in `dev/design-typed-write.md`
- [ ] Mark Phase 3 items done in `dev/design-read-execution.md`
- [ ] Update `dev/setup-sqlite-vec-capability.md` with decisions made

## Verification

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
# with extension (when available):
cargo test --workspace --features sqlite-vec
```
