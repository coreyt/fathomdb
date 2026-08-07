# Errors

Single-rooted exception hierarchy. Python root is `EngineError`; TS
root is `FathomDbError`. Both bindings expose 1:1 the same **27**
classes below the root (idiomatic spelling: Python snake_case payload
fields, TS camelCase). Panic carriers are deliberately outside the
catch-all root.

Two of the 27 are themselves parents — `VectorError`
(`KindNotVectorIndexedError`) and `EmbedderError`
(`EmbedderNotConfiguredError`) — so catching a parent also catches its
child.

Authoritative spec: [`dev/design/errors.md`](https://github.com/coreyt/fathomdb/blob/main/dev/design/errors.md).

## Catch-all base

| Binding    | Class            | Module                |
| ---------- | ---------------- | --------------------- |
| Python     | `EngineError`    | `fathomdb.errors`     |
| TypeScript | `FathomDbError`  | `fathomdb` (top-level)|

Catch-all examples:

```python
from fathomdb.errors import EngineError
try:
    engine.write([...])
except EngineError as e:
    log.exception("fathomdb call failed", exc_info=e)
```

```ts
import { FathomDbError } from "fathomdb";
try {
  await engine.write([...]);
} catch (e) {
  if (e instanceof FathomDbError) { /* ... */ }
  throw e;
}
```

## Class matrix

| Class                              | Trigger                                                                       | Typed payload (Py / TS)                                                  | Recovery hint           |
| ---------------------------------- | ----------------------------------------------------------------------------- | ------------------------------------------------------------------------ | ----------------------- |
| `StorageError`                     | SQLite-layer fault on a non-corruption path                                   | —                                                                        | retry; if persistent, run `doctor check-integrity` |
| `ProjectionError`                  | Projection apply fault                                                        | —                                                                        | run `doctor check-integrity --full`; recover with `--rebuild-projections` |
| `VectorError`                      | `sqlite-vec` fault                                                            | —                                                                        | run `doctor check-integrity`; recover with `--rebuild-vec0` |
| `EmbedderError`                    | Embedder call failed                                                          | —                                                                        | check embedder process / timeout; see `embedder_call_timeout_ms` |
| `EmbedderNotConfiguredError`       | Vector op attempted with no embedder configured                               | —                                                                        | configure an embedder via `admin.configure` |
| `KindNotVectorIndexedError`        | Vector op attempted on a kind that has no vector projection                   | —                                                                        | add vector projection in schema |
| `SchedulerError`                   | Background scheduler fault                                                    | —                                                                        | retry; on persistent failure, restart process |
| `OpStoreError`                     | Op-store (write log) fault                                                    | —                                                                        | run `doctor check-integrity --full` |
| `WriteValidationError`             | A caller-supplied **shape** failed validation: a missing/empty/reserved `source_id`, an unsatisfiable `valid_from >= valid_until` window, a non-integer temporal bound, or a projection spec carrying `fts`/`vector` without the `searchable` role | — (message-less; see below) | fix the batch or spec before calling |
| `SchemaValidationError`            | Admin schema configuration failed validation                                  | —                                                                        | fix the schema |
| `OverloadedError`                  | Backpressure: queue full                                                      | —                                                                        | slow producers; raise `embedder_pool_size` or `scheduler_runtime_threads` |
| `ClosingError`                     | Operation issued while engine is closing                                      | —                                                                        | do not reuse a closed engine |
| `DatabaseLockedError`              | On-disk lock held by another process                                          | `holder_pid` / `holderPid`                                               | wait for holder to release, or kill it |
| `CorruptionError`                  | Open-time integrity failure                                                   | `kind`, `stage`, `recovery_hint_code` / camelCase + `doc_anchor`         | follow `recovery_hint_code`; see `doctor` + `recover` |
| `IncompatibleSchemaVersionError`   | DB on-disk schema not compatible with this build                              | —                                                                        | upgrade engine; or downgrade DB                                |
| `MigrationError`                   | Migration step failed                                                         | —                                                                        | see `doctor check-integrity`; may require `recover` |
| `EmbedderIdentityMismatchError`    | Configured embedder identity differs from stored                              | `stored_name`, `stored_revision`, `supplied_name`, `supplied_revision`   | restore prior embedder OR re-embed with new identity |
| `EmbedderDimensionMismatchError`   | Configured embedder dimension differs from stored                             | `stored`, `supplied`                                                     | restore prior dimension OR re-embed |
| `ExtractorError`                   | BYO-LLM extraction harness protocol error (`ingest_with_extractor`)          | —                                                                        | check extractor command + stderr |
| `ConsolidatorError`                | BYO-LLM consolidation provider protocol error (`consolidate_with_provider`)  | —                                                                        | check provider command + advertised tasks |
| `InvalidArgumentError`             | Invalid argument — e.g. `depth > 3` in `graph.neighbors`, a ranked-search limit outside `1..=100`, an unrecognised enum spelling, or a `ReadView` existence flag on the search path | —                          | fix the call argument |
| `InvalidFilterError`               | Invalid filter predicate — e.g. non-allowlisted `json_path` in `read.list`   | —                                                                        | use an allowlisted path (`$.status`, `$.priority`, `$.tags`, `$.kind`, `$.created_at`) |
| `VectorEquivalenceMismatchError`   | Open-time vector-equivalence self-check found a divergence, so vector-dependent arms refuse at query time | `reason`                                     | use `search_text_only` / `searchTextOnly`; re-embed under the current backend |
| `IllegalTransitionError`           | `transition` asked for a lifecycle move the state machine forbids            | `from_state`, `to_state`, `legal` / `fromState`, `toState`, `legal`      | pick a target from `legal` |
| `NotLifecycleAddressableError`     | `transition` / `purge` addressed a non-`l:` id (`h:` content, `p:` passage)  | `id_space` / `idSpace`                                                   | only `logical`-space ids are lifecycle-addressable |
| `ErasureIncompleteError`           | An erasure verb deleted its rows but could not finish the erasure at rest    | `stage`, `detail`                                                        | retry the verb (idempotent) once concurrent readers have finished |
| `ProjectionDestructiveError`       | `configure_projections` refused a destructive change to a live projection with no explicit `drop` | `name`, `delta`                                     | re-issue naming the projection in `drop`, or make the change non-destructive |

## `WriteValidationError` carries no payload (breaking in 0.8.20)

`WriteValidationError` is a **message-less** class: Python raises it with the
fixed string `"write validation error"`, TypeScript with the envelope
`FDB_WRITE_VALIDATION` / `data: null`.

Two refusals moved onto it in 0.8.20 and no longer name the offending value:

- an unsatisfiable node validity window (`valid_from >= valid_until`) —
  previously `InvalidArgumentError` **carrying both bounds**;
- a projection spec that declares an `fts` / `vector` sub-object without the
  `searchable` role.

If you parsed the bounds out of the message, validate the pair before calling
instead. `InvalidArgumentError` is unchanged for every other use.

## Catching erasure failures

`erase_source` / `eraseSource` can fail after deleting rows but before the
erasure is complete at rest. The class is **`ErasureIncompleteError`** — note
the `Error` suffix:

```python
from fathomdb.errors import ErasureIncompleteError, WriteValidationError

try:
    report = engine.erase_source("tenant-a")
except ErasureIncompleteError as e:
    # Deleted, but not finished at rest (e.g. a reader pinned the WAL).
    # Idempotent — retry once the reader has finished.
    log.warning("erasure incomplete at stage %s: %s", e.stage, e.detail)
except WriteValidationError:
    # Empty, whitespace-only, or reserved ("_"-prefixed) source_id.
    raise
```

```ts
import { ErasureIncompleteError, WriteValidationError } from "fathomdb";

try {
  const report = await engine.eraseSource("tenant-a");
} catch (e) {
  if (e instanceof ErasureIncompleteError) {
    console.warn(`erasure incomplete at stage ${e.stage}: ${e.detail}`);
  } else if (e instanceof WriteValidationError) {
    throw e;
  } else {
    throw e;
  }
}
```

## Recovery hint codes

`CorruptionError.recovery_hint_code` is a stable string identifier
(e.g. `E_CORRUPT_INTEGRITY_CHECK`) keyed in `dev/design/errors.md`.
Operators dispatch on the code, not the message.

## Panic carriers

Rust runtime panics surface as:

- Python: `pyo3_runtime.PanicException` (PyO3-owned; **not**
  `EngineError`).
- TypeScript: `FathomDbPanicError` (TS-owned; **not** `FathomDbError`).

Panics indicate a contract bug. They are deliberately outside the
catch-all root so `except EngineError` / `catch FathomDbError` does
not silently swallow them.

## Worked example — corruption recovery

```python
from fathomdb import Engine
from fathomdb.errors import CorruptionError

try:
    engine = Engine.open("./mydb.fdb")
except CorruptionError as e:
    print("kind:", e.kind)
    print("stage:", e.stage)
    print("hint:", e.recovery_hint_code)
    # operator path: see CLI reference
    # fathomdb doctor check-integrity --full --json
    # fathomdb recover --accept-data-loss --rebuild-vec0 --json
    raise
```

## See also

- [Python API](python-api.md)
- [TypeScript API](typescript-api.md)
- [CLI](cli.md)
- [Erasure](../operations/erasure.md)
- Authoritative spec: [`dev/design/errors.md`](https://github.com/coreyt/fathomdb/blob/main/dev/design/errors.md)
