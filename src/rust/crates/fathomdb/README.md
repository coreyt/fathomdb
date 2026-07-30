# fathomdb

The Rust facade crate for **FathomDB** — a local-first, embedded retrieval engine for application
and agent workloads, built on SQLite.

If you are writing a Rust application against FathomDB, this is the crate to depend on. It is a thin
re-export of the public surface of [`fathomdb-engine`](https://crates.io/crates/fathomdb-engine), so
you get one dependency and one import path instead of the workspace's internal crate split.

## Status: pre-1.0, beta

The 0.8.x line is under active development. The public surface can change between minor releases,
and scale/stability guarantees are staged for 0.9.x and later. Do not read the release engineering
maturity of the 0.8.x publish pipeline as an API stability promise.

## Install

```bash
cargo add fathomdb
```

## Example

```rust
use fathomdb::{Engine, InitialState, PreparedWrite, SourceId};

let opened = Engine::open("./app.sqlite").expect("open");
let engine = opened.engine;

engine
    .write(&[PreparedWrite::Node {
        kind: "doc".to_string(),
        body: "the quick brown fox".to_string(),
        // Provenance is MANDATORY on every canonical row: `source_id` is the
        // axis `erase_source` erases on, so a row without one is unerasable.
        source_id: SourceId::new("import-2026-07").expect("valid source id"),
        logical_id: None,
        state: InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }])
    .expect("write");

let result = engine.search("brown fox").expect("search");
println!("{} hit(s)", result.results.len());

engine.close().expect("close");
```

## What the default surface gives you

With default features the crate exposes the *governed application surface*:

- open / close / drain, plus engine counters and a subscriber seam
- `write` of canonical nodes and edges, with mandatory `source_id` provenance
- hybrid retrieval — `search`, `search_filter`, `search_reranked`, `search_explained`,
  `search_text_only`, and graph-expanded `search_expand`
- read verbs — `read_get`, `read_get_many`, `read_list`, `read_collection`, `read_mutations`,
  `graph_neighbors`, `crossed_boundary_since`
- record lifecycle — `transition`, `purge`, `erase_source`
- declarative projections — `configure_projections`, `read_projections`

The default build is deliberately free of recovery-named and raw-SQL methods.

## Cargo features

| Feature | Effect |
| --- | --- |
| `operator` | Adds the operator / recovery seam (`rebuild_*`, `excise_source`, `dump_*`, `trace_source_ref`, `truncate_wal`, `verify_embedder`, `check_integrity`, `safe_export`, `recompute_mean`) and its report types. This is a gate, not a behaviour switch — the engine behaves identically either way. [`fathomdb-cli`](https://crates.io/crates/fathomdb-cli) enables it; applications normally should not. |

Embedder and GPU features (`default-embedder`, `embed-cuda`, `embed-metal`, `default-reranker`,
`rerank-cuda`, `rerank-metal`) live on
[`fathomdb-engine`](https://crates.io/crates/fathomdb-engine) and are not forwarded through this
facade.

## Related crates

| Crate | Role |
| --- | --- |
| [`fathomdb-engine`](https://crates.io/crates/fathomdb-engine) | The engine core this facade re-exports |
| [`fathomdb-cli`](https://crates.io/crates/fathomdb-cli) | The `fathomdb` operator / diagnostics binary |
| [`fathomdb-embedder-api`](https://crates.io/crates/fathomdb-embedder-api) | The `Embedder` trait contract |

There are also Python (`pip install fathomdb`) and Node.js (`npm install fathomdb`) SDKs over the
same engine.

## License

MIT. See the `LICENSE` file shipped in this crate.

Source, issues and full documentation: <https://github.com/coreyt/fathomdb>
