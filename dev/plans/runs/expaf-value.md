# EXP-AF — agent-feedback value test (0.8.11 Slice 30, KILL/GO → HITL #4)

> The decisive HITL-gated experiment. **Real measured numbers** — the CE reranker is ACTIVE (`default-reranker`); `ce_score` is real, confirmed by a degeneracy guard.

- **Hypothesis:** an agent relevance/intent signal beats internal ce_score-only routing NET of round-trip cost, on the existing substrate, within the 1-2 re-plan depth bound (PSD §III.D).
- **Break-even cells:** low ce_top (<0.2) — where Slice-25 located the potential VoI (cheap agent there was dominated by ce_score).
- **Mechanism:** stronger agent (claude-sonnet) sees the top-20 ce-reranked pool (NOT just top-1) and flags relevant passages; promote them above ce order; measure strict retrieval-success (all-gold-in-top-10) lift, then subtract round-trip cost. Depth-2 expands to top-40 on depth-1 failures (the single allowed re-plan).
- **CE-active guard:** max ce_norm=0.999944, spread=0.999935, alpha=1.0 reorders relevant→rank1 (order=[0, 1, 2]). PASS.
- **Agent / spend:** model `claude-sonnet`, status OK, $3.6642 of $5 (n_calls 624, errors 0).

## $0 headroom pre-gate (break-even cells)

Of n=406 break-even queries (base retrieval-success 0.4557), the MAX lift any reranker could realize (all-gold reachable in the shown window but not top-10 under ce):

- depth-1 ceiling (top-20→top-10): **0.1182**
- depth-2 ceiling (top-40→top-10): **0.2094**

## Arm 1 — reranking lift (PRIMARY, decisive)

- n=406 ({'needle': 194, 'multi_session': 103, 'temporal': 109}); ce retrieval-success 0.4557 → agent depth-1 0.4631.
- **reranking lift (agent − ce, paired):** **0.0074 [-0.0074,0.0222]** (n=406).
- mechanism: promoted 6 gold into top-10, demoted 3 out.

**Lift NET of round-trip cost (decisive KILL/GO number):**

| c_rt (per round-trip) | net lift point | net lift CI | GO? |
|---|---|---|---|
| 0.00 | 0.0074 | [-0.0074,0.0222] | False |
| 0.02 | -0.0126 | [-0.0274,0.0022] | False |
| 0.05 | -0.0426 | [-0.0574,-0.0278] | False |
| 0.10 | -0.0926 | [-0.1074,-0.0778] | False |

**Depth-1 lift by intent:**

| intent | n | ce rc | agent rc | lift [CI] |
|---|---|---|---|---|
| needle | 194 | 0.5515 | 0.5619 | 0.0103 [-0.0103,0.0309] |
| multi_session | 103 | 0.3301 | 0.3398 | 0.0097 [-0.0291,0.0488] |
| temporal | 109 | 0.4037 | 0.4037 | 0.0 [0.0,0.0] |

## Arm 2 — detection lift (Slice-25-comparable)

Does the stronger agent's top-1 relevance flag beat `ce_top` at predicting retrieval-success? (Slice-25 cheap agent: lift −0.138.)

- agent top-1 relevance rate 0.1379; balanced-acc agent 0.4974 vs ce@best 0.5255.
- **detection lift (agent − ce, paired acc):** **-0.0296 [-0.0715,0.0123]** (n=406); AUC ce 0.4515 vs agent(binary) 0.4974.
- vs Slice-25 cheap-agent lift -0.1378.

## Arm 3 — one-shot vs iterative (depth 1 vs 2)

- depth-2 re-plan trigger rate 0.5369 (~1.537 round-trips/query); recovered 6 extra gold.
- **incremental lift (depth-2 − depth-1):** 0.0148 [0.0049,0.0271].
- total lift depth-2 vs ce: 0.0222 [0.0049,0.0395].

**Depth-2 lift NET of round-trip:**

| c_rt | net lift point | net lift CI | GO? |
|---|---|---|---|
| 0.00 | 0.0222 | [0.0049,0.0395] | True |
| 0.02 | -0.0085 | [-0.0258,0.0088] | False |
| 0.05 | -0.0546 | [-0.0719,-0.0373] | False |
| 0.10 | -0.1315 | [-0.1488,-0.1142] | False |

## KILL/GO verdict (HITL #4)

- **DECISION: KILL**
- decisive number — depth-1 reranking lift NET of one round-trip (c_rt=0.02): **-0.0126 [-0.0274,0.0022]**, GO=False.
- rule: GO iff the depth-1 reranking-lift CI lower bound, NET of one round-trip (c_rt=0.02 accuracy-equivalent), exceeds 0.
- **recommended depth:** 2 — depth-2 (one re-plan) recovers gold beyond depth-1 net of its doubled round-trip
- detection: stronger-agent lift {'point': -0.0296, 'lo': -0.0715, 'hi': 0.0123, 'n': 406} vs Slice-25 cheap-agent -0.1378.
- **implications:** KILL — even a stronger agent on the break-even cells does NOT beat ce_score net of round-trip. The L2 prototype (Slice 35) DROPS the agent-signal loop (feedback_arm=False; router stays on internal ce_score); record_feedback STAYS instrumentation (overrides any F-8b promote).

