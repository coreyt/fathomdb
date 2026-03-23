# Memex Gap Map Against `fathomdb` Typed-Write + Read-Execution Plans

## Scope

This note compares:

- Memex's current storage requirements, as evidenced by its SQLite schema and
  storage behavior
- the planned work described in `dev/design-typed-write.md`
- the planned work described in `dev/design-read-execution.md`

This is intentionally narrower than "does all of fathomdb solve Memex." It is a
gap map against the two planned slices named above.

## Bottom Line

`fathomdb`'s planned typed-write and read-execution work is a strong fit for
Memex's desired direction in one important area: **SQLite as canonical
authority with engine-owned derived FTS/vector projections and explicit
provenance**. That is already close to Memex's current SQLite-primary plus
rebuildable-projection model.

Those planned slices are **not yet enough** for Memex's full storage/runtime
surface. The main missing pieces are:

- broad typed write coverage for Memex's many durable record classes
- richer read result shapes than node-only rows
- explicit update/supersession/version semantics
- delete/restore/purge lifecycle semantics
- graceful degradation behavior when vector/secondary capabilities are absent
- operational admin/recovery surfaces on par with what Memex already depends on

## Memex Storage Surface

Memex currently persists much more than a knowledge graph:

- core knowledge objects: `items`, `sources`, `links`, `trails`,
  `trail_items`, `item_versions`, `embeddings`
- session/runtime state: `session_context`, `conversation_log`,
  `conversation_embeddings`, `notifications`
- operational state: `scheduled_tasks`, `task_runs`, `intake_log`,
  `user_settings`, `audit_log`, `connector_health`
- meetings: `meetings`, `meeting_recordings`, meeting link tables,
  `meeting_intelligence_artifacts`, `meeting_extraction_runs`
- planning/world model sidecars: `wm_entities`, `wm_intents`,
  `wm_knowledge_objects`, `wm_actions`, `wm_events`, `wm_tasks`,
  `wm_commitments`, `wm_goals`, `wm_action_plans`, `wm_plan_steps`,
  `wm_execution_records`, `wm_knowledge_ingest_runs`, and related tables

Evidence:

- `src/memex/migrations.py:17-244`
- `src/memex/migrations.py:293-690`
- `src/memex/migrations.py:695-1253`

## Capability Matrix

| Memex storage need | Evidence in Memex | Planned coverage in `fathomdb` | Assessment |
|---|---|---|---|
| SQLite canonical authority | `src/memex/store.py:860-866`, `src/memex/store.py:879-956` | Typed write assumes canonical rows plus derived projections inside one SQLite-backed writer path: `dev/design-typed-write.md:17-30`, `dev/design-typed-write.md:96-105` | Direct fit |
| Single-writer discipline with WAL-friendly reads | `src/memex/store.py:862-865` | Explicitly in scope: writer-thread transaction discipline and WAL-friendly reader connections: `dev/design-typed-write.md:19-29`, `dev/design-read-execution.md:22-30` | Direct fit |
| Engine-owned FTS derivation from canonical data | Memex already keeps SQLite FTS surfaces synchronized from canonical tables: `src/memex/migrations.py:177-196`, `src/memex/migrations.py:375-440` | Explicit goal: engine derives required projections; first slice derives FTS from chunks: `dev/design-typed-write.md:96-105`, `dev/design-typed-write.md:135-144` | Direct fit |
| Vector search as a derived capability, not separate authority | Memex uses SQLite as primary and rebuilds/search projections from it: `src/memex/store.py:986-1045`, `src/memex/memory/migrate_to_ladybug.py:180-202` | Design keeps vectors as projection work, initially optional backfill, with runtime capability checks on reads: `dev/design-typed-write.md:107-115`, `dev/design-read-execution.md:123-134` | Strong fit |
| Explicit provenance for later trace/excise | Memex stores provenance in `items` and many world-model tables: `src/memex/migrations.py:147-170`, `src/memex/migrations.py:795-1253` | Design makes `source_ref` first-class and "mandatory enough" for repair tooling: `dev/design-typed-write.md:88-95` | Direct fit |
| Knowledge graph-ish records: nodes, chunks, eventually edges | Memex knowledge layer already has item/link/chunk/embedding style data, split across SQLite and Ladybug projection: `src/memex/migrations.py:130-244`, `src/memex/memory/ladybug_schema.py:15-115` | First slice covers nodes + chunks only; edges are deferred to phase 3: `dev/design-typed-write.md:75-86`, `dev/design-typed-write.md:184-191` | Partial fit; edges still missing in planned slice |
| Broad durable runtime tables beyond graph data | Memex persists scheduler, meetings, intake, settings, notifications, audit, and many `wm_*` tables: `src/memex/migrations.py:293-690`, `src/memex/migrations.py:695-1253` | Design mentions `RunInsert`, `StepInsert`, `ActionInsert` but defers broader runtime coverage after the narrow slice: `dev/design-typed-write.md:79-85`, `dev/design-typed-write.md:184-191` | Missing for current planned work |
| Rich read shapes across heterogeneous record types | Memex reads many non-node records from SQLite-backed stores and APIs | Read execution intentionally starts with one narrow node-shaped result (`row_id`, `logical_id`, `kind`, `properties`) and defers wider result decoders until later: `dev/design-read-execution.md:63-79`, `dev/design-read-execution.md:107-121`, `dev/design-read-execution.md:198-204` | Missing for current planned work |
| In-place update plus append/history semantics | Memex uses both updates and history/version tables today: `src/memex/migrations.py:40-73`, `src/memex/memory/store.py:285-299`, `src/memex/memory/store.py:440-463` | Design recognizes supersession/version questions but does not resolve them yet: `dev/design-typed-write.md:117-132`, `dev/design-typed-write.md:173-183` | Missing / unresolved |
| Soft-delete, restore, purge lifecycle | Memex uses `deleted_at`, soft delete, restore, purge, and soft-deleted listing: `src/memex/migrations.py:366-369`, `src/memex/memory/store.py:401-436` | Not covered in the typed-write/read-execution designs | Missing |
| Deterministic projection rebuild from canonical state | Memex explicitly rebuilds Ladybug projection from SQLite: `src/memex/store.py:986-1045`, `src/memex/memory/migrate_to_ladybug.py:180-202` | Design direction supports this by separating canonical writes from derived projections, but the two docs do not yet define rebuild/admin APIs | Directionally aligned, but operational surface not yet provided |
| Reverse recovery / import back into canonical store | Memex can repopulate SQLite knowledge tables from LadybugDB: `src/memex/memory/lbug2sqlite_recovery.py:15-122` | Not covered in either planned slice | Missing |
| Graceful degraded operation when projection capability is unavailable | Memex degrades to SQLite-only when Ladybug is locked/unavailable and tracks degraded modes: `src/memex/store.py:883-955`, `src/memex/store.py:974-984`, `src/memex/memory/dual_write.py:58-64` | Read design prefers explicit capability error when vector support is unavailable: `dev/design-read-execution.md:123-134` | Mismatch; Memex currently wants graceful degradation more often than hard failure |
| Backup/admin orchestration around the store | Memex already has online SQLite backup and orchestrated Ladybug backup/replay: `src/memex/backup.py:26-118`, `src/memex/backup.py:135-227` | Out of scope in these two design docs | Missing |
| Path and file layout discipline for one canonical local DB | Memex centralizes path resolution for SQLite and projection stores: `src/memex/paths.py:1-42` | Consistent with `fathomdb`'s model, but not discussed in the two planned slices | Compatible, but not addressed here |

## Concrete Mapping By Memex Area

### 1. Knowledge Items, Chunks, FTS, and Vector Search

Memex area:

- `items`
- `sources`
- `links`
- `embeddings`
- chunk/search projections

Fit:

- `items`/node-like canonical records are a good conceptual fit.
- chunk-derived FTS is a good fit.
- vector as a derived capability is a good fit.

Gaps:

- `sources` and `links` are not covered in the first typed-write slice.
- Memex currently needs link reads and writes now, not after a later edge phase.
- Memex also needs lifecycle semantics on these records, not just insert-only.

Assessment:

- Best candidate area for eventual migration.
- Still requires extension before it can replace Memex's current storage layer.

### 2. Versioning, Supersession, and Deletion Lifecycle

Memex area:

- `goals_history`
- `item_versions`
- `version` / `supersedes_id`
- `deleted_at`, restore, purge

Fit:

- `fathomdb` already thinks in terms of `logical_id` vs `row_id` and
  append-oriented state.

Gaps:

- the typed-write design leaves ID generation unresolved
- append-oriented supersession support is still on the checklist
- soft delete / restore / purge semantics are absent from the planned slice

Assessment:

- This is a major gap. Memex needs explicit lifecycle policy, not just an
  eventual note that append-oriented updates will arrive later.

### 3. Scheduler, Intake, Notifications, Settings, and Audit

Memex area:

- `scheduled_tasks`
- `task_runs`
- `intake_log`
- `notifications`
- `user_settings`
- `audit_log`
- `connector_health`

Fit:

- some runtime records might map conceptually to future `RunInsert`,
  `StepInsert`, and `ActionInsert`

Gaps:

- current planned work does not define typed shapes for most of this surface
- current read execution does not define decoders for these record types

Assessment:

- Not provided by the planned work.
- Memex would need a much broader canonical runtime schema and typed API before
  this area could move.

### 4. Meetings and Meeting-Intelligence Storage

Memex area:

- `meetings`
- `meeting_recordings`
- `meeting_goal_links`
- `meeting_entity_links`
- `meeting_intelligence_artifacts`
- `meeting_extraction_runs`

Fit:

- meeting artifacts could eventually be represented as canonical typed nodes
  plus linked runtime records

Gaps:

- none of the specific storage semantics are modeled in the two planned docs
- the read model is too narrow for this data
- update/state-machine semantics are not covered

Assessment:

- Not provided by the planned work.

### 5. World-Model and Planning Sidecars

Memex area:

- `wm_entities`
- `wm_intents`
- `wm_knowledge_objects`
- `wm_actions`
- `wm_events`
- `wm_tasks`
- `wm_commitments`
- `wm_goals`
- `wm_action_plans`
- `wm_plan_steps`
- `wm_execution_records`
- `wm_knowledge_ingest_runs`

Fit:

- conceptually aligned with `fathomdb`'s broader aim: typed canonical state with
  provenance and graph-friendly structure

Gaps:

- the current typed-write slice is far too narrow
- the current read-execution slice is far too narrow
- Memex needs queries across many typed record families, not just node rows

Assessment:

- Strong conceptual fit, but large implementation gap.
- This area likely needs a dedicated schema/result-model design in `fathomdb`,
  not just incremental extension from nodes/chunks.

### 6. Recovery, Rebuild, and Degraded Modes

Memex area:

- rebuild projection from canonical SQLite
- recover knowledge back into SQLite from a secondary store
- continue operating when the secondary capability is locked/unavailable

Fit:

- provenance discipline and canonical-vs-derived separation are the right
  foundations

Gaps:

- explicit rebuild/recover/admin APIs are not in these two planned slices
- vector capability is modeled as an execution error, whereas Memex often wants
  a degraded but still-usable path

Assessment:

- Foundations: yes
- Memex-grade operational behavior: not yet

## What `fathomdb` Would Need Before Memex Could Realistically Adopt It

### Must-Have Extensions

1. Expand typed writes well beyond `NodeInsert` and `ChunkInsert`.
   Minimum practical surface for Memex would include:
   - edges/relationships
   - update/supersession operations
   - soft-delete and restore operations
   - typed runtime records for scheduler/intake/meeting/planning state

2. Expand read results beyond node rows.
   Memex would need:
   - typed decoders for multiple record families
   - join-capable result paths for runtime tables
   - enough diagnostics to debug planner/runtime behavior

3. Define lifecycle semantics.
   Memex needs explicit answers for:
   - ID generation
   - updates vs append-only replacement
   - version history
   - supersession
   - deletion and purge

4. Define degraded-mode behavior.
   For Memex, missing vector capability should often mean:
   - continue with FTS/structured retrieval
   - report degraded capability
   not simply fail the read.

5. Add operational/admin surfaces.
   At minimum:
   - deterministic projection rebuild
   - backup/export/import story
   - recovery/excision hooks that match the provenance model

### Nice-To-Have Extensions

- richer support for temporal/runtime record queries
- compatibility layer for Memex's existing world-model sidecars
- migration tooling from Memex's current SQLite schema into `fathomdb`'s
  canonical schema

## Recommended Framing

The best current framing is:

- `fathomdb` is a **good architectural target** for Memex's knowledge-storage
  direction.
- the specific typed-write and read-execution slices are **not yet a sufficient
  storage substrate** for Memex as a whole.
- the fastest path to Memex usefulness would be:
  1. finish typed edges plus update/supersession semantics
  2. widen read results beyond node rows
  3. define graceful degraded-mode behavior
  4. define runtime-table coverage for at least tasks/runs/intake and one
     planning slice

## Verdict By Storage Area

### Good Fit Now

- canonical SQLite authority
- single-writer plus WAL-reader discipline
- engine-owned FTS derivation
- provenance-first canonical writes
- projection-first vector strategy

### Fits With Extension

- item/source/link knowledge storage
- typed world-model entities and relationships
- planning/runtime action records
- deterministic projection rebuild

### Not Yet Provided By The Planned Work

- broad runtime-table persistence
- rich heterogeneous read models
- deletion/restore/purge lifecycle
- explicit supersession/version semantics
- Memex-style degraded capability behavior
- Memex-grade operational backup/recovery/admin flows
