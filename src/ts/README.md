# fathomdb

Node.js SDK for **FathomDB** — a local-first, embedded retrieval engine for application and agent
workloads, built on SQLite.

A FathomDB database is a single file on your disk. There is no server, no daemon and no network
call: you `open` a path and you have hybrid full-text + vector search over your own data, in your
own process.

## Status: pre-1.0, beta

The 0.8.x line is under active development. The public surface can change between minor releases,
and scale/stability guarantees are staged for 0.9.x and later.

## Install

```bash
npm install fathomdb
```

The package is a thin TypeScript wrapper (ESM, with type declarations) over a native
[napi-rs](https://napi.rs) binding to the FathomDB Rust engine. The compiled binary is **not** in
this package: each host triple ships as its own package (for example
`fathomdb-linux-x64-gnu` or `fathomdb-linux-arm64-gnu`), declared as an optional dependency and
resolved automatically on a matching host. You install `fathomdb` and nothing else. The next
versioned release targets Linux x64 and Linux AArch64 glibc; macOS, Windows, and Linux musl remain
without an installable native package.

Built and tested on Node.js 25.9.0.

## Quick start

```ts
import { Engine } from "fathomdb";

const engine = await Engine.open("./app.sqlite");

await engine.write([
  {
    kind: "doc",
    body: "the quick brown fox",
    // Provenance is MANDATORY on every canonical row: sourceId is the axis
    // eraseSource() erases on, so a row without one can never be erased.
    sourceId: "import-2026-07",
  },
]);

const result = await engine.search("brown fox");
console.log(result.results);

await engine.close();
```

Every write field accepts both the camelCase and the snake_case spelling.

## What is on the surface

- **Engine lifecycle** — `Engine.open`, `engine.close`, `engine.counters()`, plus an open report and
  an event-subscriber seam.
- **Writes** — `engine.write(batch)` of canonical nodes (`{ kind, body, sourceId, logicalId?,
  state?, reason?, validFrom?, validUntil? }`) and edges (`{ edge: { kind, from, to, sourceId } }`).
- **Search** — `engine.search(query, ...)` fuses FTS5 lexical retrieval with dense vector retrieval,
  with optional cross-encoder reranking, JSON filters, and an explain sidecar. A `softFallback`
  field tells you when a branch could not contribute rather than silently degrading.
- **Reads** — the `read` namespace: `read.get`, `read.getMany`, `read.list`, `read.collection`,
  `read.crossedBoundarySince`, each taking a `ReadView` that selects existence state and world-time
  validity.
- **Graph** — `graph.neighbors` and `graph.searchExpand` for BFS expansion from search hits.
- **Record lifecycle** — `engine.transition`, `engine.purge`, `engine.eraseSource`.
- **Projections** — `engine.configureProjections`, declaring which fields are filterable,
  searchable or rankable; the engine owns every derived index.
- **Typed errors** — a leaf-class hierarchy under `FathomDbError`, so you can branch on
  `InvalidFilterError` or `IllegalTransitionError` instead of matching on message strings.

## Embeddings

Vector search needs vectors. `Engine.open(path, { useDefaultEmbedder: true })` opts into the
engine's pinned default embedder; on first use the weights are downloaded and cached under
`~/.cache/fathomdb/embedders/`. The default is `false` — a plain `open` performs no network access
at all, and vector writes then fail with `EmbedderNotConfiguredError` rather than quietly doing
nothing.

## Other SDKs

The same engine is available as a Rust crate ([`fathomdb`](https://crates.io/crates/fathomdb)) and a
Python package (`pip install fathomdb`).

## License

MIT. See the `LICENSE` file shipped in this package.

Source, issues and full documentation: <https://github.com/coreyt/fathomdb>
