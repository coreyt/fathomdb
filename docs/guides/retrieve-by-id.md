# Retrieving records by id and reading op-store mutations

Beyond `search`, the governed `read.*` namespace gives you **deterministic
point lookups** by `logical_id` and **paginated read-back** of an op-store
append-only log — without any raw SQL. Every read rides the engine's reader
pool on a stable snapshot, so reads never block the single writer.

## `read.get` / `read.get_many` — point lookup by id

`logical_id` is the stable, cross-re-ingestion identity you supply at write
time. `read.get` returns the **active** version only (a superseded version is
never returned); a missing or superseded id is a normal `None`/`null`, not an
error. `read.get_many` returns one slot per requested id, **in request order**,
with `None`/`null` for any miss.

### Python

```python
from fathomdb import Engine, read

engine = Engine.open("memory.sqlite")
engine.write([{"kind": "fact", "body": "the sky is blue", "logical_id": "F1"}])

record = read.get(engine, "F1")
print(record.logical_id, record.kind, record.body)  # F1 fact the sky is blue
print(read.get(engine, "missing"))                   # None

rows = read.get_many(engine, ["F1", "missing"])
print(rows[0].body, rows[1])                          # the sky is blue None
engine.close()
```

### TypeScript

```ts
import { Engine, read } from "fathomdb";

const engine = await Engine.open("memory.sqlite");
await engine.write([{ kind: "fact", body: "the sky is blue", logicalId: "F1" }]);

const record = await read.get(engine, "F1");
console.log(record?.logicalId, record?.kind, record?.body);
console.log(await read.get(engine, "missing")); // null

const rows = await read.getMany(engine, ["F1", "missing"]);
console.log(rows[0]?.body, rows[1]); // the sky is blue null
await engine.close();
```

## `read.collection` / `read.mutations` — paginated op-store read-back

Append rows to an `append_only_log` collection, then page them back in append
order. `limit` is **mandatory** (the engine clamps it to a ~1M cap — there is no
unbounded read); `after_id` / `afterId` is the exclusive cursor for the next
page. `read.mutations` is an alias over the same read-back.

### Python

```python
from fathomdb import Engine, read

engine = Engine.open("events.sqlite")
engine.write([{"admin_schema": {"name": "events", "kind": "append_only_log",
                                "schema_json": "{}", "retention_json": "{}"}}])
for i in range(5):
    engine.write([{"op_store": {"collection": "events",
                                "record_key": f"e{i}", "body": "{}"}}])

page1 = read.collection(engine, "events", limit=3)
page2 = read.collection(engine, "events", after_id=page1[-1].id, limit=3)
print([r.record_key for r in page1], [r.record_key for r in page2])
engine.close()
```

### TypeScript

```ts
import { Engine, read } from "fathomdb";

const engine = await Engine.open("events.sqlite");
await engine.write([{ adminSchema: { name: "events", kind: "append_only_log",
                                     schemaJson: "{}", retentionJson: "{}" } }]);
for (let i = 0; i < 5; i++) {
  await engine.write([{ opStore: { collection: "events", recordKey: `e${i}`, body: "{}" } }]);
}

const page1 = await read.collection(engine, "events", { limit: 3 });
const page2 = await read.collection(engine, "events", { afterId: page1[page1.length - 1].id, limit: 3 });
console.log(page1.map((r) => r.recordKey), page2.map((r) => r.recordKey));
await engine.close();
```

## See also

- [Python API — `read.*`](../reference/python-api.md)
- [TypeScript API — `read.*`](../reference/typescript-api.md)
- [Working with structured search hits](structured-search-hits.md)
