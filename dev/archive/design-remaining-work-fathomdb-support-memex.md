# Design: Remaining Work For FathomDB Support Of Memex

## Purpose

Enumerate the items from [`remaining-work-fathomdb-support-memex.md`](/home/coreyt/projects/fathomdb/dev/remaining-work-fathomdb-support-memex.md)
that still require design work, and describe the intended design for each.

This note excludes work that is primarily:

- documentation cleanup only
- proof/verification only
- already-decided implementation follow-through with no remaining design choice

That means the stale vector-doc cleanup item is **not** a standalone design
section here. Its remaining verification obligations are folded into the
restore/purge section where they materially affect lifecycle correctness.

## Design Items

The remaining items that still require design are:

1. generic restore/purge lifecycle APIs
2. write-bundle builders and request-local reference helpers
3. richer read/query result shapes
4. lightweight `last_accessed` support without full supersession
5. filtered reads for operational collections
6. optional schema validation for operational payloads

Each section below identifies the feature, the remaining items needed for
acceptance, and the design outline.

## 1. Restore/Purge Lifecycle APIs

### Feature

Generic lifecycle completion for:

- restore after retire
- hard purge after retire
- auditable destructive operations

Requirement:

- restore must re-establish full pre-retire content, not only reactivate the
  logical row
- that includes the object row, directly related retired edges, chunks, and
  the projection state needed for search/vector behavior to match the
  pre-retire object again

### Remaining Items For Acceptance

- add restore for retired logical objects
- redefine retire as reversible rather than as destructive cleanup that removes
  restoration prerequisites
- define how restore chooses the latest retired revision deterministically when
  multiple lifecycle events share the same second-level timestamps
- define the restorable unit of state for one retire scope
- define which directly related edges are restored with the object
- classify which retired state must be preserved directly and which may be
  re-established by deterministic rebuild
- add hard purge with deterministic cascade to chunks, FTS, vec rows, and
  directly connected edges
- define exactly what purge removes once retire preserves reversibility
- define whether source-based lifecycle variants are part of the same surface
- define restore scope reporting and linkage between retire, restore, and purge
- define audit/tombstone behavior for destructive lifecycle operations
- define integrity/recovery expectations for preserved retired state
- prove lifecycle correctness for:
  - purge plus vec cleanup
  - excision plus vec cleanup
  - restore/purge interaction with regenerated vectors

Acceptance criteria:

- restoring a retired object re-establishes its last pre-retire active content
  state, not a degraded subset
- restore does not leave the restored object orphaned from directly related
  edges retired in the same retire scope
- restore never depends on external re-ingest, application resubmission, or
  operator repair steps
- restore always revives the last pre-retire revision deterministically, even
  when multiple retires share the same second-level timestamp bucket
- purging is the only irreversible operation and removes no-less/no-more than
  the documented purge scope
- purging leaves no orphaned canonical rows, edges, chunks, FTS rows, vec rows,
  or retained restore-only state
- purge and restore are visible in provenance/admin tooling
- failed restore is diagnosable and clearly reports why full restoration was
  impossible
- semantic checks and recovery flows agree with lifecycle outcomes and can
  distinguish valid preserved-retire state from broken restore state

### Design Outline

- Add explicit admin lifecycle operations at the logical-id layer first.
- Change the lifecycle model so retire preserves reversibility and purge owns
  irreversible destruction.
- Define restore against a well-defined retire scope or snapshot, not as a
  vague `superseded_at = NULL` reactivation.
- Restore target selection must use a durable lifecycle ordering, not only
  second-precision timestamps. When timestamps tie, the design requires a
  stable write-order tie-break so restore cannot drift to an older retired
  revision under rapid successive updates.
- Edge restoration scope must not rely on exact timestamp equality with the
  node retire event. The design treats the latest node-retire provenance event
  as a durable lower bound in lifecycle ordering; adjacent edge-retire events
  in the same lifecycle source and at-or-after that lower bound remain
  eligible for restore.
- Treat the pre-retire logical row, its chunks, and directly related retired
  edges as restorable state that must survive retire.
- Treat FTS and vec behavior as part of restore completeness. The design may:
  - preserve retired projection rows directly, or
  - preserve enough retired canonical state that FTS/vec state can be rebuilt
    deterministically during restore
  The contract requirement is full-fidelity restore, not a particular storage
  mechanism.
- Purge should explicitly remove:
  - active/superseded canonical rows in scope
  - preserved retired content/projection state
  - directly attached edges in purge scope
  - derived rows and restore-only retained state
- Purge should cascade one hop across directly attached edges, then remove
  chunks, FTS rows, vec rows, and any retained state required only for
  reversibility.
- Runtime history should not be broadly purged by default; destructive
  lifecycle should target semantic objects and their direct projections.
- Restore should return a scope report describing what was restored at least at
  the level of object rows, edges, chunks, and projection state.
- Provenance should link restore back to the retire scope it reverses, and
  purge to the scope it irreversibly destroys.
- Recovery tooling and semantic checks must treat preserved retired state as
  intentional lifecycle material, not corruption, while still detecting missing
  restorable content or broken restoration prerequisites.
- Purge should leave a bounded audit/tombstone record through provenance so the
  system can prove the action occurred without retaining the purged content.
- Source-based lifecycle variants should be designed only if they compose
  cleanly with logical-id lifecycle semantics; logical-id operations are the
  primary surface.

## 2. Write-Bundle Builders And Request-Local References

### Feature

Higher-level authoring support for large atomic `WriteRequest`s.

### Remaining Items For Acceptance

- define Rust builder surface for multi-object graph writes
- define matching Python builder/helpers
- add request-local aliases or reference helpers so bundle authors do not need
  to pre-thread every generated ID manually
- preserve explicit caller-owned final IDs and current atomicity semantics

Acceptance criteria:

- common Memex write flows can be expressed as one bundle without hand-wiring
  dozens of IDs in user code
- bundle helpers compile to ordinary `WriteRequest`s with no hidden extra
  transaction behavior
- cross-object references are validated before submit where possible

### Design Outline

- Keep `WriteRequest` as the canonical engine input.
- Add client-side builder layers in Rust and Python that accumulate nodes,
  edges, chunks, runtime rows, and operational writes into one request.
- Introduce request-local symbolic references for objects created in the same
  bundle; the builder resolves those references to caller-provided final IDs at
  build time.
- Do not move ID generation into the engine; the builder may help generate IDs,
  but the final request remains explicit and deterministic.
- Prefer a small number of bundle primitives over many domain-specific helpers:
  add object registration, edge attachment, chunk attachment, and optional
  runtime/operational additions.

## 3. Richer Read/Query Result Shapes

### Feature

Generic read surfaces that can return a root object plus bounded related
context, not only flat node lists.

### Remaining Items For Acceptance

- define bounded traversal result shapes
- define how related context is grouped in a way that matches the current DSL
  and can map cleanly to future richer query frontends
- add expanded generic predicates, especially timestamp and numeric comparison
- define search-result enrichment so text/vector hits can return selected
  related context in one logical query

Acceptance criteria:

- a bounded 1-2 hop read can return root plus grouped neighbors in one result
- common drill-in views no longer require N follow-up lookups per hit
- query compilation remains deterministic and bounded

### Design Outline

- Keep the query language generic and graph-shaped rather than adding
  Memex-specific read verbs.
- Keep the existing flat query result path intact and add a parallel richer
  grouped-result type rather than replacing the flat surface immediately.
- Extend the query/result model from “row list” to “root rows plus attached
  related sets,” returned by the grouped-result path.
- Group related context by semantic query-defined expansion slots. In the
  current DSL, those slots are declared explicitly on bounded expansion
  clauses. The slot identity is part of the query contract and is not merely
  an incidental AST index or an edge-label bucket.
- Define expansion slots generically enough that a future Cypher layer can map
  pattern/projection aliases onto the same grouped-result model.
- Limit traversal depth to bounded, explicit hops to preserve predictable query
  planning and output shape.
- Grouped expansion execution must honor the same hard-limit budget used by
  flat traversal reads so one enriched root cannot walk or materialize an
  unbounded reachable subgraph.
- Add timestamp and numeric predicate support as first-class generic filters so
  application code does not have to overuse JSON text matching.
- Treat search-result enrichment as composition: search defines roots, then a
  bounded enrichment clause attaches selected neighbors and metadata through
  named expansion slots.

## 4. Lightweight `last_accessed` Support

### Feature

A generic high-churn metadata update path that avoids full semantic-row
supersession on every read.

### Remaining Items For Acceptance

- choose the primary substrate shape for `last_accessed`
- define write semantics for batched touch/update
- define how the chosen approach interacts with provenance, recovery, and
  query/read paths
- expose the chosen path consistently in Rust and Python

Acceptance criteria:

- touching many logical IDs in one operation is cheaper than full node
  supersession for each item
- the chosen path remains recoverable and visible to integrity/admin tooling
- read paths can consume the resulting `last_accessed` data without ad hoc
  side-channel SQL

### Design Outline

- Choose batched touch/update as the primary design.
- Use a dedicated engine-owned metadata table keyed by `logical_id` as the
  primary substrate, rather than operational-store backing.
- The API accepts a bounded list of logical IDs and one timestamp, then
  updates lightweight access metadata in one operation without creating new
  node versions.
- Batch semantics are strict: reject empty batches, reject any batch that
  contains an unknown or inactive logical ID, and apply nothing on failure.
- Touches remain ordinary engine writes for provenance purposes. In strict
  provenance mode the request must carry a source reference or the batch is
  rejected before mutation.
- De-duplicate repeated logical IDs before applying updates or reporting the
  touched count.
- Surface `last_accessed_at` as a separate optional read field on node results
  rather than mutating node properties.
- Treat this as a generic substrate primitive rather than a Memex-only feature.
- Record bounded provenance for touches so the write remains recoverable and
  auditable without turning access metadata into a new semantic object type.
- Preserve access metadata across ordinary upsert and retire/restore because
  the stable key is `logical_id`; remove it on logical purge and when source
  excision leaves no active node for the logical ID.
- Let semantic/integrity checks detect truly orphaned access metadata rows that
  no longer correspond to any surviving node history.
- Keep operator tooling in sync with the engine semantic contract so new access
  metadata inconsistencies are surfaced through the Go layer-2 diagnostic path,
  not only the Rust admin report.
- Preserve append-only access-event materialization as a possible later
  extension if access analytics become important.

## 5. Filtered Reads For Operational Collections

### Feature

Read support for non-key filtering over operational collections, especially
`append_only_log` collections such as `audit_log`.

### Remaining Items For Acceptance

- define the allowed filter model for operational reads
- define how collections declare or opt into indexed/filterable fields
- define which collection kinds can use which filters efficiently

Acceptance criteria:

- `audit_log`-style reads can filter by declared payload fields without full
  client-side scans of large histories
- filter support stays bounded and explicit rather than becoming arbitrary SQL
- the design composes with export/recovery/bootstrap and does not create a raw
  table-escape hatch

### Design Outline

- Declare filterability in a dedicated engine-owned contract field
  (`filter_fields_json`), not inside `schema_json` or `retention_json`.
- Older collections need an explicit post-upgrade contract update path for
  `filter_fields_json`; do not infer filterable fields from historical payloads
  during migration.
- Add a separate filtered read surface rather than overloading
  `trace_operational_collection`; trace remains the diagnostic/history API.
- Prefer a narrow model: conjunctive exact/prefix/range filters over declared
  top-level payload fields, not arbitrary JSON predicates or nested boolean
  logic.
- Support `append_only_log` in v1 and reject unsupported collection kinds such
  as `latest_state` clearly rather than degrading silently.
- Materialize engine-owned extracted filter values for declared fields in a
  dedicated table so `audit_log`-style reads do not require client-side scans
  or arbitrary payload-JSON querying.
- Updating `filter_fields_json` for an existing collection must rebuild the
  extracted filter values for that collection’s existing mutation history in
  the same transaction so upgraded databases can use filtered reads without
  re-registering or rewriting the collection.
- Bridge/CLI filter transport must preserve explicit zero-valued range bounds;
  `0` is a valid timestamp/integer boundary, not equivalent to “bound absent”.
- Treat declared field types as read semantics only: `schema_json` remains
  documentation-only by default, so write-time payload validation is still
  deferred.
- Keep exact-key and history streaming semantics unchanged; filtered reads are
  an additional access mode, not a replacement.

## 6. Operational Payload Schema Validation

### Feature

Optional runtime validation of operational payloads beyond documentation-only
`schema_json`.

### Remaining Items For Acceptance

- define what `schema_json` means if validation is enabled
- define collection-level opt-in behavior
- define compatibility/evolution behavior when payload shapes change

Acceptance criteria:

- validation can be enabled without breaking existing documentation-only
  collections by surprise
- invalid payloads fail deterministically with operator-visible errors
- schema evolution remains possible without making old collections unrecoverable

### Design Outline

- Keep v1 behavior as-is: `schema_json` remains documentation-only by default.
- If validation is added, make it explicitly opt-in per collection or per
  format version.
- Support only a narrow JSON-shape contract initially: required fields, simple
  scalar/object/list types, and optional fields.
- Treat validation as write-time contract enforcement only; do not add a broad
  query-time schema engine.
- Version the contract so future stricter validation does not reinterpret old
  collection metadata incorrectly.

## Recommended Design Sequence

1. Restore/purge lifecycle APIs
2. Write-bundle builders and request-local references
3. Richer read/query result shapes
4. Lightweight `last_accessed` support
5. Filtered operational reads
6. Optional operational payload validation

## Bottom Line

The remaining Memex-relevant work is no longer about domain modeling. It is a
set of generic substrate design tasks:

- complete lifecycle semantics
- improve write ergonomics
- improve read result shape expressiveness
- support lightweight high-churn metadata
- selectively deepen the operational store contract

Those are the items that still require real design before implementation.
