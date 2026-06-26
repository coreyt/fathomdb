# Audit Notes — Planner/Router PSD

**Audited:** `dev/design/planner-router-psd.md` (the colleague's "planning/composition shim" PSD)
**Audited against:** FathomDB's in-repo, measured/decided ground truth —
`dev/design/initial-arch-planner-router-0.8.x.md` (FathomDB's own router/observability stance, load-bearing),
`dev/design/0.8.x-portfolio-features-and-experiment-tree.md` (features/functions, experiment tree, stacking matrix),
`dev/plans/runs/0.8.4-COMPREHENSIVE-REPORT.md` (GraphRAG head-to-head; note the SUPERSEDED banner).
**Date:** 2026-06-26
**Citation key:** `psd` = the PSD under audit · `arch` = initial-arch doc · `tree` = portfolio/experiment-tree doc · `084` = the 0.8.4 comprehensive report.
*Voice:* this audit keeps the PSD's own discipline — no *deterministic/guaranteed/no-hallucination* framing; `[TBD: …]` for unresolved; *Provisional:* for review positions; every quality/threshold number is provisional until measured.

---

## Executive verdict

This is a strong, well-structured systems document. Its information-retrieval and decision-theory instincts are largely correct: it rightly rejects all-in-long-context, insists a dynamic planner must *break even* against a direct frontier call, frames the central problem as the cost/efficacy estimation asymmetry, and proposes an oracle-routing upper bound as a kill/justify gate — all of which reconcile cleanly with how FathomDB already attacks these problems. The PSD's gap is not analytical quality; it is **FathomDB-specific fit**. Three load-bearing framings are inverted or aspirational relative to what FathomDB has measured and decided: (1) the shim is cast as the *authority* that "decides what to call," whereas FathomDB's settled contract is "mechanism in Fathom, judgment in the agent" — the agent owns intent and is never superseded (`arch §0`/§1/§7); (2) the canonical happy path and operator registry lean on **graph BFS** as a primary retrieval path, an arm FathomDB has refuted twice (ΔF1 −0.0405, ~0 recall add, default-OFF — `tree §2/§3.5`; `084 §4`); and (3) the rich per-result **provenance object** is treated as available substrate the shim merely packages, when in fact `ce_score` (EXP-0, landed 2026-06-25) is the *only* retrieval-explainability field shipped today and the rest is the unbuilt EXP-OBS surface (`arch §3/§6`). Several experiments and the Gate-0/Gate-2 criteria also duplicate existing assets or use the wrong comparator (an internal fixed baseline rather than competitor parity vs Mem0/GraphRAG). Net: keep the architecture's spine and discipline; re-anchor its authority model, its primary retrieval path, its provenance assumptions, its gates, and its experiment ladder onto FathomDB's existing facts and experiment tree — and add the agent as a relevance-signal partner, which the PSD's internal-only loop currently omits.

---

## What the PSD gets right

These are genuine strengths; keep them, with only the noted reconciliations.

- **Oracle-routing upper bound as a kill/justify gate** (`psd §III.B`). Sound and aligned with FathomDB's own efficacy method — oracle-style ceilings plus measured per-feature numbers (e.g. the Mem0 gold-only oracle measured +0.39 above competitor; `dev/design/0.8.x-parity-portfolio-strategy.md §36-37`, `dev/experiments-ledger.md`). Keep it as a diagnostic ceiling; only the *comparator* needs to change (see Critical-C4).
- **Rejecting all-in-long-context as the default** (`psd §IV.B`) — token cost, latency, and context pollution on precise queries. Consistent with the footprint invariant: the in-library query path stays $0/local (`084 §6`).
- **The token break-even discipline** (`psd §V.C`, Experiment 9): a closed loop must not cost more than a direct frontier call. This is exactly the VoI ask-vs-decide tradeoff FathomDB wants applied to the agent round-trip (ground truth #17b).
- **The voice discipline** — `[TBD:]`, *Provisional:*, no guaranteed/no-hallucination framing — matches FathomDB's evidence-gated culture. Preserve it verbatim.
- **Deferring learned routing (bandits/RL) until "logs + a golden set + a reward signal" exist** (`psd §V.E`). Correct posture; the agent relevance signal is precisely that missing reward signal (see the Agent-feedback subsection). *Note:* this deferral framing is the PSD's own — do not cite it back to the arch doc, which does not yet use bandit/RL/reward-signal language.
- **Provenance → agent self-correction as a thing worth measuring** (`psd §III.D` Exp 10). Directionally aligned with EXP-OBS and the agent-as-relevance-partner concept; elevate it rather than burying it at #10.
- **Temporal handling as a pre-retrieval valid-time filter** (`psd §I.C`). Correctly shipped: bi-temporal edges + supersession (G0 logical_id) + valid-time filter are `[S]` (`tree §2 F3`). Keep it (with the recency caveat in Minor-d).

---

## Findings

Grouped Critical / Major / Minor. Each: **PSD claim** (quote + section) → **FathomDB reality** (doc cite) → **recommended fix**.

### Critical

**C1 — The shim is framed as the routing *authority*; FathomDB's contract is the inverse.**
- **PSD:** "*The shim's job is planning and composition: it decides what FathomDB functions to call and how to compose them. It owns the planner, the router/executor, and the provenance contract … everything between intent and result is the shim*" (`psd §I.A`); the agent merely "*sends a typed intent and consumes a typed result*."
- **FathomDB reality:** The governing principle is "**mechanism in Fathom, judgment in the agent**." Routing "*does not own the decision. The agent owns intent; FathomDB owns mechanism … it defaults in Fathom but defers to the agent — it never supersedes Memex*" (`arch §0`); "*a FathomDB-internal classifier can only ever be a guess at what the agent already knows*" and FathomDB "*must not make routing an authority the agent has to live with*" (`arch §1`); "*Memex is the authority*" (`arch §7`).
- **Fix:** Reframe the component from "a shim that owns/decides" to **a transparent, observable, hintable, overridable DEFAULT router (the SQLite query-planner analogy) over a mechanism surface, with the agent as final authority on intent.** State explicitly that the router never supersedes the agent, that a bare call still works (default-not-mandate), and that internal intent classification is a fallback only. Keep the correct sub-claim that storage/indexing/embedding/graph-extraction are substrate *below* the operator calls. (The defect is the authority framing, not the existence of an intermediary layer — FathomDB explicitly permits an agent-side router locus, `arch §2`.)

**C2 — The canonical happy path leans on graph BFS as a primary retrieval operator; the graph arm is refuted ×2.**
- **PSD:** the critical user journey "*emits a DAG (temporal filter → graph BFS over `Decision` edges from the `SINE` node → cross-encoder rerank)*" (`psd §I.C`), and the operator table lists "*Graph BFS / traversal | Partial | Integrate | Native to FathomDB's graph layer*" (`psd §IV.C`).
- **FathomDB reality:** the graph arm is **refuted twice** — `ppr_fusion`/BFS loses to fused-RRF (ΔF1 −0.0405; CI upper +0.031 < +0.04 materiality) and adds ~0 recall vs BM25; entity co-mingling drives a length-norm bias; `use_graph_arm` is **default-OFF** (`tree §2 F5` "graph mechanism refuted ×2", `tree §3 F5` "graph adds ~0 / refuted (ΔF1 −.0405)", `tree §3.5` "graph-BFS × fused-RRF | NEUTRAL (refuted) | default off"; M1 decisive NO-GO on MuSiQue, n=300).
- **Fix:** Rewrite the canonical trace so the primary retrieval path is the **measured-strong stack: temporal valid-time filter → fused-RRF leaf retrieval → CE-rerank** (CE α=1.0: MRR .347→.587, r@1 ×3.9). Mark graph BFS/traversal as default-OFF, refuted-as-a-recall-lever, retained only for explicit known-anchor relationship-walks (the shipped G6 `graph_neighbors`/`search_expand` verbs, value unvalidated and roadmap-deferred to 0.8.1) — not as a sensemaking/recall happy path. Do not anchor the flagship ConOps on a refuted operator.

**C3 — The rich provenance object is presented as available substrate; observability is the real gap.**
- **PSD:** output is "*ranked content chunks plus a provenance object — nodes traversed, edges used, temporal bounds applied, per-result confidence scores, and the executed plan*" (`psd §I.B`), and §II.D is titled "Provenance & metadata **packaging**" — treating it as assembly over existing data.
- **FathomDB reality:** observability is "**weak today — the real gap**." `ce_score` (EXP-0, **LANDED 2026-06-25**) is the **first and only** retrieval-explainability field. There is **no per-arm provenance, no score breakdown (ce vs rrf), no retrieval-EXPLAIN**; `EXPLAIN QUERY PLAN` exists only as a graph-BFS test seam (`explain_graph_neighbors_for_test`), not a caller surface; `TraceReport`/`source_id` are **write-lineage** (which source doc a row came from), not retrieval explainability (`arch §3/§6`). EXP-OBS specifies building the rest.
- **Fix:** Mark the retrieval-provenance object as **TO-BUILD (EXP-OBS)**, not available substrate: per-arm arm-provenance + rrf/ce score breakdown + confidence + opt-in `explain=True`. Treat `ce_score` as the first brick. Distinguish the legitimately shim-side parts that *are* packageable (the executed plan/topology is the shim's own artifact; "temporal bounds applied" rides on shipped valid-time filtering) from the FathomDB-emitted retrieval fields that EXP-OBS must build. Note observability is **doubly load-bearing** (it is also the substrate for any agent relevance signal — see the Agent-feedback subsection), so under-scoping it as "packaging" understates a prerequisite deliverable.

**C4 — The Go/No-Go gate uses the wrong comparator (internal baseline +15–20%) instead of competitor parity.**
- **PSD:** "*if oracle routing does not beat the fixed baseline (BM25 + vector + RRF) by ≥ 15–20% on the primary metric (Recall@K and/or answer accuracy), reject the dynamic planner … and ship the fixed baseline*" (`psd §III.B/§III.C`).
- **FathomDB reality:** gates are **PARITY-vs-COMPETITOR**, not beat-an-internal-baseline-by-a-margin: near-parity-or-better vs **Mem0** via `decide_083` (paired-delta, per memory class, MDE ≤ 0.05) and vs **Microsoft GraphRAG** via `decide_084` (win-rate near-parity band ε=0.05, question-clustered bootstrap), each with power guards (`tree §3` matrix, `§5` power table). The "fixed hybrid BM25+vector+RRF" **is FathomDB's shipped fused-RRF path** — the product itself, the floor it already meets (it beat the refuted graph arm), not a strawman to clear by 15–20%. MEMORY's 0.8.3/0.8.4 reframe explicitly says *not* to reintroduce beat-fused-RRF framing.
- **Fix:** Rewrite the gate as competitor-parity-or-better under `decide_083`/`decide_084` (MDE ≤ 0.05, ε=0.05). **Keep** the oracle upper bound as a diagnostic (it reconciles with FathomDB's oracle ceilings), but measure the oracle against the competitor parity band, not an internal fixed-baseline margin.

**C5 — The router is framed as an operator/index *picker*; it must be config-carrying.**
- **PSD:** the physical layer "*Binds each plan node to a concrete operator from the registry … chooses topology … and runs the DAG*" (`psd §II.B`) — router output = an operator pick per node.
- **FathomDB reality:** because one function serves many features with **conflicting configs**, the router must be **config-carrying, not just index-picking**: it selects the full `(index, retrieval, α, pool_n, MMR, recency)` **tuple per feature**, and a config chosen for feature X must not regress feature Y — the §3-coupling hazard, guarded by **EXP-B′.5** (`arch §5` "Config-carrying, not just index-picking"; `tree §3-coupling note`, `§4 EXP-B′.5`). E.g. `ce_rerank` wants α=0.3/narrow for F1 (the C6 guard) but a wider pool for F2; `Engine.embed` serves opposite granularities. The tree calls this shared-function conflict "the single most load-bearing cell."
- **Fix:** Redefine router output as a **per-feature config tuple**, not an operator/index pick, and add the cross-feature joint-regression guard (EXP-B′.5) so a shared-function config tuned for one feature does not silently regress another.

### Major

**M-A — Single monolithic shim vs FathomDB's two layered surfaces (L1/L2).**
- **PSD:** "*everything between intent and result is the shim; everything below the operator calls is FathomDB*" (`psd §I.A`); the only "Build / novel surface" is the planner + registry (`psd §IV.C`).
- **FathomDB reality:** **two layered surfaces**, both exposed — **L1** = mechanism surface (governed verbs + knobs + cost/latency tier, **partially shipped**: `Engine.search(query, source_type, kind, created_after, status, rerank_depth, use_graph_arm, alpha, pool_n)`); **L2** = optional in-library router/dispatcher ("Fr", **unbuilt**). Resolution = **both, layered**: L2 an overridable default, L1 fully drivable underneath; a sophisticated agent (Memex) drives L1 directly and bypasses the router (`arch §2/§3/§7`).
- **Fix:** Replace the single-shim model with the L1 (mechanism verbs+knobs, partly shipped *inside* FathomDB) / L2 (optional unbuilt config-carrying router) layering. Credit L1 as already partly built; position the planner/router work as L2 + config-carrying logic, not a greenfield front door that every caller must traverse.

**M-B — Internal intent classifier as primary mechanism vs fallback-only.**
- **PSD:** "*An intent/complexity classifier tags the query class … and routes simple queries to a fast-path*" (`psd §II.A`); the planner "*classifies this as temporal + graph-relational*" (`psd §I.C`) — the internal classifier is the sole routing mechanism.
- **FathomDB reality:** "*The router classifies intent only as a fallback. Preferred: the agent passes intent, or Fathom calls back to Memex for the intent label (the provider-callback pattern already used for community summaries)*" (`arch §5.5`; `§1`, `§7.3`).
- **Fix:** Demote the internal classifier to a fallback. State the preferred order: **agent-passes-intent > provider-callback-to-Memex > internal-classifier-as-last-resort.** Add the provider-callback pattern as a first-class path — it is entirely absent from the PSD.

**M-C — Cross-encoder rerank labeled "integrate off-the-shelf"; it is already built.**
- **PSD:** "*Cross-encoder rerank | Yes | Integrate | Off-the-shelf models exist*" (`psd §IV.C`); Experiment 7 evaluates it only as a binary "RRF vs RRF+cross-encoder."
- **FathomDB reality:** `ce_rerank` is **already built in-engine** (`crates/fathomdb-engine/src/lib.rs:4973`; blend = α·sigmoid(ce_logit) + (1−α)·minmax(rrf)). EXP-0 (landed 2026-06-25, local main) exposed `alpha`/`pool_n`/`ce_score` through `ce_rerank → rerank_fused → search_reranked` and added `ce_score` to `SearchHit`. Default α=0.3 is the C6 guard; α=1.0 is opt-in for the agentic-answer path; measured CE α=1.0 → MRR .347→.587, r@1 ×3.9 (`tree §5-ce`, `§2 F1`, `§3` matrix).
- **Fix:** Relabel from "integrate off-the-shelf" to "**built in-engine (EXP-0 landed, local main); expose/tune.**" Carry α/pool_n as **per-feature configs** (F1 default 0.3/10 keeps the C6 guard) rather than a generic drop-in. Reconcile the registry with EXP-0's shipped surface.

**M-D — Map-reduce / community summarization treated as freely composable operators; cross-wire lossiness is a measured hazard.**
- **PSD:** the registry lists "*map-reduce query-focused summarization, clustering/community summarization*" as freely composable operators the router can stack into any plan (`psd §II.B`, `§IV.C` "Integrate / Commodity graph algorithms"); the only validator is scoped to plan stability (`psd §II.E`).
- **FathomDB reality:** **cross-wire lossiness** — the summarize/map-reduce step that **helps F4** (sensemaking) **hurts F1/F2** needle (discards the exact needle); measured **blind distiller −0.362**. "*The router must not cross these wires*"; "**router-isolation, not a stack**" (`tree §3` lossiness note, `§3.5` map-reduce-C × precision-rerank row).
- **Fix:** Add an explicit **router-isolation rule**: summarization/map-reduce is valid only for sensemaking-class (F4) intents and must never sit on a needle/factoid (F1/F2) path. Encode the −0.362 cross-wire as a forbidden composition in the validator (extend §II.E from stability-only to lossiness-aware). This is also priced into EXP-Fr-acc's asymmetric mis-route matrix.

**M-E — Intent taxonomy is six mechanism-flavored classes; FathomDB's is five job-to-be-done features.**
- **PSD:** six classes "*temporal, graph-relational, lexical-exact, semantic, exploratory, multi-hop*" (`psd §II.A`, `§III.A` with a self-flagged "[TBD: ratify class count]" and a note the original said five).
- **FathomDB reality:** the taxonomy is **5 features** `{needle/factoid, multi_session/session, temporal, global/sensemaking, multi-hop}`, and the EXP-Fr-acc classifier is exactly `{needle | multi_session | temporal | global | multi-hop}` (`tree §2 F1–F5`, `§4 EXP-Fr-acc`). "graph-relational" rests on the refuted graph arm; "lexical-exact"/"semantic" are sub-mechanisms (BM25/full-text and vector ANN) inside one feature's stack, not top-level classes; "exploratory" maps to global/sensemaking. The PSD is missing **multi_session** (F2, the recall surpass-blocker) and **global/sensemaking** (F4, the GraphRAG-parity fork) — exactly where the open competitor gaps live.
- **Fix:** Adopt the 5-feature taxonomy; resolve the PSD's own [TBD: ratify class count] toward five. Drop graph-relational as a top-level class, merge lexical/semantic into needle/factoid, and add multi_session and global. Reconcile to EXP-Fr-acc's named classifier.

**M-F — Stacking is treated as additive (binary RRF-vs-RRF+CE); it is a constrained joint optimization.**
- **PSD:** Experiment 7 frames "RRF vs RRF+cross-encoder" as a single pass/fail add-on (`psd §III.D`); §II.C presumes composable stacks.
- **FathomDB reality:** recall-expansion × CE-rerank is a **constrained joint optimization, not additive** (the EXP-B′ crux). Measured: **α=1.0 @ pool_n=50 DROPS r@10 0.548→0.498** — CE-confident distractors displace base-favored gold, and deep-recalled gold lands ~rank 15–30, invisible to a pool_n=10 CE. "Recall up then narrow back for free" is false; it requires jointly tuning `candidate_k × pool_n × α × final_K` with order dependencies OD-1..3 (`tree §3.5` row 1, `§4 EXP-B′`).
- **Fix:** Replace the binary experiment with the 3-stage joint-tuning experiment (EXP-B′), honoring OD-2 (pool_n ≥ the depth where recalled gold lands). State explicitly that stacking is a constrained joint optimization, and that the shipped (α=1.0, pool_n=10) is in latent conflict with the F2 recall stack.

**M-G — Gate 0 ("build 50–100 labeled queries from scratch") duplicates existing assets and ignores the corpus cap.**
- **PSD:** "*Build a suite of 50–100 labeled queries spanning all six query classes … each with a gold answer and the gold supporting nodes. … Nothing downstream is measurable without this*" (`psd §III.A`).
- **FathomDB reality:** the eval substrate is **already rich** — LME, LOCOMO, AP-News, MuSiQue (4,834 answerable) — with frozen, power-guarded decision rules `decide_083`/`decide_084` already built (`tree §5`). Critically, registration is **corpus-capped**: M6 sensemaking is capped at N=200 (the AP-News maximum) and needs ~269 entity-rich questions; question-clustered bootstrap means **more runs cannot tighten MDE — only more questions can** (`tree §4 EXP-D` "corpus-acquisition, not a re-run"; `084` superseded banner "N=200 is the corpus maximum"). A generic 50–100-query set neither solves the cap nor uses the existing labeled assets.
- **Fix:** Reuse the existing corpora + `decide_083`/`decide_084` rules rather than greenfielding. Where new questions *are* needed (M6 sensemaking), scope it as the corpus-cap problem (~269 entity-rich AutoQ-style Q), not generic query labeling. *Legitimate kernel to preserve:* the PSD's "gold supporting **nodes**" are FathomDB-node-level retrieval labels the answer-oriented competitor substrate may not fully cover — so a small per-node provenance-labeling pass over the reused corpora is genuine new work, far smaller than a fresh golden set.

**M-H — F4 community/clustering summarization labeled "commodity integrate"; it is the live, split GraphRAG-parity battleground.**
- **PSD:** "*Clustering / community summary | Yes | Integrate | Commodity graph algorithms*" and "*Map-reduce QFS | Partial | Integrate*" (`psd §IV.C`) — implicitly a solved sensemaking win.
- **FathomDB reality:** F4 is a **split verdict, not a commodity**: **C** (map-reduce QFS) is a **provisional SURPASS** vs full-strength Microsoft GraphRAG 3.1.0 (comp .72/div .61/emp .72) but **expensive** (LLM tier, reads everything); **D2** (cheap coverage index) is **below parity**. Both `decide_084 = NOT_REACHED` on power (corpus-capped at N=200). The earlier "GraphRAG wins decisively" verdict is **SUPERSEDED** (see the `084` banner). A hand-built community-summary reimplementation lost decisively; only the real Microsoft Leiden-community pipeline won, and FathomDB's entity/Leiden equivalent is unbuilt and HITL-gated (Fork E, EXP-E).
- **Fix:** Reclassify as an open, expensive product fork (EXP-C/D, Fork-E HITL-gated) with an LLM cost tier and a footprint implication — not commodity integration. Surface C's cost tier, keep it router-isolated from needle paths (M-D), and do not cite the superseded "GraphRAG wins decisively" framing.

**M-I — EXP-OBS (the observability build) is omitted from the ladder, and provenance is sequenced after the router.**
- **PSD:** observability appears only as packaging (`psd §II.D`) and logging (`psd §V.E`); the ladder has no node that *constructs* the retrieval-EXPLAIN surface, and Experiment 10 (provenance value) sits last, after the planner/router is built.
- **FathomDB reality:** **EXP-OBS** is a declared node — build the retrieval-EXPLAIN surface (per-arm provenance + score breakdown + opt-in `explain=True`) — and its sequencing is explicit: it rides alongside EXP-A/B′, **before** EXP-Fr, because "*you cannot ship a transparent router without the transparency surface*" (`arch §6/§8`). Observability is doubly load-bearing (ground truth #17).
- **Fix:** Add EXP-OBS to the ladder as an explicit prerequisite node sequenced **before** the router (Fr), riding alongside the recall/joint-tuning work. State that OBS is both transparency and the substrate for any agent-feedback/reward signal.

**M-J — The closed-loop re-plan is driven by an internal CE signal only; the agent relevance signal is omitted.** *(Detailed in the Agent-feedback subsection below.)*
- **PSD:** "*When a mid-execution signal (e.g. cross-encoder confidence below [TBD: threshold]) indicates a low-quality path, the executor halts and asks the planner for a revised DAG*" (`psd §II.C`). No exogenous/agent signal source.
- **FathomDB reality:** internal CE confidence is gameable — α=1.0 @ pool_n=50 drops r@10 0.548→0.498, and deep-recalled gold at rank ~15–30 is invisible to a pool_n=10 CE (`tree §3.5` row 1). HITL 2026-06-26 (ground truth #17) introduced the **agent as a relevance-signal partner**: the agent holds the intent/goal-graph FathomDB cannot see and gives a real exogenous relevance judgment that can confirm/override/pre-empt the internal CE signal.
- **Fix:** See the Agent-feedback subsection.

**M-K — Router *locus* is assumed (single local shim), not posed as the declared decision FathomDB has opened.**
- **PSD:** "*runs locally alongside FathomDB*" (`psd §V.B`); a single-shim locus is assumed throughout; no node treats locus as open.
- **FathomDB reality:** router **locus** (agent-side / in-library / both-layered) is an explicit **declared decision** with different determinism/latency/testability, owned by EXP-Fr-acc and listed as steward open-decision #2 — "*decided as part of EXP-Fr-acc, not by default*"; recommended resolution is **both-layered** (`arch §5.7`, `§9.2`, `§8`).
- **Fix:** Add router-locus as an explicit open decision routed through EXP-Fr-acc, with the recommended both-layered resolution, tied to the EXP-S outcome and the determinism/testability trade-off.

**M-L — FathomDB is treated as a fixed substrate with coexisting graph/coverage/leaf indexes; that substrate is partially realized (EXP-S).**
- **PSD:** FathomDB internals are "*a fixed substrate the shim orchestrates*" (`psd §I.A`), and the happy path/operator tables assume coexisting graph/coverage structures exist to compose.
- **FathomDB reality:** schema row-`kind` = **document-type only today** (email/article/paper); leaf-vs-coverage-vs-graph row-kinds are a **future engine step (EXP-S)**, and the D2 coverage index lives **eval-side** for now — "architecturally intended, partially realized" (`tree §1`, `§4 EXP-S`; `arch §8/§10`). EXP-S has an explicit **KILL path**: if determinism/perf breaks, the router stays agent-side and indexes stay eval-side.
- **Fix:** Mark the coexisting-index substrate as partially-realized (EXP-S, with a KILL path), not a fixed given. Make the router design contingent on EXP-S landing; don't assume graph/coverage indexes are queryable substrate today. (The *existing* primitives — 1-bit quant ANN, FTS5/BM25, RRF, `Engine.embed` — are shipped and stable; the gap is specifically the kind-tagged coexisting-index substrate.)

**M-M — The fresh 10-experiment ladder does not reconcile with the existing experiment tree.**
- **PSD:** a greenfield ladder (Experiments 1,3,4,5,6,7,8,9,10) gated behind a new Gate 0/Gate 2, with no reference to FathomDB's existing nodes (`psd §III.D`, `§VI`).
- **FathomDB reality:** an existing tree the ladder must reconcile with — EXP-0 (landed) → (EXP-A recall ‖ EXP-M4 embedder ‖ EXP-S substrate) → EXP-B′ (3-stage joint tuning) → register-or-diverge → EXP-Fr-acc (router accuracy + asymmetric mis-route cost) → EXP-Fr (build dispatcher), plus EXP-C/D/E, EXP-F0, EXP-OPP2, EXP-OBS (`tree §4`; `arch §8`).
- **Fix:** Map each PSD experiment onto the existing tree (see the Reconciliation map). Flag duplication of landed work (Exp 7 / EXP-0) and where it genuinely adds (the closed-loop/VoI re-plan node). Treat EXP-OBS as the prerequisite the PSD's provenance/Experiment-10 work depends on.

**M-N — The dynamic planner is partly justified by an implied graph-routing multi-hop win, which is refuted.**
- **PSD:** the static hybrid is "*Weak on multi-hop reasoning and graph-relational queries*" (`psd §IV.A`), used to motivate the dynamic planner (implying routing to graph recovers them).
- **FathomDB reality:** FathomDB **measured** that routing to the graph arm does **not** recover multi-hop — on MuSiQue the graph/ppr_fusion arm loses to fused-RRF (ΔF1 −0.0405, decisive NO-GO) and adds ~0 recall; HippoRAG-2 is unmeasured (`tree §2 F5`, `§3` matrix, `§5 M7`).
- **Fix:** Don't justify the dynamic planner via a graph-routing multi-hop win — that path is refuted. If multi-hop is pursued, scope it as EXP-E (stand up HippoRAG-2, then possibly Fork-E entity/Leiden), not as a graph-BFS routing payoff.

**M-O — PII/jailbreak intent screening is placed at the front of the planner; intent screening is the agent's.**
- **PSD:** "*PII / jailbreak gating at the front of the planner — intents screened before any planning*" (`psd §V.D`).
- **FathomDB reality:** the agent owns intent; intent/jailbreak screening is Memex's responsibility, not a planner-front authority gate — front-gating intent presumes FathomDB owns intent, contradicting "mechanism in Fathom, judgment in the agent" (`arch §0/§1`). The legitimately FathomDB-side concern is the **provenance-leakage** check (don't return content the caller shouldn't see), which the PSD also names. A heavy front classifier on the query path also compounds the footprint concern (M-P).
- **Fix:** Move PII/jailbreak/intent screening to the agent boundary. Keep provenance-leakage control inside FathomDB (mechanism). Don't position FathomDB (or a shim in front of it) as the intent-policing authority.

**M-P — A planner LLM on the query path is treated as local-and-free; it crosses the $0/local boundary if in-library.**
- **PSD:** "*runs locally alongside FathomDB. [TBD: VRAM/CPU budget for planner LLM … confirm the planner can co-reside]*" (`psd §V.B`); "[TBD: is the planner an LLM?]" (`psd §II.E`).
- **FathomDB reality:** the **library query path stays $0/local** (the footprint invariant); LLM spend is eval-side/airlock or **USER-controlled opt-in** (`084 §6` "*footprint invariant intact … never the library query path*"; MEMORY function-over-footprint). A co-residing planner LLM on the *in-library* query path crosses that boundary and must be declared, not silently assumed free.
- **Fix:** Make the boundary crossing explicit. Resolve "[TBD: is the planner an LLM?]" against the footprint invariant. Keep a **no-LLM / distilled-planner default** that preserves the $0 query path; if a planner LLM runs in-library, surface it as a **USER-controlled opt-in cost tier the agent can veto**. *Calibration:* as the PSD currently frames the planner as a shim *above* FathomDB (`psd §I.A`), a shim-side planner is the agent's own spend and honors the boundary — the crossing bites specifically for FathomDB's own optional in-library L2 dispatcher, so tie this to the router-locus decision (M-K).

### Minor

**Minor-a — Cost/efficacy asymmetry: efficacy-is-hard is correct; the cost side needs a small reframe.**
- **PSD:** "*the shim can estimate cost (latency, tokens) before running, but it cannot reliably estimate efficacy … That asymmetry is the project's spine*" (`psd §II`).
- **FathomDB reality:** the asymmetry is sound and FathomDB agrees efficacy is the hard axis. The nuance: per-arm **latency/cost tiers are a measured first-class OUTPUT of EXP-A/B′/C** (CPU/GPU/local-LLM/net-LLM), so cost is estimable *because it was measured*, not free a priori (`tree §4` cross-cutting; `arch §5.6`).
- **Fix:** Keep efficacy-is-hard as the spine. Note cost tiers are measured per arm and should be surfaced (so a cost-aware agent can veto an expensive route). Reconcile the Gate-2 oracle bound with FathomDB's measured per-feature numbers rather than presenting cost as the easy/free side.

**Minor-b — Provenance framed as graph node/edge traversal; recast as per-hit arm-provenance + score breakdown.**
- **PSD:** per-result provenance expressed as "*nodes traversed, edges used*" (`psd §I.B/§II.D`).
- **FathomDB reality:** this conflates two things — graph traversal is the refuted, default-off arm, and `TraceReport`/`source_id` are write-lineage, not retrieval explainability. The needed retrieval-EXPLAIN is per-hit arm-provenance (vector-ANN / FTS-BM25 / graph) + score breakdown (`rrf_norm`, `ce_score`, blended), which mostly does not exist yet (`arch §3/§6`).
- **Fix:** Recast provenance as per-hit **arm-provenance + score breakdown + executed plan**, drawn from EXP-OBS — not graph traversal. Keep write-lineage (`source_id`) as a separate, already-shipped field distinct from retrieval explainability.

**Minor-c — "RRF can dilute a single strong signal" is technically correct but mis-aimed as motivation.**
- **PSD:** "*RRF can dilute a single strong signal*" (`psd §IV.A`).
- **FathomDB reality:** true of rank-based RRF (it is score-magnitude-agnostic), but fused-RRF is FathomDB's measured strong baseline (it beat the graph arm). The live signal-mixing risk is the inverse: the **C6 guard** (default α=0.3) exists so a high-CE-wrong "strong signal" does not displace a BM25-correct factoid, and the real composition hazard is the recall×CE joint-opt (M-F) (`tree §5-ce`, `§3.5` row 1).
- **Fix:** Keep the note but reframe to the C6 displacement guard and the α/pool_n joint-tune. Don't use "RRF dilutes" to motivate the dynamic planner over the fused baseline.

**Minor-d — Temporal valid-time filter is a correct strength; guard recency against content-rewrite.**
- **PSD:** "*temporal filter*" as a pre-retrieval operator (`psd §I.C`).
- **FathomDB reality:** valid-time filtering is shipped `[S]` and composes in order (expand → filter → rerank, OD-4). But F3's recency **provider (OPP-2) is unbuilt**, and recency must be a **post-retrieval rerank weight, never a pre-retrieval content rewrite** (a rewrite invokes the −0.362 lossiness penalty / can delete a wanted fact; OD-6) (`tree §2 F3`, `§3.5`).
- **Fix:** Keep the pre-retrieval valid-time filter. Add the constraint that recency ranking (OPP-2/EXP-OPP2) must be a post-retrieval rerank weight, never a content rewrite — a forward-looking guard so the planner doesn't conflate temporal filtering with lossy consolidation.

**Minor-e — Latency SLOs float free of FathomDB's existing gates and cost tiers.**
- **PSD:** "*Planner overhead p95 ≤ 600–800 ms; total retrieval p95 ≤ 3000 ms; fast-path p95 ≤ 1000 ms*" (`psd §V.A`, provisional).
- **FathomDB reality:** FathomDB has substrate latency/concurrency gates **AC012/013/020** and per-arm cost/latency tiers as measured outputs of EXP-A/B′/C (`tree §3` matrix F0/M8/M9, `§4`; `arch §5.6`).
- **Fix:** Anchor the total-retrieval SLO to AC012/013/020 and express budgets in the measured per-arm cost tiers (map planner-overhead to the shim layer separately). Keep the numbers flagged *Provisional* / [TBD: ratify] until reconciled.

**Minor-f — Plan determinism is framed as a variance target; for the substrate it is a locus-deciding KILL gate.**
- **PSD:** "*Variance target [TBD], measured by Experiment 8*" (`psd §II.E/§III.D`).
- **FathomDB reality:** EXP-S has an explicit KILL path keyed on determinism: "*determinism/perf breaks → router stays AGENT-side only, indexes stay eval-side*" (`tree §4 EXP-S`; `arch §8`). *Calibration:* these are two distinct axes — the PSD's Experiment 8 is **plan-output** stability (a property of an LLM planner), whereas EXP-S's KILL gate is **substrate-write** determinism. The PSD has no substrate-coexistence experiment and presumes an agent-side shim, so its plan-variance number does not itself flip the locus.
- **Fix:** Keep the plan DSL + validator + memoization as the plan-determinism mechanism, but distinguish plan-output from substrate-write determinism, and reconcile Experiment 8 with EXP-S's substrate KILL criterion and EXP-Fr-acc's locus decision — so a determinism failure is recognized as potentially go/no-go on *where* the router/indexes live, not merely a variance metric.

---

## Agent-feedback loop — the one genuinely new concept the PSD omits

**The omission (M-J), framed as a FathomDB-aligned extension.** The PSD's closed-loop re-plan is driven by a single *internal* signal — cross-encoder confidence below a threshold (`psd §II.C`). FathomDB's measured reality shows that internal CE confidence is both gameable and blind: α=1.0 @ pool_n=50 drops r@10 0.548→0.498 because CE-confident distractors displace base-favored gold, and deep-recalled gold at rank ~15–30 is invisible to a pool_n=10 CE (`tree §3.5` row 1). The HITL direction of 2026-06-26 (ground truth #17) makes the agent a **relevance-signal partner in route planning**: FathomDB owns the *mechanism* (a good default route + honest observability, and **must not abdicate** getting the right data), while the agent (Memex) holds the **intent / goal-graph context FathomDB structurally cannot see** (`arch §1`) and can return a real exogenous relevance judgment on whether the returned data — and the telemetry, when requested — served the query. That signal is admitted as a **second, higher-quality input** that can confirm, override, or pre-empt the internal CE judgment. This is "mechanism in Fathom, judgment in the agent" (`arch §0`) applied to the closed loop — it **extends** the PSD's internal-only re-plan, it does not replace it.

**Grounded in CS / IS / Math (not hand-waved):**
- **IR relevance feedback (Rocchio).** An agent supplying online relevance labels is a *real* relevance judge — the principled **superset of pseudo-relevance feedback (PRF)**. This **removes the circularity** FathomDB already flagged: PRF × CE-rerank is a `CONFLICT-risk` because "PRF presumes the precision the rerank adds" (`tree §3.5`). A real exogenous judgment comes from outside the retrieved pool (the agent's goal graph), so it does not bootstrap relevance from the pool whose precision is in question. Model agent feedback as Rocchio-style, explicitly distinguished from any internal top-k assumption.
- **Value of Information (VoI) / decision theory.** Asking the agent costs a round-trip; "ask vs decide internally from `ce_score`" is a VoI tradeoff — the same break-even discipline the PSD already applies to re-plan depth and to planning-vs-direct-frontier-call (`psd §II.C/§V.C`). It honors the $0 boundary: the agent call is the *agent's* spend, not the in-library path. The asymmetric mis-route cost is already measured (needle → C map-reduce = −0.362 + an LLM call; `tree §3.3/§4`), so high mis-route cost → high VoI of asking. Make the ask an explicit policy: don't ask when `ce_score` is high and the arm is cheap/non-lossy; **do** ask near a route-confidence boundary or when the candidate arm is high-cost/lossy (C/D2).
- **Contextual bandits / RL — the missing reward signal.** Route selection over the L1 arms is explore–exploit; a bandit/RL learner needs a reward signal. The PSD (and `arch §8`) defer learned routing precisely until "logs + a golden set + a reward signal" exist — and **the agent relevance signal is exactly that missing reward signal**. Log `(context, chosen_arm, agent_relevance_signal, ce_score)` now; learn from it later; keep the deferral ADR intact but with a concrete path to un-defer.
- **Active learning.** Agent labels are scarce, costly supervision; spend the budget where uncertainty is highest — low `ce_score`, near the classifier boundary. `ce_score` (EXP-0) is the natural uncertainty signal that decides *where* to ask, bounding round-trip spend and protecting the latency SLO.

**Tie to the experiment tree (insertions/extensions only — do not greenfield):**
- **Extend EXP-Fr-acc** with a sub-measurement (EXP-Fr-acc.6): does admitting an agent relevance signal reduce mis-routes over `ce_score`-only routing, and at what round-trip cost? Derive the VoI break-even and the uncertainty-sampling targeting. Mis-route cost uses the existing asymmetric matrix.
- **EXP-OBS is the prerequisite + reward-signal logger.** The agent cannot judge *the route* (vs merely the content) without per-arm provenance + score breakdown + executed plan — so observability is **doubly load-bearing** and EXP-OBS gates the value of any agent-feedback work. EXP-OBS also persists the reward stream that un-defers learned routing. Sequencing unchanged: EXP-OBS rides alongside EXP-A/B′, before EXP-Fr.
- **Optional EXP-AF (agent-feedback) node**, placed on the EXP-Fr-acc → EXP-Fr edge: does an agent relevance signal improve route quality over `ce_score` alone, at what round-trip cost, one-shot vs iterative, does it cut mis-routes? **KILL path:** if the agent signal does not beat `ce_score` net of round-trip cost, keep it as an overridable L1 opt-in, not the default loop. *Crucial coupling:* any signal-triggered re-plan acts on the **EXP-B′ tuple** `(candidate_k × pool_n × α × final_K)` jointly (honoring the joint-opt crux and the EXP-B′.5 cross-feature guard) — an "this was irrelevant → widen recall" reflex can *worsen* precision if applied as a free additive widen.

---

## Reconciliation map

How the PSD's gates and experiments map onto FathomDB's existing experiment tree. Verdict: **duplicate** (already done / exists), **contradicts** (wrong comparator or framing), **genuine new gap** (a real addition the tree should absorb).

| PSD element | Existing FathomDB node(s) | Verdict |
|---|---|---|
| **Gate 0** — build 50–100 labeled queries | LME/LOCOMO/AP-News/MuSiQue + `decide_083`/`decide_084`; corpus-cap = EXP-D | **duplicate** (eval substrate already rich) + new gap is only the ~269-Q entity-rich set EXP-D needs + a thin gold-supporting-node labeling pass |
| **Gate 2** — oracle-routing upper bound | oracle ceilings (EXP-A recall@K_deep) + parity gates (`decide_083/084`) | **keep**; comparator **contradicts** (15–20% over internal fixed baseline vs competitor parity) |
| **Exp 1** — per-operator characterization | EXP-A + the cross-cutting per-arm cost/latency envelope | **duplicate** |
| **Exp 2** — (the Gate-2 oracle run) | EXP-A oracle ceiling | **duplicate / keep** |
| **Exp 3** — intent/complexity classifier accuracy | EXP-Fr-acc | **duplicate** |
| **Exp 4/5** — routing / fast-path efficacy | EXP-Fr-acc (+ EXP-A/B′) | **duplicate** |
| **Exp 6** — upfront vs closed-loop re-plan | EXP-Fr-acc → **EXP-AF** (agent-feedback) | **genuine new gap** (the agent relevance signal / VoI loop) |
| **Exp 7** — RRF vs RRF+cross-encoder | **EXP-0 (LANDED)** + EXP-B′ | **duplicate** (landed) + **contradicts** (binary vs constrained joint-opt) |
| **Exp 8** — planner determinism/stability | EXP-S KILL path (substrate determinism) | partial overlap; distinct axis (plan-output vs substrate-write) |
| **Exp 9** — token break-even (planning vs frontier) | cost tiers (EXP-A/B′/C) + VoI ask-policy | **keep / reconcile** |
| **Exp 10** — provenance → agent self-correction | **EXP-OBS** (prerequisite) + EXP-Fr-acc/**EXP-AF** | **genuine new gap** — elevate, don't bury at #10 |
| *(missing)* observability build | **EXP-OBS** | **genuine new gap** (omitted; it is the prerequisite for a transparent router) |
| *(missing)* recall expansion | EXP-A | gap (not referenced) |
| *(missing)* embedder | EXP-M4 | gap |
| *(missing)* coexisting-index substrate | EXP-S | gap (assumed fixed) |
| *(missing)* 3-stage joint tuning | EXP-B′ / EXP-B′.5 | gap (stacking assumed additive) |
| *(missing)* F4/F5 + corpus forks | EXP-C / EXP-D / EXP-E | gap (F4 assumed commodity) |
| *(missing)* fidelity / recency | EXP-F0 / EXP-OPP2 | gap |

---

## Open questions for the steward

1. **Authority model:** ratify reframing the shim from "decides what to call" to a transparent, overridable default router (query-planner analogy) with the agent as final intent authority (C1, M-B)? This reshapes §I.A and the gate framing.
2. **Hybrid closed loop:** adopt the internal-CE default + VoI-gated agent ask as the PSD's §II.C, superseding internal-only (M-J)? Recommended pending EXP-AF.
3. **Gate comparator:** replace "beat fixed baseline by 15–20%" with competitor-parity (`decide_083`/`decide_084`, MDE ≤ 0.05, ε=0.05), keeping the oracle bound as a diagnostic against the parity band (C4)?
4. **Router locus:** route the agent-side / in-library / both-layered decision through EXP-Fr-acc (recommended both-layered), tied to EXP-S's KILL path (M-K)?
5. **Planner-LLM footprint:** confirm a no-LLM/distilled default preserves the $0/local query path, and that any in-library planner LLM is a declared USER-controlled opt-in cost tier, not a silent local default (M-P)?
6. **Agent-signal contract shape `[TBD]`:** scalar relevance score, per-hit label vector (Rocchio-style), arm-level "wrong route" flag, or free-text rationale? Each implies a different re-plan action and telemetry payload.
7. **Ask locus `[TBD]`:** is the agent ask a provider-callback (judgment stays in Memex, mechanism in Fathom) or agent-side? Confirm alongside the router-locus decision.
8. **Reward-log retention / privacy `[TBD]`:** the reward stream contains query-context — reconcile with the PSD's data-classification `[TBD]` (`psd §V.D`) and the $0/local boundary.
9. **Un-defer trigger `[TBD]`:** when (if ever) to un-defer learned routing (bandits/RL) from the logged reward stream — keep the deferral until the reward stream is validated, and define the trigger.
10. **EXP-AF placement `[TBD]`:** does EXP-AF gate EXP-Fr, or ride parallel? Recommended on the EXP-Fr-acc → EXP-Fr edge — steward to ratify.

*Provisional throughout:* this audit is a position for review. Every efficacy/cost claim about the agent signal is unmeasured until EXP-OBS + EXP-Fr-acc.6 + EXP-AF return data; all FathomDB-substrate facts are cited to in-repo measured/decided sources (`psd`/`arch`/`tree`/`084`).
