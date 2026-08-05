# Getting Started

> **Published release.** **v0.8.21 is published** to crates.io / PyPI /
> npm. Native Python and npm artifacts cover Linux x86_64/glibc and Linux
> AArch64/glibc, and npm installs use its `next` dist-tag. FathomDB is pre-1.0 and
> the surface is **beta**. See the
> [CHANGELOG](https://github.com/coreyt/fathomdb/blob/main/CHANGELOG.md)
> for what changed since 0.8.9.

## Where to go

- [Quickstart](quickstart.md) — install, open, write, search,
  counters, close, exit. ~5 minutes.
- [Install — Python](../install/python.md)
- [Install — TypeScript / Node.js](../install/typescript.md)
- [Install — Rust](../install/rust.md)

## What ships in 0.8.21

The governed SDK surface (identical names in Python, TypeScript and
Rust, in each language's idiomatic spelling — pinned by
`src/conformance/governed-surface-allowlist.json`):

- **Core** — `Engine.open`, `write`, `search`, `close`,
  `admin.configure`.
- **Retrieval** — `search` (hybrid FTS5 + vector, RRF-fused, optional
  cross-encoder rerank and graph arm), `search_text_only`, `rerank`,
  `embed`.
- **Reads** — `read.get`, `read.get_many`, `read.list`,
  `read.collection`, `read.mutations`, `read.crossed_boundary_since`,
  `read.projections`.
- **Graph** — `graph.neighbors`, `graph.search_expand`.
- **Lifecycle / erasure** — `transition`, `purge`, `erase_source`.
- **Projections** — `configure_projections` (declarative registry) +
  `read.projections`.
- **BYO-LLM** — `ingest_with_extractor`, `consolidate_with_provider`.
- Engine-attached instrumentation: `open_report`, `drain`, `counters`,
  `set_profiling`, `set_slow_threshold_ms`, host-logger / subscriber
  attach.
- Operator CLI: `fathomdb doctor` (integrity, safe-export,
  verify-embedder, trace, dumps, `orphan-provenance`) and
  `fathomdb recover --accept-data-loss` (truncate-wal, rebuild-vec0,
  rebuild-projections, excise-source, excise-record).
- Local-first storage on SQLite (FTS5 + `sqlite-vec`), on-disk schema
  version **25**.
- Two-axis versioning: workspace lockstep across the
  runtime/binding/CLI crates and the independently versioned
  `fathomdb-embedder-api` trait crate.

## Breaking since 0.8.9

- `SearchHit.id` is a typed `IdSpace` (`{space, value}`), no longer an
  integer row cursor. See [structured search hits](../guides/structured-search-hits.md).
- `source_id` is **mandatory** on every canonical node/edge write.
- Search results are ordered by **RRF fusion**, not the old
  union-dedup.
- An unsatisfiable `valid_from >= valid_until` window is refused with
  `WriteValidationError` (message-less), not `InvalidArgumentError`.
- A projection spec carrying `fts`/`vector` without the `searchable`
  role is refused.

The [CHANGELOG](https://github.com/coreyt/fathomdb/blob/main/CHANGELOG.md)
is the authoritative list.

## Use the right SDK

Prefer **Python** for production pilots. The TypeScript SDK covers the
same governed surface and error taxonomy but is the less heavily
exercised binding. Rust users consume the `fathomdb` facade crate or
the `fathomdb-cli` operator binary.

## Known gaps

- **Performance gates AC-012, AC-013, AC-019, AC-020 remain open** —
  see [compatibility § performance posture](../compatibility/index.md).
- **Custom Python / TypeScript embedder implementations** are not
  exposed; the binding choice is the built-in default embedder or none.
- **No 0.5.x compatibility shims** and no in-place upgrade from a
  pre-0.8.20 database whose edges must survive — see
  [compatibility](../compatibility/index.md).
