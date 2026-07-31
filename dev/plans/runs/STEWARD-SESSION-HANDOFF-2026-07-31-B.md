# Steward session hand-off — 2026-07-31-B

**Supersedes `STEWARD-SESSION-HANDOFF-2026-07-31-A`.**

## ★ IMMEDIATE NEXT STEP

Slice 40 is commissioned but **E1 is hard-stopped and not closed**. Do not begin
E2, integrate, push, or alter the release record outside the reconciled
Linux-first scope. A fresh E1 continuation must first apply the HITL ruling at
ledger `seq-234`, then resolve B5's already-authorized evidence route.

**Active work (2026-07-31):** one fresh role-simulated E1 continuation
orchestrator is commissioning a single canary implementer to reconcile the
Linux-first scope in the CI/release workflow and planning prose. It is bounded:
no integration, PR, CI accrual, B5 implementation, tag, dispatch, publish, or
credential action. Its receipt must be independently reviewed before the
Steward commissions the next E1 step.

The epoch base is `0f7642cc904186e7a1a7a815340404689a0f5679`. Before every E1
action and before integration, fetch `origin` in a private clone and re-verify
`BASE=$(git rev-parse origin/main)` is that SHA or a verified descendant. The
shared checkout cannot update `FETCH_HEAD`; do not write it or use linked
worktrees. Do not branch from the manifest's Slice-39 SHA.

## 1. Verified E0 — COMPLETE

E0 passed and is not to be repeated unless its inputs change.

- Manifest command `scripts/commission-manifest.sh 0.8.20 40`: rc **0**, 17
  paths, 24 anchors, 0 dead.
- The sole `{{MANDATE}}` witness is the exact `## 9. Immediate next slice`
  heading in `plan-0.8.20.md` (rc **0**).
- Fresh independent adversarial review: **PASS**.
- The Steward → simulated orchestrator → simulated implementer → simulated
  orchestrator → Steward canary passed in separate private clones. Its one local
  disposable commit was `d1e08db`; `git fetch . HEAD` rc **0**.
- No E0 tag, push, dispatch, publish, registry/credential action, release-record
  edit, or shared-checkout mutation occurred.

Artifacts remain at `/tmp/fathomdb-s40-e0-GdJTHR/orchestrator/dev/experiments/s40-epoch-handshake/`.

## 2. E1 BASE — current verified position

Each accepted patch is isolated, RED-first, and independently reviewed. **None is
integrated or pushed.** Keep all clones until the Steward verifies the subsequent
E1 receipt.

| unit | accepted head | review | remaining condition |
| --- | --- | --- | --- |
| B1 | `91a5b81c` | PASS | ready for integration |
| B2 | `6730faf3` | PASS | ready for integration |
| B3 | `fbc7d7d0` | PASS after remediation | ready for integration |
| B4 | `d4928c4d` | CANCELLED by `seq-234` | macOS/Windows CI and artifact scope move to 0.8.22; do not integrate this patch for 0.8.20 |
| B5 | `2e6376c3` | BLOCK | canonical binding lacks `test-hooks`; first whole-file run rc=1 (10 passed, 1 failed) |
| B6 | `0533eb8d` | PASS | ready for integration |
| B7 | `3c92505f` | PASS | ready for integration |
| B8+B8b | `1ab83b09` | PASS after one P1 repair | ready for integration |
| B9 | `1880fc9b` | PASS after one P1 repair | CI-only proof still awaits the BASE push |
| B10 | `c32501a0` | PASS | ready for integration |

**Decision 1 — Linux-first scope (HITL):** **RULED at ledger `seq-234`** —
0.8.20 supports/publishes Linux x86_64 native artifacts only. macOS/Windows CI
and artifact generation defer to 0.8.22. B4 is cancelled for 0.8.20; do not
integrate its patch or claim structural-serialisation credit. TC-91 still ships
on Linux, so its five-consecutive-run acceptance becomes relevant Linux CI
proof, not a waiver. Cargo source crates are not represented as platform-excluded.

**Decision 2 — B5 (HITL):** **APPROVED at ledger `seq-233`** — provision a safe,
test-hooks-capable canonical binding route, then perform the required accrual. The
prior canonical binding whole-file run failed because its `_fathomdb.abi3.so`
lacks `test-hooks` (rc=1, 10 passed/1 failed). Never rebuild or `pip install -e`
from a private clone, and do not mutate the shared checkout to manufacture evidence.

E1 has not created an integration branch, rebased, pushed the one BASE change,
started or observed CI, edited `release-state`, `STATUS`, the master schedule, or
any ledger. Its pre-push full gates and B9's CI-only proof remain outstanding.
The private continuation receipt is
`/tmp/fathomdb-s40-e1-orchestrator-qMnUCj/repo/dev/experiments/s40-e1-continuation-receipt-20260731.md`
at private commit `18d44ff0`.

## 3. Authority and hard stops

- Slice 40 remains **one indivisible slice**. Do not create 40a/40b, fractional
  slices, or a successor release to avoid the work.
- Preserve TDD, explicit-path staging, one writer per checkout, and independent
  adversarial review of every patch. Never accept self-declared closure.
- E2 begins only after the Steward independently accepts an E1 receipt.
- E5 stops after landing CI and a `dry_run=true` rehearsal. Never tag, dispatch a
  non-dry-run workflow, publish, use registry credentials, or change the release
  record to claim progress.
- PUBLISH remains the sole unruled, run-halting HITL decision. Gate (i) is every
  executed job green on Slice 40's landing commit; gate (ii) is explicit HITL
  approval. Neither is currently met.

## 4. Cold-start reading order

1. `AGENTS.md`, `.claude/commands/steward.md`, and
   `dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md`.
2. `scripts/steward-orient.sh`, the live board, active ladders, release-state,
   Slice-40 brief, and the remote epoch control.
3. This hand-off, then the E0 artifacts above and the E1 receipts/reviews under
   `/tmp/fathomdb-s40-e1-vNc3YH/` and `/tmp/fathomdb-s40-e1-b{1..6}-*/`.
4. Re-verify every claim from clone Git state and real command exit codes before
   commissioning the continuation.

The shared checkout was `main@f7569854` with pre-existing `.gitignore`,
`AGENTS.md`, and `.codex/` dirt. Treat that dirt as user-owned. The authorized
release base remains the separately re-verified `origin/main` SHA above.
