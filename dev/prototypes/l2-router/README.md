# Agent-side L2 router prototype (0.8.11 Slice 35)

CALLER-SIDE, `$0`, no LLM. A working agent-side query router that **recommends a
retrieval stack per query WITHOUT executing it**, plus the committed **per-intent
tuple registry** the 0.8.15 dispatcher integrates as its pre-stage artifact.

> **SCREENING DATA — NOT VALIDATED CONFIG.** The tuples are 0.8.11 screening
> results (single-corpus LME; rerank knobs derived from the 0.8.3 CE-pass; 2 of 5
> classes provisional EXP-0 pins; lexical fallback classifier). **0.8.15 must
> re-validate every tuple.** See `registry.json` `confidence_header`.

## Footprint (R-L2-4)

Lives **outside** the shipped library. Imports **nothing** from `fathomdb`
(engine/SDK); runs no retrieval. Zero diff to `src/rust` / `src/python/fathomdb`
/ `src/ts`. The smoke test asserts `fathomdb` is never imported.

## Files

- `router.py` — `L2Router.recommend(query, *, agent_hint=None) -> Recommendation`
  (frozen dataclass: `intent, stack, config, confidence, cost_tier, rationale,
  feedback_arm`). Plus `check_forbidden()` (the EXP-B'.5 router-isolation
  validator seam the dispatcher inherits) and the internal lexical classifier.
- `registry.json` — the per-intent tuple registry (DP-B / R-L2-2). Generated, not
  hand-typed.
- `build_registry.py` — regenerates `registry.json` from the committed experiment
  outputs (`expb-joint-tune-output.json` + `gate2-oracle-output.json`).
- `test_smoke.py` — `$0` smoke test (R-L2-1/2/3/4 + forbidden refusal).

## Intent resolution (PSD §II.A)

1. `agent_hint` if given → used **verbatim**, `confidence=1.0`, **no** fallback to
   the classifier (R-L2-3). The agent owns intent.
2. internal lexical TF-IDF nearest-centroid classifier (mirrors Slice-20; a
   lower-bound proxy, measured macro 0.768) — fallback only.

## feedback_arm = False (EXP-AF KILL)

EXP-AF (Slice 30) = **KILL**: even a stronger agent did not beat internal
`ce_score` net of round-trip (depth-1 lift net of one round-trip = −0.0126
[−0.0274, 0.0022]). The router stays on internal `ce_score`; there is **no**
agent-signal escalation loop. `record_feedback` stays instrumentation.

## Run

```bash
.venv/bin/python dev/prototypes/l2-router/build_registry.py   # regenerate registry
.venv/bin/python dev/prototypes/l2-router/test_smoke.py       # 42 checks, $0
```
