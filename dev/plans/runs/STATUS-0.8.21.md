# STATUS — FathomDB 0.8.21

> **Board of record.** The single writer is
> `dev/plans/release-state-0.8.21.json`; the release plan is
> `dev/plans/plan-0.8.21.md`.

## Current state

| | |
|---|---|
| **Slice in flight** | **NONE.** Slices 0 (`2ea2c884`), 5 (`a6cf2bbe`), 10 (`f94275e1`), 15 (`19d8f072`), and 20 (`354ee9b4`) are landed on `main`. |
| **Status** | Every 0.8.21 implementation slice is landed. The latest landing is the public-documentation drift guard at `d25bd63b`. |
| **Release boundary** | 0.8.21 remains label-only. It must not advertise or publish an artifact without the evidence required by `dev/platform-capabilities.json`. Release closure and opening 0.8.22 remain explicit state transitions. |
| **Repository hygiene** | Verified-safe stale worktrees, superseded branches, and clean temporary verification clones have been retired. Remaining non-primary worktrees are retained because they have unique commits, untracked evidence, or a Claude lock; they require explicit disposition. |
| **Build outputs** | About 12.9 GiB of ignored, reproducible `target/` output remains: 1.3 GiB in the primary checkout and about 11.6 GiB across retained worktrees. It has been measured, not deleted. |
| **Immediate next action** | Obtain HITL disposition for the retained worktrees and whether to purge their ignored build outputs; then explicitly close 0.8.21 or open the next release. Do not infer either transition from the completed ladder. |
