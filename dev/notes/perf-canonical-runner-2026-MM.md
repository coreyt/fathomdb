---
title: Canonical-runner perf re-measurement (0.6.1)
date: 2026-05-22
target_release: 0.6.1
desc: AC-012 canonical x86_64 tier-1 CI re-measurement; runner spec + numbers + verdict. Template; fill at AC012-measure slice close.
blast_radius: dev/test-plan.md § Current Perf Attribution; dev/plans/0.6.1-implementation.md § Per-AC scoreboard; docs/release-notes/0.6.1.md § Performance
status: template
---

# Canonical-runner perf re-measurement (0.6.1)

Template authored in the 0.6.1 planning slice (2026-05-22). The
`0.6.1-AC012-measure` slice fills the numbers + verdict; the file
flips `status: template` → `status: measured` at slice close.

Per `dev/plans/0.6.0-Phase-9-Pack-7-canonical-perf-measurement.md`
§ Track 1 deliverable shape (L46-62).

## Runner spec

(Fill at measurement time.)

- **CPU model:** TBD
- **Core count / threads:** TBD
- **OS:** TBD (canonical: Linux)
- **Kernel:** TBD
- **glibc version:** TBD
- **SQLite version** (from `rusqlite::version()`): TBD
- **rustc version:** TBD
- **Runner identifier** (GitHub Actions runner label or host name): TBD
- **Workflow run URL:** TBD

## Measurement protocol (locked)

Per `dev/adr/ADR-0.6.0-text-query-latency-gates.md` L40-75 +
`dev/test-plan.md` § "Current Perf Attribution" L91-142:

- Corpus: 1,000,000 chunks (`AC_FULL_SCALE=1`).
- Workload: single-token MATCH + one phrase MATCH; tokens drawn
  from 50–90th percentile term-frequency band of a Zipfian (s=1.0)
  distribution.
- Concurrency: QPS = 1 (sequential), single-process, no concurrent
  writes.
- Cache state: warm. Warmup pass discarded; second pass measured.
- Sample count: ≥ 1,000 queries (`P-PERF-SAMPLES = 1000`).
- Latency boundary: in-process client call → result list.
- Budgets (must not relax): **p50 ≤ 20 ms; p99 ≤ 150 ms.**

## Measured numbers

(Fill at measurement time.)

| Metric     | Observed (ms) | Budget (ms) | Verdict |
| ---------- | ------------- | ----------- | ------- |
| AC-012 p50 | TBD           | ≤ 20        | TBD     |
| AC-012 p99 | TBD           | ≤ 150       | TBD     |

## Pack 7 un-defer trigger evaluation

Per `dev/plans/0.6.0-implementation.md` § "Pack 7 un-defer trigger"
L254-269. Evaluate at fill-time:

- [ ] Re-measure regression > 20% vs ADR-pinned budget? → trigger fires.
- [ ] 0.6.0 consumer reported retrieval-perf incident? → trigger fires.
- [ ] Phase 12 evidence sweep finds budgets cannot be "design-on-paper"?
      → trigger fires.

If GREEN on all three: 0.6.1 ships as planned; AC-012 deferral lifted.
If any RED: STOP and surface to orchestrator; Pack 7 may enter 0.6.1
or escalate to 0.7.x (`0.7.0-planning.md` Track 3 territory).

## Verdict

(Fill at measurement time.)

**Verdict:** TBD (GREEN | RED)

**Action:** TBD

- If GREEN: update `dev/test-plan.md` L135-142 to lift AC-012
  deferral; update `docs/release-notes/0.6.1.md` § Performance.
- If RED: stop; orchestrator surfaces Pack 7 un-defer to HITL.

## References

- `dev/adr/ADR-0.6.0-text-query-latency-gates.md` — budgets + workload.
- `dev/adr/ADR-0.6.0-tier1-ci-platforms.md` — canonical runner shape.
- `dev/plans/0.6.0-Phase-9-Pack-7-canonical-perf-measurement.md` Track 1.
- `dev/test-plan.md` § Current Perf Attribution.
- `dev/plans/0.6.1-implementation.md` § Per-AC scoreboard, § Pack 7
  trigger evaluation snapshot.
- `src/rust/crates/fathomdb-engine/tests/perf_gates.rs` — harness.
