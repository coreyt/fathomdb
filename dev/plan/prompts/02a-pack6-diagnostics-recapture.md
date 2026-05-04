# Pack 6 diagnostics — Post-Pack-5 perf re-capture and attribution

This prompt is read-only. No production code changes. Use it when the human
wants a fresh attribution pass before running the Pack 6 intervention prompt.

## Purpose

Re-capture AC-020 perf evidence on the current `0.6.0-rewrite` tip and classify
the residual mutex/atomic symbols more precisely than Pack 5 did.

Questions to answer:

1. Does the post-revert tip still show mutex/atomic dominance comparable to
   Pack 5 A.1/A.2?
2. Can the residual hot symbols be attributed more specifically to:
   - `ReaderPool::borrow` / `release` / `Condvar`,
   - wrapper-side / handoff-side synchronization,
   - or WAL shared-memory atomics?
3. Is the planned Pack 6 intervention still the best closure attempt?

## Read order

1. `dev/plan/runs/STATUS.md`
2. `dev/plan/runs/A1-perf-capture-output.json`
3. `dev/plan/runs/A2-symbol-focus-output.json`
4. `dev/notes/performance-whitepaper-notes.md` §11
5. `dev/plan/runs/final-synthesis-output.json`

## Required work

1. Run the split AC-020 capture again on the current tip with the same capture
   shape as A.1.
2. Extend symbol grouping as needed so the final note can distinguish:
   - `ReaderPool` / `Condvar` / pool hot path,
   - wrapper-side / handoff-side synchronization,
   - SQLite / WAL shared-memory atomics.
3. Write output JSON and a short reviewer-ready markdown note.
4. Do not patch production code. If a tiny temporary diagnostic hook is truly
   required, stop and ask for a separate prompt.

## Output

- Updated output JSON under `dev/plan/runs/`
- Short note answering:
  - whether the Pack 5 attribution still holds,
  - whether Pack 6 F.0 remains the best intervention,
  - whether the residual now points elsewhere strongly enough to cancel F.0.

## Stop rule

This prompt ends after evidence is written. No implementation follows from this
prompt automatically.
