# Install — Rust

Two consumption paths for Rust users:

- **`fathomdb` facade crate** — re-exports the runtime verbs from
  `fathomdb-engine` for downstream Rust libraries and applications.
- **`fathomdb-cli` operator CLI** — `fathomdb doctor` and
  `fathomdb recover` verbs. Operator-only; does **not**
  ship `search` / `get` / `list` query verbs.

> **0.8.21 is published to crates.io.** FathomDB is pre-1.0 and the surface
> is **beta**.

## Requirements

- Rust **stable** toolchain (`rustup default stable`).
- SQLite headers + a system `sqlite-vec` build, or vendored equivalent
  (the workspace builds `sqlite-vec` from source by default).
- A Rust-supported target and a working SQLite + `sqlite-vec` build. The
  published Python and npm native-artifact boundary is Linux
  `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`; Rust crates
  compile from source.

## Install the published crates

Library:

```bash
cargo add fathomdb
```

CLI:

```bash
cargo install fathomdb-cli --version 0.8.21
```

## Install (current path — from git)

Library:

```bash
cargo add fathomdb \
  --git https://github.com/coreyt/fathomdb \
  --branch main
```

CLI:

```bash
cargo install fathomdb-cli \
  --git https://github.com/coreyt/fathomdb \
  --branch main
```

## Feature flags

- **default (no features)** — the *governed application surface*: `Engine::open`
  / `write` / `search` / `close`, the `read_*` and `graph_*` verbs, the
  lifecycle verbs (`transition`, `purge`, `erase_source`) and the projection
  registry. No recovery-named method and no raw-SQL method resolves.
- **`operator`** — un-gates the operator/recovery seam (`rebuild_*`,
  `excise_source`, `dump_*`, `check_integrity`, `safe_export`, …) and the
  report types those methods return. `fathomdb-cli` enables it. Gating, not
  deletion: engine behaviour is identical either way.
- **`default-embedder`** — compiles in the in-process `bge-small-en-v1.5`
  embedder (opt in per engine at `open`). See the
  [Default Embedder guide](../embedder.md).

## Verify

```rust
use fathomdb::{Engine, PreparedWrite, SourceId};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // `Engine::open` returns an `OpenedEngine { engine, report }`.
    let opened = Engine::open("./hello.fdb")?;
    let engine = opened.engine;

    engine.write(&[PreparedWrite::Node {
        kind: "note".into(),
        body: "hello".into(),
        // `source_id` is MANDATORY since 0.8.20. The `SourceId` newtype makes
        // an un-provenanced write a COMPILE error, not a runtime rejection.
        source_id: SourceId::new("demo-source")?,
        logical_id: None,
        state: Default::default(),
        reason: None,
        valid_from: None,
        valid_until: None,
    }])?;

    engine.search("hello")?;
    engine.close()?;
    println!("ok");
    Ok(())
}
```

`PreparedWrite` is `#[non_exhaustive]`: new variants can land in a micro
release, so `match` on it with a `_` arm.

For the CLI:

```bash
fathomdb doctor check-integrity --quick --json
```

## See also

- [Reference — CLI](../reference/cli.md)
- [Reference — errors](../reference/errors.md)
- Rust API docs are auto-published to `docs.rs/fathomdb` once the crate
  publishes. Until then, run `cargo doc --open`, or read
  [`src/rust/crates/fathomdb/`](https://github.com/coreyt/fathomdb/tree/main/src/rust/crates/fathomdb).
