---
title: Scaling vector retrieval to 1–2M chunks — latency roadmap & decision record
date: 2026-07-02
status: proposal / decision-support (pending HITL scheduling sign-off)
supersedes_context: refines the deferral rationale in dev/design/ann-index-vec0.md
desc: >
  Decision-support record for taking FathomDB's vector retrieval from its shipped
  O(N) two-phase bit-KNN path to interactive latency at a 1–2M-chunk target.
  Produced by an independent, no-FathomDB-context ("clean") evaluator against a
  code-verified featureset survey. Primary finding: at 1–2M the correct first move
  is a fast exact-quantized scan kernel (Path A), NOT an ANN index — ANN (IVF) is
  the durability track, budget relaxation is insurance. Everything gates on one
  prerequisite experiment (E1, overhead attribution).
---

# Scaling vector retrieval to 1–2M chunks — latency roadmap & decision record

> **What this is.** A decision-ready analysis of how FathomDB should take its
> vector-retrieval latency from ~1.5 s p50 @ 1M (measured) to within budget at a
> **1–2 million-chunk** target. It was produced by an **independent evaluator given
> no prior FathomDB knowledge** ("clean-room"), fed only a **code-verified featureset
> survey**, so the recommendation is not anchored to the existing plan-of-record. The
> analysis started from a general "when does SQLite + `sqlite-vec` stop scaling" write-up
> (Appendix A), was adversarially critiqued (Appendix B), then confronted with FathomDB's
> actual measured numbers and shipped capabilities — which reversed its conclusion twice
> (Appendix C). This document records the final recommendation and the reasoning trail.
>
> **Status: PROPOSAL.** The version placements below (esp. pulling latency work into
> 0.8.14) are recommendations that require HITL / Steward scheduling sign-off under the
> 0.8.x numbering/governance policy. Nothing here is an approved slice.
>
> **Load-bearing caveat.** The entire recommendation hinges on **experiment E1**
> (overhead attribution). If E1's predicted result (the scan is per-row-overhead-bound,
> not bandwidth-bound) does not hold, the ranking changes. Run E1 first.

---

## 1. The problem

FathomDB's production vector read path (`read_search_in_tx`, two-phase bit-KNN + f32
rerank in `fathomdb-engine`) has **no ANN index**: phase-1 is a per-query **O(N) linear
scan** over all binary codes. Measured, production-faithful, 384-d (`dev/design/ann-index-vec0.md`,
`dev/plans/runs/0.7.2-PR-3-perf-data.md`):

| N | p50 | 80 ms / 300 ms budget | status |
|---:|---:|---|---|
| 10,000 | 36 ms (real bge) | ✅ met | **binding release gate (0.x/1.x)** |
| 100,000 | ~147 ms | ❌ over p50, under p99 | tracked, not gated |
| 1,000,000 | ~1.5 s (O(N) extrapolation; f32-brute anchor 2,048 ms) | ❌ over both | tracked, not gated |

The linear scan meets the 80 ms p50 budget only to **~50k rows**. The 100k/1M tiers are
measured-but-not-asserted, explicitly deferred to post-1.0 ANN work.

**Why this is now urgent.** The deferral in `ann-index-vec0.md` rests on a stated assumption:
*"the 0.x/1.x consumer profile … runs corpora well under the ~50k crossover."* **If a
1–2M-chunk target is real, that assumption is falsified**, and the rationale for parking
the latency work at "2.x" collapses. That premise change is what triggered this review.

---

## 2. Decision

**Ranked, for the 1–2M-chunk target:**

> **Path A (drive p50 down on the exact/quantized architecture) > Path C (adopt ANN / IVF) > Path B (relax the budget).**

**Ship a packed/sharded exact-quantized scan kernel as the primary latency fix; run the
IVF spike in parallel as the durability track; adopt a modest, justified budget split only
as insurance; hold ANN productionization as an explicitly-triggered growth step. Gate every
branch on E1 first.**

### Why A over C at *this* scale

The measurement reframes the bottleneck. f32-brute = 2048 ms; binary (32× less data at
384-d, 1536 B → 48 B/row) = ~1500 ms — **a 32× data reduction bought only 1.37× speedup.**
That is not a bandwidth/compute-bound scan; it is a **per-row iteration-bound scan** (vec0
`xColumn` vtable dispatch, per-row blob fetch, top-192 heap maintenance). A tight popcount
over 48 MB of codes should be ~5–10 ms — the shipped path is ~150–300× off that floor.

Consequences that fall directly out of this:

- **int8 and Matryoshka/truncated-dim will NOT fix latency.** They shrink bytes-per-row;
  the bottleneck is rows-visited × fixed per-row overhead. Anything still touching all N rows
  lands at ~1–2 s. (They are *recall* levers, not latency levers.)
- **The cheapest fix is not an index.** Bypass vec0's generic per-row path with a **contiguous
  packed binary-code store + a SIMD popcount kernel + f32 rescore**, sharded across the existing
  partitions. Exact semantics, **zero recall risk**, plausibly **1.5 s → <100 ms at 2M**.
- **ANN (removing O(N)) is the *durable* answer but not yet *needed*.** At 1–2M you don't need
  sublinearity; you need to shrink the O(N) constant, which the kernel does — moving the crossover
  to ~20–50M rows. IVF becomes the right build when the corpus will grow past ~5–10M, or if the
  kernel can't close the gap for global/unscoped queries.

### Why the "already-tried the cheap move" story is nuanced

FathomDB **already ships** the very thing generic advice would recommend as the pre-ANN step —
binary Hamming prefilter → exact f32 rescore, plus a `source_type` vec0 partition key and a
recall-oracle harness. The naive read ("cheap move shipped and it still misses → therefore ANN")
is **wrong here**, because the miss is caused by vec0 per-row overhead, not by the algorithm.
The fix is to make the *existing exact algorithm* fast, not to replace it with an approximate one.

---

## 3. What already exists (code-verified, v0.8.9 / schema v15)

Authoritative retrieval is a Rust monolith (`fathomdb-engine`); bindings (PyO3 / NAPI) are thin.
Full inventory in **Appendix D**. The load-bearing facts for this decision:

- **Storage:** SQLite (WAL), durable. **Explicit canonical-vs-derived split already exists** —
  canonical = nodes/edges/op-store; derived (FTS5, vec0) are rebuildable projections. This is
  the "truth vs rebuildable index" pattern; any new index (packed codes, IVF centroids) fits it.
- **Vectors:** `sqlite-vec` `vec0` table storing **both** f32 `embedding` and binary `embedding_bin bit[dim]`.
  `source_type` is a **vec0 partition key**.
- **Vector search:** two-phase — binary Hamming KNN top-192 (**O(N) scan**) → exact f32 L2 rerank top-10.
  **No ANN** (grep for hnsw/ANN in `src/` is empty).
- **Quantization:** **binary only** (+ optional mean-centering). No int8, f16, or PQ.
- **Lexical:** FTS5 + BM25, porter unicode61 tokenizer, separate node & edge FTS tables.
- **Fusion:** weighted RRF (text 3.0 / vector 1.0 / graph 1.0; K=30) + recency reweight.
  Optional CE reranker (TinyBERT-L2, α=0.3) **off by default**. Graph BFS arm off + empirically
  refuted ×2. **No query planner/router** in code (design-only).
- **Filtering:** closed metadata filter (source_type, kind, created_after time-window, status)
  lowered to an **indexed pre-KNN WHERE inside vec0** (filter-before-KNN). Arbitrary JSON predicates
  rejected on the search path. **No permissions/ACL; no per-tenant partitioned indexes** beyond the
  single source_type key.
- **Perf posture:** perf gates enforced only at the **10k tier**; 100k/1M measured-but-not-asserted.
  Binary-quant recall fidelity floor **0.90 @ k=10** vs f32 ground truth. **A recall-oracle harness
  already exists** (Rust `ir_recall_eval` + 56-script Python eval layer).

**Net:** three ingredients the generic advice says to "go build" — binary prefilter + exact rescore,
canonical/derived split, and a recall oracle — are **already shipped**. The gap is (a) the scan is
overhead-bound, and (b) partitioning is coarse (source_type only) and not pruned at query time.

---

## 4. The three paths (ranked)

### Path A — Drive p50 down on the exact architecture — **RANK 1 for 1–2M**

| Lever | Expected Δ @1M | Recall risk | Removes O(N)? |
|---|---|---|---|
| **Packed/SIMD contiguous popcount kernel** (bypass vec0 vtable; scan one contiguous code blob) | **1500 → ~10–30 ms** | **None** (identical semantics) | No — but constant so small crossover → ~20–50M |
| **Shard scan across cores** (parallel over partitions, merge heaps) | further ~C× (~3–5 ms / 8 cores) | None | No |
| **Query-time `source_type` partition pruning** | ÷ selectivity for scoped queries | None (scoped); **zero help for global** | No |
| Truncated-dim binary prefilter (256/128-d) | ~0 if overhead-bound | Moderate (bge not Matryoshka; f32 rescore backstops) | No |
| Reduce candidate count (192→64) | **~0** (full scan + heap dominate) | Small | No |
| int8 intermediate tier | **negative** (adds a stage) | *recall* gain, not latency | No |
| Better mean-centering / rotation pre-sign-quant | negligible latency | raises bit-recall → safely lower k | No |

**Hits budget at 1–2M:** the kernel (very likely alone), optionally + sharding + pruning for 2M
headroom. **Only a constant / mislabeled:** truncated-dim, candidate-count, int8, rotation (recall
levers or bandwidth-plays E1 will likely rule out). **None removes O(N)** — the kernel makes the
constant tiny enough that the crossover sits far past the 1–2M target.
**Cost:** build a custom scan path (contiguous-codes shadow table + Rust SIMD kernel / sharded driver)
instead of vec0's generic vtable. Bounded; Rust is already in-tree.

### Path B — Relax the 300 ms p99 budget — **RANK 3 standalone; escape hatch only**

**Primary finding: don't relax the 300 ms retrieval budget — Path A makes it reachable, so relaxation
is unnecessary as a rescue.** When the budget is *reachable* on the exact architecture, spending your
one budget-relaxation chip to avoid a bounded, one-time kernel task is a bad trade — it permanently
concedes UX and positioning to dodge engineering you should do anyway.

**Capture the real value by *splitting*, not relaxing.** There is a genuine argument that 300 ms of
retrieval is invisible when an LLM call (seconds) dominates end-to-end. Capture that by keeping
**retrieval p99 ≤ 300 ms** (A reaches it) and giving the **CE reranker its own separate budget line**
(e.g. +200–400 ms when enabled) — preserving the sub-300 ms retrieval story while acknowledging
LLM-dominated end-to-end latency.

**If relaxed anyway — the two regimes:**

- **~500 ms:** low-regret. Buys CE headroom / slack if A lands ~350 ms; lets you not over-tune the
  kernel. Trades a little interactive snap and the clean "sub-300 ms" line.
- **~700 ms–1 s:** the *only* relaxation that changes architecture — a tuned exact-quantized scan can
  survive to ~2M with **no index at all**, so you'd build **neither IVF nor ANN**. But it concedes
  competitive positioning vs sub-300 ms cloud players, still **degrades with N** (O(N)), and eats the
  reranker's headroom. **Reserve strictly as the escape hatch** — take it *only if* E1 proves the scan
  is genuinely compute/bandwidth-bound (kernel can't help) **and** the IVF sweep can't hit
  recall-at-budget, i.e. when both Path-A sub-levers have failed.

**Net: hold 300 ms for retrieval; split out a CE budget rather than relax; keep a 700 ms–1 s
relaxation as the contingency-of-the-contingency.**

### Path C — Adopt ANN (IVF-via-partition-key) — **RANK 2, the durability track**

Mechanism: a **`centroid_id` vec0 partition key** (k-means, ~√N ≈ 1000–1400 centroids at 1–2M),
query restricted to the `nprobe` nearest centroids. Architecture-native (reuses partition key +
filter-before-KNN), **derived/rebuildable** (fits canonical/derived), stays **inside sqlite-vec**
(no external index, crash-consistency preserved). It is the **only path that removes O(N)**, so it is
the right build if: **(i)** E1 shows the scan is *not* overhead-bound (kernel won't help); **(ii)** the
kernel still can't hit budget at 2M; **(iii)** global/unscoped high-dim queries dominate; or **(iv)**
the corpus will grow past ~5–10M. Run its experiment (E2/E3) **in parallel** with A so the durable
answer is ready, but ship A first.

External/in-file ANN variants remain **further-contingent** and are *not* the default flavor:

- **RAM-HNSW sidecar (Vectorlite/usearch):** best raw recall/latency Pareto, but (1) index lives
  outside SQLite → breaks canonical/derived crash-consistency (needs watermark sync + startup
  reconciliation); (2) hnswlib soft-deletes accumulate under supersession churn → periodic rebuild;
  (3) **HNSW under filter-before-KNN suffers worse recall collapse than IVF**. Your architecture
  actively favors IVF.
- **libSQL/Turso in-file DiskANN:** keeps ANN in-file (preserves consistency) but is a **substrate
  swap** (you're on sqlite-vec/vec0) and DiskANN is over-scaled at 2M (RAM-resident anyway).

---

## 5. Roadmap scaffold & sequence

```text
Phase 0  E1 overhead attribution        ── gates EVERYTHING; must precede any "smaller codes" or index proposal
Phase 1  Oracle-harness extension        ── ANN-fidelity + filtered-fidelity metrics (nearly free; enabler for A-gate and C)
Phase 2  Packed/sharded exact kernel (A) ── the primary latency fix; depends on E1 confirming overhead-bound
Phase 2b IVF spike (E2) IN PARALLEL (C)  ── durability track; measured, not necessarily shipped
Phase 3  Partition pruning + filtered de-risk (E-A2 / E3) ── exploits source_type key + filter-before-KNN
Phase 4  GA hardening: ASSERT the gate @100k/1M/2M, rebuild/cutover, drift retrain
Phase 5  [CONTINGENT] IVF productionization, else HNSW/DiskANN bake-off ── only if E1 / E-A1 / E-A2 fail
```

**Ordering rationale:** the oracle harness must exist before tuning any approximate path or asserting
a gate. The kernel (A) is the smallest change that touches the real bottleneck with zero recall risk.
Filtered de-risk precedes GA because filter-before-KNN is central and is exactly where approximate
candidate-gen silently loses recall. ANN productionization is last and contingent because it is the
only step that could break the clean in-file consistency or force a substrate swap.

### Proposed version mapping (requires HITL scheduling sign-off)

| Version | Currently penciled | Proposed | Rationale |
|---|---|---|---|
| **0.8.14** (next open slot) | experiments (EXP-S) | **E1 + packed/sharded kernel (A) + partition pruning + oracle-harness ext + IVF spike (E2, parallel)** | The measurement makes latency a GA blocker, not a post-1.0 nicety — pull it forward |
| **0.8.16** | ONNX embedder portability | **ONNX as planned** (contingency: convert to IVF productionization if kernel can't hit 2M) | Orthogonal; decoupled embedder also helps any future re-embed |
| **0.8.18** | vector / GA (release-engineering GA only, per F-17) | **Kernel makes the 100k/1M/2M gate technically *meetable*; formalize the budget split.** Hard-*asserting* the scale gate follows the F-17 maturity ladder, not this slot | Previously deferred *because* the scan couldn't meet it; now meetable |
| **0.8.20** | **deps-only** (ANN/HNSW = **2.x**, F-16) | **Stays deps-only; ANN stays 2.x.** + Slice-0 agenda note: *if* G-1 (2MM premise) = YES, the **sqlite-vec migration is a predecessor** of any future vec0-internal kernel work | ANN is **not** pulled forward (M1); the early-reopen escalation path is retained in §8 |

> **Reconciled 2026-07-03 (M1, cross-product roadmap pass).** Per the governance ledger,
> **HNSW/ANN is an F-16 "2.x" item and stays there** — 0.8.20 remains **deps-only**; ANN is **not**
> pulled forward to 0.8.20. **0.8.18 "GA" is *release-engineering* GA only**; the *scale/latency* bound
> rides the **F-17 maturity ladder** (0.9.0 soft → 0.9.3 stated → 1.0.0 exit-beta → 1.1.0 hard). So the
> **only** forward-pull this doc proposes is the **Path-A latency kernel** (exact, 1-bit, deterministic —
> invariant-preserving) to **0.8.14**, evidence-gated on G-1 (premise) + G-2 (E1). **Path C (ANN/IVF)
> stays a 2.x contingency** and reopens early *only* via the written escalation path (§8): G-1 = YES ∧
> E1 shows not-overhead-bound ∧ the kernel can't hit budget on global/unscoped queries. The hard scale
> *gate* lands on the F-17 ladder (≈0.9.x → 1.1.0) regardless of when the kernel ships. The kernel's
> 0.8.14 placement is a schedule/theme change requiring HITL / Steward sign-off.

---

## 6. Experiments & decision rules

- **E1 — Overhead attribution (prereq, ~1 day).** Standalone in-process packed popcount over the same
  1M binary codes vs the shipped bit-KNN.
  **Rule:** standalone ≤50 ms while vec0 ≥1 s ⇒ **overhead-bound ⇒ build the kernel (A); don't reach
  for ANN or budget relaxation.** If standalone is *also* ~seconds ⇒ compute/bandwidth-bound ⇒ kernel
  won't save you ⇒ escalate to C, and B becomes more relevant. *(Predicted: overhead-bound.)*
- **E-A1 — Kernel latency at scale.** Ship the kernel; measure p50/p99 at 1M & 2M on target hardware,
  exact recall unchanged. **Rule:** PASS if p99 ≤ 300 ms (or the justified relaxed retrieval budget) at
  2M ⇒ ship A, keep ANN deferred. FAIL ⇒ trigger C.
- **E-A2 — Partition-pruning coverage.** Measure the fraction of real queries carrying a source_type /
  time-window filter and the rows-scanned distribution. **Rule:** if the *global/unscoped* p99 (prunes
  nothing) still fails after the kernel, that is the specific signal O(N) must be *removed* (C).
- **E2 — IVF recall/latency sweep (the C go/no-go).** ncentroids ∈ {√N, 4√N}; nprobe ∈ {1,4,8,16,32};
  ANN-recall@10 vs f32 oracle + p50/p99 @ 1M/2M. **Rule:** min nprobe with ANN-recall@10 ≥ 0.95 AND
  end-to-end labeled recall ≥ today's binary path; PASS if that nprobe also yields p99 ≤ 300 ms @ 2M.
- **E3 — Filtered-IVF collapse de-risk (critical for this design).** Stratify labeled set by filter
  selectivity {>50%, 5–50%, <5%}; ANN-recall@10 per bucket at chosen nprobe. **Rule:** if <5% bucket
  recall < 0.95, install an **adaptive exact-fallback** — when estimated survivors (selectivity ×
  nprobe-cell population) < ~4·k, bypass IVF and exact-scan the already-small filtered set. Because
  filter-before-KNN already narrows rows, exact-scanning a <5% slice of 2M is <100k rows ≈ tens of ms.
  **Ship the rule, don't hope.**
- **E-A3 — Rotation/centering recall lift (optional).** Learned rotation before sign-quant to raise
  bit-recall enough to lower k (192→64). **Rule:** adopt only if it lifts (or preserves-at-lower-k)
  end-to-end recall@10.
- **E-B1 — Budget justification.** Instrument end-to-end latency; retrieval share vs LLM/CE stages on
  representative agent turns. **Rule:** relax the retrieval budget only if retrieval is a small fraction
  of end-to-end *and* A lands just over 300 ms; name the number (proposed ~400–500 ms retrieval, CE
  separate) and record what's traded.
- **E4 — int8 as a recall tier (conditional).** Only if binary's 0.90 fidelity floor is shown to lose
  relevant docs end-to-end. **Rule:** adopt int8 only if it lifts end-to-end recall@10 by ≥2–3 points.
- **E5 — HNSW/DiskANN bake-off (gated on E2/E3 failing).** usearch/Vectorlite RAM-HNSW and/or libSQL
  in-file DiskANN; measure E2/E3 metrics + rebuild time + delete/tombstone behavior under supersession.
  **Rule:** pick the option meeting p99 ≤ 300 ms ∧ recall ≥ 0.95 ∧ filtered-recall ≥ 0.95 @ 2M with the
  least ops surface; prefer in-file to preserve crash-consistency.
- **E6 — Upstream extension watch (STANDING, not a scheduled slice).** The primary and contingency plans
  both build *in-engine* on the shipped sqlite-vec/`vec0` substrate; no new extension is adopted by default.
  But upstream is moving, so track it as a build-vs-adopt input for Path C:
  - **sqlite-vec's own native ANN surface** — its pre-release/experimental IVF, DiskANN, and rescore work
    (the reframe of the very question this doc answers).
  - **`vec1`** (official SQLite vector extension, IVFADC+OPQ, exhaustive + ANN modes) — release/packaging maturity.
  - **Vectorlite (hnswlib) / usearch** and **libSQL/Turso in-file DiskANN** — maturity for the E5 bake-off.
  **Rule:** if sqlite-vec ships a *stable, packaged* native IVF (or `vec1` reaches stable) before the Path-C
  build opens, fold an **adopt-vs-build evaluation into E5** — compare the native surface against the
  in-engine IVF-via-partition-key on recall-at-budget, filtered-recall, **in-file crash-consistency**, and
  ops surface. **Default stays build-in-engine** (it preserves the canonical/derived split, reuses the
  `source_type` partition key + filter-before-KNN, and keeps the index inside the DB file) *unless* the
  native surface demonstrably matches on recall-at-budget **and** filtered-recall **and** in-file consistency
  with **less** ops surface. Also note the naming/upgrade hazard: sqlite-vec's module is `vec0`; do not conflate
  it with the separate `vec1` extension. This watch is echoed in `dev/design/ann-index-vec0.md`'s open question
  "sqlite-vec native ANN surface vs an engine-side index"; keep the two in sync.

**"int8 enough vs ANN mandatory," stated flatly:** quantization is *never* the latency answer here
(bytes, not rows). **A's kernel is the latency answer at 1–2M.** **C is mandatory iff** E1 shows
not-overhead-bound, OR E-A1/E-A2 show the kernel can't hit budget for global queries, OR growth exceeds
~5–10M.

---

## 7. Trade-offs (3-way)

- **(A) Fast exact scan — best fit at 1–2M.** *Pro:* exact/deterministic (no recall gate), zero new
  index/consistency/rebuild/tombstone surface, reuses canonical/derived + partition key as-is, plausibly
  under budget. *Con:* still O(N) — a huge constant, not sublinearity; doesn't generalize past ~10–20M or
  rescue global-unscoped queries if the kernel alone falls short; requires a custom scan path outside vec0.
- **(B) Relax the budget — weakest standalone.** *Pro:* free, unlocks CE headroom, honest about
  LLM-dominated end-to-end. *Con:* can't rescue the current path at 1–2M without A's cheap wins; concedes
  sub-300 ms positioning and snappiness; degrades with N (postpones the reckoning). Use as a modest
  split-budget safety valve.
- **(C) ANN / IVF-via-partition-key — durable, deferred.** *Pro:* only path that removes O(N);
  architecture-native (partition key, derived/rebuildable, filter-before-KNN + exact-fallback); right
  long-term substrate. *Con:* approximate recall to gate, k-means train/retrain on drift, filtered-collapse
  risk, rebuild/cutover surface — costs you don't need at 1–2M if A hits budget. RAM-HNSW/libSQL-DiskANN
  further-contingent (break in-file consistency or force a substrate swap; DiskANN over-scaled at 2M).

---

## 8. Open items / what this doc does not settle

- **E1 has not been run.** The whole ranking is predicated on the scan being overhead-bound. Run E1 first.
- **Scheduling is a proposal.** The schedule-of-record places
  HNSW/ANN at **2.x (F-16)**, 0.8.20 as **deps-only**, and treats **0.8.18 "GA" as release-engineering only**
  with the scale bound on the **F-17 maturity ladder** (0.9.0 soft → 1.1.0 hard). Per M1 (cross-product pass),
  **ANN stays at 2.x and 0.8.20 stays deps-only** — the *only* forward-pull proposed is the invariant-preserving
  *latency kernel* (Path A) to 0.8.14 (currently EXP-S), and even that is gated on G-1 (premise) + G-2 (E1) and
  needs HITL / Steward sign-off. `dev/design/ann-index-vec0.md` should be updated to reflect the reframed bottleneck
  (overhead-bound, kernel-first — not an index-first problem) and the falsified <50k assumption **if the 1–2M
  target is confirmed**.
- **Upstream extension watch (E6) is standing, not scheduled.** The build-vs-adopt call for Path C depends on
  sqlite-vec's native ANN / `vec1` maturity; that watch has no owner or cadence assigned here. Assign one, and
  keep it synced with `ann-index-vec0.md`'s "native ANN surface vs engine-side index" open question.
- **Global-query fraction unknown.** E-A2 (how many real queries carry a filter) materially changes whether
  the kernel alone suffices or C is forced; it should be measured on real consumer traffic (Memex/Hermes/OpenClaw).
- **Provenance.** Recommendation authored by a clean-room evaluator (no FathomDB context) from a code-verified
  survey; it is decision-support, not an independent re-derivation of the engine internals. Verify kernel/vec0
  overhead claims against the code before committing engineering.

---

## Appendices

- **Appendix A** — Original "when does SQLite + sqlite-vec stop scaling" write-up (the source material).
- **Appendix B** — Clean-room evaluation of that write-up (factual corrections + reasoning critique).
- **Appendix C** — Decision arc: NOT-YET → FLIP → RE-RANK (how the measured numbers moved the answer).
- **Appendix D** — Code-verified featureset inventory (v0.8.9 / schema v15).
- **Appendix E** — Supporting perf data & the crossover analysis.

---

## Appendix A — Original write-up (source material)

> A general-audience argument that at 1–2M chunks one should stop treating `sqlite-vec`
> exact scan as the main retrieval path, keep SQLite as canonical store, add an ANN index
> as a rebuildable projection, and demote exact scan to oracle + rescore. Reproduced to
> preserve provenance; its specific claims are assessed in Appendix B.

At **1–2 million chunks**, I would stop treating `sqlite-vec` exact scan as the main retrieval path. SQLite can still be your **canonical store**, but exact vector search over all chunks will become the wrong default unless your embeddings are very small, your latency target is loose, or your filters narrow the candidate set heavily.

`sqlite-vec` has historically been strongest as a small, portable, exact/full-scan vector extension; its own earlier benchmark notes said it was comparing brute-force full scans, not ANN indexes like HNSW, IVF, or DiskANN. Newer `sqlite-vec` pre-release notes now mention experimental ANN work including rescore, IVF, and DiskANN, but it is still pre-v1 and breaking-change territory.

**Keep:** SQLite (documents, chunks, FTS5/BM25, metadata/provenance/permissions/timestamps, sqlite-vec exact vectors for smaller filtered sets or reranking). **Add a derived vector index:** (A) SQLite + Vectorlite/HNSW; (B) SQLite + vec1 IVFADC/OPQ when mature; (C) SQLite canonical + external vector service.

**Default recommendation for 1–2MM chunks:** Canonical truth = SQLite; lexical recall = FTS5; semantic candidate generation = ANN index; final precision = exact sqlite-vec rescore + reranker.

**Query flow:** (1) FTS5 top 200–1000 lexical; (2) ANN top 200–1000 semantic; (3) merge via RRF or weighted; (4) exact distance rescore against stored vectors; (5) optional cross-encoder/LLM reranker; (6) return top 10–50 with citations.

**Why exact becomes painful:** storage is fine; repeated distance computation is the problem. Raw embedding memory: 1M×384-d f32 ≈ 1.5 GB, 768-d ≈ 3.1 GB, 1536-d ≈ 6.1 GB; 2M doubles each (12.3 GB at 1536-d). 384/768-d may still be usable for offline/loose latency; 1536/3072-d full-scan over 1–2M will feel bad interactively.

**Options:** (1) sqlite-vec exact — narrow to rescore/small-filtered/oracle/dev/WASM; (2) Vectorlite/HNSW — loadable extension over hnswlib, approximate (needs recall measurement); (3) SQLite vec1 — official, IVFADC+OPQ, exhaustive+ANN modes, pre-release; (4) sqlite-vector — compact encodings, F32/F16/BF16/Int8/UInt8/1-bit/2–4-bit quant.

**Recommended architecture:** split into truth (SQLite canonical), recall (HNSW/IVF/DiskANN/vec1-style ANN, rebuildable, not source of truth), ranking (exact rescore + BM25 + recency/source priors + graph/entity priors + reranker).

**Migration:** Stage 1 keep sqlite-vec + add metrics (exact latency + Recall@20/50/100 vs labeled set); Stage 2 add ANN sidecar, keep sqlite-vec as exact oracle/rescorer; Stage 3 hybrid (FTS5 500 + ANN 500 + metadata filter + RRF + exact rescore + rerank); Stage 4 partition (source type, workspace, time, person/org, collection, embedding-model version).

**Key rule:** optimize for high-recall candidate generation, not raw vector-search speed. Failure mode = "failed to retrieve the one email/meeting/doc that proves the answer," not "query took 800 ms." Targets: candidate-gen recall world-class ≥98–99% Recall@100/@200, good ≥95%, risky <90%; final citation recall world-class = relevant source in top 10–20.

*(Cited sources in the original: alexgarcia.xyz sqlite-vec v0.1.0; dev.to "SQLite as a Vector Database"; github vectorlite; sqlite.org vec1 + forum update; github sqliteai/sqlite-vector.)*

---

## Appendix B — Clean-room evaluation of the write-up

**Verdict.** The destination is broadly right (SQLite as canonical truth + derived ANN + exact
rescore + hybrid fusion), but the *argument* is weaker than the conclusion.

**Factual corrections.**

- sqlite-vec = SIMD brute-force `vec0`; historically benchmarked scan-vs-scan. ✅ "pre-v1, breaking-change" fair.
- Memory table arithmetic correct (decimal GB): 1M×768×4 = 3.07 GB; 2M×1536×4 = 12.29 GB, etc.
- Vectorlite = hnswlib-backed loadable extension ✅ — but omits: index lives *outside* SQLite
  transactions (no crash-consistency), hnswlib **soft-deletes only** (tombstones accumulate),
  RAM-resident, small single-maintainer project. Skepticism applied asymmetrically vs sqlite-vec.
- vec1 = IVFADC+OPQ: unverifiable but *architecturally coherent* (page/disk-friendly, fits SQLite
  better than HNSW's random access). Stated with more confidence than an unreleased project earns.
  Naming hazard: sqlite-vec's module is `vec0`; easy to confuse with "vec1."
- sqlite-vector quant claims match public docs — but it's a **SIMD exact scan**, not ANN; listing it
  as an "index option" muddies the taxonomy (and cuts against the writeup's own thesis).
- Missing strongest options: **libSQL/Turso native DiskANN-style in-file index** (ANN *inside* the DB
  file with transactional consistency — the most direct answer to the question posed), usearch sidecar, Faiss.

**Reasoning critique.**

1. **Biggest flaw: the memory table doesn't support the ANN conclusion.** HNSW keeps all vectors +
   graph links in RAM (~3.2 KB/vec at d=768,M=16 → ~6.4 GB for 2M — *more* than the float payload the
   table warns about). ANN cuts compute-per-query, not memory. The table actually argues for
   **quantization/disk**, not HNSW.
2. **Latency hand-waved where it should be computed.** Single-thread exact ≈ (N·d·4)/(10–20 GB/s):
   ~150–300 ms @1M×768, ~0.6–1.2 s @2M×1536, but ~75–150 ms @1M×384. "Feels bad" holds for
   *high-dim, unpartitioned, float32*, not universally.
3. **Strongest alternative never engaged: quantized exact scan + rescore — which sqlite-vec already
   supports** (`bit`/`int8`). 2M×768 binary = 192 MB, popcount ~10–20 ms, rescore top ~1000 exact →
   interactive at 2M with zero new infra. *(Note: this is the theoretical floor; FathomDB's measured
   ~1.5 s shows a real per-row-overhead gap — see Appendix C/E.)*
4. **Recall targets conflate two recalls.** ≥98–99% Recall@100 is routine for **ANN fidelity** (vs exact
   oracle, tunable) but **unachievable for end-to-end relevance vs human labels** (embedder-bound, often
   60–90%). Calling <90% "risky" implies most real systems are broken. Determines *which knob you turn*.
5. **Pipeline ordering muddled:** step-4 exact-distance rescore as written discards the RRF fusion from
   step-3. Correct: rescore ANN distances → fuse (RRF + BM25 + priors) → CE rerank top ~50.
6. **Filtered-ANN recall collapse absent** — the most likely silent production failure for personal-memory
   (selective person/project/time filters), and unmentioned.
7. **"Rebuildable from SQLite" asserted, not designed** — no sync watermark, crash-consistency,
   delete/tombstone, or blue/green rebuild-gate story. **DiskANN at 1–2M is a category error** (its design
   point is 50M–1B+).

**What the reviewer would change:** reorder the ladder — (1) shrink/truncate embeddings, (2) partition +
exact-scan partitions, (3) binary/int8 prefilter + f32 rescore *inside sqlite-vec*, (4) ANN sidecar only if
global unscoped high-dim search is still slow. Split the recall metric (ANN-fidelity vs embedder-ceiling).
Add filtered-search + index-consistency contracts. Fix the rescore→fuse→rerank ordering.

---

## Appendix C — Decision arc (how the numbers moved the answer)

The recommendation was not static; it moved twice as real data arrived. Preserved because the *why* matters.

**Stage 1 — NOT-YET on ANN.** Given only the generic write-up, the reviewer gated ANN behind cheaper moves:
"(a) partition + exact-scan; (b) binary/int8 prefilter + f32 rescore inside sqlite-vec — both may reach
interactive latency at 2M with zero new infra. Flip me to YES if binary-prefilt + rescore *misses the p95
latency target at your real dimension/hardware — measure it.*" Keep the vector index a rebuildable projection
from day one so ANN can be added later without re-architecting.

**Stage 2 — FLIP to "an index is warranted."** The featureset survey showed FathomDB **already ships** the
cheap move (binary prefilter → f32 rescore, source_type partition key, recall oracle) — and it is **measured at
~1.5 s p50 @1M vs a 300 ms budget** (~5× over, ~10× at 2M). That is exactly the named flip-condition. First
conclusion: build a candidate-reduction index; the architecture-native form is **IVF-via-partition-key**, not a
HNSW sidecar; hold HNSW/DiskANN as a contingency.

**Stage 3 — RE-RANK to A > C > B.** The overhead-attribution insight (E1) reframed it: 32× smaller binary codes
bought only **1.37× speedup**, so the scan is **per-row-overhead-bound, not bytes-bound**. Therefore int8/Matryoshka
can't fix latency, and the cheapest fix is a **packed/SIMD exact-scan kernel** (Path A) — exact, zero recall risk,
plausibly <100 ms @2M. IVF (Path C) removes O(N) but isn't needed until ~5–10M; budget relaxation (Path B) can't
rescue the un-optimized path standalone. Final ranking: **A > C > B**, everything gated on E1.

**Reading:** this independently reproduces (and sharpens) the standing internal position that HNSW is *not* the
first lever — but relocates the first lever from "IVF" to "make the existing exact scan fast," and surfaces that
the "defer to 2.x" rationale assumes consumers stay under ~50k rows, which a 1–2M target would falsify.

---

## Appendix D — Code-verified featureset inventory (v0.8.9 / schema v15)

Authoritative retrieval is a Rust monolith: `src/rust/crates/fathomdb-engine/src/lib.rs`; DDL in
`fathomdb-schema/src/lib.rs`. Cited from code, not docs.

**1. Storage.** SQLite (WAL) + sidecar flock; forward-only transactional migrations. Explicit
`CANONICAL_TABLES` (canonical_nodes, canonical_edges, operational_collections/mutations/state); derived
(FTS, vec0, `_fathomdb_*`) excluded by design, rebuildable from canonical. canonical_nodes carries
`write_cursor, kind, body, source_id, logical_id, superseded_at` (partial unique index on logical_id WHERE
superseded_at IS NULL). canonical_edges carries fact-text + bi-temporal validity (t_valid/t_invalid, confidence,
extractor_model_id, temporal_fallback, logical_id, superseded_at).

**2. Vector search.** `vec0` virtual table `vector_default`, dimension-parameterized, built in engine
(runtime dim): `embedding float[<dim>]` + `embedding_bin bit[<dim>]` + `source_type TEXT partition key`,
`kind`, `created_at`, `status`. **Each vector stored twice** (f32 + binary). Two-phase:
`build_vector_phase1_sql` — `WHERE embedding_bin MATCH vec_quantize_binary(vec_f32(?1)) ORDER BY distance
LIMIT 192` (`TOP_K_BIT_CANDIDATES = 192`) → **O(N) linear scan**; phase-2 re-rank by exact `vec_distance_l2`
against f32, `LIMIT 10` (`SEARCH_RERANK_LIMIT = 10`). **No HNSW/IVF/DiskANN** (grep empty). Embedders:
bge-small-en-v1.5 @ **384-d** default; nomic-embed-text-v1.5 @ **768-d** alt; Candle local. Quantization:
**binary/bit only** (sign-quant) + optional per-workspace mean-centering. No int8/f16/PQ.

**3. Lexical.** FTS5 + BM25 (`search_index` node bodies + `search_index_edges` edge facts); tokenizer
`porter unicode61 remove_diacritics 2`. Injection safety tested.

**4. Hybrid.** Weighted RRF (`fuse_three_arms`/`fuse_rrf`, `RRF_K=30`, weights TEXT 3.0 / VECTOR 1.0 /
GRAPH 1.0) + additive recency reweight (`RECENCY_WEIGHT=0.002`). Optional CE reranker (`ce_rerank`,
blended = α·sigmoid(ce_logit)+(1-α)·rrf_norm, α=0.3, pool_n; cross-encoder/ms-marco-TinyBERT-L2-v2 ~17 MB,
CPU, lazy) — **off by default** (gated `rerank_depth>0` + `default-reranker` feature). Graph BFS arm behind
`use_graph_arm`, **off by default**, empirically refuted ×2. **No planner/router** in code (0.8.15 design only).
Entry points: `Engine::search`/`search_filtered`/`search_reranked`/`search_explained` → `search_inner_with_stats`.

**5. Filtering/partitioning.** Closed metadata filter as **indexed pre-KNN WHERE** inside vec0
(`vector_filter_clause`): source_type, kind, created_after (→ created_at ≥ bound), status. Arbitrary JSON
predicates **rejected** on search (allowed only on read.listFilter). Partitioning = single `source_type` vec0
partition key (+ fixed kind→source_type CASE map). **No permissions/ACL/multi-tenant; no per-workspace
partitioned indexes; no sharding** (one SQLite file per workspace).

**6. Perf.** Gates test to N=1M; enforced **only at 10k tier** (`AC012_GATE_N`/`AC013_GATE_N=10_000`);
100k/1M measured-but-not-asserted. Budgets: vector/hybrid p50=80/p99=300 ms; text 20/150 ms. Recall floor
`AC013B_RECALL_FLOOR=0.90` @ k=10 vs f32 ground truth. **Recall-oracle harness exists**: Rust
`tests/ir_recall_eval.rs` (+ ir_c_recall_run, recall_gate_predicate, ir_c_pooling_floor_gate) + 56-script
Python eval (`gate2_oracle_run.py`, `graph_arm_recall.py`, `ce_rerank_probe.py`, `expm4_embedder_ceiling_run.py`, …).

**7. Language split.** Rust authoritative (fathomdb-engine + schema/query/embedder/embedder-api). Python =
thin PyO3 (`search`, `read_*`, `write`) + non-shipping eval harness. TypeScript = thin NAPI (filter lowering,
rerankDepth/poolN/alpha validation), delegates to Rust.

**8. Versioning.** Manifests 0.8.9 (lockstep); schema v15. Future-work docs (design, not implemented):
`dev/design/ann-index-vec0.md` (ANN on vec0, tracked/not-started, post-1.0/pre-2.1); `planner-router-psd-0.8.x.md`;
`dev/research/personal-agent-database-market-2026-07-02.md` ("HNSW explicitly deferred to 0.8.20"). Git log
confirms "HNSW=2.x (F-16)". No int8/PQ/f16 or partitioned-index design doc; binary-quant is the shipped approach.

**Explicit absences (load-bearing):** no ANN index; no int8/f16/PQ; no query planner/router in code; no
permissions/ACL on search; no per-workspace partitioned indexes; no sharding; 100k/1M tiers measured but not gated.

---

## Appendix E — Supporting perf data & crossover analysis

**Measured / extrapolated, production-faithful 384-d** (`dev/design/ann-index-vec0.md`,
`dev/plans/runs/0.7.2-PR-3-perf-data.md`):

| N | p50 | 80/300 ms | note |
|---:|---:|---|---|
| 10,000 | 36 ms (real bge) / 15 ms (synthetic) | ✅ met | binding 0.x/1.x gate |
| 100,000 | ~147 ms | ❌ p50 / ✅ p99 | tracked, not gated |
| 1,000,000 | ~1.5 s (bit-KNN, O(N)); f32-brute anchor 2,048 ms | ❌ | tracked, not gated |

**Crossover:** 80 ms p50 met only to **~50k rows**.

**The overhead-bound diagnosis (the crux):**

- f32 brute-force @1M = 2,048 ms; binary @1M = ~1,500 ms.
- Binary codes are **32× smaller** at 384-d (1536 B → 48 B/row), yet buy only **1.37× speedup**.
- A tight popcount over 48 MB of codes should be **~5–10 ms** → the shipped path is **~150–300× off** that floor.
- ⇒ bottleneck is **per-row iteration overhead** (vec0 `xColumn` vtable dispatch, per-row blob fetch, top-192
  heap maintenance), **not** bytes streamed or compute. This is what makes int8/Matryoshka ineffective as latency
  levers and makes a packed/SIMD contiguous-scan kernel the highest-leverage first move. **E1 confirms or refutes
  this attribution and must run before any build.**

**Deferral assumption at risk:** `ann-index-vec0.md` justifies parking the work at 2.x because "the 0.x/1.x
consumer profile runs corpora well under the ~50k crossover." A confirmed **1–2M target falsifies this** and
reclassifies latency from post-1.0 nicety to GA blocker.
