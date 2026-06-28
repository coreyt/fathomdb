# FathomDB 0.8.3 — Plan: reach near-parity-or-better vs Mem0 (agentic memory)

> **What this version is — the RESOLUTION.** 0.8.3's success is no longer "does a retrieval lever beat
> a synthetic comparator." It is a measured outcome: **FathomDB reaches near-parity-or-better with Mem0
> on agentic memory**, end-to-end, under an identical-answerer protocol — with **Graphiti/Zep** as a
> second comparator on the same axis. The non-graph levers M1 surfaced (D1 dense/embedder, D2 fielded
> enrichment) are **means to close the Mem0 gap**, not ends. (History: M1 closed NO-GO — graph multi-hop
> refuted on both recall and answer-F1, `runs/0.8.2-m1-FINDINGS.md`; the 2026-06-21 capability report,
> `runs/0.8.x-capability-status-report.md`, found the product's defining number — identical-answerer
> accuracy vs Mem0/Zep — was **never measured**, with the four memory classes at N=0 gold since 0.8.1
> Slice 25. 0.8.3 makes that number exist and drives it to parity.)
>
> **Parity is the floor, not the ceiling — surpass-option before sign-off (HITL 2026-06-21).** Whenever a
> lever is projected to *overshoot* parity (a stronger embedder clearing the eu8 ceiling with headroom; a
> memory-class-specific lever), the orchestrator **presents the surpass-Mem0 experiment as an explicit
> option BEFORE the HITL signs off the parity gate.** Sign-off is a choice between "ship at parity" and
> "spend to surpass" — never a silent stop at the parity line.

Ladder shape + reserved-gap policy: reuse [`0.8.1-plan.md`](0.8.1-plan.md) §"Ladder".
Process: [`../design/orchestration.md`](../design/orchestration.md) (three-role separation, codex §9,
worktrees §11). Slice prompts generate from [`prompts/0.8.0-SLICE-TEMPLATE.md`](prompts/0.8.0-SLICE-TEMPLATE.md).
Roadmap home + adjustments ADJ-1..10: [`../roadmap/0.8.3.md`](../roadmap/0.8.3.md). Capability evidence:
[`runs/0.8.x-capability-status-report.md`](runs/0.8.x-capability-status-report.md).

**0.8.3 contains real ENGINE changes** (CLS-pooling fix; a possible embedder swap; fielded-FTS / BM25F +
tunable-`b`). Engine slices carry X1 SDK parity, schema migration + `SCHEMA_VERSION` bump, determinism
pins, the eu7 fidelity floor, and an ADJ-9 latency gate. **ADJ-2: D2's fielded form promotes the F5
(BM25F/fielded-FTS) + tunable-`b` levers forward from 0.8.5** — ratified at the Slice-0 HITL gate.

---

## 0. Goal (0.8.3) — the resolution gate

**Reach `FathomDB − Mem0` ≥ −ε (near-parity) or > 0 (better) on the agentic-memory axis**, per memory
class, under the identical answerer, with **eu7 ≥ 0.90 held** and **no latency-budget breach** —
secondary comparator **Graphiti/Zep**. The resolution is achieved by closing the measured gap with the
**fewest sufficient experiments**, then stopping at parity (or, on HITL choice, pursuing a surpass lever).

### Two-level measurement (the key reframe)

- **External — the resolution gate.** `FathomDB − {Mem0, Graphiti/Zep, naive-RAG}` per memory class
  (factoid / knowledge_update / multi_session / temporal), identical-answerer, on the **re-pinned,
  power-sized** gold corpus. This decides *done*.
- **Internal — attribution only.** Each lever vs the corrected-fused baseline (the M1 comparator). Tells
  us *which* lever closed the gap; it does not, by itself, mean the resolution is reached.

Slice 0 freezes both in code (`eval/decision_rule_083.py`, extending `eval/m1_decision_rule.py`).

### Goals → instruments

| Goal | Instrument | Role |
|---|---|---|
| **Agentic-memory parity (resolution)** | identical-answerer accuracy + strict Recall@K vs **Mem0 / Graphiti-Zep / naive-RAG** on the re-pinned gold (non-zero gold on the 4 classes) | **GATE** (external) |
| Exploratory recall | IR corpus eu8 relevance recall@10 | lever signal |
| Deep-exploratory recall | IR corpus ~596-query hard subset (recall@10/@50, median rank) | lever signal |
| Multi-hop QA | MuSiQue all-bridges@K (presence) **and** F1-given-all-bridges (composition) co-primary (ADJ-10); pooled ≥3-hop F1 confirmatory | lever signal |
| System health (hard gate) | IR corpus **eu7 fidelity recall@10 ≥ 0.90** | BLOCK gate |
| System health — latency (hard gate, ADJ-9) | AC012/AC013/AC020 re-measured on read-path slices | BLOCK gate |

### Pre-registered decision rule (frozen at Slice 0)

- **Resolution REACHED:** the external `FathomDB − Mem0` per-class delta clears the near-parity band on
  the **power-sized** corpus (per-class MDE ≤ the parity tolerance; ADJ-1/ADJ-3 power discipline), eu7 ≥
  0.90, latency intact. ⇒ **Present the surpass-option package; HITL signs "ship at parity" or "pursue
  surpass."**
- **Resolution NOT reached:** record the **residual gap + the named binding constraint** (most likely the
  embedder ceiling or the reader), and the explicit fork — (a) carry a stronger embedder, (b) the
  caller-side reader axis (out of the library footprint), or (c) a memory-class-specific lever. This
  replaces the old "both-null fallback": 0.8.3 always ends in a *decision*, never an open redirect.
- **Internal attribution** (lever vs corrected-fused, MDE-feasible materiality — the M1 rule was
  underpowered even at N=1165) is reported per lever so the gap-closure is explainable.

---

## 1. What "fair" requires (carried from M1 + the capability report)

- **Identical-answerer, same depth, one corpus.** Adapters expose only `retrieve(question, k)`; one
  answerer / one prompt / one context budget across FathomDB, Mem0, Graphiti/Zep, naive-RAG — so any
  delta is retrieval, not prompt divergence (the R2 invariant). Reader = `gpt-5.4` (M1-proven), the one
  priced EVAL-ONLY seam; cheap-validate (`gemini-2.5-flash-lite`) before any priced run.
- **Power before claims (ADJ-1/ADJ-3).** The base-retrieval study had per-class MDE ≈21pp at n=160 — too
  coarse for 5–7.5pp class deltas. Slice 0 sizes per-class N so **MDE ≤ the parity tolerance**; D0a must
  hit that N per class (escalate the source if it can't). No parity claim on an under-powered class.
- **eu7 re-clear is a HARD gate (ADJ-3).** Any embedder/pooling change rewrites the stored vectors; the
  1-bit index must re-pass **eu7 ≥0.90** (point 0.896, CI-hi 0.925 today). A floor breach is **BLOCK →
  HITL**, even if relevance improves. **The 0.937→0.896 "regression" is now bisected (2026-06-22,
  `runs/0.8.3-eu7-bisect-report.md`): NOT the CLS/embedding path (case B ruled out — embedder src
  byte-identical v0.7.2→v0.8.0); it is case A (vector-path/SUT), most consistent with a measurement-SUT
  change (0.937 = pre-correction `search()` anchor, 0.896 = the `vector_stage_only` seam — not directly
  comparable), with no fidelity-loss commit found. So 0.937 is NOT a recoverable target; judge Slice-20
  eu7 fresh against the 0.90 floor, and if it breaches, the fork is the QUANT path, not pooling.**
- **Embedder is a first-class lever, not a contingency (ADJ-6).** Dense is embedder-bound (eu8 ceiling
  ≈0.571; fused *ties* BM25). So the $0 embedder probe is **primary**: it selects the lever, and PRF is
  secondary. **Re-embed once** on the chosen embedder (CLS-correct), re-clear eu7 once — no double-jeopardy.
- **Composition co-primary (ADJ-10).** Dense gets bridges more often (0.68 vs 0.65) but composes worse
  (0.464 vs 0.552); a lever that wins presence but loses composition is **not** progress.
- **Cheap-validate-before-ENGINE (ADJ-7).** No schema migration / in-engine read-path lever is built
  until its $0 LLM-free gate passes (embedder probe before the re-embed/PRF build; D2 content-at-scale
  proxy before the fielded-FTS migration). A failed gate ⇒ the engine slice does not run; record it.
- **D2 placebo discipline.** A length-matched foreign-token placebo separates lexical-bridge *content*
  value from the length artifact; fielded indexing keeps enrichment tokens out of the body channel.
- **Latency is a fairness axis (ADJ-9).** Read-path changes re-measure AC012/AC013/AC020; a breach blocks
  promotion (→ HITL), same as eu7.

---

## 2. Cross-cutting Definition of Done (binds every slice)

- **Footprint invariant:** CPU-only, no-API at the library boundary, 1-bit-safe, deterministic. Offline
  extraction = local Qwen3.6-27B ($0). The answerer is the one priced, EVAL-ONLY seam. Every technique is
  tagged **IN-LIBRARY / CALLER-SIDE BYO-LLM / OFFLINE-BUILD / EVAL-ONLY**; no in-library LLM.
- **X1 / X2 / X3 (engine slices):** surface/behavior/schema changes land in **both** Python and
  TypeScript bindings + a live functional harness (X1); `mkdocs build` green (X2); `docs/` + DOC-INDEX in
  the closing commit (X3).
- **Determinism pins:** RRF byte-deterministic; fielded/weighted fusion + custom ranking **extend, never
  weaken** the pins; PRF deterministic (bounded, fixed expansion).
- **Schema:** the fielded-FTS channel is additive + `SCHEMA_VERSION` bump + EXPLAIN index-driven (no
  SCAN/temp-B-tree) per the 0.8.1 Slice-33 precedent.
- **Budget discipline ([[0.8.1-budget-discipline-cheap-validate-and-ledger]]):** cheap-validate before any
  priced run; strong reader = `gpt-5.4`; $ ledger in `runs/STATUS-0.8.3.md`; **the three priced runs
  (parity, D1 confirmatory, D2 confirmatory) are budgeted in aggregate at Slice 0**.
- **Resilient priced runs ([[priced-runs-need-resilience-before-spend]]):** reuse M1's resilient harness
  (auto-resume, atomic checkpoint, 429/5xx backoff, failure≠abstention, completeness validity guard).
- **Latency-regression gate (ADJ-9):** every engine / read-path slice (20; 25) and the final stack (30)
  re-measures AC012/AC013/AC020; a breach is a **BLOCK → HITL**.
- **Surpass-option discipline:** at every Phase-B gate the orchestrator records whether a credible
  surpass-Mem0 lever exists; the Slice-30 sign-off package presents it.
- **Pre-registration + reviews:** endpoints + rule frozen as code before data; design reviewed; codex §9
  on every slice before a verdict is recorded; cross-check green claims vs printed numbers
  ([[background-exit-masks-real-exit]]).

---

## 3. Critical path (resolution-driven: measure the gap → close it cheapest-first → stop at parity)

```text
0  Design + pre-register the resolution gate (two-level), power-sized N, $0-probe criteria,
   eu7-break fork, surpass-option protocol, F5/tunable-b scope ruling
        │
   PHASE A — establish the target (the gap IS the spine)
        │
5  D0a: power-sized gold re-pin (non-zero gold on the 4 classes) + answerer seam
       + Mem0-OSS DE-RISK SPIKE & stand-up  (diagnose the prior blocker first)
        │
10 D0b: measure FathomDB − {Mem0, Graphiti/Zep, naive-RAG} per-class gap = THE TARGET
        │
   PHASE B — close the gap, cheapest-sufficient lever first, re-measure vs Mem0, STOP at parity
        │
15 $0 triage probes (LLM-free, no engine change):  15a embedder-ceiling (PRIMARY lever pick)
                                                    ∥ 15b D2 content-at-scale proxy
        │
20 D1 build: CLS-fix + CHOSEN embedder (one re-embed) + one eu7 re-clear + corrected baseline
       + deterministic vector PRF (only if the embedder alone leaves a gap) → re-measure vs Mem0
       [eu7-break → fidelity-recovery fork + 0.937→0.896 bisect]   ←── may already reach parity → §surpass
        │
25 D2 build (only if 15b passed AND a gap remains): fielded-FTS/BM25F + tunable-b engine
       + offline enrichment keys + length-matched placebo → re-measure vs Mem0
        │
30 RESOLUTION verdict: final FathomDB − {Mem0, Zep} per-class; parity reached?
       + the SURPASS-OPTION package → HITL signs "ship at parity" or "pursue surpass"
```

**Stop-at-parity:** if Slice 20 already reaches near-parity-or-better, Slice 25 (D2) is **not run for
completeness** — it runs only if a gap remains (or as a surpass lever the HITL elects). Levers are ordered
by expected gap-closure × cheapness: embedder (highest-leverage per the report) → PRF → fielded enrichment.

---

## 4. Per-slice contracts

### Slice 0 — Design + pre-registration (resolution gate + surpass protocol + F5 scope) · `[design-adr]` · depends-on: — · gaps: 1–4
>
> **✅ CLOSED on main 2026-06-21 (codex §9 PASS after fix-1).** Frozen rule `eval/decision_rule_083.py`
>
> - `dev/design/0.8.3-mem0-parity.md` (`decision-ready`). Commits: `c611535c`(RED) `424752f8`(GREEN)
> `705bf515`(design) `e38868c8`(F5-ADR/DOC-INDEX) + fix-1 `15ae7735`(RED) `9da63574`(GREEN). Review #1 =
> CONCERN 2×[P2] (probes stricter than frozen §5 — raw-point gates); fix-1 dropped them to CI-lower-only;
> re-review PASS. Verdicts: `runs/0.8.3-slice-0-review-20260621T201312Z.md`,
> `runs/0.8.3-slice-0-fix-1-review-20260621T202646Z.md`. **✅ HITL Slice-0 gate SIGNED 2026-06-21** —
> pre-registration frozen; **ADJ-2 F5 promotion RATIFIED** (ADR re-pointed to Slice 25, conditional);
> **surpass-option protocol RATIFIED**. Engine commitment (20/25) + priced runs (10/20/25/30) are unblocked
> subject to their per-slice cheap-validate / HITL spend checks. **NEXT = Slice 5 (D0a)** — IN PROGRESS.

**Objective.** Freeze: the **two-level measurement** (external Mem0/Zep gate; internal corrected-fused
attribution), the **near-parity band ε** + **power-sized per-class N** (MDE ≤ ε), the **$0-probe pass/fail
criteria** (15a embedder headroom; 15b enriched-beats-placebo-at-power), the **eu7-break fork**, the
**surpass-option protocol**, and the **ADJ-2 F5/tunable-`b` scope ruling** (promote or defer). Freeze the
rule as `eval/decision_rule_083.py`.
**Deliverables:** (1) `dev/design/0.8.3-mem0-parity.md`; (2) the falsifiable Slice 5/10/15/20/25/30 AC
list; (3) the aggregate priced-run budget + the F5-ADR re-point.
**Acceptance bar:** design `status: decision-ready`; gate + N + probe criteria + fork + surpass protocol
frozen + dated. **HITL gate:** sign before any engine commitment (20/25) or priced run (10/20/25/30); the
F5 ruling + the surpass protocol are part of this gate.
**Reserved follow-on (1–4):** power re-estimate if Slice-10 variance is wider than assumed.

### Slice 5 — D0a: power-sized gold re-pin + answerer seam + Mem0-OSS de-risk & stand-up · `[implementation (eval-infra)]` · depends-on: 0 · gaps: 6–9
>
> **✅ CLOSED on main 2026-06-21 (codex §9 PASS after fix-1 + fix-2).** Re-pinned gold: `corpus_hash`
> `1859817a` (stable LME corpus) + gold `repin_hash` `2916cace`; per-class counts factoid 156 /
> knowledge_update 150 / multi_session 150 / temporal 150 (all ≥ n_min=150 — the Slice-25 N=0 defect fixed).
> Mem0-OSS de-risk = **GO for D0b** (footprint-safe local backend `eval/mem0_local.py`; airlock Qwen +
> local bge-small + on-disk chroma, per-run isolated). Answerer seam wired + cheap-validated (~$0.0001).
> Commits (slice + 2 fix rounds): `76bf9c75`/`65f96ceb`/`988aa499` + fix-1 `02b8726b`/`88ba3772` + fix-2
> `aaa0a7b8`/`94952b69`. Reviews: `runs/0.8.3-slice-5-review-20260621T212116Z.md` (2×P1+P2),
> `runs/0.8.3-slice-5-fix-1-review-20260621T215345Z.md` (2 new regressions), `…-fix-2-review-20260621T220452Z.md`
> (PASS). **Carry forward:** Slice-10 paired power-check may HITL-escalate an under-powered class;
> `knowledge_update` is partly synthetic (≥2-session-correct) — sanity-review before citing its delta.
> **NEXT = Slice 10 (D0b)** — first priced parity run; per-run cheap-validate → HITL spend check before spend.

**Objective.** Rebuild/re-pin the R2 gold so all four memory classes carry **≥ N_min gold** (sized at
Slice 0; the prerequisite that made every class delta null in Slice 25); wire the identical-answerer seam
(`R2_RUN=1`, `R2_ANSWERER_*`). **First task = a de-risk spike diagnosing the prior Mem0-OSS blocker**
(dependency / API / embedding-model) so D0b is not assumed tractable; stand up the local Mem0-OSS backend
behind the shared R2 adapter. Reuse `r2_parity_eval.py`.
**TDD.** RED: a corpus-validity test (each class ≥ N_min gold, no silent N=0); an answerer-seam smoke test;
a Mem0 adapter-conformance test (returns top-K under the shared metric contract). GREEN: the re-pin builder

- wired seam + Mem0 backend. Cheap-validate the seam before any priced call. Output →
`runs/0.8.3-d0a-corpus-manifest.json` (new `corpus_hash`).
**DoD.** EVAL-ONLY. Determinism: pinned `corpus_hash`, frozen qrels. Codex §9 (corpus validity — no
vacuous/empty gold; this is eval infra, not a product AC — [[acceptance-md-locked-no-feature-acs]]).
**Reserved follow-on (6–9):** if a class can't reach N_min, or Mem0-OSS is genuinely infeasible → **HITL
escalation**, not a silent deferral (the resolution depends on this number existing).

### Slice 10 — D0b: measure the FathomDB − Mem0 gap (= the target) · `[implementation (measurement)]` · depends-on: 5 · gaps: 11–14
>
> **✅ CLOSED on main 2026-06-22.** The per-class `FathomDB − Mem0` gap LANDED (priced 606 q, $10.75,
> `runs/0.8.3-d0b-parity-n606.json`): `decide_083 = NOT_REACHED` (eu7-blocked + underpowered),
> **−20…27 pp accuracy** behind Mem0 on 3 classes (temporal a tie), with the **accuracy gap ≫ recall
> gap** ⇒ **answer/memory-formation is the binding constraint, not retrieval**. codex §9 done (P2 fixes
> `22b63dc8`/`349be76f`). Phase-A+B + persistent Mem0 ingest + powered LME+LOCOMO recall all landed;
> capability-report #1 caveat cleared. **Reframes Phase B** (retrieval levers D1/D2 close at most the
> recall portion) → the **gap-decomposition probe** (formation vs retrieval) interprets the gap before
> any lever pivot; the powered + post-eu7 PRICED verdict is Slice 20/30.

**Objective.** Produce the **per-class `FathomDB − {Mem0, Graphiti/Zep, naive-RAG}` delta** (strict
Recall@K + identical-answerer accuracy) on the re-pinned, power-sized corpus. This is the **target the
Phase-B levers must close** — and the headline number the capability report's #1 gap demands. Graphiti/Zep
is the second comparator (the 0.8.4 §5 G-HH-1 head-to-head, pulled onto the agentic-memory axis here).
**TDD.** RED: a parity-harness test (all arms run on the same `corpus_hash` + answerer; per-class deltas +
paired CI emitted; power-check asserts MDE ≤ ε per class). GREEN: the multi-arm runner. Cheap-validate →
resilient priced parity pass. Outputs → `runs/0.8.3-d0b-parity-{recall,accuracy}-n*.json`.
**DoD.** EVAL-ONLY; $ ledger entry; resilient harness. Codex §9. **HITL gate:** the R2 metric/threshold
stays a HITL eval gate ([[fathomdb-recall-fidelity-vs-relevance]]); report the gap into
`runs/0.8.3-report.md` + the capability report conclusion (clears its #1 caveat).
**Reserved follow-on (11–14):** none — a *third* external comparator is OUT; the **GraphRAG/HippoRAG**
multi-hop head-to-head is the 0.8.4 resolution ([`../roadmap/0.8.4.md`](../roadmap/0.8.4.md)).

### Slice 15 — $0 triage probes: embedder-ceiling (primary) ∥ D2 content-at-scale proxy · `[implementation (measurement, $0 LLM-free)]` · depends-on: 10 · gaps: 16–19
>
> **✅ CLOSED AS-IS 2026-06-23.** **15a = NO embedder swap** — no candle-feasible candidate cleared
> `probe_15a_pass` on the hard subset (recall lever spent; [[0.8.3-slice15a-embedder-probe-no-swap]]).
> **15b = D2 proxy code only**; the full `--full` eligibility run was deferred (the chosen lever became
> CE-rerank precision, not D2). Branch `0.8.3-slice-15a/15b` cherry-picked findings; codex §9 clean.

**Objective (ADJ-6, ADJ-7).** Two $0, LLM-free probes that pick the Phase-B levers by expected
gap-closure:

- **(15a) Embedder-ceiling — PRIMARY.** A/B the CLS-corrected `bge-small` vs **genuinely stronger
  CPU/ONNX embedders** (`bge-base`, `gte-base`, `e5-base`, a retrieval/memory-tuned candidate) on eu8 +
  the ~596-q hard subset + (where cheaply derivable) the memory-class recall, within the candle/ONNX
  footprint (`EmbedderChoice::Caller`+ONNX, [[embed-completeness-and-gpu-readiness]]). **Output:** the
  embedder projected to close the most Mem0 gap subject to CPU-feasibility + 1-bit survivability.
- **(15b) D2 content-at-scale proxy.** Measure fielded-enrichment content value at scale in an **in-harness
  separate index (no migration)** vs the corrected baseline + a length-matched placebo; confirm content >
  placebo at power and that fielding removes the FTS length-norm penalty.
**TDD.** RED: each probe asserts its pre-registered metric + a mechanical PASS/FAIL; placebo length-match
(15b); ONNX-embedder load determinism (15a). GREEN: the two runners. Outputs →
`runs/0.8.3-s15a-embedder.json`, `runs/0.8.3-s15b-d2-proxy-n*.json`.
**Gate semantics:** **15a** picks the Slice-20 embedder (corrected `bge-small` if no candidate clears it
with headroom; else the winner); **15b FAIL** ⇒ Slice 25 (D2 engine) defers/drops. **Surpass check:** if a
15a candidate *overshoots* the projected parity gap, flag it as a surpass-Mem0 option for the Slice-30
package.
**DoD.** EVAL-ONLY / $0 / deterministic. Codex §9 (probe validity + gate logic). No HITL gate
(HITL-approved measurement); outcomes feed Slice 20/25.
**Reserved follow-on (16–19):** one more embedder candidate if 15a is borderline; entity-only vs fact-only
field split if 15b is borderline.

### Slice 20 — D1 build: CLS-fix + chosen embedder (one re-embed) + eu7 re-clear + PRF-if-needed · `[implementation (engine + measurement)]` · depends-on: 15 · gaps: 21–24
>
> **✅ CLOSED AS-IS 2026-06-23 — re-scoped from "D1 embedder build" to "the realizable PRECISION lever".**
> The gap-decomposition (n=606) showed the lever is retrieval **precision**, not the embedder — so Slice 20
> became the **CE-rerank accuracy arm**: α=0.3 (citable PASS, marginal NO-GO, `…-rerank-accuracy-n606.json`)
> → $0 α-tuning sweep (α is the dominant knob) → **α=1.0 reblend** (provisional surpass, `ABORTED_INCOMPLETE`
> n=354/606, `…-reblend-a1-n606.json`). **CE-batch reranker** (release+batched, 10-50×) + the **reblend
> adapter** landed; codex §9 done (this session: 2×P2 remediated). The **engine wire** (raise `ALPHA`→~1.0 /
> narrow pool in `ce_rerank`, re-embed, eu7 re-clear) is **deferred to 0.8.4** (no priced spend left; OpenAI
> usage-limit). eu7 0.937→0.896 bisect = case-A/SUT not CLS → quant-path recovery fork. See the resolution
> verdict (`runs/0.8.3-resolution-verdict.md`).

**Objective.** Build the chosen D1 lever **once**: fix `CandleBgeEmbedder` CLS pooling and adopt the
Slice-15a embedder (a single re-embed/index rebuild on the final vectors); **re-clear eu7 ≥0.90**;
re-measure the corrected dense/fused baseline; **add deterministic vector PRF only if the embedder alone
leaves a Mem0 gap** (bounded query-vector expansion → one re-query → deterministic RRF). Re-measure the
external Mem0 delta + the internal co-primaries (all-bridges@K presence **and** F1-given-all-bridges
composition; ADJ-10).
**TDD.** RED: a Rust CLS-pooling pin (+ embedder-swap functional test if the embedder changed); an eu7
harness test; PRF determinism + capped-expansion + a **composition guard** (PRF must not drop
F1-given-all-bridges below corrected fused); the verdict harness computes the pre-registered external +
internal endpoints. GREEN: the fix/swap + PRF arm + runner. **eu7 gate:** the CLS-fix rewrites the stored
vectors, so re-measure eu7 **fresh** on the `vector_stage_only` seam against the **0.90 floor** (baseline
is the true vector-stage **0.896**, NOT 0.937 — see the bisect below). fidelity < 0.90 → **BLOCK → HITL**,
and trigger the **fidelity-recovery fork**. **The 0.937→0.896 bisect is DONE
(`runs/0.8.3-eu7-bisect-report.md`): the cause is NOT CLS/pooling (case B ruled out) — it is case A
(vector-path/SUT). So the fork is the QUANT path: rotation/whitening before sign-quant → raise ANN fan-out
K>192 → a 2-bit option. Treat the CLS-fix as an eu8/relevance lever, not the eu7 fidelity-recovery.**
Cheap-validate before the priced confirmatory pass. Outputs → `runs/0.8.3-d1-eu7.json`,
`runs/0.8.3-d1-verdict-n*.json`, `runs/0.8.3-d1-latency.json`.
**DoD.** X1 (Py+TS embedder/PRF parity + functional harness), X2, X3. IN-LIBRARY, CPU, deterministic.
**ADJ-9 latency gate** (re-embed + PRF re-query): AC012/AC013/AC020; breach → BLOCK → HITL. Codex §9.
**HITL gate:** D1 promotion + **the surpass-option check** (does the chosen embedder overshoot? present it)
is HITL-signed; **if D1 already reaches near-parity, Slice 25 runs only on a remaining gap or as an elected
surpass lever.**
**Reserved follow-on (21–24):** PRF source ablation (reranked-f32 vs fused top-k) only if borderline.

### Slice 25 — D2 build: fielded-FTS/BM25F + tunable-`b` + enrichment (conditional) · `[implementation (engine + offline-build + measurement)]` · depends-on: 20, 15 · gaps: 26–29
>
> **⏭️ NOT RUN (deferred) 2026-06-23.** Conditional slice; its gate did not fire — 15b's full eligibility
> run was not completed and the chosen lever was CE-rerank precision (Slice 20), not D2 enrichment. D2
> fielded enrichment is now a **0.8.4 recall lever** (the path to *surpass* Mem0). No schema migration on an
> unvalidated/unneeded lever.

**Gate.** Runs **only if** (a) Slice-15b passed, (b) the Slice-0 ruling promoted F5/tunable-`b`, **and**
(c) Slice-20 left a Mem0 gap (or the HITL elected D2 as a surpass lever). Else D2 defers/drops — no schema
migration on an unvalidated/unneeded lever.
**Objective.** Land the **F5 promotion (ADJ-2):** a separate **fielded/sidecar FTS5 channel** (entity/
alias/fact/temporal fields, kept out of the body channel) with **per-column BM25F weighting** + a
**tunable-/lower-`b`** length-norm path (additive migration + `SCHEMA_VERSION` bump + EXPLAIN check).
Offline-extract enrichment keys (reuse 0.8.2 cached MuSiQue extractions; extract LongMemEval sessions + IR
corpus with local Qwen, $0) → index into the fielded channel → measure **enriched vs plain vs
length-matched placebo**, fused with the Slice-20 stack → re-measure the external Mem0 delta.
**TDD.** RED: schema-migration test (additive, version bump, EXPLAIN no-SCAN); fielded-search test (body
unaffected); BM25F weighting determinism; `b`-sweep determinism; coverage test (every doc/session has
keys); the verdict harness asserts **enriched-beats-BOTH-plain-AND-placebo** + the external delta. GREEN:
engine + bindings + build/index/measure runner. **Determinism pins extended.** Cheap-validate before the
priced pass. Outputs → `runs/0.8.3-d2-verdict-n*.json`, `runs/0.8.3-d2-engine-latency.json`.
**DoD.** X1/X2/X3 + DOC-INDEX. IN-LIBRARY + OFFLINE-BUILD; no body-channel contamination. **ADJ-9 latency
gate.** Codex §9.
**Reserved follow-on (26–29):** entity-only vs fact-only vs alias-only field ablation before the BM25F
weight vector is frozen; field-weight auto-tuning is OUT (would need a learned ranker).

### Slice 30 — Resolution verdict + surpass-option package · `[implementation (measurement)]` · depends-on: 25, 20 · gaps: 31–34
>
> **✅ CLOSED AS-IS 2026-06-23 (HITL "take results as-is").** Resolution = **provisional parity-or-better
> with Mem0 via retrieval precision** (CE-rerank α=1.0): +0.21 over Mem0 on the answered cells (paired
> n=354), **taken as-is** — NOT a fully-powered citable claim (`decide_083` strict REACHED unavailable:
> eu7-blocked + arm `ABORTED_INCOMPLETE` + just-underpowered). Accuracy is **retrieval-gated**. Surpass lives
> in retrieval **recall** (0.8.4). Verdict: `runs/0.8.3-resolution-verdict.{md,json}`. **Steward
> recommendation: SHIP-AT-PARITY**, surpass via recall in 0.8.4 — pending the HITL sign-off + version-gate.

**Objective.** Compute the **final `FathomDB − {Mem0, Graphiti/Zep}` per-class delta** on the power-sized
corpus, fused-stack, identical answerer; re-check eu7 ≥0.90 + latency on the final stack. Determine
**parity reached?** per the frozen rule, and **assemble the surpass-option package**: any lever projected
to overshoot Mem0 (a 15a embedder with headroom; a memory-class-specific lever; the caller-side reader
axis, flagged out-of-library) with its cost + expected lift.
**TDD.** RED: the verdict harness asserts the pre-registered external gate + emits the parity decision
mechanically + lists the surpass candidates with projected deltas. GREEN: the runner. Outputs →
`runs/0.8.3-resolution-verdict.json`, report `runs/0.8.3-report.md`, `runs/0.8.3-final-latency.json`.
**DoD.** $ ledger finalized. Codex §9 (verdict + report; cross-check green vs printed numbers). **HITL
gate:** the resolution (parity reached or residual-gap+binding-constraint) **and the surpass-option
decision** ("ship at parity" vs "pursue surpass") is HITL-signed; update `runs/STATUS-0.8.3.md`, DOC-INDEX,
the capability report conclusion, and the 0.8.4 "prior" framing.
**Reserved follow-on (31–34):** if the HITL elects a surpass lever, scope it as a reserved-gap slice here.

---

## 5. What 0.8.3 deliberately does NOT do

- **No graph traversal / PPR / BFS** — refuted twice; stays dropped (any re-entry needs a new mechanism).
- **No in-library LLM** — query rewrite / HyDE / decomposition / IRCoT / answer generation are
  **caller-side BYO-LLM only**; the library exposes deterministic search + the fielded channel + PRF.
- **No whole-doc dense (R4), no bundled CPU extractor (R3b), no portable-DB vector guard** — stay in
  [`../roadmap/0.8.5.md`](../roadmap/0.8.5.md); R3b's gate is S1-alone (0.8.4).
- **No GraphRAG/HippoRAG head-to-head, no GraphRAG sensemaking (S1)** — those are the **0.8.4
  GraphRAG-parity resolution** ([`../roadmap/0.8.4.md`](../roadmap/0.8.4.md)). 0.8.3's competitor scope is
  the agentic-memory axis (Mem0 + Graphiti/Zep) only.

## 6. Reuse inventory (new infra is minimal + footprint-safe)

| Need | Reused asset |
|---|---|
| Agentic-memory parity harness (D0) | 0.8.1 `r2_parity_eval.py` (identical-answerer, shared adapter/metric contract) |
| MuSiQue corpus + bridge diagnostic + 5-arm QA harness | 0.8.2 M1 (`musique_hash 3cff37fd…`, cached extractions, resilient priced runner) |
| LongMemEval recall harness | 0.8.1 (`graph_arm_recall.py` recall path, per-class strata) |
| IR corpus eu7/eu8 + hard subset | 0.8.0 eval (`corpus_hash fe973fcd`, frozen qrels) |
| Lexical / dense / fusion / CE-rerank | engine (FTS5, 1-bit ANN + f32 rerank, RRF k=60, TinyBERT-L-2 via `fathomdb.rerank`) |
| Offline extraction | Qwen3.6-27B Airlock vLLM batch ($0) |
| Answerer (priced, EVAL-ONLY) | gpt-5.4 (temp 0, seed 0); cheap-validate = gemini-2.5-flash-lite |
| Decision-rule frozen-as-code | extend `eval/m1_decision_rule.py` → a new `eval/decision_rule_083.py` module |
| Enrichment value at scale (D2 proxy) | extend `eval/r6_index_key_enrichment.py` (placebo control) into the separate-index proxy |
| New (eval-infra, D0) | re-pinned power-sized gold (non-zero gold on 4 classes) + answerer seam; local **Mem0-OSS** + **Graphiti/Zep** backends behind the R2 adapter |
| New (harness, $0) | embedder-ceiling probe (ONNX `bge-base`/`gte-base`/`e5-base` via `EmbedderChoice::Caller`); D2 content-at-scale proxy; deterministic vector PRF arm; enrichment-key builder + fielded index loader; placebo |
| New (engine) | CLS-pooling fix; (optional) embedder swap; fielded-FTS/BM25F + tunable-`b` ranking (the F5 promotion) |
| Latency gate (ADJ-9) | existing AC012/AC013/AC020 perf harness, re-run on read-path slices |
