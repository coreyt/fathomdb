# AC-013 / AC-019 — canonical-scale measurement policy

Written: 2026-05-26
Status: **POLICY-LOCKED.** Do not gate CI on AC-013 / AC-019 at
N=1,000,000 corpus.

## Why this note exists

The 0.7.0 perf-experiments campaign attempted a canonical-CI
measurement of AC-013 (vector retrieval latency) and AC-019
(concurrent-mixed read tail) at N=1M corpus under the locked
W4.1-stacked-O1 PCACHE2 stack
(workflow run `26482279276`, dispatched 2026-05-27 00:01 UTC).

The run took **>166 minutes** on the canonical 4-core EPYC
runner and approached the workflow's 240-min timeout. A
parallel local run on dev-box pinned to 4 cores
(`taskset -c 0-3`) confirmed AC-013 alone takes >23 minutes of
CPU time at N=1M; AC-013 + AC-019 together legitimately run
80–150+ minutes on 4-core hardware regardless of harness state.

This makes the test **unsuitable for any auto-triggered CI
posture**: gating PR merges or pushes on a 2-3 hour test is
infeasible.

## The policy

1. **The `perf-canonical.yml` workflow MUST remain
   `workflow_dispatch:` only.** No `push:` / `pull_request:` /
   `schedule:` triggers may be added that invoke AC-013 or
   AC-019 at `ac013_corpus_n >= 1000000`.

2. **AC-013 / AC-019 RED tests in `tests/perf_gates.rs` MUST
   remain `AGENT_LONG=1`-gated.** The default `cargo test` path
   must not block on them. Current implementation at
   `perf_gates.rs:488-489` (AC-013) and `perf_gates.rs:610-611`
   (AC-019) calls `if !long_run_enabled() { return; }` —
   preserve this.

3. **The default `ac013_corpus_n` workflow input MUST stay
   small** (currently `10000`; do not raise the default). A
   maintainer who wants the canonical measurement explicitly
   sets `ac013_corpus_n=1000000` at dispatch time and budgets
   ~3 hours of runner time.

4. **The canonical N=1M measurement is a once-per-release
   exercise**, not a per-slice gate. AC-013 / AC-019 budgets in
   the revised-budgets ADR are pinned to the *most recent*
   manual canonical-CI dispatch; slice closure-JSONs cite the
   measurement run URL, not a fresh re-measure.

## What's allowed for gating

The AC-013 / AC-019 RED tests at the **default** N (50_000) under
`AGENT_LONG=1` run in seconds and CAN be safely included in any
manually-dispatched, AGENT_LONG-aware long-run job. They are not
the canonical-scale gates — they are smoke tests that the
budget-expression is parseable.

If a slice author wants a canonical-scale verdict for AC-013 or
AC-019, the path is:

1. Manually dispatch `perf-canonical.yml` with
   `targets="ac013 ac019"`, `ac013_corpus_n=1000000`,
   `run_full_scale=true`, and the locked W4.1-stacked-O1 env
   knobs (`memstatus_off=true`, `pcache2=true`,
   `writer_pragmas='page_size=8192'`, `reader_pragmas` per the
   HITL doc).
2. Budget 2–3 hours of runner wall time.
3. Capture the closure-JSON into `dev/plans/runs/0.7.0-PERF-EXP-*.json`.
4. Cite that JSON in the slice's per-ring evidence section, not
   a fresh PR-time re-measurement.

## If the canonical runner hardware changes

Several factors that drive the long runtime are 4-core-specific:
- 8-thread AC-019 stress on 4 cores = heavy oversubscription.
- ANN search through 1M-row vec0 without an index is per-query
  linear; 4 cores serialise the 1000-query measurement passes.

If `ubuntu-latest` migrates to a ≥8-core SKU (or we move the
perf-canonical workflow to a `runs-on` larger SKU), the test may
fit within a tighter wall-clock envelope. Re-measure at that
point and revisit this policy — but the workflow_dispatch-only
posture is conservative-by-default and should be preserved
regardless.

## Long-term follow-up

The real fix for AC-013 cost at canonical scale is an **ANN
index** (HNSW / IVF / DiskANN) on the `vec0` virtual table.
Per-query cost drops from O(N) to O(log N) or O(sqrt(N)).
Tracked outside 0.7.0 — neither in scope nor a 0.7.0 budget
dependency. When that lands, AC-013 / AC-019 at N=1M may become
fast enough to gate on routinely; revisit the policy then.

## Related

- `dev/plans/0.7.0-HITL-recommendations.md` — locks AC-013/AC-019
  as "not descoped — finish the work" (with this policy as the
  caveat).
- `dev/notes/pcache2-followups.md` — captures the LRU / 16K
  variations for a separate follow-up slice.
- `.github/workflows/perf-canonical.yml` — the workflow this
  policy governs.
- `src/rust/crates/fathomdb-engine/tests/perf_gates.rs` —
  AC-013 (`ac_013_vector_retrieval_latency`) and AC-019
  (`ac_019_mixed_retrieval_stress_workload_tail`).
