---
name: perf-tuning-design-sweeps-not-adhoc
description: "For throughput/perf tuning that involves infra knobs the user controls (e.g. AIRLOCK_VLLM_BATCH_CONCURRENCY), design a controlled sweep and direct the user — do not improvise one-off timing probes"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 54df08ae-c01b-4eae-8862-514eb2cfd198
---

When tuning throughput/perf and the lever is an infra knob the HITL controls (server
env + restart), the user wants a **designed experiment**: a controlled sweep with a
fixed workload, a table of {what to change, to what value, when}, and a decision rule —
NOT an ad-hoc single timing probe. The user interrupted a one-off 32-session timing run
and asked for a batch-size/concurrency sweep plan instead (2026-06-15).

**Why:** the user owns the knob (`AIRLOCK_VLLM_BATCH_CONCURRENCY` + `systemctl --user
restart airlock`); a one-off probe at the default doesn't find the optimum and wastes a
run. A sweep with a held-constant workload isolates the variable and lands a real answer.

**How to apply:** for perf tuning, present (1) the knob + how to change it, (2) a fixed
workload, (3) a trial table (value → restart → I measure), (4) a decision rule (pick the
knee: lowest setting at the throughput plateau with zero errors), (5) a clear hand-off
protocol (user sets+restarts+confirms, I run the timed trial). Watch for error-lines /
throughput collapse = past the knee. Related: [[c1-graph-arm-seeding-live]] (this tuning
is for the Qwen3.6-27B graph-extraction throughput), [[0.8.1-budget-discipline-cheap-validate-and-ledger]].
