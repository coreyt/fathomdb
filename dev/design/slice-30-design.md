# Slice 30 design memo — G2 `read.get`/`read.get_many` + G3 `read.collection`/`read.mutations` (`read.*`)

Status: design-first (consumed by the TDD RED→GREEN→refactor implementation).
Gates: G0 (Slice 15) + Slice 25 supersession sign-off both CLOSED on `main`.
Authoritative contract: `dev/plans/0.8.0-implementation.md` § "Slice 30";
governance: `dev/adr/ADR-0.8.0-supersede-five-verb-surface-cap.md` (read-verb
allowlist, `read.*` namespace B1, recovery denylist, typed/no-raw-SQL boundary);
substrate: `dev/adr/ADR-0.8.0-canonical-identity-substrate.md` (active =
`superseded_at IS NULL`, legacy rows carry `logical_id = NULL`).

This memo binds **gap labels G2/G3 + F2 + F4-READ + AC-074 + REQ-053** by the
TDD test names below. It does **not** mint a new AC/REQ/NEED id (acceptance.md is
`status: locked`).

## (a) ReaderResponse widening strategy — per-request typed channels (NO Search reshape)

The banner correction holds against disk: `ReaderResponse` (`lib.rs:423`) is
already `type ReaderResponse = rusqlite::Result<(u64, Option<SoftFallback>,
Vec<SearchHit>)>` and carries `Vec<SearchHit>`. The contract's "widen from
`Vec<String>`" framing is stale.

**Decision: do NOT reshape `ReaderResponse` or the `Search` arm.** Each new
reader request variant carries its OWN typed `respond: SyncSender<…>`, mirroring
the debug-only `LookasideStatus { respond: SyncSender<i32> }` /
`CacheStatus { respond: SyncSender<(String,i32,i32,i32)> }` precedent already in
the enum. This leaves the `Search` path byte-identical → the Slice 10
byte-identical-unfiltered Search pin (`pr_g10_filtered_knn.rs`) stays green by
construction (no edit to `read_search_in_tx`, `search_inner`, or the `Search`
variant).

New variants on `ReaderRequest` (`lib.rs:375`):

```rust
GetById {
    logical_ids: Vec<String>,
    respond: SyncSender<rusqlite::Result<Vec<Option<NodeRecord>>>>,
},
ReadCollection {
    collection: String,
    after_id: Option<i64>,
    limit: usize,
    respond: SyncSender<rusqlite::Result<Vec<OpStoreRow>>>,
},
```

- `GetById` returns `Vec<Option<NodeRecord>>` — one slot per requested
  `logical_id`, **request order preserved**, `None` where no active row carries
  that `logical_id` (not-found is a normal absence, see (d)).
- `ReadCollection` returns `Vec<OpStoreRow>` in `ORDER BY id`.

Both reader arms open the **DEFERRED reader tx** (`reader.transaction_with_behavior(
TransactionBehavior::Deferred)`), mirroring `read_search_in_tx` (`lib.rs:3657`).
They are dispatched through `reader_pool.dispatch(...)` exactly like
`search_inner` (`lib.rs:2122-2135`): a per-call `mpsc::sync_channel(1)`, send the
request, `recv()` the typed response. **Never `self.connection.lock()`** (the
writer path) on these reads.

## (b) read.* SDK signatures (identical structure across Py + TS)

Native row carriers (new public engine types):

```rust
pub struct NodeRecord {     // G2 active node row
    pub logical_id: String, // the queried id (echoed; always Some on a hit)
    pub kind: String,
    pub body: String,
    pub write_cursor: u64,  // interim id carrier (parity with SearchHit.id)
}
pub struct OpStoreRow {     // G3 op-store mutation row
    pub id: i64,
    pub collection: String,
    pub record_key: String,
    pub op_kind: String,
    pub payload: String,    // payload_json
    pub schema_id: Option<String>,
    pub write_cursor: u64,
}
```

Engine public methods (dispatch through `reader_pool`):

```rust
pub fn read_get(&self, logical_id: &str) -> Result<Option<NodeRecord>, EngineError>;            // delegates to read_get_many
pub fn read_get_many(&self, logical_ids: &[String]) -> Result<Vec<Option<NodeRecord>>, EngineError>;
pub fn read_collection(&self, collection: &str, after_id: Option<i64>, limit: usize)
    -> Result<Vec<OpStoreRow>, EngineError>;   // `read.collection`
pub fn read_mutations(&self, collection: &str, after_id: Option<i64>, limit: usize)
    -> Result<Vec<OpStoreRow>, EngineError>;   // `read.mutations` — alias surface over the same op-store read-back
```

`read.collection` and `read.mutations` are the two G3 verbs the allowlist names.
Both read `operational_mutations` for a `collection_name` with the SAME mandatory
limit + after-id cursor; they are distinct allowlist verbs (collection-oriented
vs mutation-log-oriented naming) over one in-tx reader fn `read_collection_in_tx`.
`limit` is a **required** parameter on every public path — there is no overload
that omits it, so no API path can yield an unbounded SELECT. The engine clamps
the effective SQL `LIMIT` to `min(limit, READ_COLLECTION_MAX_LIMIT)` where
`READ_COLLECTION_MAX_LIMIT = 1_000_000` (~1M cap). A caller-supplied `limit == 0`
returns an empty `Vec` without issuing a SELECT (no scan). The SQL is always
`... WHERE collection_name = ?1 AND id > ?after ORDER BY id LIMIT ?clamped`.

SDK arg shapes (structurally identical; house casing preserved):

| verb | Python | TypeScript |
| --- | --- | --- |
| `read.get` | `read.get(engine, logical_id: str) -> NodeRecord \| None` | `read.get(engine, logicalId: string): Promise<NodeRecord \| null>` |
| `read.get_many` | `read.get_many(engine, logical_ids: list[str]) -> list[NodeRecord \| None]` | `read.getMany(engine, logicalIds: string[]): Promise<(NodeRecord \| null)[]>` |
| `read.collection` | `read.collection(engine, collection: str, *, after_id: int \| None = None, limit: int) -> list[OpStoreRow]` | `read.collection(engine, collection: string, options: { afterId?: number; limit: number }): Promise<OpStoreRow[]>` |
| `read.mutations` | `read.mutations(engine, collection: str, *, after_id: int \| None = None, limit: int) -> list[OpStoreRow]` | `read.mutations(engine, collection: string, options: { afterId?: number; limit: number }): Promise<OpStoreRow[]>` |

`limit` is required in both bindings (Python: keyword-only **without** a default;
TS: a required `limit` field on the options object). The Python wrappers validate
`limit` is a positive int and raise `ValueError` on a missing/negative value
(client-side typed boundary; mirrors `admin.configure`'s `ValueError` guards).

## (c) get_many partial-vs-all-or-nothing

**Partial, order-preserved** (per the Slice 25 ADR non-goal note + this memo).
`read.get_many([a, b, c])` returns a 3-element list aligned to the request; a
missing/superseded id is `None`/`null` in its slot. No all-or-nothing failure.
`read.get` delegates to `read_get_many(&[id])` and returns slot 0.

## (d) not-found = None/null (NotFound class → reserved-gap 31)

A missing or superseded `logical_id` is a **normal result**, never an exception:
`read.get` → `None`/`null`; `read.get_many` → the `None` slot. No new typed
error class is introduced (a `NotFound` class would have to land identically in
BOTH bindings — that is reserved-gap Slice 31). Genuine read failures (storage,
closing) surface through the EXISTING typed hierarchy (`engine_error_to_py` /
`engine_error_to_napi`), unchanged.

## (e) op-store read mapping

`operational_mutations(id PK AUTOINCREMENT, collection_name, record_key,
op_kind CHECK('append'), payload_json, schema_id, write_cursor)` (migration 004).
`read_collection_in_tx`:

```sql
SELECT id, collection_name, record_key, op_kind, payload_json, schema_id, write_cursor
FROM operational_mutations
WHERE collection_name = ?1 AND id > ?2   -- ?2 = after_id (or 0 when None)
ORDER BY id
LIMIT ?3                                  -- ?3 = clamped mandatory limit
```

→ `OpStoreRow { id, collection, record_key, op_kind, payload, schema_id, write_cursor }`.
The `_for_test` SELECTs (`lib.rs:2436/2456/2470`) are a **shape oracle only**;
this is a new statement, not a promotion of those.

G2 `read_get_by_id_in_tx` mirrors the `:4170` canonical projection columns plus
`logical_id`, active-only:

```sql
SELECT logical_id, kind, body, write_cursor
FROM canonical_nodes
WHERE logical_id IN (...placeholders...) AND superseded_at IS NULL
```

Returned rows are re-ordered into request order by a `logical_id → NodeRecord`
map (a `logical_id` may appear once active; duplicates in the request echo the
same hit). `superseded_at IS NULL` is the active-only default → superseded
versions are never returned.

## Conformance — genuinely enforced (NOT vacuous; §3.4)

The two surface suites already introspect REAL symbols (`dir(Engine)` minus an
exclusion set; `admin.__all__`). The `read.*` flip mirrors that exactly:

- Python `read.py` exposes `__all__ = ["get","get_many","collection","mutations"]`.
  `_live_python_command_surface()` iterates `read.__all__` and emits
  `read.<verb>` for each callable — identical mechanism to the `admin.__all__`
  loop. Snake_case verbs match the dotted allowlist names verbatim.
- TS `read.ts` exports a `read` object with `get/getMany/collection/mutations`.
  `liveTsCommandSurface()` introspects the `read` object's own function keys and
  normalizes camelCase → snake_case (`getMany` → `get_many`) to emit
  `read.<snake>`, matching the single shared allowlist. The normalization is a
  small documented transform; it is the ONLY place TS verb identity maps to the
  canonical allowlist name, so a one-sided extra/missing verb still fails parity.

Falsifiability demonstrated in RED:

- A hypothetical extra live `read.delete` (or `Engine.delete`) enters `live` and
  fails the subset-of-allowlist check (P1).
- A `read.*` verb present in one binding but missing from the other fails the
  now-live presence assertion (each of the four must be live in both suites).
- The allowlist stays single-source (`governed-surface-allowlist.json`); the 9
  members are unchanged — the verbs move from documented-only to **live-asserted**.

The honest-green "not live until Slice 30" comments in `test_surface.py`,
`surface.test.ts`, and the JSON `_comment` are corrected to "live as of Slice 30".

## Test plan (TDD)

RED (engine + conformance), committed first:

- `tests/pr_g2_get_by_id.rs` — active-only by `logical_id`; superseded NOT
  returned; request order preserved; missing id → `None` slot (not an error);
  reads ride the reader pool (concurrent read while a writer tx is open).
- `tests/pr_g3_read_collection.rs` — rows for a `collection_name` ORDER BY id;
  mandatory limit (no unbounded path); after-id cursor paginates across the
  boundary; ~1M clamp.
- Conformance flip: `test_surface.py` + `surface.test.ts` introspect `read.*`
  live; add the now-live presence assertions.

GREEN: engine (variants + reader arms + in-tx fns + public methods) → PyO3 +
napi bindings (four verbs each + `.pyi`/`binding.ts` types) → SDK wrappers
(`read.py` / `read.ts` + exports) → conformance introspection.

X1: `test_functional_retrieve.py` + `functional-retrieve.test.ts` (shared
`functional_retrieve_fixture.json`): write nodes (with logical_ids) + register an
append_only_log collection + append op-store rows → `read.get`/`read.get_many`
return by id → `read.collection`/`read.mutations` honor cursor/limit →
`admin.configure` exercised → cross-binding equivalence (Py ≡ TS per verb).

X3: docs (`python-api.md`, `typescript-api.md`, `op-store.md`, a retrieve guide,
`architecture.md`, `requirements.md`/`traceability.md` referencing existing ids,
`DOC-INDEX.md`). X2: `mkdocs build`.

Reserved-gap candidates surfaced (not built): 31 (typed `NotFound` class),
32 (cursor/limit hardening under ~1M load), 33 (CLI op-store read-back),
27 (Rust-facade allowlist pin — Q5=BIND-RUST).
