  Current state vs. design doc checklists:

  ┌─────────────────────────────────────────────────────────────┬──────────────────────────────────────────────────────────┐
  │                            Item                             │                          Status                          │
  ├─────────────────────────────────────────────────────────────┼──────────────────────────────────────────────────────────┤
  │ Typed write structs (NodeInsert, ChunkInsert,               │ ✅ done                                                  │
  │ Run/Step/ActionInsert)                                      │                                                          │
  ├─────────────────────────────────────────────────────────────┼──────────────────────────────────────────────────────────┤
  │ prepare_write() + PreparedWrite split                       │ ✅ done                                                  │
  ├─────────────────────────────────────────────────────────────┼──────────────────────────────────────────────────────────┤
  │ FTS derivation from chunks in same request                  │ ✅ done                                                  │
  ├─────────────────────────────────────────────────────────────┼──────────────────────────────────────────────────────────┤
  │ execute_compiled_read() + shape-hash cache                  │ ✅ done                                                  │
  ├─────────────────────────────────────────────────────────────┼──────────────────────────────────────────────────────────┤
  │ NodeRow, QueryRows result types                             │ ✅ done                                                  │
  ├─────────────────────────────────────────────────────────────┼──────────────────────────────────────────────────────────┤
  │ bind_value_to_sql() adapter                                 │ ✅ done — no unit tests                                  │
  ├─────────────────────────────────────────────────────────────┼──────────────────────────────────────────────────────────┤
  │ EngineRuntime façade                                        │ ✅ done                                                  │
  ├─────────────────────────────────────────────────────────────┼──────────────────────────────────────────────────────────┤
  │ Vector execution fails when table absent                    │ ✅ done — error is opaque rusqlite, not explicit         │
  ├─────────────────────────────────────────────────────────────┼──────────────────────────────────────────────────────────┤
  │ execute_compiled_read opens new connection per call         │ ❌ Phase 2 bug                                           │
  ├─────────────────────────────────────────────────────────────┼──────────────────────────────────────────────────────────┤
  │ dispatch_compiled_read() is vestigial                       │ ❌ was never retired                                     │
  ├─────────────────────────────────────────────────────────────┼──────────────────────────────────────────────────────────┤
  │ FTS projection test verifies nothing in DB                  │ ❌ test only checks receipt.label                        │
  ├─────────────────────────────────────────────────────────────┼──────────────────────────────────────────────────────────┤
  │ Chunks cannot reference pre-existing nodes                  │ ❌ documented Phase 1 limit, Phase 2 should lift it      │
  ├─────────────────────────────────────────────────────────────┼──────────────────────────────────────────────────────────┤
  │ EdgeInsert missing                                          │ ❌ compiler has traversal JOINs on edges; no typed write │
  │                                                             │  path exists                                             │
  └─────────────────────────────────────────────────────────────┴──────────────────────────────────────────────────────────┘

  ---
  Plan: Engine Features — Phase 2 + Phase 3

  Four tasks in dependency order. t-038 and t-039 are independent and can be worked in parallel; t-040 depends on t-039's
  writer refactor; t-041 depends on t-040 (uses EngineRuntime for the round-trip test).

  ---
  t-038 — Read Path Hardening

  Goals (design-read-execution.md Phase 2):
  - Persistent reader connection in ExecutionCoordinator
  - Retire dispatch_compiled_read() from the public API
  - Explicit EngineError::CapabilityMissing for vector reads without sqlite-vec
  - Unit tests for bind_value_to_sql

  Design decisions:

  Connection model: Store Mutex<rusqlite::Connection> in ExecutionCoordinator, opened once at open() time with
  initialize_connection() already called. execute_compiled_read() acquires the lock and uses this connection — no
  open_connection() per call, no per-call bootstrap().

  Statement cache: The statement_cache: Mutex<HashMap<ShapeHash, String>> stays for the cached_statement_count() diagnostic.
  The real statement cache is rusqlite's prepare_cached() operating on the persistent connection.

  dispatch_compiled_read(): Remove from the public API and from lib.rs. The one test that uses it
  (coordinator_caches_by_shape_hash) is replaced with an equivalent using execute_compiled_read().

  Capability error: Add EngineError::CapabilityMissing(String) variant. In execute_compiled_read(), catch rusqlite errors whose
   message contains "no such table: vec_nodes_active" and remap to CapabilityMissing. Update
  vector_read_returns_error_when_table_absent to assert the specific variant.

  Tests to write first:
  1. bind_value_to_sql_text_produces_text_value
  2. bind_value_to_sql_integer_produces_integer_value
  3. bind_value_to_sql_bool_true_produces_one
  4. bind_value_to_sql_bool_false_produces_zero
  5. Update vector_read_returns_error_when_table_absent → assert EngineError::CapabilityMissing
  6. Update coordinator_caches_by_shape_hash → use execute_compiled_read instead of dispatch_compiled_read

  Files: crates/fathomdb-engine/src/coordinator.rs, crates/fathomdb-engine/src/lib.rs

  ---
  t-039 — Write Path Test Coverage + Provenance Validation

  Goals (design-typed-write.md Phase 2):
  - Prove FTS rows actually land in the database (not just a receipt label check)
  - Make provenance gaps observable without blocking writes
  - Improve the chunk-references-unknown-node error message

  Design decisions:

  FTS verification: The existing writer_executes_typed_nodes_chunks_and_derived_projections test is enhanced — after submit(),
  open the DB directly and assert fts_nodes row count, chunk_id, node_logical_id, kind, and text_content.

  Provenance policy: source_ref stays Option<String> — hard enforcement is out of scope. Add provenance_warnings: Vec<String>
  to WriteReceipt. In apply_write(), after inserting nodes, collect warnings for any node with source_ref: None. This makes
  provenance gaps observable without breaking existing callers (the field defaults to empty).

  Error message: Improve the prepare_write() error for unknown node_logical_id to say it is a v1 limitation, pointing toward
  co-submitting the node or using upsert.

  Tests to write first:
  1. writer_fts_rows_are_written_to_database — after submit, open DB, assert fts_nodes content
  2. writer_receipt_warns_on_nodes_without_source_ref — submit NodeInsert with source_ref: None, assert
  receipt.provenance_warnings is non-empty
  3. writer_receipt_no_warnings_when_all_nodes_have_source_ref — all nodes have source_ref, warnings empty

  Files: crates/fathomdb-engine/src/writer.rs

  ---
  t-040 — Lift Chunk→Pre-Existing-Node Limitation

  Goals (design-typed-write.md Phase 2 — the remaining unresolved design question):
  - Allow ChunkInsert to reference a node that already exists in the database but is not in the current WriteRequest

  Design decision:

  The pure prepare_write() in submit() stays as a shape validator. The FTS derivation step moves into the writer thread itself,
   running before BEGIN IMMEDIATE. This preserves the "heavy work outside the locked transaction" principle:

  submit():
    validate_write_shape(request)    ← pure, stays in submit()
    send to writer thread

  writer loop:
    receive message
    resolve_fts_rows(&conn, &mut prepared)  ← new: reads DB for unknown node kinds
    apply_write(&mut conn, &prepared)        ← BEGIN IMMEDIATE, unchanged

  resolve_fts_rows() does SELECT kind FROM nodes WHERE logical_id = ?1 AND superseded_at IS NULL for any chunk whose
  node_logical_id wasn't in the same request's nodes. If the node still can't be found in either place, the write is rejected
  with a clear error.

  Tests to write first:
  1. writer_accepts_chunk_for_pre_existing_node — write node in request 1, write chunk for that node in request 2, assert FTS
  row exists
  2. writer_rejects_chunk_for_completely_unknown_node — no node in request or DB, expect EngineError::InvalidWrite

  Files: crates/fathomdb-engine/src/writer.rs

  ---
  t-041 — EdgeInsert + Traversal Round-Trip

  Goals (design-typed-write.md Phase 3 — closes the write/read coherence gap):
  - Typed edge writes through the same WriterActor path
  - End-to-end traversal test: write nodes + edge, compile traversal query, execute read, assert results

  Design decision:

  EdgeInsert mirrors NodeInsert with the schema's actual fields (confirmed from bootstrap.rs):

  pub struct EdgeInsert {
      pub row_id: String,
      pub logical_id: String,
      pub source_logical_id: String,
      pub target_logical_id: String,
      pub kind: String,
      pub properties: String,
      pub source_ref: Option<String>,
      pub upsert: bool,   // supersedes the active edge with this logical_id
  }

  edges has logical_id (the unique row identity for supersession), like nodes. The upsert flag supersedes WHERE logical_id = ?1
   AND superseded_at IS NULL before inserting the new row, matching node supersession behavior.

  Tests to write first:
  1. writer_inserts_edge_between_two_nodes — writes two nodes + edge, opens DB, verifies edge row
  2. writer_upsert_supersedes_prior_active_edge — writes edge v1 then edge v2 with upsert, asserts only v2 active
  3. traversal_query_returns_connected_node_via_typed_writes — full round-trip: write node A, node B, edge A→B via WriterActor;
   compile QueryBuilder::nodes("Meeting").text_search("...", 5).traverse(Out, "HAS_TASK", 1); execute via ExecutionCoordinator;
   assert node B is in results

  The third test is the critical coherence test that proves the compiler and writer are operating on the same data.

  Files: crates/fathomdb-engine/src/writer.rs, crates/fathomdb-engine/src/lib.rs

  ---
  Task ordering

  t-038 (read hardening) ──────────────────────────────────────┐
                                                                ▼
  t-039 (write test coverage + provenance) ──► t-040 (lift chunk limit) ──► t-041 (EdgeInsert + traversal)

  t-038 and t-039 are independent. t-040 requires t-039's writer refactor. t-041 requires t-040 (uses EngineRuntime for the
  round-trip test and needs the write path clean before adding edges).
