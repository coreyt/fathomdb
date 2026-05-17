# Quickstart

End-to-end walkthrough: install, open a fresh DB, write rows, search,
inspect counters, close, exit cleanly. Python is the primary language
(more mature than TS in 0.6.0 — see
[release notes § TypeScript SDK parity](../release-notes/0.6.0.md));
TS snippets sit alongside.

This page covers the same five operations in the same order as the
post-publish smoke `scripts/release/smoke/smoke-pypi-wheel.sh`
(AC-056): `Engine.open` → `write` → `search` → `close` → process-
exit. The two scripts diverge in ergonomics only — the smoke reads
the DB path from `sys.argv[1]` and uses a one-letter variable name
for CI; this quickstart hardcodes a relative path and uses
`engine` for readability, and prints `engine.counters()` as an
instrumentation example. If the two scripts diverge on the
**five-operation contract**, treat it as a release-gate blocker.

## 1. Install

See the per-language install page:

- [Python](../install/python.md)
- [TypeScript / Node.js](../install/typescript.md)
- [Rust](../install/rust.md)

Verify install with a one-liner before proceeding:

```bash
python -c "from fathomdb import Engine; print(Engine)"
```

## 2. Open a fresh DB

`Engine.open(path)` opens (or creates) a local-first SQLite database
at `path`. The handle owns the writer thread, the reader pool, and
the scheduler.

Python:

```python
from fathomdb import Engine

engine = Engine.open("./quickstart.fdb")
```

TypeScript:

```ts
import { Engine } from "fathomdb";

const engine = await Engine.open("./quickstart.fdb");
```

> **0.6.0 caveat — open report.** Both bindings currently return only
> the engine handle. The structured open report
> (`migration_version_reached`, `embedder_identity_confirmed`, open-
> stage data) defined in `dev/design/engine.md` is populated on the
> Rust side but dropped at the binding boundary. Surfacing the report
> defers to **0.6.1** (slice `12-TX-OPENREPORT`). See
> [release notes § Engine.open structured open report](../release-notes/0.6.0.md).

## 3. Write a small batch of canonical rows

`engine.write(batch)` enqueues a batch of canonical rows and returns
a `WriteReceipt` whose `cursor` advances monotonically.

Python:

```python
receipt = engine.write([])
print(receipt.cursor)  # → 1
```

TypeScript:

```ts
const receipt = await engine.write([]);
console.log(receipt.cursor); // → 1
```

The smoke uses an empty batch — exercising the writer thread and
op-store wiring is the contract. Real client batches are
caller-shaped canonical rows; see
[concepts](../concepts/index.md).

## 4. Run a search query

`engine.search(query)` runs hybrid retrieval (FTS5 + vector). Returns
a `SearchResult` with `projection_cursor`, optional `soft_fallback`,
and a `results` list.

Python:

```python
result = engine.search("hello")
print(result.projection_cursor)
print(result.soft_fallback)  # → None if neither branch fell back
print(result.results)
```

TypeScript:

```ts
const result = await engine.search("hello");
console.log(result.projectionCursor);
console.log(result.softFallback);
console.log(result.results);
```

## 5. Inspect counters

`engine.counters()` returns a `CounterSnapshot` with six fields:
`queries`, `writes`, `write_rows`, `admin_ops`, `cache_hit`,
`cache_miss`.

Python:

```python
snap = engine.counters()
print(snap.queries, snap.writes, snap.cache_hit, snap.cache_miss)
```

TypeScript:

```ts
const snap = engine.counters();
console.log(snap.queries, snap.writes, snap.cacheHit, snap.cacheMiss);
```

After step 3 + 4 you should see `writes >= 1` and `queries >= 1`.

## 6. Close + exit cleanly

`engine.close()` releases the SQLite handles, joins the writer thread,
drains the scheduler, and releases the on-disk lock. The process must
exit cleanly afterwards — the wheel-on-disk lock cleanup and process
exit are the bug signal `smoke-pypi-wheel.sh` watches for.

Python:

```python
engine.close()
print("ok")
```

TypeScript:

```ts
await engine.close();
console.log("ok");
```

## Full Python program

```python
from fathomdb import Engine

engine = Engine.open("./quickstart.fdb")
engine.write([])
result = engine.search("hello")
print(engine.counters())
engine.close()
print("ok")
```

This program exercises the same five-operation contract as
`smoke-pypi-wheel.sh` (with `engine.counters()` added as an
instrumentation example). The CI smoke variant reads the DB path
from `sys.argv[1]`, uses a one-letter variable name, and searches
for `"smoke"`; CI ergonomics aside, both scripts cover the same
`Engine.open` → `write` → `search` → `close` → process-exit
sequence per AC-056.

## Next steps

- [Concepts](../concepts/index.md) — engine lifecycle, canonical rows,
  embedder model, recovery surface.
- [Reference — Python API](../reference/python-api.md) — full surface.
- [Reference — errors](../reference/errors.md) — 18-leaf taxonomy +
  recovery hints.
- [Reference — CLI](../reference/cli.md) — operator verbs (`doctor`,
  `recover`).
