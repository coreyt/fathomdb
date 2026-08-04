# 0.8.20 Slice-40 CI-fix receipts

Closure receipts from four Slice-40 CI-remediation worktrees, preserved here on
2026-08-04 before those worktrees were retired.

Each was an **untracked** file in its worktree and existed nowhere else. They
are retained as the evidence record for work whose code changes are already on
`main`; none of them is a live instruction.

| Receipt | Origin worktree | Branch state at retirement |
| --- | --- | --- |
| `ci-fixture-contract-fix-output.json` | `/tmp/fathomdb-s40-ci-fixture-fix` | strict ancestor of `main` |
| `ci-python-contract-fix-output.json` | `/tmp/fathomdb-s40-ci-python-fix` | patch-equivalent to `main` |
| `ci-timeout-budget-fix-output.json` | `/tmp/fathomdb-s40-ci-timeout-fix` | strict ancestor of `main` |
| `ci-verify-timeout-60-output.json` | `/tmp/fathomdb-s40-ci-verify-timeout-60` | patch-equivalent to `main` |

Disposition rationale and the full per-branch audit are in
`dev/plans/runs/0.8.21-cleanup-and-drift-resolution-plan.md`.
