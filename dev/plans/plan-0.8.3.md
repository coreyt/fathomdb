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
>
> **EXTENDED 2026-06-21 (capability-status-report review, ADJ-6..10 — measurement scope HITL-approved).**
> The capability report (`runs/0.8.x-capability-status-report.md`) surfaced two facts that re-shape the
> sequence: the dense arm is **embedder-bound** (eu8 ceiling ≈0.571 on `bge-small`; neither the pooling
> fix nor PRF raises it), and FathomDB has **no end-to-end agentic-memory accuracy number** vs Mem0/Zep
> (the product's defining axis is unmeasured). So 0.8.3 now (1) inserts two **$0 cheap gates** — an
> **embedder-ceiling probe** (ADJ-6) and a **D2 content-at-scale harness-proxy** (ADJ-7) — *before* the
> two engine builds, so neither migration is committed on an unvalidated lever; (2) adds a first-class
> **D0 measurement-unblock** track (ADJ-8) so the parity number becomes producible; (3) makes
> **latency-regression** a DoD gate on read-path slices (ADJ-9); (4) reframes D1 as **corrected-fused vs
> corrected-dense with composition co-primary** (ADJ-10); and (5) pre-registers a **both-null fallback**.
> The ADJ-6..10 measurement + prerequisite work is **HITL-approved (2026-06-21)** and may begin ahead of
> the Slice-0 gate; **engine commitment (Slices 5, 20) and priced runs still wait on Slice-0 sign-off.**

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
| Best agentic memory (retrieval) | LongMemEval recall@10/@20 + per-class (factoid/knowledge_update/multi_session/temporal) | yes |
| Best agentic memory (end-to-end parity) — **D0, ADJ-8** | identical-answerer accuracy vs Mem0-OSS + naive-RAG on a **re-pinned gold corpus** (non-zero gold on the 4 memory classes) | priced (cheap-validate → HITL) |
| Excellent exploratory recall | IR corpus eu8 relevance recall@10 | yes |
| Excellent deep-exploratory recall | IR corpus ~596-query hard discrimination subset (recall@10/@50, median rank) | yes |
| Dense embedder ceiling — **ADJ-6** | eu8 recall@10 + hard subset, corrected `bge-small` vs stronger CPU/ONNX embedders ($0 A/B) | yes |
| Excellent multi-hop QA | MuSiQue **all-bridges@K (presence)** + **F1-given-all-bridges (composition)** — co-primary (ADJ-10); pooled ≥3-hop answer-F1 confirmatory | bridge/composition=yes; F1=priced |
| System health (hard gate) | IR corpus **eu7 ANN-quantization fidelity recall@10 ≥ 0.90** | yes |
| System health — engine latency (hard gate) — **ADJ-9** | **AC012 read p50/p99 · AC013 scan p50/p99 · AC020 concurrent speedup** re-measured on read-path slices | yes |

**Pre-registered primary endpoints (frozen at Slice 0; ADJ-1, ADJ-10).** The **GO signal leads on the
LLM-free metrics** (runnable at full N, so adequately powered): MuSiQue **all-bridges@K (presence)** AND
**F1-given-all-bridges (composition)** as co-primaries (ADJ-10), eu8 recall@10 + the hard subset, and
LongMemEval R@10/@20. The **priced MuSiQue pooled ≥3-hop answer-F1 is confirmatory** (bounded N,
cheap-validate → HITL spend gate). Registered comparator stays **fused-RRF (k=60)** (ADJ-4); CE-rerank
stays a **logged secondary** arm (it was null on M1), never a default. **The two $0 gates (ADJ-6 embedder
ceiling, ADJ-7 D2 content-at-scale) are registered pass/fail pre-conditions** for the D1b and D2-engine
slices respectively.

**Pre-registered decision rule (frozen at Slice 0).** Because the M1 `decide()` rule was underpowered
even at N=1165 (P(GO)≈0.45 at materiality 0.04), 0.8.3 **re-pre-registers an MDE-feasible materiality**
from the Slice-5 cheap-metric variance, and:
- **D1 GO (strengthen the dense MEMBER of the fused stack; ADJ-10):** D1a clears **eu7 ≥0.90**, the
  corrected/strengthened dense arm lifts the LLM-free co-primaries over the *corrected fused-RRF* baseline
  by the registered margin (CI lower > 0) **without degrading composition** (F1-given-all-bridges not
  worse than corrected fused), with priced MuSiQue ≥3-hop F1 point-estimate **not worse than** corrected
  fused. A dense arm that wins presence but **loses composition is a NO-GO**, not a GO.
- **D2 GO (promote fielded enrichment):** the ADJ-7 proxy passes at scale AND fielded enrichment beats
  **both** the corrected baseline **and** the length-matched placebo on the LLM-free metrics at adequate
  power.
- **NO-GO:** flat-or-negative ⇒ record the clean negative; the honesty prior holds (across 0.8.1+0.8.2 no
  cheap lever has yet won, so each null is pre-registered, not a moved goalpost).
- **Both-null fallback (pre-registered, not a redirect):** if D1 and D2 are both NO-GO, the next fork is
  named now — (a) **embedder upgrade** (carry the ADJ-6 winner forward), (b) the **reader / answer-
  synthesis axis** (the capability report's ~5× reader swing), or (c) finish **D0** so end-to-end parity
  can be claimed/bounded. 0.8.3 ends with a decision among these, not an open redirect.

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
- **ADJ-6 — embedder ceiling probed before PRF.** Dense is embedder-bound; the corrected `bge-small`
  ceiling is A/B'd against stronger CPU/ONNX embedders ($0, LLM-free) and **D1b PRF only proceeds if the
  best feasible embedder leaves headroom.** A stronger embedder that breaks the ceiling becomes the D1
  lever and is carried forward (re-clearing eu7) instead of PRF.
- **ADJ-7 — D2 validated at scale before the engine migration.** A $0 in-harness separate-index proxy
  must show fielded content beating the length-matched placebo at adequate N (and that fielding removes
  the FTS length-norm penalty) **before** the Slice-20 schema migration is built. Cheap-validate-before-
  engine, mirroring cheap-validate-before-spend.
- **ADJ-9 — latency is a fairness axis.** Engine/read-path changes re-measure AC012/AC013/AC020; a budget
  breach blocks promotion (→ HITL) just like an eu7 breach — retrieval quality bought at silent latency
  cost is not a fair win.
- **ADJ-10 — composition co-primary.** The registered comparator is fused-RRF and D1 must lift bridge
  *presence* without degrading *composition*; the dense arm is evaluated as a **member of the fused
  stack**, not a replacement for it.

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
- **Cheap-validate-before-ENGINE (ADJ-6/ADJ-7):** no schema migration or in-engine read-path lever is
  built until its $0 LLM-free gate passes — D1b PRF waits on the embedder-ceiling probe (Slice 10), the
  D2 fielded-FTS migration waits on the D2 content-at-scale proxy (Slice 10). A failed gate ⇒ the engine
  slice does not run; record the negative.
- **Latency-regression gate (ADJ-9):** every engine / read-path slice (5, 20) and the fused-stack verdict
  (25) re-measures **AC012/AC013/AC020** against the current budgets and emits the artifact; a breach is a
  **BLOCK → HITL**, never shipped silently (same discipline as the eu7 floor).
- **Resilient priced runs ([[priced-runs-need-resilience-before-spend]]):** reuse M1's resilient-by-
  construction harness (auto-resume, atomic checkpoint, 429/5xx backoff, failure≠abstention,
  completeness validity guard) for any confirmatory F1 pass.
- **Pre-registration + reviews:** endpoints + rule frozen as code before data; design reviewed; codex §9
  on every slice (engine + harness) before a verdict is recorded; cross-check green claims against printed
  numbers ([[background-exit-masks-real-exit]]).

---

## 3. Critical path

`0 (design + pre-register + F5-scope ratify + both-null fallback)
→ 5 (D1a: CLS engine fix + eu7 re-clear + corrected baseline + latency)
→ 10 (NEW $0 gates: embedder-ceiling probe ∥ D2 content-at-scale proxy)
→ { 15 (D1b: vector PRF — gated by the embedder probe — + D1 verdict)  ∥  20 (D2 engine: fielded-FTS/BM25F
+ tunable-b — gated by the D2 proxy — + latency) }
→ 25 (D2 build + measure + 0.8.3 verdict + latency)`.

**Slice 10 is the cheap de-risking junction:** two $0 LLM-free probes that decide whether the two engine
builds are worth committing. **D1b (15)** proceeds only if the corrected dense arm shows headroom under the
best feasible embedder (else carry the stronger embedder forward instead of PRF); the **D2 engine (20)**
proceeds only if fielded enrichment beats the placebo at scale (else D2 defers / drops). Slices 15 and 20
are independent off Slice 10 and may run in parallel; **Slice 25** joins them (it indexes into 20's fielded
channel and fuses 15's PRF/embedder arm into the final stack and re-checks latency).

**Parallel HITL-approved track — D0 (measurement-unblock, non-blocking).** `D0a (gold-corpus re-pin +
answerer seam) → D0b (Mem0-OSS baseline)` runs alongside the ladder from the start; it gates nothing on
the critical path but makes the end-to-end agentic-memory parity number producible within 0.8.3 (ADJ-8).

---

## 4. Per-slice contracts

### Slice 0 — Design + pre-registration + F5 scope ratify · `[design-adr]` · depends-on: — · gaps: 1–4
**Objective.** Author/sign the 0.8.3 design: the D1/D2 endpoints (incl. the **ADJ-10 composition
co-primary**), the **re-pre-registered** decision rule (ADJ-1, MDE-feasible materiality, LLM-free lead +
priced confirmatory) **with the both-null fallback fork named**, the strong-baseline definition, the
power plan, the **ADJ-2 scope ruling** (promote F5/tunable-`b` into 0.8.3, re-pointing the F5 ADR — or
defer D2 to body-append), and the **registered pass/fail criteria for the two ADJ-6/ADJ-7 $0 gates**
(embedder-ceiling probe; D2 content-at-scale proxy). Freeze the rule as code (extend M1's
`eval/m1_decision_rule.py` → a 0.8.3 module).
**Deliverables:** (1) `dev/design/0.8.3-nongraph-retrieval.md` (datasets, arms, endpoints, rule, placebo,
the two $0-gate criteria, the both-null fallback, footprint tags); (2) the falsifiable Slice 5/10/15/20/25
+ D0 AC list; (3) the budget plan + the F5-ADR re-point.
**Acceptance bar (replaces TDD):** design `status: decision-ready`; primary endpoints + rule + gate
criteria + fallback frozen + dated.
**HITL gate:** sign before any engine commitment (Slice 5/20) or priced run (Slice 15/25); the F5-scope
ruling is part of this gate. **Note:** the ADJ-6..10 measurement + prerequisite work (Slice 10 probes,
the D0 track) is **HITL-approved (2026-06-21)** and may begin ahead of this sign-off; only engine
commitment and priced runs wait on it.
**Reserved follow-on (1–4):** a power re-estimate if Slice-5/10 cheap-metric variance is wider than assumed.

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
deterministic. Codex §9 (engine correctness + the fidelity claim). **ADJ-9 latency gate:** re-measure
**AC012/AC013/AC020** after re-embed/index rebuild and emit `runs/0.8.3-d1a-latency.json`; a budget breach
→ **BLOCK → HITL** (re-embed must not silently regress read/scan/concurrency).
**Reserved follow-on (6–9):** if eu7 is borderline, a quantization-aware re-check (rotation/whitening of
the CLS vectors before 1-bit sign-quant) to recover fidelity headroom — measurement first, no pre-commit.

### Slice 10 — NEW $0 gates: embedder-ceiling probe ∥ D2 content-at-scale proxy · `[implementation (measurement, $0 LLM-free)]` · depends-on: 5 · gaps: 11–14
**Objective (ADJ-6, ADJ-7 — HITL-approved).** Two independent $0, LLM-free probes off the Slice-5
corrected baseline that gate the two engine builds:
- **(10a) Embedder-ceiling probe (ADJ-6).** A/B the corrected `bge-small-en-v1.5` ceiling vs 1–2 stronger
  CPU/ONNX-runnable embedders (`bge-base`, `gte-small`, `e5-small`) on the frozen eu8 set + the ~596-q
  hard subset, within the candle/ONNX footprint (`EmbedderChoice::Caller`+ONNX). **Gate output:** does the
  corrected/best-feasible dense arm leave registered headroom over the corrected fused baseline?
- **(10b) D2 content-at-scale proxy (ADJ-7).** Measure fielded-enrichment content value at scale in an
  **in-harness separate index (no schema migration)** vs the corrected baseline and a length-matched
  placebo; confirm content > placebo at adequate N and that fielding removes the FTS length-norm penalty.
**TDD.** RED: a probe-harness test asserting each gate computes its *pre-registered* metric and emits a
mechanical PASS/FAIL (10a headroom rule; 10b enriched-beats-placebo-at-power rule); a placebo length-match
assertion for 10b; determinism of the ONNX embedder load for 10a. GREEN: the two probe runners.
**Gate semantics:** **10a FAIL** ⇒ Slice 15 (PRF) does not run as-is — carry the winning stronger embedder
forward (re-clearing eu7) as the D1 lever instead; **10b FAIL** ⇒ Slice 20 (D2 engine migration) defers/
drops (record the negative). Outputs → `runs/0.8.3-s10a-embedder-ceiling.json`,
`runs/0.8.3-s10b-d2-proxy-n*.json`.
**DoD.** EVAL-ONLY / harness (no engine change, no schema change, no priced LLM). Footprint: CPU, $0,
deterministic. Codex §9 (probe validity + the gate logic). **No HITL gate** (HITL-approved measurement) —
but the gate *outcomes* are recorded and feed the Slice 15/20 go/no-go.
**Reserved follow-on (11–14):** if 10a is borderline, one more embedder candidate; if 10b is borderline,
an entity-only vs fact-only field split before the field set is frozen for Slice 20.

### Slice 15 — D1b: deterministic vector PRF arm + D1 verdict · `[implementation (measurement)]` · depends-on: 10 · gaps: 16–19
**Gate (ADJ-6).** Runs **only if Slice 10a passed** (corrected dense leaves headroom under the best
feasible embedder). If 10a failed, this slice is **replaced** by "carry the winning stronger embedder
forward (re-embed + re-clear eu7 + re-measure)" as the D1 lever; either way the D1 verdict below is run
against the corrected-fused baseline.
**Objective.** Add **deterministic vector pseudo-relevance feedback** (bounded query-vector expansion from
the top-m corrected-dense/fused candidates → one re-query → deterministic RRF over original-lexical,
original-dense, PRF-dense), as a treatment vs the Slice-5 corrected baseline. No LLM. Run the **D1 verdict**:
LLM-free co-primaries (all-bridges@K presence **and** F1-given-all-bridges composition; ADJ-10), eu8 + hard
subset, LongMemEval R@10/@20 + confirmatory MuSiQue F1.
**TDD.** RED: PRF determinism (identical input → byte-identical ranking); a capped-expansion test (no
unbounded drift); a no-regression pin (PRF never drops a baseline top-K below its floor); a **composition
guard** (PRF must not degrade F1-given-all-bridges below corrected fused; ADJ-10); the verdict harness
asserts it computes the *pre-registered* endpoint and derives GO/NO-GO mechanically. GREEN: the arm +
runner. Cheap-validate before the priced confirmatory pass. Output → `runs/0.8.3-d1b-verdict-n*.json`.
**DoD.** IN-LIBRARY (PRF in the engine read path, CPU, deterministic) or harness-measured first then promoted
— state which. **ADJ-9 latency gate** if PRF lands in the read path (the extra re-query): re-measure
AC012/AC020. Codex §9. **HITL gate:** D1 promotion (ship corrected-dense ± PRF / stronger embedder) is
HITL-signed.
**Reserved follow-on (16–19):** PRF source ablation (expand from reranked-f32 top-k vs fused top-k) only if
D1b is borderline.

### Slice 20 — D2 engine: fielded-FTS / BM25F channel + tunable-`b` ranking · `[implementation (engine)]` · depends-on: 10 · gaps: 21–24
**Gate (ADJ-7).** Runs **only if Slice 10b passed** (fielded content beats the placebo at scale and
fielding removes the length-norm penalty) **and** the Slice-0 ADJ-2 ruling promoted F5/tunable-`b`. If 10b
failed, this engine migration **defers/drops** and D2 records the clean negative — no schema change is
committed on an unvalidated lever.
**Objective.** Land the **F5 promotion (ADJ-2):** a separate **fielded / sidecar FTS5 channel** for
enrichment keys (entity/alias/fact/temporal fields) kept **out of the body channel**, with **per-column /
BM25F weighting** and a **tunable-/lower-`b`** length-norm path (FTS5's `b` is fixed → custom deterministic
ranking). Additive schema migration + `SCHEMA_VERSION` bump + EXPLAIN index-driven check.
**TDD.** RED: schema-migration test (additive, version bump, EXPLAIN no-SCAN); a fielded-search test
(query matches a doc via its entity/fact field, body unaffected); a BM25F weighting test (column weights
change ranking deterministically); a `b`-sweep determinism test. GREEN: the engine + bindings. **Determinism
pins extended, not weakened.** Outputs: migration + the new search surface.
**DoD.** X1 (Py+TS fielded-search + weighting parity + functional harness), X2, X3 + DOC-INDEX. Footprint:
IN-LIBRARY, CPU, deterministic, no body-channel contamination. **ADJ-9 latency gate:** the new fielded
channel + custom ranking re-measure **AC012/AC013/AC020** → `runs/0.8.3-d2-engine-latency.json`; breach →
BLOCK → HITL. Codex §9 (schema + ranking determinism).
**Reserved follow-on (21–24):** field-weight auto-tuning is OUT (would need a learned ranker) — keep weights
a deterministic config; record any weight grid searched (no silent caps).

### Slice 25 — D2 build + measure + 0.8.3 verdict · `[implementation (offline-build + measurement)]` · depends-on: 20, 15 · gaps: 26–29
**Objective.** Offline-extract enrichment keys (reuse the 0.8.2 cached MuSiQue extractions where present;
extract for LongMemEval sessions + the IR corpus with local Qwen, $0) → index into the Slice-20 fielded
channel → measure **enriched vs plain (corrected baseline) vs length-matched placebo** across the LLM-free
instruments, fused with the Slice-15 PRF/embedder stack; priced MuSiQue F1 confirmatory. Apply the
pre-registered rule → the **0.8.3 GO/NO-GO** for D2 and the overall version verdict (incl. the **both-null
fallback fork** if D1+D2 are both NO-GO).
**TDD.** RED: a coverage test (every sampled doc/session has enrichment keys; fielded index populated); the
verdict harness asserts the pre-registered endpoint + the **enriched-must-beat-BOTH-plain-AND-placebo** gate;
a placebo length-match assertion. GREEN: build + index + measure runner. Cheap-validate before the priced
pass. Outputs → `runs/0.8.3-d2-verdict-n*.json` + report `runs/0.8.3-report.md`.
**DoD.** $ ledger finalized. OFFLINE-BUILD extraction ($0); IN-LIBRARY fielded search. **ADJ-9 latency
gate** on the final fused stack (PRF/embedder + fielded channel): re-measure AC012/AC013/AC020 →
`runs/0.8.3-final-latency.json`. Codex §9 (harness + report claims; cross-check green vs printed numbers).
**HITL gate:** the 0.8.3 verdict (and what, if anything, ships, plus the both-null fork if taken) is
HITL-signed; update `runs/STATUS-0.8.3.md` + DOC-INDEX + the 0.8.4 "prior" framing.
**Reserved follow-on (26–29):** if D2 is borderline GO, the entity-only vs fact-only vs alias-only field
ablation decides which fields carry the value before committing the BM25F weight vector.

---

### Parallel track — D0: agentic-memory measurement-unblock · `[HITL-approved, non-blocking]`

> **ADJ-8 — HITL-approved (2026-06-21), runs alongside the ladder; gates nothing on the critical path.**
> Closes the capability report's #1 gap: FathomDB has no end-to-end agentic-memory accuracy number vs
> Mem0/Zep because the answerer + Mem0-OSS backend were blocked and the four memory classes had **N=0
> gold** (since 0.8.1 Slice 25). Goal: make the parity number **producible within 0.8.3**.

### Slice D0a — gold-corpus re-pin + answerer seam · `[implementation (eval-infra) — prerequisite]` · depends-on: — · gaps: D0a.1–4
**Objective.** Rebuild/re-pin the R2 gold corpus so `temporal`/`multi_hop`/`knowledge_update`/
`multi_session` carry **non-zero gold** (the prerequisite that made every memory-class delta null in Slice
25), and wire the identical-answerer seam (`R2_RUN=1`, `R2_ANSWERER_*`) end-to-end. Reuse `r2_parity_eval.py`.
**TDD.** RED: a corpus-validity test asserting each of the four classes has ≥ N_min gold (no silent N=0);
an answerer-seam smoke test (one question → one answer → scored) on the cheap-validate model. GREEN: the
re-pin builder + the wired seam. Output → `runs/0.8.3-d0a-corpus-manifest.json` (new `corpus_hash`).
**DoD.** EVAL-ONLY (answerer is the priced seam; everything else $0). Determinism: pinned `corpus_hash`,
frozen qrels. Codex §9 (corpus validity — no vacuous/empty gold classes; [[acceptance-md-locked-no-feature-acs]]
note: this is eval infra, not a product AC). **Cheap-validate** the seam with `gemini-2.5-flash-lite`
before any priced answerer pass.
**Reserved follow-on (D0a.1–4):** if a class still can't reach N_min from the current source, escalate the
corpus-source decision (HITL) rather than ship an underpowered class.

### Slice D0b — Mem0-OSS baseline stand-up · `[implementation (eval-infra) — measurement]` · depends-on: D0a · gaps: D0b.1–4
**Objective.** Stand up a working **local Mem0-OSS** backend behind the existing R2 adapter so the parity
delta `fathomdb_minus_{mem0,naive_rag}` is computable on the re-pinned corpus under the **identical-answerer**
protocol (one answerer, one prompt, one context budget — any gap is retrieval, not prompt divergence).
**TDD.** RED: an adapter conformance test (Mem0 backend returns top-K under the shared metric contract); a
parity-harness test asserting all three arms run on the same corpus_hash + answerer. GREEN: the backend +
the three-arm runner. Cheap-validate, then the priced parity pass under the resilient runner. Outputs →
`runs/0.8.3-d0b-parity-{recall,accuracy}-n*.json`.
**DoD.** EVAL-ONLY. Budget: $ ledger entry; resilient priced harness ([[priced-runs-need-resilience-before-spend]]).
Codex §9. **HITL gate:** the R2 **metric/threshold** stays a HITL eval gate (per [[fathomdb-recall-fidelity-vs-relevance]]
the parity claim is a product-value judgement) — but standing up the baseline + producing the number is
the approved deliverable. Report the e2e parity number into `runs/0.8.3-report.md` and the conclusion of
`runs/0.8.x-capability-status-report.md` (clears its #1 caveat).
**Reserved follow-on (D0b.1–4):** none — a second external comparator is **explicitly OUT of 0.8.3** to
keep D0 focused on **Mem0-type function**. The **Graphiti/Zep** agentic-memory head-to-head (and the
**GraphRAG/HippoRAG** multi-hop head-to-head) is a recorded **0.8.4 known-gap** — see
[`../roadmap/0.8.4.md`](../roadmap/0.8.4.md) §5.

---

## 5. What 0.8.3 deliberately does NOT do

- **No graph traversal / PPR / BFS** — refuted twice; stays dropped. Any graph re-entry needs a
  fundamentally different, evidence-backed mechanism (→ not here).
- **No in-library LLM** — query rewrite / HyDE / decomposition / IRCoT / answer generation are
  **caller-side BYO-LLM only**; the library exposes deterministic search + the fielded channel + PRF.
- **No whole-doc dense (R4), no bundled CPU extractor (R3b), no portable-DB vector guard** — those stay in
  [`../roadmap/0.8.5.md`](../roadmap/0.8.5.md); R3b's gate is now S1-alone (0.8.4).
- **GraphRAG sensemaking (S1)** is **0.8.4**, a different structure on a different axis — not 0.8.3.
- **No competitor head-to-head beyond Mem0.** D0 stands up **Mem0-OSS only** (Mem0-type function). The
  **Graphiti/Zep** (agentic-memory) and **GraphRAG/HippoRAG** (multi-hop) as-is head-to-heads are a
  recorded **0.8.4 known-gap** ([`../roadmap/0.8.4.md`](../roadmap/0.8.4.md) §5), not 0.8.3 scope; until
  then those competitors stay literature-only / not apples-to-apples (per
  [`runs/0.8.x-capability-status-report.md`](runs/0.8.x-capability-status-report.md)).

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
| Agentic-memory parity harness (D0) | 0.8.1 `r2_parity_eval.py` (identical-answerer, shared adapter/metric contract) |
| Enrichment value at scale (D2 proxy) | extend `eval/r6_index_key_enrichment.py` (placebo control) into the in-harness separate-index proxy |
| New (engine) | CLS-pooling fix; fielded-FTS/BM25F channel + tunable-`b` ranking (the F5 promotion) |
| New (harness, $0) | embedder-ceiling probe (ONNX `bge-base`/`gte-small`/`e5-small` via `EmbedderChoice::Caller`); D2 content-at-scale proxy; deterministic vector PRF arm; enrichment-key builder + fielded index loader; placebo for the fielded channel |
| New (eval-infra, D0) | re-pinned gold corpus (non-zero gold on the 4 memory classes) + answerer seam; local **Mem0-OSS** baseline behind the R2 adapter |
| Latency gate (ADJ-9) | existing AC012/AC013/AC020 perf harness, re-run on read-path slices |
