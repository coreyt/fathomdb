# Concepts

Mental model for the unpublished 0.8.22 development candidate. The current
published release is 0.8.21; candidate-only APIs described here are not
available from a registry until the held release gates complete. Detailed
treatment lives in internal design docs under
[`dev/design/`](https://github.com/coreyt/fathomdb/tree/main/dev/design);
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
   governed verb surface. Writes are serialized through the
   writer thread; retrieval reads are served by the reader pool; admin
   configurations apply in write order. Pure projection introspection
   (`read.projections` / `read.projection_status`) instead uses the ordinarily
   opened Engine connection and may briefly take its connection lock; it neither
   mutates nor promises a separately opened read-only SQLite mode.
3. **Close** — `engine.close()` joins the writer thread, drains the
   scheduler, releases SQLite handles, and releases the on-disk lock.
   Idempotent.
4. **Process exit** — the process must exit cleanly after close. The
   wheel-on-disk lock cleanup and process exit are the bug signal the
   release smokes watch for (per `feedback_release_verification`).

## Governed runtime surface

The **core** five, present in every binding:

- `Engine.open(path, **config)` — open or create a DB.
- `engine.write(batch)` — enqueue canonical rows.
- `engine.search(query)` — hybrid retrieval (FTS5 + vector).
- `engine.close()` — release resources.
- `admin.configure(engine, name=..., body=...)` — apply a schema /
  embedder / projection configuration.

Around them sit the rest of the governed surface: the `read.*` and
`graph.*` namespaces, the lifecycle/erasure verbs (`transition`,
`purge`, `erase_source`), the projection registry
(`configure_projections`, `read.projections`), the retrieval
primitives (`search_text_only`, `rerank`, `embed`) and the BYO-LLM
verbs (`ingest_with_extractor`, `consolidate_with_provider`). The
membership is pinned by
`src/conformance/governed-surface-allowlist.json` and asserted by both
binding test suites.

Engine-attached instrumentation (`open_report`, `drain`, `counters`,
`set_profiling`, `set_slow_threshold_ms`, subscriber attach) is not
an additional top-level verb; it is a method namespace on the
`Engine` handle.

## Provenance is mandatory

Every canonical node and edge carries a **`source_id`** — the
provenance handle `erase_source` addresses. It is structurally
mandatory: in Rust an un-provenanced write does not compile
(`SourceId` has no other constructor), and the Python / TypeScript
bindings raise `WriteValidationError`. A row without one would be
reachable by no erasure verb. Treat `source_id` as a public identifier
and keep personal data out of it — see
[Erasure](../operations/erasure.md).

## Identity

Three distinct identifiers, deliberately not interchangeable:

- **`logical_id`** — the caller-supplied, cross-re-ingestion identity
  of a *governed* row. Re-writing the same `logical_id` supersedes the
  prior active version (invalidate-not-delete). Only `logical_id`-keyed
  rows are addressable by `transition` / `purge`.
- **`SearchHit.id`** — a typed `IdSpace` (`{space, value}`) returned by
  `search`. `space` is `logical` (`l:`), `content` (`h:`) or `passage`
  (`p:`). Stable across sessions and re-ingest.
- **`write_cursor`** — the engine's positional book-keeping value. It
  is reassigned on re-projection, is **not** cross-session stable, and
  the SDKs no longer surface it as a hit id.

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
[`src/rust/crates/fathomdb-embedder-api/`](https://github.com/coreyt/fathomdb/tree/main/src/rust/crates/fathomdb-embedder-api)).

## Recovery surface

The CLI exposes two roots:

- `fathomdb doctor <verb>` — read-only or artifact-producing
  diagnostics. `check-integrity`, `safe-export`, `verify-embedder`,
  `trace`, `dump-schema`, `dump-row-counts`, `dump-profile`,
  `dump-mutations`, `orphan-provenance`, `warm-cache`,
  `recompute-mean`.
- `fathomdb recover --accept-data-loss <sub-flag>` — the only lossy
  root. `--truncate-wal`, `--rebuild-vec0`, `--rebuild-projections`,
  `--excise-source <id>`, `--excise-collection`/`--excise-record-key`.

Engine errors from the SDK carry recovery hints (notably
`CorruptionError.recovery_hint_code`); operators dispatch on the
hint code, not the message.

The CLI is **operator-only** — it does not mirror the SDK application
surface. There is no `fathomdb search` / `get` / `list`.

**Erasure does not require the CLI.** Since 0.8.20 the SDK ships
`purge` (one governed node, by `logical_id`) and `erase_source` (every
row from one provenance), so an embedded consumer with no `fathomdb`
binary on `PATH` can discharge a deletion obligation. The CLI seam
`fathomdb recover --accept-data-loss --excise-source <id>` remains, and
is the *only* route into the engine's reserved `_`-prefixed provenance
namespace. See [Erasure](../operations/erasure.md).

## See also

- [Quickstart](../getting-started/quickstart.md)
- [Reference — Errors](../reference/errors.md)
- [Reference — CLI](../reference/cli.md)
- [Compatibility](../compatibility/index.md)
- [Erasure](../operations/erasure.md)
