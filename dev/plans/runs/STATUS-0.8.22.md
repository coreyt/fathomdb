# STATUS — FathomDB 0.8.22

> **Board of record.** The single writer is
> `dev/plans/release-state-0.8.22.json`; the release plan is
> `dev/plans/plan-0.8.22.md`.

## Current state

<!-- BEGIN GENERATED release-state:0.8.22:status-current-state -->**Next is Slice 0 (CONTRACT), IN_PROGRESS.** Landed on `origin/main`:  — verified reachable, not asserted.<!-- END GENERATED release-state:0.8.22:status-current-state -->

| | |
| --- | --- |
| Stable target matrix | Linux glibc x64/ARM64, macOS x64/ARM64, and Windows x64. |
| npm policy | Publish `fathomdb` under `next`; promote only the main package to `latest` after all registry smokes and co-tagging. |
| Explicitly unsupported | Linux musl, Windows ARM/32-bit, and every triple outside the matrix. |
| Publish authority | The trusted-publishing bootstrap and normal explicit HITL authorization remain required before a real tag or registry publish. |

## Slice ladder

| Slice | Scope | Status |
| ---: | --- | --- |
| 0 | Contract, acceptance, and trusted-publishing runbook | In flight — candidate under review. |
| 5 | SQLite dependency migration and TC-76 proof | Not started. |
| 10 | Five-target platform-package topology | Not started. |
| 15 | Native build and validation matrix | Not started. |
| 20 | Ordered publication and registry smokes | Not started. |
| 25 | `next` to `latest` promotion and release truth | Not started. |

## Immediate next action

| | |
| --- | --- |
| **Immediate next action** | <!-- BEGIN GENERATED release-state:0.8.22:status-next-action -->**Commission Slice 0 (CONTRACT)** — release contract, platform acceptance, and trusted-publishing runbook. **Remaining ladder:** 0 → 5 → 10 → 15 → 20 → 25.<!-- END GENERATED release-state:0.8.22:status-next-action --> |
