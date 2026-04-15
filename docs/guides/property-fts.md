# Full-Text Search for Structured Nodes

This guide covers **property FTS projections** -- engine-managed full-text
indexing of JSON properties on structured node kinds. If your application has
node kinds that need to be searchable by keyword but don't have document
chunks, this is the feature to use.

For background on chunks and the standard chunk-based FTS path, see
[Data Model](../concepts/data-model.md) and
[Querying Data](./querying.md#unified-search-recommended).

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
The unified [`search(...)`](./querying.md#unified-search-recommended)
entry point — and the advanced `text_search(...)` override — transparently
searches both and returns unified
[`SearchRows`](../reference/query.md): each `SearchHit` carries a
`source` field indicating whether it came from a chunk or a property-FTS
row. The strict query grammar is documented in
[Text Query Syntax](./text-query-syntax.md).

This is the mechanism that lets `search()` match tokens inside
structured JSON properties. Register a property FTS schema on a kind,
and text inside the declared paths becomes first-class retrievable
content — without chunks, without reshaping your write path, and
composable with the `filter_json_*` family on the resulting
`SearchBuilder`. See
[Querying Data](./querying.md#unified-search-recommended) for the
`filter_json_*` vs property FTS contrast.

## How It Works

1. **Register a schema** for each node kind that should have property FTS.
   The schema declares which JSON paths to extract and how each one should
   be walked (scalar vs recursive).
2. **Write nodes normally.** The engine extracts the declared paths at write
   time and maintains a derived FTS row plus (for recursive paths) a
   position-map row per leaf, all in the same transaction as the node write.
3. **Search with `search(...)`.** The unified retrieval pipeline
   transparently covers both chunk-backed and property-backed hits. No
   separate query API is needed. (The advanced `text_search(...)` and
   `fallback_search(...)` overrides read the same projections.)

Property FTS rows — both the blob and the position map — are **derived
state**. They are rebuilt from canonical nodes and schemas. You never
write them directly.

## Scalar vs Recursive Paths

Every registered path carries a **mode**:

| Mode | Behavior |
|---|---|
| `scalar` | Resolve the path once and append the value (or, for an array of scalars, each element). This matches the legacy pre-Phase-4 behavior and is the default. |
| `recursive` | Walk the subtree rooted at the path and emit every scalar leaf as an extracted value. Each leaf also produces one position-map entry, making it eligible for per-hit match attribution. |

Scalar mode is the right choice when the searchable text lives in a small
set of fixed top-level fields (`$.title`, `$.description`, `$.rationale`).
Recursive mode is the right choice when the searchable text is spread
through an opaque structured blob (`$.payload`, `$.content_tree`) and you
don't want to enumerate every leaf by hand.

Recursive mode also unlocks two features that scalar mode doesn't:

- **Match attribution** — `with_match_attribution()` on a text-search
  builder populates `hit.attribution.matched_paths` with the JSON paths
  that actually produced the FTS match.
- **Subtree exclusions** — `exclude_paths` lets you prune subtrees from the
  recursive walk (e.g. secret payloads, redundant ID fields) without
  rewriting the upstream schema.

## Registering a Schema

There are two registration APIs. Use whichever matches the shape of your
schema:

- `register_fts_property_schema(kind, paths, separator)` — the
  convenience shim. All entries are registered in **scalar** mode. Use
  this when you just want to point at a few top-level fields.
- `register_fts_property_schema_with_entries(kind, entries, separator,
  exclude_paths)` — the full-shape API. Each entry is an
  `FtsPropertyPathSpec(path, mode)`, so you can mix scalar and recursive
  paths and supply `exclude_paths`. Use this any time any path needs
  recursive-mode indexing.

### Scalar-only (convenience API)

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

### Mixed scalar + recursive (full-shape API)

=== "Python"

    ```python
    from fathomdb import (
        Engine,
        FtsPropertyPathMode,
        FtsPropertyPathSpec,
    )

    db = Engine.open("agent.db")

    db.admin.register_fts_property_schema_with_entries(
        "KnowledgeItem",
        entries=[
            FtsPropertyPathSpec(path="$.title", mode=FtsPropertyPathMode.SCALAR),
            FtsPropertyPathSpec(path="$.payload", mode=FtsPropertyPathMode.RECURSIVE),
        ],
        separator=" ",
        exclude_paths=["$.payload.secret"],
    )
    ```

=== "TypeScript"

    ```typescript
    engine.admin.registerFtsPropertySchemaWithEntries({
        kind: "KnowledgeItem",
        entries: [
            { path: "$.title", mode: "scalar" },
            { path: "$.payload", mode: "recursive" },
        ],
        separator: " ",
        excludePaths: ["$.payload.secret"],
    });
    ```

### Path Syntax

Paths must use simple `$.`-prefixed dot-notation:

| Path | Meaning |
|---|---|
| `$.name` | Top-level `name` field |
| `$.payload.summary_text` | Nested field |

Array indexing (`$.tags[0]`), wildcards (`$.*`), and recursive descent
(`$..name`) are **not supported as path syntax** and will be rejected at
registration. "Recursive mode" is a separate, declared behavior on an
otherwise-simple path — it tells the engine to walk scalar leaves under
that path, not to reinterpret the path itself.

### Idempotent Upsert

Calling any register API again for the same kind overwrites the previous
schema in place.

#### Async shadow-build path (default in 0.4.1+)

`register_fts_property_schema_async` registers the schema and returns
immediately (typically under 500 ms). The FTS rebuild runs in the background
via `RebuildActor`. During the rebuild:

- Search **reads from the previous schema** until the rebuild completes.
  There is no period where search returns empty results or degraded results
  due to a schema transition.
- Once the rebuild reaches `COMPLETE`, the new schema is live and all
  subsequent searches use it.

Poll `get_rebuild_progress(kind)` to observe the rebuild state machine:
`PENDING → BUILDING → SWAPPING → COMPLETE` (or `FAILED` on error).

```python
import time

db.admin.register_fts_property_schema_async(
    "KnowledgeItem",
    ["$.title", "$.summary"],
    separator=" ",
)

# Poll until complete
while True:
    progress = db.admin.get_rebuild_progress("KnowledgeItem")
    if progress is None or progress.state == "COMPLETE":
        break
    if progress.state == "FAILED":
        raise RuntimeError(f"Rebuild failed: {progress.error_message}")
    time.sleep(0.5)
```

#### Eager mode (synchronous, maintenance-window use)

`register_fts_property_schema` and `register_fts_property_schema_with_entries`
use `RebuildMode::Eager`: the rebuild runs synchronously inside the same
transaction as the schema upsert. Schema changes are immediate on return —
there is no lazy mark-stale path, and there is no versioned co-existence of
old and new schemas. If the upsert commits, the derived state is consistent
with the new schema. Use this mode when you need synchronous visibility and
can afford to block the caller for the duration of the rebuild.

#### Crash recovery

If the engine restarts during an async rebuild, the in-progress state is
marked `FAILED` on the next engine open. Call `register_fts_property_schema_async`
again to retry the rebuild.

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

Use the unified [`search(...)`](./querying.md#unified-search-recommended)
entry point you already use for chunk-based FTS. It returns
[`SearchRows`](../reference/query.md) — see
[Querying Data](./querying.md#unified-search-recommended) for the full
contract. (The advanced `text_search(...)` override works identically if
you need to pin the modality.)

=== "Python"

    ```python
    rows = db.nodes("Goal").search("redesign", 10).execute()
    for hit in rows.hits:
        print(hit.node.logical_id, hit.score, hit.source.value, hit.snippet)
    ```

=== "TypeScript"

    ```typescript
    const rows = engine.nodes("Goal").search("redesign", 10).execute();
    for (const hit of rows.hits) {
        console.log(hit.node.logicalId, hit.score, hit.source, hit.snippet);
    }
    ```

The query compiler emits a unified search plan over both the chunk FTS
table and the property FTS table. Your application does not need to know
which source produced a given hit — but if it wants to, `hit.source` is
`"chunk"` or `"property"` accordingly. A search against a kind that has
both chunks and property projections returns results from both.

## Match Attribution (opt-in)

For kinds with recursive-mode property paths, the engine maintains a
sidecar position map (see [Position Map](#position-map) below). Callers
can opt in to **per-hit match attribution**, which tells them which
registered path(s) actually produced the FTS match for each hit. It is
available on every search surface — unified `search()`, the advanced
`text_search()` override, and `fallback_search()` — via the same
`with_match_attribution()` builder method. See
[Querying Data](./querying.md#unified-search-recommended) for how
`search()` carries the attribution through to `SearchHit.attribution`.

=== "Python"

    ```python
    rows = (
        db.nodes("KnowledgeItem")
        .search("quarterly docs", 10)
        .with_match_attribution()
        .execute()
    )
    for hit in rows.hits:
        if hit.attribution:
            print(hit.node.logical_id, hit.attribution.matched_paths)
    ```

=== "TypeScript"

    ```typescript
    const rows = engine
        .nodes("KnowledgeItem")
        .search("quarterly docs", 10)
        .withMatchAttribution()
        .execute();
    for (const hit of rows.hits) {
        if (hit.attribution) {
            console.log(hit.node.logicalId, hit.attribution.matchedPaths);
        }
    }
    ```

Attribution is **opt-in**. Hits returned from a call without
`with_match_attribution()` have `attribution == None` / `undefined`, and
the default query path pays no extra cost — no position-map lookup, no
extra joins. A scalar-only schema can still be queried with
`with_match_attribution()`, but the attribution will be empty for hits
that came from scalar entries, since scalar extraction doesn't record
per-leaf positions.

Property FTS composes cleanly with the Phase 12.5 read-time embedder.
When `Engine.open(...)` is given `embedder="builtin"` (or a caller-
supplied `EmbedderChoice::InProcess`), `search()` still runs the
property-FTS branch exactly as described above — property hits keep
flowing through `SearchHitSource.PROPERTY`, and
`with_match_attribution()` still reports which registered JSON path
matched — while the vector branch fires in parallel against the
engine-embedded query text. See
[Read-time embedding](./querying.md#read-time-embedding) in the
querying guide for the embedder configuration surface and its
degradation semantics.

## Position Map

Recursive extraction is backed by a sidecar table,
`fts_node_property_positions`. One row is inserted per scalar leaf
emitted by the recursive walk, with a
`UNIQUE(node_logical_id, kind, start_offset)` constraint: every emitted
leaf has a distinct offset into the concatenated FTS blob for that node.
The position map is **derived state** — it is rebuilt from canonical
nodes + schemas whenever the FTS blob is rebuilt (node upsert, recursive
schema registration, explicit `admin.rebuild("fts")`).

Scalar-only entries contribute to the FTS blob but do not populate the
position map. They remain searchable and returnable as `SearchHit`s —
you just can't attribute their matches to specific paths.

## Recursive Extraction Guardrails

Recursive walks are bounded by two fixed limits plus an optional
per-schema exclusion list:

| Guardrail | Value | What it does |
|---|---|---|
| `MAX_RECURSIVE_DEPTH` | `8` | The recursive walk stops descending below eight levels of nesting. Leaves at depth > 8 are not emitted. |
| `MAX_EXTRACTED_BYTES` | `65 536` | The walk stops emitting leaves once the concatenated extracted text for a single node exceeds 64 KiB. |
| `exclude_paths` | per schema | Any leaf whose JSON path matches an entry in `exclude_paths` is skipped. Each entry must start with `$.`. |

When any guardrail fires, the row is **still indexed** — we never skip a
node outright. We just stop extracting past the guardrail. An agent that
wants to know the guardrail fired can inspect `check_semantics` /
`check_integrity` reports, which flag property-FTS drift.

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

`describe_fts_property_schema(kind)` returns the schema record for a
single kind (or `None` / `null` if it is not registered).
`list_fts_property_schemas()` returns every registered schema. Both
return an `FtsPropertySchemaRecord` with the following fields:

| Field | Meaning |
|---|---|
| `kind` | Node kind the schema applies to. |
| `property_paths` / `propertyPaths` | Flat display list of registered paths. Does not carry mode information. |
| `entries` | Per-entry `(path, mode)` list. Read this for mode-accurate round-tripping. |
| `exclude_paths` / `excludePaths` | Subtree exclusions for recursive walks. Empty for scalar-only schemas. |
| `separator` | String inserted between extracted values. |
| `format_version` | Schema wire-format version. |

!!! tip "Prefer `entries` for new code"

    `property_paths` is a legacy flat list kept for backwards
    compatibility. New code should read `entries` (Python) or `entries`
    (TypeScript — same field name) so that recursive mode and
    scalar mode round-trip faithfully.

=== "Python"

    ```python
    record = db.admin.describe_fts_property_schema("KnowledgeItem")
    if record:
        for entry in record.entries:
            print(entry.path, entry.mode.value)
        print("excluded:", record.exclude_paths)
    ```

=== "TypeScript"

    ```typescript
    const record = engine.admin.describeFtsPropertySchema("KnowledgeItem");
    if (record) {
        for (const entry of record.entries) {
            console.log(entry.path, entry.mode);
        }
        console.log("excluded:", record.excludePaths);
    }
    ```

### List All

=== "Python"

    ```python
    for schema in db.admin.list_fts_property_schemas():
        print(schema.kind, [(e.path, e.mode.value) for e in schema.entries])
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

- **Path syntax**: Simple dot-notation only. No array indexing, wildcards,
  or JSONPath recursive-descent syntax — "recursive mode" is a declared
  behavior on a simple path, not a path-syntax feature.
- **No per-field weighting**: All extracted values contribute equally to
  the FTS score.
- **No field-scoped queries**: You cannot search only the `$.name` field.
  The extracted values are concatenated into a single FTS document;
  match attribution tells you *after the fact* which path matched.
- **Adaptive relaxation is engine-owned**: you cannot tune the relaxed
  branch per call. Use
  [`Engine.fallback_search`](./querying.md#advanced-explicit-two-shape-fallback-search)
  if you need to supply a strict and relaxed shape verbatim.
- **Recursive rebuild is synchronous**: registering a schema with a new
  recursive path rebuilds derived state for every active node of that
  kind in the upsert transaction. For very large kinds this is the
  dominant cost of introducing recursive mode.
