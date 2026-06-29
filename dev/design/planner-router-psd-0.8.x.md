# FathomDB — Preliminary Solution Design: the query-planner/router surface (DRAFT)

**Component:** FathomDB's **candidate in-library query-planner / router** — a transparent, hintable, overridable *default* route over a portfolio of coexisting retrieval mechanisms, plus the observability surface that makes it inspectable and the agent-feedback loop that makes it improvable. It is **not** an authority in front of FathomDB; it is the planner FathomDB runs *by default* and the agent can always inspect, hint, or override. *(Locus — agent-side / in-library / both-layered — is **gated by `EXP-Fr-acc` + `EXP-S`**, not assumed; "in-library" is the recommended target, not a settled decision. See §II.E, §VI.E, and `initial-arch` §2/§9.2.)*
**Status:** `Draft` · **Decision owner:** [TBD: name/role] · **Gate requested:** authorize the reconciliation gates (Gate 0 re-scope + Gate 2 parity, Phase 1) against the **existing** experiment tree.
**Convention:** `[TBD: …]` = unresolved, must be settled before this section is final. *Provisional:* = a position taken for review, expected to be ratified or overturned by evidence.
**Voice rules in force:** no *deterministic/guaranteed/no-hallucination* framing; every quality/threshold number below is provisional until measured in §III. Where a position rests on a measured result, the FathomDB source doc + section is cited.

**Grounding docs (read as the substrate this PSD must fit):** `initial-arch-planner-router-0.8.x.md` (FathomDB's own router/observability stance — load-bearing), `0.8.x-portfolio-features-and-experiment-tree.md` (features/functions + experiment tree + stacking matrix), `0.8.4-COMPREHENSIVE-REPORT.md` (the GraphRAG head-to-head — **conclusion SUPERSEDED**, read its banner).

**Doc relationship:** this PSD is the **planning input** and **informs the planning**; the **contract, architecture, and design realization** are often written into `initial-arch-planner-router-0.8.x.md` (the stance/contract layer). The two must **start aligned — aligned, not identical**: this PSD plans *against* that contract; that doc records what the design *realizes*.

---

## I. Concept of Operations (ConOps)

### A. Primary actors, ownership, and system boundaries

Consumers are **personal agents** — ephemeral coding harnesses and remote LLMs (Memex is the driving consumer) — and, transitively, the human developer driving them.

The single most important framing correction over earlier drafts: **the agent owns INTENT; FathomDB owns MECHANISM** (`initial-arch` §0 "The contract in one box," §1). The router is **FathomDB's query planner, modeled on SQLite's**: it runs **by default** (batteries-included), it is **observable** (an `EXPLAIN`-equivalent), it is **hintable**, it is **overridable**, and it **never supersedes the agent** (`initial-arch` §0, §4-Q3, §7). Routing is a *judgment about intent*, and the agent holds the goal-graph / session / user-model context FathomDB structurally cannot see — so a FathomDB-internal classifier "can only ever be a guess at what the agent already knows" (`initial-arch` §1). The router therefore classifies intent **only as a fallback**; the preferred paths are *the agent passes intent* or *Fathom calls back to Memex* (the provider-callback pattern, `initial-arch` §5.5/§7.3).

**Two layered surfaces, not one monolithic shim** (`initial-arch` §2 "Two surfaces"):

- **L1 — mechanism surface.** Each mechanism is a governed verb with **knobs** and a **cost/latency tier**: leaf retrieval, fused-RRF `search`, CE-rerank (`alpha`/`pool_n`), `C` map-reduce, `D2` coverage, the (default-off) graph arm. *Partially shipped today*: `Engine.search(query, source_type, kind, created_after, status, rerank_depth, use_graph_arm, alpha, pool_n)` already lets the caller steer filters, rerank depth, and — as of **EXP-0 (landed 2026-06-25)** — the CE blend `alpha`/`pool_n` (`initial-arch` §3). A sophisticated agent **drives L1 directly**; the router is bypassed.
- **L2 — router/dispatcher surface (`Fr`, UNBUILT).** An optional in-library `query(intent=…)` that auto-classifies, picks an L1 stack, and **returns what it picked and why**. It is an **overridable default** for thin consumers and a **hint** for sophisticated ones (`initial-arch` §2 line 46, §7).

The **recommended target** for "agent-side **or** in-library dispatcher?" is **both, layered** — ship L2 as an overridable default, keep L1 fully drivable underneath — but this is **contingent on `EXP-Fr-acc` (locus decision) + `EXP-S` (substrate KILL path)**, not a settled decision (`initial-arch` §2/§9.2). The boundary below the operator calls — storage, indexing, embedding, graph extraction — is FathomDB's; the planner/router does not own it. Note that substrate is *evolving, not frozen* (see §V.B and §VI): kind-tagged coexisting indexes are a prerequisite the router depends on (`EXP-S`), not a static given.

### B. API surface & interface contract

*Provisional:* primary surface is a **typed tool-calling JSON schema** (the consumers are LLM tool-callers), with a **GraphQL** option for structured non-LLM clients. `[TBD: confirm primary surface with consumer teams — this choice reshapes §I and §II.D.]`

Sample input (illustrative):

```json
{ "intent": "retrieve",
  "query": "architectural decisions about the SINE module in Q1",
  "agent_intent_label": "needle|temporal",     // agent-supplied; falls back to router classify
  "constraints": { "temporal": {"from":"2026-01-01","to":"2026-03-31"},
                   "relation": "Decision", "subject": "SINE" },
  "explain": true,                              // opt-in retrieval-EXPLAIN (EXP-OBS), zero-cost when off
  "budget": { "max_latency_ms": 3000, "max_tokens": 8000 } }
```

Structural output: **ranked content chunks** plus — *when `explain=True`* — a **retrieval-`EXPLAIN` object** (per-arm provenance + per-hit score breakdown + executed plan; see §II.D and §V.E). The agent verifies, cites, re-queries, or **returns a relevance signal** from this rather than trusting an opaque answer.

**⚑ Substrate flag.** The rich `EXPLAIN` object is **aspirational substrate (`EXP-OBS`), not available today.** Observability is *weak today — the real gap* (`initial-arch` §3): `ce_score` (EXP-0) is the **first and only** retrieval-explainability field; there is **no per-arm provenance, no `ce`-vs-`rrf` score breakdown, no retrieval-`EXPLAIN`** (`EXPLAIN QUERY PLAN` exists only as a graph-BFS test seam, not a caller surface). `TraceReport`/`source_id` are **write-lineage** (which source doc a row came from), **not** retrieval explainability. The agent-supplied parts of the object — `agent_intent_label`, the **executed plan/topology** (**available once L2 (`Fr`) exists** — the router builds the DAG, so it knows it; for an **L1-only call today there is no router-built DAG/rationale**, only caller-supplied params + `ce_score`), and `ce_score` (partially real) — are available; **per-arm provenance + score breakdown are `[TBD: EXP-OBS deliverable]`**. EXP-OBS is **doubly load-bearing**: it is the prerequisite for *both* a transparent router *and* the agent-relevance-signal loop (§I.D, §V.E).

### C. Critical user journey (happy path) — fused-RRF + CE-rerank, not graph BFS

An ephemeral agent is asked to refactor the SINE module and queries for *"all architectural decisions about SINE in Q1."*

1. **Intent.** The agent passes `agent_intent_label = needle|temporal` (its own classification; the router would only *guess* this, so the agent's label is preferred — `initial-arch` §1/§5.5). No internal classification is needed.
2. **Plan.** The router emits a DAG on the **measured strong path**: fused-RRF `search` (BM25 ⊕ vector ANN) with the agent's filters → **valid-time filter as a precision gate** → **CE-rerank** (`alpha`/`pool_n` carried per-feature). Ordering follows **OD-4** — *expand → valid-time filter → rerank* — because filtering *before* expansion shrinks the pool the recall lever is meant to grow (`portfolio` §3.5, OD-4). The graph BFS arm is **default-OFF** and not on this path (it is refuted ×2; see §IV.C).
3. **Execute + observe.** The executor runs the DAG. With `explain=True`, each hit carries its arm provenance + `ce_score`/`rrf_norm` breakdown.
4. **Relevance check (the new first-class step).** Internal `ce_score` is high and the route margin is wide → the **VoI policy decides internally** (no agent round-trip needed; §I.D, §V.C). Had confidence been low, the router would have escalated to the agent for a relevance signal.
5. **Return.** The agent receives the decision chunks + provenance and writes the refactor against cited sources.

**⚑ FLAG:** ConOps depth is capped by the unresolved surface choice (B) and the `EXP-OBS` provenance schema. One trace lives here; multi-hop/sensemaking traces go to an appendix.

### D. The agent as a relevance-signal partner *(first-class)*

The agent is **part of the solution, not just the caller.** FathomDB owns getting-the-right-data (mechanism + a good default route + honest observability) and **must not abdicate it** — but the agent holds the intent/goal-graph context FathomDB cannot see (`initial-arch` §1), so it can supply an **exogenous relevance signal** on whether the returned data (and the telemetry, when requested) is relevant to what it was querying for. That signal is a **first-class input to route planning** — a *second, higher-quality* source that can **confirm, override, or pre-empt** FathomDB's internal `ce_score` judgment.

The loop: **route → return data (+ optional telemetry) → agent judges relevance → (optionally) signals back → route re-plan / route-prior update.** This *refines* the purely-internal closed loop of §II.C by adding the second signal source; it does not replace it.

*Provisional:* this is grounded in four strands of theory, each tied to a concrete design choice and a FathomDB node (detail in §II.C, §III.D, §V.C):

- **IR relevance feedback (Rocchio / interactive IR):** the agent is a *real* relevance judge, the principled superset of pseudo-relevance feedback (PRF) — and it **removes PRF's circularity** that FathomDB flagged (`PRF × CE-rerank` CONFLICT-risk: "PRF presumes the precision the rerank adds," `portfolio` §3.5).
- **Value of Information / decision theory:** asking the agent costs a round-trip, so *ask-vs-decide-internally* is a VoI break-even — the same discipline the PSD applies to re-plan (§V.C), and it honors the $0 boundary (the agent call is the agent's spend; §V.B).
- **Contextual bandits / RL:** route selection over arms is explore-exploit over a context; the agent relevance signal is exactly the **missing reward signal** the deferred-learned-routing ADR waits on (§V.E).
- **Active learning:** agent labels are scarce, costly supervision — sample them where uncertainty is highest (low `ce_score`, narrow route margin).

How the FathomDB⇄agent interplay is parameterized — internal-only vs agent-signal vs hybrid-via-VoI — is **settled by experiment and data** (§III.D), not asserted.

## II. Core Architecture Hypothesis  `[BUILD-DEFERRED — gate-design depth only]`

**Central problem this section confronts:** the router can estimate **cost** and **cannot a-priori estimate per-query efficacy** the way a SQL optimizer uses table statistics. *Provisional, with one correction:* **efficacy is the genuinely hard axis** and remains the spine — but **cost is not free a-priori either**. Per-stack latency/cost tiers are a **first-class measured OUTPUT of EXP-A/B′/C** (CPU / GPU / local-LLM / net-LLM), not a costless estimate (`portfolio` §4 cross-cutting; `initial-arch` §5.6). And efficacy is **partially estimable** via oracle-style upper bounds + measured per-feature numbers (e.g. CE α=1.0 → MRR .347→.587; `portfolio` §3, §5-ce). FathomDB attacks efficacy with measurement, not resignation; §III's oracle bound *reconciles with* those existing bounds rather than inventing a new framing.

### A. Logical layer — the planner

Translates an incoming intent into a DAG over a **fixed vocabulary of typed operators with knobs**. Intent class comes, in preference order: **(1) the agent's passed label**, **(2) a provider-callback to Memex**, **(3) an internal classifier as fallback only** (`initial-arch` §5.5). The classifier's taxonomy is the project's **5 feature/intent classes** — `{needle | multi_session | temporal | global | multi-hop}` (`portfolio` §2 F1–F5; EXP-Fr-acc, `portfolio` §4) — **not** a six-way `{temporal, graph-relational, lexical-exact, semantic, exploratory, multi-hop}` set. *Lexical-exact* and *semantic* are **sub-mechanisms inside one feature's RRF-fused stack** (`portfolio` §2 F0), not top-level classes; *graph-relational* is not a primary class (the graph arm is refuted). The planner retrieves nothing itself.

### B. Physical layer — the **config-carrying** router/executor

The router does **not** merely pick an operator/index per plan node. Because **one function serves many features with conflicting configs** (one code-path, many configurations — `ce_rerank` wants α=0.3/narrow for F1's C6 guard but a *wider* pool for F2; `Engine.embed` serves opposite granularities), the router selects the **full `(index, retrieval, α, pool_n, MMR, recency)` tuple per feature** (`initial-arch` §5 "Config-carrying, not just index-picking"; `portfolio` §3-coupling note). A config chosen for feature X **must not regress feature Y** — the §3-coupling hazard, guarded by **EXP-B′.5** (the router-stack joint-regression guard).

It binds each plan node to a concrete operator-with-config from the registry — **BM25/full-text, vector ANN, RRF, CE-rerank (`α`/`pool_n`), map-reduce QFS (F4-only), community summarization (F4-only), valid-time filter, native-RAG vs long-context adapters**, with **graph BFS default-OFF** — chooses topology (sequential pipeline vs parallel scatter-gather), and runs the DAG.

**Router-isolation, not a free stack.** Operators do **not** all compose. The summarize/map-reduce step that *helps* F4 sensemaking *hurts* F1/F2 needle (blind distiller **−0.362**, `portfolio` §3 / §3.5 "map-reduce-C × precision-rerank"). The router **must not cross these wires**: map-reduce/QFS and community summarization are **valid only for the `global` (sensemaking) class** and are **forbidden on `needle`/`multi_session`/factoid paths**. This is a router-isolation constraint, encoded as a forbidden composition in the plan validator (§II.E), not a stackable convenience.

### C. Closed-loop re-plan mechanism — **hypothesis under test (two signal sources)**

The earlier draft closed the loop on **one** signal: a mid-execution `ce`-confidence dip. We keep that but add the agent relevance signal as a **second, higher-quality source**, and let experiment settle the interplay. Three regimes, *to be discriminated by experiment, not assumed*:

1. **Internal-only.** Re-plan driven solely by `ce_score`. Cheap ($0-local, no round-trip), but `ce_score` is the *only* retrieval-explainability field that exists (`initial-arch` §3) and CE confidence can be **wrong in a known way** — a high-CE-wrong candidate can displace a BM25-correct factoid (the **C6 guard**, default α=0.3; `portfolio` §5-ce). So internal confidence is exactly the quantity that benefits from an exogenous check. It is also **`pool_n`-blind**: deep-recalled gold can land at rank ~15–30, invisible to a `pool_n=10` CE (OD-2, `portfolio` §3.5).
2. **Agent-signal.** The agent judges relevance-to-intent and signals back. Higher quality (it holds intent FathomDB lacks, `initial-arch` §1; breaks PRF's circularity, `portfolio` §3.5) but costs a round-trip (the agent's spend, §V.B).
3. **Hybrid via VoI (the recommended target shape — *Provisional:*).** Internal `ce_score` runs by default; the **ask-or-not VoI policy** escalates to an agent signal only under uncertainty (active-learning trigger). The agent signal can **confirm** (cheap validation), **override** (veto a route Fathom was confident in), or **pre-empt** (supply intent up front so the route is right first time — the provider-callback case).

**VoI ask-or-not policy (sketch — all thresholds *Provisional:* until measured):**

- **Decide internally** when `ce_score` is high **and** the margin to the runner-up route is wide (`initial-arch` §6 surfaces both).
- **Ask the agent** when `ce_score` is low **or** the route margin is narrow (active learning) **and** the expected mis-route cost saved exceeds the round-trip cost (VoI).
- **Mis-route cost is asymmetric and partly measured:** routing a needle to `C` (map-reduce) summarizes the needle away — **−0.362 + an LLM call** (`portfolio` §3.5; EXP-Fr-acc owns this cost matrix). So the "ask" threshold is **not symmetric**: be far more willing to pay a round-trip when the candidate route risks a high-cost cross-wire than when it risks a cheap same-tier miss.
- **Depth bound** inherits `[TBD: 1–2]` re-plan cap; whether the agent loop is **one-shot or iterative** is an open experiment question (§III.D, EXP-AF).

**This loop — and the relative value of the three regimes — is the claim Gate 2, EXP-Fr-acc, and the optional EXP-AF must justify. It is not asserted as correct here.**

Note the recall × CE-rerank interaction this loop sits on top of is a **constrained joint optimization, not additive**: α=1.0 @ `pool_n=50` *drops* r@10 0.548→0.498 (CE-confident distractors displace base-favored gold), the EXP-B′ crux (`portfolio` §3.5 row 1). An agent relevance signal can help **re-rank within a recalled pool**, but it does **not** dissolve the joint optimization, and it does not manufacture recall the substrate never produced.

### D. Provenance & metadata packaging — the retrieval-`EXPLAIN` surface (`EXP-OBS`)

When `explain=True`, the router returns the **retrieval-`EXPLAIN` object** (`initial-arch` §6):

- **Per-hit provenance:** which arm(s) surfaced this hit (vector-ANN / FTS-BM25 / graph), rank in each. `[TBD: EXP-OBS deliverable]`
- **Per-hit score breakdown:** `rrf_norm`, `ce_score` (shipped), the blended `score`, and which filters excluded candidates. `[TBD: EXP-OBS deliverable — only ce_score exists today]`
- **Query-level trace:** k, `pool_n`, α, MMR, embedder identity, timings (reuse `counters()`/profiling).
- **Executed plan + router rationale (when L2 exists):** chosen stack, runner-up, confidence, cost tier (this part is the router's own knowledge).
- **Shape:** opt-in, **zero-cost when off**, generalizing the existing graph-`EXPLAIN` seam + `TraceReport` rather than inventing new machinery.

Keep **write-lineage** (`source_id`: which source doc a row came from) as a *separate, already-shipped* field, **distinct** from retrieval explainability (`initial-arch` §3). EXP-OBS is the substrate the agent **judges relevance against** (§I.D) and the future **reward log** (§V.E) — hence doubly load-bearing.

### E. Plan stability & determinism

*Provisional:* constrained **plan DSL over typed operators + a validator** (which encodes the §II.B router-isolation rules as forbidden compositions), with **plan caching/memoization** keyed by query template; distillation considered later. Planner-output variance target `[TBD]`, measured downstream. **⚑ FLAG:** if the planner is *not* an LLM, much of E and the §V overhead math change — `[TBD: is the planner an LLM?]`.

**Distinct from a second determinism that lives a layer down.** Planner-output stability (above) is *not* the same as **substrate determinism**. The coexisting-index substrate is **not fixed**: kind-tagged leaf/coverage/graph row-kinds are a future engine step (`EXP-S`), and a substrate **determinism/perf failure is a go/no-go on router locus** — its KILL path is "router stays agent-side, indexes stay eval-side" (`portfolio` §4 EXP-S; `initial-arch` §8). Do not conflate the two; the locus decision is owned by EXP-Fr-acc.

## III. The Validation Gates (Design of Experiments)  ★LOAD-BEARING

The gates below **reconcile with the existing experiment tree** (`portfolio` §4; `initial-arch` §8) — they are *not* greenfield. The spine is already partly landed:

`EXP-0 (LANDED) → (EXP-A recall ‖ EXP-M4 embedder ‖ EXP-S substrate) → EXP-B′ (3-stage joint tuning) → register-or-diverge → EXP-Fr-acc (router accuracy + asymmetric mis-route cost) → EXP-Fr (build dispatcher)`, with **EXP-OBS riding alongside EXP-A/B′, before EXP-Fr**, plus EXP-C/D/E (F4/F5 + corpus), EXP-F0 (fidelity, opportunistic), EXP-OPP2 (recency).

### A. Gate 0 — Reuse the existing eval substrate; scope new data to the real gap

The eval substrate is **already rich**: **LME/LOCOMO** (memory classes, vs Mem0), **AP-News** (sensemaking, vs GraphRAG), and **MuSiQue (2,417 answerable of 4,834 total)** (multi-hop / F5) — with **registered, HITL-signed decision rules where they apply**: `decide_083` (paired-delta, MDE ≤ 0.05, per memory class) governs the **Mem0 memory-class** axis, and `decide_084` (win-rate near-parity band ε=0.05, question-*clustered* bootstrap) governs the **AP-News / GraphRAG sensemaking** axis (both with power guards, `portfolio` §5). MuSiQue is existing **F5 substrate**, but its **HippoRAG-2 comparison rule is still `[TBD: decide_08x]`** (`portfolio` §5; competitor unbuilt). So "nothing is measurable without a fresh 50–100-query build" is **false**: a measurement framework + assets exist.

Therefore Gate 0 is **re-scoped from "build from scratch" to "reuse existing assets + decide_083/084 rules."** Any per-intent-class gold-supporting-node labels needed for oracle routing are **derived from the existing corpora where they exist** (e.g. MuSiQue ships supporting-paragraph labels) rather than via a fresh generic 50–100-query build — **but** where the reused corpora **lack FathomDB-node-level retrieval labels** for an intent class, a **small, scoped gold-supporting-node labeling pass** over those corpora is genuine new work (far smaller than a fresh golden set), budgeted in §VI.B. Where genuinely new *data* is required, it is **scoped to the specific gap**: an **entity-rich AutoQ-style ~269+-question set for F4/M6 registration** (EXP-D), which remains the only new corpus-acquisition item. Acknowledge the **corpus cap**: `decide_084` is corpus-capped at N=200 (AP-News max; comp MDE 0.058 > ε), and because the bootstrap is question-clustered, **more runs cannot tighten the MDE — only more questions can** (`portfolio` §5; `0.8.4-report` banner: "N=200 is the corpus maximum"). Registration is a **corpus/decision problem, not a re-run.**

*(Class count: this PSD uses the project's **5** classes, resolving the earlier draft's `[TBD: ratify class count]` toward `{needle | multi_session | temporal | global | multi-hop}`.)*

### B. Gate 2 — Oracle-routing upper bound (kill/justify discipline, kept)

Exhaustively run candidate plans per query, pick the best (a perfect router's ceiling) to **bound the most dynamic routing could ever buy**. This is sound and **reconciles with FathomDB's existing oracle-style efficacy bounds** (recall@K_deep gold-in-pool, EXP-A; per-feature CE numbers; the measured oracle +0.39 over Mem0 in the 0.8.3 ledger) — feed it the **measured per-arm cost tiers** (EXP-A/B′/C) so it is cost-aware, and couple it to **EXP-Fr-acc's asymmetric mis-route cost matrix** rather than a single Recall@K margin.

### C. Go / No-Go criteria — **parity-vs-competitor**, not beat-an-internal-baseline

The earlier draft's "beat a fixed BM25+vector+RRF baseline by ≥ 15–20%" is **wrong-comparator**: the fused-RRF hybrid is **FathomDB's own strong baseline** — it already *beat* the refuted graph arm (ΔF1 −0.0405, `portfolio` §3.5) — it is the **floor already met**, not a strawman to clear by a margin. MEMORY/strategy explicitly forbid reintroducing "beat-fused-RRF" framing.

*Provisional:* the go/no-go is **competitor-parity-or-better** under the registered rules — `decide_083` vs **Mem0** (F1/F2/F3) and `decide_084` vs **Microsoft GraphRAG** (F4), each ε=0.05 near-parity band + power guard (`portfolio` §5). The router earns its build via **EXP-Fr** (gated on EXP-B′ stacks-diverge-per-intent ∧ EXP-S substrate ∧ EXP-Fr-acc classifier accuracy / asymmetric mis-route cost), **not** a recall margin over a fixed baseline. `[TBD: ratify the per-feature parity margin — the 15–20% figure does not apply; the operative bands are decide_083/084's ε=0.05.]`

### D. Downstream experiment ladder — **mapped onto the existing tree, not a fresh ladder**

The earlier draft's 10-experiment ladder largely **duplicates landed/named work**; map each onto the tree and flag duplications:

| Draft experiment | Maps to | Status / note |
|---|---|---|
| 7 — RRF vs RRF+cross-encoder | **EXP-0** | **LANDED 2026-06-25** — measured (MRR .347→.587, r@1 ×3.9). Re-running it duplicates shipped work. |
| 1 — per-operator characterization | **EXP-A** (+ §3 feature×measure matrix) | EXP-A is narrower (recall-generation for F2); reuse it. |
| 3 — intent/complexity classifier accuracy | **EXP-Fr-acc** | near-exact match. |
| 6 — upfront vs closed-loop re-plan | **EXP-Fr-acc extension + EXP-AF (new)** | the genuine net-new contribution (the two-signal / VoI loop). |
| 10 — provenance → agent self-correction | **EXP-OBS-gated** | extend to measure the **agent relevance signal feeding back into routing**, not just self-correction on output. |
| 4/5/8/9 — topology / estimator / determinism / e2e | feed EXP-B′, EXP-Fr-acc, EXP-S, and the parity runs | reconcile, don't greenfield. |

**Prerequisites and net-new nodes:**

- **EXP-OBS (extend).** The retrieval-`EXPLAIN` surface of §II.D **plus a reward-signal logger** (executed plan, chosen+runner-up stack, confidence, `ce_score`, and — when returned — the agent relevance label keyed to the route). This is the "logs + reward signal" the deferred-learned-routing ADR requires (§V.E). Prerequisite for the agent-signal arm and any future bandit/RL router.
- **EXP-Fr-acc (extend).** Keep classifier accuracy + asymmetric mis-route cost + router-locus decision; **add** (a) *value-of-signal* — does an agent relevance signal beat internal `ce_score` alone? (b) *ask-or-not VoI policy* — at what `ce_score`/route-margin does asking pay for its round-trip? (c) *asymmetric weighting* — does it cut the high-cost cross-wire mis-routes (needle→C) more than cheap ones?
- **EXP-AF (new, optional).** Dedicated agent-feedback node: lift vs `ce_score`-only on the **existing** substrate (no fresh 50–100-query build); round-trip cost + realized VoI break-even; one-shot vs iterative within the `[TBD: 1–2]` depth bound; mis-route reduction. **Prerequisite: EXP-OBS.** May live inside EXP-Fr-acc or split out (steward's call). **KILL path:** if the agent signal does not beat `ce_score`-only net of round-trip cost → drop the agent-signal loop; router stays on internal `ce_score` (mirrors EXP-S's KILL discipline).

## IV. Trade-offs & Alternatives Considered  ★LOAD-BEARING

### A. Static hybrid pipeline (fused-RRF) — FathomDB's **strong baseline / floor**

Fast, well-understood, already shipped. It is the **measured strong baseline** (it beat the graph arm), not a fallback strawman. RRF *can* dilute a single strong signal in the textbook sense — but for FathomDB the live signal-mixing concern is the **inverse**: the **C6 guard (default α=0.3)** deliberately keeps the RRF blend dominant so a high-CE-*wrong* "strong signal" does **not** displace a BM25-correct factoid (`portfolio` §5-ce). Do **not** use "RRF dilutes" to motivate a dynamic router over the fused baseline; the real composition hazard is the **recall × CE joint-optimization** (§II.C, EXP-B′ crux). The legitimate multi-hop/sensemaking weaknesses (below) are the motivators — *not* a graph-routing win (the graph remedy is refuted).

### B. Massive context / no retrieval ("put it all in a long-context model")

Zero routing logic. Rejected as the default: prohibitive token cost and latency at scale, context pollution on precise temporal/edge queries where the answer is a few nodes, and it **crosses the $0/local footprint invariant** (§V.B). This aligns with FathomDB's preference for local $0 retrieval over corpus-in-context (`0.8.4-report` §6) — an operating-point preference, not a hard prohibition.

### C. Per-operator build-vs-integrate *(corrected to measured state)*

| Operator | State today | Build / Integrate | Note (grounded) |
|---|---|---|---|
| BM25 / full-text | Shipped [S] | Integrate | Commodity; F0 substrate. |
| Vector ANN (1-bit quant) | Shipped [S] | Integrate | F0 substrate; relevance ceiling ≈0.571 (`portfolio` §3). |
| RRF fusion | Shipped [S] | — | The strong baseline (§IV.A). |
| **CE-rerank (`α`/`pool_n`/`ce_score`)** | **BUILT [S] — EXP-0** | **Expose/tune, *not* integrate** | Already in-engine; EXP-0 (LANDED 2026-06-25) exposed the knobs. Default α=0.3 (C6 guard); α=1.0 opt-in (MRR .347→.587). Remaining work = per-feature `α`/`pool_n` config + the joint-tuning hazard, not model integration (`portfolio` §5-ce). |
| Valid-time filter | Shipped [S] | — | Real & shipped (see §V; OD-4 ordering). |
| Graph BFS / traversal | Shipped [S] **but measured-REFUTED ×2; default-OFF** | Opt-in only | ~0 recall add, ΔF1 −0.0405 vs fused-RRF; entity co-mingling = length-norm bias. F5 multi-hop value unproven; HippoRAG-2 **unmeasured**. **Do not present as a primary arm** (`portfolio` §2 F5, §3 matrix, §3.5). |
| Map-reduce QFS (`C`) | Built eval-side [E] | **Open, expensive product fork** | *Not* a commodity drop-in: **`C` provisional SURPASS** vs full-strength MS GraphRAG 3.1.0 (comp .72/div .61/emp .72) but **expensive (LLM tier, reads everything)**; `decide_084 = NOT_REACHED` (corpus-capped). **F4-only; router-isolated from needle paths.** |
| Coverage index (`D2`) | Built eval-side [E] | **Open fork — BELOW parity** | Cheap (CPU) but **below-parity**; its earlier surpass was a community-level-0 artifact (**Fork E re-opened**). `0.8.4-report` "GraphRAG wins decisively" is **SUPERSEDED** (read its banner). |
| **Router + typed operator registry (`Fr`, L2)** | **Unbuilt [N]** | **Build** | The novel surface — **config-carrying** planning/composition over a *temporal-graph* portfolio. |

Commodity parts (hybrid search, **already-built CE-rerank**, semantic routers) are not the build investment; the build is concentrated on the **config-carrying L2 router** over the `EXP-S` substrate, plus `EXP-OBS`.

### D. F4 sensemaking — the live, split, corpus-capped product fork

Reflect the **current SPLIT verdict** (`0.8.4-report` banner; `portfolio` §2 F4): **`C` surpass-but-expensive**, **`D2` below-parity**, **both `decide_084 = NOT_REACHED`** on power (corpus-capped at N=200, need ~269 entity-rich Q). Do **not** repeat "GraphRAG wins decisively" — that verdict is superseded. Treat this as an open, HITL-gated, $-tier fork (EXP-C/D, Fork-E), surfacing `C`'s LLM cost tier and keeping it router-isolated from needle paths.

## V. Operability & Constraints

### A. Latency SLOs — anchored to existing gates + measured cost tiers

*Provisional:* planner overhead p95, total retrieval p95, fast-path p95 `[TBD]`. **Anchor these to FathomDB's existing latency gates `AC012/013/020`** (the F0 substrate carries them, `portfolio` §3 matrix M8) and express budgets in terms of the **measured per-arm cost/latency tiers** (CPU / GPU / local-LLM / net-LLM) that are a first-class output of EXP-A/B′/C — so the router can **surface a tier and a cost-aware agent can veto an expensive route** (`initial-arch` §5.6). Do not float SLO numbers free of these gates.

### B. Compute, deployment, and the **footprint invariant**

The **in-library query path stays $0/local** — the footprint invariant (`0.8.4-report` §6: "$0 was spent on the in-library boundary … never the library query path"). LLM/GPU spend is **EVAL-side/airlock or USER-controlled opt-in**; HITL (2026-06-23) allows local/cloud/frontier LLMs + GPU as **USER-controlled opt-in spend knobs** ("function can outweigh footprint" — one operating point, not a hard gate).

*Provisional:* the router runs **locally** alongside FathomDB. **A planner LLM is an explicit USER-controlled opt-in spend tier, not a silent local default.** `[TBD: is the planner an LLM?]` — prefer a **non-LLM / distilled planner, or the agent's own LLM**, for the default $0 path; if a planner LLM is used, **surface its cost tier to the agent** so it can veto. The **agent-relevance round-trip is the agent's spend** — the $0 boundary is honored only because the call originates agent-side. `[TBD: VRAM/CPU budget for any co-resident planner + embedder + cross-encoder.]`

### C. Token economics, the overhead question, and Value-of-Information

A closed-loop retrieval (decompose + execute + re-plan) must not cost more than calling a frontier model directly — the core overhead risk. *Provisional:* token ceiling per query `[TBD]`; **fast-path** for low-complexity queries, bypassing decomposition; break-even measured downstream (EXP-Fr-acc / EXP-AF). **This is the same break-even discipline applied to the agent round-trip:** *ask-the-agent vs decide-internally (`ce_score`)* is a **Value-of-Information** decision — pay for the agent's relevance signal only when its expected reduction in (asymmetric) mis-route cost exceeds the round-trip cost (§II.C). Cost tiers are **estimable** (measured per arm, EXP-A/B′/C); **efficacy is the hard axis**, attacked with oracle bounds + per-feature numbers (§II intro, §III.B).

### D. Security & compliance — relocated per **agent-owns-intent**

The earlier draft's "PII / jailbreak gating at the **front of the planner**" mis-locates authority. Screening incoming **intent** for PII/jailbreak **is intent classification**, which belongs to the **agent (Memex)** — it holds the user/session/goal context FathomDB cannot see (`initial-arch` §1). FathomDB must **not** position itself as the intent-policing authority, and a heavy front classifier would also cross the $0/local boundary (§V.B).

- **Agent boundary (Memex):** intent / PII / jailbreak screening.
- **FathomDB (mechanism):** the **provenance-leakage check** — returning traversed nodes/provenance must not expose content the caller shouldn't see — and `[TBD: data classification — does FathomDB hold PII?]`.

### E. Observability — doubly load-bearing; the reward log

Log every plan, operator-with-config call, chosen topology, retrieval-`EXPLAIN` payload, quality signal (`ce_score`), agent relevance label (when returned), and re-plan event; support full trace replay. This telemetry is **doubly load-bearing**: (1) it is the **transparency surface** a hintable/overridable router requires (`initial-arch` §6), and (2) it is the **substrate the agent judges relevance against** — the agent cannot give a good signal without telemetry to judge against. The same log is the **reward signal** for a future learned router (bandits/RL), which is **deferred until the logs, a golden set, and a reward signal exist** (§V.E ADR) — and **the agent relevance signal is precisely that missing reward signal**. EXP-OBS is therefore prerequisite + reward-logger; today **only `ce_score` exists** (`initial-arch` §3).

### F. Temporal & recency — shipped filter vs unbuilt provider

**Temporal is real and shipped**, and the router treats it correctly: **bi-temporal edges + supersession (G0 `logical_id`) + a valid-time filter** are all `[S]` shipped (`portfolio` §2 F3; MEMORY: G0 identity = `logical_id` alone). The **valid-time filter** is a precision gate that **composes in order** with recall expansion — *expand → filter → rerank* (**OD-4**, `portfolio` §3.5): it runs **post-expansion / pre-rerank**, **not before expansion** — filtering before the recall lever runs shrinks the very pool OD-4 is there to grow. (A purely non-lossy metadata constraint may be pushed earlier only if that is separately intended and measured.)

**Caution — recency is not the same as the valid-time filter, and must not be done lossily.** The F3 "latest-fact" **recency provider is unbuilt (`OPP-2` / `EXP-OPP2`, `[N]`)**. When it lands, **recency must be a POST-retrieval rerank *weight*, never a pre-retrieval content *rewrite*** (**OD-6**, `portfolio` §3.5): a pre-retrieval rewrite/consolidation invokes the measured **−0.362** blind-distiller lossiness penalty and can delete a still-wanted fact. The planner must therefore keep two things distinct: (a) `temporal` valid-time **filtering** (shipped, post-expansion/pre-rerank, lossless) vs (b) recency **judgment** (unbuilt, post-retrieval rerank weight) — and must not conflate temporal filtering with lossy consolidation.

## VI. Execution Proposal & Resource Request

### A. Phase 1 scope

Approval authorizes the **reconciliation gates + the EXP-OBS first increment**: the **Gate 0 re-scope** (adopt existing LME/LOCOMO/AP-News/MuSiQue assets + the applicable decide_083/084 rules; scope new data to the ~269-Q F4 gap), the **Gate 2 parity bound** (oracle ceiling reconciled with the existing tree), and the **EXP-OBS first increment** (the retrieval-`EXPLAIN` surface — `ce_score` is already shipped; next is per-arm provenance + score breakdown), since observability rides alongside EXP-A/B′ and is the prerequisite for everything downstream (§III, §V.E). **No L2 router, closed-loop re-plan, or agent-feedback code is built in Phase 1** — those remain gated on **EXP-OBS ∧ EXP-S ∧ EXP-B′ ∧ EXP-Fr-acc** per §III.

### B. Resource & time allocation *(provisional)*

`[TBD: ~2 engineers × ~3 weeks]` for the reconciliation + EXP-OBS first increment. `[TBD: $ budget]` is **corpus-acquisition** for the F4 entity-rich set (EXP-D) + judged parity runs + a **small gold-supporting-node labeling pass** over the reused corpora where node-level labels are missing (§III.A) — **not** generic eval-data generation (the substrate exists). The agent round-trip is the agent's spend (§V.B).

### C. Post-gate decision point

On `[TBD: date]`, review the reconciled Gate-2 / EXP-Fr-acc results with stakeholders to choose: proceed to the L2 router build (EXP-Fr over EXP-S), diverge to per-intent stacks, or stay on the L1-drive + fused-RRF floor (§IV.A).

### D. Sign-off line

Decision owner `[TBD]`; reviewers `[TBD: engineering lead, product, architecture, steward]` — single approval line, no RACI.

### E. ADR spin-out register

As gates resolve: **closed-loop vs upfront, internal-only vs agent-signal vs hybrid** (trigger: EXP-Fr-acc + EXP-AF) · **plan DSL vs free-form LLM output** (trigger: §II.E) · **router locus — agent-side vs in-library** (trigger: EXP-Fr-acc; determinism-coupled, `initial-arch` §5.7) · **defer learned routing** (trigger: EXP-OBS reward log + golden set available, §V.E) · **primary interface: typed tool-calling vs GraphQL** (trigger: §I.B).

---

### Open unknowns blocking finalization

1. **Primary interface + `EXP-OBS` provenance schema** (§I.B/§II.D) — and note the schema's *substrate* is largely unbuilt today (only `ce_score`).
2. **Is the planner an LLM?** (§II.E/§V.B) — reshapes stability and the footprint/spend story.
3. **Agent relevance-signal protocol shape** (§I.D) — per-hit binary, graded score, or free-text "what I wanted"? Each implies a different Rocchio-style update. Reuse the **provider-callback** transport (`initial-arch` §5.5), not a new channel.
4. **VoI cost-model parameters / ask-or-not policy** (§II.C/§V.C) — concrete break-even (round-trip token+latency price vs expected asymmetric mis-route cost saved). Until measured, all thresholds stay *Provisional:*.
5. **One-shot vs iterative agent loop + depth bound** (§II.C) — inherits `[TBD: 1–2]`; EXP-AF resolves it.
6. **Route prior persisted (cross-query) vs current-only** (§I.D) — persisted ⇒ closer to bandits/RL; current-only ⇒ single-query relevance feedback. Different determinism/testability (cf. locus, `initial-arch` §5.7).
7. **Reward-log retention/format** for a future bandit-RL router (§V.E) — what schema makes the agent label a usable RL reward later.
8. **Who pays the agent round-trip** — confirmed: the **agent's** spend (§V.B); the $0 boundary holds only if the call originates agent-side.
9. **Stakeholder success thresholds + the F4 corpus cap** (§III.A) — parity margins are decide_083/084's ε=0.05, not 15–20%; F4 registration is corpus-capped (~269 Q, +69 past the AP-News max).
10. **EXP-S substrate determinism** (§II.E/§VI) — its KILL path decides router locus; the substrate is not fixed.
