# Admin & Operator Operations Reference

All operations are available through the Python SDK (`AdminClient`) and most
through the `fathom-integrity` CLI. Both invoke the same Rust engine via the
admin bridge binary.

Common CLI flags: `--db <path>` (SQLite database), `--bridge <path>` (admin bridge binary).

---

## 1. Integrity & Diagnostics

**check_integrity** -- SQLite `integrity_check`, foreign-key validation, FTS consistency.

```python
report = admin.check_integrity()
```
```sh
fathom-integrity check --db /data/my.db --bridge ./fathomdb-admin-bridge
```

The `--bridge` flag is optional; without it only Layer 1 (raw SQLite) checks
run. With the bridge, Layer 2 engine-level checks are included.

**check_semantics** -- Orphaned chunks, dangling edges, stale projections, null `source_ref` values.

```python
report = admin.check_semantics()
```

Semantic checks are included in `check` when the bridge is provided.

---

## 2. Source Provenance

**trace_source** -- List every object (nodes, edges, chunks) written by a `source_ref`.

```python
report = admin.trace_source("ingest-batch-42")
```
```sh
fathom-integrity trace --db /data/my.db --bridge ./fathomdb-admin-bridge \
  --source-ref ingest-batch-42
```

**excise_source** -- Soft-delete all objects from a source and restore prior versions.

```python
report = admin.excise_source("ingest-batch-42")
```

No dedicated CLI subcommand; use the Python SDK or invoke the bridge directly.

**purge_provenance_events** -- Delete provenance events older than a timestamp. Optionally preserve specific event types. Supports `dry_run`.

```python
report = admin.purge_provenance_events(
    before_timestamp=1711670400, preserve_event_types=["schema_migration"], dry_run=True)
```
```sh
fathom-integrity purge-provenance-events --db /data/my.db --bridge ./fathomdb-admin-bridge \
  --before-timestamp 1711670400 --preserve-event-types schema_migration,audit
```

---

## 3. Object Lifecycle

**restore_logical_id** -- Reactivate a retired node and its edges. Validates that edge endpoints still exist to prevent dangling references.

```python
report = admin.restore_logical_id("node-abc-123")
```
```sh
fathom-integrity restore-logical-id --db /data/my.db --bridge ./fathomdb-admin-bridge \
  --logical-id node-abc-123
```

**purge_logical_id** -- Permanently delete all versions of a retired logical ID. Irreversible.

```python
report = admin.purge_logical_id("node-abc-123")
```
```sh
fathom-integrity purge-logical-id --db /data/my.db --bridge ./fathomdb-admin-bridge \
  --logical-id node-abc-123
```

---

## 4. Projections

**rebuild** -- Drop and rebuild projection indexes. Target: `fts`, `vec`, or `all` (default).

```python
report = admin.rebuild(ProjectionTarget.FTS)
```
```sh
fathom-integrity rebuild --db /data/my.db --bridge ./fathomdb-admin-bridge --target fts
```

**rebuild_missing** -- Rebuild only missing projection entries without touching existing ones.

```python
report = admin.rebuild_missing()
```
```sh
fathom-integrity rebuild-missing --db /data/my.db --bridge ./fathomdb-admin-bridge
```

---

## 5. FTS Property Schemas

FTS property schemas declare which JSON property paths should be extracted and
indexed for full-text search on structured node kinds. Once registered,
`text_search(...)` transparently covers both chunk-backed document text and
property-backed structured text via a UNION query. The search expression uses
the same safe subset documented in the querying guide: terms, quoted phrases,
implicit `AND`, uppercase `OR`, and uppercase `NOT`. Unsupported syntax stays
literal rather than passing through as raw FTS5 control syntax.

### Schema Lifecycle

**register_fts_property_schema** -- Register (or update) an FTS property
projection for a node kind. This is an idempotent upsert. Paths must use
simple `$.`-prefixed dot-notation (e.g. `$.title`, `$.address.city`). Array
indexing, wildcards, and recursive descent are rejected. Registration does
not rewrite existing FTS rows; run `rebuild(fts)` to backfill.

```python
record = admin.register_fts_property_schema(
    "Goal", ["$.name", "$.description"], separator=" ")
```
```typescript
const record = engine.admin.registerFtsPropertySchema(
    "Goal", ["$.name", "$.description"]);
```

**describe_fts_property_schema** -- Return the schema for a single kind, or
`None`/`null` if not registered.

```python
record = admin.describe_fts_property_schema("Goal")
```

**list_fts_property_schemas** -- Return all registered schemas.

```python
schemas = admin.list_fts_property_schemas()
```

**remove_fts_property_schema** -- Delete the schema row for a kind. This does
**not** delete existing derived `fts_node_properties` rows; an explicit
`rebuild(fts)` is required to clean them up. Errors if the kind is not
registered.

```python
admin.remove_fts_property_schema("Goal")
admin.rebuild(ProjectionTarget.FTS)  # clean up stale derived rows
```

### Diagnostics

`check_integrity` reports `missing_property_fts_rows` (active nodes that
should have a property FTS row but don't). `check_semantics` reports:

| Field | Meaning |
|---|---|
| `stale_property_fts_rows` | Rows for superseded/missing nodes |
| `orphaned_property_fts_rows` | Rows for unregistered schema kinds |
| `mismatched_kind_property_fts_rows` | Rows whose kind differs from the active node |
| `duplicate_property_fts_rows` | Logical IDs with more than one property FTS row |
| `drifted_property_fts_rows` | Rows whose text no longer matches canonical extraction |

### Export & Recovery

`fts_property_schemas` is canonical metadata and is preserved by `safe_export`.
`fts_node_properties` rows are derived state and rebuildable. Recovery
correctness must not depend on `fts_node_properties` contents -- run
`rebuild(fts)` after importing an export to restore property FTS from
canonical state.

---

## 6. Safe Export

Consistent backup with optional WAL checkpoint and SHA-256 manifest.

```python
manifest = admin.safe_export("/backups/snapshot", force_checkpoint=True)
```
```sh
fathom-integrity export --db /data/my.db --bridge ./fathomdb-admin-bridge \
  --out /backups/snapshot --force-checkpoint
```

`--force-checkpoint` requests a full WAL checkpoint before copying; stricter but may fail while readers are active.

---

## 7. Operational Collections

Operational collections are append-only, versioned data stores. The lifecycle
is: register, configure, write, read, maintain, retire.

### Schema & Configuration

| Operation | Python SDK | CLI subcommand |
|-----------|-----------|----------------|
| Register | `admin.register_operational_collection(request)` | -- |
| Describe | `admin.describe_operational_collection(name)` | -- |
| Update filters | `admin.update_operational_collection_filters(name, json)` | `update-operational-filters --collection <name> --filter-fields-json <json>` |
| Update validation | `admin.update_operational_collection_validation(name, json)` | `update-operational-validation --collection <name> --validation-json <json>` |
| Update indexes | `admin.update_operational_collection_secondary_indexes(name, json)` | `update-operational-secondary-indexes --collection <name> --secondary-indexes-json <json>` |
| Disable | `admin.disable_operational_collection(name)` | `disable-operational --collection <name>` |

### Query & Trace

**trace** -- Inspect the mutation history, optionally narrowed to a single record key.

```python
report = admin.trace_operational_collection("audit_log", record_key="rec-77")
```
```sh
fathom-integrity trace-operational --collection audit_log --record-key rec-77
```

**read** -- Read current records with declared filter clauses.

```python
report = admin.read_operational_collection(
    OperationalReadRequest(collection_name="audit_log", filters=[...], limit=100))
```
```sh
fathom-integrity read-operational --collection audit_log \
  --filters-json '[{"field":"severity","op":"eq","value":"error"}]' --limit 100
```

### Repair & Rebuild

| Operation | Python SDK | CLI subcommand |
|-----------|-----------|----------------|
| Rebuild current state | `admin.rebuild_operational_current(name)` | `rebuild-operational-current --collection <name>` |
| Validate history | `admin.validate_operational_collection_history(name)` | `validate-operational-history --collection <name>` |
| Rebuild secondary indexes | `admin.rebuild_operational_secondary_indexes(name)` | `rebuild-operational-secondary-indexes --collection <name>` |

Omitting `--collection` from `rebuild-operational-current` rebuilds all collections.

### Retention, Compaction & Purge

**plan_operational_retention** -- Preview retention policy enforcement.

```python
plan = admin.plan_operational_retention(now_timestamp=1711670400, max_collections=5)
```
```sh
fathom-integrity plan-operational-retention --now 1711670400 --max-collections 5
```

**run_operational_retention** -- Execute retention policy. Supports `--dry-run`.

```python
report = admin.run_operational_retention(now_timestamp=1711670400, dry_run=True)
```
```sh
fathom-integrity run-operational-retention --now 1711670400 --dry-run
```

**compact** -- Merge append-only mutations into a single record per key. Supports `--dry-run`.

```python
report = admin.compact_operational_collection("audit_log", dry_run=False)
```
```sh
fathom-integrity compact-operational --collection audit_log --dry-run
```

**purge** -- Permanently delete mutations older than a timestamp.

```python
report = admin.purge_operational_collection("audit_log", before_timestamp=1711670400)
```
```sh
fathom-integrity purge-operational --collection audit_log --before 1711670400
```

---

## 8. Vector Regeneration

The `regenerate-vectors` CLI command and its bridge counterparts
(`RestoreVectorProfiles`, `RegenerateVectorEmbeddings`) handle bulk
re-embedding when a vector model changes or embeddings become stale.

For the full contract, configuration format, and generator policy flags, see
[docs/vector-regeneration.md](vector-regeneration.md).
