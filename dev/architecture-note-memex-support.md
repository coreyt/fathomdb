# Architecture Note: Memex Support Without Expanding Domain-Specific Engine Schema

## Purpose

This note records the gaps surfaced by Memex during the `fathomdb` migration
discussion and proposes `fathomdb` changes that would make Memex a strong
client without violating the current architecture boundary:

- engine-owned typed tables stop at `runs`, `steps`, and `actions`
- application concepts such as goals, meetings, scheduling artifacts,
  notifications, and knowledge objects remain application-defined node/edge
  kinds or application-owned adjunct state

That boundary is permanent in `dev/ARCHITECTURE.md` and `.wake/constraints.md`.
The goal here is therefore not "add Memex tables to the engine." The goal is to
grow generic primitives, lifecycle support, and read/write ergonomics so Memex
can model more of its world on top of `fathomdb` safely.

<memex_response>
Agreed. Memex has no interest in pushing its product schema into the engine.
The `wm_*` ontology was designed as application-level semantics on top of a
storage substrate, and expressing those semantics as application-defined node
kinds and edge kinds on fathomdb is exactly the right framing. The architecture
boundary as stated here is correct and Memex would not benefit from violating
it — freezing Memex's current schema into the engine would make both projects
harder to evolve independently.
</memex_response>

## Context

The recent Memex notes establish three important points:

1. Memex's `wm_*` ontology already maps cleanly onto `fathomdb` nodes, edges,
   chunks, provenance, and supersession.
2. The main remaining issues are not conceptual graph mismatch. They are
   lifecycle, query/read-model, and write-composition gaps.
3. Some earlier Memex analysis is already stale. In particular, vector cleanup
   on node retire is no longer deferred in current `fathomdb`.

The current writer transaction already:

- applies one atomic `WriteRequest` across nodes, edges, chunks, runs, steps,
  actions, optional projection tasks, and vec inserts
- removes FTS, chunks, and vec rows on node retire
- removes vec rows on replace-upsert when chunk replacement is requested

That means the relevant work now is mostly:

- better lifecycle APIs
- better generic read/query surfaces
- better write-composition helpers
- better operational-state modeling support
- correcting stale documentation

<memex_response>
Point 3 is accepted — the earlier Memex feasibility analysis (Gap 2 in
memex-reply-to-memex-remodel-notes.md) described vector cleanup on retire as
deferred. That was based on the state of the code and design docs at the time
of the review. If the writer now atomically removes FTS, chunk, and vec rows
on retire and on replace-upsert with chunk replacement, then that gap is
closed and Memex's earlier mitigation discussion around scheduled
check_semantics + vector cleanup for retire is no longer needed.

This also strengthens the Phase 2 feasibility assessment for knowledge items.
The earlier analysis rated knowledge item migration as "High feasibility,
Medium risk" partly because of concern about stale vector rows after forget
operations. With atomic vec cleanup on retire, the risk drops — forget via
retire is now a clean single-WriteRequest operation with no orphaned
projections.

The reframing from "conceptual graph mismatch" to "lifecycle, query, and
write-composition gaps" matches Memex's own conclusion. The ontology design
work is done. What remains is substrate ergonomics.
</memex_response>

## Memex-Identified Gaps

### 1. Restore and purge lifecycle semantics are incomplete

Memex needs explicit support for:

- soft-delete / retire
- restore during a grace period
- hard purge / forget after a grace period
- auditability around the full lifecycle

`fathomdb` already supports replace/retire and source-based excision, but it
does not yet expose a complete generic lifecycle contract for targeted restore
and purge.

<memex_response>
This is Memex's most pressing substrate gap. The `forget` tool is a
user-facing product feature with a specific contract:

1. User says "forget X" — Memex finds matching knowledge items
2. Items are soft-deleted (currently `deleted_at` timestamp on the item row)
3. A 1-hour grace period begins (user can undo via the attention drawer)
4. After the grace period, a scheduled task hard-purges: deletes the item row,
   all chunks, all embeddings, all links, all entity mentions, and all
   trail_items that reference it

The cascade is the hard part. Today Memex does this with explicit multi-table
DELETE statements in a single SQLite transaction. On fathomdb, the equivalent
would be:

- retire the knowledge node (soft-delete, already supported)
- restore the knowledge node if the user undoes within the grace period (not
  yet supported)
- purge the knowledge node after the grace period, cascading to all chunks,
  FTS rows, vec rows, and edges that reference it (not yet supported as a
  single admin operation)

The purge cascade is the critical piece. Memex cannot leave orphaned edges
pointing at a purged node — that creates dangling references that corrupt
provenance walks and drill-in queries. A purge operation that only removes the
node row but leaves edges, chunks, and vec rows behind would be worse than the
current Memex implementation.

Memex would also need purge to be auditable. The `forget` tool is a
destructive operation on personal data. Memex logs it in its audit table
today. A fathomdb purge that leaves a tombstone or provenance_event recording
what was purged, when, and by what source_ref would satisfy the auditability
requirement.
</memex_response>

### 2. Rich read models are still too node-shaped

Memex often wants a query result closer to:

- a node plus linked provenance edges
- a node plus related entities
- a node plus neighboring context
- a search result plus relevant linked state

The current substrate is structurally capable of this, but the exposed
query/builder ergonomics are still relatively low-level and node-first.

<memex_response>
Concrete Memex examples of read patterns that are currently multi-query or
require application-side assembly:

**Goal drill-in**: When the user asks about a goal, Memex shows the goal plus
its parent chain, blocked-by relationships, linked entities, linked meetings,
active plan with steps, and recent execution records. Today this is 6-8
separate SQL queries across `goals`, `wm_goals`, `wm_provenance_links`,
`wm_action_plans`, `wm_plan_steps`, `wm_execution_records`, and
`meeting_goal_links`. On fathomdb, the ideal would be a single query that
returns the goal node plus a bounded traversal of its outgoing edges and their
target nodes, grouped by edge kind.

**Retrieval context building**: When Memex prepares context for an LLM turn,
it runs FTS + vector search, then for each matching knowledge node, looks up
typed knowledge metadata, entity links, and provenance. Today this is a search
query followed by N individual lookups. The ideal would be a search that
returns matched nodes plus selected edge metadata in one compiled plan.

**Meeting detail view**: A meeting node plus its recordings, artifacts,
promoted goals, attendee entities, and extraction run history. Currently 5
separate queries. Would benefit from a single traversal rooted at the meeting
node.

The common pattern is: root node + bounded 1-2 hop traversal + edge kind
filtering + property projection on the neighbors. If the query builder could
express this as a single compiled query returning a structured result (root
node + grouped neighbor sets), it would eliminate most of Memex's multi-query
assembly code.

Memex does not need arbitrary-depth graph traversal. Bounded traversal (1-2
hops) with edge-kind filtering covers every current read pattern.
</memex_response>

### 3. Atomic writes exist, but composing them is still too manual

A single `WriteRequest` is already atomic and broad enough for most Memex write
flows. The real gap is the ergonomics of building that request correctly when a
client needs to submit a cluster of related nodes, edges, chunks, and runtime
rows in one shot.

<memex_response>
Agreed. The single-WriteRequest atomicity is sufficient — Memex does not need
cross-request transactions. The pain point is assembly.

A representative Memex write flow that would benefit from composition helpers:

**Meeting transcription completes** — one logical mutation that today spans:
1. Upsert the meeting node (update state, transcribed_at, segments)
2. Upsert the meeting_recording node (update state, segments, duration)
3. Create N meeting_artifact nodes (transcript segments, action items, notes)
4. Create edges: meeting → recording, meeting → each artifact, artifact →
   recording
5. For each action item with an assignee: create a commitment node, create
   edges commitment → meeting and commitment → goal
6. Create chunks for each artifact's text content
7. Create a meeting_extraction_run node with edges to the meeting and recording

Today Memex builds this as a series of INSERT statements across 5+ tables.
On fathomdb, this is one WriteRequest with ~20-40 inserts. Building that
request correctly — generating IDs, wiring edges to the right logical_ids,
attaching chunks to the right nodes — is where ergonomic helpers would pay
off.

The request-local ID/alias system proposed below would directly address this.
Memex generates UUIDs client-side already (via `uuid4()`), so the
caller-provided ID model is not a problem. The issue is readability and
correctness when assembling a large request with many cross-references.
</memex_response>

### 4. Operational-state support is underdefined

Memex has state such as:

- intake/replay state
- connector-health snapshots
- queue/retry bookkeeping
- notifications / operator-visible state
- usage/telemetry counters

Some of this maps well onto nodes/edges plus `runs`/`steps`/`actions`. Some of
it looks more like classic SQLite operational tables with update-heavy access
patterns. The architecture needs a clearer answer for which should live inside
`fathomdb` primitives and whether there is any safe adjunct "operational table"
primitive worth supporting.

<memex_response>
Memex can concretely sort its operational state into the two categories
proposed below:

**History-bearing (graph-native fit):**
- `notifications` — durable, user-visible, searchable, auditable. These have
  action_type/action_data payloads and link to goals. They are already
  graph-shaped and would benefit from provenance edges. Node kind is correct.
- `intake_log` — lifecycle records for ingested content. Already has
  source/action_type/status/timestamps. Maps directly to nodes or to
  runs/steps/actions.
- `task_runs` (execution history) — the completed/failed run history is
  auditable provenance. Maps to fathomdb runs.
- `audit_log` — append-only event stream. Could be nodes or provenance_events.

**High-churn bookkeeping (poor supersession fit):**
- `connector_health` — updated every health check cycle (~60s). 6-8 rows,
  each updated in place. Full node supersession would create ~1440
  superseded rows per connector per day. This is the clearest example of
  state that should not be modeled as superseding nodes.
- `tool_usage_stats` — counter increments on every tool call. Same problem.
- `auto_ingest_sources` — `last_check` and `last_result` updated on every
  poll cycle. Same problem.
- `scheduled_tasks` — the task definition is durable (node-worthy), but the
  `enabled` toggle, `last_run` cursor, and cron tick state are high-churn
  mutable fields that would generate write amplification under supersession.
- `task_runs` (queue state) — the queued/running/retrying status transitions
  are high-frequency operational bookkeeping. The runs/steps/actions runtime
  tables handle this, but only if upsert-based status transitions are cheap.

The split proposed here — graph-native for history-bearing state, adjunct
operational tables for high-churn bookkeeping — matches Memex's needs. Memex
would use fathomdb nodes/edges for the first category and either fathomdb's
adjunct operational tables (if they exist) or a co-resident SQLite file for
the second.

The key requirement for the adjunct path is that fathomdb's backup/export/
integrity tooling must include those tables. Memex has been burned by
split-store backup gaps before (the LadybugDB WAL corruption incidents
happened partly because backup tooling did not cover the shadow store
correctly). Any adjunct operational surface must participate in the same
backup and recovery contract as the main store.
</memex_response>

### 5. Documentation around vector cleanup is stale

Earlier notes described vector cleanup on retire as deferred. That is no longer
accurate. The writer now deletes vec rows in the same transaction as retire and
replace-with-chunk-replacement flows.

<memex_response>
Acknowledged. The earlier Memex feasibility analysis (Gap 2 in
memex-reply-to-memex-remodel-notes.md) should be considered retracted on this
point. The statement "fathomdb explicitly defers atomic vector cleanup when a
node is retired" was accurate at the time of review but is now stale.

Memex will update its own gap tracking to reflect that vector cleanup on
retire and replace-with-chunk-replacement is implemented. The remaining
vector lifecycle questions from Memex's perspective are:

- Does purge (once implemented) also clean up vec rows? (Expected yes, but
  needs confirmation when purge ships.)
- Does excise_source clean up vec rows for all affected nodes? (Same
  question.)
- If vectors are regenerated from an embedding contract after a node is
  retired, does the regeneration skip retired nodes? (Expected yes, but worth
  confirming.)

These are follow-up verification items, not blockers.
</memex_response>

## Proposed Changes

### A. Add generic restore/purge lifecycle APIs

This is the highest-value substrate addition.

`fathomdb` should grow explicit lifecycle/admin operations such as:

- `restore_logical_id(logical_id)`
- `retire_logical_id(logical_id, reason/source_ref)`
- `purge_logical_id(logical_id, mode)`
- `purge_source(source_ref, mode)`
- retention-aware helpers for "retire now, purge after policy window"

These should be generic, not Memex-shaped.

<memex_response>
Strongly agreed. This is the single most important fathomdb addition for
Memex adoption.

Two notes on the proposed API surface:

1. `restore_logical_id` needs to handle the case where the node was retired
   and its edges were also retired as part of the same or a subsequent
   operation. Memex's forget workflow retires the knowledge node and would
   expect edges to also be cleaned up. If restore brings back the node but
   not its edges, the restored node is orphaned. Memex would need either:
   - restore to also restore edges that were retired in the same
     source_ref-tagged operation, or
   - restore to return a report of what was restored and what was not, so
     Memex can reconstruct edges explicitly.

   The first option is cleaner for Memex. The second is more generic.

2. `purge_logical_id` with cascade to edges is essential. A purged node with
   surviving edges creates dangling references. Memex's provenance walks
   (goal → plan → step → execution → action) would break if any node in the
   chain was purged but its inbound/outbound edges remained. The cascade
   scope should be clearly documented: does purge remove only edges where
   the purged node is source or target, or does it also remove edges-of-edges?
   For Memex, one hop of edge cascade (remove edges directly touching the
   purged node) is sufficient.
</memex_response>

#### Lifecycle contract requirements

1. Restore must be explicit and auditable.
2. Purge must define exactly what happens to:
   - restorable retired state
   - active and superseded canonical rows
   - chunks
   - FTS rows
   - vec rows
   - provenance events
   - dependent edges and runtime references
3. Restore must re-establish full pre-retire content, not only reactivate the
   logical row. This requires preserving or otherwise deterministically
   restoring the retired content/projection state needed to bring back the
   pre-retire node, chunks, FTS rows, vec rows, and directly related edges.
4. Purge must be recoverability-aware:
   - no silent hard deletes without audit
   - clear scope reporting before mutation
   - deterministic post-purge integrity state
5. Excision, restore, and purge semantics must compose cleanly.

<memex_response>
All five requirements are correct from Memex's perspective.

On requirement 2, "dependent edges and runtime references" — Memex
specifically needs clarity on whether purging a node that is referenced by a
run/step/action (via source_ref or properties) also affects those runtime
rows. Memex would prefer that runtime rows are NOT purged when a knowledge
node is purged — the execution history of "we ingested this, then forgot it"
is valuable audit context even after the content itself is gone. The
tombstone or provenance_event from the purge provides the link.

On requirement 3, "clear scope reporting before mutation" — a dry-run or
preview mode for purge would be valuable. Memex's forget tool already shows
the user what will be deleted before confirming. If fathomdb's purge can
return a scope report (N nodes, M edges, K chunks, J vec rows to be removed)
without executing, Memex can surface that to the user.

On requirement 4, the composition that matters most for Memex is:
retire → (grace period) → purge. This is the forget lifecycle. If a node is
retired, then restored during the grace period, then the scheduled purge
fires — purge must be a no-op on active (restored) nodes. The grace-period
window is application-owned (Memex manages the timer), but the substrate
must not purge a node that was restored between retire and purge.
</memex_response>

#### Design direction

Prefer admin-driven lifecycle operations over ad hoc client-issued raw deletes.
The admin path is where recoverability, provenance, and deterministic cleanup
already belong.

`purge` should probably support at least two modes:

- `redact_content_keep_audit`: remove content-bearing rows/projections but keep
  a bounded audit trail
- `hard_purge_with_tombstone`: remove canonical/projection rows while leaving a
  minimal tombstone proving the purge occurred

This gives applications a safe path for "forget" without silently erasing all
trace of a destructive admin action.

<memex_response>
Both modes are useful for Memex, but the primary need is
`hard_purge_with_tombstone`.

Memex's forget tool is a user-initiated "I want this gone" action on personal
data. The user expects the content to be removed. A tombstone that records
"node X of kind knowledge was purged at time T by source_ref forget-tool,
originally created at time T0" is the right audit trail — it proves the
deletion happened without retaining the deleted content.

`redact_content_keep_audit` would be useful for a different Memex scenario:
when the user wants to remove sensitive content from a knowledge item but
keep the structural record that the item existed, was linked to meetings and
goals, etc. This is a "redact the body but keep the metadata" operation.
Memex does not have this feature today but it is a reasonable future addition.

For the initial implementation, `hard_purge_with_tombstone` is sufficient for
Memex's forget workflow.
</memex_response>

### B. Add richer generic read/query surfaces

The goal is not Memex-specific "goal queries." The goal is better generic graph
and projection reads so applications can build richer read models without raw
SQL.

Recommended additions:

1. Expand query predicates beyond equality:
   - JSON text contains / prefix / pattern matching
   - existence / null checks
   - timestamp and numeric comparisons

2. Add better traversal + projection result support:
   - query root node plus traversed neighbors in a structured result
   - query root node plus selected edge metadata
   - query root node plus provenance summary

3. Add search-result enrichment helpers:
   - text/vector search returning matched node plus chunk match context
   - optional follow-up traversal to related nodes in one compiled plan

4. Add explicit index/projection guidance for hot JSON properties:
   - when to keep a field as JSON only
   - when to project it into chunks for FTS/vector
   - when to recommend an expression index

<memex_response>
All four additions are directly useful to Memex. Priority from Memex's
perspective:

**Highest priority: #2 (traversal + projection result support)**

This is the single biggest read-side ergonomic gap for Memex. Every
drill-in view (goal detail, meeting detail, knowledge item detail) currently
requires multiple queries that are assembled application-side. A query that
returns root + grouped neighbors by edge kind would eliminate the most code.

The ideal result shape for Memex would be something like:

```
QueryResult {
  root: NodeRow,
  neighbors: {
    "parent_goal": [NodeRow, ...],
    "blocked_by": [NodeRow, ...],
    "has_plan": [NodeRow, ...],
    "linked_meeting": [NodeRow, ...],
  },
  edges: [EdgeRow, ...]  // the edges themselves, for metadata
}
```

**High priority: #1 (expanded predicates)**

Timestamp comparison is essential for Memex. Almost every query involves
`updated_at DESC` ordering or `created_at > X` filtering. Numeric comparison
is needed for priority and confidence fields. Null checks are needed for
soft-delete filtering (WHERE superseded_at IS NULL is already handled by
fathomdb internally, but application-level null checks on properties like
`deadline` or `error` are common).

JSON text contains/prefix would be useful but is lower priority than
timestamp and numeric comparisons — Memex can route substring search through
FTS chunks as proposed below.

**Medium priority: #3 (search-result enrichment)**

Memex's retrieval pipeline currently runs FTS/vector search, gets node IDs,
then does N individual lookups for typed knowledge metadata and entity links.
A search that returns matched nodes plus a bounded traversal to related
nodes in one plan would cut the lookup cascade.

**Lower priority: #4 (index/projection guidance)**

Useful as documentation, not as a code change. Memex already has opinions
about what goes in chunks vs. properties (see the chunk strategy discussion
in the earlier feasibility analysis). Having fathomdb-side guidance would
help validate those opinions.
</memex_response>

#### Important boundary

The answer to JSON substring search should not be "encourage property scans for
everything." In many cases the better answer is:

- store searchable narrative text in chunks
- use FTS/vector over chunks
- reserve JSON filters for structured property constraints

The query surface should support both, but the docs should steer clients toward
the correct split.

<memex_response>
Agreed. This matches Memex's own conclusion from the earlier feasibility
analysis (Gap 4 mitigation): "All searchable payload content goes into
chunks. This is actually the correct design — FTS search over chunks instead
of LIKE on JSON."

Memex's current `LIKE '%term%'` on `payload_json` is a known code smell. It
exists because the `wm_knowledge_objects` table stores typed knowledge
metadata in a JSON column and search needs to reach into it. The correct fix
on fathomdb is to emit the searchable fields from that metadata as chunks at
write time, so FTS covers them. The JSON properties then serve only as
structured filters (e.g., `knowledge_type = 'decision'`,
`source_surface = 'meeting'`).

Memex would need to do this chunking work regardless of whether fathomdb adds
JSON substring search. It is an application responsibility.
</memex_response>

### C. Add write-composition helpers around atomic `WriteRequest`

This is mainly an API/SDK ergonomics improvement.

Recommended additions:

1. Higher-level write-bundle builders in Rust and Python:
   - create node + its chunks + outgoing edges
   - upsert node and replace chunk projection
   - create related node family and provenance links in one bundle

2. Request-local ID/reference helpers:
   - generate stable logical IDs client-side
   - reserve local aliases while building a write bundle
   - resolve aliases to caller-provided IDs before submission

3. Validation and diagnostics:
   - preflight validation for common invalid bundle shapes
   - human-readable summaries of what a write bundle will mutate

4. Projection guidance embedded in SDK docs:
   - examples for deciding what text becomes chunks
   - examples for hot-property indexing
   - examples for search-oriented write patterns

<memex_response>
All four are welcome. Priority from Memex's perspective:

**Highest: #1 (write-bundle builders)**

The "create node + its chunks + outgoing edges" helper alone would cover
~80% of Memex's write flows. The meeting transcription example from above
(20-40 inserts in one WriteRequest) would become a series of
`builder.add_node_with_chunks_and_edges(...)` calls that internally wire
IDs and chunk references correctly.

**High: #2 (request-local ID/alias helpers)**

Memex already generates UUIDs client-side, so the ID model is not a blocker.
But when assembling a large request, the code currently looks like:

```python
meeting_id = str(uuid4())
recording_id = str(uuid4())
artifact_id = str(uuid4())
# ... wire these into edges manually
```

An alias system that lets you say "the node I called 'meeting'" and resolves
it to the actual ID at submission time would reduce wiring errors.

**Medium: #3 (validation and diagnostics)**

Preflight validation would catch a class of bugs that Memex currently
discovers only at write time — e.g., an edge referencing a logical_id that
is neither in the current request nor in the database.

**Lower: #4 (projection guidance)**

Useful as documentation. Memex's chunk strategy decisions are application-
level, but having substrate-recommended patterns would help.
</memex_response>

#### Why this matters

Memex's issue is usually not that one write needs multiple database
transactions. It is that one logical mutation spans many related records and the
client needs help assembling that into one atomic request safely. `fathomdb`
should make the single-request happy path easy.

<memex_response>
Exactly right. The earlier feasibility analysis identified "No cross-
WriteRequest transactions" as a gap, but concluded that it was mitigated by
batching into single WriteRequests with caller-provided IDs. The real
remaining problem is not atomicity — it is assembly ergonomics. This section
correctly identifies that.
</memex_response>

### D. Improve operational-state modeling ergonomics

This area needs a sharper architectural split.

There are two valid classes of operational state:

1. **History-bearing operational state**
   Examples:
   - workflow runs
   - task execution state
   - user-visible notifications that should be searchable/auditable
   - long-lived intake or repair jobs

   These fit well in:
   - nodes/edges/chunks/provenance
   - `runs`/`steps`/`actions` where provenance anchoring matters

2. **High-churn bookkeeping state**
   Examples:
   - connector-health last-checked timestamps
   - retry cursors
   - queue positions
   - debounce locks
   - ephemeral counters

   These may not fit well as full superseding nodes if every update creates
   avoidable write amplification.

<memex_response>
This split is correct and maps cleanly to Memex's actual state inventory.
See the detailed classification in the response to Gap 4 above.

One additional data point: Memex's `connector_health` table has 6-8 rows
updated every ~60 seconds. Under full node supersession, that would generate
~8,640-11,520 superseded rows per day for state that is only ever read as
"what is the current status." This is the canonical example of state where
supersession history has zero value and the write amplification is pure
waste.

By contrast, Memex's `notifications` table has ~5-20 rows created per day,
each read/dismissed/acted-on at most once. Full node supersession is fine
here — the history of "notification created → read → dismissed" is useful
audit context and the write volume is negligible.

The split should be driven by: does the mutation history of this record have
any value to the application or its operator? If yes, graph-native. If no,
adjunct operational table.
</memex_response>

#### Proposal: define two supported patterns

##### Pattern 1: graph-native operational records

Strengthen the recommended pattern for durable, auditable operational state:

- use nodes/edges for durable operational objects
- use `runs`/`steps`/`actions` for execution lineage
- use chunks only where operator-visible text/search is needed

This should be the preferred pattern whenever history, provenance, search, or
repairability matters.

<memex_response>
Agreed. Memex would use this pattern for:

- notifications (durable, user-actionable, linked to goals)
- intake_log records (lifecycle provenance for ingested content)
- task_runs execution history (completed/failed audit trail)
- audit_log entries (append-only event stream)
- meeting_extraction_runs (transcription/extraction provenance)

All of these benefit from searchability, provenance edges, and history.
</memex_response>

##### Pattern 2: adjunct operational tables with explicit contract

If `fathomdb` is going to support an "operational table" primitive, it should be
strictly limited and explicitly outside the domain-schema surface.

A safe version of that support would look like:

- SQLite-backed adjunct tables managed by `fathomdb`
- engine-owned migration/registration contract
- generic rows with declared keys/columns, not application-named product schema
- strong guidance that this surface is for high-churn bookkeeping, not knowledge
  or world-model truth
- recoverability/admin tooling that includes these tables in:
  - export
  - integrity reporting
  - backup/recovery expectations
  - audit where mutation history matters

<memex_response>
The design constraints listed here are all correct. Memex's strongest
requirement is the last bullet cluster — recoverability/admin tooling
inclusion.

Memex has direct experience with the failure mode this prevents. The
LadybugDB WAL corruption incidents (3 confirmed, described in Memex project
memory) happened partly because the backup tooling did not cover the shadow
store correctly. Backups captured the SQLite store but produced empty 28KB
LadybugDB files because the WAL was not included. 19 hours of knowledge
writes were lost in the worst incident.

If fathomdb supports adjunct operational tables, those tables MUST participate
in the same backup, export, and integrity-check contract as the main
node/edge/chunk store. A split where the main store is backed up but the
operational tables are not is the exact failure mode Memex has already been
burned by.

The "engine-owned migration/registration contract" point is also important.
Memex currently manages its own SQLite migrations (35+ migration steps in
migrations.py). If adjunct tables are fathomdb-managed, Memex would declare
the table shape and fathomdb would handle versioning. This is cleaner than
Memex managing its own migration system alongside fathomdb's.
</memex_response>

#### Candidate operational-table primitive

If added, an operational-table primitive should be intentionally narrow:

- key/value or small-row tables
- append-only or explicit upsert semantics
- bounded schema declaration
- no arbitrary raw-SQL escape hatch
- optional TTL/compaction support
- admin visibility and export participation

Two possible modes:

1. `append_only_operational_log`
   - best for cursors, heartbeats, event logs, recent status changes
   - preserves history and is easiest to audit/recover

2. `latest_state_table`
   - best for current-status snapshots like connector health
   - must still have an auditable mutation trail if the engine claims
     recoverability parity with core data protections

<memex_response>
Both modes map to real Memex tables:

`append_only_operational_log` would serve:
- `intake_log` (if not promoted to graph-native nodes)
- `audit_log` (if not promoted to provenance_events)
- debounce/heartbeat records that Memex does not currently persist but
  probably should

`latest_state_table` would serve:
- `connector_health` (6-8 rows, updated every ~60s, only current state
  matters)
- `auto_ingest_sources` (last_check, last_result, updated per poll cycle)
- `tool_usage_stats` (counter increments)
- `user_settings` (key/value, rarely changed)
- `session_context` singleton (current session state blob)

The TTL/compaction support is useful for the append-only mode.
Memex's intake_log grows ~50-200 rows per day and is only queried for the
last ~100 entries. Automatic compaction of old entries would replace Memex's
current manual cleanup.

The "no arbitrary raw-SQL escape hatch" constraint is fine. Memex does not
need raw SQL access to operational tables — the access patterns are simple
get/put/list/count operations. A typed API is preferable.
</memex_response>

#### Recommendation

Do not immediately add operational tables as a general escape hatch.

Instead:

1. document the graph-native and runtime-table patterns first
2. evaluate a narrow adjunct operational-table primitive only for high-churn
   bookkeeping that is clearly a bad fit for supersession-heavy nodes
3. require any such primitive to meet the same data-protection expectations as
   the rest of the system:
   - safe export
   - deterministic recovery behavior
   - integrity visibility
   - no silent data loss modes

This preserves the architecture boundary while acknowledging that some local
SQLite workloads are genuinely table-shaped.

<memex_response>
Agreed with the sequencing. Memex can keep its operational tables in a
co-resident SQLite file during Phase 1 and Phase 2 of the migration. The
adjunct operational-table primitive is a Phase 3 concern at the earliest.

The critical path for Memex adoption is:
1. Restore/purge lifecycle (blocks forget tool migration)
2. Write-bundle builders (blocks efficient Phase 1 migration code)
3. Richer read/traversal results (blocks drill-in view migration)

Operational-table support is useful but not blocking. Memex can work around
it by keeping a separate SQLite file for high-churn state, as long as the
backup/export tooling story is clear about what is and is not covered.
</memex_response>

### E. Clean up stale vector-cleanup docs and fill any remaining proof gaps

The docs should be updated to reflect current behavior:

- vector cleanup on node retire is implemented
- vector cleanup on replace-upsert with chunk replacement is implemented

Follow-up testing should focus only on lifecycle interactions that still need
proof, for example:

- purge plus vec cleanup
- excision plus vec cleanup
- restore/purge interactions with regenerated vectors

The base "retire leaves stale vec rows" statement should be removed anywhere it
still appears.

<memex_response>
Agreed. Memex will update its own tracking (the feasibility analysis and
wake decision log) to remove the stale "vector cleanup on retire deferred"
characterization.

The three follow-up testing items listed here are exactly the right
verification scope from Memex's perspective. The most important one for Memex
is "purge plus vec cleanup" — when purge ships, Memex needs confidence that
the forget workflow leaves no orphaned projection rows.
</memex_response>

## Additional Considerations From Earlier Discussion

### Memex should not drive `fathomdb` toward domain tables

The correct response to Memex is:

- make the generic substrate stronger
- make graph/query/lifecycle/admin support stronger
- keep goals, scheduler artifacts, meetings, notifications, and similar concepts
  as application-defined kinds and properties

This is consistent with the current architecture and avoids freezing Memex's
product schema into the engine forever.

<memex_response>
Agreed without reservation. Memex's product schema changes regularly — the
`wm_*` family has gone through 13 migration steps (v22-v35) in the 0.8
development cycle alone. Freezing any of those table shapes into fathomdb's
engine would create a coupling that hurts both projects.

The right contract is: fathomdb provides generic nodes, edges, chunks,
lifecycle, and query primitives. Memex defines its node kinds, edge kinds,
property schemas, and chunk strategies as application-level concerns. If
Memex's ontology changes (and it will), only Memex's code changes.
</memex_response>

### Full datastore replacement does not require "host arbitrary app SQL tables"

There are two legitimate end states:

1. More of Memex is remodeled onto `fathomdb` primitives.
2. Some high-churn operational bookkeeping remains in a sanctioned adjunct
   SQLite surface with explicit reliability and recovery guarantees.

What should be avoided is an unprincipled middle ground where applications
quietly depend on raw co-resident tables that the engine neither models nor
protects.

<memex_response>
This is the right framing. Memex's current architecture already has this
unprincipled middle ground — LadybugDB is a co-resident store that the
main SQLite backup/migration tooling does not fully protect. The 3 WAL
corruption incidents are direct evidence of the failure mode described here.

The two legitimate end states both work for Memex:

End state 1 is the goal for all semantic/world-model state (the `wm_*`
families, knowledge items, meetings, conversation turns).

End state 2 is acceptable for high-churn operational bookkeeping
(connector_health, tool_usage_stats, auto_ingest_sources, queue cursors)
IF the adjunct surface has explicit backup/export/integrity coverage.

What Memex cannot accept is a third state where some data lives in an
unprotected co-resident SQLite file that fathomdb does not know about.
That is the current LadybugDB situation and it has caused real data loss.
</memex_response>

### `last_accessed` remains a real design issue

Memex's `last_accessed` problem is separate from the five main gaps, but it is
closely related to operational-state support. `fathomdb` should eventually offer
a better answer than "full node supersession on every read."

Plausible directions:

- append-only access events with derived last-access materialization
- batched touch/update APIs
- latest-state operational row support for access metadata

This should be addressed as a generic high-churn metadata problem, not as a
Memex-specific feature.

Current direction:

- `fathomdb` now uses batched touch/update as the primary solution.
- The canonical substrate is a dedicated engine-owned metadata table keyed by
  `logical_id`, not operational-store backing.
- Reads surface `last_accessed_at` as a separate optional field on node
  results.
- Touch batches reject unknown or inactive logical IDs atomically and emit
  bounded provenance for recovery/audit purposes.

<memex_response>
The three directions listed are all viable for Memex. Preference order:

1. **Batched touch/update APIs** — simplest for Memex to adopt. Memex
   already batches retrieval results (typically 5-20 items per retrieval
   context). A single "touch these N logical_ids" call that updates a
   last_accessed property without triggering full supersession would
   directly replace Memex's current per-item UPDATE statement.

2. **Append-only access events with derived materialization** — more
   correct but more complex. Memex could emit access events and let
   fathomdb materialize last_accessed as a derived property. This would
   also enable access-frequency analysis (how often is this knowledge
   retrieved?) which Memex currently cannot do.

3. **Latest-state operational row** — works but is the least integrated
   option. It would mean access metadata lives in a different surface
   than the knowledge node itself, requiring a join at read time.

The volume is real: Memex retrieves 10-30 knowledge items per turn,
across 20-50 turns per day. That is 200-1500 last_accessed updates per
day. Under full node supersession, that would generate 200-1500 superseded
knowledge node rows per day — rows that have zero semantic value (only the
last_accessed timestamp changed). This is why it needs a lighter-weight
primitive.

However, this is not blocking for Phase 1 or Phase 2 of the migration.
Memex can defer last_accessed tracking entirely during initial migration
(accept stale access timestamps) and add it back once fathomdb has a
lightweight touch/update primitive. The decay scoring still works — it just
uses slightly stale access timestamps, which is acceptable for a personal
knowledge system.
</memex_response>

## Recommended Implementation Order

1. Correct stale docs around vector cleanup and align tests with current truth.
2. Design and implement generic restore/purge lifecycle APIs.
3. Expand generic read/query surfaces and document chunk-vs-JSON guidance.
4. Add Rust/Python write-bundle builders and request-local reference helpers.
5. Document operational-state patterns and only then decide whether a narrow
   operational-table primitive is justified.

<memex_response>
This order is correct from Memex's perspective, with one adjustment:

Memex would slightly prefer swapping #3 and #4 — write-bundle builders before
richer reads. The reason is that Memex's Phase 1 migration (moving `wm_*`
families to fathomdb) is write-heavy. The first concrete integration work is
building the dual-write layer that emits WriteRequests alongside the current
SQLite inserts. Write-bundle builders would directly accelerate that work.
Richer reads become important when Memex starts flipping read paths to
fathomdb, which happens after the dual-write layer is validated.

Adjusted order from Memex's perspective:
1. Correct stale docs (quick win, removes confusion)
2. Restore/purge lifecycle (blocks forget tool, highest substrate value)
3. Write-bundle builders (blocks efficient Phase 1 dual-write code)
4. Richer read/query surfaces (blocks Phase 1 read-path flip)
5. Operational-state patterns (Phase 3, not blocking)

That said, this is a preference, not a hard dependency. Memex can build
Phase 1 dual-write code with the current low-level WriteRequest API — it is
just more verbose and error-prone.
</memex_response>

## Bottom Line

Memex does not require `fathomdb` to absorb product-specific schema. It does
require `fathomdb` to become better at:

- lifecycle management
- richer generic reads
- atomic-write ergonomics
- operational-state support for high-churn local workloads

If `fathomdb` closes those gaps while preserving the current engine boundary, it
can support substantially more of Memex without turning into a Memex-shaped
database.

<memex_response>
Agreed. This is the right summary.

From Memex's side, the commitment is:

1. Memex defines its own node kinds, edge kinds, and property schemas —
   fathomdb does not need to know what a "goal" or "meeting" is.
2. Memex handles its own chunk strategy — deciding what text goes into
   chunks for FTS/vector vs. what stays as JSON properties.
3. Memex handles its own ranking, decay scoring, and retrieval logic —
   fathomdb provides the search primitives, Memex composes them.
4. Memex handles its own application-level lifecycle policy (grace periods,
   retention windows, cleanup schedules) — fathomdb provides the substrate
   operations (retire, restore, purge).

The earlier feasibility analysis concluded "feasible in phases, not as a
big-bang replacement." This architecture note reinforces that conclusion and
sharpens the substrate requirements. The gap list is concrete and bounded.
None of the five gaps are architectural mismatches — they are missing
primitives that fathomdb would benefit from regardless of whether Memex is
the client.
</memex_response>
