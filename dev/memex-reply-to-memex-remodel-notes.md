# Feasibility Analysis: Memex → fathomdb Migration

## Executive Summary

**Yes, it is feasible — in phases, not as a big-bang replacement.**

fathomdb's primitives match Memex's world-model direction closely. The strongest signal is that Memex already designed its ontology (the `wm_*` family) in a graph-native shape. Moving those families to fathomdb is largely mechanical. The remaining surfaces range from "straightforward design work" to "real gaps that require workarounds or deferred for later."

---

## Dimension 1: Core Fit — What Maps Cleanly

| Memex surface | fathomdb primitive | Fit |
|---|---|---|
| `wm_goals`, `goals_history` | `goal` node kind + supersession | ✅ Excellent |
| `wm_entities`, `wm_entity_attributes` | `entity`/`entity_attribute` node kinds | ✅ Excellent |
| `wm_action_plans`, `wm_plan_steps`, `wm_execution_records`, `wm_plan_step_transitions` | node kinds + edges | ✅ Excellent |
| `wm_provenance_links` | fathomdb edges | ✅ Exact match |
| `GoalEntityLink`, `MeetingGoalLink` join tables | edges | ✅ Better than current |
| `goals_history`, `item_versions` | logical_id versioning + supersession | ✅ Replaces cleanly |
| `wm_semantic_mappings`, `wm_tasks`, `wm_commitments` | node kinds | ✅ Excellent |
| `wm_knowledge_ingest_runs` | `knowledge_ingest_run` node kind | ✅ Excellent |
| `meetings`, `meeting_recordings`, `meeting_intelligence_artifacts`, `meeting_extraction_runs` | node kinds + has_recording/produced_artifact edges | ✅ Graph-native already |
| `items_fts`, `conversation_log_fts`, LadybugDB vectors | fathomdb chunks + FTS5 + vec_nodes_active | ✅ Replaces entirely |
| `task_runs` status transitions | fathomdb `runs`/`steps`/`actions` | ✅ Strong fit |
| `parent_id` goal tree, `blocked_by_goal_id` | edges + recursive CTE traversal | ✅ Better than current |

The `wm_*` family — 20+ tables representing ~60% of Memex's semantic state — maps to fathomdb with almost no conceptual translation. The 0.8 migration already did the ontology design work; this would be expressing that design in fathomdb's primitives instead of SQLite sidecar tables.

---

## Dimension 2: Partial Fits — Require Design Work

**Conversation log (`conversation_log`, `conversation_embeddings`)**

Append-only, high-volume (every turn). Mapping as `conversation_turn` node kinds with chunks for FTS/vector is correct. The concern is write volume — every turn would be a `NodeInsert` + `ChunkInsert` + `VecInsert`. Retention policy (fathomdb has no TTL/expiry primitive today) would need application-layer cleanup via retire + periodic admin excise.

**Knowledge items (`items` table + LadybugDB)**

Currently split: SQLite for relational fields, LadybugDB for graph/vector. Moving to fathomdb unifies both — items become `knowledge` node kinds with chunks for FTS/vector. This **eliminates LadybugDB entirely**, removing the dual-write coordination layer and backup complexity that caused the 3 WAL corruption incidents. Strong motivation.

The half-life decay scoring (`exp(-ln(2) * days_since_access / half_life_days)`) lives in application code and stays there — fathomdb doesn't replace ranking logic, only storage.

**`last_accessed` tracking**

Memex updates `last_accessed` on every retrieval hit. On fathomdb this would require a `NodeInsert` (upsert) per accessed node. Write-on-read is expensive. Possible mitigations: batch updates, accept eventual consistency for access timestamps, or track separately in a lightweight SQLite table.

**Singleton blobs (`session_context`, `human_partner_profile`, `agent_self_model`)**

Trivially modeled as nodes with stable logical IDs and upsert semantics. No real challenge.

---

## Dimension 3: Gaps — Real Blockers or Workarounds Needed

**Gap 1: No cross-WriteRequest transactions**

fathomdb guarantees atomicity within a single `WriteRequest`. There is no multi-request transaction. Memex has complex multi-step mutations, e.g.:

```
create scheduled_task
→ project to wm_task
→ insert wm_provenance_links (multiple rows)
→ update wm_action_plan.selected_step_id
```

**Mitigation**: In practice, most of these can be batched into a single `WriteRequest` containing multiple `NodeInsert`s and `EdgeInsert`s. fathomdb's single-writer discipline means the batch is atomic. The main exception is sequences where write B depends on a DB-assigned ID from write A — fathomdb requires caller-provided IDs, so Memex generates UUIDs before submitting, eliminating that dependency.

**Gap 2: Vector cleanup on node retire is deferred**

fathomdb explicitly defers atomic vector cleanup when a node is retired (only FTS/chunks are cleaned). If Memex retires a knowledge node (`forget` operation), stale vector rows remain until a separate admin operation.

**Mitigation**: Memex already has a scheduled hard-purge workflow. The same scheduler slot can run `check_semantics` + vector cleanup. This is a manageable operational workaround, not a fundamental blocker.

**Gap 3: No restore or purge semantics**

fathomdb has `retire` (soft-delete) but no `restore` (un-retire) and no `purge` (permanent delete). Memex has a `forget` tool with soft-delete → grace period → hard cascade delete across items, chunks, embeddings, links.

**Mitigation**: The forget grace period already exists as a scheduled task. fathomdb's `AdminService::excise_source` provides bulk delete by source_ref. Targeted excise by `logical_id` would need to be added, or handled by an admin operation that Memex calls explicitly. This is a substrate gap that needs a fathomdb-side addition or a workaround using source_ref tagging.

**Gap 4: JSON substring search**

Memex uses `LIKE '%term%'` on `payload_json` fields across `wm_knowledge_objects` and others. fathomdb's query builder exposes `filter_json_text_eq` (equality only). Substring search in properties would fail.

**Mitigation**: All searchable payload content goes into chunks. This is actually the correct design — FTS search over chunks instead of LIKE on JSON. Requires Memex to identify which payload fields need to be searchable and emit them as `ChunkInsert`s at write time. Non-trivial but structurally correct.

**Gap 5: Operational scheduler bookkeeping**

`auto_ingest_sources`, `intake_log`, `connector_health`, `tool_usage_stats` — these are operational plumbing tables with specific access patterns (check last_check, update status, query recent). They can be modeled as nodes, but the overhead of node versioning for something like `connector_health` (updated every health check) may not be worth it.

**Mitigation**: These are the explicit "later or never" candidates from the remodel notes. Keep them in a thin SQLite store alongside fathomdb for as long as needed.

---

## Dimension 4: Migration Path Assessment

The remodel-notes 3-phase cutover is sound. Here's the feasibility of each:

**Phase 1 — `wm_*` families as fathomdb node/edge kinds**

Feasibility: **High**. These tables have no external consumers that would break. The migration is additive: dual-write to both SQLite `wm_*` tables and fathomdb simultaneously, validate read parity, then flip reads to fathomdb and retire the `wm_*` tables. All 20 `wm_*` tables can move without changing any product surface.

Risk: Low. These tables are already sidecar-style — they don't drive any preserved operator-visible behavior directly.

**Phase 2 — Meetings, knowledge items, conversation turns**

Feasibility: **Medium-high**. The meeting pipeline is graph-native. Knowledge items lose LadybugDB (win). Conversation turns are high-volume but structurally simple.

Risk: Medium. The chunk strategy for FTS/vector needs careful design. `last_accessed` tracking write volume needs a plan. Eliminating LadybugDB removes a known operational pain point but requires migration of existing knowledge content.

**Phase 3 — Operational telemetry (notifications, audit, connector health)**

Feasibility: **Medium**. Possible but not obviously worth it. Notifications and audit are durable state that _could_ benefit from provenance/search. Connector health and tool usage stats are polling-heavy state that generates write noise.

Risk: Low (these are isolated tables). Motivation: Low unless unified search across operational + semantic state is a product goal.

---

## Dimension 5: What fathomdb Gains from This Migration

This matters because fathomdb is actively developed. Taking on Memex as a real client would drive:

1. **Purge semantics** — needed for `forget` tool
2. **Vector cleanup on retire** — currently deferred, would need to close
3. **Richer read models** — multi-table join results (e.g., goal + its provenance links + its entity links in one query)
4. **Generic operational state extensibility** — the 3 runtime tables (runs/steps/actions) cover Memex's scheduler but not notifications/connector health cleanly
5. **`last_accessed` without write-on-read** — a property update path that doesn't trigger full supersession

---

## Overall Verdict

| Area | Feasibility | Risk | Priority |
|---|---|---|---|
| `wm_*` families (Phase 1) | Very high | Low | Do first |
| Meeting + artifacts | High | Low-medium | Phase 2 |
| Knowledge items (replaces LadybugDB) | High | Medium | Phase 2 |
| Conversation turns | Medium | Medium | Phase 2 |
| Operational tables (notifications, etc.) | Medium | Low | Phase 3 / optional |
| Scheduler internals | Medium | Medium | Phase 3 |
| Full big-bang replacement | Low | High | Never |

**The remodel-notes conclusion is correct**: the feasible path is promoting the `wm_*` ontology into fathomdb-defined node/edge kinds, moving knowledge and meeting data to use fathomdb's chunk/FTS/vector layer (retiring LadybugDB), and keeping thin operational state in SQLite for the foreseeable future.

The main work is not conceptual — fathomdb's primitives align well. The main work is:
1. Chunk strategy for every text-bearing node kind (what goes in chunks vs. properties)
2. Handling `last_accessed` write volume without thrashing supersession
3. Working around the missing purge semantic until fathomdb adds it
4. Mapping the scheduler's retry/queue bookkeeping onto fathomdb's `runs`/`steps`/`actions` tables

None of these are blockers. They are design problems with known solution patterns.
