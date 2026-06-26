---
name: priced-runs-need-resilience-before-spend
description: "Before ANY long/priced/rate-limited run, mandate incremental checkpointing + backoff-and-recover + window-fit as hard preconditions — cheap-validate is not enough."
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 96e4e413-ca17-4a36-a348-762259ee12c5
---

A priced, long-running, rate-limited run (e.g. an LLM-answerer eval pass over thousands of calls) must be
made **idempotent, checkpointed, and self-recovering BEFORE the first priced call** — this is a gate, not
a nice-to-have. In 0.8.2 Slice 20 the orchestrator (me) skipped it and burned **~$15** across three avoidable
failures.

**The three failures (all orchestration, not infra):**
1. **End-only persistence.** The prompt said "persist paired_records" but it wrote only at the END of a
   successful run, so a mid-run process death lost ~160 completed answers (~$4). Required: persist **every
   cell as it lands** (incremental checkpoint, e.g. every N items → `*.checkpoint.json`) so any death is
   resumable.
2. **No 429 backoff from the start.** Exponential-backoff-and-recover on 429/5xx was added only AFTER the
   first $5.71 rate-limit storm. A priced call against a quota-limited endpoint must never go out without
   retry-with-backoff already in place; throttle concurrency (workers ≤ 3-4) too.
3. **Ignored the runtime window.** Launched a ~1.5h run into a ~60-min background-process wall-clock limit
   with no chunking/resume → structurally guaranteed to die and (without checkpoints) restart from scratch,
   looping and re-spending.

**Why:** cheap-validate (which I did do — [[0.8.1-budget-discipline-cheap-validate-and-ledger]]) validates
field population + wiring, NOT the run's resilience under partial failure. The expensive run is exactly
where idempotency/checkpointing/recovery matter, and it's the one place a missing guard costs real money.

**How to apply (precondition checklist before spawning any priced/long run):**
- Incremental checkpoint of results to disk every N items; the runner has a `--resume <checkpoint>` that
  re-calls ONLY missing/None cells (verify the round-trip in a test, not just that the field exists).
- Retry-with-exponential-backoff on 429/5xx/timeouts so a transient error recovers the cell, not abstains it;
  low concurrency to avoid tripping quotas.
- Estimate wall-clock vs the environment's process/agent time limit; if the run can exceed it, CHUNK it so
  each chunk fits the window and resumes — or run it scoped (e.g. only the primary cell).
- A validity guard (answer_completeness ≥ floor) that marks a partial run INVALID rather than emitting a
  verdict from deflated data.
- Watch for runaway re-launch loops: a "completed" sub-agent woken by its own monitor can relaunch + re-spend;
  killing the process AND its monitor(s) is needed to break it.
