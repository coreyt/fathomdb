# Install — Rust

Two consumption paths for Rust users:

- **`fathomdb` facade crate** — re-exports the runtime verbs from
  `fathomdb-engine` for downstream Rust libraries and applications.
- **`fathomdb-cli` operator CLI** — `fathomdb doctor` and
  `fathomdb recover` verbs (Phase 10a). Operator-only; does **not**
  ship `search` / `get` / `list` query verbs.

## Requirements

- Rust **stable** toolchain (`rustup default stable`).
- SQLite headers + a system `sqlite-vec` build, or vendored equivalent
  (the workspace builds `sqlite-vec` from source by default).
- A platform that matches the
  [release matrix](../compatibility/index.md): linux x86_64/aarch64,
  darwin x86_64/arm64, windows x86_64.

## Install (post-GA)

Library:

```bash
cargo add fathomdb
```

CLI:

```bash
cargo install fathomdb-cli --version 0.6.0
```

## Install (pre-GA, from git)

Library:

```bash
cargo add fathomdb \
  --git https://github.com/coreyt/fathomdb \
  --branch 0.6.0-rewrite
```

CLI:

```bash
cargo install fathomdb-cli \
  --git https://github.com/coreyt/fathomdb \
  --branch 0.6.0-rewrite
```

## Verify

```rust
use fathomdb::Engine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::open("./hello.fdb")?;
    engine.write(&[])?;
    engine.search("hello")?;
    engine.close()?;
    println!("ok");
    Ok(())
}
```

For the CLI:

```bash
fathomdb doctor check-integrity --quick --json
```

## See also

- [Reference — CLI](../reference/cli.md)
- [Reference — errors](../reference/errors.md)
- Rust API docs (auto-published to `docs.rs/fathomdb` post-GA). Until
  then, see [`src/rust/crates/fathomdb/`](https://github.com/coreyt/fathomdb/tree/0.6.0-rewrite/src/rust/crates/fathomdb).
