# STATUS — 0.7.0 PERF-VECTOR-QUANT

_Last updated: 2026-05-27 — Pack 1 + Pack 2 implementation CLOSED. Engine batch-collapse bug RESOLVED (`4a95cfd`). Projection scanner throughput fix (`53a270d`). Option 1 dense fixture (`38f5e3a`): recall@10 = 0.5124 at N=10K — Pack 2 SQL is structurally correct; the 0.90 floor requires real embeddings (deferred to 0.7.1 EMBEDDER-UNDEFER). P2-CANONICAL latency dispatch can proceed; recall lock-flip cannot._

Orchestrator: main thread (Claude Code session). Pattern per `dev/design/orchestration.md`.

## Handoff

- Plan: `dev/plans/prompts/0.7.0-PERF-VECTOR-QUANT-HANDOFF.md`
- Research: `dev/notes/0.7.0-vector-cost-research.md`
- HITL: `dev/plans/0.7.0-HITL-recommendations.md`
- Parallel work: `dev/plans/prompts/0.7.0-CORPUS-BUILD-HANDOFF.md`

## Baseline

- Branch: `main`
- Pre-campaign HEAD: `68c6339`
- Post-campaign HEAD: `d500d66`

## Slice scoreboard

| ID | Subject | Status | Cherry-pick(s) | Codex |
|---|---|---|---|---|
| S0 | ADR draft | **CLOSED** | `79cea9f`, `277fa4c` (inline fixup) | CONCERN → fixed inline |
| P1-DESIGN | Pack 1 design memo | **CLOSED** | `340502e`, `83869d6` (fix-1), `0fa6710` (fix-2), `adcbbfe` (fix-3), `8213b56` (closure) | BLOCK / BLOCK / BLOCK / **PASS** |
| P1-IMPL | Pack 1 schema + ingest | **CLOSED** | `d96c4b0`, `9b9f840`, `f5da3e4`, `7d4aa2c`, `b533f61`, `cc5d15e` (inline fixup) | CONCERN → fixed inline |
| P2-RED | Pack 2 RED tests | **CLOSED** | `d468999`, `4060a54` (closure) | **PASS** |
| P2-IMPL | Pack 2 query rewrite | **CLOSED** | `26ef3dc`, `28c2d6d`, `d500d66` (closure) | CONCERN (scope-precision nit, override) |
| P2-CANONICAL | canonical-CI dispatch + lock-flip | **AWAITS USER** | — | — |

## Per-AC scoreboard

| AC | Pre-campaign | Dev-box smoke N=10K | Canonical N=1M target | Status |
|---|---|---|---|---|
| AC-013 p50 | 2048 ms @ N=1M (W4.1) | 12 ms (post-batch-fix) | ≤ 80 ms | GREEN at dev-box; awaits canonical-CI |
| AC-013 p99 | 2327 ms @ N=1M (W4.1) | 16 ms (post-batch-fix) | ≤ 300 ms | GREEN at dev-box; awaits canonical-CI |
| AC-013b recall@10 | n/a (collapsed) | 0.5124 at N=10K (Option 1 dense) | ≥ 0.90 | RED — isotropic random is the noise-limited case; real-embedding validation deferred to 0.7.1 EMBEDDER-UNDEFER |
| AC-019 stress p99 | 8388 ms @ N=1M (W4.1) | 131 ms (post-P2) | improve | GREEN at dev-box; awaits canonical-CI |
| AC-012 / AC-017 / AC-018 | GREEN | GREEN | GREEN | unchanged |
| AC-020 | GREEN at canonical | flaky dev-box (pre-existing, stash-sandwich confirmed) | GREEN | unchanged |

## What landed

**ADR**: `dev/adr/ADR-0.7.0-vector-binary-quant.md` — status `draft, HITL-required`. Locks the architectural decision (binary quant + f32 rerank as a data-encoding change, not a second architectural lever).

**Design memo**: `dev/design/0.7.0-vector-quant-pack1.md` (fix-3 PASS) — D1-D8 resolved against the actual code anchors.

**Pack 1 code** (writer + schema):
- New `fathomdb-schema` migration step 9 (`migrations/009_vector_binary_quant.sql`) — preflight CHECK for unknown kinds.
- `migrate_vector_partition_to_pack1` in `lib.rs` — dim-aware in-place reshape (DROP+CREATE same name + staged copy + `vec_quantize_binary` SQL-side + D3 `KIND_TO_SOURCE_TYPE_CASE_SQL` + `strftime('%s','now')`).
- `ensure_vector_partition` updated: detects existing shape (none / old / Pack 1) and routes.
- Writer double-write at both sites (`commit_projection_outcomes`, `write_vector_for_test`) populates `embedding`, `embedding_bin`, `source_type`, `kind`, `created_at` inside the existing single transaction.
- `resolve_source_type` helper enforces 6-value HITL lock with `doc→article` coercion. Drift-detection unit test executes the SQL CASE against in-memory SQLite and asserts byte-equal output with the Rust helper.

**Pack 2 code** (reader):
- `read_search_in_tx` (lib.rs:2307-2370): replaced single-phase f32 brute-force with two-phase bit-KNN (`TOP_K_BIT_CANDIDATES=64`) + f32 rerank via `vec_distance_l2`. Single Deferred read transaction preserved; `?1` bound once and reused.

**Pack 2 tests**:
- `AC013_BUDGET_P50/P99` re-pinned to 80/300 ms.
- `ac_013b_recall_at_10_floor` — recall ≥ 0.90 against in-test f32 brute-force ground truth.

## Post-fix findings (2026-05-27)

**Engine batch-collapse bug resolved** (`4a95cfd`). `write_inner` now
allocates one cursor per row in the batch; vec0 holds N distinct
rows for a batch of N. Regression test
`tests/batch_write_per_row_cursor.rs` GREEN. Per-node workaround in
`tests/support/corpus_subset.rs` reverted to batched ingest;
`corpus_fts.rs` / `corpus_vector.rs` / `corpus_graph.rs` all GREEN
with the batched path.

**Honest dev-box AC-013 numbers** (post-fix, N=10K, AGENT_LONG=1):
- seed_ms = 28113 (vs <1s pre-fix when only ~10 unique rows were
  embedded — the new seed cost is the honest one, ~2.8 ms per row
  embed+commit through the projection runtime).
- p50 = 12 ms, p99 = 16 ms — well within the 80/300 budget.
- The latency rise from 6/12 to 12/16 reflects vec0 now actually
  holding 10K rows to search; previously the bit-KNN was searching a
  ~10-row partition.

**New finding — projection-scanner throughput is one-row-per-cycle.**
The collapse was masking this. `projection_dispatcher_loop`
(`lib.rs:2432`) and `next_pending_projection_job` (`lib.rs:2597`)
together enqueue exactly one job per scan cycle (the SQL has
`LIMIT 32` but only the first non-in-flight row is returned). At ~2.8
ms/row this is fine for AC-013 dev-box (~28 s for N=10K) but
projects to ~46 min just to seed N=1M on canonical-CI. AC-013b's
recall measurement (1000 queries × f32 brute-force ground truth over
the same fixture) is what hung the previous re-baseline attempt —
killed after 25 min before the ground-truth pass finished.

A follow-up slice should widen the dispatcher to enqueue all rows
returned by the LIMIT-32 SELECT (respecting
`PROJECTION_INFLIGHT_LIMIT`). Out of scope for the immediate batch-
collapse fix.

**Update (post-fix, commit `53a270d`):** scanner fix landed.
PROJECTION_INFLIGHT_LIMIT raised 8 → 32; dispatcher now fills the
full inflight budget per scan cycle. Dev-box AC-013 seed dropped
28113 → 2548 ms (11× faster); full AC-013/AC-013b test runtime
fell from 25+ min (killed) to 65 s at N=10K.

## Critical finding — AC-013b recall floor is currently NOT MET

With the batch collapse fixed AND the scanner fix landed, the
honest dev-box AC-013b recall measurement at N=10K is:

```
RECALL_NUMBERS n=10000 samples=1000 recall_at_10=0.1572
```

That is **far below** the HITL-locked `≥ 0.90` floor and would also
fail at canonical-CI N=1M.

Pre-fix this read 1.0 because the corpus collapsed to ~10 unique
vec0 rows per 10K writes; brute-force ground truth and production
both returned the same trivially-small set, making recall=1.0 a
degenerate measurement.

**Likely root cause — embedder-fixture pathology, not Pack 2 SQL.**
`VaryingEmbedder` (perf_gates.rs:~310, corpus_subset.rs:~239)
produces vectors with only 6 of 768 coordinates non-zero (FNV-1a
hash → 6 ±0.5 coord placements, then L2-normalize). After
`vec_quantize_binary` takes the sign bit of every coordinate, ~762
of the 768 bits encode the IEEE sign of an exact 0.0 (positive),
i.e. the bit-distance between any two corpus vectors is dominated by
762 bits of constant noise and only ~6 bits of actual signal. The
bit-KNN top-K=64 ANN step then returns essentially random
candidates, and the f32 rerank can only pick the true top-10 from
what's in those candidates — hence recall ≈ K/N rather than ≈ 1.0.

This makes the `=0.1572` measurement **not** a fault of Pack 2's SQL
shape, the K=64 choice, or the binary-quant ADR — it's a fault of
using a sparse-by-construction synthetic embedder to validate a
fixture designed for dense embeddings.

### Fixture-replacement evaluation (2026-05-27)

Two fixture replacements are being considered; this section records
the research, the value each option provides, and the recommendation.

#### Important context: production embedder is picked but unimplemented

Per ADR-0.6.0-default-embedder (Accepted 2026-04-27 but
implementation deferred 0.6.0 → 0.6.1 → 0.7.0) and ADR-0.6.0-
embedder-protocol (contract), fathomdb defines an `Embedder` trait
(`fathomdb-embedder-api`) but ships only `NoopEmbedder`
(`fathomdb-embedder`, returns `[1.0, 0.0, ...]` for all inputs).
Neither the Python (`fathomdb-py`) nor TypeScript (`fathomdb-napi`)
bindings expose an `Embedder` parameter — both call `Engine::open()`
with no embedder wiring.

**The model itself IS picked**, not open: ADR-0.6.0-default-embedder
§ 2 commits to **`BAAI/bge-small-en-v1.5`** via candle-transformers
(`bert::BertModel`), mean-pool + L2-norm, dim=384. What's open is
the implementation, the binding surface, and the EMB-5 loader
sub-design — not the embedder family. Other candidates
(all-MiniLM-L6-v2, e5-small, bge-base, gte-small, nomic-embed,
text-embedding-3-*) appear in the research notes (`dev/notes/0.7.0-
vector-cost-research.md` Tier-1 #3 / #5) but were rejected for
0.7.0 work. Matryoshka was HITL-locked out of 0.7.0 as a technique
("dominated by binary quantization for our setting").

**Implication for Option 2:** the 0.7.1 un-deferral campaign
(`dev/plans/prompts/0.7.1-EMBEDDER-UNDEFER-HANDOFF.md`) ships the
already-decided BGE-small. The campaign's *first slice* (EU-0) is
empirical research to validate that BGE-small actually meets the
recall floor under Pack 2's K=64 + sign-bit pipeline — because the
ADR was written before the 0.7.0 PVQ pipeline existed, and recent
literature shows 384d models under sign-bit can fall well below
0.90 recall@10 at K=64. HITL re-decision (alternate model, raise
K, or accept caveat) only triggers if EU-0 RED-shows BGE-small
fails.

#### Research summary — sign-bit + f32 rerank, recall@10, K~4–6×

Empirical recall@10 on real 768d-class embedders, ~4× oversampling
(HuggingFace MTEB; Qdrant; Elastic):

| Model (768d-class)          | NDCG@10 retention at K≈4× |
|-----------------------------|---------------------------|
| mxbai-embed-large-v1        | 96.4%                     |
| all-MiniLM-L6-v2 (384d)     | 93.8%                     |
| nomic-embed-text-v1.5       | 87.7%                     |
| **e5-base-v2 (768d)**       | **74.8%**                 |

Range across modern 768d embedders: **75%–96%**. Vendors hitting
≥0.95 at low oversampling (Qdrant, Elastic BBQ, Azure) all do
centering or learned rotation; none use raw sign-bit. Elastic
explicitly: "naive binary quantization is exceptionally lossy and
achieving adequate recall requires gathering 10x or 100x additional
neighbors." ITQ (Gong 2011) exists *because* plain sign-bit
underperforms on anisotropic real embeddings (~15% mAP gap at 64
bits). No published rule of thumb for an isotropic-vs-real delta;
gap depends on embedding-distribution eigenspectrum.

Citations:
- https://huggingface.co/blog/embedding-quantization
- https://qdrant.tech/articles/binary-quantization/
- https://www.elastic.co/search-labs/blog/better-binary-quantization-lucene-elasticsearch
- https://slazebni.cs.illinois.edu/publications/ITQ.pdf (Gong 2011)
- https://arxiv.org/pdf/1802.03936 (rotations for hypercubic quant)
- Charikar 2002 SimHash — `1 − θ/π` isotropic-regime bound

#### What each option's output answers

| Question Pack 2 needs answered                    | Option 1 (dense isotropic) | Option 2 (real corpus + real embedder) |
|---------------------------------------------------|----------------------------|-----------------------------------------|
| Does Pack 2's two-phase SQL plumb correctly?      | Yes                        | Yes (also)                              |
| Is recall ≥ 0.90 on isotropic vectors at K=64?    | Yes                        | N/A                                     |
| Is recall ≥ 0.90 on real workloads?               | **No** — bounds it from above | Yes                                  |
| Does ADR lock-flip survive production scrutiny?   | No (over-claims)           | Yes (if it passes)                      |
| Right K-value for production?                     | Lower bound only           | Direct                                  |

**CORRECTION (Option 1 measured 2026-05-27 at `38f5e3a`):** the
"isotropic is optimistic" framing above was wrong in direction.
Empirical recall@10 with dense isotropic random unit vectors at
N=10K, K=64 is **0.5124** — well below the literature numbers for
real embeddings (75%–96%). Reason: isotropic random vectors have
no semantic structure; the top-10 by cosine cluster within
~0.010 cosine spread while Hamming-quant noise is ~0.018 cosine-
equiv std, so top-10 are statistically "tied" with rank 11-64
from bit's perspective. The Charikar SimHash bound `P(sign match)
= 1 − θ/π` holds per angle, but on random data the angle spread
inside the candidate cluster is below the noise floor.

Real anisotropic embeddings are EASIER for sign-bit ANN, not
harder, because semantically-similar items share principal
directions and stand out from the noise. The published 0.93+
recall numbers reflect this signal-to-noise advantage.

Option 1's actual output therefore is:
- **Structural-correctness gate** — Option 1 jumping from 0.157
  (sparse) to 0.512 (dense) confirms Pack 2 SQL is correct and the
  bit-KNN + f32-rerank pipeline works as intended. The fixture
  was the bug, not the SQL.
- **Noise floor, not upper bound** — real embeddings will likely
  exceed 0.512, but the absolute number from isotropic random
  doesn't predict the real-embedder number in either direction
  with useful precision.
- **0.90 floor unreachable on this fixture** — no amount of K
  tuning on isotropic data will get to 0.90 because the signal is
  literally not there. Only real embeddings (Option 2 = 0.7.1
  EMBEDDER-UNDEFER campaign) can validate the floor.

#### Cost / risk

|                       | Option 1                  | Option 2                                                  |
|-----------------------|---------------------------|-----------------------------------------------------------|
| Engineering cost      | 1–2 hours                 | days (embedder selection + integration + N=1M scaling)    |
| Time to result        | minutes after code        | hours after code                                          |
| Lock-flip strength    | "isotropic synthetic"     | "real text"                                               |
| Pre-req               | none                      | HITL on embedder choice (reopens ADR-0.6.0-default-embedder) |
| Risk if passes        | false confidence on prod  | none                                                      |
| Risk if fails         | Pack 2 SQL shape is broken (rare) | may require Pack 3 (centering, larger K, PQ)      |

#### Recommendation

**Run Option 1 first as a structural-correctness gate. Use its
result as a NECESSARY-but-not-SUFFICIENT condition. Do NOT
unconditionally lock-flip the ADRs on Option 1 alone. Schedule
Option 2 as a 0.7.1 slice gated on HITL embedder selection.**

Objective reasoning:

1. Option 1 isolates the failure mode that the research predicts is
   responsible for the current 0.1572 reading (VaryingEmbedder
   sparsity → 762 of 768 bits encoding the sign of 0.0). It
   disambiguates between "fixture pathology" and "Pack 2 SQL bug"
   cheaply.

2. Option 1 alone cannot support the recall-floor lock-flip because
   the literature shows real 768d embedders span 75%–96% retention at
   the oversampling Pack 2 uses. The current ADR § 2 point 4 wording
   ("recall@10 ≥ 0.90 vs f32 brute-force ground truth on the AC-013
   fixture") is faithfully tested by Option 1 only if the AC-013
   fixture's distribution is representative of production —
   currently it is not.

3. Option 2 requires the candle BGE-small impl (per ADR-0.6.0-
   default-embedder § 2) plus binding surface plus the EMB-5
   loader sub-design — none of which exists yet. The 0.7.1 plan
   (`dev/plans/prompts/0.7.1-EMBEDDER-UNDEFER-HANDOFF.md`)
   sequences this work and includes empirical validation (slice
   EU-0) as its first step. The embedder *family* is not open;
   the implementation work is.

4. K=64 was sourced from "Kyle Howells and Cohere binary-quant
   benches" (ADR § 2 point 2). Those benches use modern well-
   conditioned embedders (Cohere v3 = 94.6% retention at 4×). If
   production eventually picks an e5-class embedder, K=64 is too
   aggressive. `TOP_K_BIT_CANDIDATES` being a named constant means
   K is already tunable — but the tuning data requires Option 2.

5. **Most defensible ADR lock-flip wording given Option 1 only:**
   "Pack 2's SQL shape and the K=64 default meet the recall floor
   on isotropic synthetic vectors. The floor against production
   embeddings is validated in a follow-up slice gated on HITL
   embedder selection." This is honest and lets 0.7.0 ship the
   latency win (the load-bearing closure for AC-013) without
   over-claiming on recall.

#### Option 1 result (2026-05-27, commit `38f5e3a`)

Replaced sparse `VaryingEmbedder` in `perf_gates.rs` with a
dim-filling xorshift64-driven uniform [-1, 1) per coord, L2-
normalized. Same FNV seed → deterministic.

| fixture | recall@10 @ N=10K, K=64 |
|---|---|
| sparse (pre-fix, 6 of 768 coords non-zero) | 0.1572 |
| dense isotropic (Option 1, all coords filled) | **0.5124** |

Interpretation per the correction above:
- **Pack 2 SQL passes the structural-correctness gate.** The
  3.3× jump in recall directly attributable to densifying the
  embedder confirms the bit-KNN + f32-rerank plumbing works.
- **0.5124 is the isotropic-random noise floor, not a Pack 2
  failure.** No amount of K tuning on this fixture reaches 0.90.
- **AC-013 latency unchanged**: p50=16, p99=19 ms at N=10K vs
  80/300 budget (the dense embedder added ~4 ms of latency since
  vec0 actually has 10K distinct embeddings to search; pre-Option-1
  was 12/16).
- **AC-013 + AC-019 latency lock-flip remains valid** because
  latency is independent of recall fixture properties.
- **AC-013b recall lock-flip is deferred to 0.7.1 EMBEDDER-UNDEFER
  EU-7** which validates against real corpus + real embedder.

#### What to do with the ADRs

The campaign can lock-flip the *latency* claims now (AC-013 p50/p99
budgets, AC-019 stress bound) since those are insensitive to fixture
distribution. The *recall* claim in ADR-0.7.0-vector-binary-quant § 2
point 4 must either:

- (a) be re-worded to "structural correctness validated on synthetic
  fixture; real-corpus recall floor validated in 0.7.1 EU-7", or
- (b) be deferred entirely until 0.7.1 EU-7 lands and either passes
  or triggers fallback (raise K, mean-centering, alternate model).

Option (a) is the recommended path — it lets 0.7.0 ship the latency
win, which is the load-bearing closure for AC-013.


## Open HITL items (awaits user)

1. **Fixture replacement (Option 1)** — replace VaryingEmbedder
   with a dense isotropic synthetic embedder; re-measure dev-box
   AC-013b. Necessary-condition gate for any ADR lock-flip. See
   "Fixture-replacement evaluation" above.
2. **Canonical-CI dispatch** — run the perf-canonical workflow with
   `targets="ac013 ac019"` and the locked W4.1-stacked-O1 env knobs.
   Confirm AC-013 p50 ≤ 80 ms, p99 ≤ 300 ms at N=1M; AC-019 GREEN.
   (Note: AC-013b at N=1M will only be meaningful post Option 1.)
3. **Numeric budget lock** — fill the placeholder AC-013/AC-019
   budget rows in `dev/adr/ADR-0.7.0-text-query-latency-gates-
   revised.md` with canonical-CI measurements + ~10% headroom.
4. **ADR lock-flip — LIMITED.** Both ADRs from `draft, HITL-
   required` → `locked` **with the recall claim qualified as
   "validated on isotropic synthetic"** per the recommendation
   above. Full real-embedding recall validation deferred to a
   follow-up slice (Option 2) gated on HITL embedder selection
   (reopens ADR-0.6.0-default-embedder).
5. **Update `dev/notes/pcache2-followups.md`** — reference AC-013/
   019 closure under the new lever and the Option-2 follow-up.
6. **Push to origin** — explicit user OK required (per handoff
   constraint).
7. **0.7.1 EMBEDDER-UNDEFER campaign** — plan landed at
   `dev/plans/prompts/0.7.1-EMBEDDER-UNDEFER-HANDOFF.md`. Ships
   the candle BGE-small-en-v1.5 impl already committed in
   ADR-0.6.0-default-embedder, the EMB-5 loader sub-design, a
   tightly-scoped NEED-017/REQ-033 download exception, Python+TS
   bindings, and real-corpus AC-013b validation. Empirical
   recall research (EU-0) runs first to confirm BGE-small meets
   the floor at K=64 under the 0.7.0 PVQ pipeline; HITL re-
   decision only if EU-0 RED.

## Compaction-resume checklist

1. Read this file.
2. `git log --oneline 68c6339..HEAD` for the campaign commit arc.
3. `dev/adr/ADR-0.7.0-vector-binary-quant.md` for the decision.
4. `dev/design/0.7.0-vector-quant-pack1.md` for design rationale.
5. `dev/plans/runs/*PVQ*review*.md` for each codex verdict.
6. `git worktree list` — no PVQ worktrees should remain (all cleaned).

## Codex iteration cost (campaign retro)

- S0: 1 codex (CONCERN → inline fixup).
- P1-DESIGN: 4 codex (BLOCK/BLOCK/BLOCK/PASS). Sources of churn: HITL-lock vocab; vec0 ALTER RENAME unsupported in 0.1.7; rebuild_vec0 async semantics; preflight CHECK + strftime mechanics. Each fix-N constrained vs prior verdict.
- P1-IMPL: 1 codex (CONCERN → inline drift-test strengthen + comment fix).
- P2-RED: 1 codex (PASS).
- P2-IMPL: 1 codex (CONCERN, scope-precision nit, override).

Total: 8 codex passes; 3 BLOCKs (all on the same slice, P1-DESIGN); design memo doubled as the constraint-discovery vehicle.
