# fathomdb

Local datastore for persistent AI agents. Graph, vector, and full-text search
on SQLite.

fathomdb is canonical local storage for AI agent systems that need a durable
world model. It provides a graph backbone with logical identity and
supersession, full-text search (FTS5) over both document chunks and structured
node properties, vector search (sqlite-vec), an operational state store, and
provenance tracking with source attribution.

## Installation

```bash
npm install fathomdb
```

## Quick Start

```typescript
import { Engine, WriteRequestBuilder, newId, newRowId } from "fathomdb";

// Open a database. Opt into the Phase 12.5 read-time embedder so
// search() fires its vector branch on natural-language queries. The
// "builtin" shape requires a fathomdb build with the default-embedder
// Cargo feature; when that feature is off it silently falls back to
// text-only search.
const engine = Engine.open("agent.db", {
  embedder: "builtin",
  vectorDimension: 384,
});

// Write data
const builder = new WriteRequestBuilder("ingest");
const node = builder.addNode({
  rowId: newRowId(),
  logicalId: newId(),
  kind: "Document",
  properties: { title: "Meeting notes", status: "active" },
  sourceRef: "my-agent",
});
builder.addChunk({
  id: newId(),
  node,
  textContent: "Discussed Q1 budget and hiring plan.",
});
engine.write(builder.build());

// Query by kind
const rows = engine.nodes("Document").limit(10).execute();
console.log(rows.nodes[0].properties); // { title: "Meeting notes", ... }

// Unified search — returns SearchRows, not QueryRows. This is the
// recommended retrieval entry point.
const searchRows = engine.nodes("Document")
  .search("budget", 5)
  .execute();
for (const hit of searchRows.hits) {
  console.log(hit.node.logicalId, hit.score, hit.modality,
              hit.source, hit.matchMode, hit.snippet);
}

// Filter by property
const filtered = engine.nodes("Document")
  .filterJsonTextEq("$.status", "active")
  .execute();

// Close when done
engine.close();
```

## Key Features

- **Graph backbone**: nodes, edges, logical identity, supersession (upsert
  without mutation), runs/steps/actions for agent execution tracking
- **Unified `search(...)` retrieval** via SQLite FTS5 -- one call runs a
  strict-then-relaxed text pipeline plus an optional vector branch and
  returns ranked `SearchHit` rows over both document chunks and
  structured property projections. `textSearch(...)`,
  `vectorSearch(...)`, and `fallbackSearch(...)` remain available as
  advanced modality-specific overrides.
- **Read-time query embedder** (Phase 12.5): pass `{ embedder:
  "builtin", vectorDimension: 384 }` to `Engine.open(...)` to let
  `search()` fire its vector branch on natural-language queries. See
  the [querying guide](../../../docs/guides/querying.md#read-time-embedding)
  for the full configuration surface. The `"builtin"` embedder requires
  a fathomdb build with the `default-embedder` Cargo feature on
  `fathomdb-engine`; when the feature is off, the engine logs a warning
  and silently falls back to text-only search.
- **Vector search** via sqlite-vec
- **Immutable query builder**: fluent, chainable API with 14+ filter methods
- **Typed results**: all query/admin results are fully typed TypeScript interfaces
- **Progress callbacks**: optional feedback events for monitoring long operations
- **Admin operations**: integrity checks, projection rebuilds, source tracing,
  safe export, operational collection management
- **Provenance tracking**: source attribution on every write, trace/excise lineage

## API Overview

### Engine

```typescript
const engine = Engine.open("path.db", {
  provenanceMode: "warn",       // or "require"
  vectorDimension: 384,         // optional, for vector search
  telemetryLevel: "counters",   // or "statements", "profiling"
  embedder: "builtin",          // optional; "none" | "builtin"
});

engine.write(request);
engine.nodes("Kind").execute();
engine.telemetrySnapshot();
engine.admin.checkIntegrity();
engine.close();
```

### WriteRequestBuilder

```typescript
const builder = new WriteRequestBuilder("label");
const node = builder.addNode({ rowId, logicalId, kind, properties });
const chunk = builder.addChunk({ id, node, textContent });
const edge = builder.addEdge({ rowId, logicalId, kind, properties, source: node, target: "other-id" });
builder.addVecInsert({ chunk, embedding: [0.1, 0.2, ...] });
builder.retireNode(node);
const request = builder.build();
```

### Query

The unified `.search(...)` call returns a `SearchBuilder`, which exposes
filter methods plus `.withMatchAttribution()` and `.execute()`. The
terminal `execute()` returns `SearchRows` whose `hits` carry
`logicalId`, `score`, `modality`, `source`, `matchMode`, and `snippet`:

```typescript
const searchRows = engine.nodes("Goal")
  .search("budget", 10)
  .filterJsonTextEq("$.status", "published")
  .filterJsonIntegerGt("$.year", 2025)
  .withMatchAttribution()
  .execute();
for (const hit of searchRows.hits) {
  console.log(hit.node.logicalId, hit.score, hit.matchMode);
}
```

Graph traversal and expansion live on the classic `Query` builder (the
non-search path) and return `QueryRows`:

```typescript
engine.nodes("Meeting")
  .filterJsonTextEq("$.status", "active")
  .filterJsonIntegerGt("$.year", 2025)
  .traverse({ direction: "out", label: "OWNS", maxDepth: 2 })
  .expand({ slot: "related", direction: "out", label: "REFS", maxDepth: 1 })
  .limit(20)
  .execute();
```

### Unified search

```typescript
import { Engine } from "fathomdb";

const engine = Engine.open("/tmp/fathom.db");

// search() is the primary retrieval entry point — engine owns the
// strict-then-relaxed policy and returns SearchRows, not QueryRows.
const rows = engine.nodes("Goal").search("ship quarterly docs", 10).execute();
for (const hit of rows.hits) {
  console.log(hit.node.logicalId, hit.score, hit.modality, hit.source,
              hit.matchMode, hit.snippet);
}
console.log(rows.strictHitCount, rows.relaxedHitCount, rows.vectorHitCount);

// Recursive property FTS schema + opt-in match attribution.
engine.admin.registerFtsPropertySchemaWithEntries({
  kind: "KnowledgeItem",
  entries: [{ path: "$.payload", mode: "recursive" }],
});
const attributed = engine
  .nodes("KnowledgeItem")
  .search("quarterly docs", 10)
  .withMatchAttribution()
  .execute();
for (const hit of attributed.hits) {
  if (hit.attribution) {
    console.log(hit.node.logicalId, hit.attribution.matchedPaths);
  }
}

// Advanced overrides (pin the modality or supply both shapes verbatim):
//   engine.nodes("Goal").textSearch("ship quarterly docs", 10).execute()
//   engine.fallbackSearch("quarterly docs", "quarterly OR docs", 10).execute()
// See docs/guides/querying.md for when each is the right tool.
```

### AdminClient

```typescript
engine.admin.checkIntegrity();
engine.admin.checkSemantics();
engine.admin.rebuild("all");
engine.admin.traceSource("source:my-import");
engine.admin.exciseSource("source:bad-data");
engine.admin.safeExport("/path/to/backup.db");
engine.admin.restoreLogicalId("node:retired-id");
engine.admin.purgeLogicalId("node:old-id");

// FTS property schema management
engine.admin.registerFtsPropertySchema("Goal", ["$.name", "$.description"]);
engine.admin.describeFtsPropertySchema("Goal");
engine.admin.listFtsPropertySchemas();
engine.admin.removeFtsPropertySchema("Goal");
```

### Progress Callbacks

```typescript
engine.write(request, (event) => {
  console.log(`${event.phase} ${event.operationKind} ${event.elapsedMs}ms`);
});
```

## Errors

All errors extend `FathomError`:

- `DatabaseLockedError` - another process holds the database lock
- `CompileError` - query compilation failed
- `InvalidWriteError` - write request validation failed
- `WriterRejectedError` - writer thread rejected the transaction
- `SchemaError` - schema operation failed
- `SqliteError` - underlying SQLite error
- `IoError` - file system I/O failure
- `BridgeError` - native bridge internal error
- `CapabilityMissingError` - required capability not configured
- `BuilderValidationError` - write builder handle validation failed

## Requirements

- Node.js 20+
- The native binding (`.node` file) must be built from the Rust source:
  `cargo build -p fathomdb --features node`

## Development — refreshing the native binding

Prebuilt `.node` files under `prebuilds/` are not committed. For local
development the vitest runner will (in order):

1. Use `FATHOMDB_NATIVE_BINDING` when set (absolute path to a `.node`).
2. Use a prebuilt binary from the local `prebuilds/` directory.
3. Use the freshly built cdylib at
   `<repo-root>/target/{debug,release}/libfathomdb.{so,dylib}` or
   `fathomdb.dll`, copying it into `test/.native/fathomdb.node`.
4. As a last-resort fallback, use a prebuilt binary from the main
   worktree's `prebuilds/` directory (useful for linked worktrees).

Run `cargo build -p fathomdb --features node` from the repo root before
invoking `npm test` when iterating on Rust changes that affect the
JavaScript surface.

## License

Licensed under either of MIT or Apache-2.0 at your option.
