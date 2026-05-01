# Cross-Language SDK Consistency Tests

Proves that the Python and TypeScript SDKs produce identical behavior when
operating on the same data. Both drivers read the same `scenarios.json`,
execute the same writes/queries/admin operations, and emit normalized JSON
manifests. The orchestrator diffs the manifests to verify parity.

## Design

The harness has three parts:

1. **`scenarios.json`** -- Declarative test scenarios: writes, queries, admin
   operations, and expected results. Both drivers read this file.
2. **Drivers** (`python/driver.py`, `typescript/driver.ts`) -- Each driver
   opens a FathomDB database, executes the scenarios through its SDK, and
   prints a sorted JSON manifest to stdout.
3. **`orchestrate.sh`** -- Runs both drivers, then cross-reads (TypeScript
   reads the Python-written DB and vice versa), and diffs all four manifests.

### Execution Flow

```
orchestrate.sh
  |
  |-- Phase 1: Python  writes to py.db,  reads back -> py-wrote.json
  |-- Phase 2: TypeScript writes to ts.db, reads back -> ts-wrote.json
  |-- Phase 3: TypeScript reads py.db                 -> ts-read-py.json
  |-- Phase 4: Python reads ts.db                     -> py-read-ts.json
  |
  |-- Compare: py-wrote.json == ts-wrote.json    (same input -> same state)
  |-- Compare: py-wrote.json == ts-read-py.json  (cross-read parity)
  |-- Compare: ts-wrote.json == py-read-ts.json  (cross-read parity)
```

### Driver Modes

Each driver accepts `--db <path>` and `--mode {write,read}`:

- **write**: Runs `setup_admin` (global pre-write operations like schema
  registration), then executes all scenario writes, then runs all queries
  and admin operations. Emits the full manifest.
- **read**: Skips writes and setup (assumes the DB was already written by
  the other driver). Runs queries and admin operations only. Emits the
  manifest for comparison.

## Scenario Format

`scenarios.json` has two top-level keys:

```jsonc
{
  "scenarios": [ ... ],      // Array of test scenarios
  "setup_admin": [ ... ]     // Optional: admin ops to run before any writes
}
```

### `setup_admin`

An array of admin operation objects that run once before any scenario writes
in `write` mode. Use this for operations that must precede writes, such as
registering FTS property schemas so that write-time projections are created.

```json
{
  "setup_admin": [
    {
      "type": "register_fts_property_schema",
      "kind": "Goal",
      "property_paths": ["$.name", "$.description"],
      "separator": " "
    }
  ]
}
```

### Scenario Object

Each scenario has:

| Field | Type | Description |
|---|---|---|
| `name` | string | Unique scenario identifier (used as manifest key) |
| `writes` | array | Write requests to execute (in order) |
| `queries` | array | Queries to execute after writes |
| `admin` | array | Admin operations to execute after writes |

### Write Definitions

Each write object maps to a `WriteRequest`:

```json
{
  "label": "my-write",
  "nodes": [{ "row_id": "...", "logical_id": "...", "kind": "...", "properties": {...}, "source_ref": "...", "upsert": true }],
  "node_retires": [{ "logical_id": "...", "source_ref": "..." }],
  "edges": [{ "row_id": "...", "logical_id": "...", "source_logical_id": "...", "target_logical_id": "...", "kind": "...", "properties": {...} }],
  "chunks": [{ "id": "...", "node_logical_id": "...", "text_content": "..." }],
  "runs": [...], "steps": [...], "actions": [...]
}
```

### Query Types

| Type | Required Fields | Description |
|---|---|---|
| `filter_logical_id` | `kind`, `logical_id` | Filter by exact logical ID |
| `text_search` | `kind`, `query`, `limit` | Adaptive text search (strict then derived-relaxed fallback) |
| `fallback_search` | `strict_query`, `relaxed_query`, `limit` | Explicit two-shape search (no engine rewriting) |
| `filter_content_ref_not_null` | `kind` | Filter nodes with content_ref set |
| `traverse` | `kind`, `start_logical_id`, `direction`, `label`, `max_depth` | Graph traversal |

### Adaptive-Search Assertion Shape

`text_search` and `fallback_search` queries emit a rich structured result that
validates the full `SearchRows`/`SearchHit` wire shape, not just the hit count.
Both drivers report the same normalized `actual` object per query:

```jsonc
{
  "type": "text_search",
  "name": "strict_hit_property",
  "actual": {
    "hit_count": 1,
    "strict_hit_count": 1,
    "relaxed_hit_count": 0,
    "fallback_used": false,
    "was_degraded": false,
    "hits": [
      {
        "logical_id": "goal:xlang-strict-budget",
        "kind": "Goal",
        "source": "property",            // "chunk" | "property" | "vector"
        "match_mode": "strict",          // "strict" | "relaxed"
        "snippet_non_empty": true,
        "written_at_recent": true,       // within last 300 seconds
        "projection_row_id_present": true,
        "attribution_matched_paths": ["$.payload.body"]  // only when with_match_attribution=true
      }
    ]
  },
  "pass": true,
  "failures": []
}
```

Non-deterministic fields (`score`, raw `written_at`, raw `projection_row_id`)
are replaced with deterministic booleans so the Python and TypeScript drivers
can diff their manifests byte for byte even though each writes its database
independently.

Each search query may declare any subset of the following optional expected
fields. Missing fields are not evaluated; declared fields must match exactly
for `pass` to be `true`.

| Field | Type | Semantics |
|---|---|---|
| `expect_hit_logical_ids` | `string[]` | Ordered list of hit logical IDs |
| `expect_hit_sources` | `("chunk"\|"property"\|"vector")[]` | Parallel to logical IDs |
| `expect_match_modes` | `("strict"\|"relaxed")[]` | Parallel to logical IDs |
| `expect_snippets_non_empty` | `boolean` | Every hit has a non-empty snippet |
| `expect_written_at_seconds_recent` | `boolean` | Every hit's `written_at` is within 300s of `now` |
| `expect_projection_row_ids_unique` | `boolean` | Every hit has a populated `projection_row_id` |
| `expect_strict_hit_count` | `number` | Exact `strict_hit_count` value |
| `expect_strict_hit_count_min` | `number` | Minimum `strict_hit_count` |
| `expect_relaxed_hit_count` | `number` | Exact `relaxed_hit_count` value |
| `expect_relaxed_hit_count_min` | `number` | Minimum `relaxed_hit_count` |
| `expect_fallback_used` | `boolean` | Matches `rows.fallback_used` |
| `expect_was_degraded` | `boolean` | Matches `rows.was_degraded` |
| `expect_matched_paths` | `[{hit_index, paths}]` | Per-hit expected attribution paths (order-insensitive) |
| `with_match_attribution` | `boolean` | Call `.with_match_attribution()` on the builder |
| `repeat_runs` | `number` | Run the same query N times (for determinism checks) |
| `expect_deterministic_across_runs` | `boolean` | All N repeated runs must produce byte-identical `actual` |
| `expect_min_count` | `number` | Back-compat: minimum `hit_count` |

`fallback_search` queries additionally require `strict_query`, `relaxed_query`
(may be `null` for strict-only), `limit`, and an optional `kind_filter` string
that is applied via `.filter_kind_eq(...)` on the builder.

### Admin Types

Admin operations can be a bare string (e.g. `"check_integrity"`) or an
object with a `type` field and parameters:

| Type | Parameters | Description |
|---|---|---|
| `check_integrity` | -- | Physical + FK + FTS consistency |
| `check_semantics` | -- | Orphaned chunks, dangling edges, stale projections |
| `trace_source` | `source_ref` | Trace objects written by a source |
| `register_fts_property_schema` | `kind`, `property_paths`, `separator` | Register/update scalar FTS projection schema |
| `register_fts_property_schema_with_entries` | `kind`, `entries`, `separator`, `exclude_paths` | Register schema with per-path modes (supports `"recursive"` for match attribution) |
| `describe_fts_property_schema` | `kind` | Describe a single schema |
| `list_fts_property_schemas` | -- | List all registered schemas |

## Adding a New Scenario

1. Add a new object to the `scenarios` array in `scenarios.json`.
2. If the scenario needs pre-write setup (e.g. schema registration), add
   entries to `setup_admin`.
3. If the scenario uses a new query type, add a handler to `executeQuery`
   in both `python/driver.py` and `typescript/driver.ts`.
4. If the scenario uses a new admin type, add a handler to `executeAdmin`
   in both drivers.
5. Both handlers must return the same normalized JSON structure.
6. Run `./orchestrate.sh` to verify parity.

Use unique needle strings in text content (e.g. `"propertyftsneedle"`) so
full-text searches don't accidentally match content from other scenarios.

## Running

### Prerequisites

```bash
# Python SDK
pip install -e python/ --no-build-isolation

# TypeScript SDK (Node.js native binding)
cargo build -p fathomdb --features node
cd typescript && npm install
cd tests/cross-language/typescript && npm install
```

### Execute

```bash
./tests/cross-language/orchestrate.sh
```

### Run a Single Driver

```bash
# Python only
python tests/cross-language/python/driver.py --db /tmp/test.db --mode write

# TypeScript only (needs FATHOMDB_NATIVE_BINDING set)
vite-node tests/cross-language/typescript/driver.ts -- --db /tmp/test.db --mode write
```
