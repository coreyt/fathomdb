# Projection registry & the async-embed execution model

> Part of the [record lifecycle & projection protocol](README.md). **Status: PROPOSED.** This is the seam
> that lets **interpretive content be retrievable** without recreating CR-057 — and the execution/concurrency
> contract for the one projection type (vector) that cannot be same-transaction. Companion:
> [structural lifecycle contract](structural-lifecycle-contract.md).

## 1. Why this exists — the retrievability-of-meaning problem

Meaning that must be *searched/filtered/ranked* has to reach the **structural indexes** (FTS, vector,
typed `Filter`, graph). A rule of "the app owns meaning, the engine owns retrieval" leaves interpretive
content that needs to be retrievable with **nowhere clean to live** — and that is exactly what caused
**CR-057**: with no sanctioned projection path, the app hand-denormalized status/title into search fields
and it rotted. Banning denormalization removes the *legitimate* channel and guarantees the *illegitimate*
one returns.

**The fix — a third pillar:** the engine owns **state + the projection contract**; the app owns **meaning +
projection declarations.** Typed **edges** already live in FathomDB's substrate (`canonical_edges`), but
there is **no EAV/attribute store and no property-FTS today** — only `body`-FTS (`search_index`) + vector.
So the canonical **attribute store + property-FTS the registry projects *from* are net-new** (they converge
with Memex's entity-schema-registry EAV, which is likewise being built). Ownership is over **schemas and
semantics**, not bytes.

## 2. The projection registry

Per interpretive **attribute** or **edge-type**, Memex **declares** its index visibility:

| Declaration | Projects into | Purpose |
|---|---|---|
| `filterable` | a typed `Filter` / indexed column | pre-KNN constraint, cheap equality/range |
| `rankable` | the F9 importance / recency signal | affects ordering (not exclusion), under the engine signal algebra |
| `searchable` | property-FTS field **and/or** an embedded vector | full-text / dense recall of the meaning text |

Tokenizer / embedder choices are part of the declaration where relevant. Projections are **rebuild-durable**
(re-derivable from the canonical graph — the CQRS answer to drift), which works precisely because the
canonical interpretive values live in engine storage. This **converges with Memex's entity-schema-registry**
(hot kinds → Fathom property-FTS): Memex is already building the app-side of exactly this registry.

**"Never denormalize" is softened** to its true intent: never *hand*-denormalize into ad-hoc structural
fields — flow through the registry, or resolve at read.

## 3. Staleness contract — SPLIT BY PROJECTION TYPE (load-bearing)

A single "same-transaction, non-stale" guarantee is **unachievable** for vector projections: embedding runs
the (CPU-pinned, serialized, seconds/batch) embedder, and doing that inside the write transaction stalls
every writer behind embed latency and couples the write path to GPU availability. So:

| Projection | Staleness |
|---|---|
| `filterable`, `searchable → FTS` | **same-transaction** — inline (write a column / tokenize); non-stale on commit |
| `searchable → vector` | **async, rebuild-durable** — carries an engine-set readiness flag `dense_readiness ∈ {ready, embedding}` |

> **Naming:** `dense_readiness` (**index-readiness**, an engine mechanism) is a **different dimension** from
> the admission state `pending` (**quarantine/trust**, an app judgment). They are orthogonal (a record can be
> `active ∧ is_latest ∧ admissible` yet `dense_readiness = embedding`). The token "pending" is **reserved for
> the admission axis**; the readiness flag deliberately does **not** reuse it.

### 3.1 The `commit ≠ fully-retrievable` contract (state it plainly to callers)

- A newly-committed record is **FTS/filter-retrievable immediately**, and **dense/vector-retrievable once
  embedded**.
- The **partial-dense-arm signal rides every read's default result metadata** (not verbose-`explain`-only) —
  because the opt-in coverage/verify gates are exactly what a forgetful host forgets. This is the
  non-optional, on-the-hot-path backstop against "green rebuild, lossy live."
- **RRF over a partially-embedded corpus UNDER-RANKS, it does not hide:** a record present in the FTS arm
  but absent from the vector arm enters fusion with one arm missing, so its fused rank is systematically
  depressed vs fully-indexed peers. Recent memories are **under-ranked until embedded**, not invisible.

## 4. Async-embed execution model

> **Reconciled to shipped code.** FathomDB **already owns an in-process async projection worker** — a
> dispatcher + worker-thread pool (`projection_dispatcher_loop` / `projection_worker_loop`, `lib.rs:876-887`)
> that embeds **off the write path**, cursor-scheduled via `_fathomdb_projection_state.last_enqueued_cursor`
> (`schema:166`). The earlier "no daemon; host owns cadence; sync-inline default" framing was **wrong about
> today** — the engine's default IS an async worker. The reconciled model **keeps that worker** and adds the
> missing controls on top of it (all **net-new**): a `dense_readiness` flag, a `flush_embeddings()` drain, and
> an optional sync-inline mode. **Flush reuses the shipped `drain(timeout_ms)` barrier** (`lib.rs:4360`) — not
> a new `flush_embeddings()`; deferred/backfill rows enqueue on the same projection runtime `drain` waits on.
> See [`api-surface.md`](api-surface.md).

Two **host-selectable** modes over the existing worker:

- **sync-inline (default, interactive single writes):** embed in/adjacent to the write ⇒ `commit ⇒ fully
  retrievable`. Fast on GPU (single-digit-to-tens of ms, illustrative — not a promised figure); the
  slow-CPU cost is the host's to accept.
- **deferred + `flush_embeddings()` (opt-in, bulk ingest):** the write commits with
  `dense_readiness = embedding`; the host batches embedding via an explicit flush. The loss window is thus
  **host-chosen and host-visible**, never silent.

The engine promises **observability, not an SLA** (it gave up its clock, so it cannot enforce a time bound):
`_fathomdb_vector_rows.kind` coverage tracking + a `verify_embed_db`-style completeness gate + the default
partial-dense metadata + an oldest-`embedding` dwell metric. The **host owns the cadence and the bound.**

### 4.1 Invariants (implementation-level, must hold)

1. **Atomic readiness flip (the last sharp edge, load-bearing):** `{ vector-insert ∧ dense_readiness := ready }`
   is **one transaction**. The **only** tolerable torn state is `dense_readiness = embedding` with the vector
   absent (safe — the query treats the dense arm as partial). A torn `ready`-without-vector is **forbidden** —
   it would reintroduce the silent miss as a torn write that rebuild-durability papers over.
2. **Chunked, yielding flush:** `flush_embeddings()` persists vectors in **bounded chunks, releasing the
   single writer between chunks**, so interactive writes interleave at chunk boundaries (the write-stall does
   not relocate to flush-vs-write). A user write landing mid-flush serializes at a chunk boundary.
3. **Skip non-current rows in flush:** a row **superseded or deleted** between commit and flush is **not**
   embedded, and readiness is **never** flipped on a now-deleted row.

## 5. How this fixes CR-057 (without recreating it)

The registry is the **sanctioned, non-stale** denormalization channel: single-writer makes same-transaction
projection cheap for FTS/filter; vector is async but rebuild-durable and its staleness is **observable**, not
silent. One owner (the engine, driven by app declarations), one write path, transactional (for FTS/filter) or
explicitly-tracked (for vector). That is what makes governed denormalization non-stale where CR-057's
hand-rolled copy rotted.

## 6. Open contract items (named, not closed)

- **F9 signal algebra** — the `rankable` contract (range / monotonicity / missing-value default / combination
  law with BM25·vector·RRF·recency) is specced when F9 lands (~0.8.16).
- **`history_as_of`** — transaction-time travel is deferred; only `valid_as_of` (world-time) ships. The
  append-only version history does not preclude it.
- **GDPR file-erasure preconditions** — `secure_delete`/`VACUUM` + opaque `logical_id` (see the structural
  contract §1.2).
