# Getting Started

> **Pre-release notice.** 0.6.0 has not yet been published to
> crates.io / PyPI / npm. This page previews the install paths
> that will be available when the GA tag fires. For current
> status see `release-notes/0.6.0.md`.

## Planned installation (post-GA)

When `v0.6.0` is published:

### Python

```bash
pip install fathomdb==0.6.0
```

```python
from fathomdb import Engine

engine = Engine.open("./my.db")
receipt = engine.write([{"kind": "doc", "body": "hello"}])
result = engine.search("hello")
engine.close()
```

### TypeScript / Node.js

```bash
npm install fathomdb@0.6.0
```

```ts
import { Engine } from "fathomdb";

const engine = await Engine.open("./my.db");
const receipt = await engine.write([{ kind: "doc", body: "hello" }]);
const result = await engine.search("hello");
await engine.close();
```

> TypeScript SDK is not yet Python-parity. See
> `release-notes/0.6.0.md` § "TypeScript SDK parity".

### Rust

```toml
[dependencies]
fathomdb = "0.6.0"
```

Or for the CLI:

```bash
cargo install fathomdb-cli --version 0.6.0
```

## What's in the box (preview)

- Five-verb runtime SDK: `Engine.open`, `write`, `search`,
  `close`, `admin.configure`.
- Engine-attached instrumentation: `drain`, `counters`,
  `set_profiling`, `set_slow_threshold_ms`, host-logger attach.
- CLI verbs: `fathomdb doctor` (integrity checks,
  safe-export, recovery info) and `fathomdb recover`
  (accept-data-loss path). Logical-id verbs deferred to 0.7.x.
- Local-first storage on SQLite (FTS5 + `sqlite-vec`).
- Two-axis versioning: workspace lockstep plus the independently
  versioned `fathomdb-embedder-api` trait crate.

## Editable install from source (current)

Until 0.6.0 publishes, the only install path is from source on
the `0.6.0-rewrite` branch:

```bash
git clone https://github.com/coreyt/fathomdb
cd fathomdb
git checkout 0.6.0-rewrite
pip install -e src/python/
```

This builds the PyO3 binding via maturin against the local Rust
workspace.

For TypeScript:

```bash
cd src/ts && npm install && npm run build
```

For Rust:

```bash
cargo build --workspace
```

## Next steps

- `guides/` — task-based walkthroughs (post-GA).
- `concepts/` — data model + lifecycle (post-GA).
- `reference/` — public API surface (post-GA).
- `release-notes/0.6.0.md` — what's deferred + what ships.
