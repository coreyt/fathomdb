# EXP-AF — Does an agent relevance signal beat the engine's `ce_score`? (FathomDB 0.8.11, Slice 30)

**Verdict: KILL.** Decision date 2026-06-28 · HITL gate #4 · Branch `0.8.11` ·
Spend **$3.66 of $5** · Model `claude-sonnet`.

> This is the narrative companion to the machine-generated result
> (`dev/plans/runs/expaf-value.md` + `expaf-value-output.json`). Every number here is
> measured, never fabricated; the CE reranker was confirmed ACTIVE before any
> measurement. Reproduce with
> `python -m eval.expaf_value_run --model claude-sonnet --depth2 --max-usd 5.0`.

---

## 1. The question, and why it was the decisive one

FathomDB's planner-router design (PSD §I.D, §II.C) proposes that **the agent is a
relevance-signal partner**, not just a caller: after a retrieval, the agent can judge
whether the returned passages are actually relevant and feed that signal back so the
router re-plans or re-ranks. The appeal is theoretical — relevance feedback (Rocchio),
value-of-information, contextual bandits — but the design explicitly refused to assume
it works. It made the value an **experiment**, gated to a go/no-go before any of the
loop is built.

EXP-AF is that experiment. The hypothesis, stated precisely:

> *An agent relevance/intent signal beats the engine's internal `ce_score`-only routing
> **net of round-trip cost**, on the existing substrate (no fresh rebuild), within the
> 1–2 re-plan depth bound.*

The "net of round-trip cost" clause is the whole game. An agent round-trip is not free —
it costs latency and tokens — so the signal must not merely *help*, it must help **more
than it costs**.

## 2. What Slice 25 already told us (and what it left open)

Slice 25 (EXP-Fr-acc/VoI) ran the cheap version of this test: a low-cost agent
(`gemini-flash-lite`) re-judged the relevance of the **single top-1 passage**, and we
asked whether that judgment predicted retrieval success better than the engine's free
`ce_score`. It did not — and not by a little: **lift −0.138 [−0.189, −0.087]**, with the
agent's AUC (0.545) far below `ce_score`'s (0.667). A cheap, general LLM re-judging one
passage is doing exactly the job the engine's specialized cross-encoder already does
well, so losing was unsurprising.

Slice 25 was careful to call this a **qualified** kill, and named two escape hatches that
EXP-AF was built to close:

1. **A weak agent.** Maybe a *stronger* frontier agent recovers the value.
2. **A starved view.** The eval agent saw only the top-1 passage, not the pool — and not
   the user-intent context a deployed agent would hold.

Slice 25 also localized *where* any value could possibly live: the **break-even cells** —
queries with **low CE confidence (`ce_top` < 0.2)**, where the engine itself is unsure.
Outside those cells, `ce_score` is confident and correct, and there is nothing for an
agent to add.

## 3. How EXP-AF was designed to be a fair, decisive test

EXP-AF deliberately stacks the deck **in favor** of the agent, so that a KILL is
conclusive rather than an artifact of a weak setup:

- **A genuinely strong agent.** `claude-sonnet`, a frontier model — not the cheap
  flash-lite of Slice 25.
- **The full pool, not one passage.** The agent sees the **top-20 CE-reranked passages**
  and flags every one that supports the answer. This directly fixes the Slice-25
  "top-1-only" caveat.
- **A real reranking mechanism, not a meta-prediction.** Agent-flagged passages are
  promoted above the `ce_score` order, and we measure the change in **actual retrieval
  success** (strict all-gold-in-top-10) — the deployment mechanism, not a proxy.
- **Aimed at the break-even cells.** All 406 break-even queries (`ce_top` < 0.2) from the
  LME real-CE substrate, balanced across needle / multi_session / temporal.
- **A free $0 pre-gate.** Before spending a cent, we computed the **headroom**: the
  maximum lift *any* reranker could ever achieve, given where the gold actually sits.

That headroom check is important. On the break-even cells, the gold answer is reachable
in the shown top-20-but-not-top-10 for **11.8%** of queries (the depth-1 ceiling), and in
the top-40-but-not-top-10 for **20.9%** (the depth-2 ceiling). So there *was* real room
for an agent to help — the experiment was not dead on arrival. The question was purely
whether a strong agent could *capture* that room.

## 4. What we measured

### 4.1 Reranking lift (the primary, decisive arm)

Over all 406 break-even queries, promoting `claude-sonnet`'s relevance judgments above
the `ce_score` order changed retrieval success by:

| Metric | Value |
|---|---|
| ce retrieval-success (baseline) | 0.456 |
| agent depth-1 retrieval-success | 0.463 |
| **reranking lift (agent − ce, paired)** | **+0.0074 [−0.0074, +0.0222]** |
| mechanism | promoted **6** gold into top-10, demoted **3** out |

The lift's confidence interval **spans zero even at a free round-trip** (c_rt = 0). Net
of a modest round-trip cost it goes negative:

| round-trip cost c_rt | net lift | net lift CI | GO? |
|---|---|---|---|
| 0.00 | +0.0074 | [−0.0074, +0.0222] | No |
| **0.02** | **−0.0126** | **[−0.0274, +0.0022]** | **No** |
| 0.05 | −0.0426 | [−0.0574, −0.0278] | No |
| 0.10 | −0.0926 | [−0.1074, −0.0778] | No |

No intent class clears noise on its own: needle +0.010 [−0.010, +0.031], multi_session
+0.010 [−0.029, +0.049], temporal exactly 0.0.

The most telling number is not the lift itself but **how little of the headroom the agent
captured**. The depth-1 ceiling was 0.118; the agent realized +0.0074 — roughly **6% of
the available room**. It promoted 6 buried gold passages and *demoted 3 correct ones*,
nearly cancelling out. A frontier model, handed the full candidate pool, still could not
reliably tell which of the low-confidence passages was the right one.

### 4.2 Detection (the apples-to-apples comparison with Slice 25)

Reusing the same calls, we asked the Slice-25 question directly: does the strong agent's
relevance flag on the *top-1* passage beat `ce_top` at predicting retrieval success?

- **Detection lift −0.0296 [−0.0715, +0.0123]** (AUC ce 0.452 vs agent 0.497).

The stronger agent **closes most of the cheap agent's −0.138 gap** — going from a heavy
loss to roughly a wash — but it still does **not beat** the free internal `ce_score`. The
point estimate remains negative and the CI spans zero. Strength helped; it was not enough.

### 4.3 One-shot vs iterative (the depth-bound decision)

Depth-2 is the single allowed re-plan: on depth-1 failures, expand the agent's view to
the top-40 and ask again (trigger rate 0.537, so ~1.54 round-trips per query). It
recovered 6 additional gold passages:

- incremental lift (depth-2 − depth-1) **+0.0148 [+0.0049, +0.0271]** — positive
- total lift vs ce **+0.0222 [+0.0049, +0.0395]** — positive at $0 cost
- but **net of the extra round-trip, negative at any c_rt > 0** (−0.0085 at c_rt = 0.02)

So iterating *does* recover more gold in gross terms, but the second round-trip never pays
for itself. **If** an agent loop were ever shipped, one-shot (depth 1) would be the choice
— but under the KILL this is moot, since no loop ships at all.

## 5. The verdict, and why it is conclusive

**KILL.** The go/no-go rule was: GO if and only if the depth-1 reranking-lift CI lower
bound, net of one round-trip (c_rt = 0.02), exceeds zero. It is **−0.0274 < 0**.

This is a *strong* KILL, not a marginal one, because the experiment was built to give the
agent every advantage and it still lost:

- The agent was a frontier model, not a cheap one.
- It saw the entire candidate pool, not one passage.
- It was aimed at exactly the cells where value was hypothesized to live.
- There was genuine headroom (11.8%) for it to capture.

It captured almost none of it. The root cause is structural and matches the PSD's own
caution (§II.C): **in low-`ce` cells the engine is uncertain because the answer genuinely
is not cleanly present in the retrieved text** — these are recall-bound, hard queries. An
agent cannot manufacture recall the substrate never produced; it can only reshuffle what
is there, and when the right answer is ambiguous or absent, even a strong agent reshuffles
about as often into error as into improvement (6 promoted, 3 demoted).

## 6. What this changes downstream

- **Slice 35 (L2 router prototype).** The prototype **drops the agent-signal loop**. The
  `Recommendation.feedback_arm` flag is `False`; the router routes on the internal
  `ce_score` alone. This removes an entire class of complexity (the re-plan loop, the VoI
  ask-or-not policy, the relevance-signal protocol) from the prototype with measured
  justification.

- **F-8b (`record_feedback` governance).** The Slice-0 decision made promotion of
  `record_feedback` from instrumentation to a governed application command **conditional
  on EXP-AF going GO**. EXP-AF KILLed, so **`record_feedback` stays instrumentation** — no
  allowlist change, no Slice-40 reserved-gap patch. The EXP-AF KILL explicitly overrides
  any promote. This also closes HITL #1 (F-8b) on the negative branch.

- **The broader program.** This does **not** say agent feedback is worthless in principle
  — only that, on FathomDB's current substrate and with the engine's already-strong CE
  reranker, an agent relevance signal does not earn its round-trip. The lever that *does*
  have measured value (Gate-2, EXP-A, EXP-B′) is **config-carrying per-intent tuning**
  inside the engine, not an external agent-judgment loop.

## 7. Provenance and reproducibility

| Item | Path |
|---|---|
| This report (prose) | `dev/plans/runs/expaf-value-report.md` |
| Machine result (tables) | `dev/plans/runs/expaf-value.md` |
| Raw result (JSON) | `dev/plans/runs/expaf-value-output.json` |
| Experiment script | `src/python/eval/expaf_value_run.py` |
| Pricing pin (airlock alias) | `src/python/eval/gap_decomposition_run.py` (`claude-sonnet` → claude-sonnet-4-6, $3/$15·1M) |
| Substrate | `dev/plans/runs/0.8.3-rerank-tune.ce-pass.json` (606 LME, real CE pool + gold) |
| Pre-registration | `dev/plans/0.8.11-implementation.md` §1 (EXP-AF row), §5 (F-8b) |
| Design rationale | `dev/design/planner-router-psd-0.8.x.md` §I.D, §II.C, §III.D |
| Ledger entry | `dev/experiments-ledger.md` (EXP-AF, RESOLVED) |
| Status | `dev/plans/runs/STATUS-0.8.11.md` (Slice 30 DONE; HITL #4) |

**Method controls.** CE-active degeneracy guard PASS before measurement (max ce_norm
0.99994, spread 0.99994, α=1.0 reorders an adversarial probe to rank-1). Resilient harness:
per-item checkpoint, idempotent `--resume`, 429/5xx backoff, `BudgetLedger` pre-call
$-guard at $5. Cheap-validated (3 calls) before the full run. 624 calls total
(406 depth-1 + 218 depth-2 re-plans), 0 errors, ~1.84K input / ~14 output tokens per call.
Paired bootstrap CIs (2000 resamples, fixed seed).
