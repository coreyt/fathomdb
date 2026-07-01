# V-3 plan — OPP-1 decomposition-iteration multi-hop (EXP-ITER-D/-P/-POLICY) on the improved-recall substrate (DRAFT for HITL)

> **Status: DRAFT for HITL approval. Do NOT run yet.** This pins the concrete, runnable design for
> **V-3 = OPP-1**, the deliberate kickoff of OPP-1's gate now that **V-1 has LANDED** (the keystone
> live-CE re-validation, `dev/plans/runs/V-1-results.md`, GATE: PASS) and the **GPU CE-rerank path is
> proven safe** (#19 stability check cleared `cuda:0`, ~20× CE speedup; the display K620 stays off).
> Per the plan V-3 is **held strictly behind V-1** and is **NOT pulled forward** (handoff §8c; plan
> §2). It iterates *on top of* V-1's re-validated per-intent config registry + recall envelope; running
> iteration on the *unimproved* (0.8.3-fallback) substrate is the exact error the handoff throughline
> warns against.
>
> **Ground sources:**
> `dev/plans/runs/0.8.11-handoff-to-0.8.15.md` §2 (the V-3 gate definition + the two-"iterations"
> distinction), §2b (the at-power registered experiments EXP-ITER-D/-P/-POLICY + EXP-AF-MH), §8b
> (JOINT ownership split) + §8c (strict V-1→V-7 sequencing);
> `dev/plans/plan-0.8.11.2.md` §2 (the `Phase A → V-1 → OPP-1@V-3 → OPP-3@V-7` sequencing), §4 (OPP-1
> exit criteria), §2A (the `$75` pooled envelope + the cross-repo message bus + the Adopt-GO
> hard-stop), §2B (B-1: what gates the adoption arms);
> the concrete design `memex/dev/fathomdb/OPP-1-experiments.md` (arms A0–A4, D-1..D-4, decision rule);
> the prior-mechanism result `dev/plans/runs/expaf-value.md` (the EXP-AF KILL A4 must re-open);
> the landed keystone `dev/plans/runs/V-1-results.md` (the substrate: tuples + recall envelope);
> the harnesses `src/python/eval/r2_parity_eval.py` (identical-answerer F1/EM + `decide_08x`),
> `src/python/eval/expaf_value_run.py` (the EXP-AF protocol for A4), and the corpora on disk
> (`data/corpus-data/raw/{musique_dev.jsonl,hotpotqa_dev.jsonl}`).

---

## 1. Objective + what V-3 gates (the decision it resolves)

**Objective.** Test whether **decomposition-iteration** — forming NEW sub-queries and retrieving again
(IRCoT / Self-Ask / Iter-RetGen class) — **manufactures recall** above V-1's single-shot envelope on
**true compositional multi-hop**, and if so which *shape* (parallel vs sequential) and per-intent
`iteration_policy` to recommend. This is FathomDB's **largest untested exposure**: single-shot shipped +
an EXP-AF KILL that never tested `multi_hop` (handoff §2a #1), on the one class where the iterative family
posts its biggest paper gains and via the one mechanism that *adds to* the pool rather than reshuffling a
capped one (the EXP-AF failure mode).

**The load-bearing distinction (handoff §2, V-3 row).** There are **two** "iterations":
- **(a) feedback / re-rank** — an agent reshuffles a *capped* pool. EXP-AF **KILLed** this for needle
  (recall-bound; can't help; `expaf-value.md` decisive number −0.0126 [−0.0274,0.0022] @ c_rt=0.02).
  V-3 re-opens it **only** on multi_hop as the deferred, lower-priority arm **A4 = EXP-AF-MH**.
- **(b) decomposition / multi-hop** — forms *new* sub-queries and retrieves again, **manufacturing
  recall**. **Never tested.** This is the V-3 primary (arms A1/A2/A3) and attacks EXP-AF's own root cause.

**What V-3 gates (decisions, from `OPP-1-experiments.md` §1):**

| # | Decision | Metric / rule | Resolves |
|---|----------|---------------|----------|
| **D-1** | Does decomposition **manufacture recall** over single-shot on true multi-hop? | Δ supporting-fact recall@k **AND** Δ answer F1/EM, A1/A2 vs A0, net of read cost, at power | the V-3 gap — the one untested multi-hop exposure |
| **D-2** | **Parallel-fan-out (A1) vs sequential-iterate (A2)** — which, when? | quality-per-unit-cost by dependency-type stratum (independent vs compositional hop labels) | Memex's shape preference (parallel where independent; sequential only for hop-dependent) |
| **D-3** | Does the lift survive on a **personal** corpus, and is multi_hop **frequent enough** to earn the LLM-decompose tier? | frequency audit + win / non-regression on real personal gold | the **Adopt-GO** gate (academic wins are directional only) |
| **D-4** | What per-intent **`iteration_policy` ∈ {single_shot, parallel_decompose, sequential_iterate}**? | cheapest arm that captures the lift, per intent | the recommend-only router hint (Slice-36 seam generalizes `feedback_arm` → `iteration_policy`) |

**Pass / GO rule (the two-gate maturity guard — plan §4, `OPP-1-experiments.md` §6):**
- **Build-GO** (authorizes building the decompose path **default-OFF**): A1/A2 show a **significant
  recall + answer lift** over A0 on **MuSiQue + HotpotQA** at power, **net of read cost**, AND A3 (oracle)
  confirms head-room exists. Wire into the Slice-36 `_maybe_escalate()` seam; default stays `single_shot`
  (= today's shipped behavior; preserves the EXP-AF KILL for needle).
- **Adopt-GO** — a **SEPARATE gate and a HITL hard-stop** (see §5/§7): Build-GO **AND** the frequency audit
  shows enough real compositional turns **AND** a win / non-regression on **real personal gold**. Otherwise:
  keep single_shot default; decomposition ships per-intent opt-in or stays parked.

**What "on the improved-recall substrate" means.** V-3 consumes V-1's landed artifact:
- the **per-intent tuples** (§3 below) as the single-shot **A0 baseline configuration** — A1/A2 must beat
  A0 *tuned at its V-1 optimum*, not a strawman;
- the **recall envelope** (V-1 §3) as the ceiling A1/A2's manufactured recall is measured *against*
  (MuSiQue base-order gold-in-pool: @10 0.534 → @50 1.000 — decomposition's job is to lift the @10);
- the **live CE `ce_score`** (V-1 confirmed `ce_norm_is_active`, max ≈0.999) reranking every sub-query's
  retrieval.

---

## 2. Arm matrix

**The SUT is the FathomDB substrate + the Memex decompose loop** (`OPP-1-experiments.md` §3). Arms:

| Arm | Description | Owner of the new step | $ |
|-----|-------------|-----------------------|---|
| **A0** single-shot baseline | one `search`/`ce_rerank` pass at the V-1 per-intent tuple (multi_hop musique 200/**20**/0.3; hotpotqa 200/**10**/0.3), then one reader answer | FathomDB harness | reader-priced |
| **A1** parallel-decompose | Memex LLM decomposes → *k* **independent** sub-queries fired in **one round** → merge → reader answer | **Memex** (decompose+merge LLM) | decompose + reader priced |
| **A2** sequential-iterate | IRCoT / Self-Ask: hop-N query **conditioned on** hop-(N-1) results → merge → reader answer | **Memex** (decompose+reason LLM) | decompose×hops + reader priced |
| **A3** oracle-decompose *(upper bound)* | gold sub-questions = the **native MuSiQue `question_decomposition` field** (present on disk — verified, see §6). Isolates decomposition *quality* from retrieval — **retrieval is $0**, reader still priced | FathomDB (uses gold) | reader-priced only (no decompose LLM) |
| **A4** *(DEFERRED, lower-priority)* = **EXP-AF-MH** | agent re-rank/feedback over a **capped** multi_hop pool (**reshuffle, NOT manufacture**) — closes the "EXP-AF KILL is current-substrate-provisional, multi_hop untested" loophole. **Run under the EXP-AF protocol** (`expaf_value_run.py`: asymmetric cost weighting + the `c_rt` round-trip cost) so a multi_hop GO/KILL is apples-to-apples with the original needle KILL | FathomDB (agent-signal) | agent-priced |

**Stratifiers (all arms):** dependency type (independent vs compositional, from MuSiQue hop labels),
hop count (2 / 3 / 4), retrieval depth (`n_docs`), intent class (`OPP-1-experiments.md` §3).

**GPU disposition — CE rerank on `cuda:0`.** V-1 ran the TinyBERT-L-2 CE on **CPU** (Candle,
`lib.rs` L5658). V-3 multiplies the CE-pass count: A1 fires *k* sub-queries/question, A2 iterates over
2–4 hops, over 2,417 MuSiQue + subsampled HotpotQA × up-to-4 arms. The **#19 stability check cleared the
GPU rerank path as safe with a ~20× CE speedup**, so V-3 **runs the CE rerank on `cuda:0`** (the analog
of the 0.8.7 `embed-cuda` path, validated on RTX 3090 / CUDA 12.6, `STATUS-0.8.7.md` R-GPU-3). The
**display Quadro K620 (index 2) stays off** — do not target it (`STATUS-0.8.7.md` L118-119); the second
3090 (`cuda:1`) is available if a parallel embed/rerank split helps. This is what makes the multiplied
sub-query CE load tractable inside the wall-clock + `$75` envelope. **Build-vehicle caveat (open Q, §7):**
running the CE on GPU needs a reranker-on-cuda build analog to `embed-cuda`, built on the **MAIN tree**
(not this worktree — the `.venv` mutex / maturin-worktree ban); confirm the reranker-cuda feature exists
or fall back to CPU CE (correct, just ~20× slower).

**Arms that actually run (V-3 scope):** **A0, A1, A2, A3** are the primary EXP-ITER-D/-P set. **A4
(EXP-AF-MH)** is explicitly **deferred / lower-priority** and is **not a start gate** — schedule it only
after A0–A3 land, under its own EXP-AF cost protocol.

---

## 3. Corpora × classes + the V-1 tuples consumed (PER-CORPUS, never pooled)

| Corpus | Role | On disk? | Tuple consumed (A0 baseline, from V-1-results §1) |
|--------|------|----------|---------------------------------------------------|
| **MuSiQue (full 2,417)** | primary at-power; compositional 2–4 hop; carries **A3 oracle** decomposition + **D-2** stratification | ✅ `data/corpus-data/raw/musique_dev.jsonl` (2,417 answerable; **`question_decomposition` VERIFIED present** — keys incl. `hop_count`, `paragraphs`, `question_decomposition`) | **candidate_k 200\*, pool_n 20, alpha 0.3** (r@10 0.535, the load-bearing multi_hop tuple) |
| **HotpotQA** | secondary at-power; 2-hop | ✅ `data/corpus-data/raw/hotpotqa_dev.jsonl` (7,405 rows, native `supporting_facts`) — **subsample to paper-N** (see below) | **candidate_k 200\*, pool_n 10, alpha 0.3** (r@10 vacuous — 10-para pools; read **nDCG@10 0.902 / MRR 0.947** instead) |
| **2WikiMultiHopQA** | **deferred** — the third corpus for the §2b / **V-5** multi-corpus close, **NOT a V-3 start gate** | ❌ not on disk (either repo); cheap to acquire later | — |
| **Memex personal gold** | **the Adopt-GO gate** (D-3) — Memex-owned, gated on B-1 + Cause-A | Memex-side (`eval/goldset/...`; real-gold via OPP-9 capture) | — (adoption arm; HITL hard-stop) |

\* multi_hop corpora are per-question ~10–20 paragraph distractor pools, so `candidate_k` is **not** a
meaningful recall axis (whole pool < any candidate_k); the live axes are `pool_n × alpha`. This is V-1's
own footnote — carry it: on MuSiQue the decomposition arms' recall gain shows up in the **@10 lift** (base
@10 = 0.534, saturates to 1.000 by @50), i.e. decomposition's job is to pull supporting facts into the
top-10, not to widen an already-saturated pool.

**Tuple-consumption notes (from the prompt's pins + V-1 caveats):**
- **needle 200/200/0.7 · multi_session 300/200/1.0** — the non-multi_hop A0 configs if EXP-ITER-POLICY
  sweeps them (needle stays `single_shot` per EXP-AF; these anchor the per-intent policy readout D-4).
  Both land on `pool_n = 200` = the **grid edge** (V-1 boundary flag) — true optimum may be deeper; a cheap
  `POOL_NS`-extend follow-up, not a V-3 blocker.
- **temporal — PIN V-1's live-CE `500/50/1.0`**, NOT any base-shifted `500/20` (0.8.3 fallback) or
  `300/20` variant. **Temporal base-caveat:** the V-1 temporal tuple is **LME-only** — the LOCOMO
  corroboration was acquired but **NOT run** (V-1-results accepted-carry #2), and the recency knob was
  **under-powered** (+0.10 point but CI-lo +0.0267 < +0.04, off-default). Temporal is peripheral to V-3
  (multi_hop is the class), but if EXP-ITER-POLICY reports a temporal row, flag it as the weaker-anchored
  tuple.
- **multi_hop musique 20/0.3 · hotpotqa 10/0.3** — the load-bearing A0 baselines (V-1 measured, per-corpus,
  never pooled). MuSiQue is load-bearing; **HotpotQA r@10 is vacuous** (10-para pools ⇒ r@10 ≡ 1.0) → judge
  it on **nDCG@10 / MRR**.

**Per-corpus discipline** is enforced by `opp3_eval_support.decide_per_corpus` (raises on any reserved
cross-corpus pool key) — MuSiQue and HotpotQA are each their own decision; **never pooled**.

**HotpotQA subsample.** 7,405 rows at ~4 arms × multi-LLM-call is the dominant cost driver; subsample to a
fixed paper-scale N (propose **~1,000**, seed-pinned) for the at-power run and record it as the power
target — the full set is available if a power check demands it. MuSiQue runs the **full 2,417** (its
paper-N).

---

## 4. Metrics + decision rule

**Per-corpus / per-arm metrics** (`OPP-1-experiments.md` §3; harness `r2_parity_eval.py`):

- **supporting-fact recall@k** — did the retrieved pool contain the gold supporting facts (MuSiQue
  `paragraph_support_idx` / HotpotQA `supporting_facts`)? The *manufactured-recall* signal (D-1). $0.
- **answer F1 / EM** — the identical-answerer protocol (`r2_parity_eval.py` L146-227, `R2Harness`
  L761): the **same reader** answers over each arm's retrieved context; F1/EM scored against gold.
  **Reader is priced** (LLM generation); F1/EM scoring itself is deterministic / $0.
- **n_reads + latency + token cost** — the cost side of quality-per-unit-cost.
- **quality-per-unit-cost (decisive)** — a lift that doubles reads must clear a **higher** bar; this is the
  arm-selection metric for D-2 / D-4.
- **nDCG@10 / MRR** — the discriminating multi_hop signal on HotpotQA (r@10 vacuous, §3).
- Significance via FathomDB's **`decide_08x` paired bootstrap + CIs** (`decision_rule_084.py:decide_084`;
  `paired_bootstrap_ci`; per-corpus via `decide_per_corpus`).

**Judge (firewall).** The Fathom substrate is the SUT → **judge-as-Memex** with gold for answer
correctness; a held-out judge is **not** required here (it is required only when a *Memex model choice* is
the SUT, per the OPP-11 firewall, `OPP-1-experiments.md` §3). So answer correctness = F1/EM vs gold, no
separate priced judge model.

**Decision rule (what V-3 must show):**

1. **D-1 recall manufacture (blocking for Build-GO).** On MuSiQue + HotpotQA at power, A1 and/or A2 show a
   **significant** Δ(supporting-fact recall@k) **AND** Δ(answer F1/EM) over A0, **CI-lower-bound net of read
   cost > 0** (the quality-per-unit-cost bar). A3 (oracle) must show **head-room exists** (oracle > A0) —
   else the ceiling is decomposition-quality-bound and no realistic decomposer can win.
2. **D-2 shape.** Compare A1 (parallel) vs A2 (sequential) by dependency-type stratum: parallel should
   capture most of the lift on **independent** sub-Qs at lower latency; sequential wins only on
   **hop-dependent** compositions.
3. **D-4 policy.** Emit the per-intent `iteration_policy` = the cheapest arm capturing the lift
   (needle = `single_shot` per EXP-AF; multi_hop = the D-1/D-2 winner; global = map-reduce fan-out,
   already shipped).
4. **A4 (if run).** Apply the **EXP-AF rule verbatim** (`expaf-value.md`): GO iff the depth-1 reranking-lift
   CI-lower-bound, net of one round-trip (c_rt = 0.02), exceeds 0 — now measured on `multi_hop` (the class
   EXP-AF excluded). A multi_hop GO would flip the "EXP-AF KILL is permanent" read (handoff §3); a KILL
   closes the loophole honestly.

**Gate:** V-3 **lands** (produces a committed verdict/artifact, R-U-1) when D-1/D-2/D-4 have live-substrate
verdicts on MuSiQue + HotpotQA with `decide_08x` CIs. **Build-GO** is the automatic outcome if D-1 passes;
**Adopt-GO is NOT decided here** — it is the HITL hard-stop (§5/§7). A4 staying deferred is an acknowledged
carry, not a blocker.

---

## 5. $0-vs-priced — **V-3 HAS a priced component** (draws the `$75` pool; bus-announce before spend)

**Unlike V-1 ($0), V-3 is priced.** The priced surface is the **caller-side LLM** the iterative arms
require (plan §0 #2: "OPP-1: the frontier-answerer passes" is an enumerated pool draw):

| Component | Priced? | Why |
|-----------|:-------:|-----|
| A0 / A3 retrieval (search + CE on `cuda:0`) | **$0** | local engine; A3 uses **gold** sub-questions (no decompose LLM) |
| supporting-fact recall@k, F1/EM scoring, `decide_08x` | **$0** | deterministic over gold |
| **A1 / A2 decompose + reason LLM** | **PRICED** | Memex-owned decompose/dependency/merge calls (1 call A1; 2–4 calls/question A2) |
| **A0–A3 reader (answer generation)** | **PRICED** | the identical-answerer must generate an answer per question per arm for F1/EM |
| **A4 agent-signal (if run)** | **PRICED** | the EXP-AF `claude-sonnet`-class agent over the capped pool |

**Cost estimate (BAND — must be pinned with Memex before spend).** Dominant driver = LLM calls =
(questions) × (arms) × (calls/arm). Rough order: MuSiQue 2,417 + HotpotQA ~1,000 ≈ **3,400 questions** ×
{A0≈1 reader, A1≈1 decompose+1 reader, A2≈3 reason+1 reader, A3≈1 reader} ≈ **~30k LLM calls**. At a
cheap/mid tier (~$0.0005–0.002/call) that is **~$15–$45** — a **large fraction of the `$75` pool**, and
sensitive to model choice + N. **Recommendations (per the budget-discipline memory):**
- **Cheap-validate first:** a `gemini-2.5-flash-lite`-class pilot on ~50 questions/arm to confirm the
  decompose+answer path populates before any powered spend; strong reader only for the powered run.
- **Resilience preconditions before spend** (plan R-U-7): incremental checkpoint + verified `--resume` +
  429/5xx backoff + window-fit + completeness guard + running `$` ledger; **auto-stop at the cap**.
- **Bus-announce before ANY spend** (plan §2A, R-U-8): post a `request` line on
  `/home/coreyt/projects/memex-worktrees/0.5.1-fathom-chat/fathom-memex-chat.jsonl` naming the arm + the
  projected `$`, and **coordinate the split so the two sides do not double-spend** the shared `$75` pool
  (the decompose/reason LLM is Memex-owned; the reader may be either side — pin at handoff).

**HITL Adopt-GO hard-stop (plan §2A, §4; `OPP-1-experiments.md` §6).** Even a clean **Build-GO does NOT
authorize adoption.** Adopt-GO is a **separate gate and one of the only two autonomous-run hard-stops**:
it requires (a) the **multi_hop-frequency audit** on the live Memex workload (Memex-owned, D-3) and
(b) a win / non-regression on **real personal gold**, which is keyed by **Cause-A `stable_id`** and
**gated on B-1** (plan §2B: the academic/build arms run regardless of B-1; only the as-Memex **adoption**
arms wait on it). The Steward **stops for HITL at Adopt-GO** and does not proceed to any product
commitment autonomously.

---

## 6. Prerequisite status + runnability

| Prereq | Status | Evidence |
|--------|--------|----------|
| **V-1 landed** (the substrate) | ✅ DONE | `V-1-results.md` GATE: PASS — tuples + recall envelope + live CE |
| **P0-3 MuSiQue `question_decomposition`** (unblocks A3) | ✅ DONE | `musique_dev.jsonl` 2,417 answerable; **verified** keys carry `question_decomposition` (+`hop_count`,`paragraphs`) |
| **HotpotQA on disk** (A0/A1/A2 secondary) | ✅ DONE | `hotpotqa_dev.jsonl` 7,405 rows, native `supporting_facts` |
| **Identical-answerer harness** (F1/EM + `decide_08x`) | ✅ present | `r2_parity_eval.py` (`R2Harness`, `BaseAnswerer`, per-corpus decide) |
| **EXP-AF protocol harness** (A4) | ✅ present | `expaf_value_run.py` (asymmetric cost + `c_rt`); Slice-36 `_maybe_escalate()` seam (`dev/prototypes/l2-router/`) |
| **GPU CE rerank cleared** | ✅ (asserted) | #19 stability check → `cuda:0` safe, ~20× CE; 0.8.7 `embed-cuda` on RTX 3090 (`STATUS-0.8.7.md`) |
| **Cause-A `stable_id`** (adoption keying only) | CUT IN PROGRESS | `CAUSE-A-sizing.md` GO; **gates only the D-3 adoption arm**, NOT A0–A3 |
| **B-1 Memex refit** (adoption vehicle) | Option B in progress | plan §2B; gates only the as-Memex adoption arms |

**BLOCKERS / flags to clear before V-3 can run:**

- **B1 — the Memex decompose/reason LLM is not wired here (BLOCKER for A1/A2).** A1/A2's new step is
  **Memex-owned** (`OPP-1-experiments.md` §4, handoff §8b). It must be dispatched over the bus to the
  `memex-steward` orchestrator (already spawned, `STATUS-0.8.11.2.md`), with the per-experiment split
  pinned at kickoff. **A0 + A3 can run FathomDB-side first** ($0 retrieval; only the reader is priced) —
  they establish the baseline + oracle head-room while the Memex loop stands up.
- **B2 — reader-on-GPU/CE build vehicle (BLOCKER for the ~20× speedup).** Running the CE on `cuda:0` needs
  a reranker-cuda feature analog to `embed-cuda`, built on the **MAIN tree** (`maturin develop` is banned
  in this worktree — `.venv` mutex). Confirm the feature exists; else fall back to CPU CE (correct, ~20×
  slower — may pressure the wall-clock but not the `$` budget). Who owns the MAIN-tree rebuild?
- **B3 — the priced envelope is shared and un-split (BLOCKER for spend).** The `$75` pool is pooled across
  ALL priced passes (OPP-1 + OPP-3 + OPP-6). The A1/A2 answerer passes must be bus-announced + the
  FathomDB↔Memex spend split agreed before the first priced call (§5).
- **B4 — 2Wiki absent (NOT a V-3 blocker).** Deferred to the V-5 multi-corpus close (`OPP-1-experiments.md`
  §5/§7); V-3 begins on MuSiQue + HotpotQA per the decision rule.
- **B5 — Adopt-GO inputs are Memex-side + B-1-gated (NOT a Build-GO blocker).** The frequency audit + real
  personal gold are the adoption gate, not the build gate; they are the HITL hard-stop (§5).

---

## 7. Execution steps + open questions

**Execution (once HITL approves + B1/B2/B3 dispositioned):**

1. **Stand up the reader + GPU CE** on the MAIN tree: confirm/enable the reranker-cuda build (or accept CPU
   CE); wire `r2_parity_eval` reader over the cheap-validate model first.
2. **A0 baseline (FathomDB, $0 retrieval)** — single-shot at the V-1 multi_hop tuples (musique 200/20/0.3,
   hotpotqa 200/10/0.3), CE on `cuda:0`; compute supporting-fact recall@k + (priced) reader F1/EM.
3. **A3 oracle (FathomDB, $0 retrieval)** — retrieve on the native MuSiQue `question_decomposition` gold
   sub-questions; confirm **head-room** (oracle > A0). If oracle shows no head-room, the ceiling is
   decomposition-quality-bound → down-weight A1/A2 before spending on them.
4. **Cheap-validate pilot** — ~50 q/arm on the flash-lite reader + a cheap decomposer; confirm the path
   populates; **bus-announce** the projected powered `$` and pin the spend split.
5. **A1 / A2 (JOINT, priced)** — Memex decompose/reason loop over the bus; parallel (A1) vs sequential
   (A2); stratify by dependency type + hop count; `decide_08x` per corpus, **never pooled**; running `$`
   ledger, auto-stop at cap.
6. **Verdicts** — D-1 (recall manufacture), D-2 (shape), D-4 (per-intent `iteration_policy`); write
   `V-3-output.json` + `V-3-results.md`; update `STATUS-0.8.11.2.md` OPP-1 row → the landed verdict.
7. **A4 (EXP-AF-MH)** — deferred; run only after A0–A3 land, under the EXP-AF `c_rt` protocol.
8. **HARD-STOP at Adopt-GO** — do NOT proceed to adoption; surface the frequency audit + real-personal-gold
   requirement (B-1 / Cause-A gated) to HITL.

**Open questions for HITL:**

1. **Priced-envelope split (B3).** Confirm the A1/A2 answerer/decompose passes draw the shared `$75` pool,
   the FathomDB↔Memex spend split, and the bus-announce-before-spend protocol. Approve the **cheap-validate
   pilot → powered run** two-stage with auto-stop at cap. (Est. **~$15–$45**, model+N sensitive.)
2. **Reader / decomposer model choice.** Which reader for the identical-answerer F1/EM (a strong reader
   makes the arms comparable but costs more), and which Memex-side decomposer? Pin the model + HotpotQA
   subsample N (proposed ~1,000) to fix the cost.
3. **GPU CE build vehicle (B2).** Confirm the reranker-cuda MAIN-tree build (or accept CPU CE fallback);
   who owns the rebuild? (Same MAIN-tree / worktree-maturin constraint as V-1's B2.)
4. **A4 scope.** Run EXP-AF-MH in this V-3, or keep it deferred to a follow-up? (It closes the "EXP-AF KILL
   is provisional / multi_hop untested" loophole but is lower-priority than A1/A2 and adds agent spend.)
5. **Adopt-GO sequencing.** Confirm the Steward **hard-stops at Adopt-GO** (frequency audit + real personal
   gold, Cause-A/B-1 gated) and that Build-GO alone lands autonomously — matching the plan §2A stop posture.
6. **Temporal / non-multi_hop policy rows.** For EXP-ITER-POLICY (D-4), confirm needle stays `single_shot`
   (EXP-AF) and that the temporal row (if emitted) carries the V-1 LME-only / under-powered-recency caveat.

---

### Underspecification note (per the prompt's ask)

OPP-1 / V-3 is **well-specified** across the docs — the handoff §2/§2b (gate + at-power experiments),
`OPP-1-experiments.md` (arms A0–A4, D-1..D-4, decision rule, corpora), and plan §4 (exit criteria) form a
coherent, runnable design; V-1 landed the substrate it consumes. The genuinely **open** items are not
design gaps but **coordination/HITL calls**: (i) the priced split + model choice (§5, Q1/Q2); (ii) the GPU
CE build vehicle (Q3); (iii) whether A4 runs now or later (Q4). This draft proposes concrete defaults for
each rather than leaving them silent. The one thing V-3 **cannot** resolve autonomously by design is
**Adopt-GO** — a deliberate HITL hard-stop.
