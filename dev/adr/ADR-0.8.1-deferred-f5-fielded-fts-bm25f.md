---
title: ADR-0.8.1-deferred-f5-fielded-fts-bm25f
date: 2026-06-13
target_release: 0.8.5+
desc: Framing ADR for F5 — fielded FTS / BM25F column-weighted scoring. Deferred pending R0 candidate-recall CDF (Slice 5) and R2 parity eval (Slice 25) signal. Captures what BM25F would require, why it is deferred, and the decision criteria. RE-POINTED 2026-06-16 from 0.8.2+ → 0.8.5+ (0.8.2–0.8.4 re-sequenced for the graph-adjudication track; lexical-baseline tuning must not be folded into the graph adjudication, where it would handicap the comparison).
status: DEFERRED — 0.8.5+
origin: dev/plans/0.8.1-implementation.md Slice 35 ("deferred-feature framing ADRs"); dev/design/0.8.0-v05-feature-triage.md F5 (DEFER 0.8.x); dev/adr/ADR-0.8.0-filter-grammar.md §7 follow-on
---

# ADR-0.8.1 — Deferred F5: Fielded FTS / BM25F Column-Weighted Scoring

**Status:** `DEFERRED — 0.8.5+` (re-pointed 2026-06-16 from 0.8.2+; 0.8.2–0.8.4 host the
graph-adjudication track — `dev/roadmap/0.8.2.md`). **No code change follows from this ADR.** Framing-only.
HITL sign-off is NOT required in Slice 35; this ADR is authored to capture the decision
model for future work.

---

## 1. Context — what is F5?

F5 is the feature gap covering **fielded FTS / BM25F column-weighted scoring** over
`canonical_nodes`. The v0.5.6 feature triage (`dev/design/0.8.0-v05-feature-triage.md` §F5)
classified F5 as `DEFER 0.8.x`. The current FTS5 index (`search_index`) is a **single-column
body-only** index using the `unicode61` tokenizer + the content-OR compiler. BM25F would
allow per-column weight boosting (e.g., `kind` match = 2×, `status` match = 1×, `body`
match = 1×) to improve precision on structured queries.

**Why it matters (hypothetically).** For a memory graph with strongly-typed nodes (kinds:
`todo`, `person`, `project`, `decision`), boosting kind-column matches could improve
query-routing precision — "find all my open todos about FathomDB" benefits from the `kind`
field carrying strong signal independent of body. BM25F formalizes this as a weighted sum
across columns rather than a boolean filter (G10) or a body-only BM25 score (current).

---

## 2. What signal the R0/R2 eval results will provide

**2.1 R0 candidate-recall CDF (Slice 5)**
The R0 harness measures `found@K` for K ∈ {50/100/200/500/1000} per query class on the
frozen corpus. It will answer: *at what retrieval depth does the current single-column FTS5
find the gold fact?* If `found@50` is already ≥ 0.90 for all query classes, adding BM25F
column weighting is optimization; if factoid queries have a lexical gap, fielded scoring
may close it. **Decision trigger: R0 shows a query class with `found@K` below the R@10
ceiling at meaningful K.**

**2.2 R2 parity eval (Slice 25)**
R2 measures end-to-end answer quality (Evidence Recall@K per class). If the factoid or
multi-hop class shows a gap between the RRF-fused system and the Mem0-OSS baseline on
queries where kind-typed retrieval would help (e.g., "what are all the decisions I made
about X project?"), that is evidence F5 is product-value-positive. **Decision trigger: R2
shows a class gap attributable to lexical recall on structured kind-typed queries.**

Without these signals, F5 is speculative improvement — the triage's "DEFER" reflects that
the current body-only BM25 is functional for the table-stakes workloads. Adding BM25F
without eval evidence risks premature optimization.

---

## 3. What BM25F would require

### 3.1 New FTS5 schema (migration)

BM25F in SQLite FTS5 requires a **multi-column** virtual table. The current schema:

```sql
CREATE VIRTUAL TABLE search_index USING fts5(body, content=canonical_nodes, ...);
```

A BM25F upgrade would need at minimum:

```sql
CREATE VIRTUAL TABLE search_index_v2 USING fts5(
  kind,           -- column 0, weight W_kind
  body,           -- column 1, weight W_body
  status,         -- column 2, weight W_status (optional)
  content=canonical_nodes,
  content_rowid=rowid,
  tokenize='unicode61 remove_diacritics 1'
);
```

The weight vector is supplied at query time via `bm25(search_index_v2, W_kind, W_body, ...)`.
This is a **new FTS5 virtual table** — the existing `search_index` cannot be altered in
place (FTS5 `ALTER TABLE ADD COLUMN` is not supported). A migration step would CREATE the
new table + re-populate it. **Schema version bump required.**

### 3.2 Query rewrite

The existing query path (`compile_text_query` in `fathomdb-query`) builds a single
`search_index MATCH ?` clause. BM25F requires:

```sql
SELECT rowid, bm25(search_index_v2, 10.0, 1.0) as score
FROM search_index_v2
WHERE search_index_v2 MATCH ?
ORDER BY score
LIMIT ?
```

where the weight vector `(10.0, 1.0)` is the kind-vs-body boost. The query compiler would
need a new `BM25fQueryPlan` path alongside the existing OR-token path.

### 3.3 RRF fusion impact

The lexical arm score currently uses `bm25()` (single-column). Switching to `bm25(v2, ...)` 
changes the score magnitude but not the RRF fusion logic (RRF ranks, not scores). RRF
fusion in `pr_g9_rrf_fusion.rs` is byte-deterministic on the current scoring; BM25F would
require the determinism pins to be extended and re-verified.

### 3.4 Index rebuild cost

A migration that re-builds the FTS5 index is an O(N) corpus operation at engine-open time.
For the 10k-binding latency target (AC-013), this is acceptable only if the rebuild can be
deferred (lazy) or amortized. The current schema accretion policy (no DROP, additive only)
forbids dropping `search_index` without a migration version bump and a deprecation path.

---

## 4. Why it is deferred

1. **No eval evidence the gap exists.** The R0/R2 harnesses have not yet run. Opening an
   F5 implementation without knowing whether the current FTS5 lexical arm is the bottleneck
   would be premature optimization.
2. **Schema migration cost.** A new FTS5 virtual table requires a migration step (SCHEMA_VERSION
   bump), an O(N) index rebuild, and test coverage of the migration path. This is non-trivial.
3. **Query compiler complexity.** The `fathomdb-query` crate would need a new compilation
   path for multi-column BM25F queries, with new parity tests across Py+TS bindings.
4. **Determinism re-verification.** The RRF fusion determinism pins (`pr_g9_rrf_fusion.rs`)
   would need to be extended for the new score range.

The 0.8.2 slot is the right home: by then, R0 and R2 will have measured the gap (or confirmed
the current FTS5 is sufficient), and the schema migration can be sequenced cleanly after
the 0.8.1 graph and reranker work is stable.

---

## 5. Decision model (when to open the implementation slice)

Open the F5 implementation slice when:

1. **R0 CDF shows a factoid/structured-query class `found@K` below 0.75 at K=100** — i.e.,
   the lexical arm is meaningfully missing structured queries that column-weighting could fix.
2. **R2 shows an end-to-end class gap** (Evidence Recall@10 delta ≥ 0.05 for factoid or
   multi-hop vs Mem0-OSS) on queries attributable to lexical column mismatch.
3. **HITL sign-off** on the column schema, weight vector, and migration strategy.

If R0 and R2 show the current BM25 is adequate (factoid `found@100 ≥ 0.85`, parity gap
attributable to other factors), defer F5 to 0.9.x as a quality optimization.

---

## 6. Reserved follow-on

- `dev/roadmap/0.8.2.md` tracks F5 as a 0.8.2 candidate contingent on R0/R2 results.
- Slices 5 (R0) and 25 (R2) are the decision inputs.
- The `fathomdb-query` crate's `compile_text_query` is the natural extension point for a
  future `compile_bm25f_query` variant; the FTS5 `content=` virtual table pattern is already
  present in the schema.
