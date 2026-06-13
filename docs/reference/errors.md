# Errors

Single-rooted exception hierarchy. Python root is `EngineError`; TS
root is `FathomDbError`. Both bindings expose 1:1 the same 20 leaf
classes (idiomatic spelling: Python snake_case payload fields, TS
camelCase). Panic carriers are deliberately outside the catch-all
root.

Authoritative spec: [`dev/design/errors.md`](https://github.com/coreyt/fathomdb/blob/0.6.0-rewrite/dev/design/errors.md).

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

## Leaf class matrix

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
| `WriteValidationError`             | Caller-supplied write batch failed validation                                 | —                                                                        | fix the batch; check schema |
| `SchemaValidationError`            | Admin schema configuration failed validation                                  | —                                                                        | fix the schema |
| `OverloadedError`                  | Backpressure: queue full                                                      | —                                                                        | slow producers; raise `embedder_pool_size` or `scheduler_runtime_threads` |
| `ClosingError`                     | Operation issued while engine is closing                                      | —                                                                        | do not reuse a closed engine |
| `DatabaseLockedError`              | On-disk lock held by another process                                          | `holder_pid` / `holderPid`                                               | wait for holder to release, or kill it |
| `CorruptionError`                  | Open-time integrity failure                                                   | `kind`, `stage`, `recovery_hint_code` / camelCase + `doc_anchor`         | follow `recovery_hint_code`; see `doctor` + `recover` |
| `IncompatibleSchemaVersionError`   | DB on-disk schema not compatible with this build                              | —                                                                        | upgrade engine; or downgrade DB                                |
| `MigrationError`                   | Migration step failed                                                         | —                                                                        | see `doctor check-integrity`; may require `recover` |
| `EmbedderIdentityMismatchError`    | Configured embedder identity differs from stored                              | `stored_name`, `stored_revision`, `supplied_name`, `supplied_revision`   | restore prior embedder OR re-embed with new identity |
| `EmbedderDimensionMismatchError`   | Configured embedder dimension differs from stored                             | `stored`, `supplied`                                                     | restore prior dimension OR re-embed |
| `ExtractorError`                   | BYO-LLM extraction harness protocol error (Slice 15 / G11)                   | —                                                                        | check extractor command + stderr |
| `InvalidArgumentError`             | Invalid argument — e.g. `depth > 3` in `graph.neighbors` (Slice 20 / G5/G6)  | —                                                                        | fix the call argument |
| `InvalidFilterError`               | Invalid filter predicate — e.g. non-allowlisted `json_path` in `read.list` (Slice 35 / G4) | —                                                               | use an allowlisted path (`$.status`, `$.priority`, `$.tags`, `$.kind`, `$.created_at`) |

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
- Authoritative spec: [`dev/design/errors.md`](https://github.com/coreyt/fathomdb/blob/0.6.0-rewrite/dev/design/errors.md)
