# Engine Features: t-038 to t-041

## t-038 — Read Path Hardening
_design-read-execution.md Phase 2_

- [x] Unit tests for `bind_value_to_sql` (Text, Integer, Bool true, Bool false)
- [x] Update `vector_read_returns_error_when_table_absent` → assert `EngineError::CapabilityMissing`
- [x] Update `coordinator_caches_by_shape_hash` → use `execute_compiled_read` (retire `dispatch_compiled_read` test usage)
- [x] Add `EngineError::CapabilityMissing(String)` variant to `lib.rs`
- [x] Store `Mutex<Connection>` in `ExecutionCoordinator`, opened once at `open()` time
- [x] Remove per-call `open_connection` + `bootstrap` from `execute_compiled_read`
- [x] Detect vec_nodes_active "no such table" error → map to `CapabilityMissing`
- [x] Remove `dispatch_compiled_read` from public API and `lib.rs` re-export
- [x] All 39 Rust tests pass (4 new bind_value unit tests added)

**Status:** done

---

## t-039 — Write Path Test Coverage + Provenance Validation
_design-typed-write.md Phase 2_

- [x] Test: `writer_fts_rows_are_written_to_database` — open DB after submit, assert fts_nodes content
- [x] Test: `writer_receipt_warns_on_nodes_without_source_ref`
- [x] Test: `writer_receipt_no_warnings_when_all_nodes_have_source_ref`
- [x] Add `provenance_warnings: Vec<String>` to `WriteReceipt`
- [x] Populate `provenance_warnings` in `apply_write()` for nodes with `source_ref: None`
- [x] Improve `prepare_write()` error message for unknown node_logical_id (v1 limitation note)
- [x] All 42 Rust tests pass

**Status:** done

---

## t-040 — Lift Chunk→Pre-Existing-Node Limitation
_design-typed-write.md Phase 2 — unresolved design question_

- [x] Test: `writer_accepts_chunk_for_pre_existing_node` — node in request 1, chunk in request 2, FTS row exists
- [x] Test: `writer_rejects_chunk_for_completely_unknown_node` — no node in request or DB → `InvalidWrite`
- [x] Move FTS row resolution into writer thread (before `BEGIN IMMEDIATE`)
- [x] `resolve_fts_rows(&conn, &mut prepared)` queries DB for node kinds not in the request
- [x] `prepare_write()` becomes a pure shape validator (no FTS derivation)
- [x] All 44 tests pass

**Status:** done

---

## t-041 — EdgeInsert + Traversal Round-Trip
_design-typed-write.md Phase 3_

- [x] Test: `writer_inserts_edge_between_two_nodes` — verify edge row in DB
- [x] Test: `writer_upsert_supersedes_prior_active_edge`
- [x] Test: `traversal_query_returns_connected_node_via_typed_writes` — full write→compile→execute round-trip
- [x] Add `EdgeInsert` struct (row_id, logical_id, source_logical_id, target_logical_id, kind, properties, source_ref, upsert)
- [x] Add `edges: Vec<EdgeInsert>` to `WriteRequest` and `PreparedWrite`
- [x] Add edge insert + upsert logic to `apply_write()`
- [x] Export `EdgeInsert` from `lib.rs`
- [x] Update `design-typed-write.md` checklist
- [x] All 47 tests pass

**Status:** done

---

## Design doc updates on completion

- `design-typed-write.md` Phase 2 + Phase 3 checkboxes
- `design-read-execution.md` Phase 2 checkboxes
