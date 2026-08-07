# STATUS — FathomDB 0.8.22

> **Board of record.** The single writer is
> `dev/plans/release-state-0.8.22.json`; the release plan is
> `dev/plans/plan-0.8.22.md`.

## Current state

<!-- BEGIN GENERATED release-state:0.8.22:status-current-state -->**Next is Slice 21 (PROJ-STATE), NOT_STARTED.** Landed on `origin/main`: 0 (`55792858b2adce00d3d87193d02b23a5d8d52dd7`) · 5 (`55792858b2adce00d3d87193d02b23a5d8d52dd7`) · 10 (`4c7bb26b`) · 12 (`72a83049`) · 17 (`5a7f2484`) · 15 (`13341688fca3d02d11c10bb10eb26232156f8032`) · 18 (`8fdb27dbf00a0663772ffc8e27a243ac1e7dcd74`) — verified reachable, not asserted.<!-- END GENERATED release-state:0.8.22:status-current-state -->

| | |
| --- | --- |
| Stable target matrix | Linux glibc x64/ARM64, macOS x64/ARM64, and Windows x64. |
| npm policy | Publish `fathomdb` under `next`; promote only the main package to `latest` after all registry smokes and co-tagging. |
| Explicitly unsupported | Linux musl, Windows ARM/32-bit, and every triple outside the matrix. |
| Publish authority | The trusted-publishing bootstrap and normal explicit HITL authorization remain required before a real tag or registry publish. |

## Slice ladder

| Slice | Scope | Status |
| ---: | --- | --- |
| 0 | Contract, acceptance, and trusted-publishing runbook | Landed — `55792858b2adce00d3d87193d02b23a5d8d52dd7`. |
| 5 | SQLite dependency migration and TC-76 proof | Landed — `55792858b2adce00d3d87193d02b23a5d8d52dd7` (folded into Slice 0). |
| 10 | Five-target platform-package topology | Landed — `4c7bb26b`. |
| 12 | Current-authority and document-debt inventory | Landed — `72a83049`; Phase 2 remains unauthorized. |
| 17 | Pre-register 0.8.23 scale-measurement protocol | Landed — `5a7f2484`; protocol only, no scale run or scale claim. |
| 15 | Native build and validation matrix | Landed — `13341688fca3d02d11c10bb10eb26232156f8032`; CI run #31186535382 passed the full heavy verifier, five-runner native runtime matrix, and five wheel-size gates. |
| 18 | Ranked retrieval result limits and SDK parity | Landed — `8fdb27dbf00a0663772ffc8e27a243ac1e7dcd74`; default 10 and validated 1..=100 limits across Rust, Python, and TypeScript. |
| 19 | Canonical FTS join indexes and planner proof | Closed on reviewed local integration commit `550c4b03`; pending protected-branch landing and closure measurement. |
| 21 | Truthful projection runtime state and safe boot graft | Not started; depends on Slice 19 and its pickup decision record. |
| 22 | Governed pure projection-status read | Not started; depends on Slice 21 and its governed-read decision. |
| 20 | Ordered publication and registry smokes | Not started; now depends on Slice 22 as well as the prior release gates. |
| 25 | `next` to `latest` promotion and release truth | Not started. |

## Immediate next action

| | |
| --- | --- |
| **Immediate next action** | <!-- BEGIN GENERATED release-state:0.8.22:status-next-action -->**Commission Slice 21 (PROJ-STATE)** — truthful projection runtime state and safe boot graft. **Remaining ladder:** 21 → 22 → 20 → 25.<!-- END GENERATED release-state:0.8.22:status-next-action --> |
