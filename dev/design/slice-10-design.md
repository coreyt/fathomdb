# Slice 10 design memo — G9 RRF fusion + rerank seam, G10 filtered KNN, G12-recency

Status: design-first for `dev/plans/prompts/0.8.0-slice-10.md`. Authoritative
contract: `dev/plans/0.8.0-implementation.md` § "Slice 10" as amended by the
HITL banner (lines ~465–482) and
`dev/adr/ADR-0.8.0-agent-memory-retrieval-and-identity.md` § "HITL decisions
(2026-06-02)". On any conflict the contract-as-amended wins; this memo
operationalizes it.

## 0. The HITL decisions that shape the slice

- **Q1 = Option 1A.** G9 RRF **and** G10 filtered-KNN are table-stakes; both ship
  here. `SearchFilter` is a **closed struct** `{source_type, kind, created_after,
  status}`, not an open DSL.
- **Q3 = documented-only, NO knob.** RRF is the **unconditional** new ranking.
  There is **no** `fusion_mode` atomic, **no** `fusion_mode=Legacy` escape hatch,
  **no** legacy-union-ordering code path, and **no** legacy-reproduction test. The
  ordering change is a documented behavior-compat event (Slice-40 release note),
  pinned by `pr_g9_rrf_fusion.rs` asserting RRF **determinism**.
- The `rerank_fused` identity-stub seam STAYS (G9's rerank hook).
- The unfiltered-search byte-identity pin STAYS for G10 (`filter=None` ⇒ phase-1
  SQL byte-identical to 0.7.2).
- G12-recency is gated behind a **dedicated recency flag, off by default** (NOT
  `fusion_mode`).

## 1. Score comparability — why RRF on *rank*, not raw score

The vector branch carries `vec_distance_l2` (lower = closer); the text branch
carries `bm25()` (more-negative = more-relevant). These live on different,
non-affine scales — you cannot add or compare them raw, and min-max normalizing
either is corpus-sensitive and unstable. Reciprocal Rank Fusion sidesteps the
scale problem entirely: it fuses the *ordinal rank* each branch assigns, never the
raw score. This is the standard hybrid-retrieval result (Zep/Mem0/Azure;
`0.8.0-agent-memory-fit.md` §8d). Hence Slice 10 fuses ranks.

## 2. G9 — RRF formula + tiebreak

`RRF_K = 60.0` (the standard `k≈60`). For a body `b` surfaced by branch with
1-based rank `r` in that branch:

```
score(b) = Σ_branch 1 / (RRF_K + rank_branch(b))
```

A body appearing in **both** branches accumulates both terms, so agreement
boosts it above a body in only one branch — the entire point of hybrid fusion
that the prior scoreless union-dedup could not express.

- **Keyed on `SearchHit.body`** (the prior dedup key; keeps the merge identity
  stable and matches the recall harness, which extracts `hit.body`).
- **Representative hit:** when a body is in both branches it surfaces **once**
  with the **vector** branch's `id/kind/branch` (vector-first identity preserved).
- **Tiebreak (deterministic):** sort by `score` descending; on equal score,
  **vector-first** (a hit that appeared in the vector branch precedes a text-only
  hit); on a further tie, preserve the vector-branch rank then the text-branch
  rank (insertion order). A stable sort over an insertion-ordered accumulator
  makes the output a pure function of the two input lists — no `HashMap` iteration
  order leaks in.

The fused value is written into `SearchHit.score: f64` (Slice-5 carrier). The
`SearchHit` doc comment ("raw per-branch relevance") is updated to say the score
is the RRF-fused value.

### Vector-empty signal **before** branch collapse

The `soft_fallback` "vector branch could not contribute" signal is computed from
`vector_results.is_empty()` **before** the fusion merges the branches — exactly as
0.7.2 does. Fusion must not erase that signal. Order in `read_search_in_tx`:

1. run the vector phase-1/phase-2 → `vector_results`;
2. `vector_rows_visible = !vector_results.is_empty()`; compute `soft_fallback`;
3. run the text branch;
4. `fuse_rrf(...)` → `apply_recency_reweight(...)` → `rerank_fused(...)`.

### `rerank_fused` identity stub (the rerank seam)

```
fn rerank_fused(hits: Vec<SearchHit>) -> Vec<SearchHit> { hits }  // identity, for now
```

The MMR/cross-encoder rerank is a later increment; the seam exists now so that
landing it is additive. It is **not** the dropped knob.

### Testable architecture

The merge region is refactored into a pure, `#[doc(hidden)] pub` function
`fuse_rrf(vector_hits, text_hits) -> Vec<SearchHit>` so the formula/tiebreak/dedup
are unit-tested directly on synthetic branch inputs (no embedder, fully
deterministic), and `read_search_in_tx` calls it. A second end-to-end test asserts
repeated identical `Engine::search` calls return byte-identical order + scores.

## 3. G10 — `SearchFilter` + 3-way shape-sentinel

### Shape

```rust
pub struct SearchFilter {
    pub source_type: Option<String>,
    pub kind: Option<String>,
    pub created_after: Option<i64>,  // created_at >= this unix-seconds bound
    pub status: Option<String>,
}
```

All fields optional; `Option<SearchFilter>` threaded through
`ReaderRequest::Search` → `reader_worker_loop` → `read_search_in_tx`, and exposed
as `Engine::search_filtered(query, Option<SearchFilter>)`. `Engine::search(query)`
delegates with `None` (public Rust surface unchanged for existing callers).

### Where it is applied

- **Vector branch (authoritative, pinned):** the present predicates are appended
  to the **single** phase-1 candidates statement inside the CTE `WHERE`:
  `... MATCH vec_quantize_binary(vec_f32(?1)){clause} ORDER BY distance LIMIT
  top_k`, where `{clause}` is `AND source_type=?n AND kind=?n AND created_at>=?n
  AND status=?n` for **only the present** fields, numbered from `?3` (`?1` = bin
  vector, `?2` = f32 query vector). `status` is a plain vec0 metadata column so it
  is constrainable under the KNN `WHERE` (aux columns hard-error there — see
  below). Keep `ORDER BY distance LIMIT top_k` — **no `k=`** parameter form.
- **`filter=None` (or all-None) ⇒ `{clause}` is empty ⇒ the SQL string is
  byte-identical to 0.7.2.** Pinned by a test that diffs the builder's `None`
  output against the frozen 0.7.2 literal.
- **Text branch:** when a filter is supplied, text hits are constrained in Rust
  against the same metadata (`kind` direct on the hit; `source_type` via
  `resolve_source_type(kind)`; `created_after`/`status` looked up from
  `vector_default` by `rowid == write_cursor`). A text-only row absent from
  `vector_default` is excluded when `created_after`/`status` is set (its
  vector-metadata cannot satisfy the predicate) — filtered semantic search is a
  vector-metadata capability. The text **SQL** is left byte-unchanged so the
  `None` path is trivially identical; the constraint is purely a post-filter.

### 3-way shape-sentinel (fixes the `embedding_bin` no-op bug)

`status` must land on **existing Pack-1 DBs**, not just freshly created ones. The
current `ensure_vector_partition` no-ops whenever the table SQL
`contains("embedding_bin")` — which is true for every Pack-1 DB, so a `status`
column added only to `create_vector_partition` would never reach an upgraded DB.
Replace that single check with a 3-way sentinel on the existing table SQL:

1. **`status` present** ⇒ already Pack-2 ⇒ **no-op**.
2. **`embedding_bin` present (no `status`)** ⇒ Pack-1 ⇒ **stage + recreate +
   back-fill**: copy `(rowid, embedding, source_type, kind, created_at)` to a
   staging table, drop `vector_default`, recreate at the Pack-2 shape (adds
   `status TEXT`), re-insert with `vec_quantize_binary(embedding)` and `status`
   back-filled NULL. Same transactional discipline as
   `migrate_vector_partition_to_pack1` (single `Connection::transaction()`,
   readers not opened until `ensure_vector_partition` returns, cross-process
   serialized by the sidecar lock).
3. **neither** ⇒ legacy single-column ⇒ `migrate_vector_partition_to_pack1`
   (existing path; that path is updated to create the Pack-2 shape directly).

### `status` is plain `TEXT`, not aux

vec0 **auxiliary** columns (`+col`) are stored out-of-band and **cannot appear in
a KNN `WHERE`** (sqlite-vec hard-errors). `status` must be filterable in the
phase-1 KNN statement, so it is declared as a plain metadata `TEXT` column. A
regression test asserts that declaring it aux breaks a filtered KNN (documents
*why* it is TEXT).

### `status` ships NULL plumbing only (known limitation)

There is **no real `status` population source** in 0.8.0. `status` is threaded
into `create_vector_partition`, the sentinel back-fill, the
`commit_projection_outcomes` INSERT, and `run_pin_and_requantize_pass`
DELETE+INSERT — all as **NULL**. Filtering by `status=Some(_)` therefore prunes
everything (a NULL column never `= ?`). This is **reserved-gap candidate 13** (a
real population source); recorded in `output.json`, not built here.

## 4. G12-recency (recency half only)

- Recency is derived from `write_cursor` (= `SearchHit.id`): higher cursor ⇒ more
  recent. After fusion (i.e. **after** bit-KNN — recency is **never** a vec0
  predicate), `apply_recency_reweight` normalizes ids over the result set to
  `[0,1]` and adds `RECENCY_WEIGHT * normalized` to each fused score, then
  re-sorts (stable, same tiebreak).
- `RECENCY_WEIGHT = 0.5 / RRF_K` (≈ 0.0083) — smaller than one RRF rank-step
  (`1/(RRF_K+1) ≈ 0.0164`), so recency breaks near-ties and nudges, but does not
  override a clear RRF signal. Documented; conservative by construction.
- **Gated behind a dedicated `recency_reweight_enabled` flag, off by default**
  (an `AtomicBool` on `ProjectionRuntimeShared`; test seam
  `set_recency_reweight_enabled_for_test`). Off ⇒ `apply_recency_reweight` is a
  no-op ⇒ order is pure RRF. **NOT** `fusion_mode`.
- A reweight-latency gate asserts the reweight pass is cheap (well under a small
  budget for a top-`k` result set).
- **Deferred (NOT built here):** G12-importance (the M-effort vec0-reshape half)
  and F9 confidence — reserved-gap / 0.8.x.

## 5. SDK parity (Py + TS, same slice)

`SearchFilter` is threaded as an **optional** argument through both bindings in
lockstep — `fathomdb-py` (`PySearchFilter` → `RustSearchFilter`) and
`fathomdb-napi` (`SearchFilter` napi object) — plus the pure-Python/TS wrappers
and `binding.d.ts`/`binding.ts` types. Same surface, no drift. The optional arg
defaults to "no filter" so existing call sites are unchanged.

## 6. Test plan (RED first)

- `tests/pr_g9_rrf_fusion.rs` — unit: `fuse_rrf` on synthetic branches pins
  `Σ1/(RRF_K+rank)`, both-branch agreement outranks single-branch, vector-first
  tiebreak, dedup-on-body, representative = vector hit. e2e: repeated
  `Engine::search` ⇒ byte-identical order + scores (determinism). Plus: vector
  branch empty + text match ⇒ `soft_fallback` still fires (signal before
  collapse). **No legacy-reproduction assertion.**
- `tests/pr_g10_filtered_knn.rs` — `Option<SearchFilter>` prunes in phase 1
  (kind/source_type/created_after); `status` lands on a **simulated Pack-1 DB**
  (drop+recreate `vector_default` at Pack-1 shape via `execute_for_test`, reopen ⇒
  sentinel back-fills `status`; assert column present + rows preserved);
  aux-vs-`TEXT` regression (aux `status` hard-errors under a KNN `WHERE`);
  filtering `status=Some` on the NULL-plumbed column prunes all. **Byte-identical
  unfiltered pin** lives here: `vector_phase1_sql_for_test(None)` equals the
  frozen 0.7.2 literal.
- `tests/pr_g12_recency.rs` — flag **off** ⇒ pure-RRF order (no effect); flag
  **on** ⇒ equal-RRF hits reorder by `write_cursor` (recent first); reweight
  applied AFTER bit-KNN; reweight-latency gate.

## 7. Guardrails honored

No `fusion_mode`/legacy path/legacy-repro test (Q3). No new SDK verb, no schema
migration (AC-057a-clean — the vec0 reshape is an `ensure_vector_partition`
runtime concern, not a `fathomdb-schema` step). Recall floor ≥ 0.90 re-measured
in self-check (eu7/eu8 read `hit.body`); a breach is a substantive blocker → HALT,
never overridden. `filter=None` byte-identity is a blocker if it diffs. Deferred
scope (G12-importance, F9) untouched.
