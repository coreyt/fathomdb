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
  window and raises `InvalidArgumentError`. Validation runs before any insert,
  so the **whole batch** is rejected.
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
`{ name, roles: frozenset[str], fts, fts_tokenizer, vector, vector_embedder }`.
`ProjectionRole` (`fathomdb.types.ProjectionRole`) has exactly three members —
`FILTERABLE`, `RANKABLE`, `SEARCHABLE`; `searchable→FTS` and `searchable→vector`
are tier labels carried by the `fts`/`vector` sub-object flags, not roles. Cheap
roles (`filterable`, `searchable→FTS`) build same-transaction; `rankable` and the
`searchable→vector` sub-target are persisted-but-deferred (reported in
`ProjectionDelta.deferred`). The `vector` sub-object is stored here for Slice 20
to attach `dense_readiness` to. `ProjectionDelta` is
`{ built, dropped, deferred, unchanged }`.

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
