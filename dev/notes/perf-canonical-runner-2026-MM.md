---
title: Canonical-runner perf re-measurement (0.6.1)
date: 2026-05-23
target_release: 0.6.1
desc: AC-012 canonical x86_64 tier-1 CI re-measurement; runner spec + numbers + verdict. Verdict RED — Pack 7 un-defer trigger fires.
blast_radius: dev/test-plan.md § Current Perf Attribution; dev/plans/0.6.1-implementation.md § Per-AC scoreboard; docs/release-notes/0.6.1.md § Performance
status: measured
---

# Canonical-runner perf re-measurement (0.6.1)

Measurement performed 2026-05-23 via the `perf-canonical` GitHub
Actions workflow (`.github/workflows/perf-canonical.yml`). Workflow
run: <https://github.com/coreyt/fathomdb/actions/runs/26346417896>.
Job ID: 77556958345.

Per `dev/plans/0.6.0-Phase-9-Pack-7-canonical-perf-measurement.md`
§ Track 1 deliverable shape (L46-62).

## Runner spec

- **CPU model:** AMD EPYC 9V74 80-Core Processor (4 cores allocated
  to the GitHub Actions runner slice).
- **Core count / threads:** 4 (per `nproc`).
- **OS:** Ubuntu 24.04.4 LTS.
- **Kernel:** `Linux runnervmg397c 6.17.0-1013-azure
#13~24.04.1-Ubuntu SMP Wed Apr 15 16:52:17 UTC 2026 x86_64`.
- **glibc version:** GLIBC 2.39 (Ubuntu).
- **SQLite version:** bundled via `libsqlite3-sys 0.28.0` in
  `Cargo.lock`; ships SQLite 3.45.x. Exact patch version was not
  captured at runtime (the workflow's `scripts/sqlite-version/`
  helper does not exist; followup: extract `rusqlite::version()`
  in a future workflow step). Per ADR-0.6.0-tier1-ci-platforms,
  this is the canonical bundled SQLite.
- **rustc version:** `rustc 1.95.0 (59807616e 2026-04-14)`.
- **Runner identifier:** `ubuntu-latest` (Linux / X64).
- **Workflow run URL:**
  <https://github.com/coreyt/fathomdb/actions/runs/26346417896>.

This is the canonical x86_64 tier-1 reference target per
`dev/adr/ADR-0.6.0-tier1-ci-platforms.md`.

## Measurement protocol (locked)

Per `dev/adr/ADR-0.6.0-text-query-latency-gates.md` L40-75 +
`dev/test-plan.md` § "Current Perf Attribution" L91-142:

- Corpus: 1,000,000 chunks (`AC_FULL_SCALE=1`,
  `AC012_CORPUS_N=1000000`).
- Workload: single-token MATCH + one phrase MATCH; tokens drawn
  from 50–90th percentile term-frequency band of a Zipfian (s=1.0)
  distribution.
- Concurrency: QPS = 1 (sequential), single-process, no concurrent
  writes.
- Cache state: warm. Warmup pass discarded; second pass measured.
- Sample count: 1000 queries (P-PERF-SAMPLES = 1000) — per ADR.
- Latency boundary: in-process client call → result list.
- Budgets (must not relax): **p50 ≤ 20 ms; p99 ≤ 150 ms.**

## Measured numbers

Raw harness output (from `perf-canonical-ac012.log`):

```
AC012_NUMBERS n=1000000 samples=1000 seed_ms=33618 p50_ms=140 p99_ms=458
thread 'ac_012_text_query_latency_on_fts5_path' (2594) panicked at src/rust/crates/fathomdb-engine/tests/perf_gates.rs:474:5:
AC-012 failed: p50=140.947602ms > budget 20ms at n=1000000
test ac_012_text_query_latency_on_fts5_path ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 10 filtered out; finished in 382.89s
```

| Metric     | Observed (ms) | Budget (ms) | Verdict | Multiple over budget |
| ---------- | ------------- | ----------- | ------- | -------------------- |
| AC-012 p50 | 140.95        | ≤ 20        | **RED** | 7.05×                |
| AC-012 p99 | 458           | ≤ 150       | **RED** | 3.05×                |

Seeding 1,000,000 chunks: 33.6 seconds (≈ 29,750 chunks/sec) —
unrelated to AC-012's measured latency boundary; informational
only.

Total test wall-clock: 382.89 s (seed + warmup + 1000 measured
queries).

## Pack 7 un-defer trigger evaluation

Per `dev/plans/0.6.0-implementation.md` § "Pack 7 un-defer trigger"
L254-269. Triggers (any one fires Pack 7):

- [x] **Canonical re-measure regression > 20% vs ADR budget.**
      Observed: p50 is 605% over budget (7.05×), p99 is 205% over
      budget (3.05×). **FIRES.**
- [ ] 0.6.0-rewrite consumer reported retrieval-perf incident
      traceable to deferred budgets. No incident reported in this
      session.
- [ ] Phase 12 release-evidence sweep finds deferred budgets cannot
      be honestly marked "design-on-paper". Not evaluated here;
      Pack 7 fires on the first criterion already.

**Pack 7 un-defer trigger: FIRES.** The AC-012 deferral cannot
be lifted via re-measurement alone; ADR budgets are not met at
canonical scale on canonical hardware.

## Verdict

**Verdict: RED.**

**Action:**

- Do NOT lift the AC-012 deferral in `dev/test-plan.md`.
- Do NOT bump axis-W to 0.6.1 yet — the patch-release plan
  pre-condition (AC-012 closes here) is not met.
- Surface to orchestrator + HITL: Pack 7 un-defer trigger fires.
  Decision needed:
  - (a) Pack 7 enters 0.6.1 scope (AC-012 budget unmet → engine
    work needed); 0.6.1 ships only after Pack 7 closes the gate.
  - (b) Pack 7 escalates to 0.7.x; 0.6.1 ships the remaining three
    deferred-item closures (OPENREPORT-py/ts, Dependabot,
    axis-E demo) under a re-titled scope that does NOT claim
    "AC-012 closed".
  - (c) Re-open the ADR. ADR-0.6.0-text-query-latency-gates
    budgets may have been set without empirical canonical-scale
    measurement (Pack D dev-runner numbers were aarch64). Codex
    - HITL evaluate whether the 20 ms p50 / 150 ms p99 numbers are
      achievable on canonical x86_64 4-core ubuntu-latest at all,
      or whether the budget needs revision. Note: per
      ADR-0.6.0-no-shims-policy §54, budget relaxation in a patch
      release is **out of scope**; this option is for the next minor
      release. The Pack 7 escalation track (option b) is also where
      this assessment lands naturally.

## Observations / caveats

- **Runner core count: 4.** ADR-0.6.0-tier1-ci-platforms specifies
  `x86_64-unknown-linux-gnu` as the canonical reference target but
  does not pin a specific core count. Single-process QPS=1 FTS5
  latency should not be core-count-sensitive at QPS=1 (no
  contention), so the 4-core slice is not the cause of the RED
  result.
- **SQLite version capture failed.** The workflow's
  `scripts/sqlite-version/` helper path does not exist; the
  workflow logged the fallback message. Proxy: `libsqlite3-sys =
"0.28.0"` per `Cargo.lock` → SQLite 3.45.x. Follow-up: add a
  one-liner cargo example or update the workflow to extract
  `rusqlite::version()` at runtime.
- **The p50/p99 numbers are consistent in shape with the aarch64
  Tegra Pack D measurement.** At N=100,000, Tegra reported
  p50=29.7 ms p99=85 ms. Canonical x86_64 at N=1,000,000 reports
  p50=140.95 ms p99=458 ms — roughly 10× scale increase translates
  to ~5× latency growth on both percentiles. The bottleneck scales
  with corpus size, suggesting an FTS5 index-pathology that
  doesn't disappear with better hardware.
- **Seed throughput is not the issue.** Seeding 1M chunks took
  33.6s (a one-time cost, well outside the measured query
  latency).

## References

- `dev/adr/ADR-0.6.0-text-query-latency-gates.md` — budgets + workload.
- `dev/adr/ADR-0.6.0-tier1-ci-platforms.md` — canonical runner shape.
- `dev/plans/0.6.0-Phase-9-Pack-7-canonical-perf-measurement.md` Track 1.
- `dev/plans/0.6.0-implementation.md` § "Pack 7 un-defer trigger"
  L254-269 (the trigger language).
- `dev/test-plan.md` § Current Perf Attribution L91-142 (the
  deferral framing; do NOT modify to lift AC-012).
- `dev/plans/0.6.1-implementation.md` § Per-AC scoreboard, § Pack 7
  trigger evaluation snapshot.
- `src/rust/crates/fathomdb-engine/tests/perf_gates.rs` — harness.
- Workflow run:
  <https://github.com/coreyt/fathomdb/actions/runs/26346417896>.
- Dry-run that validated the workflow shape:
  <https://github.com/coreyt/fathomdb/actions/runs/26346370369>
  (N=10,000, p50=0 ms, p99=3 ms — out-of-budget pathology only
  surfaces at canonical scale).
