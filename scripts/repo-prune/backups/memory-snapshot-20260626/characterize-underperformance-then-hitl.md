---
name: characterize-underperformance-then-hitl
description: "When FathomDB under-performs a competitor/benchmark, characterize the gap (real vs artifact, mechanism, levers) and bring it to HITL — don't accept the loss or pick a fork solo."
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 1a66de90-c67e-434a-a0e5-9ae699d3289c
---

When confronted with FathomDB under-performance (a competitor win, a missed floor, a benchmark
loss), the required move is: **characterize it, then use HITL.** Decompose the gap into structural/
real vs measurement-artifact, name the mechanism, and enumerate the concrete adjustment levers with
their cost + footprint impact — then bring that to the human for the spend/direction decision. Do
**not** accept the loss as final, and do **not** pick a fork solo.

**Why:** the 0.8.4 GraphRAG loss looked decisive (comprehensiveness win-rate 0.00) but a mechanism
read showed it was *part* real (flat top-K can't cover a global question) and *part* self-inflicted
artifact (FathomDB reader capped at 600 tokens vs GraphRAG's ~1500; top-K hardcoded at 8). The HITL
was "not ready to take the loss" and surfaced fixable levers I'd under-weighted. Recording an
overstated gap, or blank-checking a big build, would both have been wrong.

**How to apply:** on any under-performance result — (1) split decisive-but-real from
hobbled-ourselves; (2) name the mechanism in the competitor's favor; (3) list the cheap fairness-fixes
AND the heavier capability builds, each with $ + footprint cost; (4) present to HITL and let them
choose what to "spend" on function. See [[fathomdb-function-over-footprint-user-spend]] (function can
outweigh the small/local goal) and [[perf-tuning-design-sweeps-not-adhoc]].
