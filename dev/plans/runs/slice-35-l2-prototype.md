# Slice 35 — Agent-side L2 router prototype + dispatcher pre-stage (hand-off)

> 0.8.11 final slice + the **keystone hand-off** for the 0.8.15 dispatcher.
> CALLER-SIDE, `$0`, no LLM. Code: `dev/prototypes/l2-router/`
> (`router.py`, `registry.json`, `build_registry.py`, `test_smoke.py`, `README.md`).
>
> Binding: `dev/plans/0.8.11-implementation.md` §3 (tuple format) + §4 (Recommendation
> API); PSD `dev/design/planner-router-psd-0.8.x.md` §II.A / §II.B.

## Honest caveat (read first)

The registry tuples are **0.8.11 SCREENING DATA — NOT validated config**: a
single-corpus LME measurement (needle/multi_session/temporal), rerank knobs
**derived** from the landed 0.8.3 CE-pass (the in-session `.venv` build had the CE
block gated OFF), **2 of 5 classes provisional** EXP-0-global pins (no per-class
measurement), and a **lexical** fallback classifier (lower-bound proxy, macro
0.768). **0.8.15 must re-validate every tuple** (fresh `default-reranker` CE build;
LOCOMO/MuSiQue corroboration; AP-News `decide_084` win-rate for `global`). This is
carried verbatim in `registry.json` `confidence_header` so the hand-off cannot
mislead.

## The API (contract §4)

`recommend(query, *, agent_hint=None) -> Recommendation` — **recommends WITHOUT
executing retrieval**. `Recommendation` is a frozen dataclass:

| field | meaning |
| --- | --- |
| `intent` | one of the 5 classes (from `agent_hint`, else the classifier) |
| `stack` | operator chain from the intent's EXP-B' tuple |
| `config` | `(index, retrieval, alpha, pool_n, mmr, recency, forbidden_ops)` (§3) |
| `confidence` | classifier confidence (`1.0` if `agent_hint` overrides) |
| `cost_tier` | `low`/`medium`/`high` from Gate-2 per-arm cost tiers |
| `rationale` | chosen stack + provenance + config + the screening caveat |
| `feedback_arm` | **always `False`** (EXP-AF KILL) |

**Intent resolution preference order (PSD §II.A):** (1) `agent_hint` → verbatim,
`confidence=1.0`, **no** classifier fallback (R-L2-3); (2) internal lexical
TF-IDF nearest-centroid classifier (mirrors Slice-20; fallback only).
Provider-callback (PSD preference #2) is out of scope for a `$0` caller-side proto.

## Registry contents (the 0.8.15 pre-stage artifact, DP-B / R-L2-2)

`cost_tier` = bucket of the worst per-arm Gate-2 tier across the stack
(`ce_rerank`=medium ⇒ retrieval intents medium; `map_reduce_qfs`=high ⇒ global high).

| intent | provenance | source_exp | stack tail | alpha | pool_n | cand_k | cost_tier | r@10 |
| --- | --- | --- | --- | ---: | ---: | ---: | --- | --- |
| needle | **measured** | EXP-B' | …rrf, ce_rerank | 0.7 | 50 | 200 | medium | 0.644 [0.59,0.70] |
| multi_session | **measured** | EXP-B' | …rrf, ce_rerank | 1.0 | 100 | 300 | medium | 0.467 [0.39,0.55] |
| temporal | **measured** | EXP-B' | …rrf, ce_rerank | 1.0 | 20 | 500 | medium | 0.513 [0.43,0.59] |
| global | *provisional* | EXP-0-global | …map_reduce_qfs, community_summary | 0.3 | 10 | 200 | high | n/a (decide_084) |
| multi_hop | *provisional* | EXP-0-global | …rrf, ce_rerank | 0.3 | 10 | 200 | medium | n/a (no CE pass) |

**Measured vs provisional split: 3 measured (needle/multi_session/temporal), 2
provisional (global/multi_hop).** Provisional reasons: `global` is reference-free
(win-rate axis, no node-level labels by design); `multi_hop` has node-level gold
but no fresh fused+CE pass (same feature-off rerank build blocker) → EXP-0 pin.

## feedback_arm = False — EXP-AF KILL rationale

EXP-AF (Slice 30, HITL #4) = **KILL**. Even a stronger agent (claude-sonnet) on
the break-even cells did **not** beat internal `ce_score` net of round-trip:
depth-1 reranking lift net of one round-trip (`c_rt=0.02`) = **−0.0126
[−0.0274, 0.0022]**, GO=False. ⇒ The prototype **drops the agent-signal loop**;
the router stays on internal `ce_score`; `feedback_arm=False` is hard-wired (never
`True`). `record_feedback` stays instrumentation (overrides any F-8b promote).

## Forbidden-composition encoding (EXP-B'.5 → the 0.8.15 validator seam)

The registry carries the full `forbidden_composition_guard.router_isolation_rule`
from EXP-B'. `L2Router.check_forbidden(intent, stack)` raises
`ForbiddenCompositionError` if a stack uses an op forbidden for the intent.
`recommend()` calls it on every emitted stack. Rule: `map_reduce_qfs` /
`community_summary` are valid **only for `global`** and **forbidden** on
needle/multi_session/temporal/multi_hop — the §II.B blind-distiller cross-wire,
confirmed by EXP-Fr-acc base (needle→C = **−0.300 [−0.47, −0.10]** at 8
distractors ≈ the prior −0.362). The 0.8.15 plan validator inherits this seam.

## Smoke-test result (`$0`, real run)

`.venv/bin/python dev/prototypes/l2-router/test_smoke.py` → **42 passed, 0
failed** (exit 0). Covers: R-L2-1 (all 5 classes return a Recommendation), R-L2-2
(each carries a registered tuple + valid cost_tier), R-L2-3 (`agent_hint` verbatim
at confidence 1.0, overrides query text, unknown hint raises — no silent
fallback), R-L2-4 (`fathomdb` never in `sys.modules`; no retrieval executed), the
forbidden-composition refusal (needle refuses `map_reduce_qfs`; global allows it),
and `feedback_arm is False` on every class. The internal classifier also resolves
the 5 example queries to their expected classes (needle/global/multi_hop/temporal/
multi_session).

## R-L2 requirements met

- **R-L2-1** all 5 classes route → Recommendation ✓
- **R-L2-2** each carries a registered EXP-B' tuple (registry = 0.8.15 pre-stage) ✓
- **R-L2-3** `agent_hint` override verbatim, no classifier fallback ✓
- **R-L2-4** CALLER-SIDE: zero changes to `src/rust` / `src/python/fathomdb` /
  `src/ts`; no `fathomdb` import; recommends without executing ✓

## Deferrals (carried to 0.8.15)

- Re-validate all 5 tuples with a fresh `default-reranker` CE build; convert
  `global`/`multi_hop` from provisional to measured.
- Provider-callback intent path (PSD preference #2) — not built in this `$0` proto.
- The lexical fallback classifier is a lower-bound proxy; an embedding-based
  classifier (no torch/sklearn in env) is the upgrade path.
