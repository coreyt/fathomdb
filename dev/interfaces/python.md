---
title: Python Public Interface
date: 2026-04-24
target_release: 0.6.0
desc: Public Python surface for 0.6.0
blast_radius: src/python/; design/bindings.md; design/errors.md; design/lifecycle.md; design/engine.md
status: locked
---

# Python Interface

This file owns Python-visible symbol spelling and attribute casing.
Cross-binding parity remains owned by `design/bindings.md`.

## Runtime surface

The canonical runtime verbs available to Python callers are:

- `Engine.open(...)`
- `engine.write(...)`
- `engine.search(...)`
- `engine.close()`
- `admin.configure(...)`

`Engine.open(...)` returns the engine handle. The structured open report owned
by `design/engine.md` is accessible after open via `engine.open_report()` (see
Engine-attached instrumentation / control below).

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

- `WriteReceipt.cursor`
- `SearchResult.projection_cursor`
- `SearchResult.soft_fallback.branch`

`soft_fallback.branch` uses the typed values owned by `design/retrieval.md`.

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
above is unchanged. The fields-only delta is **PROPOSED, NOT SIGNED**.

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

**Layering note.** This is the GOVERNED SDK write surface. ISO-8601 survives
ONLY on the **BYO-LLM extractor wire** (`fathomdb.extract.v1`), where the engine
normalises each timestamp to epoch seconds with a HARD REJECTION of any value
`strftime('%s', ?)` cannot parse — an unparseable timestamp must never coerce to
NULL, because a NULL `t_invalid` reads as "still valid" and would resurrect an
invalidated edge. Fields-only delta, **PROPOSED, NOT SIGNED**.

## Projection registry (0.8.20 Slice 15d, R-20-PR / C-1)

Two net-new governed verbs declare and inspect projections over interpretive
attributes. **PROPOSED, NOT SIGNED.**

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

- **Exactly two spellings: `"ready"` and `"embedding"`.** `"pending"` is
  DELIBERATELY not one of them — that token is RESERVED for the orthogonal
  **admission** axis (quarantine/trust, an app judgment). Index-readiness and
  admission are different dimensions: a record can be admissible and still read
  `"embedding"`. Do not reuse the word.
- **Derived, never stored.** There is no schema step and no `SCHEMA_VERSION`
  bump; the value is computed per `read.projections` call from outstanding
  projection work (the same predicate `drain` uses), which is what makes
  `{vector-insert ∧ readiness := ready}` atomic by construction — `"ready"` can
  never be observed with the vector row absent.
- **Accept-inert on the way in.** Passing `vector_dense_readiness` to
  `engine.configure_projections` neither stores nor changes anything: it is not
  part of the declaration and the engine always reports the derived truth. That
  is deliberate, so `read.projections` output stays feedable straight back into
  `configure_projections` as a no-op (`ProjectionDelta(unchanged=True)`).
- **Two shapes are still hard-rejected**, because they could never round-trip:
  a readiness supplied with `vector=False`, and any spelling outside
  `{"ready", "embedding"}` (including `"pending"`, `""`, and `"Ready"`). Both
  raise the EXISTING `InvalidArgumentError` — **no new error type is minted**.
  `None` is always accepted.
- **Additive.** A caller who never reads the field sees identical behaviour, and
  the slice adds ZERO net-new governed commands.

### `engine.drain()` is the flush-to-readiness barrier (0.8.20 Slice 20c, R-20-DR)

There is **no `flush_embeddings()` verb**. The shipped
`engine.drain(timeout_s=...)` — note **SECONDS** here, milliseconds in
TypeScript — carries those semantics, so the surface gains ZERO net-new governed
commands. The pinned invariant, tested in Rust, Python and TypeScript:

> `drain()` returning normally ⟹ `vector_dense_readiness == "ready"`, **and every
> vector-eligible row has its vector row at rest.**

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
  `{"name": "summary", "roles": ["filterable"], "vector": True}` is accepted and
  round-trips verbatim, but it is **INERT**: it enrols no kind, backfills nothing,
  and makes no later write enqueue an embedding. Previously the engine keyed the
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

Python exposes one catch-all base class plus one concrete subclass per canonical
row in `design/errors.md`.

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

Payload fields remain typed attributes; callers do not dispatch on message
text.

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
baseline. It is **not** an integrity boundary against an actor with write access
to the database file: such an actor can rewrite the stored probe baseline — which
defeats the check even when it runs in full, exactly as it did before the cache —
or the cached marker, or the vectors themselves. Do not read `dense_disabled` as a
tamper signal. Threat model: §8 of the design note above.

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
              alpha=None, pool_n=None, explain=False, view=None)
engine.search_text_only(query, view=None)
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

## Non-presence

Python does not expose recovery verbs or doctor-only flags. In particular,
there is no SDK equivalent of `recover`, `check-integrity`, `--quick`,
`--full`, or `--round-trip`. See `design/recovery.md`.
