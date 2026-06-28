# FathomDB 0.8.4 — Plan: reach near-parity-or-better vs GraphRAG (sensemaking), S1 + G-HH-2

> **What this version is — the RESOLUTION.** 0.8.4's success is a measured outcome: **FathomDB reaches
> near-parity-or-better with Microsoft GraphRAG on the global-sensemaking axis**, under one
> BenchmarkQED-style LLM-judge harness (one judge, one corpus, identical bias controls). It is delivered by
> two paired workstreams: **S1** — FathomDB's own GraphRAG-style build (entities → Leiden communities →
> LLM community summaries → map-reduce query-focused summarization); and **G-HH-2** — a measured head-to-head
> of S1 vs a _running_ Microsoft GraphRAG (HippoRAG-2 a secondary multi-hop cross-check). Roadmap home +
> framing: [`../roadmap/0.8.4.md`](../roadmap/0.8.4.md).
>
> **Honest prior (do not bury it).** The cross-graph prior across 0.8.1+0.8.2 is **strongly negative** (M1
> multi-hop NO-GO; M2 dropped). S1 is a _different structure on a different axis_ (community summaries for
> global sensemaking, not fact-edge traversal for needle/multi-hop), so the multi-hop negatives do not
> refute it — but S1's (largest-of-the-program) build is funded **only if its design survives codex + HITL
> review against a strong vector-RAG + long-context baseline.** A third graph null settles the graph
> question for FathomDB; that is an acceptable, publishable outcome.
>
> **Parity is the floor — surpass-option before sign-off (HITL principle, mirrors 0.8.3).** If an S1 variant
> is projected to _overshoot_ GraphRAG, the orchestrator presents the surpass experiment as an explicit
> option **before** the HITL signs the gate — "ship at parity" vs "spend to surpass," never a silent stop.

Ladder shape + reserved-gap policy: reuse [`0.8.1-plan.md`](0.8.1-plan.md) §"Ladder".
Process: [`../design/orchestration.md`](../design/orchestration.md) (three-role separation, codex §9,
worktrees §11). Slice prompts generate from [`prompts/0.8.0-SLICE-TEMPLATE.md`](prompts/0.8.0-SLICE-TEMPLATE.md).
Carries forward the 0.8.3 Mem0-parity verdict ([`runs/0.8.3-mem0-parity-VERDICT.md`](runs/0.8.3-mem0-parity-VERDICT.md))
and its §6 reranking carry-forward ([`../roadmap/0.8.4.md`](../roadmap/0.8.4.md) §6).

---

## Execution model — the agent runs this plan as **ORCHESTRATOR** (binding)

This plan is executed by an **orchestrator** agent under the three-role separation
([`../design/orchestration.md`](../design/orchestration.md)). The orchestrator:

- **Authors, gates, and reviews — it does not write slice code itself.** Code for each slice is delegated to
  **`implementer` subagents working in pre-created git worktrees** owned by the main thread
  ([[agent-worktree-stale-base-trap]]: pre-create + verify each worktree from `$(git rev-parse main)`, give
  the implementer a fail-fast STEP-0 preflight, forbid `maturin develop`/`pip install -e` from a worktree).
- **Treats independent `codex §9` review as the load-bearing gate.** codex is the PROVEN PRIMARY §9 reviewer
  ([[orchestration-execution-traps]]): run `codex exec review` with
  `--dangerously-bypass-approvals-and-sandbox` (the USER-added `Bash(codex exec review:*)` rule); a subagent
  review is fallback only. **Self-review the diff first**, then codex, before any verdict is recorded. Give
  codex a generous timeout + run it detached (it explores the repo; a short `timeout` will kill it before it
  emits — [observed 2026-06-22]).
- **Drives TDD (RED → GREEN)** per slice; cross-checks every green claim against printed numbers
  ([[background-exit-masks-real-exit]]).
- **Does not push.** Commits land on slice branches / local main; tags/pushes are HITL-gated
  ([[release-publish-gotchas]]).
- **Stops for the HITL gate** at Slice 0 (design + pre-registration) and at the Slice-20 resolution verdict
  (+ surpass-option package); presents priced-run budgets for approval before spend.

---

## 0. Goal (0.8.4) — the resolution gate

**Reach `FathomDB(S1) − GraphRAG` ≥ −ε (near-parity) or > 0 (better) on the global-sensemaking axis**, via a
BenchmarkQED-style pairwise LLM-judge win-rate (comprehensiveness / diversity / empowerment), with the
non-negotiable bias controls applied, against a strong **vector-RAG + long-context** baseline. Secondary
cross-check: **HippoRAG-2** on the M1/D1 MuSiQue multi-hop harness (bounds the competitor; FathomDB's own
graph multi-hop is already refuted).

### Pre-registered decision rule (frozen at Slice 0 as `eval/decision_rule_084.py`)

- **Resolution REACHED:** the S1-vs-GraphRAG pairwise win-rate clears the near-parity band (≥ the parity
  win-rate floor) with bias controls applied (order-swapped + averaged, ≥5 runs, cross-family judge) and the
  length-bias corroboration not contradicting. ⇒ **present the surpass-option package; HITL signs "ship at
  parity" or "pursue surpass."**
- **Resolution NOT reached:** record the **residual win-rate gap + the named binding constraint** (judge
  variance, community-summary quality, index-time extractor ceiling) and the explicit fork — (a) a richer
  community hierarchy / hybrid summary+retrieval reader, (b) accept the third graph null and settle the
  graph question (publishable), or (c) re-scope. 0.8.4 always ends in a **decision**, never an open redirect.
- **GraphRAG win is publishable either way:** a measured GraphRAG-beats-FathomDB win-rate, with controls, is
  a valid result that resolves whether GraphRAG-style sensemaking is worth productizing in FathomDB.

---

## 1. What "fair" requires (LLM-judge discipline — these are load-bearing, not optional)

- **One judge / one corpus / one metric** across S1, GraphRAG, vector-RAG, long-context — a parity claim
  needs apples-to-apples (the capability report's #1 caveat: literature rows are not comparable).
- **Bias controls (the LLM-judge failure modes — each is mandatory):**
  - **Position bias** — randomize/swap answer order and average (arXiv:2406.07791).
  - **Stochasticity** — **≥5 runs** per comparison; report variance, not a point.
  - **Self-preference** — the **judge is from a different model family** than any system under test
    (arXiv:2410.21819).
  - **Length bias** — a Directness/claim-count non-judge corroboration (the GraphRAG paper's control).
- **Strong baselines, not strawmen.** Vector-RAG **and** a long-context "stuff-it-all-in" control (the
  honest upper bar for a corpus that fits the window — the Samsung "VectorRAG is almost enough" prior).
- **Running competitor, not a literature row.** GraphRAG is stood up as a real backend on the same harness;
  HippoRAG-2 likewise on the MuSiQue harness. Cross-vendor numbers become a measured leaderboard.
- **Pre-register before data.** Endpoints, judge model, bias controls, win-rate band, and N are frozen as
  code (`eval/decision_rule_084.py`) at Slice 0, codex-reviewed, before any judged run.

---

## 2. Cross-cutting Definition of Done (binds every slice)

- **Footprint invariant:** CPU-only, no-API **at the library boundary**, 1-bit-safe, deterministic. S1 is
  the one initiative that strains this: **community summaries need an LLM at _index_ time** — that stays the
  **OFFLINE-BUILD** seam (local **Qwen3.6-27B** extractor, $0/local, to be measured for sufficiency), never
  the library query boundary. The judge is the priced **EVAL-ONLY** seam. Tag every technique
  **IN-LIBRARY / CALLER-SIDE BYO-LLM / OFFLINE-BUILD / EVAL-ONLY**; no in-library LLM.
- **Orchestrator + reviews (see Execution model above):** implementer subagents in pre-created worktrees;
  **codex §9 on every slice before a verdict is recorded**; self-review first; green claims cross-checked vs
  printed numbers.
- **Design-first + codex-reviewed (Slice 0, HARD):** the S1/G-HH-2 design doc
  (`dev/design/0.8.4-graphrag-sensemaking.md`) is authored, reaches `status: decision-ready`, and is
  **codex §9-reviewed to a clean PASS before the HITL design gate**. No build slice (10/15) runs until the
  design + pre-registration are signed.
- **Batch-by-default for non-interactive LLM calls ([[airlock-batch-and-provider-protection]]).** Any
  airlock LLM call that is **not dependent on interactive/sequential context** MUST be submitted via the
  **OpenAI/airlock Batch interface** (~50% cheaper; **bypasses the interactive TPM quarantine** that blocked
  0.8.3 completion), per [`../design/0.8.3-openai-batch-completion-howto.md`](../design/0.8.3-openai-batch-completion-howto.md).
  In 0.8.4 this is **most** of the priced volume — the BenchmarkQED **AutoQ** question-synthesis and **AutoE**
  pairwise-judge calls are large sets of _independent_ judgments (and the ≥5-run replication multiplies them),
  so they are batch-suitable and should be batched. Interactive `/v1/chat/completions` is reserved for
  genuinely sequential/dependent calls (none expected in 0.8.4). The offline community-summary build runs on
  local Qwen (not the airlock priced seam); if ever routed through airlock, batch it too.
- **Budget discipline ([[0.8.1-budget-discipline-cheap-validate-and-ledger]]):** cheap-validate
  (`gemini-2.5-flash-lite`) before any priced run; $ ledger in `runs/STATUS-0.8.4.md`; the judged-run budget
  (AutoE × ≥5 runs × baselines) is **estimated in aggregate at Slice 0** and HITL-approved before spend.
  Batch's 50% discount is assumed in the estimate.
- **Resilient priced runs ([[priced-runs-need-resilience-before-spend]]):** auto-resume, atomic checkpoint,
  429/5xx backoff (honor server `Retry-After` to escape the airlock quarantine re-arm spiral), failure ≠
  abstention, empty/None completion → ABSENT not `acc=0.0`, completeness validity guard. (Batch's
  idempotent missing-row resume covers the batched judge calls.)
- **Determinism + judge reproducibility:** fixed seeds; the community build is deterministic given the
  extractor + Leiden seed; judge runs pin the order-permutation seed and record per-run results (no silent
  averaging that hides variance).
- **Pre-registration:** endpoints + win-rate rule frozen as code before data; design reviewed (codex + HITL).

---

## 3. Critical path (resolution-driven: build S1 + a running GraphRAG → judge → stop at parity)

```text
0  Design + pre-register (endpoints, judge model + bias controls, win-rate band, N, surpass protocol;
   freeze eval/decision_rule_084.py)  → codex §9 PASS → HITL design gate
        │   (build slices below do NOT start until this gate is signed)
        │
5  Corpus + baselines: BenchmarkQED corpus (AP-News ~1,397; EVAL-ONLY, never committed)
   + vector-RAG baseline + long-context control + the AutoQ question set (batched synth)
        │
10 S1 build: entities/relationships → Leiden community detection → LLM community summaries
   (C0–C3 hierarchy; OFFLINE-BUILD via local Qwen) — the largest new infra
        │
15 Map-reduce query-focused summarization (QFS) reader  ── KEYSTONE  ──┐
        │                                                              │ G-HH-2: stand up a RUNNING
        │                                                              │ Microsoft GraphRAG backend
        │                                                              │ (+ HippoRAG-2 on the MuSiQue
        │                                                              │  multi-hop harness, secondary)
        │←─────────────────────────────────────────────────────────────┘
20 AutoE pairwise LLM-judge adjudication (S1 vs GraphRAG vs vector-RAG vs long-context),
   ≥5 runs, order-swapped, cross-family judge, length corroboration  → RESOLUTION verdict
   + SURPASS-OPTION package → HITL signs "ship at parity" or "pursue surpass"
```

**Stop-at-parity / kill-early:** if Slice 0 design review (codex + HITL) judges the S1 hypothesis
insufficiently distinguished from the negative cross-graph prior, **S1 does not build** — record the
decision and settle the graph question. If a strong vector-RAG/long-context baseline already matches
projected S1 in a Slice-5 pilot, escalate before funding the full community build.

---

## 4. Per-slice contracts

### Slice 0 — Design + pre-registration (+ codex review + HITL gate) · `[design-adr]` · depends-on: —

**Objective.** Author `dev/design/0.8.4-graphrag-sensemaking.md`: the S1 method (Leiden + community-summary
hierarchy + map-reduce QFS), the **G-HH-2** running-competitor protocol (GraphRAG primary on AutoE;
HippoRAG-2 secondary on MuSiQue), the **judge model + the four bias controls**, the **win-rate near-parity
band ε + N (per-comparison runs ≥5)**, the **surpass-option protocol**, and the **footprint tagging**
(offline community build vs priced judge). Freeze the rule as `eval/decision_rule_084.py`.
**Deliverables:** (1) the design doc at `status: decision-ready`; (2) the falsifiable Slice 5/10/15/20 AC
list; (3) the aggregate judged-run budget (batch-discounted) + the index-time extractor-sufficiency plan.
**TDD.** RED: a frozen-rule test (`decision_rule_084` computes win-rate + band verdict from a fixture; bias
controls asserted present). GREEN: the rule module.
**Acceptance bar:** design `decision-ready`; rule + band + N + bias controls + surpass protocol frozen +
dated. **codex §9 to a clean PASS on the design + rule (load-bearing — this is the gate that funds the
largest build of the program).** **HITL gate:** sign before any build slice (10/15) or judged run (5 synth /
20 AutoE); the honest-prior bar (§roadmap "Honest expectation") is part of this gate.
**Reserved follow-on:** power re-estimate if Slice-5 pilot judge variance is wider than assumed.

### Slice 5 — Corpus + baselines + AutoQ question set · `[implementation (eval-infra)]` · depends-on: 0

**Objective.** Stand up the BenchmarkQED corpus (AP-News ~1,397 articles — **EVAL-ONLY, gitignored, never
committed** per [[0.8.3-0.8.4-corpus-adequacy-and-locomo]]), the **vector-RAG** baseline + the
**long-context** control behind the shared `retrieve`/answer seam, and synthesize the **AutoQ** question set
(persona→task→question, local↔global). A Slice-5 **pilot judge run** (small N) sanity-checks the harness +
judge before the full build is funded.
**TDD.** RED: corpus-validity + AutoQ-coverage tests (questions span local↔global; no empty buckets);
baseline adapter conformance. GREEN: the corpus loader + baselines + AutoQ synth.
**Batch:** AutoQ synthesis = a large independent call set → **submit via batch** (cheap-validate first).
**DoD.** EVAL-ONLY; $ ledger; codex §9. **Carry-forward:** if the long-context control already ≈ projected
S1 in the pilot, flag to HITL before the community build.

### Slice 10 — S1 build: Leiden communities + LLM community summaries · `[implementation (engine/offline-build)]` · depends-on: 5

**Objective.** entities/relationships → **Leiden community detection** → **LLM-generated community
summaries** (C0–C3 hierarchy; the levels literally measure the hierarchy's value). Offline build; the
extractor is local **Qwen3.6-27B** ($0). Determinism: pinned Leiden seed + extractor.
**TDD.** RED: community-build determinism (same input → same C0–C3); summary-coverage (every community
summarized). GREEN: the Leiden clusterer + community-summary builder.
**Batch:** if summary generation is ever routed through airlock instead of local Qwen, batch it (independent
per-community calls); default is local-Qwen offline.
**DoD.** OFFLINE-BUILD seam (never the library query boundary); codex §9. The largest new infra — keep it
behind the Slice-0 gate.

### Slice 15 — Map-reduce QFS reader · `[implementation (engine + measurement)]` · depends-on: 10 · **KEYSTONE**

**Objective.** The map-reduce **query-focused summarization** reader over the community hierarchy — the S1
query path. In parallel (G-HH-2): stand up a **running Microsoft GraphRAG** backend on the same corpus +
harness, and wire **HippoRAG-2** as a retrieval backend on the MuSiQue multi-hop harness (secondary
cross-check).
**TDD.** RED: a QFS-reader smoke (map-reduce over C0–C3 returns a synthesized answer under the shared
answer seam); a GraphRAG-adapter conformance test (runs under the identical-harness contract). GREEN: the
QFS reader + the competitor adapters.
**DoD.** codex §9. Competitor LLMs are **competitor-side**, never the FathomDB library boundary (EVAL-ONLY).

### Slice 20 — AutoE adjudication + RESOLUTION verdict + surpass-option · `[implementation (measurement)]` · depends-on: 15

**Objective.** The **AutoE pairwise LLM-judge** adjudication: S1 vs GraphRAG vs vector-RAG vs long-context,
**≥5 runs**, **order-swapped + averaged**, **cross-family judge**, with the **length-bias corroboration**.
Compute the win-rate + the frozen `decide_084` band verdict. Then the secondary HippoRAG-2 MuSiQue
cross-check. Package the RESOLUTION + the **surpass-option**.
**TDD.** RED: a parity-harness test (all arms judged on one corpus/judge; per-comparison win-rate + variance
across ≥5 runs; bias-control assertions: order swapped, judge family ≠ system family). GREEN: the AutoE
runner.
**Batch:** the AutoE judge calls (pairwise × questions × ≥5 runs) are independent → **submit via batch**
(the bulk of 0.8.4 spend; cheap-validate → batch). Resilient + idempotent resume.
**DoD.** EVAL-ONLY; $ ledger; resilient batched harness; codex §9; green claims cross-checked vs printed
win-rates. **HITL gate:** the resolution verdict + surpass-option package → HITL signs "ship at parity" or
"pursue surpass." Feed measured rows back into the capability report (clears its cross-vendor #1 caveat).
**Reserved follow-on:** if judge variance dominates the band, escalate N (more runs) before a verdict.

---

## 5. What 0.8.4 deliberately does NOT do

- **No in-library LLM.** Community summarization is an OFFLINE build; the judge is EVAL-ONLY. The library
  query boundary stays CPU-only/no-API.
- **No multi-hop _graph_ revival.** HippoRAG-2 is a competitor _cross-check_ bound, not a FathomDB graph
  arm — FathomDB's own graph multi-hop is refuted (M1/M2).
- **No reranking work yet.** Per the 0.8.3 verdict §6, reranking is **recall-ceilinged** — revisit it only
  _after_ recall improves; it is sequenced behind the recall levers, not inside the 0.8.4 sensemaking build.
- **No interactive LLM calls for batchable work** — non-sequential judge/synth calls go via batch.

---

## 6. Reuse inventory (new infra justified only if S1 survives design review)

- **Reuse:** the identical-answerer/`retrieve` seam + resilient priced harness (M1/0.8.3); the airlock batch
  path ([`../design/0.8.3-openai-batch-completion-howto.md`](../design/0.8.3-openai-batch-completion-howto.md));
  the $ ledger + cheap-validate discipline; the orchestration three-role process.
- **New (largest build of the program):** Leiden clustering, the community-summary builder (offline Qwen),
  the map-reduce QFS reader, the BenchmarkQED AutoQ/AutoE harness + the four bias controls, the running
  GraphRAG + HippoRAG-2 competitor adapters.
- **Carry-forward from 0.8.3:** after recall improves, re-run the α/pool_n CE-rerank sweep on the improved
  pool ([`../roadmap/0.8.4.md`](../roadmap/0.8.4.md) §6) — a separate, recall-gated workstream, not part of
  the S1 ladder.

---

## 7. Post-scale-run sequencing (HITL 2026-06-24) — the gating re-run BEFORE we lock anything in

> **Status of §§1–6 above:** the original Leiden-centric S1 ladder is **substantially superseded** by the
> Tier-1/Tier-2 measurement arc ([`../design/0.8.4-closing-graphrag-gap.md`](../design/0.8.4-closing-graphrag-gap.md))
> and the scale-powered run ([`runs/0.8.4-scale-powered-run-RESULT.md`](runs/0.8.4-scale-powered-run-RESULT.md)).
> The current measured read is: at 200 docs, FathomDB's **almost-graph-free** Tier-2 (C map-reduce QFS, D2
> depth-1 coverage index) **provisionally surpasses** a running Microsoft GraphRAG on all three sensemaking
> metrics — but the run is `NOT_REACHED` on power (mde≈0.09–0.11>ε) **and** GraphRAG ran at
> **community-level 0** (59 of 1,492 reports; finer/dynamic selection was intractable on nano). So GraphRAG
> may have been measured **below its full strength** — we do **not** lock anything in on this result.

### 7.1 The gating experiment (DO THIS FIRST)

A **fair, at-power, full-strength-GraphRAG re-run** before any board/ledger lock-in. Requirements:

1. **At power** — raise N to clear the frozen `decide_084` bar (mde ≤ ε=0.05; ≈200 questions). Converts the
   provisional surpass *direction* into a **registered** verdict.
2. **A corpus WITH ENTITIES (entity-rich)** — not just AP-News sensemaking. This is GraphRAG's claimed home
   turf and the one regime where the entity/relationship/community machinery (and FathomDB's Fork E) could
   still matter. Testing here is what makes a "FathomDB doesn't need the graph" conclusion credible.
3. **A stronger model that does NOT collapse GraphRAG to community-level 0** — strong/fast enough to run
   GraphRAG's **dynamic community selection / finer community levels** (the full Leiden hierarchy), so we
   measure GraphRAG at full strength, not a root-only configuration. (The compute-tier escalation principle
   permits a stronger/frontier model here; trade off vs latency.)
4. **Arm strategy = D2 as product, C as fallback** — the re-run measures **D2** (depth-1 coverage index;
   $0.012 one-time build, CPU-only cheap query) as the product path, with **C** (map-reduce QFS, no index)
   as the always-available fallback. This is also the **going-forward product strategy**, not just the
   re-run config.

Keep all bias controls (cross-family judge, order-swap, ≥5 runs, length corroboration) and the resilient
batched/checkpointed harness. Same `decide_084` gate — no endpoint switching.

### 7.2 TODO — only AFTER the gating re-run confirms the result

- **(a) [TODO]** Flip the Memex⇄FathomDB ledger **OPP-4** open item to the resolved decision — *D2 = product,
  C = fallback, Leiden likely unneeded at personal-agent scale* — **once the entity-rich, full-strength,
  at-power re-run holds.** (Do not flip it on the current community-level-0 result.)
  (`~/projects/memex/dev/fathomdb/LEVERAGE-OPPORTUNITIES-LEDGER.md`)
- **(b) [TODO]** Supersede `main`'s stale boards — `runs/0.8.4-COMPREHENSIVE-REPORT.md` (§3 head-to-head
  table, §8 "fork A: fund a graph build") and `runs/STATUS-0.8.4.md` still tell the **refuted** "Microsoft
  GraphRAG wins, FathomDB not at parity" story. Update them (and land the `0.8.4-tier2-embedder-graphrag-gap`
  branch) **after** the re-run, so the source-of-truth flips on the *registered* result, not the provisional
  one.

**Why this ordering:** the 0.8.3/0.8.4 program has repeatedly been bitten by measurement artifacts (the
15-doc "GraphRAG loss" was one). A provisional surpass with a known confound (community-level 0) is exactly
the kind of result to confirm *before* propagating it into contracts and source-of-truth boards. Fork E
(entity/Leiden graph) stays gated on this re-run: if D2/C still win on an entity-rich corpus vs a
full-strength GraphRAG, Fork E is decisively not indicated; if the gap reappears there, Fork E re-enters.
