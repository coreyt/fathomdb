# FathomDB — Preliminary Solution Design (DRAFT)

**Component:** A planning/composition shim in front of FathomDB (a local, agentic, multi-modal knowledge store with temporal reasoning and graph extraction).
**Status:** `Draft` · **Decision owner:** [TBD: name/role] · **Gate requested:** authorize Gate 0 + Gate 2 (Phase 1).
**Convention:** `[TBD: …]` = unresolved, must be settled before this section is final. *Provisional:* = a position taken for review, expected to be ratified or overturned by evidence.
**Voice rules in force:** no *deterministic/guaranteed/no-hallucination* framing; every quality number below is provisional until measured in §III.

---

## I. Concept of Operations (ConOps)

### A. Primary actors & system boundaries
Consumers are **personal agents** — ephemeral coding harnesses and remote LLMs — and, transitively, the human developer driving them. The shim's job is *planning and composition*: it decides **what** FathomDB functions to call and **how** to compose them. It owns the planner, the router/executor, and the provenance contract. It does **not** own FathomDB's storage, indexing, embedding, or graph-extraction internals — those are a fixed substrate the shim orchestrates. The boundary: the calling agent sends a typed intent and consumes a typed result-with-provenance; everything between intent and result is the shim; everything below the operator calls is FathomDB.

### B. API surface & interface contract
*Provisional:* primary surface is a **typed tool-calling JSON schema** (the consumers are LLM tool-callers), with a **GraphQL** option for structured non-LLM clients. `[TBD: confirm primary surface with consumer teams — this choice reshapes §I and §II.D.]`

Sample input (illustrative):
```json
{ "intent": "retrieve",
  "query": "architectural decisions about the SINE module in Q1",
  "constraints": { "temporal": {"from":"2026-01-01","to":"2026-03-31"},
                   "relation": "Decision", "subject": "SINE" },
  "budget": { "max_latency_ms": 3000, "max_tokens": 8000 } }
```
Structural output: ranked content chunks **plus a provenance object** — nodes traversed, edges used, temporal bounds applied, per-result confidence scores, and the executed plan (operators + topology). The agent can verify, cite, or re-query from this rather than trusting an opaque answer.

### C. Critical user journey (happy path)
An ephemeral agent is asked to refactor the SINE module and queries the shim for *"all architectural decisions about SINE in Q1."* The planner classifies this as **temporal + graph-relational**, emits a DAG (temporal filter → graph BFS over `Decision` edges from the `SINE` node → cross-encoder rerank), the executor runs it, confidence is high enough to skip re-plan, and the agent receives the decision chunks with provenance and writes the refactor against cited sources.

**⚑ FLAG:** ConOps depth is capped by the unresolved surface choice (B) and provenance schema. One trace lives here; multi-hop/exploratory traces go to an appendix.

## II. Core Architecture Hypothesis  `[BUILD-DEFERRED — gate-design depth only]`

**Central problem this section exists to confront:** the shim can estimate **cost** (latency, tokens) before running, but it **cannot reliably estimate efficacy** (will this path retrieve the right context?) the way a SQL optimizer uses table statistics. That asymmetry is the project's spine. The hypothesis below responds to it with a closed loop — and §III exists to test whether that response is worth its overhead.

### A. Logical layer — the planner
Translates an incoming typed/NL intent into a DAG over a **fixed vocabulary of typed operators**. An intent/complexity classifier tags the query class (temporal, graph-relational, lexical-exact, semantic, exploratory, multi-hop) and routes simple queries to a fast-path. The planner retrieves nothing itself.

### B. Physical layer — the composable router/executor
Binds each plan node to a concrete operator from the registry — **BM25/full-text, vector ANN, graph BFS/traversal, RRF, cross-encoder rerank, map-reduce query-focused summarization, clustering/community summarization, native-RAG vs long-context adapters** — chooses topology (sequential pipeline vs parallel scatter-gather), and runs the DAG.

### C. Closed-loop re-plan mechanism — **hypothesis under test**
When a mid-execution signal (e.g. cross-encoder confidence below `[TBD: threshold]`) indicates a low-quality path, the executor halts and asks the planner for a revised DAG. *Provisional:* re-plan depth bounded to `[TBD: e.g. 1–2]` to protect the latency SLO. **This loop is the claim Gate 2 and Experiment 6 must justify — it is not asserted as correct here.**

### D. Provenance & metadata packaging
Final nodes, edges, temporal bounds, confidence scores, and the executed plan are packaged with the content chunks per the §I.B contract.

### E. Plan stability & determinism
*Provisional:* constrained **plan DSL over typed operators + a validator**, with **plan caching/memoization** keyed by query template; distillation considered later. Variance target `[TBD]`, measured by Experiment 8. **⚑ FLAG:** if the planner is *not* an LLM, much of E and the §V.C overhead math change — `[TBD: is the planner an LLM?]`.

## III. The Validation Gates (Design of Experiments)  ★LOAD-BEARING

### A. Gate 0 — Golden eval set construction *(blocking prerequisite)*
Build a suite of **50–100 labeled queries** spanning all six query classes (temporal, graph-relational, lexical-exact, semantic, exploratory, multi-hop), each with a **gold answer** and the **gold supporting nodes**. *(Note: the original PSD said five classes; this draft uses six per the project's intent taxonomy — `[TBD: ratify class count]`.)* Nothing downstream is measurable without this.

### B. Gate 2 — Oracle-routing upper bound *(kill/justify gate)*
Exhaustively run candidate plans per golden query, pick the best (a perfect planner's ceiling), and compare to a **fixed hybrid baseline** (BM25 + vector + RRF). This bounds the *most* a dynamic planner could ever buy.

### C. Go / No-Go criteria
*Provisional:* **if oracle routing does not beat the fixed baseline by ≥ 15–20% on the primary metric (Recall@K and/or answer accuracy), reject the dynamic planner (§II) and ship the fixed baseline.** `[TBD: ratify the metric and the margin with stakeholders before Phase 1 starts.]`

### D. Downstream experiment ladder `[BUILD-DEFERRED]`
Run only if Gate 2 clears, readiness-ordered, each with its own pass/fail criterion: **1** per-operator characterization · **3** intent/complexity classifier accuracy · **4** pipeline vs scatter-gather by class · **5** cost/efficacy estimator calibration · **6** upfront vs closed-loop re-plan · **7** RRF vs RRF+cross-encoder · **8** planner determinism/stability · **9** end-to-end vs baselines (naive top-k, fixed hybrid, frontier-model agent) · **10** provenance → agent self-correction value.

## IV. Trade-offs & Alternatives Considered  ★LOAD-BEARING

### A. Static hybrid pipeline (BM25 + vector + RRF) — **the Gate-2-fail fallback**
Fast, well-understood, cheap to build. Weak on multi-hop reasoning and graph-relational queries; RRF can dilute a single strong signal. This is what ships if Gate 2 fails.

### B. Massive context / no retrieval ("put it all in a long-context model")
Zero routing logic. Rejected as the default for FathomDB's workload: prohibitive token cost and latency at scale, and context pollution on precise graph-edge/temporal queries where the answer is a few nodes, not a corpus.

### C. Per-operator build-vs-integrate

| Operator | Off-the-shelf? | Build / Integrate | Rationale (draft) |
|---|---|---|---|
| BM25 / full-text | Yes | Integrate | Commodity; FathomDB likely exposes it. |
| Vector ANN | Yes | Integrate | Commodity. |
| Graph BFS / traversal | Partial | Integrate | Native to FathomDB's graph layer. |
| RRF | Yes (trivial) | Build-thin | A few lines; not worth a dependency. |
| Cross-encoder rerank | Yes | Integrate | Off-the-shelf models exist. |
| Map-reduce QFS | Partial | Integrate | Use existing summarization adapters. |
| Clustering / community summary | Yes | Integrate | Commodity graph algorithms. |
| **Planner + typed operator registry** | **No** | **Build** | **The novel surface** — planning/composition over a *temporal-graph* store. |

Commodity parts (hybrid search, rerankers, semantic routers) are integrated; the build investment is concentrated on the planner and registry.

## V. Operability & Constraints

### A. Latency SLOs *(provisional, pending §I stakeholder thresholds)*
Planner overhead p95 ≤ **600–800 ms**; total retrieval p95 ≤ **3000 ms**; fast-path p95 ≤ **1000 ms**. `[TBD: ratify against consumer success criteria.]`

### B. Compute & deployment bounds
*Provisional:* runs **locally** alongside FathomDB. `[TBD: VRAM/CPU budget for planner LLM + embedding model(s) + cross-encoder running concurrently on a developer machine — confirm the planner can co-reside, or move it to a small/distilled model.]`

### C. Token economics & the overhead question
A closed-loop retrieval (decompose + execute + re-plan) must not cost more than calling a frontier model directly — the core overhead risk. *Provisional:* token ceiling per query `[TBD]`; **fast-path** for queries the classifier marks simple/low-complexity, bypassing decomposition. Break-even (planning vs direct frontier call) measured in Experiment 9.

### D. Security & compliance
**Required:** PII / jailbreak gating at the **front of the planner** — intents screened before any planning. Data handling for the local store: `[TBD: data classification — does FathomDB hold PII?]`. Provenance-leakage check: returning traversed nodes must not expose content the caller shouldn't see.

### E. Observability
Log every plan, operator call, chosen topology, provenance payload, quality signal, and re-plan event; support full trace replay. **This telemetry is the precondition for any future learned routing (bandits/RL) — which is deferred until the logs, a golden set, and a reward signal exist.**

## VI. Execution Proposal & Resource Request

### A. Phase 1 scope
Approval of this document authorizes work on **Gate 0 (golden eval set)** and **Gate 2 (oracle-routing upper bound)** *only*. No planner, router, or closed-loop code is built in Phase 1.

### B. Resource & time allocation *(provisional)*
`[TBD: ~2 engineers × ~3 weeks]` + `[TBD: $ budget]` for synthetic eval-data generation and oracle-routing API calls.

### C. Post-gate decision point
On `[TBD: date]`, review Gate 2 results with stakeholders in a `[TBD: format — e.g. 30-min readout + this doc updated to "Decided"]` to choose: proceed to the full architecture build, or pivot to the static baseline (IV.A).

### D. Sign-off line
Decision owner `[TBD]`; reviewers `[TBD: engineering lead, product, architecture]` — single approval line, no RACI.

### E. ADR spin-out register
As gates resolve questions, these become standalone ADRs: **closed-loop vs upfront planning** (trigger: Experiment 6) · **plan DSL vs free-form LLM output** (trigger: Experiment 8 + §II.E) · **defer learned routing** (trigger: telemetry + reward signal available, §V.E) · **primary interface: typed tool-calling vs GraphQL** (trigger: §I.B resolution).

---

### Open unknowns blocking finalization
1. Primary interface + provenance schema (§I.B) — reshapes §I, §II.D.
2. Is the planner an LLM? (§II.E) — reshapes stability and §V.C overhead.
3. Stakeholder success thresholds (§I) — must precede §V.A SLO numbers.
4. FathomDB data classification (§V.D) — gates the security design.
5. Gate-2 metric + margin (§III.C) — must be ratified before Phase 1 starts.
6. The entire §III.D ladder is inert until Gate 0 exists — treat it as the literal first task.
