---
title: Rust Public Interface
date: 2026-07-29
target_release: 0.8.21
desc: Public Rust surface (traits, functions, types, errors) for 0.8.21
blast_radius: src/rust/crates/fathomdb; design/engine.md; design/bindings.md; design/errors.md; design/lifecycle.md
status: locked
---

# Rust Interface

This file owns Rust-visible symbol spelling and result shape. Cross-binding
parity rules remain owned by `design/bindings.md`.

## Support posture

The Rust facade is the stable public Rust contract and is the
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
  `BoundaryCrossing`) — **HITL-SIGNED 2026-07-29 (steward `seq-157`)**, recorded
  in `src/conformance/governed-surface-allowlist.json`. `ReadView` is threaded as a
  PARAMETER on the five existing read verbs (`read_get`, `read_get_many`,
  `read_list`, `read_list_filter`, `graph_neighbors`) rather than shipped as five
  `*_with_view` sibling verbs, which is what keeps the delta at two TYPES and zero
  new verb names; `ReadView::default()` is the strict view and reproduces the
  pre-slice read semantics exactly. 0.8.20 Slices 5c/5d (R-20-E3 / R-20-E4) add
  the erasure types `SourceId` and `ExciseReport` (the latter moved out of the
  operator-gated block — it is `erase_source`'s return type) — **HITL-SIGNED
  2026-07-29 (steward `seq-157`)**. Each of those 33 resolves through the facade
  at compile time (`type_name::<…>()`). The facade ALSO `pub use`s two further
  additive groups that are **not yet members of the const**, and their sign-off
  status DIFFERS — do not read one marker across both. The Slice 15d (R-20-PR)
  projection-registry types `ProjectionSpec`, `ProjectionRole` and
  `ProjectionDelta` are **HITL-SIGNED 2026-07-29 (steward `seq-157`)** — the
  signed delta names exactly those three. Their sub-object types
  `ProjectionFts` and `ProjectionVector` are not members of that `seq-157`
  governed-command signature. `DenseReadiness`'s closed vocabulary is instead
  **HITL-SIGNED 2026-08-07 (steward `seq-246`)** by the F5/C1 record and its
  governed-surface signature. Caller input is accept-inert; Slice 21 runtime
  selection makes `read_projections` select `Unavailable` for a no-usable-
  runtime session.
  See § "Projection registry" below. The recovery /
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
- `Engine::search(...) -> Result<SearchResult, EngineError>` — defaults to 10 ranked hits.
- `Engine::search_with_limit(query, limit) -> Result<SearchResult, EngineError>` — `limit` is
  `1..=100`; out-of-range values return `EngineError::InvalidArgument`.
- `Engine::search_explained(...) -> Result<SearchResult, EngineError>` — 0.8.8
  EXP-OBS: same retrieval as `search_reranked`, additionally returning the opt-in
  `Explanation` sidecar (`SearchResult.explanation`); default paths are unchanged.
- `Engine::search_text_only(&self, query: &str) -> Result<SearchResult, EngineError>` — default 10.
- `Engine::search_text_only_with_limit(query, limit) -> Result<SearchResult, EngineError>`
- `Engine::close(...) -> Result<(), EngineError>`

### Read verbs

The canonical-row reads take a trailing `&ReadView` (`ReadView::default()` is
the strict view); the op-store reads do not — they page an append-only log that
has no existence or validity axis.

- `Engine::read_get`, `Engine::read_get_many` — `&ReadView`
- `Engine::read_list`, `Engine::read_list_filter` — `&ReadView`
- `Engine::read_collection`, `Engine::read_mutations` — `(collection, after_id,
  limit)`, no `ReadView`
- `Engine::crossed_boundary_since(&self, since: i64, view: &ReadView) ->
  Result<Vec<BoundaryCrossing>, EngineError>` — 0.8.20 Slice 10b (R-20-NV):
  which nodes entered or left validity since instant `since` (INTEGER epoch
  seconds). The one net-new command in that delta, because no existing read
  verb can express the question.

### Graph verbs

- `Engine::graph_neighbors(&self, ..., view: &ReadView) -> Result<Vec<NodeRecord>, EngineError>`
- `Engine::search_expand(&self, query: &str, filter: Option<SearchFilter>, depth: u32) ->
  Result<SearchExpandResult, EngineError>`
- `Engine::search_expand_with_limit(query, filter, depth, limit) -> Result<SearchExpandResult,
  EngineError>` — limits only initial ranked `search_hits`; expansion remains capped at 50 per root.

### Lifecycle + erasure verbs (0.8.19 Slice 10 / 0.8.20 Slice 5d)

**Governed, allowlisted, and present in all three bindings.** They are not
recovery verbs — none carries a REQ-054 denylist name — and they are on the
DEFAULT facade, not behind the `operator` feature.

- `Engine::transition(&self, logical_id: &str, to_state: LifecycleState, reason:
  Option<String>) -> Result<(), EngineError>` — move a GOVERNED node between
  existence states per the engine-enforced legal-transition table
  (`pending→active` promote, `pending→deleted` REJECTED, `active→deleted`
  soft-delete, `deleted→active` undelete). Promote/undelete CLEAR `reason`;
  reject/soft-delete SET it. `reason` is advisory and never engine-interpreted.
  Keys on the bare `logical_id` — the `l:` id space ONLY; any other space is
  `EngineError::NotLifecycleAddressable`. An illegal move is
  `EngineError::IllegalTransition { from_state, to_state, legal }`.
- `Engine::purge(&self, logical_id: &str) -> Result<(), EngineError>` —
  irreversible hard-erase of a governed node across every row-owned target (all
  versions, FTS/vector shadows, touching edges cascade-removed). DELETED-FIRST
  (legal only from `deleted`) and IDEMPOTENT. A SEPARATE verb from `transition`.
  **There is no restore counterpart on any surface.**
- `Engine::erase_source(&self, source_id: &str) -> Result<ExciseReport, EngineError>`
  — 0.8.20 Slice 5d (R-20-E4): erase every canonical row carrying `source_id`
  plus its row-owned projections, then finish the erasure AT REST (telemetry
  redaction + WAL truncating checkpoint). The COMPANION to `purge`: `purge`
  addresses a governed node by `logical_id`, `erase_source` addresses
  ANONYMOUS content (rows with no `logical_id`) by provenance, which `purge`
  cannot reach at all. Idempotent. Refuses an empty / whitespace-only /
  reserved (`_`-prefixed) id with `EngineError::WriteValidation`; the reserved
  namespace stays reachable only through the CLI `--excise-source` seam.
  Reports `EngineError::ErasureIncomplete` rather than success when the
  at-rest step could not complete.

### Projection-registry verbs

- `Engine::configure_projections(...) -> Result<ProjectionDelta, EngineError>`
- `Engine::read_projections() -> Result<Vec<ProjectionSpec>, EngineError>`

See § "Projection registry" below.

`OpenedEngine` contains:

- `engine`
- `report`

`report` is the `OpenReport` owned by `design/engine.md`.

### `OpenReport::dense_disabled` and the cached equivalence verdict (0.8.20 Slice 22, TC-68)

`OpenReport::dense_disabled` / `Engine::dense_disabled()` still mean "the dense arm
is refusing", and `EngineError::VectorEquivalenceMismatch` plus the
`Engine::search_text_only` fallback are unchanged. **What changed is when the check
behind them runs.** The 0.8.18 vector-equivalence self-check used to re-embed its
45 probes on *every* open; since 0.8.20 the engine caches that verdict against a
fingerprint of the embedder identity, the pinned `mean_vec`, the probe fixture,
both divergence floors and the stored reference baseline. An open whose fingerprint
is unchanged does **zero** probe embeds and reuses the previous verdict. The
fingerprint is held in one internal marker row — no `SCHEMA_VERSION` bump, no
migration step, no new public surface.

Read `dense_disabled` accordingly: it reports the arm's status **as verified at the
last open whose fingerprint differed**, not a fresh re-verification at this open. A
same-identity backend drift (candle CPU↔CUDA, a rebuilt library or driver) is
therefore no longer caught per-open — the ruled trade, with the residual and the
rejected mitigations written up in
`dev/design/0.8.20-tc68-equivalence-probe-fingerprint-cache.md`. An identity
*change* is unaffected: `EngineOpenError::EmbedderIdentityMismatch` /
`EmbedderDimensionMismatch` still refuse the open ahead of any cache. An unreadable
or absent cached verdict runs the probe rather than trusting it, a failing verdict
is never cached, and a divergence found on a re-run still yields
`dense_disabled = true` (`R-VEQ-4`, unchanged).

**Scope.** `R-VEQ-4` is a guarantee about **accident** — corruption, truncation, a
half-written or pre-cache workspace. The self-check is **not** tamper evidence: an
actor with write access to the database file can rewrite `_fathomdb_embed_probe`'s
stored references, which defeats the check even when it runs all 45 embeds — exactly
as it did before the cache existed — or the cached marker, or the corpus and its
vectors outright. Nothing at rest is authenticated. Do not read `dense_disabled` as
a tamper signal. **The cache does make one of those routes cheaper**, and that is
conceded rather than argued away: forging the marker needs only a publicly
computable digest, where re-authoring the baseline needs the other backend's 45
exact embeddings. Its bound is the residual above — a same-identity drift is already
served off an honestly recorded marker, no forgery required. Threat model, the
measurement, the concession and the bound: §8.3–§8.5 of
`dev/design/0.8.20-tc68-equivalence-probe-fingerprint-cache.md`.

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

- `WriteReceipt` exposes `cursor` (the batch high-water `write_cursor`),
  `row_cursors` (per-row, 1:1 with the batch) and `dangling_edge_endpoints`
- `SearchResult` exposes `projection_cursor`, which names the terminal
  projection-visible point for the search snapshot
- hybrid fallback, when present, exposes a typed branch enum whose values are
  owned by `design/retrieval.md`
- counter/profile/stress payload shapes are owned by `design/lifecycle.md`

### `SearchHit.id` is `IdSpace` (C-2, 0.8.19 / TC-8)

`SearchHit::id` is **`IdSpace`**, not the pre-0.8.19 `write_cursor: u64`. This
is the single largest consumer-visible break in the 0.8.9 → 0.8.20 span, and it
is part of the HITL-SIGNED 0.8.19 Slice-10 delta (the allowlist `_comment`
records the `SearchHit.id` `u64 → IdSpace` retype together with the
`IdSpace`/`IdSpaceKind` types).

- `IdSpace { space: IdSpaceKind, value: String }` — `value` is the BARE
  (prefix-stripped) id; `IdSpace::to_prefixed()` (`{prefix}{value}`) reproduces
  the pre-swap `stable_id` byte-for-byte, which is what made the swap a
  real-gold-keying no-op.
- Governed hits are `l:`, doc-seeded hits `h:`, synthetic passages `p:`.
- **This is the PERMANENT caller-facing identity, not an interim carrier.** The
  older "interim identity carrier, swaps to `logical_id` at G0" framing is
  superseded and must not be restated.
- `SearchHit::write_cursor: u64` survives as ENGINE-INTERNAL positional
  book-keeping (vector rowid mapping, `state='active'` lookups, RRF reweight
  keys, telemetry keying, `search_expand` re-resolution). It is reassigned on
  re-projection, is not cross-session stable, and **the Python/TypeScript
  bindings do NOT surface it**.
- Only the `logical` space is lifecycle-addressable: `transition` / `purge`
  refuse any other space with `EngineError::NotLifecycleAddressable`.

⚠ **Known Rust-facade gap (TC-39 class, documentation-only here).**
`SearchHit`, `IdSpace` and `IdSpaceKind` are **not** re-exported by the
`fathomdb` facade and are not members of `GOVERNED_SURFACE_ALLOWLIST`
(`grep SearchHit fathomdb/src/lib.rs` returns nothing). `SearchResult` IS
re-exported, so a facade consumer can *reach* the values by field access
(`result.results[0].id.value`) but cannot NAME the types — it cannot write a
function signature over a hit, match on `IdSpaceKind`, or call
`IdSpace::to_prefixed`. The Python and TypeScript bindings do surface both.
This is recorded, not fixed, by 0.8.20 Slice 39: adding a facade re-export is a
governed-surface delta and needs its own slice plus a HITL signature.

`SearchHit::source_id: Option<String>` carries the hit's source-document
provenance — the identifier `Engine::erase_source` consumes — and since TC-31
(0.8.20) it is populated on EVERY hit path, not just the graph arm.

## Caller-supplied write shapes

`PreparedWrite` is the caller-supplied input to `Engine::write` and is itself
governed surface (§ P1), so adding a variant field changes what every binding
must accept. It is `#[non_exhaustive]`.

### `source_id` is STRUCTURALLY MANDATORY (0.8.20 Slice 5c, R-20-E3)

`PreparedWrite::Node` and `PreparedWrite::Edge` both carry
**`source_id: SourceId`** — a newtype, not `Option<String>`. The change is a
TYPE change rather than a validation check on purpose: `Engine::erase_source`
addresses rows BY `source_id`, so a row written without one is reachable by no
erasure call, and Rust can make that state **inexpressible**. An
un-provenanced write is a COMPILE error for facade consumers (proven by
`fathomdb/tests/ui/`), not a runtime rejection.

- `SourceId::new(id: impl Into<String>) -> Result<SourceId, EngineError>` is
  the ONLY public constructor. It refuses an empty / whitespace-only id and any
  id in the engine's reserved `_`-prefixed namespace
  (`SourceId::ENGINE_PREFIX` = `"_engine:"`, `SourceId::LEGACY_PRE_0_8_20` =
  `"_legacy:pre-0.8.20"`), both with `EngineError::WriteValidation`.
- `SourceId` is re-exported on the facade and is part of the HITL-SIGNED
  Slice-5d allowlist delta. It MUST stay re-exported: without the constructor a
  facade consumer could not perform a canonical write at all.
- The bindings have no such type system at the boundary, so Python raises and
  TypeScript throws `WriteValidation` for a missing/empty/reserved id — the
  same rule, enforced at the binding.
- Policy note carried to the publish-facing docs: **`source_id` must not
  contain personal data.** It is echoed on every `SearchHit` and recorded in
  the retention-EXEMPT erasure-audit row, so it outlives the rows it names.

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
same way since Slice 30. The fields-only delta is **HITL-SIGNED 2026-07-29
(steward `seq-157`)**.

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
- `Engine::search_view_with_limit(query, &ReadView, limit) -> Result<SearchResult, EngineError>`
- `Engine::search_reranked_view_with_limit(query, filter, rerank_depth, use_graph_arm, alpha,
  pool_n, explain, &ReadView, limit) -> Result<SearchResult, EngineError>`
- `Engine::search_text_only_view_with_limit(query, &ReadView, limit) -> Result<SearchResult,
  EngineError>`

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

**Status: HITL-SIGNED 2026-07-29 (steward `seq-157`)** for the two net-new
commands (`configure_projections`, `read.projections`), the types
`ProjectionSpec` / `ProjectionRole` / `ProjectionDelta`, and the typed
`ProjectionDestructiveError` — all recorded in
`src/conformance/governed-surface-allowlist.json`. **AC-079 remains UNMINTED**
(it mints at Slice 40); the signature is pinned to that file's content, so any
diff re-opens the gate (T1e pin).

⚠ **`ProjectionFts` and `ProjectionVector` are NOT part of that `seq-157`
signature** — neither appears in the allowlist. `DenseReadiness` is separately
**HITL-SIGNED 2026-08-07 (steward `seq-246`)** by Slice 21 F5/C1; this signs its
closed vocabulary and no new command. It does not prematurely claim that the
runtime has begun selecting `Unavailable`.

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
- `DenseReadiness` — a three-variant enum, `Unavailable`, `Embedding`, and
  `Ready`, with `as_str` / `from_str_opt` giving the
  `"unavailable" | "embedding" | "ready"` wire spellings. Slice 21's F5/C1
  ruling (`steward-ledger` seq-246) signs the vocabulary without adding a
  governed command. Caller input accepts the spelling inertly; reads select
  `Unavailable` when the session has no usable dense runtime.
- `ProjectionDelta { built, dropped, deferred, unchanged, vector_unsupported_kinds }`.
  Cheap roles (`filterable`, `searchable→FTS`) build same-transaction; `rankable`
  and the `searchable→vector` sub-target are persisted-but-deferred (reported in
  `deferred`, never an error).
  `vector_unsupported_kinds: Vec<String>` was added additively by 0.8.20 Slice 22
  (R-20-VC, `TC-67`); see below.
- **`ProjectionDelta::vector_unsupported_kinds` — node KINDS, not attribute
  names** (0.8.20 Slice 22, `TC-67`). The first three vectors list projection
  attribute names; this one lists the vector-eligible node **kinds** present in
  the corpus that the vector writer can **never** commit, so no
  `searchable→vector` declaration will ever produce an embedding for them. Those
  rows stay fully FTS/lexically searchable. It exists because `deferred` alone
  could not distinguish *transient* ("no embedder attached this session, or an
  embed still in flight") from *permanent* ("outside the locked `source_type`
  vocabulary — never, in any session"): both produced the same `deferred` entry.
  Sorted, de-duplicated, and **empty rather than absent**.
  - It is a **STATE report, not a diff**: it does not feed `unchanged`, so an
    idempotent re-apply returns `unchanged: true` with the three diff vectors
    empty **and the report populated**.
  - It is **embedder-independent** — reported identically with and without a live
    embedder, because the vocabulary is static. Do not read it as the deferral.
  - It is **OUTPUT-ONLY**: `configure_projections` takes `&[ProjectionSpec]`, so
    the field has no inbound direction and cannot affect the `read_projections` →
    `configure_projections` round-trip.
  - **Residual — it is computed at DECLARE time.** A non-committable kind written
    *after* the call is not in a delta the caller already holds. To refresh it,
    re-apply the same spec: an idempotent no-op that returns a current report.
  - It is **not an error and not a readiness change**: with a usable dense
    runtime, readiness still reaches `Ready` (see the enrolment bullet below),
    nothing is rejected, and no `projection_failures` row is written. Without
    a usable runtime, runtime selection remains `Unavailable`.

**Spec-validation reject — an `fts`/`vector` sub-object REQUIRES the `searchable`
role (0.8.20 Slice 23, `R-20-SV`).** ⚠ **BREAKING.**
`Engine::configure_projections` REFUSES a `ProjectionSpec` that carries
`fts: Some(_)` or `vector: Some(_)` while `roles` does not contain
`ProjectionRole::Searchable`. The error is **`EngineError::WriteValidation`** —
the decision-#18 write-SHAPE family — surfaced as `WriteValidationError` in
Python and as code `FDB_WRITE_VALIDATION` (message `"write validation error"`,
`data: null`) in TypeScript.

- **Why.** `searchable→FTS` and `searchable→vector` are TIER LABELS, not roles:
  the sub-objects SELECT a sub-target of `searchable` and do not CONFER one. Both
  engine build predicates are conjunctions with the role, so without it the
  declaration builds no property-FTS, enrols no kind and embeds nothing — a
  meaningless config. HITL ruling 2026-07-24 (`dev/plans/plan-0.8.20.md` §11
  item 4, option (b) REJECT).
- **This SUPERSEDES the 0.8.20 Slice 15d fix-4 accept-and-round-trip position**
  and the "accept-inert ruling" this document previously cited as precedent
  (below). Nothing else about that fix changed.
- **Keyed on the ABSENCE of `searchable` alone.** `Filterable` / `Rankable` are
  orthogonal axes; neither supplies nor substitutes for the role, so every
  non-`searchable` role set is refused identically.
- **A rejected request is a TOTAL no-op.** Validation runs before any write, so a
  single invalid spec anywhere in `specs` aborts the whole call: valid siblings
  are not registered and `drop` entries do not apply.
- **`read_projections` is UNAFFECTED** — it is a pure read and rejects nothing.
- **Migration for the LEGACY population.** Databases that declared the shape
  while the engine accepted it still hold it at rest. Those rows still READ back
  verbatim, but the `read_projections` → `configure_projections` round-trip no
  longer closes for them: re-applying raises, and re-declaring only the valid
  half is refused as `ProjectionDestructive` (it removes the stored sub-object).
  The two remedies are **add the `searchable` role** (non-destructive, accepted)
  or **name the projection in `drop`**.
- **Known diagnostic cost (TC-95/TC-98, HITL-deferred).** `WriteValidation` is a
  UNIT variant, so with a LIST of specs the refusal cannot name WHICH spec was
  invalid. See `dev/design/errors.md`.

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

- **`DenseReadiness` has exactly three variants**, `Unavailable`, `Embedding`,
  and `Ready`, wire spellings `"unavailable"` / `"embedding"` / `"ready"`.
  The signed target meaning is: `Unavailable` for no usable dense runtime
  (absent or equivalence-refused), `Embedding` for a usable runtime with
  eligible outstanding work, and `Ready` for usable, quiescent work.
  **`pending` is RESERVED for the orthogonal ADMISSION axis** (quarantine/trust
  — an app judgment) and is deliberately never an index-readiness value.
  `from_str_opt("pending")` is `None`.
- **Runtime selection.** `read_projections` first applies one
  usable-dense-runtime predicate: no runtime or an equivalence refusal yields
  `Unavailable`. With a usable runtime it derives `Embedding` / `Ready` from
  the shared outstanding-work predicate. This adds no schema step or stored
  readiness field.
- **ACCEPT-INERT on the way in.** `Engine::configure_projections` neither stores
  nor honours a caller-supplied `dense_readiness` (`StoredProjection::from_spec`
  reads only `embedder`), so `read_projections` output re-applies as a no-op —
  the shipped read→configure round-trip.
  **⚠ 0.8.20 Slice 23 (`R-20-SV`) correction (TC-39 class):** this bullet used to
  cite "the accept-inert ruling on an `fts`/`vector` sub-object declared without
  the `searchable` role" as its precedent. **That ruling is OVERRULED** — the
  HITL ruled the shape an INVALID SPEC on 2026-07-24 and Slice 23 rejects it (see
  the spec-validation reject above). `dense_readiness` accept-inert stands on its
  OWN footing and is unchanged: it is engine-set READ METADATA, not part of the
  declaration, and the round-trip it protects is still live.
- **The BINDINGS hard-reject** the two shapes that could never round-trip: a
  readiness supplied with `vector = false`, and any spelling outside
  `{unavailable, embedding, ready}` (notably `pending`, and the empty string).
  Both reuse the
  EXISTING `EngineError::InvalidArgument` / `InvalidArgumentError` /
  `FDB_INVALID_ARGUMENT` — **no new error type is minted.** A declared readiness
  never changes what the engine reports.
- **Additive.** Callers who never look at readiness see identical behaviour.
  The vocabulary is **HITL-SIGNED 2026-08-07 (steward `seq-246`)**; runtime
  selection is derived rather than durable.

**`drain` is the flush-to-readiness barrier (0.8.20 Slice 20c, R-20-DR /
`api-surface.md` C4).** There is **no `flush_embeddings()` verb** — the shipped
`Engine::drain(timeout_ms)` carries those semantics, so the surface gains ZERO
net-new governed commands (TC-55 = INSTRUMENTATION). The pinned invariant, tested
in Rust, Python and TypeScript:

> With a usable dense runtime, `drain(timeout)` returning `Ok(())` ⟹
> `dense_readiness == Ready`,
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
  work, so readiness reaches `Ready` only with a usable dense runtime. An
  absent or equivalence-refused runtime instead selects `Unavailable`. This is
  **not** an error condition: the write is not rejected and no typed error is
  raised.
  **Since 0.8.20 Slice 22 (`TC-67`) it is no longer silent, either:** the excluded
  kinds are named in `ProjectionDelta::vector_unsupported_kinds`. Only the
  reporting changed — the exclusion, the readiness semantics and the absence of an
  error are exactly as Slice 20c left them.
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
  `{ roles: {Filterable}, vector: Some(_) }` is **REJECTED since 0.8.20 Slice 23**
  (`R-20-SV`, above); until then it was accepted and round-tripped verbatim while
  being **INERT**: it enrolled no kind, backfilled nothing, and made no later
  write enqueue an embedding. The inertness still governs the LEGACY population
  that holds the shape at rest. Previously the engine keyed
  the dense arm off the stored `vector` sub-object alone, so declaring that
  combination in a session with a live embedder silently embedded the whole
  corpus. **The inverse moves with it:** demoting the last `searchable→vector`
  projection to `{filterable} + vector`, or dropping it while an inert
  `{filterable} + vector` sibling survives, now un-enrols exactly as a literal
  drop does. `ProjectionDelta.deferred` still reports the stored-but-unbuilt
  `vector` sub-object however it was declared — the change is to what the engine
  DOES, not to what it reports.
- **Graceful-absent without a usable dense runtime.** The declaration persists
  and DEFERS rather than queueing unsafe work. A later safe open atomically
  grafts eligible durable work after identity and equivalence acceptance;
  idempotent re-apply remains a repair door.
- **…but graceful-absent stops at the enrolment boundary** (fix-4). Once a kind
  IS enrolled — i.e. some earlier session DID have an embedder — a write of that
  kind is dense work the workspace has committed to, and a session with no usable
  dense runtime cannot make it go away. Such a write is **accepted** and stays
  lexically searchable, but it stays **outstanding**: `dense_readiness` reads
  `Unavailable` and `drain` returns `EngineError::Scheduler` for the rest of that
  session, however long you wait. It is **not** lost — no failure is recorded and
  no terminal is written, so the next session opened WITH an approved runtime
  embeds it through the ordinary scheduler, with no re-apply and no operator
  `rebuild`. Callers who write to an enrolled corpus without a usable runtime
  should therefore expect `drain` to time out and should not treat that as data
  loss. (Reporting `Ready` there instead would be a torn `ready`-without-vector — the silent miss
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
was engine-internal in 0.8.20; Slice 60 exposes it through both SDK bindings.

## Nested-source projections (0.8.21 Slice 60)

`ProjectionSpec`
adds `source: Option<Vec<String>>`. Omitted preserves direct top-level lookup;
present paths are literal, safe JSON object-member segments stored durably in
the registry. Missing/null terminals create no derived row. Object/array
terminals reject both configuration backfill and normal writes atomically with
`EngineError::WriteValidation`.

`SearchFilter.attributes: Vec<(String, String)>` is portable public API: an AND
of equality predicates over declared `Filterable` projections. Values use the
existing canonical text representation, intentionally so string `"1"` and number
`1` compare equal. `Filter::try_from(&SearchFilter)` rejects a non-empty
attribute list with `EngineError::InvalidFilter`; reverse lowering has no
attributes.

On `search_explained` with attribute predicates, `Explanation.trace.dropped_edge_hits`
reports edge-FTS candidates rejected solely by the
node-scoped attribute rule. Default non-explained searches do not collect the
counter or incur its extra comparison.

- `Engine::search_projected_text(query, name, filter, &ReadView) ->
  Result<SearchResult, EngineError>` searches exactly one declared
  `Searchable` property-FTS projection. It applies metadata, validity, and
  attribute filters; orders by ascending FTS5 `bm25` then write cursor; returns
  `branch=Text`, `soft_fallback=None`, and no explanation. It never body-scans,
  embeds, or fuses with body/vector retrieval.
- `Engine::search_projected_text_with_limit(query, name, filter, &ReadView, limit) ->
  Result<SearchResult, EngineError>` has the same semantics with an explicit `1..=100` limit.

## Ranked result limits (0.8.22 Slice 18)

Every ranked search family defaults to 10 returned hits. The explicit `*_with_limit`
forms accept only `1..=100`; an out-of-range value returns `EngineError::InvalidArgument`
and is never silently clamped. The hybrid family also exposes
`search_filtered_with_limit`, `search_filter_with_limit`, `search_reranked_with_limit`, and
`search_explained_with_limit`. Vector candidate fanout is at least the requested limit; the
private test seam can only raise candidate fanout, never public result cardinality.
`graph_neighbors` remains separately capped at 50.

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

⚠ **`PurgeLogicalIdReport` / `RestoreLogicalIdReport` — still not shipped, and
NOT the same thing as `Engine::purge`.** These two report types were
forward-referenced for a CLI `purge-logical-id` / `restore-logical-id` pair
(Phase 10b-B, deferred through 0.7.x then 0.8.0). Neither the CLI verbs nor
the report types exist as of 0.8.20, and neither is planned:

- Logical-id lifecycle landed instead as the **governed SDK** verbs
  `Engine::transition` and `Engine::purge` (0.8.19 Slice 10, HITL-SIGNED). They
  are on the DEFAULT facade, return `Result<(), EngineError>`, and produce no
  report type — so they consume neither of the names above.
- **There is no restore counterpart on any surface.** `purge` is irreversible
  by construction; `restore` is also one of the five REQ-054 recovery-denylist
  names, so it can never be an SDK verb.

Do not read the forward reference above as a scheduled deliverable.

## Non-presence

The Rust runtime surface does not expose recovery verbs. Recovery remains CLI
only per `design/recovery.md` and `design/bindings.md`. The re-exported
recovery types above are present as compile-time symbols for `fathomdb-cli`;
the runtime `Engine` does NOT gain corresponding SDK methods.
