# Question-Decomposition Quality for Multi-Hop Retrieval/QA — State of the Art

**Audience:** a Memex agent working on the OPP-1 iterative decomposer. This doc is self-contained; no prior FathomDB/Memex context is assumed.
**Purpose:** answer, with real citations, whether "a decomposer must be more than just more LLM calls," and translate the literature into concrete build guidance.
**Status:** informational research digest (no code, no experiments run). Compiled 2026-06-30.

---

## 0. Why this doc exists (our concrete measurement)

We are trying to improve multi-hop retrieval by decomposing a complex question into sub-questions and solving them in sequence. The motivating measurement, on **MuSiQue** (2,417 questions):

- An **ORACLE decomposition** — gold sub-questions *with gold intermediate answers substituted into later sub-queries* — lifts supporting-fact **recall@5 by +0.28 to +0.34** over single-shot retrieval.
- The lift **grows with hop depth**: 2-hop **+0.256** → 4-hop **+0.344**.
- A **NAIVE flash-lite decomposer captured ~0%** of that available lift.

**Diagnosis we brought to the literature.** The oracle's advantage is not "it wrote nice sub-questions." It is that it substitutes *correct intermediate answers* into downstream sub-queries — i.e., it has already solved every earlier hop perfectly. A realistic decomposer must actually **solve each hop**, and hop errors **compound geometrically with depth**. That is exactly why the lift grows with hops and why a naive splitter gets ~0%: it emits sub-questions but never grounds/solves them, so it recreates none of the oracle's edge.

**The user's thesis, restated as a research question:** *is a good decomposer "more than just more LLM calls"?* The literature answer, developed below, is an emphatic **yes** — but the differentiator is the **structure** of the calls (interleaved with retrieval, grounded in evidence, verified per hop, adaptively gated, and error-recovering), **not their count**. Naively adding LLM calls reproduces the ~0% result; the SOTA methods all change the *control loop*, not the call volume.

---

## 1. Executive summary

1. **The winning pattern is interleaved solve-and-retrieve, not upfront full decomposition.** The single most robust finding (IRCoT, self-ask, ReAct, CoRAG) is that *what to retrieve next depends on what has already been derived*. Systems that alternate retrieve→reason→retrieve beat systems that decompose once and then answer. This is the biggest lever and directly reconstructs the oracle's mechanism (feed the solved sub-answer forward).

2. **Compounding error is real, documented, and geometric.** If per-hop success is `p` and there are `k` hops, naive sequential accuracy scales like `p^k`. This matches our depth-growing oracle gap. Mitigations that work: ground each hop in retrieved evidence, verify/self-consistency-check intermediate answers, confidence-gate the stop/continue decision, best-of-N sample the chain, and backtrack/re-plan on failure.

3. **A "compositionality gap" persists regardless of model size.** GPT-3-class models answer sub-questions correctly but fail to compose them ~40% of the time, and this gap **does not shrink with scale** (Press et al. 2023). So you cannot buy your way out with a bigger reader — the *harness* is the fix. Structured prompting (self-ask) plus a retriever narrows/closes it.

4. **Harness > model, within reason.** Multiple 2024–2026 results show mid-size/small models with a strong multi-step harness matching or beating much larger models with naive prompting. There is a floor (very tiny models can't leverage scaffolds), but above it the decomposition strategy dominates raw parameter count.

5. **Not every question should be decomposed.** Single-hop and "already-answerable" questions are *hurt* by forced decomposition (added error surface, latency, verbosity). SOTA systems gate decomposition adaptively (ADaPT "as-needed"; adaptive/active retrieval; sufficiency critics).

6. **MuSiQue was purpose-built to punish exactly the shortcut a naive decomposer takes.** Trivedi et al. (2022) constructed it bottom-up from *connected* single-hop pairs and minimized "disconnected reasoning," so you can only win by genuinely executing every hop. A splitter that doesn't solve-and-feed-forward will always score near single-shot here — which is what we observed.

---

## 2. Technique survey (canonical + 2024–2026 SOTA)

Each entry: core idea + the one thing it adds over "split then answer."

### Foundational decomposition / reasoning

| Method | Core idea | What it adds over naive split-then-answer |
|---|---|---|
| **QDMR / Break** (Wolfson et al. 2020) | A formal *Question Decomposition Meaning Representation*: an ordered list of NL steps (select / project / aggregate operators) that execute in sequence. 83,978 annotated questions. | Gives decomposition a **typed, executable structure** rather than free-text splitting; the operator sequence is checkable and improves open-domain multi-hop QA (HotpotQA). A schema to imitate/validate against. |
| **Least-to-Most** (Zhou et al. 2022) | Reduce a hard problem to an ordered list of subproblems, then solve them sequentially, **feeding each answer into the next subproblem's prompt**. | The **feed-forward of solved sub-answers** — precisely the oracle mechanism. On SCAN it hits ≥99% vs 16% for CoT. Establishes that *sequencing + answer substitution*, not just splitting, is what generalizes to harder compositions. |
| **Self-Ask** (Press et al. 2023) | Model explicitly asks itself follow-up questions and answers them before the final answer; structure makes it trivial to route each follow-up to a **search engine**. | Names and measures the **compositionality gap (~40%, scale-invariant)**; shows structured self-questioning + retrieval narrows it. +11 pts over CoT on Bamboogle (57.6 vs 46.4). |
| **Decomposed Prompting / DecomP** (Khot et al. 2023) | A *decomposer* LLM breaks the task into sub-tasks dispatched to a **shared library of specialized sub-task handlers** (each optimizable, further-decomposable, or replaceable by a retriever/symbolic tool). | **Modularity + recursion + tool delegation.** Each hop can use a handler tuned for that hop (e.g., a retrieval module), rather than one generic prompt doing everything. |

### Interleaved retrieval + reasoning (the core family for us)

| Method | Core idea | What it adds |
|---|---|---|
| **IRCoT** (Trivedi et al. 2023, ACL) | **Interleave** retrieval with chain-of-thought: each CoT sentence guides the next retrieval, and each retrieval improves the next CoT step. | The canonical statement that **one-shot retrieve-then-read is insufficient** for multi-hop because "what to retrieve depends on what was already derived." Up to **+21 retrieval pts / +15 QA pts** on HotpotQA/2Wiki/MuSiQue/IIRC; was SOTA on MuSiQue. **This is the template our OPP-1 loop should follow.** |
| **ReAct** (Yao et al. 2022) | Interleave free-form *reasoning traces* with *actions* (e.g., search API calls); reasoning updates the plan, actions ground it in external evidence. | Explicitly **reduces hallucination and error propagation** vs pure CoT by grounding each step in a retrieved observation. Trajectories are inspectable/debuggable. |
| **FLARE** (Jiang et al. 2023, EMNLP) | *Active* retrieval: generate a draft of the next sentence; if it contains **low-confidence tokens**, use it as a query to retrieve, then regenerate. | **Confidence-triggered retrieval** — retrieve only when the model signals it doesn't know, and retrieve for *what it's about to say next*. A cheap when-to-retrieve gate. |
| **Self-RAG** (Asai et al. 2023) | Train the model to emit **reflection tokens** that decide *when to retrieve*, and to **critique** retrieved passages and its own output for relevance/support. | **Learned adaptive retrieval + self-critique of grounding** — the model gates retrieval and verifies its own sub-answers, instead of blindly retrieving every hop. |
| **Chain-of-Note** (Yu et al. 2023) | Generate sequential **reading notes** for each retrieved doc, assessing relevance/reliability before answering; can say "unknown." | **Robustness to noisy/irrelevant retrieval** and to unanswerable hops — filters bad evidence *before* it corrupts the downstream hop. |

### Adaptive / recursive planning

| Method | Core idea | What it adds |
|---|---|---|
| **ADaPT** (Prasad et al. 2024, NAACL Findings) | **As-needed** recursive decomposition: try to execute; only if the executor LLM *fails* does the planner decompose further, recursing to match task complexity **and** model capability. | **Decompose only when needed**, and **recurse on failure** = built-in error recovery + adaptivity. +28.3% (ALFWorld), +27% (WebShop), +33% (TextCraft) over strong baselines. Directly answers "not all questions benefit." |
| **CoRAG** (Wang et al., Microsoft, 2025) | Train an "o1-like" RAG model that decomposes into sub-questions and **iteratively retrieves**, with **dynamic query reformulation**; use **rejection sampling** to synthesize intermediate retrieval chains for training, and **best-of-N test-time scaling** over chains. | **Learned iterative retrieval + test-time compute scaling + chain rejection-sampling.** With Llama-3.1-**8B** it beats a **32B** Search-o1 baseline on MuSiQue by **+14.3 EM**. Strong evidence for harness-over-model and for best-of-N as error mitigation. |

### 2024–2026 supporting / adjacent work worth knowing

- **GenDec** (2024): generative question-decomposition method targeting **robustness** of the decomposition itself.
- **GRITHopper** (2025): **decomposition-free** multi-hop dense retrieval — a useful counterpoint; sometimes a strong multi-hop retriever obviates explicit decomposition. Consider as a baseline/fallback.
- **POQD** (2025): **Performance-Oriented Query Decomposer** — decompose to optimize downstream retrieval metrics, not to look linguistically clean. Reinforces "optimize the decomposition for retrieval outcome, not prose."
- **"Question Decomposition for RAG"** (2025, arXiv 2507.00355): direct study of decomposition specifically for RAG pipelines.
- **"Mitigating Lost-in-Retrieval"** (2025): recovering when an intermediate hop's retrieval derails the chain — a named failure mode + fix.
- **Four-Axis survey** (2026, arXiv 2601.00536): organizes the whole design space along (A) execution plan, (B) index structure, (C) next-step control, (D) **stop/continue criteria** — a useful checklist for building OPP-1.

---

## 3. The compounding-error problem and what mitigates it

### It's documented and it's geometric

Multiple sources state it plainly: in the decompose-then-answer (QD+QA) paradigm, "errors tend to propagate across steps, compounding inconsistencies," and "inaccuracies in resolving any sub-question can misguide the final answer." If per-hop success probability is `p` and hops are independent, end-to-end success `≈ p^k` for `k` hops. Even at a strong `p = 0.85`: 2-hop ≈ 0.72, 3-hop ≈ 0.61, 4-hop ≈ 0.52 — **matching our observation that the oracle gap (and thus the difficulty of realistically closing it) grows with depth (2-hop +0.256 → 4-hop +0.344).** The oracle has `p = 1.0` per hop by construction; a naive decomposer has a low `p` that gets raised to the k-th power, which is why it captured ~0%.

The self-ask "compositionality gap" is the same phenomenon isolated: models that answer each sub-question correctly still fail to *compose* them ~40% of the time, and **that gap doesn't shrink with scale**. So depth-robustness has to come from the harness.

### Mitigations that the literature supports

1. **Ground every hop in retrieved evidence (don't let the LLM free-run).** ReAct and IRCoT reduce hallucination/error-propagation specifically by forcing each reasoning step to be conditioned on a fresh retrieval. Bottom-up strategies "resolve and verify sub-problems with retrieved evidence before their results are used" for parent questions.
2. **Feed the *solved and verified* sub-answer forward** into the next sub-query (least-to-most; this is literally the oracle's edge). Rewriting hop *k+1* with hop *k*'s concrete answer entity is what turns a "disconnected" decomposition into a "connected" one.
3. **Verify intermediate answers before proceeding.** Self-consistency (sample the sub-answer multiple times, take the consensus), Self-RAG-style critique tokens, and "dual-verification / PRM-style" triggers all raise per-hop `p`, which pays off geometrically. Chain-of-Note filters unreliable evidence before it enters a hop.
4. **Confidence-gate stop/continue (treat "stop" as first-class).** FLARE retrieves only on low-confidence spans; adaptive controllers with sufficiency critics decide when enough evidence exists. Stopping too early misses hops; too late inflates prompt/latency and adds error surface.
5. **Best-of-N over whole chains + rejection sampling.** CoRAG samples multiple retrieval chains and keeps the best; this consistently beats greedy decoding on MuSiQue (30.9 vs 18.6 EM). Chain-level selection tolerates a bad individual hop.
6. **Backtracking / re-planning on failure.** ADaPT recursively re-decomposes a sub-task the executor couldn't solve; "lost-in-retrieval" recovery re-issues/repairs a derailed hop instead of blindly continuing.
7. **Decompose only when it helps (when-to-decompose gating).** Single-hop or self-answerable questions should skip decomposition entirely — forcing it adds cascading-error surface for no benefit.

---

## 4. Empirical / SOTA table (with sources)

Numbers are as reported by each paper; retrieval corpora, reader models, and open- vs distractor-setting differ, so treat cross-row comparisons as directional, not head-to-head. EM = exact match; F1 = token-F1; all on the answer unless noted.

| System (year) | Setup / model | MuSiQue | HotpotQA | 2WikiMultiHopQA | Note |
|---|---|---|---|---|---|
| **Single-shot / one-step retrieve-then-read** | baseline | very low; MuSiQue built to defeat it | — | — | The regime our naive decomposer effectively sits in. |
| **Self-Ask + search** (Press et al. 2023) | GPT-3 few-shot | 15.2 | — | 40.1 | +search over self-ask: +1.4 (MuSiQue), +10.1 (2Wiki). Small MuSiQue gain shows MuSiQue's hardness. |
| **IRCoT** (Trivedi et al. 2023) | GPT-3, interleaved | SOTA-at-time; **+up to 21 retrieval / +15 QA pts** over one-step | close to SOTA | close to SOTA | The interleaving template. |
| **CoRAG** (Wang et al. 2025) | **Llama-3.1-8B**, L=10, best-of-8 | **30.9 EM / 42.4 F1** | **56.3 EM / 69.8 F1** | **72.5 EM / 77.3 F1** | Beats **Search-o1-32B** (MuSiQue 16.6/28.2) by **+14.3 EM** with a 4× smaller model. |
| **CoRAG test-time scaling** (same) | Llama-3.1-8B | L=1 greedy **18.6 EM** → L=6 **27.7** → L=10 best-of-8 **30.9** | — | — | Direct evidence that *chain length + best-of-N* buys accuracy on the hardest set. |
| **Search-o1-32B** (baseline in CoRAG) | 32B reasoning model | 16.6 EM / 28.2 F1 | — | 58.0 EM / 71.4 F1 | Bigger model, weaker harness → loses to 8B CoRAG. |
| **Beam Retrieval** (reported via survey) | supervised multi-hop retriever | — | large EM gains | **+44.6% EM (53.5→79.3), +20.2 F1** | Structured beam search over evidence chains. |
| **PropRAG (Lmax=3)** (reported) | graph/proposition RAG | — | avg **F1 64.5** vs HippoRAG2 62.9 | — | Index-structure gains, decomposition-light. |
| **Q-DREAM** (2024–25) | **Llama2-7B** | avg **EM 30.7 / F1 37.7** across 3 sets | — | — | Small model + good harness beats larger-backbone InstructRAG/ChatQA2/SURE(ChatGPT). |

**Reading of the table.** (a) MuSiQue remains the hardest — even the best listed open-setting system is ~31 EM, versus 56–79 on HotpotQA/2Wiki. (b) The biggest single jump on MuSiQue comes from **iterative retrieval + test-time chain scaling** (CoRAG), not from a bigger reader. (c) On the easier sets, structured retrieval (Beam Retrieval) and index structure (PropRAG) drive gains — but those are exactly the sets with shallower/less-connected reasoning.

### Why MuSiQue specifically resists naive decomposition (Trivedi et al. 2022)

- Built **bottom-up**: they systematically select *composable pairs of single-hop questions where one hop critically depends on the other's answer* (genuinely **connected** reasoning), then compose 2–4 hops (25K questions).
- Explicitly minimizes **disconnected reasoning** (the DiRe measurement of Trivedi et al. 2020): MuSiQue has ~**3× the human–machine gap** and a **much lower DiRe (cheatability) score** than HotpotQA.
- **Implication for us:** you can only score on MuSiQue by actually executing *every* hop and carrying the intermediate answer forward. A decomposer that emits sub-questions but doesn't solve-and-substitute has no shortcut to exploit — which is precisely why our naive flash-lite captured ~0% and why our *oracle* (which does substitute gold intermediate answers) shows a large, depth-growing lift. MuSiQue is essentially a purpose-built detector for the exact failure our OPP-1 loop must fix.

---

## 5. Model vs. strategy — the verdict

**Verdict: the strategy/harness is the dominant lever above a modest capability floor; raw model size is second-order for this task.**

Evidence:
- **CoRAG's 8B beats Search-o1's 32B on MuSiQue by +14.3 EM** — same task, 4× smaller model, better (iterative, chain-sampled) harness wins.
- **The compositionality gap is scale-invariant (~40%)** (Press et al. 2023): making the reader bigger does *not* close the compose step; only the harness (self-ask + retrieval) does.
- **Multi-step retrieval helps mid/small models most.** Radiology-QA: Mistral-Large 72%→81% with multi-step, while very large models barely move. Q-DREAM (Llama2-7B) beats larger-backbone systems. Instruction-retrieval scaffolds add 5–18 pts to small models.
- **But there is a floor:** "tiny models lack the intrinsic reasoning ability to reliably leverage retrieved scaffolds" — below some capability, scaffolding doesn't stick. So the reader must be *good enough to follow the harness and to solve a single grounded hop reliably*, not maximal.

**Direct answer to the user's thesis.** "A decomposer must be more than just more LLM calls" is **correct and well-supported**. Adding LLM calls without changing the control structure reproduces the naive ~0% result and, on MuSiQue, the compounding-error tax eats the gains. What converts calls into recall is: **interleaving them with retrieval, grounding each in evidence, feeding verified sub-answers forward, gating when to decompose/stop, and selecting over sampled chains.** Choose a *just-right* reader (small/mid is fine, above the floor) and spend the effort budget on the harness.

---

## 6. Prioritized recommendations for Memex's OPP-1 iterative decomposer

Ordered by expected recall payoff per unit of build effort. P0 = do first.

### P0 — Make the loop interleaved solve-and-retrieve, and feed verified sub-answers forward
This is *the* oracle mechanism and the strongest literature consensus (IRCoT, least-to-most, self-ask, CoRAG).
- Do **not** produce a full static decomposition upfront. Solve hop 1 (retrieve → read → extract a concrete sub-answer entity), **then rewrite hop 2's query by substituting that entity**, retrieve again, and so on. "What to retrieve next depends on what was just derived."
- The forward-substitution of a *resolved entity* (not the sub-question text) is what our oracle does and what a naive splitter omits. Instrument it explicitly: log the substituted entity at each hop.
- Expected effect: this alone should recover the bulk of the oracle's depth-growing lift, because it converts a "disconnected" decomposition into a "connected" one — exactly what MuSiQue rewards.

### P0 — Verify each intermediate answer before it propagates
Because errors compound as `p^k`, raising per-hop `p` pays off geometrically.
- Cheapest: **self-consistency** — sample the sub-answer 3–5× and take the consensus; if no consensus, flag low-confidence.
- Add a lightweight **grounding check** (Self-RAG / Chain-of-Note style): does the retrieved evidence actually support the extracted sub-answer? If not, don't carry it forward.
- Gate: a hop that fails verification should trigger re-retrieval/re-plan (see P1), not silent propagation.

### P1 — Add when-to-decompose and when-to-stop gates (adaptivity)
Not all questions benefit; forced decomposition adds error surface and latency.
- **When-to-decompose:** if a single-shot retrieval already returns sufficient supporting facts (a sufficiency critic, or high retriever confidence), answer directly. Reserve decomposition for questions the model/retriever signals it can't resolve in one hop (ADaPT "as-needed"; FLARE confidence trigger).
- **When-to-stop:** treat "stop" as a first-class action with a sufficiency/confidence criterion (Four-Axis Axis D). Cap hops (e.g., ≤ known max depth 4 for MuSiQue) but stop early when evidence is sufficient.

### P1 — Error recovery: backtrack / re-plan, and best-of-N over chains
- **Recursive re-decomposition on failure** (ADaPT): if a hop can't be solved, decompose *that hop* further or reformulate its query rather than aborting the chain.
- **Best-of-N chains** (CoRAG): when budget allows, sample a few full retrieval chains and select the one with the best evidence coverage / self-consistency. CoRAG shows this is where the extra MuSiQue points come from (18.6 → 30.9 EM). This is a controllable test-time-compute knob, not a model swap.
- Guard the **"lost-in-retrieval"** failure mode: if an intermediate retrieval returns off-topic passages, detect (Chain-of-Note relevance notes) and re-issue the query with the resolved entity made explicit.

### P2 — Modularize hops and optimize the decomposition for retrieval, not prose
- **DecomP-style handlers:** let a hop be dispatched to a specialized handler (a retriever call, a date/number extractor, a symbolic step) rather than one generic prompt.
- **POQD framing:** score/select decompositions by *downstream retrieval recall*, not by how clean the sub-questions read. If you can, tune the decomposer against supporting-fact recall directly.
- Consider a **decomposition-free strong multi-hop retriever** (GRITHopper) as a parallel arm/fallback for cases where explicit decomposition underperforms — some multi-hop questions are better served by a retriever trained to chase chains directly.

### P2 — Pick a "just-right" reader, spend the budget on the harness
- The compositionality gap is scale-invariant and an 8B-with-good-harness beat a 32B baseline; do not assume a bigger reader fixes composition. Use a mid-size model that reliably (a) follows the interleaved loop and (b) solves a *single grounded hop* — then invest effort in the loop, verification, gating, and chain selection above.
- Keep the per-hop task small and grounded; that is where modest models are reliable and where `p` (hence `p^k`) is maximized.

### Evaluation guidance
- Report supporting-fact **recall@k broken out by hop depth** (2/3/4-hop) — the oracle gap grows with depth, so a fix must be shown to help deep questions specifically.
- Track **per-hop success `p`** (via the verification signal) as a leading indicator; end-to-end recall should track `~p^k`.
- Keep the **oracle (gold sub-answers substituted)** as the ceiling and single-shot as the floor; report % of the oracle gap closed, by depth. Closing the gap on 4-hop is the hard, decisive test — and the one MuSiQue was designed to make honest.

---

## 7. Sources

**Datasets / problem framing**
- Trivedi, Balasubramanian, Khot, Sabharwal. *MuSiQue: Multihop Questions via Single-hop Question Composition.* TACL 2022. https://aclanthology.org/2022.tacl-1.31/ · https://direct.mit.edu/tacl/article/doi/10.1162/tacl_a_00475/110996/
- Trivedi, Balasubramanian, Khot, Sabharwal. *Is Multihop QA in DiRe Condition? Measuring and Reducing Disconnected Reasoning.* EMNLP 2020. https://www.semanticscholar.org/paper/2ab70f13c02436af3818d9747227643365b79e8b
- Wolfson et al. *Break It Down: A Question Understanding Benchmark (QDMR / Break).* TACL 2020. arXiv:2001.11770 — https://arxiv.org/abs/2001.11770 · https://allenai.github.io/Break/

**Decomposition & reasoning prompting**
- Zhou et al. *Least-to-Most Prompting Enables Complex Reasoning in Large Language Models.* ICLR 2023. arXiv:2205.10625 — https://arxiv.org/abs/2205.10625
- Press et al. *Measuring and Narrowing the Compositionality Gap in Language Models (Self-Ask).* Findings of EMNLP 2023. arXiv:2210.03350 — https://arxiv.org/abs/2210.03350 · https://ofir.io/self-ask.pdf
- Khot et al. *Decomposed Prompting: A Modular Approach for Solving Complex Tasks (DecomP).* ICLR 2023. arXiv:2210.02406 — https://arxiv.org/abs/2210.02406
- Prasad et al. *ADaPT: As-Needed Decomposition and Planning with Language Models.* Findings of NAACL 2024. arXiv:2311.05772 — https://arxiv.org/abs/2311.05772 · https://aclanthology.org/2024.findings-naacl.264/

**Interleaved / active / self-reflective retrieval**
- Trivedi et al. *Interleaving Retrieval with Chain-of-Thought Reasoning for Knowledge-Intensive Multi-Step Questions (IRCoT).* ACL 2023. arXiv:2212.10509 — https://arxiv.org/abs/2212.10509 · https://github.com/StonyBrookNLP/ircot
- Yao et al. *ReAct: Synergizing Reasoning and Acting in Language Models.* ICLR 2023. arXiv:2210.03629 — https://arxiv.org/abs/2210.03629 · https://react-lm.github.io/
- Jiang et al. *Active Retrieval Augmented Generation (FLARE).* EMNLP 2023. arXiv:2305.06983 — https://aclanthology.org/2023.emnlp-main.495/ · https://github.com/jzbjyb/FLARE
- Asai et al. *Self-RAG: Learning to Retrieve, Generate, and Critique through Self-Reflection.* ICLR 2024. arXiv:2310.11511 — https://arxiv.org/abs/2310.11511
- Yu et al. *Chain-of-Note: Enhancing Robustness in Retrieval-Augmented Language Models.* 2023. arXiv:2311.09210 — https://arxiv.org/abs/2311.09210

**2024–2026 iterative-retrieval / SOTA & surveys**
- Wang et al. (Microsoft). *Chain-of-Retrieval Augmented Generation (CoRAG).* 2025. arXiv:2501.14342 — https://arxiv.org/abs/2501.14342 · https://arxiv.org/html/2501.14342
- *GenDec: A robust generative Question-decomposition method for Multi-hop reasoning.* 2024. arXiv:2402.11166 — https://arxiv.org/html/2402.11166v1
- *GRITHopper: Decomposition-Free Multi-Hop Dense Retrieval.* 2025. arXiv:2503.07519 — https://arxiv.org/pdf/2503.07519
- *POQD: Performance-Oriented Query Decomposer for Multi-vector retrieval.* 2025. arXiv:2505.19189 — https://arxiv.org/pdf/2505.19189
- *Question Decomposition for Retrieval-Augmented Generation.* 2025. arXiv:2507.00355 — https://arxiv.org/pdf/2507.00355
- *Mitigating Lost-in-Retrieval Problems in Retrieval Augmented Multi-Hop Question Answering.* 2025. arXiv:2502.14245 — https://arxiv.org/pdf/2502.14245
- *Retrieval–Reasoning Processes for Multi-hop QA: A Four-Axis Design Framework and Empirical Trends.* 2026. arXiv:2601.00536 — https://arxiv.org/html/2601.00536v1
- *Multi-step retrieval and reasoning improves radiology question answering with large language models.* 2025. PMC12749912 — https://www.ncbi.nlm.nih.gov/pmc/articles/PMC12749912/
- *Big Reasoning with Small Models: Instruction Retrieval at Inference Time.* 2025. arXiv:2510.13935 — https://arxiv.org/pdf/2510.13935

**Supporting reasoning technique**
- Wang et al. *Self-Consistency Improves Chain-of-Thought Reasoning in Language Models.* ICLR 2023. arXiv:2203.11171 — https://arxiv.org/abs/2203.11171

---

*Note on numbers:* metrics are quoted as each paper reports them; retrieval corpora, reader models, and evaluation settings (open-domain vs distractor, answer-only vs supporting-fact) differ across rows, so use them directionally. The load-bearing, well-replicated claims for our purposes are: (1) interleaved solve-and-retrieve >> one-shot for multi-hop; (2) compounding error is geometric in depth; (3) the compositionality gap is scale-invariant; (4) a strong harness on a modest model beats a naive harness on a large one.
