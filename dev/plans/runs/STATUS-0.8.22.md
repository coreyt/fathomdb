# STATUS — FathomDB 0.8.22

> **Board of record.** The single writer is
> `dev/plans/release-state-0.8.22.json`; the release plan is
> `dev/plans/plan-0.8.22.md`.

## Current state

<!-- BEGIN GENERATED release-state:0.8.22:status-current-state -->**Next is Slice 20 (PUBLISH), PREP_COMPLETE_PUBLISH_HELD.** Landed on `origin/main`: 0 (`55792858b2adce00d3d87193d02b23a5d8d52dd7`) · 5 (`55792858b2adce00d3d87193d02b23a5d8d52dd7`) · 10 (`4c7bb26b`) · 12 (`72a83049`) · 17 (`5a7f2484`) · 15 (`13341688fca3d02d11c10bb10eb26232156f8032`) · 18 (`8fdb27dbf00a0663772ffc8e27a243ac1e7dcd74`) — verified reachable, not asserted.<!-- END GENERATED release-state:0.8.22:status-current-state -->

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
| 22 | Governed pure projection-status read | Closed on reviewed local integration commit `6aeee48e`; C5 seq-247, RED→GREEN→FIX-1, all-tier isolated verification, and independent re-review are complete. Protected-branch landing remains pending. |
| 20 | Ordered publication and registry smokes | Local preparation is closed at `2f94085c` after RED→GREEN→FIX-2 and independent reviews. Ordered publication and registry smokes remain explicitly held. |
| 25 | `next` to `latest` promotion and release truth | Not started. |

## Immediate next action

| | |
| --- | --- |
| **Immediate next action** | <!-- BEGIN GENERATED release-state:0.8.22:status-next-action -->**Commission Slice 20 (PUBLISH)** — ordered platform publication and registry smokes. **Remaining ladder:** 20 → 25.<!-- END GENERATED release-state:0.8.22:status-next-action --> |

## Slice 22 pickup gate

The independent pickup review is recorded in
`dev/plans/runs/0.8.22-slice-22-pickup-review-20260807.md`. The C5 signature
is recorded at steward-ledger seq-247, and the independently approved
governed-surface preparation is recorded in
`dev/plans/runs/0.8.22-slice-22-c5-prep-review-20260808.md`. Runtime review
and FIX-1 re-review are recorded in their paired Slice 22 review files. The
locally integrated implementation is closed at `6aeee48e` and awaits protected
branch landing.

## Slice 20 publish hold

The local release-safety preparation is closed at `2f94085c`; its pickup and
three review records document the RED→GREEN→FIX-2 closure. No production
workflow changed. The remaining Slice 20 action is real ordered publication and
registry smoke, which remains held pending explicit release authorization.

## Completion documentation gate

Before any 0.8.22 completion claim, independently verify that the release-state
JSON, rendered plan and STATUS board, affected `dev/` records, and affected
public `docs/` match the final implementation and release witnesses. Repair
material drift and run the applicable documentation checks; code or CI success
does not bypass this gate.

**Local result (2026-08-08): PASSED.** The independent review record at
`dev/plans/runs/0.8.22-documentation-correctness-review-20260808.md` closed
documentation FIX-1 through FIX-3 at `ad14c879` with no remaining P1/P2.
Release-state rendering, Markdown/public-doc lint, and full agent verification
passed. This is a local integration result only: protected-branch landing and
the explicitly held real publication/smokes remain outstanding.
