# Full-Text Search for Structured Nodes

This guide covers **property FTS projections** -- engine-managed full-text
indexing of JSON properties on structured node kinds. If your application has
node kinds that need to be searchable by keyword but don't have document
chunks, this is the feature to use.

For background on chunks and the standard chunk-based FTS path, see
[Data Model](../concepts/data-model.md) and
[Querying Data](./querying.md#full-text-search).

## When to Use Property FTS

Use property FTS projections when you have **structured nodes** -- nodes whose
searchable content lives in their JSON `properties`, not in chunks.

**Good fit:**

- Goals, tasks, contacts, knowledge objects, observations -- entities with
  `title`, `description`, `summary`, or similar text fields
- Nodes where the application writes structured JSON and wants keyword search
  without creating synthetic chunks
- Kinds where the searchable text is a small number of declared fields

**Not a fit (use chunks instead):**

- Document ingestion with large text bodies -- use chunks
- Content where you need per-fragment vector embeddings -- use chunks + vec
- Free-form text that doesn't map to fixed property paths

**Both together:** A node kind can have both chunks and property projections.
`text_search(...)` transparently searches both and returns a unified result.
It uses the same safe text-query subset documented in
[Text Query Syntax](./text-query-syntax.md): terms, quoted phrases,
implicit `AND`, uppercase `OR`, and uppercase `NOT`. Unsupported syntax
stays literal rather than becoming raw FTS5 control syntax.

## How It Works

1. **Register a schema** for each node kind that should have property FTS.
   The schema declares which JSON paths to extract and how to join them.
2. **Write nodes normally.** The engine extracts the declared paths at write
   time and maintains a derived FTS index row automatically.
3. **Search with `text_search(...)`.** The existing query operator transparently
   covers both chunk-backed and property-backed hits via a UNION. The search
   expression still follows the same safe subset described in
   [Text Query Syntax](./text-query-syntax.md). No new query API is needed.

Property FTS rows are **derived state** -- they are rebuilt from canonical
nodes and schemas. You never write them directly.

## Registering a Schema

Register a schema before writing nodes of that kind (or rebuild afterward):

=== "Python"

    ```python
    from fathomdb import Engine

    db = Engine.open("agent.db")

    db.admin.register_fts_property_schema(
        "Goal",
        ["$.name", "$.description", "$.rationale"],
        separator=" ",          # default; joins extracted values
    )
    ```

=== "TypeScript"

    ```typescript
    import { Engine } from "fathomdb";

    const engine = Engine.open("agent.db");

    engine.admin.registerFtsPropertySchema(
        "Goal",
        ["$.name", "$.description", "$.rationale"],
    );
    ```

### Path Syntax

Paths must use simple `$.`-prefixed dot-notation:

| Path | Meaning |
|---|---|
| `$.name` | Top-level `name` field |
| `$.payload.summary_text` | Nested field |

Array indexing (`$.tags[0]`), wildcards (`$.*`), and recursive descent
(`$..name`) are **not supported** and will be rejected at registration.

### Idempotent Upsert

Calling `register_fts_property_schema` again for the same kind overwrites the
previous schema (paths and separator). This does **not** rewrite existing FTS
rows -- call `admin.rebuild("fts")` to backfill.

## Writing Nodes

No changes to your write path. Write nodes as usual:

=== "Python"

    ```python
    from fathomdb import WriteRequestBuilder

    builder = WriteRequestBuilder("create-goals")
    builder.add_node(
        kind="Goal",
        properties={"name": "Ship v2", "description": "Launch the redesign"},
        source_ref="agent/planner",
    )
    db.write(builder.build())
    ```

=== "TypeScript"

    ```typescript
    const builder = new WriteRequestBuilder("create-goals");
    builder.addNode({
        rowId: newRowId(), logicalId: newId(),
        kind: "Goal",
        properties: { name: "Ship v2", description: "Launch the redesign" },
        sourceRef: "agent/planner",
    });
    engine.write(builder.build());
    ```

The engine sees that `Goal` has a registered schema, extracts `$.name` and
`$.description`, joins them with the separator, and inserts a property FTS row
in the same transaction as the node write.

On **upsert**, the old property FTS row is deleted and a new one is inserted.
On **retire**, the property FTS row is deleted. On **restore**, the property
FTS row is rebuilt.

## Searching

Use the same `text_search(...)` you already use for chunk-based FTS. The
supported query forms are the same safe subset documented in the querying
guide:

=== "Python"

    ```python
    results = db.nodes("Goal").text_search("redesign", limit=10).execute()
    for node in results.nodes:
        print(node.logical_id, node.properties["name"])
    ```

=== "TypeScript"

    ```typescript
    const results = engine.nodes("Goal").textSearch("redesign", 10).execute();
    for (const node of results.nodes) {
        console.log(node.logicalId, node.properties.name);
    }
    ```

The query compiler emits a UNION over the chunk FTS table and the property FTS
table. Your application does not need to know which source produced a given
hit. This also means a search on a kind that has both chunks and property
projections returns results from both.

## Normalization Rules

When extracting property values, the engine applies these rules:

| Value Type | Behavior |
|---|---|
| String | Included as-is |
| Number | Stringified (e.g. `42` -> `"42"`) |
| Boolean | Stringified (`true` -> `"true"`) |
| Null / missing | Skipped |
| Array of scalars | Each element extracted individually |
| Object | Skipped |
| Nested array | Skipped |
| Empty string | Preserved |

The separator applies between **all** extracted values, including individual
array elements. If no values remain after extraction, no FTS row is created.

## Managing Schemas

### Describe

=== "Python"

    ```python
    record = db.admin.describe_fts_property_schema("Goal")
    if record:
        print(record.kind, record.property_paths, record.separator)
    ```

=== "TypeScript"

    ```typescript
    const record = engine.admin.describeFtsPropertySchema("Goal");
    if (record) {
        console.log(record.kind, record.propertyPaths, record.separator);
    }
    ```

!!! note "Reading recursive schemas"

    `property_paths` / `propertyPaths` is a **flat display list** — it
    lists every registered path once, but does not tell you which paths
    were registered as recursive. To read the mode-accurate per-entry
    view of a registered schema, read `record.entries` (the same field
    name in both Python and TypeScript). Use `record.exclude_paths`
    (Python) / `record.excludePaths` (TypeScript) for the list of
    subtrees excluded from the recursive walk.

### List All

=== "Python"

    ```python
    for schema in db.admin.list_fts_property_schemas():
        print(schema.kind, schema.property_paths)
    ```

### Remove

Removing a schema deletes the schema row but does **not** delete existing
derived FTS rows. Run a rebuild afterward to clean them up:

=== "Python"

    ```python
    db.admin.remove_fts_property_schema("Goal")
    db.admin.rebuild("fts")    # clean up stale derived rows
    ```

=== "TypeScript"

    ```typescript
    engine.admin.removeFtsPropertySchema("Goal");
    engine.admin.rebuild("fts");
    ```

## Rebuild and Recovery

Property FTS rows are derived state. They can always be rebuilt from:

- Active canonical nodes
- Registered schemas in `fts_property_schemas`

**Full rebuild** (`admin.rebuild("fts")`) deletes all FTS rows and repopulates
from scratch -- both chunk-based and property-based.

**Missing-projection rebuild** (`admin.rebuild_missing()`) fills gaps without
touching existing rows.

After a `safe_export` and import, the canonical `fts_property_schemas` table is
preserved. Run `admin.rebuild("fts")` on the imported database to restore
property FTS rows from canonical state.

## Diagnostics

`admin.check_integrity()` reports `missing_property_fts_rows` -- active nodes
that should have a property FTS row but don't.

`admin.check_semantics()` reports property FTS drift:

| Field | Meaning |
|---|---|
| `stale_property_fts_rows` | Rows for superseded or missing nodes |
| `orphaned_property_fts_rows` | Rows for kinds with no registered schema |
| `mismatched_kind_property_fts_rows` | Rows whose kind differs from the active node |
| `duplicate_property_fts_rows` | Logical IDs with more than one property FTS row |
| `drifted_property_fts_rows` | Rows whose text doesn't match current extraction |

All of these should be zero in a healthy database. If any are non-zero, run
`admin.rebuild("fts")` to repair.

## Limitations (v1)

- **Path syntax**: Simple dot-notation only. No array indexing, wildcards, or
  recursive descent.
- **No per-field weighting**: All extracted values contribute equally to the
  FTS score.
- **No field-scoped queries**: You cannot search only the `$.name` field. The
  extracted values are concatenated into a single FTS document.
- **No highlighting or snippets**: The engine returns matched nodes, not match
  positions within the extracted text.
- **Registration does not backfill**: Registering a schema does not rewrite
  FTS rows for existing nodes. Call `admin.rebuild("fts")` after registration
  if nodes of that kind already exist.
