---
title: slice-34-cli-op-store-readback-design
date: 2026-06-06
target_release: 0.8.0
desc: CLI-only operator diagnostic `fathomdb doctor dump-mutations` — read-back of op-store rows over the existing engine read seam
blast_radius: src/rust/crates/fathomdb-cli/src/lib.rs; dev/interfaces/cli.md; dev/adr/ADR-0.6.0-cli-scope.md (amendment); dev/design/op-store.md; docs/reference/cli.md
status: accepted
---

# Slice 34 — CLI op-store read-back (`fathomdb doctor dump-mutations`)

Reserved-gap **F4-READ / reserved-gap-34**: *"CLI-only read-back of op-store data
(per ADR-0.6.0-cli-scope) without SDK-parity obligation"*
(`dev/plans/0.8.0-implementation.md:1182`). This memo settles the scope call,
fixes the verb shape, the `--json` envelope, and the exit-code mapping, then lists
the RED tests.

## (a) Scope decision — diagnostic dump, **not** an application-query verb

`ADR-0.6.0-cli-scope` chose **Option A** (two-root operator CLI: `recover` lossy,
`doctor` bit-preserving / read-only) and **rejected Option B** (a `search`/`get`/
`list` *application* query surface). The ADR is explicit (`:39`): *"ad-hoc reads
are handled via operator verbs such as `trace`, `dump-*`, and `check-integrity`,
**not** a parallel `search/get/list` application surface."* Its Consequences
(`:55`) bind: *"Future application query verbs … require this ADR to be re-opened."*

An op-store read-back is **operator diagnostic over operator/log data**:

- It reads `operational_mutations` — the **mutation log**, an internal
  operator/log table — not `canonical_nodes` (the application content store).
- It is the same family as the already-blessed `dump-row-counts` / `dump-schema` /
  `trace` diagnostics: an operator inspecting the store's own bookkeeping. The verb
  name stays in the `dump-*` family precisely to mark it as a diagnostic dump.
- It exposes **no query language** — no predicate, no filter DSL, no ranking, no
  raw SQL. The only knobs are a collection name, an opaque `id` cursor, and a page
  limit. This is a *paginated dump of a log*, structurally identical to reading a
  WAL or an audit trail, not an application query.
- The SDK already owns the *application* read surface (`read.*`, Slice 30). This
  CLI verb is the **operator** mirror of the *same engine seam*; it is deliberately
  CLI-only with **no SDK-parity obligation** (the gap wording says so explicitly).

**Conclusion:** `dump-mutations` belongs under **`doctor`** (read-only,
bit-preserving — never a `recover` action), is **CLI-only**, and is pre-authorized
by the reserved-gap wording. It does **not** re-open Option B: it adds no
`search`/`get`/`list` application-query surface over `canonical_nodes`. The
analysis does **not** read as an application-query expansion, so no escalation is
required. (The diagnostic-not-query framing was pre-signed by the operator for
this slice.)

**ADR amendment to land (§3.4 / `ADR-0.6.0-cli-scope`):** an in-place amendment
note dated 2026-06-06 on the Status line + a Consequences bullet, scoping the
0.8.0 op-store diagnostic read-back **IN** under `doctor` as a `dump-*`
diagnostic. Option B stays rejected; application query verbs still require a
re-open.

## (b) Verb shape

```text
fathomdb doctor dump-mutations <collection> [--after-id <n>] [--limit <n>] [--json] <db_path>
```

- `<collection>` — positional, required. The `append_only_log` collection whose
  appended rows are read back.
- `<db_path>` — positional, required (second positional, mirroring every other
  doctor verb's trailing `<db_path>`).
- `--after-id <i64>` — optional exclusive cursor (`WHERE id > ?`). Passed straight
  to the engine seam, which normalizes a negative value to the start of the log and
  yields an empty page for a value past the last id.
- `--limit <usize>` — optional page size. **Default 1000** when omitted: a sane
  operator page that bounds output without paging one row at a time. The engine
  clamps the effective SQL `LIMIT` to the ~1M cap (`READ_COLLECTION_MAX_LIMIT`), so
  the CLI never issues an unbounded read; the default keeps a casual invocation
  from dumping up to a million rows by surprise.
  - **`--limit` clamp (effective limit).** The CLI also clamps `--limit` to the
    *same* ~1M cap via `effective_dump_limit` (CLI-side mirror
    `DUMP_MUTATIONS_MAX_LIMIT`) **before** the read and the `next_after_id` decision.
    The engine clamps the SQL `LIMIT` but returns at most ~1M rows; if the CLI
    compared `rows.len()` to an *un-clamped* `--limit` above the cap, a full capped
    page (`rows.len() == cap < requested`) would look exhausted → `next_after_id:
    null` → pagination would stop while rows remain. Clamping both sides to the same
    effective limit avoids this; the clamp is pure + unit-pinned in `tests/parser.rs`
    (no >1M-row seeding needed).
- `--json` — accepted on every verb per `cli.md § Output posture`. Consistent with
  every existing doctor verb (and AC-038), the verb emits the JSON envelope on
  every invocation; `--json` is the pinned machine contract and **no second machine
  schema** is introduced. (We deliberately do not add a divergent human formatter —
  that would be a second schema; the one JSON object is the human-readable dump.)

Verb name `dump-mutations` chosen over `dump-collection`/`read-collection`: it
stays in the `dump-*` diagnostic family and avoids appearing to mirror the SDK
`read.collection` *application* verb.

## (c) `--json` envelope (doctor wrap pattern)

```json
{
  "verb": "dump-mutations",
  "collection": "events",
  "after_id": null,
  "limit": 1000,
  "count": 2,
  "rows": [
    { "id": 1, "collection": "events", "record_key": "k0", "op_kind": "append",
      "payload": "{\"n\":0}", "schema_id": null, "write_cursor": 1 },
    { "id": 2, "collection": "events", "record_key": "k1", "op_kind": "append",
      "payload": "{\"n\":1}", "schema_id": null, "write_cursor": 2 }
  ],
  "next_after_id": null
}
```

- `collection` / `after_id` (effective, or `null`) / `limit` (effective) echo the
  request.
- `rows` mirrors `OpStoreRow` field-for-field (serde `snake_case`):
  `{ id, collection, record_key, op_kind, payload, schema_id, write_cursor }`,
  in **`ORDER BY id`** (append) order as returned by the seam.
- `count` = `rows.len()`.
- `next_after_id` — pagination affordance: the **last row's `id` iff a full page
  was returned** (`rows.len() == effective_limit`), else `null`. A short or empty
  page means the log is exhausted at this cursor, so `next_after_id` is `null`; a
  full page hands back the cursor to resume with `--after-id <next_after_id>`. This
  guarantees no boundary overlap (the engine cursor is exclusive).

## (d) Exit codes

Reuse `run_doctor_verb` + the existing `engine_error_to_outcome` /
`engine_open_error_to_outcome` mapping — invent nothing.

| Outcome | Class | Code |
| ------- | ----- | ---- |
| success, including an **empty page** (empty / unknown / unregistered collection, or `after_id` past the end) | `Clean` | `0` |
| lock-held (`EngineOpenError::DatabaseLocked` / `EngineError::Closing`) | `LockHeld` | `71` |
| any other engine error | `Unrecoverable` | `70` |

An empty page is a **normal absence**, not a `Findings` (65) state — there is
nothing actionable about an empty or unknown log. Exit class set: `{0, 70, 71}`.

## (e) Why serialize INLINE (no `OpStoreRow` re-export)

The CLI depends only on the `fathomdb` facade, which **does not** re-export
`OpStoreRow` (verified: no `OpStoreRow` anywhere under `crates/fathomdb/src/`). The
rows are reachable as the *return type* of the seam (`Engine::read_mutations ->
Result<Vec<OpStoreRow>, _>`); their fields are `pub`. So the dispatch arm
serializes each row **inline** (`rows.iter().map(|r| json!({ "id": r.id, … }))`)
without ever naming the type. This keeps the facade public-**type** set unchanged —
no touch to the Slice-27 governed-surface allowlist, no `reexports.rs` /
`governed_surface.rs` ripple. Re-exporting `OpStoreRow` would be a shared-surface
expansion requiring escalation (§6); it is unnecessary and avoided.

## Test plan (RED first)

**`tests/parser.rs`** (parser surface):

- `dump-mutations <collection> <db_path>` parses; `--after-id`, `--limit`, `--json`
  parse; defaults are `after_id=None`, `limit=None`, `json=false`.
- `dump-mutations` rejects `--accept-data-loss` (owned by `recover`).

**`tests/operator_cli.rs`** (binary-level, real engine, no mocks; seed via
`fathomdb::PreparedWrite::{AdminSchema, OpStore}`):

- (i) `--json` emits one object `verb=="dump-mutations"`, `rows` ordered by `id`,
  carrying the `OpStoreRow` fields;
- (ii) `--limit k` caps `count` at `k`, `next_after_id` = page's last id;
- (iii) `--after-id <last>` returns the next page, no boundary overlap;
- (iv) unknown/empty collection → `rows:[]`, `count:0`, `next_after_id:null`, exit 0;
- (v) lock-held (holder engine open) → exit 71;
- (vi) `dump-mutations --help` exits 0.

Binds **F4-READ / reserved-gap-34** + these test names. **No new AC id**
(`acceptance.md` is locked).

## Non-goals (this slice)

No SDK / binding change; no engine / schema / Slice-33-index change; no
`read.get` node point-lookup CLI verb; no `recover` change; no raw-SQL / filter-DSL
escape hatch; no `--async` / concurrency knob; no facade public-type expansion; no
new AC id; no release / CI / version / tag action.
