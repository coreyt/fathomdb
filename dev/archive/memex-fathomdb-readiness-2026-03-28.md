# Memex–fathomdb Readiness Assessment (2026-03-28)

## Gap Closure: Final Scorecard

Every substrate-level gap Memex identified across the three review documents
is now closed.

### From the original feasibility analysis (`memex-reply-to-memex-remodel-notes.md`)

| Gap | Phase | Status |
|---|---|---|
| Cross-WriteRequest transactions | N/A | **Mitigated** (unchanged — single-WriteRequest batching is sufficient) |
| Vector cleanup on retire | N/A | **Closed** (before this work) |
| Restore/purge semantics | Phase 1 | **Closed** |
| JSON substring search | N/A | **Application responsibility** — Memex emits searchable content as chunks |
| Operational scheduler bookkeeping | Foundation | **Closed** |

### From the architecture note (`architecture-note-memex-support.md`)

| Gap | Phase | Status |
|---|---|---|
| A: Restore/purge lifecycle | Phase 1 | **Closed** |
| B: Rich read models | Phase 3 | **Closed** |
| C: Write-composition helpers | Phase 2 + foundation | **Closed** |
| D: Operational-state modeling | Foundation | **Closed** |
| E: Stale vector cleanup docs | Phase 1 (carried) | **Closed** |

### From the design review (`memex-review-operational-store-design.md`)

| Gap | Phase | Status |
|---|---|---|
| 1: Filtered reads | Phase 5 | **Closed** |
| 2: Increment atomicity | N/A | **Dissolved** (Put covers all Memex cases) |
| 3: Mixed Put/Increment replay | N/A | **Dissolved** |
| 4: last_accessed | Phase 4 | **Closed** |
| 5: Schema validation | Deferred | **Accepted** — non-blocking, payloads validated application-side |

## Verification Against Memex's Specific Requirements

### Phase 1 (restore/purge) — the former #1 blocker

Memex's architecture note annotations made 6 specific demands. All are met:

1. "Restore must bring back edges retired in the same operation" — restore
   re-establishes directly related retired edges in scope
2. "Restore must return a report" — scope reporting implemented
3. "Purge must cascade to edges touching the purged node" — explicit bounded
   cascade across edges, chunks, FTS, vec
4. "Purge must leave a tombstone" — provenance-visible with scope reporting
5. "Purge must be no-op on active/restored nodes" — explicitly a deterministic
   no-op
6. "Restore during grace period must prevent subsequent purge" — follows from
   #5: restored node is active, so purge is no-op

### Phase 3 (rich reads)

Memex asked for "root node + grouped neighbors by edge kind." The
implementation is more general: expansion slots rather than edge-label
buckets. This means the current DSL and a future Cypher-style frontend can
share the same result model. The result shape supports exactly the pattern
Memex described (root + grouped context), while being more composable than
what was requested.

Timestamp and numeric predicates are confirmed. Search enrichment is
confirmed.

### Phase 4 (last_accessed)

The design decision to use a dedicated `node_access_metadata` table rather
than operational-store backing is the right call. This avoids the
join-at-read-time problem flagged in the design review. `last_accessed_at`
surfaces as a separate optional field on node reads — Memex's retrieval code
can consume it directly without a side-channel query.

The touch batch validates that all logical IDs are active before applying — no
silent writes to nonexistent nodes. Provenance mode is respected. Cleanup on
purge and excision prevents orphaned metadata rows.

### Phase 5 (filtered reads)

This was the only partial entry in the Memex table mapping (`audit_log`
filtered reads degraded). Now closed. The implementation uses engine-maintained
extracted filter values for declared fields, which means the engine indexes
payload fields at write time. This is materially better than client-side
filtering — `audit_log` reads with connector/goal_id/session_id/timestamp
filters now work at the engine level without arbitrary SQL.

The explicit post-upgrade path for preexisting collections means Memex can
declare filter contracts on existing collections without re-ingesting data.

## Notable Design Decisions

Three locked decisions that deserve attention:

1. **Restore uses durable provenance ordering, not timestamp equality** —
   addresses the edge case where multiple retires happen in the same second.
   This was not something Memex flagged explicitly, but it is the kind of
   correctness detail that prevents subtle bugs in the forget/undo workflow.

2. **Disabled collection write rejection checked inside the transaction** —
   prevents a TOCTOU race between preflight check and actual write. This is
   the kind of hardening that makes operational-store state transitions
   reliable under concurrent admin operations.

3. **Phase 5 preserves `trace_operational_collection` unchanged** — the
   diagnostic/history API is kept stable while `read_operational_collection`
   adds the filtered read surface. Existing admin tooling (trace, excise) is
   not affected by adding filtered reads.

## Remaining Deferred Items

Only three items remain deferred, all correctly scoped as non-blocking:

- Optional operational payload schema validation
- Collection-declared secondary indexes beyond filtered reads
- Automatic/background retention execution

None of these block Memex adoption. The Memex table-to-collection mapping now
has all 7 candidate tables at full coverage, including `audit_log` which was
previously partial.

## Memex Table-to-Collection Mapping (final)

| Memex table | Collection kind | Status | Notes |
|---|---|---|---|
| `connector_health` | `latest_state` | ✅ | Clean fit |
| `user_settings` | `latest_state` | ✅ | Clean fit |
| `session_context` | `latest_state` | ✅ | Compaction needed |
| `auto_ingest_sources` | `latest_state` | ✅ | Clean fit |
| `tool_usage_stats` | `latest_state` | ✅ | Composite key as delimiter-joined string; use Put not Increment |
| `intake_log` | `latest_state` | ✅ | Per-row lifecycle via Put |
| `audit_log` | `append_only_log` | ✅ | Filtered reads now engine-supported via Phase 5 |
| `scheduled_tasks` | N/A | N/A | Graph-native node; accept supersession on toggle |
| `notifications` | N/A | N/A | Graph-native node for search/provenance/edges |

## Bottom Line

The substrate gaps that motivated this investigation are fully addressed.
The path from Memex's current datastore to fathomdb is no longer blocked by
missing primitives — it is an application-level migration problem.
