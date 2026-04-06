# fathomdb Test Plan

## Scope and Purpose

This document is the authoritative test plan for `fathomdb`. It covers nine
layers plus cross-cutting concerns, each building on the one below. A failure
at a lower layer can mask all higher-layer behavior. Test execution should
proceed bottom-up: confirm each layer is stable before testing the next.

1. **Layer 1 — Physical Storage** — SQLite file integrity, WAL, pragma
   bootstrap, schema migration
2. **Layer 2 — Engine Invariants** — write path, supersession, FTS derivation,
   read path, projection maintenance
3. **Layer 3 — Application Semantics** — provenance, integrity checks, excision,
   admin operations
4. **Layer 4 — Client Use Cases** — real-world workloads from Memex, OpenClaw,
   HermesClaw, and NemoClaw, ranked by frequency and value
5. **Layer 5 — Prevent and Recover** — failure injection, detection, and repair
   for each identified failure mode
6. **Layer 6 — Concurrency Safety** — GIL/thread lifecycle, concurrent
   reads/writes, multi-process lock contention
7. **Layer 7 — Input Sanitization** — FTS5 metacharacters, JSON path injection,
   table name safety, oversized input
8. **Layer 8 — Crash Recovery and Durability** — WAL recovery on reopen,
   disk-full behavior, ungraceful shutdown
9. **Layer 9 — Stress and Scale** — 10k+ node datasets, sustained concurrent
   load, rebuild performance at scale

**Cross-cutting:** Feature matrix (all flag combinations tested in CI),
platform matrix (Linux/Windows/macOS coverage), historical defect analysis

---

## Layer 1: Physical Storage

Tests in this layer confirm the SQLite foundation is correctly initialized and
that the engine can detect physical corruption before it reaches higher layers.

### 1.1 Schema Bootstrap

| Test | Description | Status |
|---|---|---|
| `schema_bootstraps_all_required_tables` | Open engine on fresh temp file; verify `nodes`, `edges`, `chunks`, `fts_nodes`, `runs`, `steps`, `actions` all exist | ✅ covered |
| `schema_has_partial_unique_index_on_nodes` | Verify `idx_nodes_active_logical_id` exists and is a partial index on `logical_id WHERE superseded_at IS NULL` | ✅ covered |
| `schema_has_partial_unique_index_on_edges` | Same for `idx_edges_active_logical_id` | ✅ covered |
| `schema_version_persists_across_reopen` | Open engine, close, reopen; verify schema version matches without re-migrating | ✅ covered |
| `migration_ordering_is_deterministic` | Apply all migrations in order; verify each leaves schema in expected state | ✅ covered |

### 1.2 Pragma Initialization

| Test | Description | Status |
|---|---|---|
| `startup_pragma_journal_mode_is_wal` | After open, query `PRAGMA journal_mode` → must return `wal` | ✅ covered |
| `startup_pragma_foreign_keys_is_on` | Query `PRAGMA foreign_keys` → must return `1` | ✅ covered |
| `startup_pragma_busy_timeout_is_set` | Query `PRAGMA busy_timeout` → must return ≥ 5000 | ✅ covered |
| `startup_pragma_synchronous_is_applied` | Query `PRAGMA synchronous` → must not be `FULL` (prefer `NORMAL`) | ✅ covered |

### 1.3 WAL Mode Behavior

| Test | Description | Status |
|---|---|---|
| `wal_mode_allows_concurrent_readers` | Start a long-running read transaction; concurrently submit a write; verify write completes without error | ✅ covered |
| `wal_checkpoint_does_not_lose_committed_data` | Submit writes, force `PRAGMA wal_checkpoint(FULL)`, reopen, verify all data present | ✅ covered |
| `check_integrity_passes_on_fresh_database` | Open engine, write nothing, `check_integrity()` → `physical_ok = true`, `foreign_keys_ok = true` | ✅ covered |
| `check_integrity_passes_after_writes` | Write nodes/edges/chunks, `check_integrity()` → `missing_fts_rows = 0` | ✅ covered |

---

## Layer 2: Engine Invariants

Tests in this layer confirm that the write pipeline, read pipeline, and
projection maintenance all uphold their core contracts regardless of caller
behavior.

### 2.1 Typed Write Path

#### Canonical row insertion

| Test | Description | Status |
|---|---|---|
| `node_insert_writes_all_fields_to_nodes_table` | Submit `NodeInsert`; open DB directly; verify `row_id`, `logical_id`, `kind`, `properties`, `source_ref`, `created_at`, `superseded_at IS NULL` | ✅ covered |
| `chunk_insert_writes_to_chunks_table` | Submit `ChunkInsert`; verify row in `chunks` with correct `node_logical_id`, `text_content` | ✅ covered |
| `edge_insert_writes_to_edges_table` | Submit two nodes + `EdgeInsert`; verify `source_logical_id`, `target_logical_id`, `kind`, `superseded_at IS NULL` in `edges` | ✅ covered |
| `run_step_action_fk_chain_writes_correctly` | Submit `RunInsert` + `StepInsert` (run_id matches) + `ActionInsert` (step_id matches); verify FK chain in DB | ✅ covered |

#### FTS derivation

| Test | Description | Status |
|---|---|---|
| `fts_rows_derived_from_chunks_in_same_request` | Submit `NodeInsert` + `ChunkInsert`; open DB; verify `fts_nodes` row with correct `chunk_id`, `node_logical_id`, `kind`, `text_content` | ✅ covered |
| `fts_rows_derived_from_chunk_for_pre_existing_node` | Write node in request 1; write chunk for that node in request 2; verify FTS rows created | ✅ covered |
| `fts_write_fails_for_completely_unknown_node` | Submit `ChunkInsert` with `node_logical_id` absent from DB and from same request; verify `EngineError::InvalidWrite` | ✅ covered |

#### Supersession invariant

| Test | Description | Status |
|---|---|---|
| `upsert_creates_new_active_row_and_retires_old` | Insert node A, then insert node with same `logical_id`, `upsert: true`; verify A has `superseded_at IS NOT NULL`, new row has `superseded_at IS NULL` | ✅ covered |
| `duplicate_insert_without_upsert_fails` | Insert node A; insert same `logical_id`, `upsert: false`; verify constraint error, original row unchanged | ✅ covered |
| `upsert_edge_supersedes_prior_active_edge` | Same pattern for `EdgeInsert` | ✅ covered |

#### ChunkPolicy on replace

| Test | Description | Status |
|---|---|---|
| `chunk_policy_replace_deletes_old_chunks_atomically` | Insert node + chunk-1; replace with `ChunkPolicy::Replace` + chunk-2; verify chunk-1 gone, chunk-2 present | ✅ covered |
| `chunk_policy_replace_deletes_old_fts_rows` | After replace: FTS search for old text → 0 results; FTS search for new text → 1 result | ✅ covered |
| `chunk_policy_preserve_keeps_old_chunks` | Replace with `ChunkPolicy::Preserve`; verify original chunk still in `chunks` table | ✅ covered |
| `chunk_policy_replace_is_atomic_on_failure` | Confirm old chunks are deleted before new node is inserted, so partial failure leaves detectable state | ✅ covered |

#### NodeRetire and EdgeRetire

| Test | Description | Status |
|---|---|---|
| `node_retire_sets_superseded_at` | Submit `NodeRetire`; verify `superseded_at IS NOT NULL` on prior active row; no active row remains | ✅ covered |
| `node_retire_deletes_chunks_and_fts_rows` | Write node + 2 chunks; retire; verify `chunks` and `fts_nodes` both have 0 rows for that `node_logical_id` | ✅ covered |
| `edge_retire_sets_superseded_at` | Submit `EdgeRetire`; verify edge superseded | ✅ covered |
| `retire_without_source_ref_produces_provenance_warning` | Submit `NodeRetire` with `source_ref: None`; verify `receipt.provenance_warnings` is non-empty | ✅ covered |

#### Runtime table upsert

| Test | Description | Status |
|---|---|---|
| `run_upsert_supersedes_prior_active_run` | Insert run (status: "active"); upsert same id (status: "completed"); verify old row superseded, new row active with "completed" | ✅ covered |
| `step_upsert_supersedes_prior_active_step` | Same pattern for `StepInsert` | ✅ covered |
| `action_upsert_supersedes_prior_active_action` | Same pattern for `ActionInsert` | ✅ covered |
| `duplicate_run_insert_without_upsert_fails` | Insert run; insert same id without `upsert: true`; verify constraint error | ✅ covered |

#### Provenance warnings

| Test | Description | Status |
|---|---|---|
| `nodes_without_source_ref_produce_provenance_warnings` | Submit `NodeInsert` with `source_ref: None`; verify `receipt.provenance_warnings` non-empty | ✅ covered |
| `nodes_with_source_ref_produce_no_warnings` | All nodes have `source_ref: Some(...)` → `receipt.provenance_warnings` is empty | ✅ covered |

### 2.2 Read Path

#### Persistent connection and statement cache

| Test | Description | Status |
|---|---|---|
| `coordinator_uses_persistent_connection` | Multiple reads execute without reopening the DB file; `cached_statement_count()` grows without per-call overhead | ✅ covered |
| `repeated_same_shape_query_reuses_cache_entry` | Execute same structural query twice; verify `cached_statement_count() = 1` | ✅ covered |
| `different_shape_queries_get_separate_cache_entries` | Execute queries with different structural constants (LIMIT, depth); verify `cached_statement_count() = 2` | ✅ covered |

#### Bind value adapter

| Test | Description | Status |
|---|---|---|
| `bind_value_text_produces_correct_result` | Execute query with `BindValue::Text`; verify correct rows returned | ✅ covered |
| `bind_value_integer_produces_correct_result` | Same for `BindValue::Integer` | ✅ covered |
| `bind_value_bool_true_produces_one` | Same for `BindValue::Bool(true)` | ✅ covered |
| `bind_value_bool_false_produces_zero` | Same for `BindValue::Bool(false)` | ✅ covered |

#### Row decoding

| Test | Description | Status |
|---|---|---|
| `execute_compiled_read_returns_decoded_node_rows` | Write node, execute matching query; verify `NodeRow` fields (`row_id`, `logical_id`, `kind`, `properties`) match insert | ✅ covered |
| `execute_compiled_read_returns_empty_for_no_match` | Write Meeting nodes; execute query for Task kind; verify `rows.nodes` is empty | ✅ covered |
| `execute_compiled_read_only_returns_active_rows` | Write node, supersede it; execute query; verify 0 results | ✅ covered |

#### Vector capability error

| Test | Description | Status |
|---|---|---|
| `vector_read_returns_capability_missing_when_table_absent` | Compile a vector query; execute without `vec_nodes_active` table; verify `EngineError::CapabilityMissing`, not a generic SQLite error | ✅ covered |

#### Graph traversal

| Test | Description | Status |
|---|---|---|
| `traversal_query_returns_connected_node_via_typed_writes` | Write node A, node B, edge A→B; compile traversal from A; verify B in results | ✅ covered |
| `traversal_does_not_follow_retired_edges` | Write edge, retire it; compile traversal; verify 0 results | ✅ covered |
| `traversal_follows_logical_id_through_superseded_node` | Write node A, supersede to A2; write edge to A; compile traversal; verify A2 (not A) returned | ✅ covered |

### 2.3 ID Generation

| Test | Description | Status |
|---|---|---|
| `new_row_id_returns_unique_ids` | Three consecutive calls return distinct values | ✅ covered |
| `new_row_id_has_expected_format` | All returned IDs contain only hex digits and dashes | ✅ covered |
| `new_row_id_is_valid_as_node_insert_row_id` | Use return value as `row_id` in `NodeInsert`; verify write succeeds | ✅ covered |

---

## Layer 3: Application Semantics

Tests in this layer confirm that the provenance, integrity, and admin surfaces
hold their contracts under both normal and degraded conditions.

### 3.1 Provenance and Tracing

| Test | Description | Status |
|---|---|---|
| `trace_source_returns_nodes_with_matching_source_ref` | Write nodes with two distinct `source_ref` values; trace the first; verify only its nodes returned | ✅ covered |
| `trace_source_returns_action_ids` | Write actions with `source_ref`; trace; verify `action_ids` and `action_rows` counts correct | ✅ covered |
| `trace_source_does_not_bleed_across_sources` | Two sources; trace one; verify nodes from the other are absent | — |
| `trace_source_includes_node_logical_ids` | Write two nodes with same source; trace; verify `node_logical_ids` contains both | ✅ covered |

### 3.2 Excision

| Test | Description | Status |
|---|---|---|
| `excise_source_supersedes_all_matching_nodes` | Write nodes with `source_ref = "bad-run"`; excise; verify all superseded | — |
| `excise_source_cleans_fts_projections` | Write node + chunk with `source_ref = "bad-run"`; excise; `check_integrity()` → `missing_fts_rows = 0` | ✅ covered |
| `excise_source_is_idempotent` | Excise same source twice; verify second call succeeds, state unchanged | ✅ covered |
| `excise_source_does_not_affect_other_sources` | Write two sources; excise one; verify other source's nodes still active | ✅ covered |
| `excise_source_restores_prior_active_version_when_available` | Write v1 (source A), write v2 (source B, upsert); excise B; verify v1 becomes active again | ✅ covered |

### 3.3 Integrity Checks

| Test | Description | Status |
|---|---|---|
| `check_integrity_detects_missing_fts_rows` | Write node + chunk; manually delete `fts_nodes` row; `check_integrity()` → `missing_fts_rows > 0` | — |
| `check_integrity_detects_duplicate_active_logical_ids` | Inject duplicate active row via writable_schema trick; `check_integrity()` → `duplicate_active_logical_ids > 0` | — |
| `check_semantics_detects_orphaned_chunks` | Write node + chunk; manually delete node without cleanup; `check_semantics()` → `orphaned_chunks > 0` | ✅ covered |
| `check_semantics_detects_stale_fts_rows` | Write node + chunk + FTS; delete chunk; `check_semantics()` → `stale_fts_rows > 0` | ✅ covered |
| `check_semantics_detects_fts_for_superseded_nodes` | Write node + FTS; supersede node; do not clean FTS; `check_semantics()` → `fts_rows_for_superseded_nodes > 0` | ✅ covered |
| `check_semantics_detects_null_source_ref_nodes` | Insert node with `source_ref: None`; `check_semantics()` → `null_source_ref_nodes > 0` | ✅ covered |
| `check_semantics_detects_broken_step_fk_chains` | Insert step with non-existent `run_id` via injection; `check_semantics()` → `broken_step_fk_chains > 0` | ✅ covered |

### 3.4 Projection Rebuild

| Test | Description | Status |
|---|---|---|
| `rebuild_projections_fts_restores_missing_rows` | Write node + chunk; delete all `fts_nodes`; `rebuild_projections(Fts)` → `check_integrity()` → `missing_fts_rows = 0` | ✅ covered |
| `rebuild_projections_fts_is_deterministic` | Rebuild FTS twice; verify row count and FTS search results are identical after both | — |
| `rebuild_projections_excludes_superseded_nodes` | Write node + chunk; supersede node; rebuild FTS; verify no FTS rows for superseded node's chunks | — |

### 3.5 Safe Export

| Test | Description | Status |
|---|---|---|
| `safe_export_creates_readable_copy` | Write data; `safe_export(path)`; open exported file as new engine; verify nodes/chunks/FTS are present | ✅ covered |
| `safe_export_checkpoints_wal_before_copy` | Export while WAL has unflushed frames; verify exported file contains all committed data *(Phase 2 — not yet implemented)* | ❌ |
| `safe_export_produces_manifest_with_sha256` | After export, verify manifest JSON exists with `schema_version`, `page_count`, `export_timestamp`, `sha256` *(Phase 2 — not yet implemented)* | ❌ |

### 3.6 Semantic Integrity End-to-End

| Test | Description | Status |
|---|---|---|
| `retire_then_check_semantics_reports_clean` | Write node + chunk; retire; `check_semantics()` → `orphaned_chunks = 0`, `stale_fts_rows = 0`, `fts_rows_for_superseded_nodes = 0` | — |
| `replace_with_chunk_replace_then_check_semantics_clean` | Write node + chunk A; replace with `ChunkPolicy::Replace` + chunk B; `check_semantics()` → all counts = 0 | — |
| `excise_then_rebuild_leaves_clean_state` | Write batch, excise source, rebuild FTS; `check_integrity()` → all clean | — |

---

## Layer 4: Client Use Cases

This layer validates `fathomdb` against the real-world workloads of four
primary clients. Tests are organized by client; the ranking table below
shows which use cases apply across the broadest client surface.

### Clients

**Memex** is a local personal AI agent (documented in `dev/`) that stores
knowledge objects, meeting transcripts, session state, ingestion runs, and
planning artifacts in SQLite with FTS and vector search projections.

**OpenClaw** (openclaw.ai) is a widely-adopted open-source self-hosted
personal AI agent (~247k GitHub stars as of early 2026) that bridges 30+
messaging platforms and uses SQLite + FTS5 + sqlite-vec for per-agent
persistent memory, a personal CRM graph, task/project tracking, and
cron-driven automations.

**HermesClaw** refers to the Hermes Agent ecosystem (NousResearch, released
February 2026) built on the OpenClaw platform. It extends OpenClaw with a
self-improvement loop: after each novel task it writes a reusable Skill
Document, deepens a user model across sessions, and supports `hermes claw
migrate` to import prior OpenClaw state. Skills, user model, conversation
history, and contact graph are the highest-value storage areas.

**NemoClaw** (NVIDIA, announced GTC March 2026) is the enterprise distribution
of OpenClaw. It adds a kernel-level sandbox (OpenShell), a privacy router,
a declarative policy engine, and a structured audit trail. Every agent action
must be durable, traceable, and compliance-reportable. The `runs → steps →
actions` provenance chain is its most critical storage requirement.

### Cross-Client Use Case Ranking

Ranked by how many clients use the capability and what breaks if it fails.

| Rank | Use Case | Clients | Failure Impact |
|---|---|---|---|
| 1 | Entity knowledge storage (NodeInsert + chunks + FTS) | All 4 | Total memory loss |
| 2 | Execution provenance (run/step/action chain) | All 4 | Audit trail missing; compliance broken |
| 3 | FTS text recall across accumulated content | All 4 | Knowledge search silent failure |
| 4 | Graph traversal (relationship queries) | All 4 | Wrong answers; broken CRM |
| 5 | Upsert/supersession (knowledge correction) | All 4 | Stale data feeds hallucinations |
| 6 | source_ref on every write | All 4 | Bad data permanently unexcisable |
| 7 | Vector semantic search | OpenClaw, HermesClaw, NemoClaw, Memex | Degraded recall; user trust drops |
| 8 | Bulk excise bad agent output | All 4 | Poisoned world model cannot be cleaned |
| 9 | Retire / soft-delete lifecycle | All 4 | Orphaned chunks; stale FTS |
| 10 | FTS rebuild after crash | All 4 | Stale search index persists silently |
| 11 | Contact/person graph (CRM pattern) | OpenClaw, HermesClaw, Memex | Broken relationship queries |
| 12 | Cron-based execution with run tracking | OpenClaw, HermesClaw, NemoClaw | Missing scheduled-run audit trail |
| 13 | Skills library (Skill nodes + semantic lookup) | OpenClaw, HermesClaw | Agent repeats solved problems |
| 14 | Multi-agent delegation provenance | NemoClaw, OpenClaw | Sub-agent artifacts untraceable |
| 15 | Capability degradation (no vector) | OpenClaw, HermesClaw, Memex | Agent stops on a device that lacks sqlite-vec |

### Memex Client Tests

**M-1: Meeting transcript ingestion (Rank 1)**
- Submit `NodeInsert(kind="Meeting", source_ref="ingest-run-1")` + multiple `ChunkInsert` rows (paragraphs)
- Verify FTS row per chunk in `fts_nodes`
- Execute `text_search("budget discussion")` → returns the meeting node
- `check_integrity()` → `missing_fts_rows = 0`

**M-2: Meeting note correction via upsert (Rank 5)**
- Write meeting v1 with chunk text "old notes"
- Write meeting v2: `upsert: true`, `ChunkPolicy::Replace`, new chunk text "corrected notes"
- FTS search "old notes" → 0 results; FTS search "corrected notes" → 1 result
- `check_semantics()` → `fts_rows_for_superseded_nodes = 0`

**M-3: Ingestion job run tracking (Rank 2)**
- Submit `RunInsert(kind="document-ingest", status="active")` + `StepInsert` + `ActionInsert`
- Upsert same run id with `status = "completed"`
- Verify: one active run row with `status = "completed"`, one historical row
- `trace_source("ingest-run-1")` → correct node count + action count

**M-4: Archive completed project via retire (Rank 9)**
- Write 3 Meeting nodes + chunks
- `NodeRetire` all three with `source_ref = "archive-op"`
- `check_semantics()` → `orphaned_chunks = 0`, `stale_fts_rows = 0`
- FTS search for meeting text → 0 results (retired nodes excluded from queries)

**M-5: Post-crash FTS repair (Rank 10)**
- Write data; inject partial FTS deletion (delete one `fts_nodes` row)
- `check_integrity()` → `missing_fts_rows > 0`
- `rebuild_projections(Fts)` → `check_integrity()` → `missing_fts_rows = 0`

**M-6: Provenance excision of bad ingest run (Rank 8)**
- Write 5 nodes with `source_ref = "bad-ocr-run"`, 3 nodes with `source_ref = "good-run"`
- `excise_source("bad-ocr-run")` → 5 nodes superseded, 3 good nodes still active
- `check_integrity()` → `physical_ok = true`, `missing_fts_rows = 0`

### OpenClaw Client Tests

**OC-1: Personal CRM — contact and relationship graph (Rank 11)**
- Write `NodeInsert(kind="Person", logical_id="person:alice")` + `NodeInsert(kind="Organization", logical_id="org:acme")`
- Write `EdgeInsert(source="person:alice", target="org:acme", kind="WORKS_AT")`
- Traverse `Person → WORKS_AT → Organization` → returns Acme node
- Alice changes jobs: `EdgeInsert` same `logical_id`, new target, `upsert: true`
- Traverse again → returns new org, not old; old edge is historical

**OC-2: Task dependency traversal (Rank 4)**
- Write Task nodes T1, T2, T3
- Write `EdgeInsert(T1→T2, kind="BLOCKS")`, `EdgeInsert(T2→T3, kind="BLOCKS")`
- Traverse `T1 → BLOCKS (depth 2)` → returns T2 and T3
- Upsert T2 status to "in_progress"
- Traverse again → same structure, T2 now has updated properties

**OC-3: Semantic memory recall over accumulated content (Rank 3 + 7)**
- Write 10 `NodeInsert(kind="Memory")` nodes with diverse chunk text
- FTS search "deployment anxiety budget" → returns most relevant memory node
- (When sqlite-vec enabled) vector search → returns semantically nearest chunks

**OC-4: Email-to-commitment-to-task provenance chain (Rank 6)**
- Write `NodeInsert(kind="Email", source_ref="gmail:msg-123")`
- Write `NodeInsert(kind="Commitment", source_ref="gmail:msg-123")`
- Write `EdgeInsert(Email→Commitment, kind="EXTRACTED")`
- Write `NodeInsert(kind="Task", source_ref="extraction-run-1")`
- Write `EdgeInsert(Commitment→Task, kind="ASSIGNED_TO")`
- `trace_source("gmail:msg-123")` → returns Email + Commitment nodes
- `excise_source("extraction-run-1")` → Task superseded; Commitment and Email still active

**OC-5: Cron briefing run lifecycle (Rank 12)**
- Write `RunInsert(kind="daily-briefing", status="active", source_ref="cron:morning-brief")`
- Write `StepInsert(kind="gather", status="active")` → upsert to `status="completed"`
- Upsert run to `status="completed"`
- Verify: active run has `status = "completed"`; one historical run row has `status = "active"`
- `trace_source("cron:morning-brief")` → returns full action chain

**OC-6: Multi-agent delegation provenance (Rank 14)**
- Write `RunInsert(kind="orchestrator", source_ref="session:main")`
- Write `RunInsert(kind="subagent-1", source_ref="session:sub-1")`
- Write `EdgeInsert(orchestrator→subagent, kind="DELEGATES_TO")`
- Write node artifact with `source_ref = "session:sub-1"`
- `trace_source("session:sub-1")` → returns artifact node but not orchestrator run
- `excise_source("session:sub-1")` → artifact superseded; orchestrator run unaffected

### HermesClaw Client Tests

**HC-1: Persistent user model with frequent upsert (Rank 5)**
- Write `NodeInsert(kind="User", logical_id="user:primary", source_ref="session:0")`
- Upsert with new properties twice (preference change, timezone change)
- Query `filter_logical_id_eq("user:primary")` → returns exactly one active row (latest version)
- Verify: 2 historical rows with `superseded_at IS NOT NULL`

**HC-2: Skills library — write and semantic recall (Rank 13)**
- Write 5 `NodeInsert(kind="Skill")` nodes with diverse chunk descriptions
- FTS `text_search("health check monitoring")` → returns the correct skill node
- (When vector enabled) vector search → top-1 semantically nearest skill is correct

**HC-3: Skill revision — supersession with version history (Rank 5)**
- Write Skill v1 with chunk describing approach A
- Replace with `upsert: true`, `ChunkPolicy::Replace`, new chunk describing approach B
- FTS search for approach A text → 0 results; approach B text → 1 result
- Verify Skill v1 row still exists as historical with `superseded_at IS NOT NULL`

**HC-4: Session search across conversation history (Rank 3)**
- Write 10 `NodeInsert(kind="Conversation")` nodes with diverse chunk text
- FTS `text_search("deployment deadline slipping")` → returns the correct conversation node
- Verify: logical_id of returned node matches expected conversation

**HC-5: Migration bulk ingestion with provenance (Rank 6 + 8)**
- Write 20 nodes with `source_ref = "openclaw-migration:run-1"`
- Write 5 nodes with `source_ref = "hermes-session:run-2"`
- `excise_source("openclaw-migration:run-1")` → 20 nodes superseded
- `check_integrity()` → `physical_ok = true`, `missing_fts_rows = 0`
- `check_semantics()` → all projection counts = 0

### NemoClaw Client Tests

**NC-1: Per-execution audit trail — source_ref on everything (Rank 2 + 6)**
- Submit `WriteRequest` with `RunInsert` + 3 `StepInsert` + 5 `ActionInsert`, all with `source_ref = "run:abc-123"`
- `trace_source("run:abc-123")` → `action_rows = 5`, `action_ids` count correct
- All nodes/edges/actions have non-null `source_ref` → `receipt.provenance_warnings` is empty

**NC-2: Web scrape deduplication via supersession (Rank 5)**
- Write `NodeInsert(kind="WebPage", logical_id="url:example.com/pricing", source_ref="scrape:run-1")` + chunk v1
- Upsert same `logical_id` with `ChunkPolicy::Replace`, new chunk v2, `source_ref = "scrape:run-2"`
- FTS search v1 text → 0 results; v2 text → 1 result
- Verify 2 historical rows for that `logical_id` (v1 superseded, v2 active)

**NC-3: Policy decision as durable node with edge (Rank 4)**
- Write `NodeInsert(kind="PolicyDecision", logical_id="policy:allow-github-api")`
- Write `NodeInsert(kind="ApiEndpoint", logical_id="api:github.com")`
- Write `EdgeInsert(PolicyDecision→ApiEndpoint, kind="APPROVED_FOR")`
- Traverse `PolicyDecision → APPROVED_FOR` → returns `ApiEndpoint` node
- Retire `PolicyDecision` at session end
- Traverse again → 0 results (retired node excluded)

**NC-4: Compliance retraction of flagged session (Rank 8)**
- Write 30 nodes across 3 sources (10 per source, `source_ref = session-id`)
- Flag session-2 as policy-violating
- `excise_source(session-2 source_ref)` → 10 nodes superseded
- `check_integrity()` → clean; `check_semantics()` → all counts = 0
- Verify session-1 and session-3 nodes still active (verified by FTS search)

**NC-5: Capability degradation on device without sqlite-vec (Rank 15)**
- Compile vector query on engine without `vec_nodes_active` table
- Execute → verify `EngineError::CapabilityMissing` returned
- Verify FTS-only query on the same engine succeeds normally
- Verify engine continues to accept writes and non-vector reads

### Python Binding Tests (`test_bindings.py`)

The Python test suite (`test_bindings.py`) exercises the engine through the
PyO3 binding layer, validating that the full operational lifecycle works
end-to-end from Python. Coverage includes:

- **Operational lifecycle**: open engine, write, read, close, reopen, verify
  data survives
- **Write/read roundtrips**: `NodeInsert`, `ChunkInsert`, `EdgeInsert` through
  the Python `WriteRequest` builder; read back via compiled queries
- **Vector degradation**: verify that queries requesting vector search on an
  engine without sqlite-vec return a meaningful error rather than crashing
- **Grouped reads**: multi-entity read requests that exercise the coordinator's
  batch path from Python
- **FTS search**: text search queries through the Python binding confirm FTS
  projections are visible to the Python caller
- **Upsert and retire**: supersession and retire operations through the Python
  API, verifying chunk cleanup and FTS consistency

---

## Layer 5: Prevent and Recover

This layer tests `fathomdb`'s robustness against each identified failure mode.
Each group follows the pattern:

1. **Prevention** — prove the failure is structurally prevented where possible
2. **Detection** — inject the failure, run a check, verify it is reported at the
   correct severity
3. **Recovery** — inject, apply the documented repair action, verify the
   database is clean

The Go injection harness (`test/corrupt/`) supplies storage-level injectors.
Engine-level injections use direct SQLite writes in test setup. Application-level
injections use bad `WriteRequest` values or deliberate raw SQL.

### 5.1 Physical Storage Failures

**WAL checksum chain corruption (silent bit flip)**
- Prevention: `walcheck` rolling-checksum validation detects truncation offset
- Detection: `InjectWALBitFlip(frameN)` → `fathom-integrity check` → Layer 1 finding reports WAL truncation at frame N, advisory emitted
- Recovery: `PRAGMA wal_checkpoint(TRUNCATE)` → verify data committed before frame N is readable; frames N+ acknowledged as lost

**File header corruption**
- Prevention: header check runs before any higher-layer check; Layer 2/3 skipped on Critical
- Detection: `InjectHeaderCorruption` → `check` → Critical finding reported, Layer 2/3 marked skipped
- Recovery: `fathom-integrity recover --db <corrupt> --dest <new>` → bootstrap schema → basic write/read roundtrip succeeds

**File truncation**
- Detection: `InjectTruncation` → `check` → `file_size_aligned = false`, severity Critical
- Recovery: `fathom-integrity recover` → at least some nodes recoverable; `check_integrity()` on recovered DB passes

**B-tree cell count too low (silent row loss)**
- Prevention: None available via SQL pragma — known SQLite blind spot
- Detection: Cannot be detected by `PRAGMA integrity_check`; document as known limitation
- Note: `sqlite3 .recover` on a separate instance can often surface hidden rows; recommend periodic `.recover` spot checks for high-value databases

**WAL present during export**
- Detection: `fathom-integrity export` before WAL checkpoint → warn that export may be missing committed frames
- Recovery (Phase 2): enforce `PRAGMA wal_checkpoint(FULL)` before file copy; fail export if checkpoint fails

### 5.2 Engine Invariant Failures

**Missing FTS projections**
- Prevention: FTS rows committed in same transaction as canonical rows; partial failure leaves both gone
- Detection: `InjectFTSDeletion` → `check_integrity()` → `missing_fts_rows > 0`
- Recovery: `fathom-integrity rebuild --target fts` → `check_integrity()` → `missing_fts_rows = 0`

**Stale FTS rows (chunk deleted, FTS row remains)**
- Prevention: `ChunkPolicy::Replace` atomically deletes FTS before chunks inside the same `IMMEDIATE` transaction
- Detection: `InjectStaleFTSRow` → `check_semantics()` → `stale_fts_rows > 0`
- Recovery: `rebuild --target fts` (full rebuild clears all stale rows)

**FTS rows for superseded nodes**
- Prevention: retire and replace both clean up FTS atomically
- Detection: `InjectFTSForSupersededNode` → `check_semantics()` → `fts_rows_for_superseded_nodes > 0`
- Recovery: `rebuild --target fts`

**Duplicate active logical IDs**
- Prevention: partial unique index rejects this via the normal write path
- Detection: `InjectDuplicateActiveLogicalID` (writable_schema trick) → `check_integrity()` → `duplicate_active_logical_ids > 0`
- Recovery: `excise_source` on the erroneous source, or manual `NodeRetire` of one duplicate

**Broken FK chains (orphaned steps or actions)**
- Prevention: `PRAGMA foreign_keys = ON` rejects bad inserts at write time
- Detection: Disable FK temporarily, inject orphaned step via direct SQL → `check_semantics()` → `broken_step_fk_chains > 0`
- Recovery: No automated repair in v1; document as operator investigation required

### 5.3 Application Semantic Failures

**Orphaned chunks (node retired without cleanup)**
- Prevention: `NodeRetire` automatically deletes chunks; `ChunkPolicy::Replace` on upsert cleans before supersede
- Detection: `InjectOrphanedChunk` → `check_semantics()` → `orphaned_chunks > 0`
- Recovery: No automated repair in v1; manual removal via direct SQL after investigation; document expected workflow

**NULL source_ref on nodes or actions (provenance gap)**
- Prevention: `WriteReceipt.provenance_warnings` alerts caller at write time; client should treat this as an error
- Detection: `InjectNullSourceRef` → `check_semantics()` → `null_source_ref_nodes > 0`
- Recovery: Re-ingest with correct `source_ref`; or accept gap (tracked as metric); node is unexcisable by source

**Partial excision chain**
- Prevention: `excise_source` is idempotent — re-running on the same `source_ref` converges
- Detection: `InjectPartialExcision` → `check_semantics()` → broken FK or orphaned rows reported
- Recovery: Re-run `excise_source` on same `source_ref` → verify clean

**Dangling edges after node retire**
- Prevention: Not yet enforced; caller must explicitly retire or update incident edges
- Detection: Retire a node without retiring its edges → run `check_semantics()` → `dangling_edges > 0`
- Recovery: retire or supersede incident edges; then re-run `check_semantics()` until clean

**Broken supersession chain (all versions retired, no active successor)**
- Prevention: Write path atomically supersedes and inserts in one transaction; crash leaves either both or neither
- Detection: Manually supersede without inserting replacement → `check_semantics()` → `orphaned_supersession_chains > 0`
- Recovery: insert/supersede to restore one active version for the affected `logical_id`

**Malformed JSON in node properties (lazy failure)**
- Prevention: Validate JSON on the client before submission
- Detection: Insert node with invalid JSON → execute query with `json_extract` predicate → error returned at read time
- Note: Silent until first JSON-aware query on that row; recommend client-side validation before `WriteRequest` is built
- Recovery: Upsert the node with corrected properties using `upsert: true`

### 5.4 Operational Failures

**Writer actor crash**
- Prevention: `WriterActor` channel signals failure immediately to caller
- Detection: Kill writer thread (panic injection) → `submit()` → verify `WriterRejected` error returned without hang
- Recovery: Engine restart; last committed transaction is durable (WAL); no partial canonical state

**sqlite-vec capability missing at query time**
- Prevention: `EngineError::CapabilityMissing` is explicit and actionable
- Detection: Execute vector query without `vec_nodes_active` table → verify `CapabilityMissing`, not opaque SQLite error
- Recovery: Enable sqlite-vec extension and bootstrap vector table, or rewrite query to use FTS-only path

**Duplicate row_id submission**
- Prevention: Use `new_row_id()` helper to avoid collisions; SQLite PRIMARY KEY rejects duplicates
- Detection: Submit two `NodeInsert` with identical `row_id` → verify constraint error returned
- Recovery: Retry write with a new unique `row_id`

**Upsert with no prior active row**
- Prevention: Write path tolerates this (UPDATE affects 0 rows, INSERT proceeds normally)
- Detection: `upsert: true` when no active row exists → write succeeds but no historical row is created; `provenance_warnings` should note this in a future improvement
- Recovery: No action needed; result is equivalent to a plain insert

### 5.5 Recovery Workflow Integration Tests

**Full recovery from storage corruption**
1. Write 50 nodes + chunks + FTS rows
2. `InjectTruncation` — lose last 3 pages
3. `fathom-integrity check` → verify Critical finding at Layer 1
4. `fathom-integrity recover --db <corrupt> --dest <new-db>`
5. Open `new-db` as new engine
6. Verify: at least 40 of 50 nodes are recoverable
7. `check_integrity()` on recovered DB → `physical_ok = true`

**Full recovery from a poisoned agent run**
1. Write 20 "good" nodes (`source_ref = "good-run"`)
2. Write 15 "bad" nodes with hallucinated properties (`source_ref = "bad-agent-run"`)
3. `check_semantics()` → note `null_source_ref_nodes` baseline
4. `excise_source("bad-agent-run")` → 15 nodes superseded
5. `rebuild_projections(Fts)` (belt-and-suspenders after excise)
6. `check_integrity()` → `missing_fts_rows = 0`, `physical_ok = true`
7. FTS search → returns only "good-run" content

**Full recovery from FTS projection drift**
1. Write 100 nodes + chunks
2. Manually delete half of `fts_nodes` rows (simulate crash mid-projection)
3. `check_integrity()` → `missing_fts_rows > 0`
4. `rebuild_projections(Fts)`
5. `check_integrity()` → `missing_fts_rows = 0`
6. FTS search over previously-missing content → returns correct results

**WAL bit-flip detection and advisory**
1. Write data that lands in WAL frames 1–5
2. `InjectWALBitFlip(frame=3)`
3. `fathom-integrity check` → verify Layer 1 advisory: WAL truncation at frame 3
4. Force `PRAGMA wal_checkpoint(TRUNCATE)`
5. Verify frames 1–2 data is accessible; frame 3+ loss is acknowledged in check output

**Safe export round-trip (Phase 2)**
1. Write 20 nodes + chunks
2. Perform writes that land in WAL (do not checkpoint)
3. `fathom-integrity export --out /tmp/test-export.db`
4. Verify exported file contains all committed data (including WAL frames)
5. Verify manifest JSON alongside export: `schema_version`, `sha256`, `page_count`
6. Open exported DB as new engine → `check_integrity()` passes

---

## Layer 6: Concurrency Safety

This layer validates that the engine behaves correctly under concurrent access,
with particular attention to the PyO3/GIL interaction that has been the single
largest source of production defects (7 commits across the project's history).

### 6.1 GIL and Thread Lifecycle

The pyo3-log bridge creates an implicit GIL dependency: any Rust code path that
logs while a Python thread holds the GIL can deadlock if it also waits on a
thread that needs the GIL to log. These tests codify the invariant: **never
hold the GIL while joining a thread or waiting on a channel connected to a
thread that uses tracing.**

| Test | Description | Status |
|---|---|---|
| `two_engines_with_debug_logging_no_deadlock` | Open two Engine instances concurrently with DEBUG logging enabled; close both; no hang | ✅ covered |
| `open_close_cycle_with_logging` | Open/close engine 5 times in succession with logging; no hang or GIL deadlock | ✅ covered |
| `gc_drop_without_close_no_deadlock` | Open engine, drop reference without explicit close; GC triggers Rust Drop; verify no deadlock from GIL reacquisition during thread join | ✅ covered |
| `context_manager_with_debug_logging` | Use engine as Python context manager with logging; exit triggers Drop; no hang | ✅ covered |

### 6.2 Concurrent Reads and Writes

| Test | Description | Status |
|---|---|---|
| `concurrent_writes_with_debug_logging` | Two threads submit writes simultaneously with DEBUG logging; both succeed; data consistent | ✅ covered |
| `concurrent_reads_and_writes_with_logging` | Reader and writer threads operate simultaneously; no deadlock, no stale reads of committed data | ✅ covered |
| `close_after_concurrent_reads_complete` | Start concurrent reads, wait for completion, then close; no hang from outstanding statement handles | ✅ covered |
| `admin_ops_with_debug_logging` | Run admin operations (check_integrity) concurrently with writes; no deadlock | ✅ covered |

### 6.3 Multi-Process and Lock Contention

| Test | Description | Status |
|---|---|---|
| `exclusive_file_lock_prevents_second_engine` | Open engine on path A; attempt second open on same path; verify `EngineError::DatabaseLocked` (not hang) | ✅ covered |
| `file_lock_released_on_close` | Open engine, close, reopen same path; verify second open succeeds | ✅ covered |
| `wal_contention_under_concurrent_readers` | Start long-running read; submit write; verify WAL mode allows both without `SQLITE_BUSY` error | ✅ covered |
| `stress_concurrent_read_write_10_threads` | 10 threads (5 readers, 5 writers) operating for 1000 iterations; verify no deadlock, no data corruption, `check_integrity()` clean | — |

---

## Layer 7: Input Sanitization

This layer validates that untrusted input cannot cause SQL injection, syntax
errors, or other security issues. SQL injection was found in 3 independent
locations during the project's history (compile.rs, check.go, admin.rs), each
fixed individually. These tests ensure the attack surface is covered
systematically.

### 7.1 FTS5 Query Sanitization

| Test | Description | Status |
|---|---|---|
| `sanitize_fts5_plain_tokens` | Plain text passes through sanitization unchanged | ✅ covered |
| `sanitize_fts5_apostrophe` | Apostrophes in user text are handled without syntax error | ✅ covered |
| `sanitize_fts5_embedded_double_quotes` | Double quotes are stripped/escaped to prevent FTS5 phrase injection | ✅ covered |
| `sanitize_fts5_operators_neutralized` | FTS5 operators (`AND`, `OR`, `NOT`, `NEAR`) in user text are neutralized | ✅ covered |
| `sanitize_fts5_special_chars` | Column filters (`:`), parentheses, asterisks, and carets are stripped | ✅ covered |
| `sanitize_fts5_empty_input` | Empty or whitespace-only input produces empty result (not syntax error) | ✅ covered |
| `fts5_query_bind_is_sanitized` | Full integration: user text with metacharacters → compiled query → FTS MATCH executes without error | ✅ covered |
| `fts5_unicode_input_preserved` | Non-ASCII text (CJK, emoji, accented) passes through sanitization intact | — |

### 7.2 JSON Path Injection

| Test | Description | Status |
|---|---|---|
| `compile_rejects_invalid_json_path` | JSON path not starting with `$.` is rejected at compile time | ✅ covered |
| `compile_accepts_valid_json_paths` | Well-formed paths like `$.name`, `$.metadata.tags[0]` compile successfully | ✅ covered |
| `json_path_compiled_as_bind_parameter` | JSON path is passed as a bind parameter to `json_extract()`, not interpolated into SQL | ✅ covered |
| `json_path_with_sql_injection_payload` | Path containing `'; DROP TABLE nodes; --` is rejected or safely parameterized | — |

### 7.3 Table and Column Name Safety

| Test | Description | Status |
|---|---|---|
| `go_count_table_rejects_unknown_names` | Go `CountTable()` only accepts allowlisted table names; arbitrary input rejected | ✅ covered |
| `go_count_source_ref_rejects_unknown_tables` | Go `count_source_ref` uses allowlist; injection payload rejected | ✅ covered |
| `rust_table_names_are_never_interpolated` | Verify all SQL in compile.rs uses compile-time constants or allowlists for table names | — |

### 7.4 Oversized and Malformed Input

| Test | Description | Status |
|---|---|---|
| `write_with_very_large_properties_json` | Submit NodeInsert with 1MB+ properties JSON; verify write succeeds or returns clear size error (not OOM crash) | — |
| `write_with_very_long_text_content` | Submit ChunkInsert with 10MB text; verify FTS derivation handles it | — |
| `query_with_deeply_nested_json_path` | JSON path with 50+ levels of nesting compiles correctly or returns clear error | — |

---

## Layer 8: Crash Recovery and Durability

This layer validates that the database recovers correctly after ungraceful
shutdown. For a production database, crash recovery is table stakes — data
committed before the crash must survive, and the engine must reach a
consistent state on reopen.

### 8.1 WAL Recovery on Reopen

| Test | Description | Status |
|---|---|---|
| `reopen_after_unclean_shutdown_recovers_committed_data` | Write data, force-kill process (simulate crash), reopen; verify all committed writes are present | — |
| `wal_replay_does_not_duplicate_fts_rows` | Write node + chunk (commits to WAL), crash before checkpoint, reopen; verify FTS row count is correct (no duplicates from replay) | — |
| `reopen_after_crash_mid_write_discards_uncommitted` | Begin write, crash before commit, reopen; verify partial write is absent, prior committed state intact | — |

### 8.2 Go Recovery Tool Integration

| Test | Description | Status |
|---|---|---|
| `recover_clean_db_round_trip` | `fathom-integrity recover` on a clean DB produces identical copy | ✅ covered |
| `recover_truncated_db_preserves_committed_data` | Truncate last 3 pages; `recover`; verify most data survives | ✅ covered |
| `recover_header_corrupted_db` | Corrupt SQLite header; `recover`; verify bootstrap schema + surviving data | ✅ covered |
| `recover_rebuilds_fts_after_sanitized_replay` | Recovery sanitizes SQL and rebuilds FTS projections | ✅ covered |
| `recover_preserves_vector_data` | Recovery on sqlite-vec enabled DB preserves vector projections | ✅ covered |

### 8.3 Disk-Full and I/O Error Behavior

| Test | Description | Status |
|---|---|---|
| `write_on_full_disk_returns_error_not_corruption` | Fill temp filesystem; submit write; verify `EngineError` returned, DB still readable after freeing space | — |
| `checkpoint_failure_leaves_wal_intact` | Simulate checkpoint failure; verify WAL frames are preserved and data accessible | — |

---

## Layer 9: Stress and Scale

This layer validates behavior at production-relevant data volumes. The client
workload tests in Layer 4 use small datasets (5-30 nodes). A production
database may hold 10k-100k+ nodes accumulated over months of agent operation.

### 9.1 Data Volume

| Test | Description | Status |
|---|---|---|
| `write_and_query_10k_nodes` | Insert 10,000 nodes with chunks; FTS search returns correct results; `check_integrity()` passes | — |
| `supersession_chain_depth_100` | Same `logical_id` upserted 100 times; query returns only latest; all 99 historical rows have `superseded_at` | — |
| `fts_search_accuracy_at_scale` | Insert 10,000 nodes with varied text; FTS search for specific term returns correct subset (precision + recall check) | — |
| `rebuild_projections_at_scale` | Delete all FTS rows for 10k-node DB; rebuild; verify all restored within acceptable time | — |

### 9.2 Concurrent Load

| Test | Description | Status |
|---|---|---|
| `sustained_concurrent_reads_under_write_load` | 5 writer threads + 20 reader threads for 60 seconds; no deadlock, no error accumulation | — |
| `check_integrity_during_active_writes` | Run `check_integrity()` while writes are in progress; verify it completes without false positives or blocking writers | — |

---

## Cross-Cutting: Feature Matrix

Tests must cover all feature flag combinations that ship in production builds.
Feature-gated code that is not tested in CI can silently break (this happened
historically — Python binding compile errors accumulated undetected until a
dedicated CI job was added).

| Feature combination | CI job | Status |
|---|---|---|
| Default (no features) | `rust-test` | ✅ covered |
| `--features tracing` | `rust-test-tracing` | ✅ covered |
| `--features python` | `python-rust-lint` (clippy + check) | ✅ covered |
| `--features python,sqlite-vec,tracing` | `python-test` (via maturin) | ✅ covered |
| `--features sqlite-vec` | `benchmark-and-robustness` | ✅ covered |
| Windows (full workspace) | `rust-test-windows` | ✅ covered |
| Windows + `--features python` | `python-test-windows` | ✅ covered |
| Windows + `--features sqlite-vec` | `python-test-windows` | ✅ covered |

---

## Cross-Cutting: Platform Matrix

Windows testing is scoped to a subset of crates. Historical defects include
file-lock behavior differences and path handling issues that only surfaced
after CI was extended to Windows.

| Platform | Scope | Status |
|---|---|---|
| Linux (ubuntu-latest) | Full workspace: all crates, all features, Go + Python | ✅ covered |
| Windows (windows-latest) | Full workspace: all crates including sqlite-vec | ✅ covered |
| Windows Go | `internal/...`, `cmd/...` (all except e2e) | ✅ covered |
| Windows Python | Full test suite + harness (baseline + vector modes) | ✅ covered |
| macOS | Not tested in CI | ❌ not tested |

---

## Historical Defect Analysis

This section documents the defect patterns that informed Layers 6-9. It is
maintained as a living record so that future test additions can be traced to
the defects that motivated them.

### Defect Categories (ranked by frequency)

**1. GIL/Thread Deadlocks (7 commits)** — The PyO3 binding layer creates an
implicit GIL dependency via pyo3-log. Any Rust code path that (a) holds the
GIL and (b) waits on a thread that logs will deadlock. This was hit 3+ times
in independent code paths: engine open, `supports_vector_mode`, and GC-triggered
Drop. Fixed systematically in the GC-safe Drop implementation, but the invariant
must be regression-tested (Layer 6).

**2. SQL Injection (5 commits)** — String interpolation of table names and JSON
paths appeared in 3 independent files (Rust `compile.rs`, Go `check.go`, Rust
`admin.rs`). Each was found and fixed separately. FTS5 metacharacter injection
(`e8846cd`) was the most recent instance. These tests are now in Layer 7.

**3. Transaction Atomicity Gaps (4 commits)** — Multi-statement mutations
(`excise_source`, FTS rebuild, `rebuild_missing_projections`) were found to
lack `IMMEDIATE` transaction wrapping. Partial failure could leave the database
in an inconsistent state. Each was fixed with transaction wrapping; Layer 2
tests now verify atomicity.

**4. CI/Build Breakage (6 commits)** — CI was not green for 50+ consecutive
runs. The `--features python` code path accumulated 3 compile errors undetected.
The Feature Matrix cross-cutting section ensures all shipped combinations are
tested.

**5. Cross-Platform Failures (2 commits)** — Windows file-lock semantics differ
from Unix (cannot read PID from lock file while held). The Platform Matrix
section documents what is and isn't tested on each platform.

---

## Test Execution Reference

### Current test counts (460+ across the workspace)

| Target | Approximate count | Focus areas |
|---|---|---|
| fathomdb-engine (Rust) | ~158 | writer, admin, coordinator, operational, IDs, sqlite |
| fathomdb-query (Rust) | ~18 | builder, compile, plan, FTS sanitization |
| fathomdb-schema (Rust) | ~12 | bootstrap, migration |
| fathomdb facade (Rust) | ~34 integration + ~27 client workload | end-to-end lifecycle, client scenarios |
| fathomdb `--features python` (Rust) | ~32 | PyQueryStep roundtrips, report parity, write request, error coverage |
| Python tests | ~48 | bindings, concurrency/deadlocks, feedback, last-access, harness |
| Go unit + integration | ~87 | bridge, CLI, config, sqlitecheck, walcheck, commands |
| Go e2e | ~57 | CLI workflows, recovery, WAL, environment |
| Go fuzz | 2 | bridge decoding, SQL sanitization |
| Rust benchmarks | 4 | write, FTS search, vector search, export |

### Rust test suite
```bash
# All workspace tests (unit + integration)
cargo test --workspace

# Black-box scaffold tests only (fathomdb crate)
cargo nextest run -p fathomdb

# Engine unit tests only
cargo nextest run -p fathomdb-engine

# Schema unit tests only
cargo nextest run -p fathomdb-schema

# Query unit tests only
cargo nextest run -p fathomdb-query

# Python binding tests (requires --features python)
cargo test -p fathomdb --features python
```

### Go test suite
```bash
cd go/fathom-integrity

# All unit tests
go test ./...

# E2E and testscript tests
go test ./test/e2e/...

# Corruption injection detection tests
go test -run TestCheck_Detects_ ./...

# Corruption injection repair tests
go test -run TestRepair_Fixes_ ./...
```

### Python test suite
```bash
# Build native extension and run all Python tests
cd python
pip install -e . --no-build-isolation
PYTHONPATH=python pytest python/tests -v --timeout=60

# Concurrency/deadlock tests only
pytest python/tests/test_concurrency_deadlocks.py -v

# Example harness (baseline and vector modes)
python -m examples.harness.app --db /tmp/test.db --mode baseline --telemetry off
python -m examples.harness.app --db /tmp/test.db --mode vector --telemetry off
```

### Parity and Bridge Test Coverage

These tests guard the boundary between the Rust engine and external callers
(Python bindings, Go tooling). They catch struct divergence, serialization
drift, and shape mismatches at compile time or in fast unit tests rather
than in production.

| Category | Count | What it covers |
|---|---|---|
| Report-type field-parity tests (`python_types.rs`) | 15 | Catches struct divergence between Rust report types and their Python representations at compile time |
| PyQueryStep deserialization roundtrips | 16 | Every variant including `FilterJsonBoolEq`; verifies JSON round-trip fidelity |
| WriteRequest field preservation tests | — | Confirms all `WriteRequest` fields survive serialization across the Python bridge |
| EngineError variant coverage test | 1 | Ensures every `EngineError` variant is representable in the Python binding layer |
| PyBindValue serialization coverage | — | Validates that all `BindValue` variants serialize correctly for Python consumption |
| Go bridge request JSON shape tests (`client_test.go`) | 18+ | Validates that Go-constructed JSON payloads match the shapes the Rust engine expects |

### CI Coverage

The base CI (`cargo test --workspace`) does not compile Python binding code.
The `python.yml` workflow adds a `python-rust-lint` job that runs:

```bash
cargo clippy --features python
cargo test -p fathomdb --features python
```

This catches Python binding compile errors, struct divergence, and
serialization regressions that base CI misses.

---

## Known Gaps and Open Items

The following gaps are identified in the current implementation. Each should
become a tracked task before the affected layer is considered complete.

### Existing Gaps

| Gap | Layer | Severity | Resolution |
|---|---|---|---|
| B-tree cell-count-too-low is undetectable via SQLite pragma | Layer 1 | Critical (blind spot) | Document; recommend periodic `.recover` spot checks |
| `safe_export` does not checkpoint WAL or write manifest | Layer 3 | Error | Phase 2 implementation required (see `fathom-integrity-recovery.md`) |
| Vector projection cleanup on chunk delete is deferred | Layer 2 | Warning | Implement when sqlite-vec capability gate is real |
| Restore semantics for retired rows not implemented | Layer 3 | Feature gap | Design (see `design-detailed-supersession.md` open items) |
| Durable retire event table not implemented | Layer 3 | Warning | Future `retire_events` table; currently only `provenance_warnings` |
| Read surface only returns node-shaped rows | Layer 2/3 | Feature gap | Extend `QueryRows` to runtime table result families |
| Degraded execution (FTS fallback when vector missing) | Layer 2 | Warning | Hard fail today; degraded path should be available |
| sqlite-vec e2e tests are Unix-scoped | Layer 2 | Resolved | Windows vector CI added: `rust-test-windows` runs full workspace; `python-test-windows` exercises vector harness |
| Go tests require sandbox workaround for build cache path | Layer 5 | Warning | Build cache path must be configured for sandboxed test environments |

### Gaps Identified from Historical Defect Analysis

| Gap | Layer | Severity | Resolution |
|---|---|---|---|
| No stress test for concurrent read/write at scale | Layer 9 | High | Add 10-thread stress test with integrity check at end |
| No crash-recovery test (kill mid-write, reopen) | Layer 8 | High | Add WAL recovery test with process kill simulation |
| No disk-full behavior test | Layer 8 | Medium | Add tmpfs-based disk-full write test |
| No test for 10k+ node datasets | Layer 9 | Medium | Add scale test to benchmark-and-robustness CI job |
| `rebuild_projections_fts_is_deterministic` not tested | Layer 3 | Medium | Rebuild twice, compare row counts and FTS results |
| `rebuild_projections_excludes_superseded_nodes` not tested | Layer 3 | Medium | Rebuild after supersession, verify no FTS for old nodes |
| `trace_source_does_not_bleed_across_sources` not tested | Layer 3 | Low | Two sources, trace one, assert other absent |
| `excise_source_supersedes_all_matching_nodes` not tested | Layer 3 | Low | Write batch, excise, verify all superseded |
| `excise_then_rebuild_leaves_clean_state` not tested | Layer 3 | Low | Combined operation end-to-end check |
| `replace_with_chunk_replace_then_check_semantics_clean` not tested | Layer 3 | Low | Combined replace + check end-to-end |
| Windows + Python bindings not tested in CI | Cross-cutting | Medium | Add Windows Python CI job or document as unsupported |
| macOS not tested in CI | Cross-cutting | Low | Add macOS runner or document as community-tested |
| No Rust fuzz tests (cargo-fuzz) | Layer 7 | Medium | Add fuzz targets for FTS sanitization, JSON path validation, write request parsing |
| Benchmark thresholds not enforced in CI | Cross-cutting | Medium | Gate CI on write <100ms, FTS <150ms, vector <200ms, export <500ms |
| No code coverage tracking | Cross-cutting | Low | Add tarpaulin or llvm-cov to CI; set minimum threshold |
