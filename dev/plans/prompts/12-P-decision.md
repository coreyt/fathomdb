# Phase 12-P — HITL Decision Package: Performance AC Deferrals

**Type:** HITL-decision slice (not implementer). Orchestrator presents
options; user decides; orchestrator records.

**Scope:** Re-confirm (or un-defer) the four deferred performance
acceptance criteria: AC-012, AC-013, AC-019, AC-020.

**Owner:** user signoff.
**Exit criterion:** per-AC written decision in
`dev/progress/0.6.0.md`; `dev/test-plan.md` § Current Perf Attribution
refreshed if any deferral changes.

## Context

Four perf ACs deferred per `dev/test-plan.md` § Current Perf
Attribution. Pre-GA HITL re-confirmation required so release notes
carry accurate disclosures. Full per-AC analysis already in
`dev/plans/0.6.0-implementation.md` § "Performance ACs deferral
analysis"; condensed here for decision.

| AC                                | Budget                             | Latest dev-runner reading                                                                       | Core-change risk if un-deferred                                                                                                                                              |
| --------------------------------- | ---------------------------------- | ----------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **AC-012** text query (FTS5)      | p50 ≤ 20 ms / p99 ≤ 150 ms         | p50=29.7 ms RED / p99=85 ms GREEN at N=100,000 (aarch64 Tegra ~3× slower than canonical x86_64) | **LOW** — likely closes on canonical runner without engine work                                                                                                              |
| **AC-013** vector retrieval       | p50 ≤ 50 ms / p99 ≤ 200 ms         | p50=33 ms / p99=48 ms GREEN at N=10,000; N=50,000 unmeasurable (vec0 seed = 5.5 inserts/sec)    | **MEDIUM** — bulk-seed engine-surface gap; Pack 7 batched-insert API work                                                                                                    |
| **AC-019** mixed-retrieval stress | p99 ≤ max(10×baseline_p99, 150 ms) | Not measured at scale (inherits AC-013 seed cost)                                               | **MEDIUM** — same as AC-013                                                                                                                                                  |
| **AC-020** N=8 concurrent reader  | concurrent ≤ sequential × 1.25 / 8 | Best 3.530× speedup vs required 5.33× (gap 1.80×, ~80 ms conc)                                  | **HIGH** — `pcache1` mutex on every page-fetch; closure requires `SQLITE_CONFIG_PCACHE2` / vendor-SQLite swap / WAL2 / reader-writer split (Pack 7 territory; weeks of work) |

## Decision per AC

For each AC, choose: **(A) keep deferred for 0.6.0 GA** (document
in release notes) OR **(B) un-defer + close pre-GA** (forces
canonical-runner re-measurement, possibly engine work).

### AC-012 text query latency

- **Recommend (A) keep deferred.** Canonical-runner re-measurement
  is mechanical follow-up; minimal core-change risk. Mark
  release-notes language: "deferred pending canonical-runner
  evidence; expected close in 0.6.1 with no engine code change."
- **(B) un-defer**: book canonical x86_64 CI runner time;
  re-measure at N=100,000; assert p50 ≤ 20 ms; if budget misses
  even on canonical runner, surface engine-side FTS5 query-plan
  work as new sub-slice. ETA: days, not weeks.

[ ] (A) keep deferred + canonical-runner re-measurement queued for 0.6.1
[ ] (B) un-defer + measure on canonical runner pre-GA

### AC-013 vector retrieval latency

- **Recommend (A) keep deferred + Pack 7 commitment.**
  Retrieval-path itself is healthy (GREEN at N=10,000); deferral
  is about bulk-seed throughput which is a separate engine-surface
  gap. Pack 7 lands batched-insert vec0 API; AC-013 closes when
  that lands.
- **(B) un-defer**: requires landing bulk-insert API in Pack 7
  scope before GA. ETA: weeks.

[ ] (A) keep deferred + Pack 7 commitment
[ ] (B) un-defer (forces Pack 7 batched-insert pre-GA)

### AC-019 mixed-retrieval stress tail

- **Recommend (A) keep deferred + Pack 7 commitment.** Inherits
  AC-013 substrate; same Pack 7 dependency.
- **(B) un-defer**: same scope-expansion as AC-013.

[ ] (A) keep deferred + Pack 7 commitment
[ ] (B) un-defer (forces Pack 7 dependency pre-GA)

### AC-020 N=8 concurrent reader scaling

- **Recommend (A) keep deferred + Pack 7 architectural commitment.**
  This is the **architectural** deferral. Pack 5 + 6 + 6.G
  exhausted every canonical-SQLite lever measurable on AC-020's
  read-only fixture (mutex / parse / pool-topology / allocator-arena
  / page-cache / WAL / checkpoint all closed). Residual is
  `pcache1` mutex on every page-fetch (hit-path) — unfixable
  without vendor-SQLite swap OR `SQLITE_CONFIG_PCACHE2` custom
  allocator install OR WAL2 OR physical reader/writer separation.
  Pack 7 is the right vehicle; not weeks, possibly months. 0.6.0
  GA ships with **documented architectural gap**.
- **(B) un-defer**: would block 0.6.0 GA on Pack 7 architectural
  work. Strongly not recommended.

[ ] (A) keep deferred + Pack 7 architectural commitment + GA disclosure
[ ] (B) un-defer (blocks GA on Pack 7 — strongly NOT recommended)

## Orchestrator recommendation

All four: **option (A) keep deferred** with the noted commitments.
Net effect on GA:

- Release notes disclose the four deferred gates honestly.
- AC-012 closes in 0.6.1 with no engine work (canonical-runner
  re-measurement).
- AC-013/019/020 close in 0.7.0 or 0.6.x with Pack 7 substrate.
- 0.6.0 GA ships with the perf surface clients should expect:
  "vector retrieval + read concurrency are deferred-budget; if you
  need them at the ADR-locked numbers, wait for the Pack 7
  release."

## Outputs after signoff

1. Append HITL decision row to `dev/progress/0.6.0.md` per
   `dev/design/orchestration.md` § 12.7 (HITL gates as conversation
   boundaries):

   ```text
   ## 2026-MM-DD — Phase 12-P HITL decision
   - AC-012: (A) keep deferred — canonical-runner re-measurement
     queued for 0.6.1
   - AC-013: (A) keep deferred — Pack 7 batched-insert commitment
   - AC-019: (A) keep deferred — inherits Pack 7 batched-insert
   - AC-020: (A) keep deferred — Pack 7 architectural commitment
     (vendor-SQLite / PCACHE2 / WAL2 / reader-writer split); 0.6.0
     ships with documented architectural gap
   ```

2. Refresh `dev/test-plan.md` § Current Perf Attribution if any
   wording needs update.
3. Update `docs/release-notes/0.6.0.md` § Performance gates to
   reflect HITL-confirmed status.
4. Mark 12-P CLOSED in `dev/plans/runs/STATUS-phase12.md` +
   `dev/plans/0.6.0-implementation.md`.

No implementer spawn. No reviewer. HITL signature in
`dev/progress/0.6.0.md` is the closure artifact.
