# Quickstart

End-to-end walkthrough: install, open a fresh DB, write rows, search,
inspect counters, close, exit cleanly. Python is the primary language
(the more heavily exercised binding — see
[SDK parity](../positions/sdk-parity.md)); TS snippets sit alongside.

This page covers the same five operations in the same order as the
post-publish smoke `scripts/release/smoke/smoke-pypi-wheel.sh`
(AC-056): `Engine.open` → `write` → `search` → `close` → process-
exit. The two differ in ergonomics — the smoke reads the DB path from
`sys.argv[1]` and uses a one-letter variable name for CI; this
quickstart hardcodes a relative path, uses `engine` for readability,
and prints `engine.counters()` as an instrumentation example. If they
diverge on the **five-operation contract**, treat it as a release-gate
blocker.

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

The structured open report is available after open via
`engine.open_report()` / `engine.openReport()`.

## 3. Write a small batch of canonical rows

`engine.write(batch)` enqueues a batch of canonical rows and returns
a `WriteReceipt` whose `cursor` advances monotonically.

**`source_id` is mandatory on every canonical node/edge item** (0.8.20).
It is the provenance handle `erase_source` addresses, so a row written
without one could never be erased; omitting it raises
`WriteValidationError`. Use an opaque document/tenant id —
[never personal data](../operations/erasure.md#source_id-must-not-contain-personal-data).

Python:

```python
receipt = engine.write([
    {"kind": "note", "body": "hello from the quickstart", "source_id": "quickstart"},
])
print(receipt.cursor)  # → 1
```

TypeScript:

```ts
const receipt = await engine.write([
  { kind: "note", body: "hello from the quickstart", sourceId: "quickstart" },
]);
console.log(receipt.cursor); // → 1
```

An **empty** batch (`write([])`) is also valid and carries no items, so
it needs no `source_id` — that is what the release smoke uses to
exercise the writer thread and op-store wiring. Real client batches are
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
for hit in result.results:
    # hit.id is a typed IdSpace: .space in {"logical", "content", "passage"}
    print(hit.id.space, hit.id.value, hit.kind, hit.score, hit.source_id)
```

TypeScript:

```ts
const result = await engine.search("hello");
console.log(result.projectionCursor);
console.log(result.softFallback);
for (const hit of result.results) {
  // hit.id is a typed IdSpace: { space, value }
  console.log(hit.id.space, hit.id.value, hit.kind, hit.score, hit.sourceId);
}
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
engine.write([
    {"kind": "note", "body": "hello from the quickstart", "source_id": "quickstart"},
])
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
- [Reference — errors](../reference/errors.md) — the 27-class taxonomy
  and recovery hints.
- [Reference — CLI](../reference/cli.md) — operator verbs (`doctor`,
  `recover`).
- [Erasure](../operations/erasure.md) — `erase_source` / `purge`, and
  what they do not reach.
