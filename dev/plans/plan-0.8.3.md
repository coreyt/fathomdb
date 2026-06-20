# FathomDB 0.8.3 — Plan: non-graph retrieval (D1 dense + D2 fielded enrichment)

> **What this version is.** 0.8.3 is the **post-M1 redirect**. The 0.8.2 M1 milestone closed NO-GO —
> the multi-hop graph arm (lexically-seeded PPR fused with BM25) did not beat fused-RRF on MuSiQue
> answer accuracy (`runs/0.8.2-m1-FINDINGS.md`), and graph multi-hop is now refuted on both recall
> (0.8.1 n=40) and answer-accuracy (0.8.2 n=300). The HITL-signed redirect (`../roadmap/0.8.3.md`)
> points 0.8.3 at the two **non-graph** levers M1 surfaced, sequenced dense-first:
>
> - **D1 — passage-dense, registered.** Repair the shipped dense path (the CLS-vs-Mean pooling bug),
>   re-clear the eu7 fidelity floor, re-measure a corrected baseline, then add deterministic vector PRF.
> - **D2 — fielded index-key enrichment.** Extract entity/fact/alias keys offline into a *separate
>   fielded lexical channel* (not blind body-append) with a length-norm fix, measured against a
>   length-matched placebo.
>
> Both directions were **independently confirmed**: two codex passes — one steered, one given only the
> neutral data + goals + system facts — converged on the same two approaches, the same dense-first
> sequence, the same footprint placement, and the same evaluation discipline
> (`runs/fathomdb-next-steps-codex-hypothesis.md`, `runs/fathomdb-next-steps-codex-confirmation-run2.md`).

Ladder shape + reserved-gap policy: reuse [`0.8.1-plan.md`](0.8.1-plan.md) §"Ladder".
Process: [`../design/orchestration.md`](../design/orchestration.md) (three-role separation, codex §9,
worktrees §11). Slice prompts generate from [`prompts/0.8.0-SLICE-TEMPLATE.md`](prompts/0.8.0-SLICE-TEMPLATE.md)
(version-neutral). Roadmap home + the five adjustments (ADJ-1..5): [`../roadmap/0.8.3.md`](../roadmap/0.8.3.md).
Evidence base (neutral): [`runs/fathomdb-retrieval-experiments-compilation.md`](runs/fathomdb-retrieval-experiments-compilation.md).

**Unlike M1, 0.8.3 contains real ENGINE changes** (the CLS-pooling fix; the fielded-FTS / BM25F channel;
tunable-/lower-`b` ranking). Those slices carry X1 SDK parity, schema migration + `SCHEMA_VERSION` bump,
determinism pins, and the eu7 fidelity floor — they are not eval-harness-only. **ADJ-2: adopting D2's
fielded form promotes the F5 (BM25F / fielded-FTS) and tunable-`b` levers forward from 0.8.5** — a scope
decision the Slice-0 HITL gate must ratify (re-point the F5 ADR) or D2 defers to the weak body-append form.

---

## 0. Goal (0.8.3)

**Determine whether either non-graph lever — a corrected/strengthened dense path (D1) or fielded
index-key enrichment (D2) — materially improves FathomDB across the four quality goals, within the
CPU/no-API/1-bit footprint, judged primarily on cheap LLM-free metrics with priced answer-F1 as
confirmation.** The four goals and their instruments:

| Goal | Instrument | LLM-free? |
|---|---|---|
| Best agentic memory | LongMemEval recall@10/@20 + per-class (factoid/knowledge_update/multi_session/temporal) | yes |
| Excellent exploratory recall | IR corpus eu8 relevance recall@10 | yes |
| Excellent deep-exploratory recall | IR corpus ~596-query hard discrimination subset (recall@10/@50, median rank) | yes |
| Excellent multi-hop QA | MuSiQue all-bridges@K (lead) + pooled ≥3-hop answer-F1 (confirmatory) | bridge=yes; F1=priced |
| System health (hard gate) | IR corpus **eu7 ANN-quantization fidelity recall@10 ≥ 0.90** | yes |

**Pre-registered primary endpoints (frozen at Slice 0; ADJ-1).** The **GO signal leads on the LLM-free
metrics** (runnable at full N, so adequately powered): all-bridges@K, eu8 recall@10 + the hard subset,
and LongMemEval R@10/@20. The **priced MuSiQue pooled ≥3-hop answer-F1 is confirmatory** (bounded N,
cheap-validate → HITL spend gate). Registered comparator stays **fused-RRF (k=60)** (ADJ-4); all-bridges@K
is a registered metric; CE-rerank stays a **logged secondary** arm (it was null on M1), never a default.

**Pre-registered decision rule (frozen at Slice 0).** Because the M1 `decide()` rule was underpowered
even at N=1165 (P(GO)≈0.45 at materiality 0.04), 0.8.3 **re-pre-registers an MDE-feasible materiality**
from the Slice-5 cheap-metric variance, and:
- **D1 GO (promote the corrected/strengthened dense path):** D1a clears **eu7 ≥0.90** AND lifts the
  LLM-free retrieval metrics over the *corrected* fused-RRF baseline by the registered margin (CI lower
  > 0), with priced MuSiQue ≥3-hop F1 point-estimate **not worse than** corrected fused.
- **D2 GO (promote fielded enrichment):** fielded enrichment beats **both** the corrected baseline **and**
  the length-matched placebo on the LLM-free metrics at adequate power.
- **NO-GO:** flat-or-negative ⇒ record the clean negative; the honesty prior holds (across 0.8.1+0.8.2 no
  cheap lever has yet won, so each null is pre-registered, not a moved goalpost).

---

## 1. What "fair" requires (carried from M1, with the adjustments)

- **Strong baseline, same answerer, same depth.** Baseline = BM25 ∪ passage-dense ∪ fused-RRF; identical
  answerer across arms for the confirmatory F1; retrieval is the only variable. Passage-level dense, not
  whole-doc (R4 stays parked).
- **ADJ-3 — eu7 re-clear is a HARD gate.** The CLS fix **changes the stored vectors**; the 1-bit binary
  index must re-pass **eu7 ≥0.90** (large-run value was 0.896) before any product promotion. If the
  CLS-quantized fidelity falls below the floor, that is a **BLOCK → HITL** even if relevance improves —
  surface the tension, do not ship a floor breach silently.
- **ADJ-4 — bridge presence AND composition.** Dense retrieves bridges more often (all-bridges@10 0.68 vs
  fused 0.65) but composes worse (F1-given-all-bridges 0.464 vs 0.552). D1 must lift presence *without*
  degrading fused's composition — hence comparator = fused-RRF and all-bridges@K registered.
- **ADJ-5 — split D1.** D1a (CLS fix → eu7 re-clear → re-measure corrected baseline, no new machinery)
  precedes D1b (deterministic vector PRF vs that baseline), so a pooling-fix gain is never misattributed
  to PRF and each lever is individually falsifiable.
- **D2 placebo discipline.** A length-matched foreign-token placebo separates lexical-bridge *content*
  value from the length artifact (the codex-reviewed R6 control); fielded indexing keeps enrichment
  tokens out of the body channel so they cannot distort body BM25 length-norm.

---

## 2. Cross-cutting Definition of Done (binds every slice)

- **Footprint invariant:** CPU-only, no-API at the library boundary, 1-bit-safe, deterministic. Offline
  extraction = local Qwen3.6-27B (Airlock vLLM batch, $0). The answerer is the one priced, EVAL-ONLY seam.
  Every technique is tagged **IN-LIBRARY / CALLER-SIDE BYO-LLM / OFFLINE-BUILD / EVAL-ONLY**; no in-library
  LLM call.
- **X1 / X2 / X3 (engine slices):** any surface/behavior/schema change lands in **both** Python and
  TypeScript bindings with a live functional harness (X1); `mkdocs build` stays green (X2); `docs/` +
  `dev/DOC-INDEX.md` updated in the closing commit (X3).
- **Determinism pins:** RRF ordering stays byte-deterministic; new fielded/weighted fusion and any custom
  ranking **extend, never weaken** the pins. PRF is deterministic (bounded, fixed expansion).
- **Schema:** the fielded-FTS channel is an additive migration with a `SCHEMA_VERSION` bump + an EXPLAIN
  check (index-driven, no SCAN/temp-B-tree) per the 0.8.1 Slice-33 precedent.
- **Budget discipline ([[0.8.1-budget-discipline-cheap-validate-and-ledger]]):** cheap-validate with
  `gemini-2.5-flash-lite` before any priced answerer run; strong reader = `gpt-5.4` (M1-proven cheap +
  resilient); running $ ledger in `runs/STATUS-0.8.3.md`.
- **Resilient priced runs ([[priced-runs-need-resilience-before-spend]]):** reuse M1's resilient-by-
  construction harness (auto-resume, atomic checkpoint, 429/5xx backoff, failure≠abstention,
  completeness validity guard) for any confirmatory F1 pass.
- **Pre-registration + reviews:** endpoints + rule frozen as code before data; design reviewed; codex §9
  on every slice (engine + harness) before a verdict is recorded; cross-check green claims against printed
  numbers ([[background-exit-masks-real-exit]]).

---

## 3. Critical path

`0 (design + pre-register + F5-scope ratify) → 5 (D1a: CLS engine fix + eu7 re-clear + corrected baseline)
→ { 10 (D1b: deterministic vector PRF arm + D1 verdict)  ∥  15 (D2 engine: fielded-FTS/BM25F + tunable-b) }
→ 20 (D2 build+measure + 0.8.3 verdict)`.

Slices 10 (D1b harness) and 15 (D2 engine) are independent off Slice 5 and may run in parallel; Slice 20
joins them (it indexes into 15's fielded channel and fuses 10's PRF arm into the final stack).

---

## 4. Per-slice contracts

### Slice 0 — Design + pre-registration + F5 scope ratify · `[design-adr]` · depends-on: — · gaps: 1–4
**Objective.** Author/sign the 0.8.3 design: the D1/D2 endpoints, the **re-pre-registered** decision rule
(ADJ-1, MDE-feasible materiality, LLM-free lead + priced confirmatory), the strong-baseline definition, the
power plan, and the **ADJ-2 scope ruling** (promote F5/tunable-`b` into 0.8.3, re-pointing the F5 ADR — or
defer D2 to body-append). Freeze the rule as code (extend M1's `eval/m1_decision_rule.py` → a 0.8.3 module).
**Deliverables:** (1) `dev/design/0.8.3-nongraph-retrieval.md` (datasets, arms, endpoints, rule, placebo,
footprint tags); (2) the falsifiable Slice 5/10/15/20 AC list; (3) the budget plan + the F5-ADR re-point.
**Acceptance bar (replaces TDD):** design `status: decision-ready`; primary endpoints + rule frozen + dated.
**HITL gate:** sign before any engine commitment (Slice 5/15) or priced run (Slice 10/20); the F5-scope
ruling is part of this gate.
**Reserved follow-on (1–4):** a power re-estimate if Slice-5 cheap-metric variance is wider than assumed.

### Slice 5 — D1a: CLS-pooling engine fix + eu7 re-clear + corrected baseline · `[implementation (engine + measurement)]` · depends-on: 0 · gaps: 6–9
**Objective.** Fix `CandleBgeEmbedder` to use **CLS pooling** for `bge-small-en-v1.5` (it defaults to
`Mean`; STATUS-0.8.2 tracked bug). Re-embed/rebuild the index; **re-clear eu7 ≥0.90** on the frozen IR
corpus; then **re-measure the corrected dense / fused-RRF baseline** across all four instruments — adding
no new retrieval machinery. This is the new baseline every later comparison is judged against.
**TDD.** RED: a Rust test pinning CLS pooling (output vector matches a CLS reference, not Mean) + a binding
functional test; a harness test asserting eu7 recall@10 is computed on the pinned corpus and emits the
fidelity artifact. GREEN: the fix + the re-measure runner. **eu7 gate:** if CLS-quantized fidelity < 0.90 →
**BLOCK → HITL** (do not ship a floor breach). Outputs → `runs/0.8.3-d1a-eu7.json`,
`runs/0.8.3-d1a-baseline-{musique,longmemeval,ir}-n*.json`.
**DoD.** X1 (Py+TS embedder behavior parity + functional harness), X2, X3. Footprint: IN-LIBRARY, CPU,
deterministic. Codex §9 (engine correctness + the fidelity claim).
**Reserved follow-on (6–9):** if eu7 is borderline, a quantization-aware re-check (rotation/whitening of
the CLS vectors before 1-bit sign-quant) to recover fidelity headroom — measurement first, no pre-commit.

### Slice 10 — D1b: deterministic vector PRF arm + D1 verdict · `[implementation (measurement)]` · depends-on: 5 · gaps: 11–14
**Objective.** Add **deterministic vector pseudo-relevance feedback** (bounded query-vector expansion from
the top-m corrected-dense/fused candidates → one re-query → deterministic RRF over original-lexical,
original-dense, PRF-dense), as a treatment vs the Slice-5 corrected baseline. No LLM. Run the **D1 verdict**:
LLM-free lead metrics (all-bridges@K, eu8 + hard subset, LongMemEval R@10/@20) + confirmatory MuSiQue F1.
**TDD.** RED: PRF determinism (identical input → byte-identical ranking); a capped-expansion test (no
unbounded drift); a no-regression pin (PRF never drops a baseline top-K below its floor); the verdict
harness asserts it computes the *pre-registered* endpoint and derives GO/NO-GO mechanically. GREEN: the arm
+ runner. Cheap-validate before the priced confirmatory pass. Output → `runs/0.8.3-d1b-verdict-n*.json`.
**DoD.** IN-LIBRARY (PRF in the engine read path, CPU, deterministic) or harness-measured first then promoted
— state which. Codex §9. **HITL gate:** D1 promotion (ship corrected-dense ± PRF) is HITL-signed.
**Reserved follow-on (11–14):** PRF source ablation (expand from reranked-f32 top-k vs fused top-k) only if
D1b is borderline.

### Slice 15 — D2 engine: fielded-FTS / BM25F channel + tunable-`b` ranking · `[implementation (engine)]` · depends-on: 5 · gaps: 16–19
**Objective.** Land the **F5 promotion (ADJ-2):** a separate **fielded / sidecar FTS5 channel** for
enrichment keys (entity/alias/fact/temporal fields) kept **out of the body channel**, with **per-column /
BM25F weighting** and a **tunable-/lower-`b`** length-norm path (FTS5's `b` is fixed → custom deterministic
ranking). Additive schema migration + `SCHEMA_VERSION` bump + EXPLAIN index-driven check.
**TDD.** RED: schema-migration test (additive, version bump, EXPLAIN no-SCAN); a fielded-search test
(query matches a doc via its entity/fact field, body unaffected); a BM25F weighting test (column weights
change ranking deterministically); a `b`-sweep determinism test. GREEN: the engine + bindings. **Determinism
pins extended, not weakened.** Outputs: migration + the new search surface.
**DoD.** X1 (Py+TS fielded-search + weighting parity + functional harness), X2, X3 + DOC-INDEX. Footprint:
IN-LIBRARY, CPU, deterministic, no body-channel contamination. Codex §9 (schema + ranking determinism).
**Reserved follow-on (16–19):** field-weight auto-tuning is OUT (would need a learned ranker) — keep weights
a deterministic config; record any weight grid searched (no silent caps).

### Slice 20 — D2 build + measure + 0.8.3 verdict · `[implementation (offline-build + measurement)]` · depends-on: 15, 10 · gaps: 21–24
**Objective.** Offline-extract enrichment keys (reuse the 0.8.2 cached MuSiQue extractions where present;
extract for LongMemEval sessions + the IR corpus with local Qwen, $0) → index into the Slice-15 fielded
channel → measure **enriched vs plain (corrected baseline) vs length-matched placebo** across the LLM-free
instruments, fused with the Slice-10 PRF stack; priced MuSiQue F1 confirmatory. Apply the pre-registered
rule → the **0.8.3 GO/NO-GO** for D2 and the overall version verdict.
**TDD.** RED: a coverage test (every sampled doc/session has enrichment keys; fielded index populated); the
verdict harness asserts the pre-registered endpoint + the **enriched-must-beat-BOTH-plain-AND-placebo** gate;
a placebo length-match assertion. GREEN: build + index + measure runner. Cheap-validate before the priced
pass. Outputs → `runs/0.8.3-d2-verdict-n*.json` + report `runs/0.8.3-report.md`.
**DoD.** $ ledger finalized. OFFLINE-BUILD extraction ($0); IN-LIBRARY fielded search. Codex §9 (harness +
report claims; cross-check green vs printed numbers). **HITL gate:** the 0.8.3 verdict (and what, if
anything, ships) is HITL-signed; update `runs/STATUS-0.8.3.md` + DOC-INDEX + the 0.8.4 "prior" framing.
**Reserved follow-on (21–24):** if D2 is borderline GO, the entity-only vs fact-only vs alias-only field
ablation decides which fields carry the value before committing the BM25F weight vector.

---

## 5. What 0.8.3 deliberately does NOT do

- **No graph traversal / PPR / BFS** — refuted twice; stays dropped. Any graph re-entry needs a
  fundamentally different, evidence-backed mechanism (→ not here).
- **No in-library LLM** — query rewrite / HyDE / decomposition / IRCoT / answer generation are
  **caller-side BYO-LLM only**; the library exposes deterministic search + the fielded channel + PRF.
- **No whole-doc dense (R4), no bundled CPU extractor (R3b), no portable-DB vector guard** — those stay in
  [`../roadmap/0.8.5.md`](../roadmap/0.8.5.md); R3b's gate is now S1-alone (0.8.4).
- **GraphRAG sensemaking (S1)** is **0.8.4**, a different structure on a different axis — not 0.8.3.

## 6. Reuse inventory (new infra is minimal + footprint-safe)

| Need | Reused asset |
|---|---|
| MuSiQue corpus + bridge diagnostic + 5-arm QA harness | 0.8.2 M1 (`musique_hash 3cff37fd…`, cached extractions, resilient priced runner) |
| LongMemEval recall harness | 0.8.1 (`graph_arm_recall.py` recall path, per-class strata) |
| IR corpus eu7/eu8 + hard subset | 0.8.0 eval (`corpus_hash fe973fcd`, frozen qrels) |
| Lexical / dense / fusion / CE-rerank | engine (FTS5, 1-bit ANN + f32 rerank, RRF k=60, TinyBERT-L-2 via `fathomdb.rerank`) |
| Offline extraction | Qwen3.6-27B Airlock vLLM batch ($0) |
| Answerer (confirmatory only) | gpt-5.4 (temp 0, seed 0); cheap-validate = gemini-2.5-flash-lite |
| Decision-rule frozen-as-code | extend `eval/m1_decision_rule.py` → `eval/m1_decision_rule.py`-shaped 0.8.3 module |
| New (engine) | CLS-pooling fix; fielded-FTS/BM25F channel + tunable-`b` ranking (the F5 promotion) |
| New (harness) | deterministic vector PRF arm; enrichment-key builder + fielded index loader; placebo for the fielded channel |
