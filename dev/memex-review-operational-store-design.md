# Memex Review: fathomdb Operational Store Design

## Baseline

This review uses the earlier Memex feasibility analysis
(`memex-reply-to-memex-remodel-notes.md`) and the Memex annotations on
`architecture-note-memex-support.md` as context. Both identified operational
state support as a key adoption requirement, with the non-negotiable constraint
being that any operational surface must participate in the same backup, export,
and integrity contract as the main store.

## Overall Assessment

**The design meets Memex's needs.** The append-only mutation log with
rebuildable current-state materialization is the right architecture. Same-file
SQLite storage eliminates the split-store backup gap that caused Memex's
LadybugDB WAL corruption incidents. The two collection kinds (`append_only_log`
and `latest_state`) map directly to Memex's two classes of operational state.

The design is sound on architecture. The gaps below are implementation-level
details, not structural problems.

## Table-by-Table Mapping

### Tables that fit `latest_state` cleanly

| Memex table | record_key | Payload shape | Rows | Write freq | Fit |
|---|---|---|---|---|---|
| `connector_health` | connector name | `{status, last_check, error, tools_discovered}` | ~10-20 | every ~60s | ✅ Clean |
| `user_settings` | setting key | `{value}` | ~20-100 | rare | ✅ Clean |
| `session_context` | `"current"` (singleton) | full SessionContext JSON blob | 1 | every turn | ✅ Clean |
| `auto_ingest_sources` | source_id | `{type, name, config, enabled, last_check, last_result, created_at}` | ~5-20 | per poll cycle | ✅ Clean |

All four tables are small, keyed by a single text column, and only need
current-state reads. The `Put` operation covers all writes. No compound key
encoding needed.

`session_context` generates the most mutation history (one mutation per turn,
~20-50/day) but the payloads are bounded JSON blobs. Compaction of old session
context mutations is safe since only the latest state matters.

### Tables that fit `latest_state` with minor encoding

| Memex table | record_key | Issue | Mitigation |
|---|---|---|---|
| `tool_usage_stats` | `tool_name:keyword_matched` | Composite PK needs string encoding | Encode as `"search_knowledge:budget"` delimiter-joined key |

`tool_usage_stats` has a composite primary key `(tool_name, keyword_matched)`.
The operational store uses a single `record_key TEXT`. Encoding as a
delimiter-joined string is straightforward and unambiguous since neither field
contains the delimiter character in practice.

The `Increment` operation covers the `call_count` field. But each write also
needs to update `last_used`. Two options:

1. Use `Put` with read-modify-write: read current payload, increment count in
   application code, write updated payload. Safe because fathomdb is
   single-writer.
2. Use `Increment` for count and accept that `last_used` updates lag behind
   until the next `Put`.

Option 1 is simpler and sufficient. Memex already does read-modify-write for
this table via `INSERT...ON CONFLICT DO UPDATE SET call_count = call_count + 1,
last_used = ?`.

### Tables that fit `append_only_log` cleanly

| Memex table | record_key | Payload shape | Rows | Write freq | Fit |
|---|---|---|---|---|---|
| `audit_log` | unique ID | `{timestamp, connector, action, capability_required, granted, goal_id, session_id, result_summary, duration_ms}` | ~10K-100K+ | medium | ✅ Clean |

The audit log is pure append-only with no updates after insertion. The read
pattern (`query_audit` with optional connector/goal_id/session_id/timestamp
filters) maps to `list_operational_mutations` with `since` for the timestamp
filter.

### Tables that fit `append_only_log` with a caveat

| Memex table | record_key | Issue | Impact |
|---|---|---|---|
| `intake_log` | unique ID | Rows receive 1-5 status transitions after creation | See below |

`intake_log` is append-on-create but then receives status updates:
`received → queued → processed` (or `failed` or `dismissed`). Each status
transition is an UPDATE on the existing row.

On the operational store, this could be modeled two ways:

**Option A — `latest_state` with lifecycle key**: Key by intake ID, use `Put`
for each status transition. Current state always reflects latest status.
Mutation history preserves the full lifecycle.

**Option B — `append_only_log` with status events**: Append a new mutation for
each status change. Read current status by scanning mutations for the intake ID
in reverse order and taking the first.

Option A is cleaner. `intake_log` is really a `latest_state` collection where
each record goes through a lifecycle. The append-only-log framing is misleading
for this table — it looks append-only at the table level but has per-row
mutations.

### Tables that should NOT move to the operational store

| Memex table | Reason |
|---|---|
| `scheduled_tasks` | Durable task definitions with graph relationships (goal links, world-model task projections). The definition is a node; only the `enabled` toggle and `updated_at` cursor are high-churn. |
| `notifications` | User-visible, actionable, linked to goals. Benefit from search, provenance, and graph edges. Should be graph-native nodes. |

`scheduled_tasks` is a hybrid: the task definition (name, cron_expr,
action_type, action_data) is durable semantic state that belongs as a node.
The `enabled` toggle is high-churn bookkeeping. Possible split: task definition
as a node, enabled/cursor state as an operational `latest_state` record keyed
by task_id. But this adds complexity for a field that changes infrequently
(user-initiated toggles, not automated polling). Simpler to keep the whole task
as a node and accept the occasional supersession on toggle.

## Gaps and Issues

### Gap 1: No filtering predicates on reads

The read API provides:

- `get_operational_current(collection, record_key)` — exact key
- `list_operational_current(collection, limit, prefix_key?)` — list with prefix
- `list_operational_mutations(collection, record_key?, since?, limit)` — history

Memex queries that filter on non-key payload fields:

- `auto_ingest_sources WHERE type='rss' AND enabled=1`
- `tool_usage_stats WHERE tool_name=? ORDER BY call_count DESC`
- `audit_log WHERE connector=? AND timestamp >= ?`

**Impact**: Low for `latest_state` collections. All are small enough (< 500
rows) that listing all rows and filtering in Python is acceptable. Memex
already does this for `connector_health` (full table scan).

**Impact for `audit_log`**: Medium. The audit log can grow to 100K+ rows.
`list_operational_mutations` with `since` handles the timestamp filter, but
filtering by `connector` or `goal_id` within results requires either:

- Scanning all mutations since the cutoff and filtering client-side (expensive
  for large logs)
- Accepting that filtered audit queries are slower than today

This is the most significant read-side limitation. Memex's `query_audit`
currently uses four separate indexes (`timestamp`, `connector`, `goal_id`,
`session_id`). The operational store has one index on
`(collection_name, record_key, created_at DESC)`.

**Recommendation**: If the operational store is not going to add filtered reads
in v1, Memex should either:

1. Keep `audit_log` in a separate application-owned SQLite table (acceptable
   since audit is read-infrequently and the backup story is explicit), or
2. Accept degraded audit query performance as a v1 tradeoff, with the
   expectation that filtered reads will be added later.

Option 1 is pragmatic. Option 2 is architecturally cleaner but slower for
Memex's operator-facing audit queries.

### Gap 2: Increment + other field update atomicity

The `Increment` operation takes a single `field` and `by: i64`. Memex's
`tool_usage_stats` needs to atomically increment `call_count` AND update
`last_used` in one write.

**Impact**: Low. As noted above, `Put` with read-modify-write handles this
cleanly for single-writer systems. `Increment` is useful for pure counters but
Memex's counter tables always have companion metadata fields. Memex would use
`Put` for all `tool_usage_stats` writes and skip `Increment` entirely.

This is not a gap in the design — `Put` is the general-purpose write and
`Increment` is a convenience for the pure-counter case.

### Gap 3: Replay semantics for mixed Put/Increment streams

The design says `operational_current` is rebuilt from `operational_mutations`.
For `latest_state`, the rebuild logic must replay mutations in order. The
semantics of replaying a sequence like `Put → Increment → Increment → Put →
Increment` need to be specified:

- Does `Put` replace the entire payload?
- Does `Increment` apply a JSON path update to the current payload?
- What happens if `Increment` targets a field that does not exist in the
  current payload?

**Impact**: Low if Memex uses only `Put` (which it likely will — see Gap 2).
Medium if mixed streams are supported, because the rebuild logic must handle
edge cases.

**Recommendation**: Specify that `Put` replaces the entire payload and
`Increment` applies `json_set(payload, '$.field', json_extract(payload,
'$.field') + by)`. If the field does not exist, `Increment` initializes it to
`by`. This is deterministic and rebuildable.

### Gap 4: `last_accessed` via operational store

The architecture note identifies `last_accessed` as a separate design issue.
The operational store could handle it as a `latest_state` collection keyed by
knowledge node logical_id with payload `{last_accessed: timestamp}`.

**Write volume**: 200-1500 mutations per day (10-30 items per retrieval × 20-50
turns per day). This is manageable but generates mutation history with zero
audit value — nobody needs the history of when each knowledge item was last
accessed.

**Recommendation**: This works in v1 with aggressive compaction (e.g., keep
only latest mutation per key, compact daily). A dedicated batched-touch API
would be more efficient long-term but the operational store is a viable interim
solution.

### Gap 5: Schema validation depth

`schema_json` is described as "declarative metadata, not executable SQL." The
design does not specify:

- What schema_json contains (field names? types? required fields?)
- Whether payloads are validated against schema_json at write time
- What happens when an application evolves its payload shape

**Impact**: Low. Memex's operational payloads are simple JSON dicts with stable
shapes. Runtime validation is a nice-to-have, not a requirement. If
schema_json is documentation-only, Memex validates payloads in its own
application code before submitting writes.

**Recommendation**: Start with schema_json as documentation-only metadata. If
runtime validation is added later, it should be opt-in per collection to avoid
breaking existing writes when payloads evolve.

## What the Design Gets Right

1. **Same-file SQLite**: Eliminates the split-store backup failure mode that
   Memex has been burned by three times. This is the single most important
   design property.

2. **Atomic writes with graph/runtime writes**: A WriteRequest that includes
   both node upserts and operational writes commits atomically. This
   eliminates inconsistency windows that Memex currently tolerates (e.g.,
   scheduler task completion updating run status and task cursor in separate
   statements).

3. **Append-only canonical history with rebuildable current state**: Matches
   fathomdb's existing philosophy. If `operational_current` is corrupted,
   rebuild from mutations. Memex has dealt with corrupted derived state
   before (LadybugDB projection staleness) and the rebuild primitive is
   exactly the right recovery model.

4. **Collection-scoped retention**: `intake_log` grows ~50-200 rows per day.
   A retention policy of "keep last 1000 rows" or "purge older than 30 days"
   with dry-run preview handles growth without manual cleanup.

5. **Provenance events for admin operations**: Collection registration,
   rebuild, compaction, and purge are auditable. Normal mutations are their
   own audit trail via the mutation log. This avoids the overhead of a
   provenance event per high-frequency write while keeping admin operations
   visible.

6. **No arbitrary SQL escape hatch**: Collections are registered through a
   typed API with declared schemas. Applications cannot CREATE TABLE or run
   raw DDL. This preserves the architecture boundary.

## Summary Verdict

| Memex table | Collection kind | Works as-designed | Notes |
|---|---|---|---|
| `connector_health` | `latest_state` | ✅ Yes | Clean fit |
| `user_settings` | `latest_state` | ✅ Yes | Clean fit |
| `session_context` | `latest_state` | ✅ Yes | Compaction needed |
| `auto_ingest_sources` | `latest_state` | ✅ Yes | Clean fit |
| `tool_usage_stats` | `latest_state` | ✅ Yes | Composite key encoding, use Put not Increment |
| `intake_log` | `latest_state` | ✅ Yes | Per-row lifecycle via Put, not truly append-only |
| `audit_log` | `append_only_log` | ⚠️ Partial | Filtered reads on non-key columns are degraded |
| `scheduled_tasks` | N/A (graph-native node) | N/A | Keep as node, accept supersession on toggle |
| `notifications` | N/A (graph-native node) | N/A | Keep as node for search/provenance |

**The design meets Memex's needs for 6 of 7 candidate tables without
modification.** The audit_log case has a read-side limitation on filtered
queries that is acceptable in v1 but worth improving later. No structural
changes to the design are needed.
