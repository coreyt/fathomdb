# Querying Operational Collections

The operational store is FathomDB's purpose-built substrate for ephemeral,
high-churn state that does not belong in the versioned graph — sync
cursors, connector health, rate-limit counters, audit logs, scheduler
runs. See [Operational Store](../concepts/operational-store.md) for the
model. This guide walks through the **read** side of the surface: how
to declare the filter fields a collection supports, how to issue a
filtered read against it, and how the three filter modes (`EXACT`,
`PREFIX`, `RANGE`) compose with the two collection kinds
(`APPEND_ONLY_LOG` and `LATEST_STATE`).

## Why operational reads are admin-only, not a `Query` chain

Graph reads go through a fluent `Query` chain
(`db.nodes("Document").search(...).execute()`). Operational reads do
not. There is no `db.operational("audit_log").filter(...).execute()`
builder, and there will not be one in 0.3.1 or 0.3.2. Operational
reads are intentionally an **admin-surface** call:

```python
report = engine.admin.read_operational_collection(request)
```

The reasoning is deliberate. Operational collections sit outside the
node/edge/chunk model: they have no schema validation, no supersession,
no text search, no vector retrieval, no joins. Exposing them as a
`Query` chain would imply composition with graph operations that is
not supported at the engine level and would have to be policed by the
binding layer. Keeping operational reads on the admin client makes the
"this is not a graph query" boundary visible at the call site.

If you need to correlate operational state with graph state, read the
two surfaces separately and join in application code.

## Pick the collection kind first

The collection kind shapes the read experience more than the filter
mode does:

- **`APPEND_ONLY_LOG`** — every mutation is preserved forever (subject
  to retention policy). Reads return the full mutation history, ordered
  by `created_at`. Use for audit trails, event logs, and status
  history. `read_operational_collection` returns the raw mutation rows
  and the caller can fold or aggregate them in application code.
- **`LATEST_STATE`** — every mutation is still appended to a canonical
  log, but a derived current-state view reflects the latest value per
  `record_key`. `read_operational_collection` returns mutation rows
  matching the filter; use `trace_operational_collection` if you want
  the current-state view explicitly. Use for cursors, health checks,
  and counters.

`read_operational_collection` returns `OperationalReadReport`, whose
`rows` field is a list of `OperationalMutationRow`. The shape is the
same for both collection kinds — the difference is *what* you get back
(full history vs. recent mutations against a tight filter).

## Declaring filter fields

Filters only work against fields the collection has declared. Declare
them at register time via `filter_fields_json` on
`OperationalRegisterRequest`, or update them later via
`admin.update_operational_collection_filters`. The JSON payload is a
list of field descriptors:

```python
from fathomdb import OperationalCollectionKind, OperationalRegisterRequest

filter_fields_json = """
[
  {"name": "actor",  "type": "string",    "modes": ["exact", "prefix"]},
  {"name": "status", "type": "string",    "modes": ["exact"]},
  {"name": "ts",     "type": "timestamp", "modes": ["range"]}
]
"""

request = OperationalRegisterRequest(
    name="scheduler_runs",
    kind=OperationalCollectionKind.APPEND_ONLY_LOG,
    schema_json='{"fields": ["actor", "status", "ts", "duration_ms"]}',
    retention_json='{"policy": "keep_all"}',
    filter_fields_json=filter_fields_json,
    format_version=1,
)
engine.admin.register_operational_collection(request)
```

Each descriptor has three keys:

- **`name`** — the payload-JSON key to match against. The engine
  extracts the value at that key from each mutation's payload.
- **`type`** — one of `string`, `integer`, `timestamp`. This is
  [`OperationalFilterFieldType`](../reference/types.md) on the Python
  side and determines what kinds of filter clause can match the field.
- **`modes`** — the list of filter modes this field supports. Values
  are `exact`, `prefix`, `range`. A field declared with `["exact"]`
  cannot be used with `PREFIX` or `RANGE` clauses.

**Rules of thumb.**

- `string` fields can declare `exact` and `prefix`.
- `integer` and `timestamp` fields can declare `exact` and `range`.
- `prefix` against an `integer` or `timestamp` is not supported.
- Any field not declared here is **not filterable**. Reads with a
  clause against an undeclared field will error.

For multi-field lookups beyond single-clause filtering, declare
secondary indexes via
`admin.update_operational_collection_secondary_indexes`. Secondary
indexes are covered in [Admin Operations](../operations/admin-operations.md).

## The read request shape

`OperationalReadRequest` has three fields:

```python
from fathomdb import (
    OperationalFilterClause,
    OperationalFilterMode,
    OperationalFilterValue,
    OperationalReadRequest,
)

request = OperationalReadRequest(
    collection_name="scheduler_runs",
    filters=[
        # one or more OperationalFilterClause — ANDed together.
    ],
    limit=100,
)
report = engine.admin.read_operational_collection(request)
```

- **`collection_name`** — the registered collection to read from.
- **`filters`** — a list of `OperationalFilterClause`, conjoined with
  AND. An empty list means "return up to `limit` rows with no
  predicate" and is how you dump a small collection.
- **`limit`** — an upper bound on returned rows. `OperationalReadReport`
  carries an `applied_limit` and a `was_limited` flag so callers can
  detect truncation.

`OperationalFilterClause` has three classmethod constructors, one per
mode, which are the idiomatic call sites:

```python
# EXACT — single value, all field types
OperationalFilterClause.exact("actor", OperationalFilterValue.string("scheduler"))

# PREFIX — string fields only
OperationalFilterClause.prefix("actor", "scheduler-")

# RANGE — integer / timestamp fields only
OperationalFilterClause.range("ts", lower=1_712_400_000, upper=1_712_486_400)
```

`OperationalFilterValue` is the typed wrapper that carries a string
or integer payload and is used with the EXACT constructor.

## Worked example — EXACT

Find every mutation in `scheduler_runs` whose payload has
`"status": "failed"`. The `status` field must have been declared with
`modes: ["exact"]`.

```python
from fathomdb import (
    OperationalFilterClause,
    OperationalFilterValue,
    OperationalReadRequest,
)

request = OperationalReadRequest(
    collection_name="scheduler_runs",
    filters=[
        OperationalFilterClause.exact(
            "status", OperationalFilterValue.string("failed")
        ),
    ],
    limit=50,
)
report = engine.admin.read_operational_collection(request)

for row in report.rows:
    print(row.created_at, row.record_key, row.payload_json)

if report.was_limited:
    print(f"Result truncated at {report.applied_limit} rows")
```

EXACT matches the literal JSON value at the declared field. For
`string` fields, pass `OperationalFilterValue.string(...)`; for
`integer` fields, `OperationalFilterValue.integer(...)`. The mismatch
cases (a string value against an integer field, or vice versa) are
rejected at read time.

## Worked example — PREFIX

Find every mutation whose `actor` field starts with `scheduler-`. The
`actor` field must have been declared with `type: "string"` and
`modes` including `"prefix"`.

```python
request = OperationalReadRequest(
    collection_name="scheduler_runs",
    filters=[
        OperationalFilterClause.prefix("actor", "scheduler-"),
    ],
    limit=100,
)
report = engine.admin.read_operational_collection(request)
```

PREFIX is a left-anchored string match — `"scheduler-"` matches
`"scheduler-nightly"` and `"scheduler-hourly"` but not
`"my-scheduler-nightly"`. PREFIX is only valid against `string`
fields; the engine rejects a PREFIX clause against an `integer` or
`timestamp` field.

## Worked example — RANGE

Find every mutation whose `ts` field falls in the last 24 hours. The
`ts` field must have been declared with `type: "timestamp"` (or
`"integer"`) and `modes` including `"range"`.

```python
import time

now = int(time.time())
one_day_ago = now - 86_400

request = OperationalReadRequest(
    collection_name="scheduler_runs",
    filters=[
        OperationalFilterClause.range("ts", lower=one_day_ago, upper=now),
    ],
    limit=500,
)
report = engine.admin.read_operational_collection(request)
```

RANGE takes `lower` and `upper` as inclusive integer bounds. Either
bound may be `None` for a one-sided range (`lower=N, upper=None` is
"at least N"). RANGE is only valid against `integer` and `timestamp`
fields.

## Combining clauses

Multiple clauses on a single request are ANDed together. A request
like "failed runs of the nightly scheduler in the last day" composes
directly:

```python
request = OperationalReadRequest(
    collection_name="scheduler_runs",
    filters=[
        OperationalFilterClause.prefix("actor", "scheduler-nightly"),
        OperationalFilterClause.exact(
            "status", OperationalFilterValue.string("failed")
        ),
        OperationalFilterClause.range("ts", lower=one_day_ago, upper=now),
    ],
    limit=100,
)
report = engine.admin.read_operational_collection(request)
```

Each clause must target a field that is both declared and supports the
clause's mode. Multi-field combinations like this benefit from a
declared secondary index — see
[Admin Operations](../operations/admin-operations.md) — but the read
API itself is unchanged.

## Reading the response

`OperationalReadReport` is the envelope:

```python
report.collection_name   # str
report.row_count         # int — len(report.rows)
report.applied_limit     # int — the limit the engine actually enforced
report.was_limited       # bool — True if more rows matched than were returned
report.rows              # list[OperationalMutationRow]
```

Each `OperationalMutationRow` carries:

```python
row.id               # str — unique mutation id
row.collection_name  # str
row.record_key       # str — the record_key argument to add_operational_*
row.op_kind          # str — "append", "put", or "delete"
row.payload_json     # Any — the decoded JSON payload
row.source_ref       # str | None — write-time attribution
row.created_at       # int — seconds since the Unix epoch
```

For `LATEST_STATE` collections, `op_kind` is `"put"` or `"delete"`;
for `APPEND_ONLY_LOG`, it is always `"append"`. `record_key` is the
stable identity within the collection and is the key used by the
derived current-state view on `LATEST_STATE`.

## When reads are the wrong tool

- **"Give me the current state of every key in a `LATEST_STATE`
  collection."** Use `admin.trace_operational_collection(...)` — it
  returns both the mutation history and the current-state view. The
  read API surfaces mutation rows, not the materialized current state.
- **"Is this operational state correlated with a graph node?"**
  Operational reads return opaque payload JSON. Correlation with graph
  state — joining a sync cursor back to the `Source` node that owns
  it, for example — is a caller concern. Read the two surfaces
  separately and join in application code.
- **"I need full-text or vector search over operational payloads."**
  Not supported. If the data is search-relevant, it belongs in the
  graph.

## See also

- [Operational Store](../concepts/operational-store.md) — concept
  material, collection-kind semantics, write-side API, lifecycle, and
  maintenance primitives.
- [Admin Operations](../operations/admin-operations.md) — the full
  admin surface, including `update_operational_collection_filters`
  and `update_operational_collection_secondary_indexes`.
- [Writing Data](./writing-data.md) — operational write API via
  `WriteRequestBuilder.add_operational_append` /
  `add_operational_put` / `add_operational_delete`.
