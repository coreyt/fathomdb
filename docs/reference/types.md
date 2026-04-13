# Types

All public data types used across the fathomdb Python SDK. Most are immutable
dataclasses returned by queries, writes, or admin operations.

---

## Enums

::: fathomdb.ProvenanceMode
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.ChunkPolicy
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.ProjectionTarget
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.OperationalCollectionKind
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.OperationalFilterMode
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.OperationalFilterFieldType
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.TraverseDirection
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.DrivingTable
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.TelemetryLevel
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.ResponseCyclePhase
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.OperationalRetentionActionKind
    options:
      heading_level: 3
      show_root_heading: true

---

## Query Result Types

::: fathomdb.NodeRow
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.RunRow
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.StepRow
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.ActionRow
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.QueryRows
    options:
      heading_level: 3
      show_root_heading: true

### SearchRows

Result set returned from `TextSearchBuilder.execute()` and
`FallbackSearchBuilder.execute()`. Unlike `QueryRows`, `SearchRows` is
organized around ranked search hits rather than decoded node rows, and
exposes the per-branch counts that describe what the adaptive pipeline
did:

| Field | Meaning |
|---|---|
| `hits` | All [`SearchHit`](#searchhit) rows in final merged order: strict block first, relaxed block second. |
| `strict_hit_count` | Number of hits contributed by the strict branch. |
| `relaxed_hit_count` | Number of hits contributed by the relaxed branch. |
| `vector_hit_count` | Number of hits in the vector block. Always `0` until vector retrieval is wired in (planned for a later phase). |
| `fallback_used` | `True` if the relaxed branch fired (i.e. the strict branch returned zero hits and the engine derived a relaxed shape). |
| `was_degraded` | `True` if the engine fell back to a simpler plan shape while executing. Mirrors `QueryRows.was_degraded`. |

::: fathomdb.SearchRows
    options:
      heading_level: 4
      show_root_heading: true

### SearchHit

A single adaptive or fallback search hit. Every hit carries the full
`NodeRow` plus search-specific metadata.

| Field | Meaning |
|---|---|
| `node` | The matched `NodeRow` (see above). |
| `score` | Raw engine score used for ordering within a block. Higher is always better, across every modality and every source (text hits use `-bm25(...)`; vector hits use a negated distance or a direct similarity). Scores are ordering-only within a block; scores from different blocks — in particular text vs. vector — are not on a shared scale and must not be compared or arithmetically combined across blocks. |
| `modality` | [`RetrievalModality`](#retrievalmodality): coarse retrieval-modality classifier (`text` or `vector`). Every hit carries this unambiguously. |
| `source` | [`SearchHitSource`](#searchhitsource): which projection surface produced the hit. |
| `match_mode` | Optional [`SearchMatchMode`](#searchmatchmode): whether this hit came from the strict or relaxed branch. Populated for text hits; `None`/`null` for future vector hits, which have no strict/relaxed notion. |
| `snippet` | Optional snippet extracted from the matched text, or `None` if the engine did not produce one. |
| `written_at` | **Seconds since the Unix epoch** (1970-01-01 UTC), matching `nodes.created_at` which is populated via SQLite `unixepoch()`. |
| `projection_row_id` | Row ID of the underlying projection row (chunk or property-FTS row), or `None` if not applicable. |
| `vector_distance` | Raw vector distance or similarity for vector hits; `None`/`null` for text hits. Stable public API, documented as modality-specific diagnostic data that is **not** cross-comparable: callers must not compare it against text-hit `score` values or combine it arithmetically with text scores. For distance metrics the raw distance is preserved (lower = closer); callers that want a "higher is better" ordering value should read `score` instead. |
| `attribution` | [`HitAttribution`](#hitattribution) if `with_match_attribution()` was set on the builder, otherwise `None`. |

::: fathomdb.SearchHit
    options:
      heading_level: 4
      show_root_heading: true

### SearchHitSource

Which full-text projection surface produced a `SearchHit`:

- `CHUNK` — the hit came from a document chunk.
- `PROPERTY` — the hit came from a property-FTS row for a structured node kind.
- `VECTOR` — reserved; not emitted by `text_search` in v1.

::: fathomdb.SearchHitSource
    options:
      heading_level: 4
      show_root_heading: true

### SearchMatchMode

Whether a hit came from the strict branch or the relaxed fallback:

- `STRICT` — literal interpretation of the caller's query.
- `RELAXED` — engine-derived relaxed shape. Only present when
  `fallback_used` is `True`.

::: fathomdb.SearchMatchMode
    options:
      heading_level: 4
      show_root_heading: true

### RetrievalModality

Coarse retrieval-modality classifier attached to every `SearchHit`.
Future phases will wire a vector retrieval branch; the field is
available today so consumers can switch on it without a breaking
change later.

- `TEXT` — the hit came from a text retrieval branch (chunk or
  property FTS). Every hit produced by the current search pipeline is
  tagged this way.
- `VECTOR` — reserved for a future vector retrieval branch. No code
  path emits this variant yet.

::: fathomdb.RetrievalModality
    options:
      heading_level: 4
      show_root_heading: true

### HitAttribution

Per-hit match attribution. Populated only when `with_match_attribution()`
is called on a `TextSearchBuilder` or `FallbackSearchBuilder`, and only
for hits backed by recursive-mode property FTS entries. `matched_paths`
lists the registered JSON paths that produced the FTS match for this
hit.

::: fathomdb.HitAttribution
    options:
      heading_level: 4
      show_root_heading: true

::: fathomdb.GroupedQueryRows
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.ExpansionRootRows
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.ExpansionSlotRows
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.CompiledQuery
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.CompiledGroupedQuery
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.QueryPlan
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.ExecutionHints
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.BindValue
    options:
      heading_level: 3
      show_root_heading: true

---

## Write Types

::: fathomdb.WriteRequest
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.WriteReceipt
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.NodeInsert
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.EdgeInsert
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.ChunkInsert
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.RunInsert
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.StepInsert
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.ActionInsert
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.NodeRetire
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.EdgeRetire
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.VecInsert
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.OptionalProjectionTask
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.OperationalAppend
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.OperationalPut
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.OperationalDelete
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.OperationalRegisterRequest
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.OperationalReadRequest
    options:
      heading_level: 3
      show_root_heading: true

---

## FTS Property Schema Types

### FtsPropertyPathMode

Extraction mode for a single registered FTS property path. `SCALAR`
resolves the path and appends the scalar value(s). `RECURSIVE` walks the
subtree rooted at the path and emits every scalar leaf, populating the
position map and making the entry eligible for match attribution. See
[Property FTS Projections](../guides/property-fts.md#scalar-vs-recursive-paths).

::: fathomdb.FtsPropertyPathMode
    options:
      heading_level: 4
      show_root_heading: true

### FtsPropertyPathSpec

A single registered property-FTS path with its extraction mode. Used as
the `entries` input to
`AdminClient.register_fts_property_schema_with_entries`.

::: fathomdb.FtsPropertyPathSpec
    options:
      heading_level: 4
      show_root_heading: true

### FtsPropertySchemaRecord

The registered FTS property projection schema for a node kind, as
returned from `describe_fts_property_schema` and
`list_fts_property_schemas`. `entries` is the mode-accurate per-path
view — read this field for new code. `property_paths` is a legacy flat
display list preserved for backwards compatibility.
`exclude_paths` is populated for recursive schemas with subtree
exclusions.

::: fathomdb.FtsPropertySchemaRecord
    options:
      heading_level: 4
      show_root_heading: true

---

## Report Types

::: fathomdb.IntegrityReport
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.SemanticReport
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.TraceReport
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.ProjectionRepairReport
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.SafeExportManifest
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.LogicalRestoreReport
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.LogicalPurgeReport
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.ProvenancePurgeReport
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.OperationalTraceReport
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.OperationalReadReport
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.OperationalRepairReport
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.OperationalPurgeReport
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.OperationalCompactionReport
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.OperationalHistoryValidationReport
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.OperationalSecondaryIndexRebuildReport
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.OperationalRetentionPlanReport
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.OperationalRetentionRunReport
    options:
      heading_level: 3
      show_root_heading: true

---

## Feedback Types

::: fathomdb.FeedbackConfig
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.ResponseCycleEvent
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.TelemetrySnapshot
    options:
      heading_level: 3
      show_root_heading: true

---

## Utility Types

::: fathomdb.RawJson
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.LastAccessTouchRequest
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.LastAccessTouchReport
    options:
      heading_level: 3
      show_root_heading: true

---

## Errors

All errors inherit from `FathomError`. Import from `fathomdb` or
`fathomdb.errors`.

::: fathomdb.errors.BuilderValidationError
    options:
      heading_level: 3
      show_root_heading: true

### FathomError

Base exception for all fathomdb engine errors.

### BridgeError

Error communicating with the admin bridge binary.

### CapabilityMissingError

A required capability is not enabled (e.g., vector search without
`vector_dimension` set on `Engine.open`).

### CompileError

Query compilation failed (invalid filter combination, unsupported operation).

### DatabaseLockedError

The database file is locked by another `Engine` instance.

### InvalidWriteError

A write request failed validation (e.g., duplicate logical ID in non-upsert
mode, invalid JSON properties).

### IoError

An I/O operation failed (file not found, permission denied, disk full).

### SchemaError

Schema validation failed (migration version mismatch, corrupt schema).

### SqliteError

An underlying SQLite error occurred.

### WriterRejectedError

The write was rejected by the engine (e.g., channel full under back-pressure).
