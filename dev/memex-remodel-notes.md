# Memex Remodel Notes

## Purpose

These notes explore how more of Memex could be remodeled onto `fathomdb`'s
node/edge/property primitives rather than kept in Memex-owned relational tables.

This is not a proposal to add Memex-specific engine tables to `fathomdb`.
Recent `fathomdb` architecture work explicitly rejects that. The question here
is narrower:

- what Memex data already behaves like durable world-model state
- how that state could be represented as application-defined nodes, edges, and
  chunked text on top of `fathomdb`
- what still looks more like app-local operational state and may not be worth
  remodeling first

## High-level conclusion

Memex already contains the beginning of this remodel in its own codebase.

The strongest signal is the `wm_*` family added in migrations v22-v35 plus the
builder helpers in:

- `src/memex/world_model_backbone.py`
- `src/memex/world_model_goal.py`
- `src/memex/world_model_task_commitment.py`
- `src/memex/world_model_plans.py`
- `src/memex/memory/world_model_reads.py`

Those sidecars show the ontology Memex is already converging on:

- entities and entity attributes
- intents
- actions
- observations
- events
- goals
- tasks
- commitments
- plans
- plan steps
- execution records
- plan-step transitions
- provenance links
- semantic mappings
- knowledge ingest runs

If Memex moves further onto `fathomdb`, the right move is not to preserve the
old relational tables and ask `fathomdb` to host them. The right move is to
promote these world-model families into Memex-defined node kinds and edge kinds
stored on `fathomdb`.

## What the Memex code already tells us

### 1. Legacy relational stores still own a lot of durable state

The current SQLite store still has first-class tables for:

- goals and goal history
- session context and conversation log
- notifications
- scheduler tasks and task runs
- meetings, recordings, intelligence artifacts, and extraction runs
- audit log and connector health
- calendar events

Those surfaces appear in:

- `src/memex/migrations.py`
- `src/memex/store.py`
- `src/memex/scheduler/store.py`
- `src/memex/meetings/store.py`
- `src/memex/notifications.py`
- `src/memex/audit.py`

### 2. Memex is already mirroring important subsets into canonical world-model shapes

The 0.8 work has already started shifting important semantics into `wm_*`
records:

- goals sync into `wm_goals`
- reminder-like scheduled tasks sync into `wm_tasks`
- meeting commitments sync into `wm_commitments`
- partner and agent-self profile state sync into `wm_entities` and
  `wm_entity_attributes`
- session intent and prompt-control outputs sync into canonical intent/action/
  event families
- planning data syncs into `wm_action_plans`, `wm_plan_steps`,
  `wm_execution_records`, and `wm_plan_step_transitions`
- provenance between these records is expressed through `wm_provenance_links`

This matters because it means Memex has already done ontology design work.
`fathomdb` integration should reuse that direction rather than inventing a
different remodel from scratch.

### 3. The existing sidecars are still relational shadows, not the real center

Today the world-model families are still implemented as dedicated SQLite tables.
That makes them useful as a design reference, but not yet the final shape.

If Memex remodels onto `fathomdb`, the `wm_*` families should become:

- application-defined node kinds
- application-defined edge kinds
- JSON properties on those nodes and edges
- chunked text where the records are text-bearing and need FTS/vector retrieval

## Remodeling principles

### Principle 1: use node kinds, not new engine tables

The target is not `fathomdb` tables named `goals`, `meetings`, or
`scheduled_tasks`.

The target is:

- node kind `goal`
- node kind `scheduled_task`
- node kind `meeting`
- node kind `meeting_recording`
- node kind `notification`
- node kind `connector_status`
- node kind `conversation_turn`
- node kind `action_plan`
- node kind `plan_step`

and so on, with properties and relationships defined by Memex.

### Principle 2: use edges for graph semantics that are currently foreign keys or link tables

Many relational fields in Memex are really relationships and should become
edges:

- goal parent/child
- goal blocked by goal
- task advances goal
- plan belongs to goal
- step belongs to plan
- step depends on step
- step blocked by step
- execution records step / plan / source-record links
- meeting produced commitment / decision
- meeting links to goal
- meeting links to entity / attendee
- notification is about goal
- calendar event relates to meeting or goal

### Principle 3: use supersession for history instead of history side tables

Memex still uses tables like `goals_history` and several mutable singleton-ish
tables.

On `fathomdb`, the cleaner fit is:

- canonical nodes/edges are append-oriented
- prior versions stay queryable
- "history" is not a separate table, it is prior physical versions of the same
  logical entity

That matches both Memex's stated direction and `fathomdb`'s model better than
keeping parallel history tables.

### Principle 4: only remodel durable semantic state first

Some Memex data is obviously worth moving onto `fathomdb`.
Some is still mostly operational plumbing.

Good first-class remodel candidates:

- goals
- tasks and commitments
- plans and execution records
- meeting records and durable meeting semantics
- conversation turns that matter to retrieval and drill-in
- partner/agent profile state
- semantic mappings
- knowledge ingest runs

Lower-priority or conditional candidates:

- notifications
- connector health
- audit summaries
- scheduler run queue internals

These may still move later, but they do not need to move first to make
`fathomdb` the knowledge/world-model center.

## Concrete remodel candidates

### A. Goals

Current code:

- `goals` and `goals_history`
- `Goal`, `GoalEntityLink`, `MeetingGoalLink`
- mirrored to `WorldModelGoal`

Recommended `fathomdb` shape:

- node kind `goal`
- properties:
  - `title`
  - `description`
  - `status`
  - `priority`
  - `blocked_reason`
  - `failure_reason`
  - `deadline`
  - `metadata`
  - `source_surface`
- edges:
  - `parent_goal`
  - `blocked_by_goal`
  - `related_entity`
  - `linked_meeting`

Why this fits:

- `Goal` is already a semantic world-model object in Memex.
- `goals_history` should collapse into normal supersession history.
- `GoalEntityLink` and `MeetingGoalLink` are already graph-shaped.

### B. Scheduled tasks and reminders

Current code:

- `scheduled_tasks`
- `task_runs`
- reminder tasks mirrored into `WorldModelTask`

Recommended `fathomdb` shape:

- node kind `scheduled_task`
- properties:
  - `name`
  - `description`
  - `cron_expr`
  - `action_type`
  - `action_data`
  - `enabled`
  - `builtin`
  - `due_at`
  - `task_kind`
  - `source_surface`
- edges:
  - `advances_goal`
  - `depends_on_task` if Memex introduces richer task dependencies

For `task_runs`:

- do not model them as a Memex-specific table in `fathomdb`
- either:
  - express them through `fathomdb` runs/steps/actions provenance anchors, or
  - use a Memex-defined node kind `task_execution` if the product needs
    first-class retrieval/drill-in over those runs

Interpretation:

- the task definition itself is durable semantic state
- the queued/running/retrying worker bookkeeping is more operational
- the execution history may belong partly in `fathomdb` provenance/runtime
  anchors rather than as a copied relational scheduler table

### C. Meetings, recordings, and meeting intelligence

Current code:

- `meetings`
- `meeting_recordings`
- `meeting_intelligence_artifacts`
- `meeting_extraction_runs`
- `meeting_goal_links`
- `meeting_entity_links`
- semantic projection already promotes some meeting outputs into goals/items/
  commitments/actions/observations

Recommended `fathomdb` shape:

- node kind `meeting`
- properties:
  - `title`
  - `state`
  - `scheduled_for`
  - `external_event_id`
  - `calendar_source`
  - `started_at`
  - `stopped_at`
  - `transcribed_at`
  - `ingested_at`
  - `speaker_count`
  - `language`
  - `notes`
  - `action_items`
  - `error`
- node kind `meeting_recording`
- properties:
  - `sequence_number`
  - `state`
  - `audio_path`
  - `duration_seconds`
  - `sample_rate`
  - `transcription_quality`
  - `tried_vad_thresholds`
  - `segments`
  - `error`
- node kind `meeting_artifact`
- properties:
  - `artifact_type`
  - `title`
  - `content`
  - `speaker`
  - `ordinal`
  - `payload`
  - `source_surface`
- node kind `meeting_extraction_run`
- properties:
  - `run_type`
  - `status`
  - `model`
  - `artifact_count`
  - `metadata`

Edges:

- `has_recording`
- `produced_artifact`
- `recorded_in`
- `attended_by`
- `promoted_action_item_goal`
- `produced_commitment`
- `produced_decision`
- `updates_goal`

Important nuance:

- transcript segments should probably not be modeled as separate durable nodes
  unless Memex needs per-segment object identity
- in `fathomdb`, the better fit is usually text-bearing meeting or recording
  nodes whose text is chunked into the engine's chunk projection layer

This is one of the strongest candidates for feature gain, because the current
meeting model is already highly graph-shaped and provenance-heavy.

### D. Notifications

Current code:

- `notifications`
- `NotificationRouter`

Recommended `fathomdb` shape if promoted:

- node kind `notification`
- properties:
  - `title`
  - `body`
  - `priority`
  - `source`
  - `action_type`
  - `action_data`
  - `read_at`
  - `dismissed_at`
- edges:
  - `about_goal`
  - `emitted_by_action`
  - `emitted_by_event`

Assessment:

- notifications are durable user-facing state, so they can fit on `fathomdb`
- but they are also a simple queue-like operational surface
- this is worth moving only after the more central world-model families are
  stable

### E. Audit log and connector health

Current code:

- `audit_log`
- `connector_health`
- `tool_usage_stats`

Recommended `fathomdb` shape if promoted:

- node kind `audit_entry`
- properties:
  - `timestamp`
  - `connector`
  - `action`
  - `capability_required`
  - `granted`
  - `result_summary`
  - `duration_ms`
- node kind `connector_status`
- properties:
  - `name`
  - `status`
  - `last_check`
  - `error`
  - `tools_discovered`
- node kind `tool_usage_stat`
- properties:
  - `tool_name`
  - `keyword_matched`
  - `call_count`
  - `last_used`

Edges:

- `for_goal`
- `for_session`
- `describes_connector`

Assessment:

- these can be remodeled, but they are not the best first targets
- they are more operational telemetry than semantic memory
- they should only move if Memex wants unified provenance/search over them

### F. Session context and conversation log

Current code:

- singleton `session_context`
- append-only `conversation_log`
- `conversation_log_fts`
- `conversation_embeddings`
- session intent already mirrored into `WorldModelIntent`
- actions and events already mirrored into `wm_actions` and `wm_events`

Recommended `fathomdb` shape:

- node kind `session`
- properties:
  - `session_id`
  - `started_at`
  - `ended_at`
  - summary / compact retained context
- node kind `conversation_turn`
- properties:
  - `role`
  - `content`
  - `classification`
  - `reflect_result`
  - `timestamp`
- edges:
  - `in_session`
  - `next_turn`
  - `responds_to`
  - `produced_intent`
  - `produced_action`
  - `produced_event`

Important nuance:

- a lot of `session_context` is transient working memory and should not be
  copied blindly into durable canonical state
- the durable part is the session envelope, the conversation turns, the latest
  summarized context, and the intent/action/event records Memex already treats
  as canonical

### G. Partner profile, agent self model, semantic mappings

Current code:

- partner and agent-self already sync into `wm_entities` and
  `wm_entity_attributes`
- speaker mappings sync into `wm_semantic_mappings`

Recommended `fathomdb` shape:

- node kind `entity`
- properties:
  - `entity_type`
  - `canonical_key`
  - `display_name`
  - `status`
  - `confidence`
  - `source_surface`
- node kind `entity_attribute`
- properties:
  - `attribute_group`
  - `attribute_key`
  - `value_text`
  - `value_json`
  - `provenance_kind`
  - `provenance_ref`
  - `observed_at`
- node kind `semantic_mapping`
- properties:
  - `mapping_kind`
  - `source_value`
  - `target_value`
  - `scope_type`
  - `scope_key`
  - `confidence`

Edges:

- `has_attribute`
- `maps_value`
- `scoped_to_entity`

This is already one of the cleanest remodel candidates in Memex.

### H. Plans, plan steps, and execution records

Current code:

- `WorldModelActionPlan`
- `WorldModelPlanStep`
- `WorldModelExecutionRecord`
- `WorldModelPlanStepTransition`
- provenance links between them already exist

Recommended `fathomdb` shape:

- node kind `action_plan`
- node kind `plan_step`
- node kind `execution_record`
- node kind `step_transition`

Properties should mostly mirror the existing `WorldModel*` models.

Edges:

- `for_goal`
- `part_of_plan`
- `selected_step`
- `depends_on_step`
- `blocked_by_step`
- `executes_step`
- `records_source`
- `emitted_by_execution`

This family is already world-model-native. It should map directly onto
`fathomdb` with almost no conceptual translation.

### I. Knowledge ingest runs

Current code:

- `wm_knowledge_ingest_runs`
- action/event records emitted from ingestion paths

Recommended `fathomdb` shape:

- node kind `knowledge_ingest_run`
- properties:
  - `ingest_kind`
  - `trigger_surface`
  - `session_id`
  - `source_ref`
  - `normalized_source_ref`
  - `requested_item_type`
  - `status`
  - `phase`
  - `fallback_used`
  - `item_id`
  - `knowledge_id`
  - `summary_model`
  - `error`
  - `payload`

Edges:

- `produced_knowledge`
- `produced_item`
- `triggered_by_action`
- `in_session`

This is a strong fit because it is already lifecycle/provenance-centric.

## The strongest remodel path

Based on the code, the best "more of Memex on fathomdb" path is:

1. Treat the existing `wm_*` families as the source ontology.
2. Re-express those families as Memex-defined node kinds and edge kinds on top
   of `fathomdb`.
3. Move product retrieval and drill-in to those canonical records.
4. Retire the parallel sidecar tables once parity is proven.
5. Only then decide which of the remaining operational tables are worth moving.

## What should likely move first

Best first candidates:

- goals
- partner/self entities and attributes
- semantic mappings
- tasks and commitments
- plans, plan steps, and execution records
- meeting records plus durable meeting artifacts
- knowledge ingest runs

These are already semantically modeled and already have partial canonical
implementations in Memex.

## What should likely move later, or maybe not at all

Later or optional candidates:

- notifications
- connector health
- audit summaries
- task-run queue bookkeeping
- singleton session context blobs

These are valid `fathomdb` candidates only if Memex wants stronger search,
provenance, or cross-surface drill-in over them. Otherwise they can remain
app-local operational state.

## Suggested cutover order

### Phase 1: canonical families already present in Memex

Use `fathomdb` node kinds and edges for:

- `wm_entities`
- `wm_entity_attributes`
- `wm_semantic_mappings`
- `wm_goals`
- `wm_tasks`
- `wm_commitments`
- `wm_action_plans`
- `wm_plan_steps`
- `wm_execution_records`
- `wm_plan_step_transitions`
- `wm_provenance_links`

### Phase 2: meeting and conversation durability

Promote:

- meetings
- recordings
- durable meeting artifacts
- conversation turns
- session envelopes

Leverage `fathomdb` chunks for transcript and turn text instead of maintaining
separate FTS/vector infrastructure in Memex.

### Phase 3: operational telemetry if it proves useful

Only after the canonical center is stable, evaluate:

- notifications
- audit entries
- connector health
- tool usage

## Main caution

The easiest failure mode would be to rebuild Memex's current sidecar schema
inside `fathomdb` as if `fathomdb` were just a new place to put the same tables.

That would miss the point.

The right target is:

- Memex-defined ontology
- `fathomdb` primitives
- graph relationships instead of join tables
- supersession instead of history tables
- chunk projections instead of ad hoc FTS/vector tables

Memex has already done enough canonical modeling work that this remodel can be
guided by code, not by speculation.
