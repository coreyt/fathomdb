# STATUS — FathomDB 0.8.21

> **Board of record.** The single writer is
> `dev/plans/release-state-0.8.21.json`; the release plan is
> `dev/plans/plan-0.8.21.md`.

## Current state

| | |
|---|---|
| **Ladder** | **Complete.** Slices 0, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50, and 55 are landed on `origin/main`; the release-state roll-up records their verified SHAs. |
| **Status** | The foundation ladder (0–20) is complete. **The ladder was EXTENDED on 2026-08-04 by HITL decision** with CI reliability, observability, and run-hygiene work, rather than opening 0.8.22. Design of record: `dev/design/ci-verify-robustness-review.md`. |
| **Release boundary** | 0.8.21 remains label-only. It must not advertise or publish an artifact without the evidence required by `dev/platform-capabilities.json`. Release closure and opening 0.8.22 remain explicit state transitions. |
| **Repository hygiene** | Verified-safe stale worktrees, superseded branches, and clean temporary verification clones have been retired. Remaining non-primary worktrees are retained because they have unique commits, untracked evidence, or a Claude lock; they require explicit disposition. |
| **Build outputs** | About 12.9 GiB of ignored, reproducible `target/` output remains: 1.3 GiB in the primary checkout and about 11.6 GiB across retained worktrees. It has been measured, not deleted. |
| **Immediate next action** | No further 0.8.21 ladder slice is authorized. Release closure and opening the next release remain explicit state transitions. |
| **Branch protection** | **APPLIED AND VERIFIED 2026-08-04.** `main` now enforces `pull_request` + `required_status_checks` (16 checks, verified live-vs-intended: 0 missing, 0 extra) + `non_fast_forward` + `deletion`. A red PR can no longer be merged, and `main` can no longer be force-pushed or deleted. Snapshot: `dev/steward/branch-protection-ruleset.json`. See `dev/steward/branch-protection.md`, steward ledger seq-240/241. |
