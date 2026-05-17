# Concepts

Mental model for the 0.6.0 public surface. Detailed treatment lives
in internal design docs under
[`dev/design/`](https://github.com/coreyt/fathomdb/tree/0.6.0-rewrite/dev/design);
this page is the consumer-facing overview.

## Engine lifecycle

```text
open → write / search / admin.configure / instrumentation → close → process exit
```

1. **Open** — `Engine.open(path)` creates or opens the SQLite database
   at `path`. The handle owns the writer thread, the reader pool with
   thread-affine workers, the scheduler, the op-store, and the
   embedder pool. Open is the only place migration runs.
2. **Write / search / configure** — application code calls the
   five-verb canonical surface. Writes are serialized through the
   writer thread; reads are served by the reader pool; admin
   configurations apply in write order.
3. **Close** — `engine.close()` joins the writer thread, drains the
   scheduler, releases SQLite handles, and releases the on-disk lock.
   Idempotent.
4. **Process exit** — the process must exit cleanly after close. The
   wheel-on-disk lock cleanup and process exit are the bug signal the
   release smokes watch for (per `feedback_release_verification`).

## Five-verb runtime surface

The canonical surface across every binding:

- `Engine.open(path, **config)` — open or create a DB.
- `engine.write(batch)` — enqueue canonical rows.
- `engine.search(query)` — hybrid retrieval (FTS5 + vector).
- `engine.close()` — release resources.
- `admin.configure(engine, name=..., body=...)` — apply a schema /
  embedder / projection configuration.

Engine-attached instrumentation (`drain`, `counters`,
`set_profiling`, `set_slow_threshold_ms`, subscriber attach) is not
an additional top-level verb; it is a method namespace on the
`Engine` handle.

## Canonical rows and projections

- **Canonical rows** are the durable ground-truth writes the client
  enqueues via `engine.write`. They are the source of truth and are
  bit-preservable across recovery actions other than explicit
  data-loss steps.
- **Projections** are engine-maintained derived state computed off the
  canonical row stream — FTS5 indexes, `sqlite-vec` vector indexes,
  and other shape-specific materializations. Projections are
  rebuildable from canonical rows via
  `fathomdb recover --accept-data-loss --rebuild-projections` (or
  `--rebuild-vec0` for the vector subset).

## Embedder model

Embedders are pluggable components that produce vector embeddings
for vector-indexed kinds. The trait surface lives in
`fathomdb-embedder-api` (Axis E — see
[compatibility § versioning](../compatibility/index.md)), and is
configured via `admin.configure`. Embedder identity (`name` +
`revision`) and `dimension` are stored on first configure; an attempt
to re-open with a different embedder raises
`EmbedderIdentityMismatchError` or `EmbedderDimensionMismatchError`.

Vector identity belongs to the embedder per `ADR-0.6.0-vector-identity-embedder-owned`.

Detailed trait + lifecycle docs: see Rust API docs (`docs.rs/fathomdb-embedder-api` post-publish; pre-GA, see
[`src/rust/crates/fathomdb-embedder-api/`](https://github.com/coreyt/fathomdb/tree/0.6.0-rewrite/src/rust/crates/fathomdb-embedder-api)).

## Recovery surface

The CLI exposes two roots:

- `fathomdb doctor <verb>` — read-only or artifact-producing
  diagnostics. `check-integrity`, `safe-export`, `verify-embedder`,
  `trace`, `dump-schema`, `dump-row-counts`, `dump-profile`.
- `fathomdb recover --accept-data-loss <sub-flag>` — the only lossy
  root. `--truncate-wal`, `--rebuild-vec0`, `--rebuild-projections`,
  `--excise-source <id>`.

Engine errors from the SDK carry recovery hints (notably
`CorruptionError.recovery_hint_code`); operators dispatch on the
hint code, not the message.

The CLI is **operator-only** in 0.6.0 — it does not mirror the SDK
five-verb application surface. There is no `fathomdb search` /
`get` / `list`.

Logical-id verbs (`purge_logical_id`, `restore_logical_id`) are
deferred to 0.7.x; bulk-delete-by-source for 0.6.0 uses
`fathomdb recover --accept-data-loss --excise-source <id>`.

## See also

- [Quickstart](../getting-started/quickstart.md)
- [Reference — Errors](../reference/errors.md)
- [Reference — CLI](../reference/cli.md)
- [Compatibility](../compatibility/index.md)
