# Experiment definitions (procedures only — NO results)

This document defines WHAT was done and HOW it was measured. It deliberately contains
no outcomes, numbers, or conclusions. Use it with `DATA-MAP.md` to locate the raw data.

## Measurement method (applies to all experiments)

- Work is performed by subagents spawned from an orchestrator. A subagent is spawned
  once; it can be re-addressed later ("resumed"), which replays its persisted
  transcript. Each subagent's full conversation is recorded as a JSONL transcript.
- Cost is measured from each transcript's per-turn `message.usage`, which reports REAL
  billed tokens split into four classes: `input_tokens` (fresh uncached input),
  `cache_creation_input_tokens` (cache write), `cache_read_input_tokens` (cache hit),
  `output_tokens`. (A 5-minute / 1-hour ephemeral-cache split is also present.)
- `parse_usage.py` segments a transcript by prompt turn (the initial spawn prompt and
  each subsequent re-address each open a new segment) and sums the four token classes
  per segment, plus a dollar estimate at Opus public rates ($/M: input 15, output 75,
  cache_write 18.75, cache_read 1.50). Rates are parameterized; ratios are model-robust.
- Wall-clock per operation is available from transcript timestamps and from the runner's
  per-task `duration_ms`.
- n = 1 per cell (single trial). Treat all values as point estimates.
- Payloads are synthetic, controlled-size text files (`payloads/p*.txt`) plus three real
  source files used as a "domain" context (a Rust pyo3 binding crate manifest, its
  ~1489-line `lib.rs`, and a status markdown board).

## Hypotheses under test (a-priori claims; NOT findings)

- **H1 — Persistence.** A resident subagent retains the full context it read and can
  answer later, re-addressed follow-ups WITHOUT re-reading, across multiple turns and
  across an idle gap.
- **H2 — Warm reuse is cheaper.** Re-addressing an existing (warm) resident to do an
  incremental task costs less than spawning a fresh subagent for that task.
- **H3 — Amortization / crossover.** Over K tasks against the same context, loading one
  resident and reusing it beats spawning a fresh subagent per task beyond some K.
- **H4 — Keep transcript small.** A resident holding a smaller transcript is cheaper to
  reuse than one holding a larger transcript of the same information.
- **H5 — Cheap distillation.** Producing a distilled summary from a resident that
  already holds the raw context is cheap (priced like a single large-output follow-up),
  making distillation an inexpensive route to a small-transcript resident.

## Round 1 — persistence + baseline

- **r1-bg-resident:** spawn a background resident; it reads three real files (A=Cargo
  manifest, B=lib.rs, C=status board) and reports readiness. Then send it TWO follow-up
  messages by re-address, each a cross-reference task (FU1 = A↔B; FU2 = C↔A), WITHOUT
  re-supplying any file content. Record whether it answers from retained context and the
  per-segment cost.
- **r1-control-fresh:** one fresh subagent reads all three files and performs BOTH
  cross-references in a single pass (the no-reuse baseline).
- **r1-fg-resident:** spawn a FOREGROUND resident (reads the three files), then
  re-address it once with a cross-reference follow-up (tests whether a foreground-spawned
  agent is re-addressable and retains context).

## Round 2 — spawn cost, reuse accretion, cache expiry

- **E1 (r2-e1-1k / -10k / -61k / -154k):** four independent fresh subagents, each reads
  ONE synthetic payload of the named approximate token size and returns its final line +
  line count. Manipulated variable: payload size P. Outcome: cold-spawn cost vs P.
- **E2 (r2-e2e3-resident, segments load + FU1..FU3):** one resident loads a ~10k payload,
  then receives three immediate, back-to-back re-address follow-ups (each: recite a
  specific line from the held payload). Outcome: marginal cost of successive warm reuses
  and how it changes as the transcript accretes.
- **E3 (same r2-e2e3-resident, FU4..FU5):** after FU3, an idle gap of ~6.2 minutes (i.e.
  longer than the ~5-minute prompt-cache TTL) elapses with no activity, then a follow-up
  (FU4), then an immediate further follow-up (FU5). Outcome: cost of reuse after the
  cache has had time to expire, vs an immediately-following reuse.

## Round 3 — high-output work, context overlap, specialist vs general

- **high-W (r3-rw-highw-general, load + FU1..FU2):** a resident loads a ~10k payload,
  then receives two follow-ups each requesting a ~600-word structured analysis of the
  held payload (large output). **r3-highw-fresh:** a fresh subagent performs one
  equivalent ~600-word analysis after reading the payload from scratch. Outcome: cost of
  high-output reuse vs high-output fresh spawn.
- **overlap / routing (one shared "domain question": pyo3 version + which Cargo feature
  is cfg-gated):**
  - **r3-rs-specialist:** a resident that has loaded the three real DOMAIN files. The
    domain question is asked of it (a) as its first reuse (idle since load) and (b) again
    after it is warm. It is also asked an H≈50% task (use held files for part 1, read one
    NEW file for part 2).
  - **r3-rw-highw-general:** after its high-W follow-ups, the SAME domain question is
    routed to it even though it holds an UNRELATED payload (H≈0 overlap → it must read the
    domain files).
  - **r3-domain-fresh:** a fresh subagent answers the domain question from scratch
    (baseline). Outcome: cost of answering the same question via warm-specialist vs
    cold-specialist vs warm-but-non-overlapping-general vs fresh.

## Round 4 — distillation (small vs large transcript) + fidelity

- **r4-distiller:** a fresh subagent reads the three real domain files (~60k tokens) and
  WRITES a dense distilled technical summary (~9k tokens) to disk
  (`payloads/distilled-domain.md`). Outcome: one-time cost to produce the summary.
- **r4-distilled-resident:** a resident loads the ~9k distilled summary.
  **r4r5-raw-resident-and-emit (Round-4 portion):** a resident loads the ~60k raw files.
  Each resident then receives: a warm-up ping, an identical 3-part fidelity query (list
  the Cargo features and which is cfg-gated; the gil_used value + three test-hooks
  function names; the root exception type + two two-level-deep nested exception types),
  then after a ~6.2-min idle gap a cold-wake query. Outcome: per-operation cost vs
  transcript size T, plus whether each resident answers the fidelity probes correctly.

## Round 5 — cheap-distillation break-even

- **r4r5-raw-resident-and-emit (Round-5 portion, final segment):** the resident that
  already holds the ~60k raw files is asked to WRITE its own ~33k-byte distilled summary
  to disk (`payloads/distilled-piggyback.md`) FROM HELD MEMORY (without re-reading the
  originals). Outcome: cost of producing the summary from an already-loaded resident.
- **r5-piggyback-loaded:** a fresh resident loads that self-emitted summary and is asked
  the 3-part fidelity query. Outcome: load cost + whether the self-emitted summary
  preserves the probed facts. (Compare emit cost to r4-distiller; compute the reuse count
  at which paying to distil is repaid by cheaper subsequent queries.)
