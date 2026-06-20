<!-- Date: 2026-06-19 · Model: codex (via `codex exec`) -->
<!-- Input: dev/plans/runs/fathomdb-retrieval-experiments-compilation.md (neutral experiment-data compilation) -->
<!-- + dev/plans/runs/fathomdb-next-steps-codex-QA-appendix.md (project-ground-truth Q&A: 82 clarifying questions answered, incl. the hard CPU/no-API/1-bit footprint invariant and the BYO-LLM-only seam). This is the Round-B (ground-truth-informed) recommendation; the earlier generic pass was superseded. -->

# Hypothesis: Next Two Sequenced Steps for FathomDB Retrieval Excellence

## Evidence Baseline

The strongest operational signal is not "use graphs"; it is "make complete evidence reach the reader, then preserve fused-RRF's answerability." In MuSiQue, answer F1 was `0.510` when all bridges were in top-10 versus `0.068` when any bridge was missing (§4.4). When all bridges were present, fused-RRF answered best at `0.552`, while passage-dense retrieved all bridges more often (`0.68` vs fused `0.65`) but answered worse (`0.464`) (§4.4). That makes the next move: increase bridge/evidence presence without degrading final evidence composition.

The graph route should stay dropped. BFS did not improve LongMemEval pooled R@10: graph ON and graph OFF both landed at `0.65` post-filter (§3.3). PPR-fusion lost to fused-RRF on MuSiQue pooled ≥3-hop: `0.4097` vs `0.4502`, ΔF1 `-0.040469`, CI `[-0.115765, 0.031080]` (§4.7). CE-rerank is also not a proven lift: `0.4152` vs fused `0.4502` on the same MuSiQue cell (§4.7).

The live opportunity is D1/D2. Passage-dense deserves a registered comparison because it beat fused on MuSiQue pooled ≥3-hop F1, `0.4866` vs `0.4502` (§4.7), despite fused remaining the registered comparator. For memory recall, current fused retrieval does not beat BM25: LongMemEval pooled R@10 was BM25 `0.625`, fused `0.606`, FTS-only `0.562` (§3.1). Enrichment is weak but plausible only if scaled and controlled: N=40 FTS-enriched `0.775` trailed FTS-only `0.800`, while placebo was `0.700` (§3.4); lower BM25 `b` improved plain/enriched behavior up to `0.75`/`0.775` at `b=0.00` (§3.5). The IR corpus remains bounded by eu7/eu8: production vector fidelity was `0.8960` on N=7,667, below the `0.90` floor (§2.1), and report-only IR relevance recall@10 was `0.571` (§2.2).

## Step 1: D1 Dense Repair + Registered PRF Evidence Retrieval

### Pipeline / Methods In Sequence

1. Fix the production BGE pooling bug: make `bge-small-en-v1.5` use CLS pooling in the Rust query embedder, matching the eval harness.
2. Rebuild/query the same one-embedder vector index; do not mix embedder identities.
3. Retrieve lexical BM25/FTS5 top depth `D`.
4. Retrieve dense 1-bit Hamming ANN candidates, then f32 rerank only inside the existing two-phase pool.
5. Fuse lexical + dense with deterministic RRF `k=60`.
6. Add deterministic vector PRF as the D1 treatment:
   - take top `m` dense/fused candidates,
   - compute a bounded query-vector expansion from their vectors or text embeddings,
   - rerun dense retrieval,
   - fuse original lexical, original dense, and PRF-dense candidates with fixed deterministic weights/RRF.
7. Select final top-10 evidence deterministically for QA harnesses. Do not use CE-rerank as the default treatment; keep it as a logged secondary arm because prior CE was null/negative (§4.7).
8. Register comparisons:
   - `passage_dense` vs fused-RRF,
   - `fused_prf` vs fused-RRF,
   - optionally `dense_prf` vs passage_dense as diagnostic only.

### Placement / Footprint Compliance

- CLS pooling fix: **IN-LIBRARY**. Rust core, CPU, no network/API, deterministic.
- BM25/FTS5: **IN-LIBRARY**. CPU SQLite/FTS5, deterministic.
- 1-bit dense ANN + f32 rerank pool: **IN-LIBRARY**. Compliant only if eu7 fidelity is `>=0.90`; current large-run value `0.8960` shows this must be rechecked after the fix (§2.1).
- RRF `k=60`: **IN-LIBRARY**. Deterministic.
- Vector PRF: **IN-LIBRARY** if implemented as bounded deterministic vector/text expansion using local CPU embeddings and existing vectors. No LLM.
- QA reader, query rewriting, decomposition, HyDE, answer generation: **CALLER-SIDE BYO-LLM ONLY**. Not part of FathomDB.
- Priced GPT/Gemini runs: **EVAL-ONLY** with HITL spend gate.
- No HITL waiver needed unless changing the shipped footprint beyond CPU/no-network/1-bit storage. A new CPU model or reranker needs normal dependency/license review but not a GPU/API waiver.

### Goals Advanced

- Best agentic memory: fixes shipped dense behavior before judging LongMemEval, where fused currently trails BM25 R@10 (`0.606` vs `0.625`, §3.1).
- Exploratory recall: directly targets eu8 relevance recall@10 ceiling `0.571` (§2.2) while preserving eu7.
- Deep-exploratory recall: PRF is the cheapest deterministic way to pull deeper semantically related candidates from the hard subset without LLM calls.
- Multi-hop QA: targets complete-bridge presence, the largest observed F1 separator: `0.510` vs `0.068` (§4.4).

### What To Measure + Acceptance Bar

- eu7 fidelity: must pass `>=0.90` recall@10. Current N=7,667 vector-stage value was `0.8960`, CI `[0.8640, 0.9250]` (§2.1), so the repaired dense path must clear the hard floor before product promotion.
- eu8 relevance: report recall@10 against the frozen IR qrels; require no regression from `0.571` (§2.2), and target `>=0.59` as a practical first lift. Also report the hard deep-exploratory subset separately; require a positive recall@10 and recall@50 movement, with no eu7 regression.
- LongMemEval: run recall@10/@20 and per-class factoid, knowledge_update, multi_session, temporal. Minimum bar: fused/PRF must beat BM25 R@10 `0.625` and R@20 `0.694` from §3.1, and must not lose the multi_session signal where fused was `0.325` vs BM25 `0.275` (§3.1).
- MuSiQue: registered pooled ≥3-hop dF1 vs fused-RRF `k=60`; GO only if ΔF1 `>=0.04` and paired-bootstrap CI lower bound `>0`, with the existing trend/EM/confident-wrong/power gates. Baseline fused is `0.4502`, passage_dense `0.4866`, and ppr_fusion `0.4097` (§4.7). Because the M1 rule was underpowered even at N=1165 per the appendix, run cheap validation, power simulation, then HITL approve any priced full pass.
- Bridge diagnostic: all-bridges@10 must improve over fused frequency `0.65` without dropping F1-when-all-bridges below fused's `0.552` (§4.4).

### Dependencies / Ordering Rationale

This must come first because the production dense path has a known pooling bug, and all later retrieval conclusions depend on dense being valid. It is also the lowest-footprint, highest-leverage step: no LLM, no graph traversal, no new storage model, and it attacks LongMemEval, eu8, hard exploratory, and MuSiQue with one repaired path.

## Step 2: D2 Fielded Index-Key Enrichment + Length-Norm Controlled Fusion

### Pipeline / Methods In Sequence

1. Keep Step 1 as the comparator baseline.
2. Build offline enrichment keys for each document/session:
   - entities,
   - canonical aliases,
   - short factual keyphrases,
   - optional edge/fact text,
   - no graph traversal at query time.
3. Index enrichment in a separate field/sidecar lexical channel rather than appending blindly to the body.
4. Add a length-matched placebo arm exactly matching enrichment token volume.
5. Fix or replace the FTS length-normalization behavior for this channel:
   - either custom deterministic lexical ranking with tunable BM25 `b`,
   - or field-specific weighting that prevents enrichment length from dominating body text.
6. Query sequence:
   - BM25/FTS5 body retrieval,
   - enrichment-key lexical retrieval,
   - dense retrieval from Step 1,
   - optional Step 1 PRF dense retrieval,
   - deterministic RRF/weighted-RRF fusion,
   - final top-10 evidence selection.
7. Register only enrichment mechanisms that beat both plain Step 1 and length-matched placebo.

### Placement / Footprint Compliance

- Entity/fact/key extraction with local Qwen or other LLM: **OFFLINE-BUILD ONLY**. Never query-time, never in-library LLM.
- Enrichment storage and fielded lexical search: **IN-LIBRARY** once materialized. CPU, SQLite/FTS/custom Rust ranking, deterministic.
- Length-matched placebo: **EVAL-ONLY** control.
- Tunable BM25/custom ranking: **IN-LIBRARY** if implemented in Rust/SQLite-compatible deterministic code.
- Dense, PRF, RRF from Step 1: **IN-LIBRARY** under the same CPU/no-network/1-bit constraints.
- Caller query decomposition or LLM rewrite: **CALLER-SIDE BYO-LLM ONLY**, not part of this step.
- HITL waiver: needed only if adopting a new shipped CPU model/dependency with footprint implications; no waiver for offline-build enrichment itself.

### Goals Advanced

- Best agentic memory: targets LongMemEval sessions where names, aliases, updates, and temporal facts are often lexical-key problems.
- Exploratory recall: adds deterministic sparse recall for concepts that dense misses.
- Deep-exploratory recall: gives hard queries more discriminative lexical handles without graph BFS/PPR.
- Multi-hop QA: can improve bridge presence by indexing bridge-relevant factual keys, while fused evidence selection protects answerability.

### What To Measure + Acceptance Bar

- LongMemEval: primary D2 gate. Beat Step 1 and BM25 on pooled R@10/R@20. Historical bars: BM25 R@10 `0.625`, R@20 `0.694`; fused R@10 `0.606`, R@20 `0.688` (§3.1). Enrichment must also beat placebo, because prior FTS-enriched `0.775` did not beat FTS-only `0.800`, while placebo was `0.700` on N=40 (§3.4).
- BM25 length norm: reproduce the `b` sensitivity at scale. Prior N=40 showed plain/enriched R@10 of `0.75`/`0.775` at `b=0.00` vs `0.70`/`0.75` at `b=0.75` (§3.5). Acceptance: enrichment lift must remain after length matching and after the chosen length-norm fix.
- MuSiQue: same registered pooled ≥3-hop dF1 rule vs fused-RRF `k=60`: ΔF1 `>=0.04` and CI lower bound `>0`. Also require all-bridges@10 improvement over fused `0.65` and preserve F1-when-all-bridges near fused `0.552` (§4.4).
- eu7/eu8: eu7 must remain `>=0.90`; eu8 must not regress from `0.571` (§2.2). Report hard deep-exploratory subset separately, including the roughly 596-query subset named in the appendix.
- QA reader cost/power: use cheap-validate first; do not treat N=300 as decisive if the power simulation says the registered rule is underpowered.

### Dependencies / Ordering Rationale

D2 depends on Step 1 because enrichment should be tested against the repaired dense/fused baseline, not against a known-bug production dense path. It also needs Step 1's candidate logging and bridge diagnostics so enrichment is judged by bridge presence, placebo-controlled recall, and final answer F1 rather than by a single optimistic recall number.

## Ordering Rationale + Falsifiable Hypothesis

Do Step 1 first because it repairs a known production defect and tests the cheapest non-LLM query-side lever against the strongest observed signal: bridge presence drives answer F1. Then do Step 2 because enrichment is only credible if it survives length-matched placebo and length-norm control at scale.

Falsifiable hypothesis: **CLS-correct dense + deterministic vector PRF will raise bridge/evidence presence and beat fused-RRF on registered MuSiQue pooled ≥3-hop by at least ΔF1 `0.04`, while clearing eu7 `>=0.90`; if it does not, fielded enrichment with length-norm control is the next bounded attempt, and graph traversal remains dropped.**
