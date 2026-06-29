# EXP-Fr-acc / VoI finalize (0.8.11 Slice 25)

> Three deliverables (PSD §III.D) extending the Slice-20 base. **Real measured numbers** — the CE reranker is ACTIVE (`default-reranker`); `ce_score` is real, confirmed by a degeneracy guard before any measurement.

- **CE-active guard:** max ce_norm=0.999944, spread=0.999935, alpha=1.0 reorders relevant→rank1 (order=[0, 1, 2]). PASS.
- **Cost model:** needle→C cross-wire **-0.3** (Slice-20 deep), same-tier -0.05, retrieval-failure 1.0.
- **Route classifier (LME held out):** routing acc 0.6419 over 606 queries; costly needle→global cross-wire produced 4 times (1.0).
- **Spend:** $0.0151 (ceiling $3). **measured agent p_catch:** 0.5445; VoI landscape at oracle p_catch=1.0.

## Deliverable 1 — value-of-signal (agent relevance vs internal `ce_score`)

- n=450 ({'needle': 150, 'multi_session': 150, 'temporal': 150}); base retrieval-correct rate 0.5289; agent says RELEVANT 0.1556.
- **agent accuracy** 0.5244 [0.4778,0.5689] vs **ce_score@best-threshold** (0.0336) 0.6622 [0.6178,0.7044].
- **LIFT (agent − ce, paired):** **-0.1378 [-0.1889,-0.0867]** (n=450). AUC: ce=0.6668, agent(binary)=0.5445.
- balanced-acc: agent 0.5445, ce@best 0.6675, ce@0.5 0.6383.
- *Caveat:* Conservative LOWER BOUND on agent value: (1) the ce_score baseline gets an in-sample oracle threshold (favors ce); (2) the eval agent sees only the top-1 passage, NOT the user-intent context a deployed agent holds (PSD §I.D — the agent's value is the intent FathomDB lacks). A cheap general LLM re-judging relevance is exactly the task the engine's specialized cross-encoder already does well, so it losing here is expected. EXP-AF (Slice 30) is the dedicated stronger-agent / record_feedback test.

## Deliverable 2 — ask-or-not VoI break-even

VoI(cell) = p_catch · E[loss if proceed] ; ask iff > c_rt. Reported at the **oracle upper bound p_catch=1.0** (the max a correct-signal agent could save); cost_wrong=1.0. *Realized value with the measured cheap agent is negative — see deliverable 1 / KILL.* measured agent p_catch=0.5445.

**Ask-region size vs round-trip cost (c_rt, accuracy-equivalent; ORACLE upper bound):**

| c_rt | ask-region query-fraction | #ask cells |
|---|---|---|
| 0.00 | 0.9983 | 15 |
| 0.02 | 0.9835 | 14 |
| 0.05 | 0.9818 | 13 |
| 0.10 | 0.9818 | 13 |

**ce_top break-even by route-margin bin (ask iff ce_top below the edge) at c_rt=0.02:**

| route-margin bin | ce_top break-even (ask below) |
|---|---|
| [0.0,0.05) | 1.01 |
| [0.05,0.1) | 1.01 |
| [0.1,0.2) | 1.01 |
| [0.2,0.4) | 0.2 |

**Representative cells (highest expected-cost-saved):**

| ce_top bin | margin bin | n | P(ret incorrect) | E[misroute] | E[loss] | cost-saved | ask@0.02 |
|---|---|---|---|---|---|---|---|
| [0.0, 0.2] | [0.2, 0.4] | 1 | 1.0 | 0.0 | 1.0 | 1.0 | True |
| [0.2, 0.4] | [0.1, 0.2] | 1 | 1.0 | 0.0 | 1.0 | 1.0 | True |
| [0.0, 0.2] | [0.1, 0.2] | 25 | 0.68 | 0.042 | 0.722 | 0.722 | True |
| [0.0, 0.2] | [0.0, 0.05] | 280 | 0.5464 | 0.0212 | 0.5677 | 0.5677 | True |
| [0.0, 0.2] | [0.05, 0.1] | 100 | 0.5 | 0.0135 | 0.5135 | 0.5135 | True |
| [0.4, 0.6] | [0.0, 0.05] | 17 | 0.4706 | 0.0176 | 0.4882 | 0.4882 | True |
| [0.6, 0.8] | [0.05, 0.1] | 6 | 0.3333 | 0.025 | 0.3583 | 0.3583 | True |
| [0.6, 0.8] | [0.0, 0.05] | 13 | 0.2308 | 0.0115 | 0.2423 | 0.2423 | True |

## Deliverable 3 — asymmetric weighting (needle→C cross-wire vs cheap same-tier)

Isolating the mis-route term (ask iff p_catch·|cost| > c_rt) at p_catch=1.0, measured cost ratio **6.0×**:

| mis-route type | |cost| | ask-threshold c_rt* (= p_catch·|cost|) |
|---|---|---|
| cross-wire → C (needle→global) | 0.3 | 0.3 |
| same-tier (retrieval↔retrieval) | 0.05 | 0.05 |

- For any round-trip cost c_rt in (0.05, 0.3] the mis-route VoI policy ASKS to block a cross-wire but DECLINES to pay for a same-tier miss → 6.0× preferential suppression of the needle→C cross-wire.
- realized mis-routes: 4 cross-wire-to-C, 213 same-tier (cross-wire share 0.0184); runner-up-`global` cross-wire-exposed queries: 2.
- asymmetric weighting **CONFIRMED**. Asymmetric weighting CONFIRMED via the measured 6× cost ratio: the ask-threshold for a cross-wire-exposed query is 6× more lenient than for a same-tier miss, so the policy suppresses needle→C preferentially. (NB: the dominant VoI term overall is retrieval-failure detection via low ce_top — the cross-wire is rare but, when exposed, carries the heaviest single ask-incentive.)

## KILL check

- **QUALIFIED KILL (cheap agent) — gemini-flash-lite relevance is DOMINATED by the free internal ce_score (negative lift); ask-or-not buys nothing with this agent, so route on internal ce_score only. The break-even LANDSCAPE (low-ce + needle→C cross-wire cells) shows where a STRONGER agent's round-trip could pay — hand that shape to EXP-AF (Slice 30) to test a stronger agent / record_feedback before committing the agent-signal loop.**
- measured agent (gemini-flash-lite) beats internal ce_score: **False** (lift {'point': -0.1378, 'lo': -0.1889, 'hi': -0.0867, 'n': 450}).
- potential break-even region exists for a stronger agent: True → EXP-AF (Slice 30).

