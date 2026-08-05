# STATUS — FathomDB 0.8.21

> **CLOSED — historical record.** `v0.8.21` is published; this board is retained
> as release evidence, not as a live execution surface.

> **Board of record.** The single writer is
> `dev/plans/release-state-0.8.21.json`; the release plan is
> `dev/plans/plan-0.8.21.md`.

## Current state

| | |
|---|---|
| **Next slice** | No implementation slice remains. Slices 0, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55, and 60 are landed; Slice 60 merged at `11f3fbf4`. |
| **Status** | The full reliability/platform/nested-projections ladder is complete. Slice 60 was remapped by HITL ruling `seq-242`, then landed through PR #195 after a green GitHub preflight. |
| **Release boundary** | HITL ruling `seq-243` upgraded 0.8.21 from label-only to a registry release. `v0.8.21` is published from `6f4b6cad`; GitHub release workflow `31033732068` and registry smokes are green. |
| **Repository hygiene** | Verified-safe stale worktrees, superseded branches, and clean temporary verification clones have been retired. Remaining non-primary worktrees are retained because they have unique commits, untracked evidence, or a Claude lock; they require explicit disposition. |
| **Build outputs** | About 12.9 GiB of ignored, reproducible `target/` output remains: 1.3 GiB in the primary checkout and about 11.6 GiB across retained worktrees. It has been measured, not deleted. |
| **Immediate next action** | Release complete: retain the published artifact evidence and begin only separately commissioned follow-on work. |
| **Branch protection** | **APPLIED AND VERIFIED 2026-08-04.** `main` now enforces `pull_request` + `required_status_checks` (16 checks, verified live-vs-intended: 0 missing, 0 extra) + `non_fast_forward` + `deletion`. A red PR can no longer be merged, and `main` can no longer be force-pushed or deleted. Snapshot: `dev/steward/branch-protection-ruleset.json`. See `dev/steward/branch-protection.md`, steward ledger seq-240/241. |
