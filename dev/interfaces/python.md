---
title: Python Public Interface
date: 2026-07-29
target_release: 0.8.21
desc: Public Python surface for 0.8.21
blast_radius: src/python/; design/bindings.md; design/errors.md; design/lifecycle.md; design/engine.md
status: locked
---

# Python Interface

This file owns Python-visible symbol spelling and attribute casing.
Cross-binding parity remains owned by `design/bindings.md`.

## Runtime surface

The **core** runtime verbs available to Python callers are:

- `Engine.open(...)`
- `engine.write(...)`
- `engine.search(...)`
- `engine.close()`
- `admin.configure(...)`

The full governed set is pinned by
`src/conformance/governed-surface-allowlist.json`, which `test_surface.py`
loads: the core five plus `engine.search_text_only`, `engine.embed`,
`rerank`, the `read.*` namespace (`get`, `get_many`, `collection`,
`mutations`, `list`, `crossed_boundary_since`, `projections`), the `graph.*`
namespace (`neighbors`, `search_expand`), the BYO-LLM verbs
(`engine.ingest_with_extractor`, `engine.consolidate_with_provider`),
`engine.configure_projections`, and the lifecycle/erasure verbs below.

### Lifecycle + erasure verbs (0.8.19 Slice 10 / 0.8.20 Slice 5d)

Governed and HITL-SIGNED; **not** recovery verbs (none carries a REQ-054
denylist name).

- `engine.transition(logical_id: str, to_state: str, reason: str | None = None)
  -> None` — move a GOVERNED node between existence states per the
  engine-enforced legal-transition table (`pending→active` promote,
  `pending→deleted` REJECTED, `active→deleted` soft-delete, `deleted→active`
  undelete). Promote/undelete CLEAR `reason`; reject/soft-delete SET it.
  `reason` is advisory and never engine-interpreted. Keys on the bare
  `logical_id` — the `l:` id space ONLY; any other raises
  `NotLifecycleAddressableError`. An illegal move raises
  `IllegalTransitionError` with `from_state` / `to_state` / `legal` (never
  `from`, which is a Python keyword — parity-safe naming, S7).
- `engine.purge(logical_id: str) -> None` — irreversible hard-erase of a
  governed node across every row-owned target. DELETED-FIRST and IDEMPOTENT.
  **No restore counterpart exists on any surface.**
- `engine.erase_source(source_id: str) -> EraseReport` — erase every canonical
  row carrying `source_id` plus its row-owned projections, then finish at rest
  (telemetry redaction + WAL truncation). The COMPANION to `purge`:
  `erase_source` reaches ANONYMOUS rows (no `logical_id`) that `purge` cannot.
  Idempotent. Raises `WriteValidationError` for an empty / whitespace-only /
  reserved (`_`-prefixed) id, and `ErasureIncompleteError` (with `stage` /
  `detail`) rather than reporting success when the at-rest step did not
  complete. `EraseReport` carries `source_ref`, `nodes_excised`,
  `edges_excised`, `projections_invalidated`.

`Engine.open(...)` returns the engine handle. The structured open report owned
by `design/engine.md` is accessible after open via `engine.open_report()` (see
Engine-attached instrumentation / control below).

### Module-level CLS batch embedding (0.8.20 Slice 40)

- `embed_batch_cls(texts: list[str]) -> list[list[float]]` — batch-embed
  `texts` with the pinned BGE-small default embedder using **CLS pooling** and
  L2 normalization. The output has one vector per input in input order; `[]`
  returns `[]` without loading weights. This is deliberately distinct from
  `engine.embed(text)`, whose engine read path uses Mean pooling. It is the
  snake_case peer of TypeScript's `embedBatchCls`; both reject invalid FFI
  strings and raise `EmbedderNotConfiguredError` when built without the default
  embedder.

`Engine.open(path, *, config=None, **engine_config)` accepts the
engine-owned knobs from `design/engine.md` in snake_case:

- `embedder_pool_size`
- `scheduler_runtime_threads`
- `provenance_row_cap`
- `embedder_call_timeout_ms`
- `slow_threshold_ms`

The keyword form and `EngineConfig` object form are equivalent. Python
executor usage remains caller-owned and is not an engine config field.

## Engine-attached instrumentation / control

These are public instance methods, not extra top-level SDK verbs:

- `engine.open_report()`
- `engine.drain(timeout_s=...)`
- `engine.counters()`
- `engine.set_profiling(enabled=...)`
- `engine.set_slow_threshold_ms(value=...)`

Subscriber attachment is provided by:

- `engine.attach_logging_subscriber(logger, *, heartbeat_interval_ms=None)`

The helper maps engine events into Python `logging.LogRecord`s with the stable
`fathomdb` payload described by `design/bindings.md`.

## Caller-visible data shapes

- `WriteReceipt.cursor`, `WriteReceipt.row_cursors`,
  `WriteReceipt.dangling_edge_endpoints`
- `SearchResult.projection_cursor`
- `SearchResult.soft_fallback.branch`

`soft_fallback.branch` uses the typed values owned by `design/retrieval.md`.

### `SearchHit.id` is `IdSpace` (C-2, 0.8.19 / TC-8)

`SearchHit.id` is **`fathomdb.IdSpace`**, not the pre-0.8.19 `int`
`write_cursor`. This is the largest consumer-visible break in the 0.8.9 →
0.8.20 span and is part of the HITL-SIGNED 0.8.19 Slice-10 delta.

- `IdSpace` has two attributes: `space` (`"logical"` | `"content"` |
  `"passage"`) and `value` (the BARE, prefix-stripped id). The prefixed form
  `f"{prefix}{value}"` (`l:` / `h:` / `p:`) reproduces the pre-swap `stable_id`
  byte-for-byte.
- It is the **PERMANENT** caller-facing identity, not an interim carrier: the
  older "interim `write_cursor`, swaps to `logical_id` later" framing is
  superseded and must not be restated.
- The engine's positional `write_cursor` is **not surfaced** as a hit id.
  `NodeRecord.write_cursor` still exists but is engine-internal book-keeping,
  reassigned on re-projection, and is NOT the same carrier.
- Only `space == "logical"` is lifecycle-addressable by `transition` / `purge`.

`SearchHit.source_id` (`str | None`) carries the hit's source-document
provenance — the identifier `engine.erase_source` consumes — and since TC-31
(0.8.20) it is populated on EVERY hit path, not just the graph arm.
`SearchHit.ce_score` (`float | None`) is the CE score for in-pool reranked hits.

### `source_id` is MANDATORY on canonical write items (0.8.20 Slice 5c, R-20-E3)

Every **node** and **edge** item passed to `engine.write` must carry a
`"source_id"` key (`str`). This is not tidiness: `erase_source` addresses rows
BY `source_id`, so a row written without one is reachable by no erasure call.

Rust makes the absence inexpressible through the `SourceId` newtype; Python has
no such type at the boundary, so `dict_source_id_required` raises
**`WriteValidationError`** for a missing, empty, whitespace-only or **reserved**
(`_`-prefixed) id. That is the Python arm of "an un-provenanced write does not
compile / raises".

```python
engine.write([
    {"kind": "note", "body": "…", "source_id": "doc-42"},
    {"kind": "mentions", "from": "a", "to": "b", "source_id": "doc-42"},
])
```

The engine's reserved namespace (`_engine:*` and the `_legacy:pre-0.8.20`
migration cohort) is refused here and is reachable only through
`fathomdb recover --excise-source`.

**Policy: `source_id` MUST NOT contain personal data.** It is echoed on every
`SearchHit` and recorded in the retention-EXEMPT erasure-audit row, so it
outlives the rows it names. Use an opaque document or tenant id.

## Node write-item validity window (0.8.20 Slice 15b, TC-34)

`engine.write([...])` takes loose mappings, not typed structs. A **node** item
accepts two optional validity keys, snake_case per this file's casing rule:

- `valid_from` — `int | None`, INCLUSIVE lower bound, INTEGER epoch **seconds**
  UTC. Omitted or `None` lands SQL NULL = unbounded below.
- `valid_until` — `int | None`, EXCLUSIVE upper bound, same units. Omitted or
  `None` lands SQL NULL = unbounded above.

```python
engine.write([
    {
        "kind": "note",
        "body": "…",
        "source_id": "s1",
        "valid_from": 1_700_000_000,
        "valid_until": 1_700_003_600,
    },
])
```

The window is **half-open** `[valid_from, valid_until)`: an instant equal to
`valid_from` is IN, an instant equal to `valid_until` is OUT.

**Omitting both keys preserves existing default-view visibility.** The pair
binds NULL/NULL — exactly what every pre-slice row already carries — so an
unchanged caller sees unchanged behaviour.

Refusals (the rule is enforced in the engine's `validate_write`, so it is
identical across Rust / Python / TypeScript and cannot drift):

- Both bounds present with `valid_from >= valid_until` is an UNSATISFIABLE
  window and raises **`WriteValidationError`**. Validation runs before any
  insert, so the **whole batch** is rejected.

  > **BREAKING (0.8.20 Slice 22, decision #18).** This raised
  > `InvalidArgumentError` **with both bounds in the message**, while a
  > non-integer bound from the same call raised `WriteValidationError`. Both are
  > now `WriteValidationError` — one family for the whole write-validation
  > boundary (`dev/design/errors.md`, 2026-07-28 amendment). **The message is now
  > the fixed string `"write validation error"`; the bounds are gone.** A caller
  > that read them out of the message must validate the pair before calling.
  > `InvalidArgumentError` is unchanged for every other use (e.g. traversal
  > `depth`, projection-spec rejections, `ReadView` misuse).
- A **one-sided** window is never refused, however extreme its single bound.
- A non-integer bound raises `WriteValidationError`; the value is never coerced.
  `bool` is rejected **explicitly** — it subclasses `int`, so `True` must not be
  silently taken as the instant `1`.

These are keys on an existing verb, not a new verb: the runtime-verb surface
above is unchanged. The fields-only delta is **HITL-SIGNED 2026-07-29 (steward
`seq-157`)**.

## Edge temporal fields (0.8.20 Slice 15c, TC-33)

An **edge** item accepts two optional temporal keys. As of TC-33
(HITL-RATIFIED 2026-07-21) these are **INTEGER epoch seconds (UTC)**, the same
representation as the node validity window above and as storage — NOT ISO-8601
strings:

- `t_valid` — `int | None`, event valid-time. `None` = unknown / still valid.
- `t_invalid` — `int | None`, event invalid-time. `None` = **still valid**.

```python
engine.write([
    {
        "kind": "works_for",
        "from": "bob",
        "to": "acme",
        "source_id": "s1",
        "t_valid": 1_546_300_800,   # 2019-01-01T00:00:00Z
        "t_invalid": None,          # still valid
    },
])
```

`None`/omitted is the ONLY way to say "unknown"; it lands SQL NULL, which reads
as **still valid**. A non-integer bound raises `WriteValidationError` and is
never coerced (`bool` rejected explicitly, as for the node window) — the same
`dict_epoch_seconds` validator serves both axes.

> ⚠ **Sign-off status of the TC-33 edge-temporal delta is UNRESOLVED — do not
> assert either way.** The allowlist `_comment` mentions `t_valid`/`t_invalid`
> only as the **Slice-30 `PreparedWrite::Edge` PRECEDENT** cited inside the
> Slice-15b (node-validity) paragraph. There is **no TC-33 sign-off recorded**
> anywhere in that file, so a mechanical grep that reports these literals as
> "signed" is reading the precedent, not a signature. Flagged for a
> Steward/HITL ruling by 0.8.20 Slice 39 (`R-20-DOC`); the marker below is left
> exactly as the implementing slice wrote it.

**Layering note.** This is the GOVERNED SDK write surface. ISO-8601 survives
ONLY on the **BYO-LLM extractor wire** (`fathomdb.extract.v1`), where the engine
normalises each timestamp to epoch seconds with a HARD REJECTION of any value
`strftime('%s', ?)` cannot parse — an unparseable timestamp must never coerce to
NULL, because a NULL `t_invalid` reads as "still valid" and would resurrect an
invalidated edge. Fields-only delta, **PROPOSED, NOT SIGNED**.

## Projection registry (0.8.20 Slice 15d, R-20-PR / C-1)

Two net-new governed verbs declare and inspect projections over interpretive
attributes. The verbs, the `ProjectionSpec` / `ProjectionRole` /
`ProjectionDelta` types and the typed `ProjectionDestructiveError` are
**HITL-SIGNED 2026-07-29 (steward `seq-157`)**. ⚠ The Slice-20 (R-20-DR)
readiness field `vector_dense_readiness` is **NOT** part of that `seq-157`
signature. Its closed `DenseReadiness` vocabulary is separately
**HITL-SIGNED 2026-08-07 (steward `seq-246`)** by Slice 21 F5/C1, with a
governed-surface signature. The static record makes its three spellings typed
and accept-inert; the later runtime GREEN must still select `"unavailable"`
when no usable dense runtime exists.

- `engine.configure_projections(specs, drop=None)` → `ProjectionDelta`.
  Declarative, idempotent apply: the engine diffs `specs` against the durable
  registry and backfills the difference in one transaction. `drop` is EXPLICIT —
  omitting a live projection from `specs` does NOT drop it; removal requires
  naming it in `drop`. A destructive change (a role removal or a
  tokenizer/embedder change) without a drop raises `ProjectionDestructiveError`
  (`name`/`delta` attributes). Re-applying an unchanged spec returns
  `ProjectionDelta(unchanged=True)`.
- `read.projections(engine)` → `list[ProjectionSpec]`, sorted by name — the
  registry introspection (folded into `read.*`).

`ProjectionSpec` (`fathomdb.types.ProjectionSpec`) is
`{ name, roles: frozenset[str], fts, fts_tokenizer, vector, vector_embedder,
vector_dense_readiness }`.
`ProjectionRole` (`fathomdb.types.ProjectionRole`) has exactly three members —
`FILTERABLE`, `RANKABLE`, `SEARCHABLE`; `searchable→FTS` and `searchable→vector`
are tier labels carried by the `fts`/`vector` sub-object flags, not roles. Cheap
roles (`filterable`, `searchable→FTS`) build same-transaction; `rankable` and the
`searchable→vector` sub-target are persisted-but-deferred (reported in
`ProjectionDelta.deferred`). `ProjectionDelta` is
`{ built, dropped, deferred, unchanged, vector_unsupported_kinds }`.

### `fts` / `vector` require the `searchable` role (0.8.20 Slice 23, R-20-SV)

⚠ **BREAKING.** `engine.configure_projections` REFUSES a `ProjectionSpec` with
`fts=True` or `vector=True` whose `roles` does not contain
`ProjectionRole.SEARCHABLE`, raising **`fathomdb.errors.WriteValidationError`**
(message `"write validation error"` — the variant is message-less).

- **Why.** `searchable→FTS` / `searchable→vector` are tier labels: the sub-object
  flags SELECT a sub-target of `searchable` and do not CONFER it, so without the
  role the declaration builds, embeds and enrols nothing. HITL ruling
  2026-07-24 (`dev/plans/plan-0.8.20.md` §11 item 4, option (b) REJECT). The
  family is `WriteValidationError` per decision #18 — a malformed submitted SHAPE
  is one family (`dev/design/errors.md`).
- **This SUPERSEDES the Slice 15d fix-4 accept-and-round-trip position** for this
  shape, and with it the "accept-inert" precedent cited under
  `vector_dense_readiness` below.
- **Keyed on the ABSENCE of `searchable` alone** — `filterable` / `rankable` are
  orthogonal and neither substitutes for it.
- **A rejected request is a TOTAL no-op**: validation runs before any write, so
  one invalid spec anywhere in `specs` aborts the whole call — valid siblings are
  not registered and `drop` entries do not apply.
- **`read.projections(engine)` is UNAFFECTED** — a pure read that rejects nothing.
- **LEGACY databases.** Rows declared in this shape while the engine accepted it
  still read back verbatim, but `read.projections` output can no longer be fed
  straight back into `configure_projections` for them: re-applying raises, and
  re-declaring only the valid half raises `ProjectionDestructiveError` (it removes
  the stored sub-object). Remedies: add `ProjectionRole.SEARCHABLE`, or name the
  projection in `drop`.
- **Known diagnostic cost (TC-95/TC-98, HITL-deferred).** `WriteValidationError`
  carries no payload, so with a LIST of specs it cannot name WHICH one was
  invalid.

### `vector_unsupported_kinds` (0.8.20 Slice 22, R-20-VC / TC-67)

`ProjectionDelta.vector_unsupported_kinds` is a `list[str]` of node **kinds** —
not attribute names. The first three lists carry projection attribute names; this
one carries the vector-eligible node kinds present in the corpus that the vector
writer can **never** commit, so no `searchable→vector` declaration will ever
produce an embedding for them.

**What it means for your data.** Rows of a reported kind still get **FTS and
lexical search**; they will simply never participate in dense/vector retrieval —
in this session or any future one. Your options are to use one of the kinds the
engine's locked vocabulary maps, or to accept lexical-only retrieval for those
rows. Waiting is not one of them, which is exactly what the field exists to say:
before it, "no embedder attached this session" (transient) and "this kind will
never be embedded" (permanent) both arrived as the same `deferred` entry.

- **Sorted, de-duplicated, and empty rather than absent** — read it
  unconditionally.
- **A STATE report, not a diff.** It does not feed `unchanged`, so an idempotent
  re-apply returns `ProjectionDelta(unchanged=True)` with `built`/`dropped`/
  `deferred` empty **and this list populated**.
- **Embedder-independent.** Identical whether or not the engine was opened with
  an embedder — the vocabulary is static, so the fact does not depend on the
  session. Do not read it as the deferral.
- **Output-only.** `configure_projections` accepts specs, never a delta, so this
  field has no inbound direction and cannot affect the `read.projections` →
  `configure_projections` round-trip.
- **Residual — computed at DECLARE time.** A non-committable kind written *after*
  the call is not in a delta you already hold. To refresh, re-apply the same spec:
  an idempotent no-op that returns a current report.
- **Not an error, not a readiness change.** Nothing is rejected and
  `vector_dense_readiness` still reaches `"ready"` — an un-enrolled kind is not
  outstanding work.

### `vector_dense_readiness` (0.8.20 Slice 20, R-20-DR)

`ProjectionSpec.vector_dense_readiness` is **engine-set READ METADATA**, hung off
the `vector` sub-object. It is `None` on every caller-authored spec and is
populated only on the way OUT of `read.projections(engine)` — and only for a spec
that declares `vector=True`. `filterable` and `searchable→FTS` are
same-transaction (non-stale on commit) so they have no readiness axis at all;
`searchable→vector` is async and rebuild-durable, so it carries one.

- **Exactly three spellings: `"unavailable"`, `"embedding"`, and `"ready"`.**
  Their signed target meanings are no usable dense runtime (absent or
  equivalence-refused), usable runtime with eligible outstanding work, and
  usable quiescent work, respectively. `"pending"` is
  DELIBERATELY not one of them — that token is RESERVED for the orthogonal
  **admission** axis (quarantine/trust, an app judgment). Do not reuse the word.
- **Static F5 record; runtime GREEN still pending.** The third spelling is
  public, typed, and accepted inertly on input, but current
  `read.projections` still selects only `"embedding"` / `"ready"` from
  outstanding work. It does **not** yet select `"unavailable"`; no-runtime →
  `"unavailable"` remains required later Slice 21 behavior. That runtime change
  must derive the state without a schema step or stored readiness field.
- **Accept-inert on the way in.** Passing `vector_dense_readiness` to
  `engine.configure_projections` neither stores nor changes anything: it is not
  part of the declaration and the engine always reports the derived truth. That
  is deliberate, so `read.projections` output stays feedable straight back into
  `configure_projections` as a no-op (`ProjectionDelta(unchanged=True)`).
  **⚠ 0.8.20 Slice 23 correction:** this accept-inert rule used to be justified by
  analogy with the `fts`/`vector`-without-`searchable` shape. That analogy is
  **OVERRULED** — that shape is now rejected (see above). `vector_dense_readiness`
  accept-inert is unchanged and stands on its own: it is engine-set READ METADATA,
  never part of a declaration.
- **Two shapes are still hard-rejected**, because they could never round-trip:
  a readiness supplied with `vector=False`, and any spelling outside
  `{"unavailable", "embedding", "ready"}` (including `"pending"`, `""`, and
  `"Ready"`). Both raise the EXISTING `InvalidArgumentError` — **no new error
  type is minted**.
  `None` is always accepted.
- **Additive.** A caller who never reads the field sees identical behaviour, and
  the slice adds ZERO net-new governed commands.

### `engine.drain()` is the flush-to-readiness barrier (0.8.20 Slice 20c, R-20-DR)

There is **no `flush_embeddings()` verb**. The shipped
`engine.drain(timeout_s=...)` — note **SECONDS** here, milliseconds in
TypeScript — carries those semantics, so the surface gains ZERO net-new governed
commands. The pinned invariant, tested in Rust, Python and TypeScript:

> With a usable dense runtime, `drain()` returning normally ⟹
> `vector_dense_readiness == "ready"`, **and every vector-eligible row has its
> vector row at rest.**

- **`drain` is a BARRIER, not a trigger.** It waits for the engine's projection
  runtime to go quiescent; it never schedules or wakes anything. Deferred/backfill
  work is enqueued on the **enqueue side** instead: `engine.configure_projections`
  enrols the vector kinds and re-opens the stranded rows before returning, so the
  very next `drain()` flushes them. Turning the dense arm on over an existing
  corpus is therefore just:

  ```python
  configure_projections(engine, [ProjectionSpec(name="summary",
                                               roles=["searchable"],
                                               vector=True)])
  engine.drain(timeout_s=60)          # flush the backfill
  assert read_projections(engine)[0].vector_dense_readiness == "ready"
  ```

- **Ordering does not matter.** Write-then-declare and declare-then-write behave
  identically. The write path performs the **same** backfill the declaration
  does, so rows of that kind written by an earlier session — for instance one
  opened with `use_default_embedder=False`, where the declaration persisted but
  deferred — are picked up too, rather than being left behind a `"ready"` that is
  not true of them.
- **The dense arm covers only the engine's locked `kind` vocabulary.** A
  `searchable→vector` declaration turns the dense arm on for node kinds in
  `{email, article, paper, meeting, note, todo, doc}`. Rows of ANY other `kind`
  are accepted and stay lexically searchable, but get **no vector** and are not
  counted as outstanding work, so readiness still reaches `"ready"`. This is
  **not** an error condition: `engine.write` does not reject them, no exception is
  raised, and there is no verb to ask about it.
- **Idempotent.** Re-applying an already-satisfied declaration re-embeds nothing
  and returns `ProjectionDelta(unchanged=True)`.
- **Dropping the last `searchable→vector` declaration turns the dense arm back
  off.** `engine.configure_projections([], drop=["summary"])` un-enrols the node
  kinds that declaration enrolled, so later writes enqueue no embed and `drain()`
  no longer waits on them. It **deletes no embedding** — vectors already at rest
  survive the drop, exactly as they always have. Re-declaring re-enrols and
  backfills, so a row written while the arm was off is picked up, not stranded.
  Edge-body vectors are unaffected.
- **The dense arm requires the `searchable` ROLE, not merely `vector=True`**
  (0.8.20 Slice 21c, `TC-71`). A spec such as
  `{"name": "summary", "roles": ["filterable"], "vector": True}` is **REJECTED
  since 0.8.20 Slice 23** (`R-20-SV`, above); until then it was accepted and
  round-tripped verbatim while being **INERT**: it enrolled no kind, backfilled
  nothing, and made no later write enqueue an embedding. That inertness still
  governs the LEGACY population holding the shape at rest. Previously the engine
  keyed the
  dense arm off the stored `vector` sub-object alone, so declaring that
  combination against a session with an embedder silently embedded the whole
  corpus. The inverse moves with it: demoting the last `searchable→vector`
  projection to `filterable + vector`, or dropping it while an inert
  `filterable + vector` sibling survives, now un-enrols exactly as a literal drop
  does. The name is still reported in `deferred`.
- **Graceful-absent without a live embedder:** the declaration persists and
  defers, then grafts on when re-applied in a session that has one.
- **…but graceful-absent stops at the enrolment boundary** (fix-4). Once a kind
  IS enrolled — i.e. some earlier session DID have an embedder — writing that
  kind from a session opened with `use_default_embedder=False` leaves real dense
  work outstanding, and this session cannot satisfy it. The write is **accepted**
  and stays lexically searchable, but `vector_dense_readiness` reads
  `"embedding"` and `drain` raises `SchedulerError` for the rest of that session,
  however long you wait. It is **not** lost: no failure is recorded and no
  terminal is written, so the next session opened WITH an embedder embeds it
  through the ordinary scheduler — no re-apply, no operator `rebuild`. Expect the
  timeout there and do not read it as data loss.
- **`drain` stays bounded** and raises the existing timeout error rather than
  blocking; size `timeout_s` for the backfill you just asked for.

## Errors

Python exposes one catch-all base class, `EngineError`, plus one concrete
subclass per canonical row in `design/errors.md` — **27** of them as of 0.8.20,
1:1 with the TypeScript set below `FathomDbError`.

Examples of caller-visible subclasses:

- `DatabaseLockedError`
- `CorruptionError`
- `MigrationError`
- `IncompatibleSchemaVersionError`
- `EmbedderIdentityMismatchError`
- `EmbedderDimensionMismatchError`
- `SchemaValidationError`
- `OverloadedError`
- `ClosingError`

The 0.8.19/0.8.20 additions, all of which the governed verbs above can raise:

- `IllegalTransitionError` (`from_state`, `to_state`, `legal`)
- `NotLifecycleAddressableError` (`id_space`)
- `ErasureIncompleteError` (`stage`, `detail`) — note the `Error` suffix; the
  class is NOT spelled `ErasureIncomplete`
- `ProjectionDestructiveError` (`name`, `delta`)
- `VectorEquivalenceMismatchError` (`reason`)
- `ConsolidatorError`

Payload fields remain typed attributes; callers do not dispatch on message
text. ⚠ `WriteValidationError` is **message-less** since decision #18 — its
message is the fixed string `"write validation error"` and it carries no
payload, so the offending value is not recoverable from the error.

## Default embedder

`Engine.open(path, use_default_embedder=True)` opts into the engine's
default embedder (`fathomdb-bge-small-en-v1.5`). On first use, weights
are downloaded from HuggingFace and cached under
`~/.cache/fathomdb/embedders/`; subsequent opens hit the warm cache. See
`dev/adr/ADR-0.7.1-default-embedder-weight-fetch.md` for the network-
surface scope (opt-in only; sha256-verified; visible via
`OpenReport.embedder_events`). The default (`use_default_embedder=False`)
opens without an embedder; subsequent vector writes fail with
`EmbedderNotConfiguredError`.

### `dense_disabled` and the cached equivalence verdict (0.8.20 Slice 22, TC-68)

`OpenReport.dense_disabled` / `engine.dense_disabled()` still mean "the dense arm
is refusing", and the typed query-time `VectorEquivalenceMismatchError` and the
FTS-only fallback are unchanged. **What changed is when the check behind them
runs.** The 0.8.18 vector-equivalence self-check used to re-embed its 45 probes on
*every* open; since 0.8.20 the engine caches that verdict against a fingerprint of
the embedder identity, the pinned mean vector, the probe fixture, the divergence
floors and the stored reference baseline. An open whose fingerprint is unchanged
does **zero** probe embeds — the dominant cost of opening a vector-indexed
workspace with a live embedder — and reuses the previous verdict.

Read `dense_disabled` accordingly: it reports the arm's status **as verified at
the last open whose fingerprint differed**, not a fresh re-verification at this
open. A backend that drifts without changing its declared identity (the same
model moved between CPU and GPU, or rebuilt against a new library) is therefore no
longer caught per-open. An identity *change* is unaffected: it still refuses the
open with `EmbedderIdentityMismatchError`, ahead of any cache. An unreadable or
absent cached verdict runs the probe rather than trusting it. Full rationale and
the residual: `dev/design/0.8.20-tc68-equivalence-probe-fingerprint-cache.md`.

**Scope.** The self-check guards **accidental** backend drift and a corrupt
baseline. It is **not** tamper evidence: an actor with write access to the database
file can rewrite the stored probe baseline — which defeats the check even when it
runs in full, exactly as it did before the cache — or the cached marker, or the
vectors themselves. Nothing at rest is authenticated. Do not read `dense_disabled`
as a tamper signal. **The cache does make one of those routes cheaper** (a forged
marker needs only a publicly computable digest; re-authoring the baseline needs the
other backend's 45 exact embeddings), bounded by the residual above: a
same-identity drift is already served off an honestly recorded marker, no forgery
required. Threat model, concession and bound: §8.3–§8.5 of the design note above.

`OpenReport` carries four embedder-related fields surfaced by EU-6:
`embedder_download_ms`, `embedder_events`, `embedder_mean_centering_required`,
and `embedder_mean_vec_pinned`. Each entry in `embedder_events` is a
`dict` keyed by `"kind"` (`"DefaultEmbedderDownload"`,
`"DefaultEmbedderCacheHit"`, or `"MeanVecPinned"`) with a variant-
specific payload in snake_case.

EU-6 FIX-2 declared `embedder_events` as a typed `TypedDict` union
(`fathomdb.types.EmbedderEvent`). The union includes `UnknownEmbedderEvent`
as a forward-compat fallback so a future or replaced native extension
emitting a new `kind` value remains type-sound. Because the unknown
fallback's `kind` field is the open type `str`, pyright cannot exclude
it purely from a literal `event["kind"] == "..."` check on the bare
union — gate the discriminant chain on `is_known_embedder_event` first
to recover precise narrowing on the three known variants:

```python
from fathomdb import Engine
from fathomdb.types import is_known_embedder_event

engine = Engine.open(path, use_default_embedder=True)
report = engine.open_report()
for event in report.embedder_events:
    if is_known_embedder_event(event):
        if event["kind"] == "DefaultEmbedderDownload":
            # pyright narrows: event["bytes"] is int, event["url"] is str.
            log(f"downloaded {event['bytes']} bytes from {event['url']}")
        elif event["kind"] == "MeanVecPinned":
            log(f"mean vec pinned at {event['doc_count']} docs (dim={event['dim']})")
    else:
        # `event` is `UnknownEmbedderEvent` — only `event["kind"]` is
        # typed; treat as opaque or log for diagnostics.
        log(f"unknown embedder event kind: {event['kind']}")
```

The two-step pattern (guard, then discriminate) is required because TS/
pyright literal narrowing on a discriminated union cannot remove an
open-typed member from the union when the discriminant is a literal —
`"DefaultEmbedderDownload"` could equal *any* `str`, so the unknown
fallback stays in the narrowed type and widens payload field access to
`object`. The exported `is_known_embedder_event` `TypeGuard` excludes
the unknown member up front, and the inner `if event["kind"] == "..."`
chain then narrows precisely to one variant `TypedDict`.

### Shipped feature axis (EU-6 FIX-1)

Released wheels published to PyPI are compiled with the `default-embedder`
Cargo feature ON, so `use_default_embedder=True`
materialises a real bge-small embedder against the published artifact
without any extra install step. The no-feature build path is preserved
as a CI sanity check (informational wheel-size signal on the minimal-
deps tree), not a shipped artifact — there is no
`pip install fathomdb[no-default-embedder]` extra in 0.7.1.

The `test-hooks` Cargo feature is dev-only and never ships: methods
like `_write_vector_for_test` and `_configure_vector_kind_for_test` do
not exist on installed wheels. They are exposed only when the editable
binding is rebuilt with `--features test-hooks` (the
`src/python/tests/conftest.py` session fixture does this for the
pytest suite). End-user callers should not rely on these symbols.

### Custom embedder implementations (deferred to 0.8.x)

Supplying a custom Python `Embedder` implementation requires a PyO3
callback bridge subject to ADR-0.6.0-embedder-protocol Invariant 3 (no
`pyo3-log` emission during `embed()`). That bridge is a multi-slice
campaign deferred to 0.8.x. In 0.7.1 the binding surface is binary:
`use_default_embedder=True` (engine's bge-small) or `False` (no embedder;
vector writes fail with `EmbedderNotConfiguredError`).

## `view=` on `search` / `search_text_only` (0.8.20 Slice 15b fix-2)

**Status: PROPOSED / NOT SIGNED.**

Both search verbs take the SAME optional `view` keyword the five read verbs
take. It is keyword-only and defaults to `None`.

```python
engine.search(query, filter=None, *, rerank_depth=0, use_graph_arm=False,
              alpha=None, pool_n=None, explain=False, view=None, limit=10)
engine.search_text_only(query, view=None, *, limit=10)
```

`view` is a `fathomdb.types.ReadView` — the same dataclass `read.get` /
`read.list` / `graph.neighbors` accept, with no new type minted.

- `view=None` (default) is the STRICT view: active-only, non-superseded, and
  valid AT QUERY TIME.
- `ReadView(valid_as_of=t)` evaluates validity at the bound instant `t`
  (INTEGER epoch SECONDS, UTC). Half-open, matching the write side and the read
  verbs: `t == valid_from` is IN, `t == valid_until` is OUT.
- `ReadView(include_out_of_window=True)` returns hits whatever their window.

**Default behaviour change.** A node whose window has closed (or has not opened)
is no longer returned by a default `search`. This is a no-op on any corpus that
never authored a window: omitting the write fields lands NULL/NULL, and NULL is
unbounded, so every pre-existing row still matches.

**Axis scope — VALIDITY only.** `ReadView(include_superseded=True)` and
`ReadView(include_inactive=True)` raise `InvalidArgumentError` on the search
path; they are REFUSED rather than silently ignored, because search hydrates
from projection indexes that are not version-complete. Use `read.list` to
enumerate history. A `view=` that is not a `ReadView` (or `None`) raises
`TypeError` at the Python boundary, matching the `rerank_depth` / `explain` /
alpha / `pool_n` guards.

These are ARGUMENTS, not new verbs — the governed command surface
(`src/conformance/governed-surface-allowlist.json`) is unchanged.

## Nested-source projections (0.8.21 Slice 60)

`ProjectionSpec.source: tuple[str, ...] | None` is a literal canonical-body
member path; `None` keeps legacy top-level lookup. Missing/null terminals produce
no row. Object/array terminals reject configuration backfill and writes atomically
with `WriteValidationError`.

`SearchFilter.attributes: tuple[tuple[str, str], ...]` is ordered AND equality
over declared `FILTERABLE` projections. Canonical text is intentional: projected
string `"1"` and number `1` both match `"1"`. `from_search_filter` rejects a
non-empty attribute list with `InvalidFilterError` rather than losing predicates.

For `engine.search(..., explain=True)` with attribute predicates,
`result.explanation.trace.dropped_edge_hits` reports edge-FTS candidates rejected
solely by the node-scoped attribute rule. The default non-explain path does not
collect this count.

`engine.search_projected_text(query, name, filter=None, *, view=None, limit=10)` searches
only the named declared `SEARCHABLE` property-FTS projection, applying metadata,
validity, and attribute filters. It does not body-scan, use vectors, or fuse;
hits are text branch with no soft fallback or explanation.

## Ranked result limits (0.8.22 Slice 18)

`engine.search`, `engine.search_text_only`, and `engine.search_projected_text` each expose a
keyword-only `limit=10`. `graph.search_expand` exposes keyword-only `search_limit=10` for its
initial ranked `search_hits`. Every value must be an integer in `1..=100`; zero, negatives, and
values above 100 raise `InvalidArgumentError` rather than silently clamping. `graph.neighbors`
remains a separately bounded traversal API with its existing 50-result cap.

## Non-presence

Python does not expose recovery verbs or doctor-only flags. In particular,
there is no SDK equivalent of `recover`, `check-integrity`, `--quick`,
`--full`, or `--round-trip`. See `design/recovery.md`.
