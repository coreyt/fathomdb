# FathomDB Memex-Support Implementation Tracker

## Status

- Current phase: complete
- Overall status: operational-store foundation, all planned Memex-support
  phases, and the follow-on operational hardening slices are implemented

## Locked Decisions

- The operational-store tranche is complete and remains the foundation for
  high-churn operational state.
- Canonical operational history lives in `operational_mutations`.
- `operational_current` is rebuildable and never treated as canonical.
- v1 operational collection kinds remain `append_only_log` and `latest_state`.
- `Increment` remains deferred and is not required for Memex adoption.
- `schema_json` remains documentation-only metadata by default.
- `scheduled_tasks` and `notifications` remain graph-native non-goals.
- The next Memex-support phases are generic substrate work, not Memex-specific
  schema work.
- Restore must re-establish full pre-retire content, not only reactivate the
  logical row.
- Restore must choose the latest retired revision by durable lifecycle
  ordering; second-precision timestamps alone are not sufficient when updates
  or retires happen rapidly.
- Retire must preserve the state required for later full-fidelity restore;
  purge is the irreversible lifecycle action that destroys that preserved
  state.
- Preserved retired state is intentional lifecycle material and must be
  understood by provenance, integrity, recovery, and admin flows.
- Disabled operational collections must reject new writes based on collection
  state observed inside the write transaction, not only a preflight lookup.
- Phase order:
  1. restore/purge lifecycle APIs
  2. write-bundle builders and request-local references
  3. richer read/query result shapes
  4. lightweight `last_accessed` support
  5. filtered operational reads
- Operational payload validation now ships as an opt-in `validation_json`
  contract with `disabled`, `report_only`, and `enforce` modes plus
  history-validation diagnostics; generic write warnings travel through
  `WriteReceipt.warnings`.
- Collection-declared operational secondary indexes now ship through
  `secondary_indexes_json` plus engine-maintained derived entries and explicit
  rebuild support.
- Operational retention now ships through explicit plan/run admin primitives
  and operator tooling, while recurring scheduling remains intentionally
  external to the engine.
- TDD is mandatory for all remaining phases: write failing requirement-level
  tests first, implement the minimum behavior to pass, refactor after green,
  then update this tracker.
- Phase 3 uses parallel query result types: the existing flat `QueryRows`
  surface remains stable, and richer root-plus-context reads use a separate
  grouped result path.
- Phase 3 groups related context by semantic query-defined expansion slots, so
  the current DSL and a future Cypher-style frontend can target the same
  grouped result model without relying on edge-label buckets alone.
- Phase 4 uses a dedicated engine-owned `node_access_metadata` table keyed by
  `logical_id`, not operational-store backing.
- Phase 4 exposes `last_accessed_at` as a separate optional node read field,
  not as a mutation of node properties.
- Phase 4 touch batches reject the whole request if any logical ID is unknown
  or inactive, and de-duplicate repeated IDs before apply/reporting.
- Phase 4 touch batches remain subject to provenance policy: when
  `ProvenanceMode::Require` is active they must carry a source reference.
- Phase 5 declares operational filterability in a dedicated
  `filter_fields_json` contract field rather than overloading `schema_json` or
  `retention_json`.
- Phase 5 adds a separate `read_operational_collection` surface and keeps
  `trace_operational_collection` stable as the diagnostic/history API.
- Phase 5 filtered reads are bounded, conjunctive, and append-only-first:
  exact/prefix/range over declared top-level payload fields for
  `append_only_log`, with unsupported collection kinds rejected clearly.
- Phase 5 uses engine-maintained extracted filter values for declared fields so
  `audit_log`-style reads do not require client-side filtering or arbitrary
  payload-JSON querying.
- Phase 5 includes an explicit post-upgrade filter-contract update path for
  preexisting operational collections; filterability is never inferred from
  historical payloads during schema migration.
- Phase 5 bridge/CLI range filters must preserve explicit zero-valued bounds;
  `0` is a valid integer/timestamp boundary, not the same as “unset”.

## Completed Foundation

### Operational-store tranche

- [x] Schema/bootstrap for `operational_collections`, `operational_mutations`,
  and `operational_current`
- [x] Writer/runtime support for `Append`, `Put`, and `Delete`
- [x] Integrity/semantics/admin support for operational collections
- [x] Bridge/Go recovery/CLI support for operational collections
- [x] Rust/Python operational admin surface
- [x] Disable/compact/purge lifecycle for operational collections
- [x] Source-based trace/excise support for `operational_mutations`

### Memex gaps closed by the operational-store tranche

- [x] operational-state modeling for high-churn bookkeeping
- [x] atomic graph + operational writes
- [x] same-file recovery/admin/export coverage for operational collections

## Active Phases

### 1. Restore/Purge Lifecycle APIs

- [x] TDD slice 1: add failing requirement-level tests for reversible
  retire/restore behavior before changing lifecycle semantics
- [x] TDD slice 1: implement the minimum lifecycle/model changes so retire
  preserves the prerequisites for later full-fidelity restore
- [x] Define and implement the reversible retire/restore lifecycle contract:
  - restore reverses retire at the logical-id lifecycle layer
  - restore is currently scoped to the latest retire scope identified by
    logical_id plus retire provenance timestamp/source_ref
  - restore of an active object is deterministic and non-destructive
- [x] Define and implement the preserved restorable state model:
  - pre-retire object content is restorable
  - directly related retired edges in scope are restorable
  - chunks are restorable
  - vec rows are preserved directly; FTS rows are rebuilt deterministically
    during restore
- [x] Define and implement purge finality under the reversible-retire model:
  - purge is the only irreversible lifecycle action
  - purge removes the preserved retired state that made restore possible
  - purge cascade scope is explicit and bounded across directly attached edges,
    chunks, FTS rows, vec rows, and restore-only retained state
  - purge of active/restored objects is currently a deterministic no-op rather
    than a destructive live delete
- [x] Define and implement restore/purge scope reporting, provenance, and
  bounded audit/tombstone behavior:
  - restore reports what was re-established
  - purge reports what was irreversibly removed
  - restore and purge are provenance-visible and linked to the retire scope
    they reverse or destroy
- [x] Add logical-id admin APIs for restore and purge
- [x] Add bridge / SDK surface for restore and purge if the admin APIs land
- [x] TDD slice 2: add failing requirement-level tests for integrity,
  recovery, and lifecycle proof obligations before wiring the final admin
  surface
- [x] Prove lifecycle correctness for:
  - purge plus vec cleanup
  - excision plus vec cleanup
  - restore/purge interaction with regenerated vectors
  - valid preserved-retire state vs broken restore prerequisites in integrity
    and recovery flows

Acceptance criteria:

- restoring a retired object re-establishes its last pre-retire active content
  state, not a degraded subset
- restore re-establishes the object together with directly related retired
  edges, chunks, and the projection state needed for search/vector behavior to
  match the pre-retire object again
- restore never depends on application re-ingest, external replay, or manual
  operator repair to regain pre-retire content
- restore deterministically revives the last pre-retire revision even when
  several retires share the same second-level timestamp bucket
- purge is the only irreversible lifecycle action and removes no less and no
  more than the documented purge scope
- purge leaves no orphaned canonical rows, edges, chunks, FTS rows, vec rows,
  or retained restore-only state
- restore and purge are visible in provenance/admin tooling with operator-
  meaningful scope reporting
- failed restore is diagnosable and clearly reports why full restoration was
  impossible
- integrity, semantic checks, and recovery flows agree with lifecycle outcomes
  and can distinguish valid preserved-retire state from broken restore
  prerequisites

### 2. Write-Bundle Builders And Request-Local References

- [x] Add failing Rust tests for bundle authoring of multi-object graph writes
- [x] Add failing Python tests for equivalent builder behavior
- [x] Add client-side Rust bundle builders that compile to ordinary `WriteRequest`
- [x] Add matching Python builder/helpers
- [x] Add request-local symbolic references resolved at build time
- [x] Add pre-submit validation for unresolved/invalid request-local references

Acceptance criteria:

- common Memex write flows can be expressed without hand-wiring every ID
- builder output is an ordinary `WriteRequest`
- invalid request-local references fail before submit

### 3. Richer Read/Query Result Shapes

- [x] Add failing tests for bounded root-plus-context reads
- [x] Add failing tests for timestamp and numeric predicates
- [x] Extend query/result surfaces beyond flat node lists with a parallel
  grouped result path
- [x] Add bounded grouping by semantic query-defined expansion slots
- [x] Add search-result enrichment support

Acceptance criteria:

- a bounded 1-2 hop read returns root plus grouped related state in one result
- common drill-in views no longer require N follow-up lookups per hit
- query compilation remains deterministic and bounded

### 4. Lightweight `last_accessed` Support

- [x] Add failing tests for batched touch/update semantics
- [x] Add a bounded touch/update API that avoids full node supersession
- [x] Add integrity/admin visibility for the chosen metadata path
- [x] Expose the surface through Rust and Python bindings

Acceptance criteria:

- touching many logical IDs is cheaper than per-node supersession
- `last_accessed` remains recoverable and admin-visible
- read paths can consume the resulting metadata without side-channel SQL

### 5. Filtered Operational Reads

- [x] Add failing tests for declared-field filtering on operational collections
- [x] Add filter metadata to the collection contract as needed
- [x] Add bounded exact/prefix/range filtering for declared operational fields
- [x] Add `append_only_log` coverage for `audit_log`-style reads

Acceptance criteria:

- `audit_log`-style reads can filter by declared fields without full client-side scans
- filter support remains bounded and does not become arbitrary SQL
- export/recovery/bootstrap remain compatible with the collection contract

## Follow-On Slices Completed

- [x] `report_only` operational payload validation warnings through the generic
  write-receipt warning surface
- [x] collection-declared secondary indexes through
  `secondary_indexes_json`, derived index entries, and rebuild support
- [x] automatic/background retention primitives through explicit
  plan/run retention admin and operator surfaces

Design notes:

- [`dev/design-operational-payload-schema-validation.md`](/home/coreyt/projects/fathomdb/dev/design-operational-payload-schema-validation.md)
- [`dev/design-operational-secondary-indexes.md`](/home/coreyt/projects/fathomdb/dev/design-operational-secondary-indexes.md)
- [`dev/design-automatic-background-retention.md`](/home/coreyt/projects/fathomdb/dev/design-automatic-background-retention.md)

Implementation plans:

- [`dev/plan-operational-payload-schema-validation.md`](/home/coreyt/projects/fathomdb/dev/plan-operational-payload-schema-validation.md)
- [`dev/plan-operational-secondary-indexes.md`](/home/coreyt/projects/fathomdb/dev/plan-operational-secondary-indexes.md)
- [`dev/plan-automatic-background-retention.md`](/home/coreyt/projects/fathomdb/dev/plan-automatic-background-retention.md)

## Verification

### Foundation verification

- [x] Rust schema/bootstrap tests
- [x] Rust writer/admin tests
- [x] Rust crate facade tests
- [x] Go bridge/client/CLI tests
- [x] Python binding tests

### Remaining-phase verification

- [x] All remaining verification stays requirement-first, not code-first
- [x] Full-fidelity restore tests for the initial logical-id lifecycle slice
- [x] Purge scope/finality tests for the initial logical-id lifecycle slice
- [x] Restore/purge reporting and provenance tests for the initial
  logical-id lifecycle slice
- [x] Preserved-retire integrity tests for retained chunks in the initial
  lifecycle slice
- [x] Recovery-flow proof coverage for preserved retired state
- [x] Lifecycle/vector interaction proof tests
- [x] Write-bundle builder tests in Rust and Python
- [x] Richer read/query result tests
- [x] Batched touch/update tests
- [x] Filtered operational read tests
- [x] Report-only operational payload validation tests
- [x] Operational secondary-index maintenance/rebuild/read-path tests
- [x] Operational retention plan/run tests

## Notes

- Keep this file updated after each completed step and after any decision changes.
- The operational-store tranche is complete and should not be reopened except
  where later phases deliberately extend its public contract.
- The first remaining phase is a lifecycle-model redesign, not a trivial admin
  add-on. The first engine slice now preserves retired chunks/vec state,
  rebuilds FTS on restore, adds logical-id restore/purge admin APIs plus
  Rust / Python / bridge / Go operator surfaces, and now has requirement-level
  proof coverage for recover, excision, purge, and vector interaction.
- Recover proof coverage required a schema bootstrap hardening fix: after
  `.recover` resets `fathom_schema_migrations`, operational-store migrations
  must still be idempotent against recovered tables that already contain
  `mutation_order`.
- Follow-up correctness hardening closed two semantic gaps discovered in
  review: restore now uses stable lifecycle ordering when same-second retire
  timestamps tie, and disabled operational collections are re-checked inside
  the write transaction before any mutation/current-row writes occur.
- Phase 1 is complete in the requirement-level sense captured here and in the
  corresponding design notes.
- Phase 2 is complete: Rust and Python now have client-side `WriteRequest`
  builders with request-local handles that compile to ordinary `WriteRequest`,
  and the Python manual `WriteRequest` surface now exposes `operational_writes`
  in parity with the Rust write protocol.
- Python SDK parity fixes closed two follow-up review gaps: the public
  `AdminClient` now exposes the operational collection lifecycle APIs, and
  optional projection backfill payloads preserve the older raw-JSON-string
  calling convention while still accepting structured Python values.
- Phase 3 is complete: Rust and Python now expose a parallel grouped query
  result path with named expansion slots, bounded search enrichment, and JSON
  integer/timestamp comparison predicates while preserving the flat `QueryRows`
  surface unchanged.
- The grouped result path is intentionally keyed by semantic expansion-slot
  identity rather than edge-label buckets so the current DSL and a future
  Cypher-style frontend can share the same richer result model.
- Grouped expansion execution must honor the query hard limit just as flat
  traversal execution does; enriched reads are bounded by design, not just by
  convention.
- Restore edge scope is keyed by durable provenance ordering from the latest
  node-retire event, not by exact same-second timestamp equality with
  separately retired adjacent edges.
- Phase 4 is complete: Rust and Python now expose a bounded
  `touch_last_accessed` API backed by a dedicated `node_access_metadata`
  table, node reads surface `last_accessed_at` directly, touches emit bounded
  provenance, and lifecycle operations clean up access metadata on purge and
  on source excision when no active node remains.
- Follow-up hardening closed four review gaps: `touch_last_accessed` now obeys
  strict provenance mode, restore edge scope uses durable provenance ordering
  rather than exact timestamp equality, grouped expansion execution is capped
  by the query hard limit, and Go layer-2 checks surface orphaned access
  metadata rows reported by the engine.
- Go layer-2 checks must mirror newly added engine semantic counters so
  recovery tooling does not under-report access-metadata inconsistencies.
- Phase 5 is complete: operational collections can now declare bounded
  filterable fields in `filter_fields_json`, append-only histories can be read
  through a separate `read_operational_collection` API, filtering is limited
  to conjunctive exact/prefix/range predicates over declared top-level payload
  fields, and Rust/Python/Go surfaces all preserve `trace_operational_collection`
  as the unchanged diagnostic history path.
- Follow-up hardening closed two Phase 5 review gaps: preexisting collections
  can now gain declared filter contracts through an explicit update path that
  backfills extracted filter values for existing mutation history, and Go
  bridge/CLI range filters now preserve explicit `0` lower/upper bounds.
- The formerly deferred operational hardening items are now implemented:
  `report_only` validation uses generic write warnings, collection-declared
  secondary indexes use derived engine-managed entries plus rebuild support,
  and retention planning/execution is available through explicit plan/run
  surfaces with recurring scheduling kept outside the engine by design.
- Stale vector-doc cleanup is not tracked as a standalone phase; its remaining
  proof obligations are carried by the restore/purge lifecycle phase.
- Python-feature verification remains environment-dependent for full Rust
  `--features python` test execution on this machine.
