---
title: ADR-0.7.0-text-query-latency-gates-revised
date: 2026-05-25
target_release: 0.7.0
desc: Revised AC-012 / AC-013 / AC-019 / AC-020 latency budgets, pinned empirically against canonical-runner measurements at 1M corpus on tier-1 x86_64 ubuntu-latest. Supersedes ADR-0.6.0-text-query-latency-gates for AC-012.
blast_radius: dev/test-plan.md § Current Perf Attribution; src/rust/crates/fathomdb-engine/tests/perf_gates.rs; .github/workflows/perf-canonical.yml; CI perf gate; dev/requirements.md REQ-010
status: draft, HITL-required
---

# ADR-0.7.0 — Text-query latency gates (revised)

**Status:** draft, HITL-required.

This is the 0.7.0 successor to
`dev/adr/ADR-0.6.0-text-query-latency-gates.md`. It revises the
AC-012 numeric budgets against the empirical canonical-runner
measurement taken at 0.6.1 close
([workflow run 26346417896](https://github.com/coreyt/fathomdb/actions/runs/26346417896),
2026-05-23), and (in omnibus form per § Scope below) does the same
for AC-013, AC-019, and AC-020 where their root causes differ.

Per the 0.7.0 release line goal (perf-only; HITL 2026-05-24): the
budgets are pinned against measurements on tier-1 canonical hardware,
**not** against theoretical "warm-cache" reasoning. Tight enough to
catch real regressions; achievable on the architecture as it stands
once the 0.7.0 engine-side levers (top-K `LIMIT` cap, PRAGMA sweep,
AC-020 architectural lever per
`ADR-0.7.0-ac020-architectural-lever`) have landed.

## Status / context

`ADR-0.6.0-text-query-latency-gates` (accepted 2026-04-27) set the
AC-012 budget at **p50 ≤ 20 ms; p99 ≤ 150 ms**. The 0.6.0 budget
was set without an empirical canonical-runner measurement: Pack D
(2026-05-05) measured AC-012 only on the aarch64 Tegra dev runner
at N=100,000 and projected forward to N=1,000,000 on x86_64 via
"warm cache" intuition.

The 0.6.1 release line included an `AC012-measure` slice that ran
the canonical-runner workflow (`perf-canonical.yml`) on the
authoritative tier-1 target
(`x86_64-unknown-linux-gnu`, ubuntu-latest, AMD EPYC 9V74,
4 cores, glibc 2.39, SQLite 3.45.x via `libsqlite3-sys 0.28.0`,
rustc 1.95.0) at the ADR-specified canonical scale (N=1,000,000,
1000 samples, warm cache, QPS=1, single-process, no concurrent
writes). The full transcript lives at
`dev/notes/perf-canonical-runner-2026-MM.md`; the machine-readable
record lives at `dev/plans/runs/0.6.1-AC012-measure-output.json`.

**Empirical result (the baseline this ADR pins against):**

| Metric     | Observed (ms) | 0.6.0 budget (ms) | Verdict | Multiple over budget |
| ---------- | ------------- | ----------------- | ------- | -------------------- |
| AC-012 p50 | 140.95        | ≤ 20              | **RED** | 7.05×                |
| AC-012 p99 | 458           | ≤ 150             | **RED** | 3.05×                |

The Pack 7 un-defer trigger
(`dev/plans/0.6.0-implementation.md` L254-269, "regression > 20%
vs ADR-pinned budget") fires by ~30×. The 0.6.0 budgets cannot
be honestly defended against the canonical-runner numbers — they
were "theoretical, warm-cache" numbers and the measurement is the
ground truth they did not have. HITL 2026-05-24 escalated the
budget revision (and the engine-side tuning that closes the
revised gap) to 0.7.0 because patch-release contract
(`ADR-0.6.0-no-shims-policy` § 54: "no API breaks. Bugfix-only.")
forbids ADR budget revision in 0.6.x.

AC-020 has a separate root cause (concurrency-mutex contention,
not single-thread FTS5 latency). Its budget and the architectural
lever that closes it are spelled out in
`ADR-0.7.0-ac020-architectural-lever`. This ADR addresses AC-020's
numeric envelope only in the omnibus shape (the sequential /
concurrent absolute-ms numbers); the mechanism is owned by the
AC-020 ADR.

AC-013 (vector retrieval latency) and AC-019 (concurrent-mixed
read tail latency) currently inherit AC-013's bulk-vec seed cost
and have not been measured at canonical scale (see
`dev/test-plan.md` L125-134). This ADR proposes their revised
budgets contingent on a measurement pass landing in the
`0.7.0-PERF-DIAG` slice; if measurement shows AC-013 / AC-019
share root cause with AC-012, this ADR is the omnibus; if they
diverge, parallel successor ADRs
(`ADR-0.7.0-ac013-retrieval-latency-gates-revised`,
`ADR-0.7.0-ac019-concurrent-mixed-latency-gates-revised`) split
out under the same revision protocol.

## Scope

Supersedes for AC-012. Omnibus shape for AC-013 / AC-019 / AC-020
**conditional on the DIAG-slice measurements** showing the same
root cause. If the diagnostic pack falsifies the common-cause
hypothesis, this ADR remains AC-012-only and parallel successor
ADRs cover AC-013 / AC-019 / AC-020.

The HITL Q1 decision (see `dev/plans/0.7.0-implementation.md`
§ Open HITL decisions) selects between:

- **Omnibus.** Single ADR covers all four ACs with the revised
  numbers below.
- **Split.** This ADR covers AC-012 only; three parallel successor
  ADRs cover AC-013 / AC-019 / AC-020.

Recommendation (draft): **split**, because AC-020's lever is
architectural-class (PCACHE2 / WAL2 / R-W split / vendor swap per
the AC-020 ADR) and its budget needs to compose with the chosen
lever's expected gain, while AC-012's lever is engine-side PRAGMA
sweep + top-K `LIMIT` cap. The two budget envelopes have different
prerequisites and a single ADR would hold up AC-012's RED-test
author-step waiting on AC-020's lever choice.

## Decision

> **HITL Q1 fills the numbers.** This draft proposes
> placeholder revised budgets so the slice structure is reviewable;
> the actual numbers are chosen at HITL Q1 lock, informed by the
> `0.7.0-PERF-DIAG` slice output. The proposals below are the
> drafter's recommendation, not the lock.

### AC-012 — text-query single-token / phrase MATCH latency

**Proposed revised budget (HITL Q1):**

- **p50 ≤ 50 ms** (was 20 ms; allows ~3× headroom over the 0.6.1
  measurement of 140.95 ms after the projected gain from PRAGMA
  sweep + top-K `LIMIT` cap closes the gap).
- **p99 ≤ 200 ms** (was 150 ms; allows tail headroom against
  canonical-runner scheduler jitter at 1M scale; ~2× headroom over
  the 0.6.1 measurement of 458 ms once the levers close the gap).

The revised budget is **lever-contingent**: a measurement-only
RED-test asserting these numbers cannot ship until the
`0.7.0-PERF-PRAGMA` slice lands its lever. The slice sequence
(`dev/plans/0.7.0-implementation.md` § Slice sequence) encodes
this dependency.

**Alternatives considered for AC-012 (HITL Q1 picks one):**

- **A — Aggressive (p50 ≤ 30 ms; p99 ≤ 100 ms).** Tighter; assumes
  PRAGMA + LIMIT close more than the diagnostic pack predicts.
  Risk: a second lever round if the first underdelivers.
- **B — Moderate (p50 ≤ 50 ms; p99 ≤ 200 ms).** Drafter's
  recommendation. Lands in a single lever round; preserves catch-
  any-real-regression discipline.
- **C — Conservative (p50 ≤ 80 ms; p99 ≤ 300 ms).** Loose enough
  to land without PRAGMA changes; abdicates "FTS5 text query is
  fast" property.
- **D — Defer to 0.8.0.** Explicitly rejected; see Rejected
  alternatives below.

### AC-013 — vector retrieval (embedder-bearing) latency

Pack D measured AC-013 only at N=10,000 (full-scale seeding blocked
by vec0 single-row insertion path: ~5.5 inserts/sec on aarch64;
1,800 s to seed 10K vectors per `dev/test-plan.md` L130-132). The
canonical-runner has not measured AC-013 at N=1,000,000 because
the seed cost itself is the gate — Pack 7 Track 2 (bulk-vec seed
seam) is the prerequisite measurement-enabler.

**Proposed revised budget (HITL Q1, contingent on DIAG):**

- p50 ≤ 80 ms (current ADR-0.6.0-retrieval-latency-gates: ≤ 50 ms).
- p99 ≤ 300 ms (current: ≤ 200 ms).

Pinned against a canonical-runner measurement that has not yet
been taken. If the `0.7.0-PERF-DIAG` slice cannot enable a
canonical AC-013 measurement (bulk-seed seam not yet landed), the
AC-013 revision is **descoped** from 0.7.0 and AC-013 stays
DEFERRED with the deferral target moved from "0.7.0" to "0.8.0".

### AC-019 — concurrent-mixed read tail latency

AC-019 reruns AC-013's protocol under 8 concurrent reader threads.
Inherits AC-013's seed cost; no canonical-runner number yet.

**Proposed revised budget (HITL Q1, contingent on DIAG):**

- Baseline p99 ≤ AC-013 revised p99.
- Stressed p99 ≤ 2× baseline p99 (current bound; preserved).

If AC-013 descopes from 0.7.0, AC-019 descopes with it.

### AC-020 — single-reader concurrency ratio

Owned by `ADR-0.7.0-ac020-architectural-lever`. This ADR records
only the absolute-ms envelope:

- Sequential ≤ 600 ms (current Pack 6.G measurement at 0.6.1 tip:
  563 ms median).
- Concurrent ≤ 200 ms (current: 161 ms median).
- Speedup ≥ 5.33× (preserved from current bound:
  `tests/perf_gates.rs:245`, `1.5 / AC020_THREADS` with
  `AC020_THREADS = 8`).

The current measured speedup is 3.530× — the gap is the AC-020
lever's job to close. The numeric envelope above is what the
revised budget will be **after** the chosen architectural lever
lands. The AC-020 ADR justifies the lever and its expected gain.

## Workload + percentile definitions

**Preserved verbatim from `ADR-0.6.0-text-query-latency-gates.md`
L48-72:**

- **Dataset:** 1,000,000 chunk rows; synthetic-English-like text
  with Zipfian (s=1.0) token-frequency distribution.
- **Mean chunk text:** ≈ 500 bytes.
- **Query mix:** single-token MATCH + one phrase MATCH; tokens
  drawn from the 50th–90th percentile term-frequency band.
- **Concurrency:** QPS = 1 (sequential), single-process, no
  concurrent writes (AC-012 only; AC-020 changes this knob).
- **Cache state:** warm. Warmup pass discarded; second pass
  measured.
- **Sample count:** ≥ 1,000 measured queries per percentile.
- **Tokenizer:** default per `design/retrieval.md`. Gate
  re-validated if the default tokenizer changes.
- **Latency boundary:** in-process client call → result list.
  Includes safe-grammar parse + FTS5 MATCH + canonical row fetch
  - result serialization. Excludes IPC / network /
    subprocess-bridge envelope, graph-expand, FTS5 `snippet()` /
    `highlight()` extraction.

**New addition for 0.7.0:** if HITL Q4 lands the top-K `LIMIT`
cap on the search verb additively (default unlimited; opt-in
cap), the AC-012 measurement protocol runs with the cap
**disabled** (default-shape behavior). A separate AC
(`AC-NNN+4` per the plan doc) gates the cap's behavioral
correctness on canonical-runner.

## Measurement protocol

Same canonical-runner workflow (`.github/workflows/perf-canonical.yml`)
at full scale: `run_full_scale: true`, `ac012_corpus_n: 1000000`,
1000 samples per percentile, ubuntu-latest. The workflow shape is
locked from the 0.6.1 invocation; no workflow edits in this ADR.

Closure-JSON schema mirrors
`dev/plans/runs/0.6.1-AC012-measure-output.json`. Each PERF-\* slice
captures `workflow_runs.canonical.{url,verdict,p50_ms,p99_ms,duration}`
in its slice closure JSON.

A slice does NOT advance to reviewer spawn unless the perf gate
is GREEN at the revised budget. See
`dev/plans/0.7.0-implementation.md` § Ring 2 — Canonical-runner
perf gate.

## Acceptance criteria

Each revised budget gets a new AC ID (proposed in
`dev/plans/0.7.0-implementation.md` § Per-AC scoreboard;
appended to `dev/acceptance.md` when HITL Q1 locks). The legacy
AC-012 wording in `dev/acceptance.md` is **superseded by reference**:
the legacy AC ID stays in the file with a status-line pointer to
the new AC ID and to this ADR.

The new AC IDs are:

- **AC-071 (proposed):** revised AC-012 budget met on canonical
  runner at 1M corpus. Depends on this ADR landing `status: locked`
  with the specific numbers chosen at HITL Q1.
- **AC-072 (proposed):** revised AC-013 budget met (conditional;
  descoped if bulk-vec seed seam not landed).
- **AC-073 (proposed):** revised AC-019 budget met (conditional on
  AC-072).
- **AC-074 (proposed):** revised AC-020 budget met after chosen
  architectural lever lands. Depends on
  `ADR-0.7.0-ac020-architectural-lever` landing `status: locked`.

If HITL Q4 lands the top-K `LIMIT` cap additively:

- **AC-075 (proposed):** caller-visible top-K `LIMIT` argument
  honored on the search verb (caller-spec change). Behavioral, not
  perf.

## Rejected alternatives

**R1 — Keep the 0.6.0 budgets and accept RED.** The 0.6.0 numbers
were set without canonical-runner measurement; defending them
against the empirical 7.05× / 3.05× miss would require either
asserting the canonical-runner is wrong (it is the authoritative
target per `ADR-0.6.0-tier1-ci-platforms`) or asserting the
workload definition is wrong (it is unchanged from the 0.6.0
ADR's L48-72). Neither claim is defensible. Keeping the budget
also locks the project into perpetual RED on the load-bearing
perf gate, which is exactly the "deferral as theatre" failure
mode the project's reliability-principles MEMORY entry rules out.

**R2 — Defer to 0.8.0.** 0.8.0 is the knowledge-store / retrieval
anchor for Memex (`dev/roadmap/0.8.0.md`) — the substrate it
consumes assumes the underlying retrieval surface is performant
enough to be used. Deferring AC-012 to 0.8.0 either (a) blocks
0.8.0 on a perf revision the 0.8.0 anchor does not own, or
(b) lets 0.8.0 ship on top of a load-bearing RED gate. Neither
is acceptable. 0.7.0 was rescoped to perf-only precisely so AC-012
has its own release vehicle.

**R3 — Drop the AC-012 gate entirely.** Equivalent to R1 with
extra rope. FTS5 text query is REQ-053's `search` underlay;
deleting the gate erases the perf-attribution-for-load-bearing-
surface discipline that the 0.6.0 ADR established and the
canonical-runner workflow is built around.

**R4 — Revise budgets without measurement.** Restate the 0.6.0
numbers in different units, or pick numbers from theory. Rejected
because the failure that produced the 0.6.0 numbers was exactly
this — theory without empirical anchor. The whole point of this
ADR is to pin against measurement.

**R5 — Tighten budgets to canonical-runner cold-cache numbers.**
Off-spec: the 0.6.0 workload definition specifies warm cache.
Changing the cache regime is a workload change, not a budget
change; would require a separate ADR.

## Consequences

- `dev/requirements.md` REQ-010 updated to point at this ADR for
  the numeric anchor (the prior reference to ADR-0.6.0 stays as
  historical context).
- `dev/test-plan.md` § Current Perf Attribution updated:
  AC-012 / AC-013 / AC-019 / AC-020 deferral target moves from
  "0.7.0" (current state) to "0.7.0-CLOSED at <slice-id>" once
  the corresponding PERF-\* slice closes GREEN.
- `src/rust/crates/fathomdb-engine/tests/perf_gates.rs` budget
  constants updated by the corresponding PERF-\* slice's RED test
  author-step. **The budget is the new number, not the historical
  0.6.0 number.** No backwards-compat budget honored.
- CI perf gate (`perf-canonical.yml`) is unchanged in shape; the
  numbers it asserts come from this ADR.
- `dev/notes/perf-canonical-runner-2026-MM.md` carries forward as
  the empirical baseline; this ADR is its first downstream
  consumer.
- The PERF-PRAGMA / PERF-TOPK-LIMIT / PERF-AC020 slice prompts
  cite this ADR in their HITL-prerequisite section. None spawn
  until this ADR reads `status: locked`.

## Citations

- HITL 2026-05-24 (release rescope; 0.7.0 → perf-only single-thrust).
- HITL Q1 lock (date pending; this ADR draft is the input).
- `ADR-0.6.0-text-query-latency-gates` (superseded).
- `ADR-0.6.0-tier1-ci-platforms` (canonical runner shape).
- `ADR-0.6.0-no-shims-policy` § 54 (patch-release contract that
  forced the escalation to 0.7.0).
- `ADR-0.7.0-ac020-architectural-lever` (parallel; owns AC-020
  mechanism).
- `dev/notes/perf-canonical-runner-2026-MM.md` (empirical
  baseline).
- `dev/notes/performance-whitepaper-notes.md` § 5 (do-not-retry
  ledger; ensures the levers in scope are not on it).
- `dev/plans/runs/0.6.1-AC012-measure-output.json` (machine-readable
  baseline).
- `dev/plans/0.7.0-implementation.md` (release plan that consumes
  this ADR).
- Pack 7 un-defer trigger language at
  `dev/plans/0.6.0-implementation.md` L254-269.
