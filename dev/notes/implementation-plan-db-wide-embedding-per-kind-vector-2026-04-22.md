# Plan: Database-wide embedding + per-kind vector indexing

## Context

Memex wants vector search to behave like a managed FathomDB projection (parallel to FTS). Today, opening the engine with `embedder="builtin"` only wires a read-time `QueryEmbedder`. Ordinary writes do NOT populate per-kind `vec_<kind>` tables, and Python `configure_vec` stores a single global `('*', 'vec')` profile while per-kind consumers expect per-kind state. Memex can ingest `KnowledgeItem` chunks and still see empty `vec_knowledgeitem`.

Design doc: `dev/notes/design-db-wide-embedding-per-kind-vector-indexing-2026-04-22.md` (restored from commit `1ca0964`; deleted in `850a6cd` cleanup but canonical).

Outcome: one database-wide embedding identity; per-kind opt-in vector indexing; FathomDB-owned durable async projection worker; explicit `semantic_search` vs `raw_vector_search` APIs; canonical writes never blocked by embedding availability.

This plan sequences 5 design stages into orchestrator packs using TDD (red→green→refactor). Orchestrator merges; implementers commit inside worktrees.

## Existing code reuse (do NOT rewrite)

| Capability | Location | Reuse as |
|---|---|---|
| `EmbedderChoice`, `QueryEmbedder`, `BatchEmbedder` | `crates/fathomdb/src/lib.rs:83`, `crates/fathomdb-engine/src/embedder/mod.rs` | Database-wide engine resolution |
| `SchemaManager::ensure_vec_kind_profile` | `crates/fathomdb-schema/src/bootstrap.rs:1263` | Per-kind vec table creation |
| `vec_kind_table_name()` | `crates/fathomdb-schema/src/bootstrap.rs:1362` | Physical table naming |
| `RebuildActor` / `RebuildClient` / `rebuild_loop` | `crates/fathomdb-engine/src/rebuild_actor.rs` | Template for `VectorProjectionActor` |
| `regenerate_vector_embeddings_in_process` | `crates/fathomdb-engine/src/admin/vector.rs:507` | Batch embed logic for backfill |
| `WriterActor.submit` + `VecInsert` | `crates/fathomdb-engine/src/writer/mod.rs:140,385` | Apply embedded results transactionally |
| FTS durable rebuild state tables, `shape_signature` | schema bootstrap + rebuild_actor | Pattern for vector work queue |
| `projection_profiles` table | `crates/fathomdb-schema/src/bootstrap.rs` schema v1 | Continues to exist; augmented not replaced |

## Files to modify (per-pack breakdown below)

- `crates/fathomdb-schema/src/bootstrap.rs` — new schema tables + migration
- `crates/fathomdb-engine/src/admin/vector.rs` — new admin entry points
- `crates/fathomdb-engine/src/admin/mod.rs` — dispatch
- `crates/fathomdb-engine/src/vector_projection_actor.rs` — NEW actor
- `crates/fathomdb-engine/src/runtime.rs` — own actor; drop order
- `crates/fathomdb-engine/src/writer/mod.rs` — post-commit enqueue
- `crates/fathomdb-engine/src/coordinator.rs` — semantic_search plan
- `crates/fathomdb-query/src/builder.rs` — `semantic_search`, `raw_vector_search`
- `python/fathomdb/_admin.py` — `configure_embedding`, `configure_vec(kind, source)`, `get_vec_index_status`
- `python/fathomdb/_query.py` — `semantic_search`
- `typescript/packages/fathomdb/src/query.ts` — mirror
- `crates/fathomdb/tests/*` — trip-wire + scheduling tests

## Orchestrator runbook compliance

- Base from integration branch `design-db-wide-embedding-per-kind-vec` at recorded commit; worktree per pack under `.claude/worktrees/<pack-name>/`.
- Canary pack (Pack A) first; parallel packs only after canary green.
- Max 3 concurrent implementer worktrees. Implementers run in background, commit in-worktree.
- Main thread (orchestrator) merges worktree→integration branch, runs `./scripts/preflight.sh`, removes worktree.
- Reviews gate merge, not next-phase launch.
- Target tests passed per-pack in prompt; `cargo nextest run --workspace` at phase boundaries.
- Serial when `Cargo.toml`/`Cargo.lock` touched (Pack A only).

## Packs

### Pack A — Schema foundations (CANARY, serial)

Scope:
- Add `vector_embedding_profile` table (db-wide identity: profile_name, model_identity, model_version, dimensions, normalization_policy, max_tokens, active_at, created_at). Singleton constraint: only one active row.
- Add `vector_index_schemas` table (kind, enabled, source_mode, source_config_json, chunking_policy, preprocessing_policy, created_at, updated_at).
- Add `vector_projection_work` durable queue table (work_id, kind, node_logical_id, chunk_id, canonical_hash, priority, embedding_profile_id, attempt_count, last_error, state, created_at).
- Schema version bump from current version to current+1; migration from existing `vector_profiles` + `projection_profiles('*','vec')` rows preserves data.

TDD:
- Red: `crates/fathomdb-schema/tests/vector_projection_schema.rs` — assertions on CREATE, migration (from prior version to new version), singleton active-profile constraint, queue indexing by (priority DESC, created_at ASC).
- Green: add bootstrap + migration.
- Files: `crates/fathomdb-schema/src/bootstrap.rs`, new test file.

Ownership: MODIFY schema crate only. READ-ONLY others. DO NOT touch admin/vector.rs yet.

### Pack B — Admin API: database-wide embedding

Scope:
- `AdminService::configure_embedding(choice_or_engine)` — resolve, validate, persist db-wide identity row, detect identity change, mark vector-enabled kinds stale.
- `AdminService::check_embedding()` / `warm_embedding()` stubs (behavior: load + embed probe).
- Python `AdminClient.configure_embedding(...)`.
- Does NOT yet run any rebuild; only marks stale.

TDD:
- Red: `crates/fathomdb-engine/tests/configure_embedding.rs` — fresh engine persists identity; identical-config no-op; identity change marks vector-enabled kinds stale; no-rebuild-impact acknowledgement path.
- Python: `python/tests/test_configure_embedding.py`.
- Green: implement in `admin/vector.rs`.

Depends: Pack A merged.

### Pack C — Admin API: per-kind vector indexing

Scope:
- `AdminService::configure_vec(kind, source="chunks", source_config=None)` — persist `vector_index_schemas` row, call `ensure_vec_kind_profile` to materialize `vec_<kind>`, enqueue backfill work for existing canonical chunks of that kind.
- `AdminService::get_vec_index_status(kind)` returning (kind, enabled, state, pending_incremental, pending_backfill, last_error, last_completed_at, embedding_identity).
- Python mirrors. Keep legacy `configure_vec(embedder)` as deprecated shim routing to `configure_embedding`.

TDD:
- Red: `crates/fathomdb-engine/tests/configure_vec_per_kind.rs` — configure for KnowledgeItem with existing 3 chunks creates vec_knowledgeitem and enqueues 3 backfill work rows; status reflects pending counts; reconfigure is idempotent.
- Green: implement; use existing `collect_regeneration_chunks` logic refactored for per-kind iteration.

Depends: Pack A merged. Can run parallel with Pack B but touches admin/vector.rs (coordinate via file ownership: B owns `configure_embedding*`, C owns `configure_vec*` + `get_vec_index_status`).

### Pack D — VectorProjectionActor (async worker)

Scope:
- New `crates/fathomdb-engine/src/vector_projection_actor.rs` modeled after `rebuild_actor.rs`: single thread, mpsc channel, durable queue read/write.
- Scheduler: drain incremental batch → one bounded backfill slice → re-check. Policy constants: `INCREMENTAL_BATCH=64`, `BACKFILL_SLICE=32`, backoff on embedder unavailable.
- Before applying embedded result: verify canonical_hash still matches; else discard/requeue.
- Apply path reuses `VecInsert` into writer.
- `EngineRuntime` owns `VectorProjectionActor`; drop order: readers → writer → vector_actor → rebuild → lock.
- Admin drain/poll hooks for tests.

TDD:
- Red: `crates/fathomdb-engine/tests/vector_projection_actor.rs` — enqueue work, drain, verify vec_<kind> row written; stale canonical (hash mismatch) discards result; embedder unavailable → canonical commits still succeed, work rows stay pending with retry metadata.
- Scheduling: seed 1000 backfill rows + enqueue one incremental; drive one tick; incremental appears in vec_<kind> before backfill drains.
- Green.

Depends: Pack A, B, C merged.

### Pack E — Write-path integration

Scope:
- `WriterActor`: after canonical commit, for each inserted/updated chunk belonging to a vector-enabled kind, enqueue high-priority work row (`priority=1000`, capture canonical_hash + active embedding_profile_id).
- Enqueue happens inside same write txn via durable table (no cross-txn drift). Notify actor via try_submit wakeup.
- If kind not vector-enabled: no-op.
- If no active embedding profile: no-op (canonical write still commits; no work enqueued).

TDD:
- Red: `crates/fathomdb-engine/tests/write_path_vector_enqueue.rs` — insert chunk into vector-enabled kind enqueues one work row with priority=1000; write to non-enabled kind enqueues zero; disabling kind then writing enqueues zero; embedder down → canonical commits, work pending.
- Green.

Depends: Pack A, D merged.

### Pack F — Query API: semantic_search vs raw_vector_search

Scope:
- Rust: `QueryBuilder::semantic_search(text, limit)` (requires db-wide embedder + kind enabled, embed at plan time) and `raw_vector_search(vec, limit)` (explicit).
- Keep `vector_search` for now with deprecation path; route natural-language strings to `semantic_search`, raw float arrays to `raw_vector_search`. Prefer hard error on ambiguous current use.
- Python + TS mirrors.
- Error surface: "kind not vector-indexed", "no embedding engine configured", "dimension mismatch", "kind catching up / degraded".

TDD:
- Red: `crates/fathomdb/tests/semantic_search_surface.rs` — end-to-end: configure embedding + configure_vec KnowledgeItem chunks; ingest chunk "Acme Corp"; drain projection; `nodes("KnowledgeItem").semantic_search("Acme", 5)` returns ≥1 hit, non-null vector_distance, was_degraded=False, no VecInsert used.
- Degradation: embedder offline → semantic_search returns empty + was_degraded=True; unified search still uses FTS branch.
- Python + TS parity tests.

Depends: Pack A–E merged.

### Pack G — Migration + deprecation + docs

Scope:
- Deprecate public `VecInsert` exposure; keep internal. Mark as admin/unsafe on any remaining public surface with identity validation.
- Deprecate ambiguous `vector_search(text)`.
- Update `dev/notes/` handoff doc summarizing what shipped per pack.
- Update Memex-facing docs to show the canonical flow.
- CHANGELOG / release notes entry.

TDD:
- Red: `crates/fathomdb/tests/memex_tripwire.rs` — the exact acceptance scenario from design doc §Acceptance Criteria + §Test Additions.
- Deprecation warnings emitted via tracing and tested.

Depends: Pack F merged.

## Phase gates

- After Pack A: `cargo nextest run -p fathomdb-schema` + preflight.
- After Packs B+C: `cargo nextest run -p fathomdb-engine configure_`; Python pytest subset.
- After Pack D: actor tests + regression (`cargo nextest run --workspace`).
- After Pack E: write-path tests + workspace regression.
- After Pack F: full workspace + Python + TS.
- After Pack G: workspace + Python + TS + memex tripwire + `./scripts/preflight.sh --baseline`.

## Scope guardrails (do NOT do in this plan)

- Do not change FTS tokenizer model or FTS rebuild actor.
- Do not support per-kind embedding engines.
- Do not implement online replacement-table swap for identity changes (in-place degraded rebuild only — design §Closed Recommendations).
- Do not require Memex to submit pre-embedded vectors.
- Do not remove per-kind `vec_<kind>` physical tables.
- Do not add cross-kind semantic_search fanout (design §Closed Recommendations: single-kind first).
- Do not add composed/recursive source modes beyond `chunks` (properties mode deferred).

## Verification (end-to-end)

1. Open fresh engine with no embedder configured; verify configure_embedding persists identity.
2. `configure_vec("KnowledgeItem", source="chunks")` creates vec_knowledgeitem, enqueues no backfill (empty DB).
3. Write chunk with text "Acme Corp" into a KnowledgeItem node.
4. Drive projection (test mode drain hook).
5. `nodes("KnowledgeItem").semantic_search("Acme", 5)` → ≥1 hit, non-null score, was_degraded=False.
6. Stop embedder, write another chunk; canonical commit succeeds; status shows pending_incremental=1.
7. Change embedding identity with rebuild-impact ack; status flips to stale; backfill drains while a new incremental write is prioritized ahead.
8. Memex SDK usage doc example: `configure_embedding → configure_vec → write → search` without VecInsert.

Commands:
- `cargo nextest run --workspace`
- `cd python && uv run python -m pytest`
- `cd typescript && npm test --workspaces`
- `./scripts/preflight.sh`

## Memory/runbook notes

- TDD required (feedback_tdd): every pack red→green; reviewer confirms red step existed.
- Orchestrator is main thread (feedback_orchestrator_thread): main conversation spawns implementer/code-reviewer subagents directly; does not nest an orchestrator subagent.
- Vector identity belongs to embedder (project_vector_identity_invariant): the db-wide embedding profile stores identity derived FROM the embedder; configs must not carry independent identity strings. New schema row stores `model_identity` copied from `QueryEmbedderIdentity`, never accepted from caller string.
