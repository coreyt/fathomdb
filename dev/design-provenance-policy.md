# Design: Explicit Provenance Policy

## Purpose

This document resolves the open Phase 2 item from
[design-typed-write.md](./design-typed-write.md):

> Require an explicit provenance policy for canonical rows that should be
> traceable or excisable.

The engine needs a clear contract for when `source_ref` is required, what
happens when it is missing, and how provenance discipline enables the repair
tooling described in
[design-repair-provenance-primitives.md](./design-repair-provenance-primitives.md).

## Current State

`source_ref` is an `Option<String>` on all canonical insert types:

```rust
pub struct NodeInsert {
    ...
    pub source_ref: Option<String>,
    ...
}
```

Missing `source_ref` produces a warning in `WriteReceipt.provenance_warnings`
but does not reject the write:

```rust
let provenance_warnings: Vec<String> = prepared
    .nodes
    .iter()
    .filter(|node| node.source_ref.is_none())
    .map(|node| format!("node '{}' has no source_ref", node.logical_id))
    .collect();
```

Only nodes are checked. Edges, runs, steps, and actions with missing
`source_ref` produce no warnings.

The admin surface (`trace_source`, `excise_source`) queries rows by
`source_ref`. Rows with `NULL` `source_ref` are invisible to both operations:
they cannot be traced and cannot be excised.

## The Problem

The current behavior is permissive enough to be dangerous:

1. **Silent untraceable rows.** A node written without `source_ref` cannot be
   surgically removed by `excise_source`. If that node's content is wrong, the
   only remediation is manual SQL or a full rebuild. This defeats the purpose
   of provenance-based repair.

2. **Incomplete warning coverage.** Only nodes produce warnings. An edge or
   run without `source_ref` is equally untraceable but triggers no signal.

3. **No caller feedback loop.** Warnings are in the `WriteReceipt`, but
   callers may ignore the receipt entirely. There is no enforcement gradient
   between "soft warning" and "hard reject."

## Design: Tiered Provenance Enforcement

### Tier Definitions

The engine supports two provenance modes, selected at `EngineOptions` time:

| Mode | Behavior |
|---|---|
| `ProvenanceMode::Warn` | Missing `source_ref` produces warnings on all canonical types. Writes succeed. This is the current behavior, extended to cover all types. |
| `ProvenanceMode::Require` | Missing `source_ref` on any canonical row rejects the write with `EngineError::InvalidWrite`. No partial writes. |

The default mode is `Warn`. Callers opt into `Require` when their use case
demands full traceability (e.g., production agents that need surgical repair
capability).

### Why Not Always Require

1. **Bootstrap and migration.** Seeding an initial database from bulk import
   may not have meaningful `source_ref` values for every row. Requiring
   provenance during bootstrap creates friction without benefit: bulk-imported
   data can be re-imported if it is bad.

2. **Development and testing.** Test fixtures and manual exploration should not
   require provenance boilerplate.

3. **Gradual adoption.** Callers integrating fathomdb incrementally can start
   with `Warn` and switch to `Require` once their provenance pipeline is
   reliable.

### Why Not Always Warn

Production agents that rely on `excise_source` for repair need a guarantee
that every canonical row is traceable. A warning that nobody reads is not a
guarantee. `Require` mode provides a hard contract.

## Detailed Behavior

### Warn Mode

`prepare_write()` collects warnings for every canonical insert type that has
`source_ref: None`:

- Nodes: `"node '{logical_id}' has no source_ref"` (already implemented)
- Edges: `"edge '{logical_id}' has no source_ref"` (new)
- Runs: `"run '{id}' has no source_ref"` (new)
- Steps: `"step '{id}' has no source_ref"` (new)
- Actions: `"action '{id}' has no source_ref"` (new)

Retire operations (`NodeRetire`, `EdgeRetire`) also warn if `source_ref` is
`None`:

- `"node retire '{logical_id}' has no source_ref"` (new)
- `"edge retire '{logical_id}' has no source_ref"` (new)

All warnings are collected in `WriteReceipt.provenance_warnings`. The write
succeeds.

### Require Mode

`prepare_write()` performs the same checks. If any canonical insert or retire
operation has `source_ref: None`, the write is rejected with:

```
EngineError::InvalidWrite("provenance required: node '{logical_id}' has no source_ref")
```

The rejection happens before the write reaches the writer thread. No partial
writes are possible.

### Where the Mode Lives

```rust
pub enum ProvenanceMode {
    Warn,
    Require,
}

pub struct EngineOptions {
    pub db_path: PathBuf,
    pub provenance_mode: ProvenanceMode,  // default: Warn
    ...
}
```

The mode is set once at engine open time and cannot change for the lifetime of
the engine instance. This avoids confusion about which writes were validated
under which mode.

## source_ref Format

The engine does not enforce a particular `source_ref` format in v1. It is an
opaque string. Callers choose a scheme that works for their provenance model:

- `"run:abc123"` — trace to a specific agent run
- `"import:2024-01-15"` — trace to a bulk import batch
- `"user:manual"` — trace to manual operator input

The engine stores, queries, and returns `source_ref` values without
interpretation. `trace_source` and `excise_source` match on exact string
equality.

A future version may add structured `source_ref` parsing (e.g., typed
`SourceRef` with `kind` and `id` fields), but this is out of scope for v1.

## ChunkInsert and source_ref

`ChunkInsert` does not carry `source_ref`. Chunks inherit traceability from
their parent node: `trace_source` can reach chunks by joining through
`node_logical_id` to the node's `source_ref`.

This is a deliberate choice. Chunks are derived content (text segments of a
node). They do not have independent provenance. If a node is excised, its
chunks are deleted (per the chunk lifecycle design in
[design-detailed-supersession.md](./design-detailed-supersession.md)).

`ChunkInsert` is therefore **exempt** from provenance warnings and
`Require` mode validation.

## Implementation Plan

### Task 1: Extend Warnings to All Canonical Types

Add provenance warning collection for edges, runs, steps, and actions in
`prepare_write()` (or the equivalent provenance-check stage).

Tests:
1. `writer_receipt_warns_on_edge_without_source_ref`
2. `writer_receipt_warns_on_run_without_source_ref`
3. `writer_receipt_warns_on_step_without_source_ref`
4. `writer_receipt_warns_on_action_without_source_ref`
5. `writer_receipt_no_warnings_when_all_types_have_source_ref` — submit one of
   each type with `source_ref`, assert zero warnings

Files: `crates/fathomdb-engine/src/writer.rs`

### Task 2: Add ProvenanceMode to EngineOptions

Add `ProvenanceMode` enum and a `provenance_mode` field to `EngineOptions`.
Default to `Warn`. Thread the mode through to `prepare_write`.

Tests:
1. `default_provenance_mode_is_warn` — open engine with default options,
   submit node without source_ref, assert write succeeds with warning
2. `require_mode_rejects_node_without_source_ref` — open engine with
   `Require`, submit node without source_ref, assert `InvalidWrite`
3. `require_mode_accepts_node_with_source_ref` — open engine with `Require`,
   submit node with source_ref, assert success
4. `require_mode_rejects_edge_without_source_ref`

Files: `crates/fathomdb-engine/src/coordinator.rs`,
`crates/fathomdb-engine/src/writer.rs`,
`crates/fathomdb-engine/src/lib.rs`,
`crates/fathomdb/src/lib.rs`

### Task 3: Add Retire Provenance Warnings

When retire operations are implemented (per
[design-detailed-supersession.md](./design-detailed-supersession.md)), add
provenance warnings for `NodeRetire` and `EdgeRetire` with missing
`source_ref`. Both modes apply the same logic: warn or reject.

This task depends on the supersession implementation and can be done in the
same PR.

## Done When

- All canonical insert types produce provenance warnings when `source_ref` is
  missing (Warn mode)
- `Require` mode rejects writes with missing `source_ref`
- `ChunkInsert` is explicitly exempt
- `ProvenanceMode` is set at engine open time and documented
- All tests pass
