# STATUS — FathomDB 0.8.22

> **Board of record.** The single writer is
> `dev/plans/release-state-0.8.22.json`; the release plan is
> `dev/plans/plan-0.8.22.md`.

## Current state

<!-- BEGIN GENERATED release-state:0.8.22:status-current-state -->**Next is Slice 22 (PROJ-STATUS), IN_PROGRESS.** Landed on `origin/main`: 0 (`55792858b2adce00d3d87193d02b23a5d8d52dd7`) · 5 (`55792858b2adce00d3d87193d02b23a5d8d52dd7`) · 10 (`4c7bb26b`) · 12 (`72a83049`) · 17 (`5a7f2484`) · 15 (`13341688fca3d02d11c10bb10eb26232156f8032`) · 18 (`8fdb27dbf00a0663772ffc8e27a243ac1e7dcd74`) — verified reachable, not asserted.<!-- END GENERATED release-state:0.8.22:status-current-state -->

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
| 21 | Truthful projection runtime state and safe boot graft | Closed on reviewed local integration commit `26bdd2ce`; FIX-1 through FIX-5 are closed and protected-branch landing remains pending. |
| 22 | Governed pure projection-status read | In progress: C5 seq-247 and its 34/5/5 governed-surface pin preparation are independently approved; the READY plan proceeds to full three-layer RED/TDD implementation. |
| 20 | Ordered publication and registry smokes | Not started; now depends on Slice 22 as well as the prior release gates. |
| 25 | `next` to `latest` promotion and release truth | Not started. |

## Immediate next action

| | |
| --- | --- |
| **Immediate next action** | <!-- BEGIN GENERATED release-state:0.8.22:status-next-action -->**Commission Slice 22 (PROJ-STATUS)** — governed pure projection-status read. **Remaining ladder:** 22 → 20 → 25.<!-- END GENERATED release-state:0.8.22:status-next-action --> |

## Slice 22 pickup gate

The independent pickup review is recorded in
`dev/plans/runs/0.8.22-slice-22-pickup-review-20260807.md`. The C5 signature
is now recorded at steward-ledger seq-247, and the independently approved
governed-surface preparation is recorded in
`dev/plans/runs/0.8.22-slice-22-c5-prep-review-20260808.md`. The plan has
transitioned from draft to ready; full RED/TDD implementation may begin.

## Completion documentation gate

Before any 0.8.22 completion claim, independently verify that the release-state
JSON, rendered plan and STATUS board, affected `dev/` records, and affected
public `docs/` match the final implementation and release witnesses. Repair
material drift and run the applicable documentation checks; code or CI success
does not bypass this gate.
