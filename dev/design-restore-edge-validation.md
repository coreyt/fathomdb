# Design: Restore Edge Endpoint Validation

## Purpose

Address the verified finding that `restore_logical_id` can create dangling
edges (H-3). Edges are restored without validating that their source and
target nodes still exist or are active.

---

## Current State

`crates/fathomdb-engine/src/admin.rs:1691-1719`

`restore_logical_id()` restores a node by unsetting `superseded_at`, then
collects associated edge logical IDs via `collect_edge_logical_ids_for_restore()`
and restores them similarly. The edge collection query correctly excludes
edges that already have an active (non-retired) version, but does not
check whether the *other endpoint* of each edge still exists.

If the counterpart node was purged between the original retire and the
restore call, the restored edge points to a non-existent node — a
referential integrity violation that `check_semantics()` will report.

---

## Design

### Validate edge endpoints before restore

After collecting edge candidates but before restoring them, check that
both endpoints are active (or being restored in the same operation):

```rust
let restoring_logical_id = logical_id; // the node being restored

let mut valid_edges = Vec::new();
let mut skipped_edges = Vec::new();

for edge_logical_id in candidate_edge_logical_ids {
    let edge = load_edge_by_logical_id(&tx, &edge_logical_id)?;

    let other_endpoint = if edge.source_logical_id == restoring_logical_id {
        &edge.target_logical_id
    } else {
        &edge.source_logical_id
    };

    let endpoint_active = node_is_active(&tx, other_endpoint)?;

    if endpoint_active {
        valid_edges.push(edge_logical_id);
    } else {
        skipped_edges.push(SkippedEdge {
            edge_logical_id,
            missing_endpoint: other_endpoint.clone(),
        });
    }
}
```

### Report skipped edges

Add `skipped_edges` to the existing `RestoreReport`:

```rust
pub struct RestoreReport {
    pub node_restored: bool,
    pub edges_restored: u64,
    pub edges_skipped: Vec<SkippedEdge>,
    // ... existing fields ...
}

pub struct SkippedEdge {
    pub edge_logical_id: String,
    pub missing_endpoint: String,
}
```

Skipped edges are not an error — they are an expected consequence of
partial purge followed by restore. The report gives the operator
visibility into what was not restored and why.

### Why not error on missing endpoints?

Erroring would make `restore_logical_id` fail whenever any edge endpoint
has been purged. This is too strict — the operator's intent is to restore
the node and as many of its relationships as possible. Partial restore
with a report is the correct UX.

### Edge case: both endpoints retired, only one being restored

If node A and node B are both retired, and the operator restores only
node A, edges between A and B should *not* be restored (B is still
retired, not purged). The existing `collect_edge_logical_ids_for_restore`
query handles this correctly — it only collects edges where the
counterpart is NOT retired. The new validation adds a check for the case
where the counterpart was *purged* (deleted entirely).

---

## Test Plan

- Retire two connected nodes. Purge one. Restore the other. Verify the
  edge is skipped and reported, not restored.
- Retire two connected nodes. Restore both. Verify the edge is restored.
- Retire a node with edges to active nodes. Restore. Verify all edges
  are restored.
- Verify `check_semantics()` reports no issues after restore with
  skipped edges.
