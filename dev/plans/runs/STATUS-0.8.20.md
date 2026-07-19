# STATUS — FathomDB 0.8.20 · OPP-12 Phase-2 + erasure completeness + the breaking-pair publish

> **Board of record** for 0.8.20 (`orchestration.md` §12.5). Ladder: `dev/plans/plan-0.8.20.md`.
> Design of record: `dev/design/0.8.20-erasure-and-h-end-state-v4.md`.
> Slice-0 design (v5 addendum): `dev/design/0.8.20-slice0-erasure-design.md`.
> **Update at every slice close.** Verify state from git, never from narration.

**Release base:** `4ca70ba6` · **Orchestration worktree:** `/home/coreyt/projects/fathomdb-worktrees/orch-0.8.20`
(branch `orch-0.8.20`, dedicated linked worktree per **TC-RUBRIC-5**).
**Last updated:** 2026-07-19 (Slice 0 in flight).

---

## 1. Current state

| | |
|---|---|
| **Slice in flight** | **Slice 0 — X0 design gate** |
| **Status** | Deliverables authored; **awaiting codex §9 → HITL X0 sign-off** |
| **Blocks** | Slice 0 blocks **everything** (X0 process gate) |
| **Next action** | Codex §9 on the Slice-0 package → drive to terminal PASS → **return to Steward for HITL X0 sign-off** |

**The orchestrator has no authority to grant X0 sign-off.** Slices 5+ MUST NOT start until the HITL signs.

---

## 2. Slice ladder

| Slice | Title | Depends-on | Status |
|------:|-------|-----------|--------|
| **0** | **X0 design gate** | — | **IN FLIGHT** |
| 5 | Erasure completeness (R-20-E1…E8, +E9a) | 0 | not started |
| 10 | `ReadView` / read-modes + node-validity (R-20-RV, R-20-NV) | 0 | not started |
| 15 | Projection registry (C-1) + EAV/property-FTS (R-20-PR, R-20-EAV) | 0 | not started |
| 20 | `dense_readiness` + `flush_embeddings()` (R-20-DR) | 15 | not started |
| 25 | Surrogate minting — governed entities ONLY (R-20-SUR) | 15 | not started |
| 30 | RUBRIC-H7 `can-i-deploy` contract gate (R-20-H7) | 10,15,20,25 | not started |
| 40 | Verification + release readiness | 5,30 | not started |

**Parallelization.** `5 ∥ 10 ∥ 15` after Slice 0. Slice 5 fixes **defects in shipped code** and does not wait on
the registry. All three touch `engine/src/lib.rs` ⇒ **serialize the merges** (rebase-then-merge one at a time).
**One `maturin develop` at a time** (shared `.venv` mutex) — and **never from a worktree**.
**Max 3 concurrent worktrees.** Canary the first launch of each new work-type before parallelizing.

---

## 3. AC scoreboard

**⚠ The plan's "new ACs continue from the AC-077 ceiling" is a RESERVED-ID COLLISION.** AC-077 is a *reserved
placeholder* for the IR-eval IR-1/IR-2 initiative (`dev/acceptance.md:1286`) and **AC-078 is conditionally
reserved to the same initiative** (`:1297`). Highest **defined, non-reserved** AC = **AC-076**.
**Recommendation: mint from AC-079.** *(HITL decision #4.)*

| AC | Covers | Status |
|---|---|---|
| **AC-079** *(proposed)* | Governed-surface delta (Phase-2 + erasure API) vs conformance allowlist — HITL-SIGNED; `recovery_denylist` unchanged (five) | proposed |
| **AC-080** *(proposed)* | Erasure completeness at rest — body absent from every row-owned projection **and** `-wal` bytes | proposed |
| **AC-041** | REQ-054 five-name recovery denylist | **UNCHANGED — must stay GREEN** (`erase_source` is not a denylist name) |

Everything else is tracked by **requirement id + TDD test name** per the locked-`acceptance.md` policy — see
`0.8.20-slice0-erasure-design.md` §4.

---

## 4. Open HITL decisions (options + recommendation)

| # | Decision | Recommendation |
|---|---|---|
| 1 | **eu7 basis** (F-22) — no-op vs bounded re-baseline | **no-op, conditional on Slice-40 proof** (design §7). **Must be decided on CPU numbers only** — §6 |
| 2 | **`embed_batch_cls` TS parity** (F-22) | **add the TS binding.** Already a documented blind-spot (`napi:709`, `py:2088`); X1 parity is a release gate and this is the first published release since 0.8.9 |
| 3 | **Erasure-audit durability** (design §2 D-A — **new finding**) | **exempt the audit collection from `enforce_provenance_retention`.** Retention-policy change ⇒ HITL |
| 4 | **AC id allocation** (§3) | **start at AC-079** |
| 5 | **Adoption arms** (build ≠ adopt, F-21) | read-modes/registry/readiness **opt-in**; erasure fixes **ship ON**; **`SourceId` is BREAKING — own call** |
| 6 | **Publish gate** (R-20-PUB) | Out of Slice-0 scope. Separate per-`x.y.z` gate; confirm Memex `0.5.x-successor` co-land readiness |

---

## 5. Process pins (bind every later slice)

### 5.1 TC-RUBRIC-7 — codex §9 transcript path (**PINNED**)

The rubric requires "a durable release-namespaced path" but names none. **Pinned for 0.8.20:**

```text
dev/plans/runs/codex/0.8.20/<slice>-<UTC-timestamp>.md
```

e.g. `dev/plans/runs/codex/0.8.20/slice-0-20260719T034500Z.md`. **Every slice persists its §9 transcript here**,
including fix-N re-reviews (one file per review round). Invoke codex **only** via
`dev/agent-tools/codex-nostdin.sh` (bare `codex exec` deadlocks on stdin).

### 5.2 TC-RUBRIC-5 — dedicated checkout

Orchestration and **all landing git-writes** run in a dedicated linked worktree. `scripts/preflight.sh --landing`
**HARD-fails on the primary checkout**, detecting primary via
`git rev-parse --git-dir` == `git rev-parse --git-common-dir`. **Built in Slice 0** (it did not previously exist).

### 5.3 Release DoD (every slice)

`cargo clippy --workspace --all-targets` **and** `cargo check --workspace --all-targets`, **both exit 0**.
Read the **real** exit code (`$?` / `PIPESTATUS`) — a trailing `echo` masking a non-zero exit is a live trap in
this repo, and **it recurred during Slice 0** (see §6.2).

---

## 6. R-20-EU7 baseline

### 6.1 Backend constraint — **eu7 is a CPU same-backend gate**

| Backend | n=7667 vector-stage recall@10 | CI | vs 0.90 floor (one-sided `ci_hi >= floor`) |
|---|---|---|---|
| **CPU** (GA-signoff) | **0.8960** | [0.8640, **0.9250**] | **PASS** |
| **GPU** (0.8.14 run log `:73`) | **0.8330** | [0.8010, **0.8640**] | **FAIL** |

The GPU figure is a **cross-backend artifact**, not a regression (TC-5 re-baseline driver, scheduled 0.8.23).
**The HITL GPU-eval mandate does not apply to eu7** — by its own fidelity caveat. A GPU eu7 run would manufacture
a false regression. **Baseline captured on CPU.**

### 6.2 Two vacuous-green hazards found in the harness (Slice 40 must guard both)

1. **The documented run command is wrong.** `tests/eu7_real_corpus_ac.rs:85-86` omits the required `operator`
   feature ⇒ **exit 101**. Working: `--features default-embedder,operator`. *Fix the docstring in Slice 5.*
2. **The corpus is unreachable from a worktree.** `data/corpus-data/` is gitignored (`.gitignore:9`) and lives
   only in the primary checkout (2.1 GB). From a linked worktree the harness **SKIPS and exits 0**.
   Slice 40 must assert a **non-skip witness**, not merely exit 0.

*(For this capture the corpus was bridged into the orchestration worktree by symlink, excluded locally via
`.git/info/exclude`; no tracked file changed.)*

### 6.3 Captured numbers

**Status: capture IN FLIGHT at time of writing** (CPU, `4ca70ba6`, `AGENT_LONG=1`,
`--features default-embedder,operator`, `EU7_N_VALUES` default `1000,7667`). Harness confirmed genuinely running
(`EU7_SETUP real_docs=18472 queries=100`), **not** skipped.

> **The numbers below MUST be filled in from the completed run before X0 sign-off. Do not sign X0 against an
> unfinished capture, and do not carry forward the historical GA-signoff figures as if they were measured at
> `4ca70ba6`.**

| N | vector-stage recall@10 | CI | p50 ms | p99 ms | verdict |
|---|---|---|---|---|---|
| 1000 | *pending* | | | | |
| 7667 | *pending* | | | | |

---

## 7. Outstanding worktrees

| Path | Branch | Purpose | State |
|---|---|---|---|
| `fathomdb-worktrees/orch-0.8.20` | `orch-0.8.20` | orchestration + Slice-0 docs (TC-RUBRIC-5) | **active** |
| `fathomdb-worktrees/slice-0-preflight-landing` | `slice-0-preflight-landing` | `preflight.sh --landing` guardrail | **active** (implementer) |

Clean up per `orchestration.md` §11 — **one destructive op per Bash call**; never `find -delete`.

---

## 8. Recent decisions (newest first)

- **2026-07-19 — Slice-0 (this board):** eu7 baseline pinned to **CPU same-backend**; TC-RUBRIC-7 transcript path
  pinned; AC allocation recommended from **AC-079** (reserved-id collision found); **four defects found in the v4
  design of record** (design §2), incl. the **non-durable erasure audit trail**.
- 2026-07-12 — **TC-11 pin A RATIFIED** (HITL). Anonymous nodes stay `h:` permanently; §2(ii) **OVERRULED**;
  surrogate leg **CANCELLED** for the anonymous class. **CLOSED — do not re-open.**
- 2026-07-12 — **REQ-037 lawful-erasure carve-out APPROVED** (HITL). `excise_source` stays CLI-only;
  **`erase_source()` ships as an SDK lifecycle verb**; AC-041 unchanged and stays GREEN.
- 2026-07-11 — TC-RUBRIC-5 dedicated-checkout guardrail ADOPTED (HITL).
- 2026-07-11 — Erasure axis = **PROVENANCE**, not the `l:`/`h:` id-space (HITL steer).
- 2026-07-10 — RUBRIC-H7 `can-i-deploy` gate; **absent-or-failing gate HOLDS the breaking pair** (HITL).
- 2026-07-09 — F-22 open-TC schedule; 2026-07-08 — F-21 build-authorized (build ≠ adopt);
  2026-07-07 — F-19/F-20 scope.

---

## 9. Compaction-resume checklist

1. `git -C fathomdb-worktrees/orch-0.8.20 rev-parse --abbrev-ref HEAD` ⇒ must be `orch-0.8.20`.
2. Read this board §1 (current slice) + §4 (open HITL decisions).
3. Read `plan-0.8.20.md` §4 (ladder) + `0.8.20-slice0-erasure-design.md` §2 (defects) and §4 (work items).
4. `git worktree list` — reconcile against §7.
5. **Never** trust a "green" without a printed real exit code.
6. **TC-11 is CLOSED.** Do not re-open the `h:` pin.
