# FathomDB 0.8.x Agent-Memory Implementation Strategy (head-of-main 0.7.2)

> How to implement the G0–G12 agent-memory gaps by leveraging FathomDB's existing
> 0.7.2 mechanisms. Produced by a map → design → adversarial-verify → synthesize
> workflow (6 subsystem maps, 13 gap designs each adversarially re-checked against
> source). Companion to [`0.8.0-agent-memory-fit.md`](./0.8.0-agent-memory-fit.md) (the gap
> ladder) and [`../adr/ADR-0.8.0-agent-memory-retrieval-and-identity.md`](../adr/ADR-0.8.0-agent-memory-retrieval-and-identity.md)
> (the classification + identity-shape ADR).
>
> Every file:line below was cited by a grounded reader and re-verified by an
> adversarial pass; where the first design was wrong, the **corrected approach**
> is what's recorded here.
>
> **Scope note (v0.5.x triage, 2026-06-01):** the add/defer/drop decisions and
> consumer-importance for each gap are consolidated in
> [`0.8.0-v05-feature-triage.md`](./0.8.0-v05-feature-triage.md). In short: G1
> (Slice A), G0 (Slice C), G8 (Slice D) + the new **G2-read** and **G3-read**
> slices below are **ADD-0.8.0**; G5/G6 (Slice H), G4, and confidence (an F9
> variant of Slice F) are **DEFER-0.8.x**; G2/G3/G4/G5/G7 SDK verbs are gated on
> the surface-supersession ADR (now "yes, under governance" rather than "blocked
> on HITL"). OpenClaw/Hermes need **no** graph — do not let them pressure G5/G6.

## 1. Engineering thesis

The ladder G0–G12 is **overwhelmingly a leverage exercise, not greenfield**.
Every gap rides one of four mechanisms that already exist in 0.7.2:

- the per-row write path (`commit_batch`, single transaction, per-row cursor),
- the reader-pool snapshot read path (`read_search_in_tx` + `ReaderWorkerPool`),
- the vec0 metadata/partition columns on `vector_default`,
- the op-store tables with their existing `_for_test` read seams.

The work divides on two axes: *additive-over-existing* vs *needs-new-schema/
surface*, and *AC-057a-clean* vs *read-surface verb (now **governed under
ADR-0.8.0-supersede-five-verb-surface-cap**, not blocked — see below)*.

**Pure-additive over existing mechanisms (no AC-057a interaction, no HITL gate):**
- **G1** structured hits — enriches the existing `search` return payload.
- **G9** RRF fusion — pure-internal rewrite of the `read_search_in_tx` merge.
- **G10** filtered KNN — optional `filter` arg on the existing `search` verb
  (AC-057a explicitly permits evolving the search verb's args) + a Rust vec0
  reshape (no SQL migration).
- **G12-recency** — surface the already-present recency signal into G9's rerank.
- **G8** edge referential check — write-path validation + an additive receipt field.
- **G11** bi-temporal edges — **ships no code at 0.7.x**; it is a *design
  constraint* on the deferred G0 substrate, deliverable = an ADR.

**Needs new schema (additive ALTER + index; no-data-migration honored):**
- **G0** record identity — the keystone: `logical_id` + `superseded_at` columns +
  partial unique index on `canonical_nodes`/`canonical_edges`, plus per-row ids in
  `WriteReceipt`.
- **G4** filtered list — one additive **index-only** migration `canonical_nodes(kind)`.
- **G5** graph walk — one additive **index-only** migration
  `canonical_edges(from_id)/(to_id)`.
- **G12-importance** — an additive `importance REAL` *vec0* column (Rust reshape).

**Governed read-surface verbs** — **G2** (`read.get`), **G3**
(`read.collection`), **G4** (`read.list`), **G5** (`read.neighbors`), **G7**
(`read.history`). All introduce *new read surface*, and that surface is **no
longer blocked**: `ADR-0.8.0-supersede-five-verb-surface-cap` retires the
five-verb cap and replaces it with a **governed** surface (SDK parity +
recovery-name denylist + typed-boundary + `read.*` namespace). So these are
*additive under governance*, not gated on a "should we open to reads?" decision —
that answer is **yes, under governance**. What remains per verb is its **G0
dependency** + the ADR's HITL sign-off on namespace/sequencing. The earlier trap
(the verify pass flagged `admin.configure` as a standalone counted verb, not an
open namespace) is resolved by landing reads under the **new `read.*` namespace**
rather than overloading `admin.*`. **G2/G3 reads are ADD-0.8.0; G4/G5/G7 are
DEFER-0.8.x** (governance path clear, sequencing pending) — see Slice G.

**The keystone — G0 — unlocks** G1's stable id, G2's `get(id)`, G5's
node-identity walk, G7's `history(id)`, and G8's endpoint reference. Nothing in
the "stable cross-restart identity" half of the ladder is buildable until
`logical_id` + `superseded_at` exist. Per ADR-0.8.0 Option 2A, **G0's
`superseded_at` IS the transaction-time half of G11's bi-temporal model** — so
G0 and G11 must not build supersession twice.

## 2. Dependency-ordered build sequence (FathomDB slices)

Each slice is authored as a slice prompt and closed with an `output.json` closure
artifact (PR-N convention). Order: AC-057a-clean schema-free read-path work first
(de-risks recall floor + binding sprawl), the keystone schema next, then the
governed read-surface verbs (under ADR-supersede) in dependency order.

### Slice A — G1: Structured search hits *(AC-057a-clean, no migration)*
- **Migration:** none; reads columns already on disk. SCHEMA_VERSION stays 10.
- **AC-057a:** untouched — enriches the *return shape* of `search`, not a new
  verb. Still a breaking data-class change requiring **simultaneous Py + TS parity**.
- **Files:** `fathomdb-engine/src/lib.rs` (`SearchHit` struct, `SearchResult` ~819,
  `ReaderResponse` ~389, `read_search_in_tx` 3196-3312, `search_inner` 1953-1961);
  `fathomdb-py/src/lib.rs` (`PySearchHit`, `PySearchResult.results`); pure-Python
  wrapper `src/python/fathomdb/engine.py:104-116`; `fathomdb-napi/src/lib.rs:327-347`;
  TS wrapper `src/ts/src/index.ts:49-53,290`, `binding.ts:63`, regen `index.d.ts`.
- **Corrected approach (from verify):** this is a **4-layer-per-SDK** change.
  - **Derive breakage:** `SearchHit` derives `Clone, Debug, PartialEq` — **NOT `Eq`**
    (f64 forbids it). **Drop `Eq` from `SearchResult` at lib.rs:819.** Most likely
    compile failure.
  - **Score is per-branch, NOT fused.** Add `vec_distance_l2(...) AS dist` to the
    phase-2 CTE SELECT (~3231); query_map returns `(i64, f64)`; thread score into
    `SearchHit`. Carry a `branch` field so L2-distance vs text-rank scales are
    legible. True fusion is G9, not this.
  - **id type:** napi has no u64 → `SearchHit.id: i64`. Interim `id = write_cursor`;
    swaps to `logical_id` when G0 lands, no carrier reshape.
- **Behavior-compat:** preserve vector-first ordering + **dedup keyed on `body`**.
- **Test fallout (load-bearing):** rework all `.results` consumers — equality
  asserts (`projection_runtime.rs:249/275/297`, `cursors.rs:75`), body-membership
  (`perf_gates.rs:815`, `cursor_read_after_write.rs:78`, `fts5_injection_safety.rs:136`),
  and **especially the recall harnesses `eu8_ir_validation.rs:325-326` +
  `eu7_real_corpus_ac.rs:477`** that gate the **0.90 recall floor** — extract
  `hit.body` so the floor measurement is identical.
- **Effort:** S.

### Slice B — G9: RRF fusion + rerank seam *(AC-057a-clean, no migration)*
- Land adjacent to A (G1's `score` field is RRF's natural home).
- **Files:** `read_search_in_tx` merge (3275-3309); new `RRF_K: f64 = 60.0` +
  `rerank_fused(...)` seam ~3025; optional `fusion_mode: AtomicU8` on
  `ProjectionRuntimeShared` (mirror `search_limit_override`) threaded through
  `ReaderRequest::Search`; new `tests/pr_g9_rrf_fusion.rs`.
- **Approach (verify: sound):** keep both branches' rank positions; RRF
  `Σ 1/(k+rank)`, `k=60`, accumulator **keyed on body** (preserves dedup), sum
  across branches, sort desc, truncate to `final_limit`. Text branch `ORDER BY
  bm25(search_index)` — intrinsic to FTS5, **no schema change**.
- **Scope:** RRF only in first cut. **Defer MMR** (needs candidate embeddings) and
  **recency reweight** (needs uniform timestamps across both branches — text hits
  have none until G12). Stub `rerank_fused` as identity.
- **Behavior-compat:** RRF changes ordering — **deliberate, documented event** with
  a pinned acceptance test + a `Legacy` `fusion_mode` escape hatch (lock-free atomic).
- **Soft-fallback:** compute the vector-empty signal **before** collapsing branches.
- **Effort:** S.

### Slice C — G0: Canonical identity substrate *(KEYSTONE — migration, AC-057a-clean)*
- **Prerequisite:** `ADR-0.8.0-canonical-identity-substrate` drafted + HITL-signed
  (settles column shape, whether edges carry identity + temporal — ADR Q2/Q4).
- **Migration:** **step 11, SCHEMA_VERSION 10→11.** Template = migration 8:
  ```sql
  ALTER TABLE canonical_nodes ADD COLUMN logical_id TEXT;
  ALTER TABLE canonical_edges ADD COLUMN logical_id TEXT;
  ALTER TABLE canonical_nodes ADD COLUMN superseded_at INTEGER;
  ALTER TABLE canonical_edges ADD COLUMN superseded_at INTEGER;
  CREATE UNIQUE INDEX IF NOT EXISTS canonical_nodes_logical_id_kind_idx
    ON canonical_nodes(logical_id, kind) WHERE superseded_at IS NULL;
  CREATE INDEX IF NOT EXISTS canonical_nodes_logical_id_idx ON canonical_nodes(logical_id);
  -- + edge equivalents
  ```
  Pure ALTER ADD + CREATE INDEX, no DROP ⇒ **requires a
  `-- MIGRATION-ACCRETION-EXEMPTION:` marker** (`check_migration_accretion`,
  schema lib.rs:362-373; migration-8 already flagged the next pack owes the offset
  budget). **Fold G4's `canonical_nodes(kind)` and G5's `canonical_edges(from_id)/
  (to_id)` indexes into this one migration** to spend a single offset budget. Legacy
  rows back-fill NULL; NULL `logical_id` is excluded from the partial unique index
  so it never collides. Add `migrations/011_logical_id.sql` doc mirror + a
  `tests/migrations.rs` case.
- **AC-057a:** **untouched** — `logical_id` is an additive optional field on the
  existing Node/Edge dicts; per-row ids are additive `WriteReceipt` fields on the
  existing `write` verb. The by-id **read** (G2) is a separate, still-open question.
- **Files:** `fathomdb-schema/src/lib.rs` (SCHEMA_VERSION, MIGRATIONS:251-254);
  `fathomdb-engine/src/lib.rs` (`PreparedWrite::Node`/`Edge` 834-849 add
  `logical_id: Option<String>`; `validate_write` 5001-5021; `commit_batch` INSERT
  5115-5146; `WriteReceipt` 794-796 add `row_cursors`; `write_inner` 1816);
  `fathomdb-py` (`translate_node`/`translate_edge` 665-684, `PyWriteReceipt` 275-282);
  `fathomdb-napi` (`row_cursors: Vec<i64>`, per-element cast — napi has no u64).
- **Idempotent upsert:** inside the **existing `commit_batch` tx** (5108), when
  `logical_id` is `Some`: `UPDATE ... SET superseded_at = ?cursor WHERE logical_id=?
  AND kind=? AND superseded_at IS NULL` (tombstone prior) **then** INSERT new — both
  in the single tx (AC-068a/b atomicity for free). Reuses per-row cursor (5113) as
  the supersession timestamp.
- **Known open follow-on (flag explicitly):** the superseded row's
  **vec0/FTS5/`_fathomdb_vector_rows`/`_fathomdb_projection_terminal` shadow rows
  are NOT excised by this slice.** The stale vector projection then **competes for
  the TOP_K_BIT_CANDIDATES phase-1 prefilter slots** and can crowd out the current
  version. **Shadow reconciliation against `superseded_at` is a required follow-on
  slice** (model on `excise_source_inner`), not part of G0.
- **Roadmap deviation (flag, don't silently drop):** roadmap names `row_id` +
  `restore_provenance`; this reuses `write_cursor` as the per-row id and drops the
  separate `row_id`/`restore_provenance`. A substrate-ADR decision to surface.
- **Effort:** M.

### Slice D — G8: Optional edge referential check *(AC-057a-clean; no G8-owned migration)*
- **Hard dependency: G0 (Slice C)** — no node-identity key to validate against
  until `logical_id` exists. Reuses G0's `(logical_id, kind)` index.
- **Files:** `commit_batch` post-loop hook ~5194, Edge arm 5139-5146; `WriteReceipt`
  add `dangling_edge_endpoints: u64`; `fathomdb-py`/`napi` receipt parity.
- **Approach (verify: sound):** the check is **cross-row** (an edge may legally
  point at a same-batch node), so it **cannot live in `validate_write`** (sees one
  row). Place it as a **post-row-insert hook inside `commit_batch`**, where
  same-batch `logical_id`s are already on disk in the open tx. **Default = FLAG, not
  reject** (record count, commit anyway) — bulk loaders insert edges before targets;
  a hard reject is hostile. Optional strict mode rolls back before `tx.commit`.
- **Corrected approach (from verify):**
  - **Probe-vs-index:** reconcile the predicate with G0's composite index — either
    endpoints reference `logical_id` alone (needs a `logical_id`-leading non-unique
    index) or edges carry the referenced `kind` (add `AND kind=?` to hit the
    composite). Resolve under the substrate ADR before claiming index efficiency.
  - **Invariant label:** `validate_batch` runs *inside* the writer lock (1781) but
    *before* the SQLite tx (5108) — it's "validation-before-transaction," not
    "before connection.lock."
- **Effort:** S (G0 does the heavy lifting).

### Slice E — G10: Metadata-filtered KNN *(AC-057a-clean via search args; Rust vec0 reshape, no SQL migration)*
- Independent of G0; composes with G9.
- **Migration:** **no `fathomdb-schema` entry** — `vector_default` is reshaped in
  Rust (`ensure_vector_partition`), not the SQL registry; accretion guard never
  fires. `source_type`/`kind`/`created_at` **already exist as vec0 metadata columns
  (filter-capable today)**; only `status` is new.
- **AC-057a:** **safe** — additive optional `filter` arg on the existing `search`
  verb (ADR-0.8.0:199-202). Py + TS simultaneously.
- **Files:** `read_search_in_tx` phase-1 CTE (3223-3237); thread `Option<SearchFilter>`
  through `ReaderRequest::Search`/`reader_worker_loop`/`search_inner`; new
  `SearchFilter { source_type, kind, created_after, status }` (small fixed
  equality+range grammar, **not a DSL**); `create_vector_partition` 4645-4656 add
  `status TEXT`; `ensure_vector_partition` 4616-4643 reshape; `commit_projection_outcomes`
  INSERT 4035-4040; `fathomdb-py`/`napi` `search` optional `filter`.
- **Corrected approach (from verify — critical):**
  - **Filter in PHASE 1, single statement:** `WHERE embedding_bin MATCH
    vec_quantize_binary(vec_f32(?1)) AND source_type=? AND kind=? AND created_at>=?
    AND status=?` so the candidate prefilter is drawn from the matching subset
    (respects vec0 brute-force: prune via metadata, never post-fetch). **Keep
    `ORDER BY distance LIMIT {top_k}`** — do NOT add `k={top_k}` (vec0 rejects both
    forms together).
  - **Declare `status` as a plain `status TEXT` metadata column, NOT a `+status`
    aux column** — aux columns hard-error under any KNN WHERE constraint. (3 metadata
    cols vs 16 max; 1 partition vs 4 max — headroom fine.)
  - **Shape-sentinel bug (real correctness gap):** `ensure_vector_partition` (~4640)
    no-ops on `sql.contains("embedding_bin")`. A pre-`status` Pack-1 table *also*
    contains `embedding_bin`, so adding `status` would **silently never land on
    existing DBs.** Replace with a **3-way detector**: `contains("status") ⇒ no-op`;
    `contains("embedding_bin") ⇒ Pack1→Pack2 reshape` (stage cols, recreate with
    status, back-fill NULL); else `⇒ migrate_vector_partition_to_pack1`. Crash-safe
    via the existing transactional stage+drop+recreate + sidecar lock.
  - **Text-branch gap (correctness + behavior-compat):** the FTS5 branch (3294-3308)
    has no metadata filter — a "filtered" search would **leak un-filtered text hits.**
    When a filter is supplied, constrain the text branch too (join
    `search_index.write_cursor` → `canonical_nodes`/`vector_default`). When NO filter
    is supplied, **both branches emit byte-identical SQL to today** (behavior-compat).
  - **Status population unresolved:** `status` ships NULL-everywhere — plumbing
    without value until a population source (body-JSON convention or future canonical
    column) lands. `kind` and engine-time `created_at` filtering work immediately;
    `status=open` is a no-op capability meanwhile. `created_at` is *projection
    wall-clock*, not event valid-time (that needs G12).
- **Effort:** M.

### Slice F — G12: Recency/importance signals *(recency AC-057a-clean; importance vec0 reshape)*
- Lands with G9 (Slice B) for recency; importance defers until G9's scoring
  contract + G1's `score` field exist.
- **Migration:** none in `fathomdb-schema`. `importance REAL` is a vec0 reshape
  (same 3-way-sentinel as Slice E). A canonical `created_at`/`importance` ALTER is a
  deferred option (step 12, exemption marker) — **keep G12 in the vec0/projection layer.**
- **Files (recency, S):** `read_search_in_tx` phase-2 CTE (3231-3235) — **project
  `v.created_at` (on disk but NOT currently selected)**, reweight in Rust **after**
  the bit-KNN prefilter (never as a vec0 predicate — would starve recall).
  **(importance, M):** `create_vector_partition` add `importance REAL`; new
  Pack1→Pack2 reshape branch + sentinel; `commit_projection_outcomes` INSERT
  (4035-4040); **preserve importance in `run_pin_and_requantize_pass` (lib.rs:3091-3140,
  SELECT 3114 + INSERT 3125)** — transitively covers `recompute_mean_in_tx_inner`
  (~4212); add `importance` to `ProjectionOutcome::Success` (3457-3462), populate in
  `run_projection_job` (3661-3666) from `job.body` JSON.
- **Corrected approach (from verify):** the headline "already in scope of the rerank
  query" is **false** — `created_at` is JOINed but **never SELECTed**; it must be
  surfaced. `created_at` is *projection* time, reset by
  `migrate_vector_partition_to_pack1` — recency would shift after a vec0 rebuild;
  **prefer deriving recency from `write_cursor`** (monotone, never reset). Importance
  is **M, not S** (8 sites incl. the reshape branch). The `score: f32` addition
  conflicts with `SearchResult`'s `Eq` derive — coordinate with Slice A.
- **Behavior-compat:** recency reweight changes ordering — **gate behind G9's
  `fusion_mode` flag.**
- **Effort:** recency S, recency+importance M.
- **F9 confidence variant (DEFER-0.8.x; needs design-ADR + experiment):**
  per-fact confidence is **not a bare column** — store as a vec0/projection REAL
  (same 3-way-sentinel reshape as importance), populate at projection time,
  reweight in Rust **after** the bit-KNN prefilter (never a vec0 predicate), gate
  behind `fusion_mode`. Requires a design-ADR disentangling confidence (epistemic:
  evidence-driven, decays, drops on contradiction — Zep/Graphiti, Belief-Memory
  arXiv 2605.05583) from G12 importance (salience) — possibly one signal suffices
  for 0.8.x. Anchor confidence-update to G0 invalidate-not-delete. See triage F9.

### Slice G — Read-surface (GOVERNED per ADR-supersede) + read verbs

> **Status change (2026-06-01):** the five-verb cap is being superseded by a
> governed surface (`../adr/ADR-0.8.0-supersede-five-verb-surface-cap.md`). These
> verbs are no longer "blocked on a should-we-open-reads HITL" — the answer is
> "yes, under governance (parity + recovery-denylist + typed-boundary + `read.*`
> namespace)." What remains per-verb is G0 dependency + the ADR's HITL sign-off on
> namespace/sequencing. **G2-read and G3-read are now ADD-0.8.0** (below); G4/G5/G7
> are DEFER-0.8.x.

**Slice G2-read — by-id (`read.get`/`read.get_many`) — ADD-0.8.0**
- `ReaderRequest::GetById` alongside `Search` (`lib.rs:350`); point lookup on
  `canonical_nodes.logical_id` (after G0 column + partial-unique index),
  active-only default (`superseded_at IS NULL`). Reference: v0.5.x
  `builder.rs:120` `Predicate::LogicalIdEq`. Gated on **G0 + ADR-supersede
  sign-off + allowlist/parity test rewrite** (`test_surface.{py,ts}`
  set-equality → allowlist+parity). Non-destructive → NOT on the recovery
  denylist. Effort S.

**Slice G3-read — operational read-back (`read.collection`/`read.mutations`) — ADD-0.8.0 (READ)**
- `ReaderRequest::ReadCollection` + worker-loop arm + `OpStoreRow` +
  `Engine::read_collection` on the ReaderWorkerPool/DEFERRED-tx path. Engine seam
  is **dormant-shippable now**; the SDK verb is **hard-gated on ADR-supersede
  sign-off**. Use `lib.rs:2245-2297` `_for_test` SELECTs as a read-**shape** oracle
  only (columns, `ORDER BY id`, cursor) — author NEW multi-row SELECTs with
  **mandatory limit + after-id cursor** for `operational_mutations` (~1M cap).
  Reference: v0.5.x `read_operational_collection` / `trace_operational_collection`
  (`admin.rs:1651-1754`). **DEFER governance** (retention/secondary-index/compact)
  — not table-stakes (Mem0/Zep leave TTL to the app). Effort S.

### Slice G(deferred) — remaining gated read verbs *(DEFER-0.8.x, governance path clear)*
- **Gaps:** G2, G3, G4, G5, G7. **Single hard gate:** HITL resolution of fit-doc §7
  Q1 (is AC-057a negotiable for reads) + Q2 (namespace: `admin.*` vs `read.*` vs
  top-level). Build **engine seams now as dormant internal code** (S each); hold the
  SDK surface.
- **Engine seams (all reuse `ReaderWorkerPool` + DEFERRED tx + owned-row results —
  NOT the writer `connection.lock()`):**
  - **G3 `read_collection` / G7 `mutations(collection)`:** `ReaderRequest::ReadCollection`
    + `reader_worker_loop` arm + `OpStoreRow` + `Engine::read_collection`. Verbatim
    promotion of the `_for_test` op-store SELECTs (2245-2297). No migration, no G0
    dep. **Mandatory `limit` + after-id cursor for `append_only_log`**
    (`operational_mutations` caps ~1M — don't materialize a huge Vec across FFI).
  - **G2 `get`/`get_many`:** `ReaderRequest::GetById` + `read_get_by_id_in_tx`
    (`... WHERE logical_id IN (...) AND superseded_at IS NULL`, preserve order).
    **Depends on G0.** `get` delegates to `get_many`.
  - **G4 `list`:** `ReaderRequest::List` + `read_list_in_tx` (`... WHERE kind=?1
    [AND <filter>] ORDER BY write_cursor LIMIT ?n`). Filter compiled in
    `fathomdb-query` to **parameterized `json_extract(body,'$.field') <op> ?`** with
    allowlisted paths (no DSL, no interpolation). Index = G0-folded
    `canonical_nodes(kind)`. **Target is `body` JSON** (no `properties` column in 0.7.2).
  - **G5 `neighbors`:** modeled on `trace_source_ref`; depth=1 single SELECT,
    depth>1 bounded recursive CTE with **`MAX_WALK_DEPTH`/`MAX_NEIGHBORS` clamps**.
    Indexes = G0-folded `canonical_edges(from_id)/(to_id)`. **Depends on G0.** Callable
    as G6's internal expand step.
  - **G7 `history(id)`:** modeled on `trace_source_ref` but **queries only
    `canonical_nodes` by `logical_id` ORDER BY write_cursor**, using `superseded_at`
    for live-vs-tombstoned. **Depends on G0.** Decide deliberately whether edges
    participate (don't conflate with `trace_source_ref`, which merges nodes+edges).
- **Surface (governed under ADR-supersede):** these land under the **`read.*`
  namespace** (ADR-supersede recommendation B1), which sidesteps the old
  `admin.*`-overload question entirely — `read.get`/`read.list`/`read.neighbors`/
  `read.history`/`read.collection` are governed additions (parity + denylist +
  typed-boundary), not a "sixth-verb" invariant breach. The governed-surface
  conformance test (allowlist+parity, replacing set-equality) admits them. Either
  way: simultaneous Py + TS + one error-class per binding.
- **CLI (parity-free, can ship in parallel):** expose **G5 `neighbors` and G7
  `history`/`mutations` as read-only `doctor` verbs** (CLI carries no SDK-parity
  obligation, ADR-0.6.0-cli-scope), reusing `doctor trace → trace_source_ref`
  wiring. App-facing `get`/`list`/`search` stay off the CLI.
- **Effort:** engine seams S each; full end-to-end M each (binding + parity +
  conformance).

### Slice H — G6: Retrieve + expand *(capstone; governed via search args; depends on G0/G1/G4/G5)*
- **G6 = G1 + G4 + G5 + G9 composed.** Not independently buildable. (G9 = the RRF
  fusion the G1 retrieve path already applies; made explicit here to match the
  authoritative composition in the triage + `ADR-0.8.0-graph-traversal-scope`. The
  earlier "G1+G4+G5" shorthand folded G9 into G1-retrieve — same capstone.)
- **Migration:** none (prerequisite indexes belong to G4/G5, folded into G0's
  migration — **all CREATE INDEX, no exemption marker needed**, contra the first design).
- **AC-057a:** additive optional `expand`/`filter` args on `search` (governed under
  ADR-supersede — args on the existing verb, no new top-level surface). The real
  dependency is G1's result-shape change (a documented behavior-compat event, not a
  blocker), plus the G4/G5 prerequisites landing.
- **Files:** `read_search_in_tx` 3275-3308 — after the vector+text merge, before
  `tx.commit`, iterate each hit's `write_cursor`, run a **bounded read-only edge walk**
  (depth=`expand`, per-hit cap) over `canonical_edges`, append neighbor body+kind+id.
  Stays inside the existing DEFERRED snapshot tx — single-writer untouched.
- **Effort:** **G6-proper is XS–S** (one search arg + orchestration of the G5 walk
  in the existing extension point). The "L" belongs to the *prerequisite stack*, not
  G6's own row.

## 3. What we leverage that already exists in 0.7.2

- **Per-row write cursor + single `commit_batch` tx** (5108-5113): G0's
  UPDATE-prior-then-INSERT supersession rides it; AC-068a/b atomicity for free.
- **`#[non_exhaustive] PreparedWrite` + `WriteReceipt`** (833-862, 794-796): the
  sanctioned additive evolution path (ADR-0.6.0-prepared-write-shape) for G0's
  `logical_id` and G8's receipt field.
- **`trace_source_ref`** (2652-2701) + `TraceReport`: the read-shaped, owned-result
  template for G2/G5/G7 seams (retargeted `source_id → logical_id`).
- **`excise_source_inner`'s shadow-delete pattern** (2840-2930): template for G0's
  required shadow reconciliation follow-on.
- **`_fathomdb_projection_terminal`** + `advance_projection_cursor`: lets
  supersession/reconciliation walk past tombstoned cursors.
- **Op-store `_for_test` read seams** (2245-2297): verbatim query shapes promoted to
  G3/G7 public reads — zero new storage.
- **vec0 `vector_default` metadata/partition columns** (`source_type`, `kind`,
  `created_at` 4645-4656): G10 filters + G12 recency are *already on disk and
  filter-capable*; only `status`/`importance` are new vec0 columns (Rust reshape, no
  SQL migration, no accretion guard).
- **`ReaderWorkerPool`** (8 connections, round-robin `dispatch` 500-510) + DEFERRED
  snapshot tx (3203): every new read routes here, off the single writer Mutex.
- **`ProjectionRuntimeShared.search_limit_override`** lock-free atomic knob: template
  for G9's `fusion_mode` escape hatch (no env on hot path).
- **`bm25(search_index)`**: intrinsic FTS5 relevance for G9's text-branch rank — no schema.
- **`admin.configure` module-fn pattern** (py:585, napi:683): binding template for
  gated read wrappers.
- **`doctor trace → trace_source_ref` CLI wiring** (cli:371): lets G5/G7 ship
  operator value with zero AC-057a obligation while the SDK surface waits.
- **Migration 8 (`source_id`) / 10 (`mean_vec`) nullable-ALTER + exemption template**:
  the exact accretion-compliant shape for G0; migration-8 already promised the offset.

## 4. Test strategy (repo conventions)

- **Migration tests** (`fathomdb-schema/tests/migrations.rs`): assert G0's columns +
  partial unique index exist post-migrate, legacy rows back-fill NULL, the folded
  `kind` / `from_id`/`to_id` indexes exist. Run `scripts/agent-lint-migrations.py`
  against the `.sql` companion (linter only sees `.sql`; the Rust
  `check_migration_accretion` is the CI gate).
- **Acceptance tests pinning behavior-compat events** (`pr_*` convention):
  `pr_g9_rrf_fusion.rs` (doc ranked by both branches outscores single-branch;
  deterministic order; dedup-by-body; soft-fallback unchanged); a G10 unfiltered
  search test asserting **byte-identical ordering to 0.7.2**; a G0 idempotent-upsert
  test (re-ingest supersedes; partial-index uniqueness; NULL legacy never collides).
- **Recall floor protection (load-bearing):** Slice A reworks
  `eu8_ir_validation.rs:325-326` + `eu7_real_corpus_ac.rs:477` to read `hit.body` so
  the **0.90 floor (ANN 0.937) is identical**. Regression test bounding G0
  stale-shadow-row crowding until reconciliation lands.
- **Perf gates** (`perf_gates.rs`): update `.results` body-membership (815) to
  `hit.body`; write-latency gate for G8's per-edge EXISTS probe (must hit the index);
  read-latency ceiling for G5's recursive-CTE walk at `MAX_WALK_DEPTH`.
- **Injection safety** (`fts5_injection_safety.rs` + new G4 grammar test): assert
  `compile_list_filter` allowlists field paths and binds all values — no
  interpolation reaches SQL.
- **`*_for_test` seams as read-shape oracle:** cross-check G3/G7 public reads against
  existing op-store seams; add `get_by_logical_id_for_test` mirroring `trace_source_ref`.
- **SDK parity / error-mapping:** conformance suite verifies the structured hit (G1)
  and any gated read verb appear in **both** Py + TS, one-error-class-per-variant
  (AC-060a, no catch-all).
- **Closure:** each slice closes with an `output.json` artifact (PR-1/PR-9 board
  convention) recording the codex review verdict.

## 5. What still needs an explicit HITL decision

1. **The read-verb surface is decided, not blocked.** The "can we add app-facing
   read verbs?" question is **answered by
   `ADR-0.8.0-supersede-five-verb-surface-cap`**: yes, under governance (SDK
   parity + recovery-denylist + typed-boundary + `read.*` namespace). G2/G3/G4/G5/G7
   are *governed additions*, not "cannot be done." What still needs HITL **sign-off
   on that ADR** is narrow: (a) namespace confirmation (`read.*` vs `admin.*`),
   (b) per-verb sequencing (which land 0.8.0 vs 0.8.x), (c) the conformance-test
   rewrite (set-equality → allowlist+parity). Engine seams are dormant-shippable
   today regardless; CLI `doctor` verbs (G5/G7) remain an AC-057a-free operator
   escape valve.
2. **G1's `SearchResult.results: Vec<String> → Vec<SearchHit>` shape change** — a
   documented behavior-compat break to the bindings.md §4 owned-rows invariant;
   achievable but must be HITL-acknowledged and coordinated across both SDKs.
3. **G0's shadow-table reconciliation under supersession** — G0 alone leaves stale
   vec0/FTS5 projections competing for phase-1 KNN slots. Reconciling shadow tables
   with `superseded_at` (and the **op-store cascade contract under supersession** —
   roadmap:89-90, named but undefined: does superseding a node touch
   `operational_state`? does `recover --purge-logical-id` delete all versions or just
   current?) needs an explicit decision.
4. **G11 full bi-temporal (valid-time) edges + G12 importance-population semantics.**
   Per ADR-0.8.0 Option 2A, 0.8.0 *designs for but does not implement* valid-time. The
   valid-time columns + a true event-time signal (vs projection wall-clock
   `created_at`) presume G12's write-time timestamps and the substrate ADR's Q4
   resolution. G10's `status` and G12's `importance` ship as plumbed-but-unpopulated
   until a population-source design (body-JSON convention vs queryable canonical
   column) is chosen — a deferred design call, not a 0.8.0 deliverable.
