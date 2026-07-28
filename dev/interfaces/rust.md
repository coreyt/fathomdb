---
title: Rust Public Interface
date: 2026-05-12
target_release: 0.6.0
desc: Public Rust surface (traits, functions, types, errors) for 0.6.0
blast_radius: src/rust/crates/fathomdb; design/engine.md; design/bindings.md; design/errors.md; design/lifecycle.md
status: locked
---

# Rust Interface

This file owns Rust-visible symbol spelling and result shape. Cross-binding
parity rules remain owned by `design/bindings.md`.

## Support posture

The Rust facade is stable public Rust contract in 0.6.0 and is the
ground-truth source for engine-side type names. The Python/TypeScript governed
SDK surface parity set is tested by AC-074 (which supersedes AC-057a's
five-verb cap). Under the signed Q5 = BIND-RUST
(`ADR-0.8.0-supersede-five-verb-surface-cap`) the Rust facade is **also** bound
by AC-074; its positive-allowlist/parity pin **landed at reserved-gap Slice 27**
(see § Governed-surface contract below). Rust keeps the facade shape below
unless a successor ADR expands it.

## Governed-surface contract (AC-074, Q5 = BIND-RUST — landed Slice 27; method-level + feature-gated by Slice 27 fix-1)

This file **owns** the governed Rust-facade surface. The `fathomdb` facade is a
**different consumer contract** than the Python/TypeScript 5-verb + `read.*`
SDK: the Rust application verbs are methods on `Engine` (`open`/`write`/
`search`/`close`), and the facade's public surface is a set of re-exported
*types*, not free verbs. So the Rust allowlist is **not** the Py/TS verb set; it
is the **typed governed application surface** this file owns. Three load-bearing
properties hold (asserted by `src/rust/crates/fathomdb/tests/governed_surface.rs`,
which binds AC-074 — not a new AC id):

- **P1 — positive allowlist (`GOVERNED_SURFACE_ALLOWLIST`, 33 types):** the
  facade re-exports exactly the curated governed application surface — the
  original 17: `Engine`, `OpenedEngine`, `OpenReport`, `WriteReceipt`,
  `SearchResult`, `PreparedWrite`, `EngineError`, `EngineOpenError`, the open-path
  diagnostics (`CorruptionDetail`, `CorruptionKind`, `CorruptionLocator`,
  `OpenStage`, `RecoveryHint`), the retrieval soft-fallback shapes (`SoftFallback`,
  `SoftFallbackBranch`), and the instrumentation handles (`CounterSnapshot`,
  `Subscription`) — plus the additive groups: Slice 20 (G5/G6) graph-traversal
  types (`TraversalDirection`, `NodeRecord`, `SearchExpandResult`, `SearchFilter`),
  Slice 35 (G4) filter-grammar types (`Predicate`, `ScalarValue`, `ComparisonOp`),
  Slice 15 (G11) BYO-LLM ingest types (`ExtractDocument`,
  `IngestWithExtractorReceipt`), and 0.8.8 Slice 5 (EXP-OBS) explain-sidecar types
  (`Explanation`, `QueryTrace`, `PerHitExplain`). 0.8.20 Slice 10b
  (R-20-RV / R-20-NV) adds the read-view / node-validity types (`ReadView`,
  `BoundaryCrossing`) — **PROPOSED, NOT SIGNED** (see
  `src/conformance/governed-surface-allowlist.json`). `ReadView` is threaded as a
  PARAMETER on the five existing read verbs (`read_get`, `read_get_many`,
  `read_list`, `read_list_filter`, `graph_neighbors`) rather than shipped as five
  `*_with_view` sibling verbs, which is what keeps the delta at two TYPES and zero
  new verb names; `ReadView::default()` is the strict view and reproduces the
  pre-slice read semantics exactly. 0.8.20 Slices 5c/5d (R-20-E3 / R-20-E4) add
  the erasure types `SourceId` and `ExciseReport` (the latter moved out of the
  operator-gated block — it is `erase_source`'s return type) — **PROPOSED, NOT
  SIGNED**. Each of those 33 resolves through the facade at compile time
  (`type_name::<…>()`). The facade ALSO `pub use`s two further additive groups
  that are **not yet members of the const**: the five Slice 15d (R-20-PR)
  projection-registry types (`ProjectionSpec`, `ProjectionRole`, `ProjectionFts`,
  `ProjectionVector`, `ProjectionDelta`) and, from 0.8.20 Slice 20 (R-20-DR), the
  single readiness enum `DenseReadiness` — both **PROPOSED, NOT SIGNED** (see
  § "Projection registry" below and
  `src/conformance/governed-surface-allowlist.json`). The recovery /
  integrity / dump operator-seam report types in § "Recovery / operator seam
  re-exports" are deliberately **excluded** from this allowlist — they are
  CLI-only ergonomic symbols (the Rust analogue of "recovery is CLI-only, not an
  SDK verb"), not governed application surface.

- **P2 — parity-in-intent (NOT membership-identity):** the Rust governed surface
  is posture-consistent with the Py/TS governed surface (a governed allowlist,
  recovery-denylist-absent, typed / no-raw-SQL) but is a different consumer
  contract — a type set, not a verb set — so it is **not** asserted
  membership-equal to the Py/TS verb allowlist. The one genuinely shared element
  is the recovery denylist, declared once in
  `src/conformance/governed-surface-allowlist.json` (`recovery_denylist`); the
  Rust test pins the same five names.

- **P3 — recovery-denylist absence:** no governed-surface symbol *is* a recovery
  verb in `{recover, restore, repair, fix, rebuild}` (exact, case-insensitive —
  not substring, so the typed `RecoveryHint` hint is correctly not flagged). The
  **canonical** denylist enforcement remains the **byte-frozen**
  `tests/no_recovery_surface.rs`; `governed_surface.rs` adds the *positive*
  allowlist half + an allowlist-scope denylist check.

Rust has no runtime symbol-table introspection (no `dir(module)`), so — exactly
like `no_recovery_surface.rs` / `reexports.rs` — the type-level pin is a
compile-time resolves-check plus this source-inspection-documented contract. See
`dev/design/slice-27-rust-allowlist-design.md`.

### Method-level boundary: default surface vs the `operator` feature (Slice 27 fix-1)

The Slice 27 type-only audit missed that the facade re-exports the engine's
`Engine` **wholesale**, so its inherent **methods** — including
`rebuild_projections`/`rebuild_vec0` (recovery-denylist names) and the
debug-only raw-SQL `execute_for_test` — were reachable. Per the signed Option B
(codex [P1], HITL 2026-06-05) the **operator/recovery seam is feature-gated**:

- **Default `fathomdb` facade (operator OFF)** — the governed runtime surface:
  the 29 governed types + the application methods `Engine::open`/`write`/`search`/
  `search_explained`/`close` (+ the engine-attached instrumentation/control methods). It exposes
  **no method whose name is in `{recover, restore, repair, fix, rebuild}`** and
  **no raw-SQL method**. This is enforced at the **method** level by
  `compile_fail` doctests in `fathomdb/src/lib.rs`
  (`governed_surface_method_absence_proof`, default build;
  `release_surface_raw_sql_absence_proof`, release build) — the only mechanism
  that can assert a method does *not* resolve.
- **`operator` feature (ON — `fathomdb-cli` enables it)** — un-gates the 12
  operator/recovery methods (`rebuild_*`, `excise_source`, `dump_*`,
  `trace_source_ref`, `truncate_wal`, `verify_embedder`, `check_integrity`,
  `safe_export`, `recompute_mean`) + the 20 operator-seam re-exports below. The
  CLI (`fathomdb recover`/`doctor`) is the operator substrate. **Gating, not
  deletion**: engine behavior is byte-identical with the feature on.

So the recovery-denylist + no-raw-SQL guarantees hold at the **method** level on
the default governed surface, while the CLI retains the seam via the feature.
See `dev/design/slice-27-fix1-operator-gate-design.md`.

## Public surface

Rust exposes:

- `Engine::open(...) -> Result<OpenedEngine, EngineOpenError>`
- `Engine::write(...) -> Result<WriteReceipt, EngineError>`
- `Engine::search(...) -> Result<SearchResult, EngineError>`
- `Engine::search_explained(...) -> Result<SearchResult, EngineError>` — 0.8.8
  EXP-OBS: same retrieval as `search_reranked`, additionally returning the opt-in
  `Explanation` sidecar (`SearchResult.explanation`); default paths are unchanged.
- `Engine::close(...) -> Result<(), EngineError>`

`OpenedEngine` contains:

- `engine`
- `report`

`report` is the `OpenReport` owned by `design/engine.md`.

## Engine-attached instrumentation / control methods

These are public instance methods, not extra top-level SDK verbs:

- `Engine::drain(timeout_ms: u64) -> Result<(), EngineError>`
- `Engine::counters() -> CounterSnapshot`
- `Engine::set_profiling(enabled: bool) -> Result<(), EngineError>`
- `Engine::set_slow_threshold_ms(value: u64) -> Result<(), EngineError>`
- `Engine::subscribe(&self, subscriber: Arc<dyn lifecycle::Subscriber>) -> Subscription`

`drain` is a bounded completion surface for post-commit projection work. It
returns `Ok(())` when the engine-owned background projection queue reaches a
quiescent state before `timeout_ms`, and returns a typed runtime error when the
timeout elapses first.

`subscribe` owns host-subscriber attachment and may carry heartbeat-cadence
options. The payload semantics remain owned by `design/lifecycle.md` and
`design/migrations.md`.

## Companion embedder contract

The Rust workspace also exposes the semver-stable companion crate
`fathomdb-embedder-api` for engine-owned embedder dispatch:

- `Embedder`
- `EmbedderIdentity { name, revision, dimension }`
- `EmbedderError`

## Caller-visible data shapes

- `WriteReceipt` has exactly one public field: `cursor`
- `SearchResult` exposes `projection_cursor`, which names the terminal
  projection-visible point for the search snapshot
- hybrid fallback, when present, exposes a typed branch enum whose values are
  owned by `design/retrieval.md`
- counter/profile/stress payload shapes are owned by `design/lifecycle.md`

## Caller-supplied write shapes

`PreparedWrite` is the caller-supplied input to `Engine::write` and is itself
governed surface (§ P1), so adding a variant field changes what every binding
must accept.

### `PreparedWrite::Node` — world-time validity window (0.8.20 Slice 15b, TC-34)

`PreparedWrite::Node` carries two optional validity bounds:

- `valid_from: Option<i64>` — INCLUSIVE lower bound, INTEGER epoch **seconds**
  UTC. `None` lands SQL NULL = unbounded below.
- `valid_until: Option<i64>` — EXCLUSIVE upper bound, same units. `None` lands
  SQL NULL = unbounded above.

The window is **half-open** — `[valid_from, valid_until)` — matching the read
predicate `ReadView::validity_sql` exactly: an instant equal to `valid_from` is
IN the window, an instant equal to `valid_until` is OUT.

These are **fields, not a new verb**. The governed *command* surface is
unchanged and allowlist membership in
`src/conformance/governed-surface-allowlist.json` is byte-identical; the
precedent is `PreparedWrite::Edge`, which has carried `t_valid`/`t_invalid` the
same way since Slice 30. The fields-only delta is **PROPOSED, NOT SIGNED**.

Slice 10b (R-20-NV) shipped the `canonical_nodes.valid_from`/`valid_until`
columns, the `ReadView` validity predicate and `Engine::crossed_boundary_since`
as a READ-ONLY axis with no writer; these two fields are that writer.

**Refusal rule (engine-owned).** Validation lives in the engine's
`validate_write`, so Rust, Python and TypeScript share one rule and cannot
drift:

- Both bounds present with `valid_from >= valid_until` describes an
  UNSATISFIABLE half-open window that no instant can ever match. It is refused
  with **`EngineError::WriteValidation`**. Validation runs **before any INSERT**,
  so the WHOLE batch is rejected. It surfaces as `WriteValidationError` in both
  bindings.

  > **BREAKING (0.8.20 Slice 22, decision #18).** This was
  > `EngineError::InvalidArgument { msg }` **naming both bounds**, which made
  > `validate_write` — one function — reject across two error families. It is now
  > the ONE family the taxonomy of record assigns to that boundary
  > (`dev/design/errors.md`, 2026-07-28 amendment). **`WriteValidation` is a unit
  > variant, so the offending bounds are NO LONGER carried in the error.** A
  > caller that parsed them out must instead validate the pair before calling.
- A **one-sided** window (exactly one bound present) can never be empty and is
  **never** refused, however extreme its single bound.

**No-regression guarantee.** Omitting both fields binds NULL/NULL — identical to
what schema step 22 left on every pre-existing row — so a write that does not
mention validity keeps exactly its pre-slice default-view visibility.

## Read-side validity on `search` (0.8.20 Slice 15b fix-2)

**Status: PROPOSED / NOT SIGNED.**

Slice 10b applied `ReadView` to the five read verbs only. Because Slice 15b made
validity windows AUTHORABLE from the SDK, the default `search` path now also
applies the validity predicate — otherwise a node hidden by `read_get` /
`read_list` would still be returned by `search`.

**Default behaviour change (deliberate, and the only one in this fix).** Every
search entry point (`search`, `search_filtered`, `search_filter`,
`search_reranked`, `search_explained`, `search_text_only`, and the opt-in graph
arm) now hides nodes that are out of window AT QUERY TIME. This is a **no-op on
any corpus that never authored a window**: schema step 22 back-filled
`valid_from` / `valid_until` as NULL with no DEFAULT, and NULL is unbounded on
that side, so the predicate matches every pre-existing row and leaves the
row-set, the `bm25()` ordering and the scores byte-unchanged.

**New methods** (additive; the six shipped search signatures are UNCHANGED):

- `Engine::search_view(query, &ReadView) -> Result<SearchResult, EngineError>`
- `Engine::search_reranked_view(query, filter, rerank_depth, use_graph_arm,
  alpha, pool_n, explain, &ReadView) -> Result<SearchResult, EngineError>` — the
  full-arity form the Python/TS `view=` bindings call, so a caller can combine a
  content filter, the CE knobs and a validity view in one query.
- `Engine::search_text_only_view(query, &ReadView) -> Result<SearchResult, EngineError>`

`search_reranked(q, f, d, g, a, p)` is exactly
`search_reranked_view(q, f, d, g, a, p, false, &ReadView::default())`.

**Axis scope — VALIDITY only.** `valid_as_of` and `include_out_of_window` are
honoured. `include_superseded` and `include_inactive` are **refused** with
`EngineError::InvalidArgument`, NOT silently ignored: search hydrates from
projection indexes (`search_index`, `vector_default`) that are not
version-complete, so the existence axis has no truthful answer on this path, and
relaxing `superseded_at IS NULL` here would re-open the stale-body leak the
Slice-15 fix-1 review closed. Use `read_list` to enumerate history. **This is a
decision owed to HITL** — refusing is the smallest coherent option, but ignoring
or fully honouring are both defensible alternatives.

The instant is INTEGER epoch SECONDS, read in Rust and BOUND as a positional
parameter (never `datetime('now')`), once per query — the same `:now` seam as
the read verbs, so search validity is deterministically testable.

## Projection registry (0.8.20 Slice 15d, R-20-PR / C-1)

**Status: PROPOSED / NOT SIGNED** (tracked in
`src/conformance/governed-surface-allowlist.json`; AC-079 UNMINTED).

Two net-new governed methods on `Engine` declare and inspect projections over
interpretive attributes. The facade re-exports the five supporting
`Projection*` types — plus `DenseReadiness` since 0.8.20 Slice 20 (R-20-DR) —
all part of the public Rust surface:

- `Engine::configure_projections(specs: &[ProjectionSpec], drop: &[String]) ->
  Result<ProjectionDelta, EngineError>` — declarative, idempotent apply: the
  engine is the SOLE projection authority (C-1) and diffs `specs` against the
  durable registry, backfilling the difference in one write transaction. `drop`
  is EXPLICIT — omitting a live projection from `specs` does NOT drop it; removal
  requires naming it in `drop`. A destructive change (a role removal or a
  tokenizer/embedder change) without a drop is refused with
  `EngineError::ProjectionDestructive { name, delta }` — the delta names what the
  caller must drop. Re-applying an unchanged spec returns
  `ProjectionDelta { unchanged: true, .. }` with the vecs empty.
- `Engine::read_projections() -> Result<Vec<ProjectionSpec>, EngineError>` — the
  registry introspection (the Rust analogue of `read.projections`), sorted by
  name. Pure read; never mutates. Since 0.8.20 Slice 20 (R-20-DR) it is also the
  surface that populates the engine-set `ProjectionVector::dense_readiness`
  READ METADATA (derived on the way out; see below).

Types:

- `ProjectionSpec { name: String, roles: BTreeSet<ProjectionRole>, fts:
  Option<ProjectionFts>, vector: Option<ProjectionVector> }`. `roles` carries SET
  semantics (an attribute can be `Filterable` AND `Searchable`).
- `ProjectionRole` — exactly three variants: `Filterable`, `Rankable`,
  `Searchable` (`searchable→FTS` and `searchable→vector` are tier labels carried
  by the `fts`/`vector` sub-objects, not roles). `as_str` / `from_str_opt` give
  the `"filterable" | "rankable" | "searchable"` wire spellings.
- `ProjectionFts { tokenizer: Option<String> }` and `ProjectionVector { embedder:
  Option<String>, dense_readiness: Option<DenseReadiness> }` — the
  `searchable→FTS` / `searchable→vector` sub-target selectors (`None` embedder ⇒
  engine default). `dense_readiness` was added additively by 0.8.20 Slice 20
  (R-20-DR); see below.
- `DenseReadiness` — a two-variant enum, `Ready` and `Embedding`, with
  `as_str` / `from_str_opt` giving the `"ready" | "embedding"` wire spellings.
  0.8.20 Slice 20 (R-20-DR), **PROPOSED, NOT SIGNED**; the only net-new type in
  that slice, which adds ZERO net-new governed commands.
- `ProjectionDelta { built, dropped, deferred, unchanged }`. Cheap roles
  (`filterable`, `searchable→FTS`) build same-transaction; `rankable` and the
  `searchable→vector` sub-target are persisted-but-deferred (reported in
  `deferred`, never an error).

**Projection-name contract (0.8.20 Slice 15d fix-4).** A projection `name` is an
app-declared identifier that becomes a SQLite JSON-path key (`$."<name>"`) at
write time, so `configure_projections` REJECTS — with
`EngineError::InvalidArgument` naming the offending value — any spec or `drop`
name that cannot round-trip through that quoted-key form: an empty name, a name
containing a double-quote `"`, a name containing a BACKSLASH `\`, or a name
containing any ASCII control char. This upholds the invariant "a name the engine
ACCEPTS is populatable" (accept ⟹ works); previously a backslash name was
accepted yet silently never populated `canonical_attributes`.

**Dense readiness on `ProjectionVector` (0.8.20 Slice 20, R-20-DR).**
`ProjectionVector::dense_readiness: Option<DenseReadiness>` is **engine-set READ
METADATA**, not a declaration. `Engine::read_projections` populates it — and only
for a spec that declares the `searchable→vector` sub-object; `filterable` and
`searchable→FTS` are same-transaction (non-stale on commit) and have no readiness
axis. It is `None` on every caller-authored spec.

- **`DenseReadiness` has exactly two variants**, `Ready` and `Embedding`, wire
  spellings `"ready"` / `"embedding"`. **`pending` is RESERVED for the orthogonal
  ADMISSION axis** (quarantine/trust — an app judgment) and is deliberately never
  an index-readiness value: a record can be `active ∧ is_latest ∧ admissible` and
  still read `Embedding`. `from_str_opt("pending")` is `None`.
- **DERIVED, never stored.** No schema step, no `MIGRATIONS` change,
  `SCHEMA_VERSION` stays 24. The value is computed on the way out of
  `read_projections` from the same outstanding-work predicate `drain` /
  `wait_for_idle` use, so "readiness is `Ready`" and "`drain` reports idle" cannot
  disagree. That is what makes `{ vector-insert ∧ readiness := ready }` atomic BY
  CONSTRUCTION: `Ready` can never be observed with the vector row absent (only the
  tolerated torn state — `Embedding` with the vector absent — is reachable). The
  predicate is corpus-wide rather than per-attribute while Slice 15d still defers
  per-attribute embedding.
- **ACCEPT-INERT on the way in.** `Engine::configure_projections` neither stores
  nor honours a caller-supplied `dense_readiness` (`StoredProjection::from_spec`
  reads only `embedder`), so `read_projections` output re-applies as a no-op —
  the shipped read→configure round-trip. Mirrors the accept-inert ruling on an
  `fts`/`vector` sub-object declared without the `searchable` role.
- **The BINDINGS hard-reject** the two shapes that could never round-trip: a
  readiness supplied with `vector = false`, and any spelling outside
  `{ready, embedding}` (notably `pending`, and the empty string). Both reuse the
  EXISTING `EngineError::InvalidArgument` / `InvalidArgumentError` /
  `FDB_INVALID_ARGUMENT` — **no new error type is minted.** A declared readiness
  never changes what the engine reports.
- **Additive.** Callers who never look at readiness see identical behaviour.
  **PROPOSED, NOT SIGNED.**

**`drain` is the flush-to-readiness barrier (0.8.20 Slice 20c, R-20-DR /
`api-surface.md` C4).** There is **no `flush_embeddings()` verb** — the shipped
`Engine::drain(timeout_ms)` carries those semantics, so the surface gains ZERO
net-new governed commands (TC-55 = INSTRUMENTATION). The pinned invariant, tested
in Rust, Python and TypeScript:

> `drain(timeout)` returning `Ok(())` ⟹ `dense_readiness == Ready`,
> **and every vector-eligible row has its vector row at rest.**

- **`drain` is a BARRIER, not a trigger.** It waits for the projection runtime to
  go quiescent; it never schedules or wakes anything. Deferred/backfill work is
  therefore enqueued on the **enqueue side**, on the same runtime `drain` waits
  on: `Engine::configure_projections` enrols the vector kinds, re-opens the
  stranded rows' readiness terminals and calls the runtime notify **after its
  commit**. Without that, declaring `searchable→vector` over an existing corpus
  reported `Ready` with no vectors and nothing that would ever create them.
- **Ordering does not matter.** Write-then-declare and declare-then-write behave
  identically: a kind first written after the declaration is enrolled on the write
  path, before the decision to wake the dispatcher is taken. That write-path
  enrolment performs the **same** backfill the declaration does (fix-2), so rows
  of that kind written by an earlier session — for instance one opened without an
  embedder, where the declaration persisted but deferred — are picked up too,
  rather than being left behind a `Ready` that is not true of them.
- **The dense arm covers only the engine's locked `kind` vocabulary** (fix-2).
  A `searchable→vector` declaration turns the dense arm on for node kinds in
  `{email, article, paper, meeting, note, todo, doc}` (plus the engine-internal
  `edge_fact` for edge bodies). Rows of ANY other `kind` are accepted and stay
  lexically searchable, but get **no vector** and are not counted as outstanding
  work, so readiness still reaches `Ready`. This is **not** an error condition:
  the write is not rejected, no typed error is raised, and there is no verb to
  ask about it — it is the same treatment those kinds had before Slice 20c.
- **Idempotent.** Re-applying an already-satisfied declaration re-opens nothing,
  rewinds no watermark, and re-embeds nothing (`ProjectionDelta::unchanged`).
- **Dropping the last `searchable→vector` declaration turns the dense arm back
  off** (fix-1). The `drop` un-enrols the node kinds the declaration enrolled, so
  subsequent writes of those kinds enqueue no embed and `drain` no longer waits on
  them. It **deletes no embedding**: vectors already at rest survive, exactly as
  they always have across a `drop`. Re-declaring re-enrols and backfills, so a
  row written while the arm was off is picked up, not stranded. Edge-body vectors
  are unaffected — the `edge_fact` kind is registered off the presence of an edge
  body, not off the projection registry.
- **The dense arm requires the `searchable` ROLE, not merely the `vector`
  sub-object** (0.8.20 Slice 21c, `TC-71`). A spec such as
  `{ roles: {Filterable}, vector: Some(_) }` is accepted and round-trips
  verbatim, but it is **INERT**: it enrols no kind, backfills nothing, and makes
  no later write enqueue an embedding — the accept-inert ruling above, now
  honoured by the engine and not only by the bindings. Previously the engine keyed
  the dense arm off the stored `vector` sub-object alone, so declaring that
  combination in a session with a live embedder silently embedded the whole
  corpus. **The inverse moves with it:** demoting the last `searchable→vector`
  projection to `{filterable} + vector`, or dropping it while an inert
  `{filterable} + vector` sibling survives, now un-enrols exactly as a literal
  drop does. `ProjectionDelta.deferred` still reports the stored-but-unbuilt
  `vector` sub-object however it was declared — the change is to what the engine
  DOES, not to what it reports.
- **Graceful-absent without a live embedder.** Opened with `EmbedderChoice::None`
  there is no dense arm, so the declaration persists and DEFERS rather than
  queueing embeds that could only fail; it **grafts on** when the same spec is
  re-applied in a session that has an embedder — the same Q6a contract as
  `rankable`.
- **…but graceful-absent stops at the enrolment boundary** (fix-4). Once a kind
  IS enrolled — i.e. some earlier session DID have an embedder — a write of that
  kind is dense work the workspace has committed to, and a session with no
  embedder cannot make it go away. Such a write is **accepted** and stays
  lexically searchable, but it stays **outstanding**: `dense_readiness` reads
  `Embedding` and `drain` returns `EngineError::Scheduler` for the rest of that
  session, however long you wait. It is **not** lost — no failure is recorded and
  no terminal is written, so the next session opened WITH an embedder embeds it
  through the ordinary scheduler, with no re-apply and no operator `rebuild`.
  Callers who write to an enrolled corpus without an embedder should therefore
  expect `drain` to time out and should not treat that as data loss. (Reporting
  `Ready` there instead would be a torn `ready`-without-vector — the silent miss
  this slice exists to eliminate.)
- **`drain` remains bounded**, returning the typed timeout error rather than
  blocking; a caller sizes `timeout_ms` for the backfill it just asked for.

**Attribute filters on `SearchFilter` (0.8.20 Slice 15e, R-20-PR / ADR-0.8.11 D3).**
`SearchFilter` gains a public field `attributes: Vec<(String, String)>` — each
`(attribute_name, value)` is an equality predicate over a declared-`filterable`
projection, lowered pre-KNN into the indexed vec0 metadata column `attr_<hex>`
(never a post-KNN `json_extract`). Empty ⇒ the byte-identical unfiltered path is
preserved. The struct is now `#[non_exhaustive]`, so EXTERNAL crates must
construct it through `..Default::default()` (a further additive field is then not
a source break). Allowlist MEMBERSHIP in
`src/conformance/governed-surface-allowlist.json` is byte-unchanged (`SearchFilter`
was already a re-exported type; this is a fields-only + attribute delta, the same
pattern as `PreparedWrite::Node`'s validity fields). Semantics are **node-scoped**:
an attribute filter EXCLUDES every edge hit on both retrieval arms (edges are never
attribute-projected), which is HITL ruling (A) — `(A)` is `(D)` endpoint-node
filtering with an empty endpoint rule; (B)/(C)/(D) are reserved widenings, none
implemented in 0.8.20. **This whole delta is PROPOSED, NOT SIGNED.** `attributes`
stays engine-internal in 0.8.20 — there is NO Py/TS wire exposure (that is a later
slice), so `SearchFilterInput` / the Python `SearchFilter` binding input are
unchanged.

## Errors

Rust exposes typed open/runtime errors without message parsing:

- `EngineOpenError`
- `EngineError`

Canonical leaf mapping lives in `design/errors.md`. This file adopts those
types without renaming them.

## Recovery / operator seam re-exports

The `fathomdb` facade re-exports the following recovery and reporting types
from `fathomdb-engine` so that `fathomdb-cli` (the only public consumer of
these types) compiles against the public Rust surface, not engine internals.
These are CLI-only ergonomic types; they are NOT exposed as runtime SDK
verbs (recovery remains CLI-only — see Non-presence below). **Since Slice 27
fix-1 these 20 re-exports — and the `Engine` methods that produce them — are
gated behind the `operator` cargo feature** (which `fathomdb-cli` enables), so
they are absent from the default facade surface (see § Method-level boundary).

Re-exported types (canonical spellings, locked 2026-05-12; extended
2026-05-15):

- `CheckIntegrityOpts`
- `IntegrityReport`
- `SafeExportArtifact`
- `TraceReport`
- `TraceEvent`
- `RebuildReport`
- `RebuildKind`
- `ExciseReport`
- `VerifyEmbedderReport`
- `VerifyEmbedderStatus`
- `DumpSchemaReport`
- `SchemaObject`
- `DumpRowCountsReport`
- `TableRowCount`
- `DumpProfileReport`
- `TruncateWalReport`
- `TruncateWalStatus`

Engine methods backing these types are owned by `design/recovery.md` and
listed in `dev/plans/0.6.0-implementation.md` (Phase 10a + Phase 10b-A).
`PurgeLogicalIdReport` and `RestoreLogicalIdReport` were originally
forward-referenced for Phase 10b-B; both verbs are deferred to 0.8.0
(originally 0.7.x per ADR-0.6.0-cli-scope 2026-05-16 amendment;
re-targeted to 0.8.0 per HITL 2026-05-24 — see `dev/roadmap/0.8.0.md`
and the deferral note in `design/recovery.md § Logical-id purge and
restore`). When 0.8.0 re-opens the scope these types land here per
the same re-export rule.

## Non-presence

The Rust runtime surface does not expose recovery verbs. Recovery remains CLI
only per `design/recovery.md` and `design/bindings.md`. The re-exported
recovery types above are present as compile-time symbols for `fathomdb-cli`;
the runtime `Engine` does NOT gain corresponding SDK methods.
