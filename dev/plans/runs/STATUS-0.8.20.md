# STATUS — FathomDB 0.8.20 · OPP-12 Phase-2 + erasure completeness + the breaking-pair publish

> **Board of record** for 0.8.20 (`orchestration.md` §12.5). Ladder: `dev/plans/plan-0.8.20.md`.
> Design of record: `dev/design/0.8.20-erasure-and-h-end-state-v4.md`.
> Slice-0 design (v5 addendum): `dev/design/0.8.20-slice0-erasure-design.md`.
> **Update at every slice close.** Verify state from git, never from narration.

**Release base:** `4ca70ba6` · **Orchestration worktree:** `/home/coreyt/projects/fathomdb-worktrees/orch-0.8.20`
(branch `orch-0.8.20`, dedicated linked worktree per **TC-RUBRIC-5**).
**Slice-5 worktree:** `/home/coreyt/projects/fathomdb-worktrees/orch-0.8.20-s5` (branch `orch-0.8.20-s5`,
cut from `origin/main` `19b568e2`, rebased onto `30ad3524`). **Terminal HEAD `8e09b950` is SUPERSEDED** — a
**regression was found after that first closure** by independent Steward verification and fixed in **fix-4**;
see §11.7. Terminal HEAD is now **`9c87d758`** plus the two **fix-5** commits (`93eca45a` + this docs commit).
**Last updated:** 2026-07-20 (Slice 5 **re-closed** after fix-4; awaiting Steward land).

---

## 1. Current state

| | |
|---|---|
| **Slice in flight** | **Slice 5 — erasure completeness (R-20-E1…E8)** — **CODE-COMPLETE ON-BRANCH** |
| **Status** | `orch-0.8.20-s5` @ **`9c87d758`** + fix-5. codex §9 returned **PASS** at `8e09b950` after three fix rounds — but **a PASS is not a run**: independent Steward verification (fresh clone, isolated venv, A/B against `origin/main`) then found a **live regression** that all four review rounds and every on-branch gate had missed. **Fixed in fix-4** (§11.7). Release-DoD gates re-verified with real exit codes (§11.3). |
| **Blocks** | Nothing is blocked *by* Slice 5. Slice 5 itself is blocked **from landing** pending the Steward + the HITL decisions in §4 (#7–#14) — notably **AC-079 governed-surface sign-off**. The **owed Python X1 is DISCHARGED** (§11.8) and TC-20/TC-27 guards are **shipped**. |
| **Next action** | **Return to Steward: land `orch-0.8.20-s5`, then obtain the HITL decisions in §4.** Nothing may be **published** until AC-079 is signed. |

**Slice 0 is COMPLETE and HITL-SIGNED** (`403eb254`, 2026-07-19) — the X0 gate is open and slices 5+ are
authorized. eu7 baseline capture remains **BLOCKED** (§6.3); resolve before Slice 40.

---

## 2. Slice ladder

| Slice | Title | Depends-on | Status |
|------:|-------|-----------|--------|
| **0** | **X0 design gate** | — | **COMPLETE — HITL-SIGNED, landed `403eb254`** |
| **5** | **Erasure completeness (R-20-E1…E8, +E9a)** | 0 | **COMPLETE on-branch @ `9c87d758`+fix-5** — first closure at `8e09b950` **re-opened by a regression** (§11.7), now fixed; awaiting land (§11) |
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
| **AC-079** | Governed-surface delta (erasure API) vs the conformance allowlist | **BUILT, ⚠ AWAITING HITL SIGN-OFF — NOT SIGNED** (below) |
| **AC-080** | Erasure completeness at rest — body absent from every row-owned projection **and** `-wal` bytes | **BUILT, GREEN** (below) |
| **AC-041** | REQ-054 five-name recovery denylist | **VERIFIED GREEN, denylist UNCHANGED at five** (below) |

**AC-079 — what was built, and what is still owed.** Slice 5d added to the *positive allowlist* in
`src/conformance/governed-surface-allowlist.json`: the command verb **`erase_source` / `eraseSource`**
(`Engine.erase_source` in Python, `Engine.eraseSource` in TypeScript) plus the non-command types
**`EraseReport`** (Py + TS), the Rust facade's net-new **`SourceId`** provenance newtype, and **`ExciseReport`**
moved from the `operator`-gated re-export block to the always-present one (it is `erase_source`'s return type).
`excise_source` **remains CLI-only** and is deliberately **not** allowlisted — it stays the recovery seam and
alone may address the engine's reserved `_`-prefixed namespace.
**The allowlist `_comment` records this delta verbatim as `AWAITING HITL SIGN-OFF, NOT SIGNED`.** It was written
so the branch is not red, **not** as an approval. `governed_surface` is 3/3 and TS surface tests are green
*against a proposal*. **Nothing may be published until this is signed** — see §4 #7.

**AC-080 — built and green.** `erasure_completeness` 10/10 asserts the erased body is absent from **every**
row-owned projection (registry-driven, incl. `search_index_v2`, the table that previously retained the body)
**and** absent from the `-wal` bytes after the verb's WAL truncation.

**AC-041 — re-verified GREEN, unchanged.** `no_recovery_surface` 1/1. The recovery denylist is still **exactly
the five REQ-054 names** — `["recover","restore","repair","fix","rebuild"]`. **`erase_source` is not one of
them**, so the denylist is untouched by this slice.

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

**Raised by Slice 5** (details in §11.5):

| # | Decision | Ledger | Recommendation |
|---|---|---|---|
| 7 | **AC-079 governed-surface sign-off** — `erase_source`/`eraseSource`, `EraseReport`, `SourceId`, `ExciseReport`. Marked **`AWAITING HITL SIGN-OFF, NOT SIGNED`** in the allowlist `_comment` | — | **Sign or amend before publish.** The REQ-037 carve-out (2026-07-12) already approved `erase_source` as an SDK verb in principle; this signs the *exact* symbol set. **Publish is blocked until signed.** |
| 8 | **Design-text correction** — the `logical_id IS NULL ONLY` backfill rule is right for NODES and **wrong for EDGES** | **TC-26** | **Correct plan §R-20-E8 + v4/v5 prose** to the shipped asymmetry. Code is right; the prose is not. TC-11 unaffected |
| 9 | **eu7 no-run prohibition is UNENFORCEABLE** — `eu7_real_corpus_ac` had no `#[ignore]` and no env gate; `scripts/agent-test.sh` carried a bare `cargo test --workspace`. Raised on **three consecutive** codex rounds. **GUARD SHIPPED in fix-4** (`eu7_real_corpus_ac.rs:760` `#[ignore]`; `agent-test.sh` can no longer invoke it) — **verified by INSPECTION ONLY, zero eu7 runs**, with a control proving the check was not vacuous | **TC-20** | **Still a decision:** the shipped `#[ignore]` is the hard gate the HITL asked for, but it creates the *opposite* vacuous-green hazard at Slice 40, where eu7 IS wanted (TC-13 class). **Slice 40 must carry a non-skip witness** and opt in with `-- --ignored` |
| 10 | **Python X1 was OWED** — 5c's `SourceId` is BREAKING and broke ~50 Python fixtures, swept but **only statically verified** (`py_compile` + `ruff` + AST audit). **DISCHARGED:** the suite has now been executed in an **isolated fresh clone with its own venv** (never the shared `.venv`) ⇒ **`2 failed, 754 passed, 7 skipped`**, and **the identical two tests fail on `origin/main`** — see §11.8. It was exactly this run that caught the fix-4 regression, vindicating the "landing blocker, not a follow-up" call | **TC-22** | **Satisfied.** The two residual failures are **pre-existing** and tracked as **TC-31** (#13) |
| 11 | **`maturin develop` fires AUTONOMOUSLY** from `src/python/tests/conftest.py::_ensure_test_hooks_binding` — merely running the Python suite from a worktree attempts to rebind the **shared** `.venv`. Observed live in fix-3. **GUARD SHIPPED in fix-4:** `conftest.py` now **refuses** to auto-run `maturin develop` without `FATHOMDB_TESTS_ALLOW_REBUILD=1` (`conftest.py:77`) | **TC-27** | **Closed by tooling** (fix-the-tooling, not a be-careful note). *No damage occurred:* the shared `.venv` was re-verified intact — `/home/coreyt/projects/fathomdb/.venv/.../fathomdb.pth` mtime still **2026-07-09**, still pointing at the **main** repo |
| 12 | **Pending-redaction queue hardening** — its "a row is removed ONLY when the obligation is discharged" invariant is upheld by **three correct call sites, not structurally**. codex found a defect in this one mechanism on **each** of rounds 1, 2 and 3 | **TC-28** | **Make it structural** (own table with no generic `DELETE` verb, or a trigger). Every known path is now closed but **nothing prevents a fifth.** Deliberately NOT attempted inside a fix round |

**Raised by Slice 5 fix-4** (details in §11.7/§11.8):

| # | Decision | Ledger | Recommendation |
|---|---|---|---|
| 13 | **WRITE/READ PROVENANCE ASYMMETRY.** 0.8.20 makes provenance **mandatory on write** (R-20-E2, `SourceId`) but it is **unreadable on a text or vector hit**: `PySearchHit.source_id` is populated **only for graph-arm hits** and is `None` for every two-arm hit (`fathomdb-py/src/lib.rs:537-539`). Consumers therefore fall back to `int(sh.id)`, which has raised `TypeError` since 0.8.19 made `SearchHit.id` an `IdSpace` (C-2). This is the measured form of the known "NO SDK EXPOSURE" erasure gap — a caller cannot tell which document a hit came from, so it cannot audit or scope an erasure | **TC-31** | **Schedule a read-side fix** — populate `source_id` on every arm. **One fix likely closes BOTH** residual Python failures (§11.8). **OUT OF SCOPE for Slice 5**: `_doc_id_of` is **byte-identical on `main`** and both failures reproduce there |
| 14 | **ENTITY-DEDUPE ERASURE GAP, adjacent to R-20-E2 — found in fix-4, NOT fixed.** Entities dedupe **within a batch** by `logical_id` derived from `(kind, name)`, so two documents naming the same entity **collapse to one row** carrying the **FIRST** document's provenance. Erasing the second document therefore **leaves that entity behind**, still attributed to the first. An **erasure-completeness gap**: the slice's own guarantee ("erase every row owned by this source") does not hold for a co-named entity | **TC-32** | **Entity-identity design question, not a fix round.** Options: per-source entity rows, or a multi-valued provenance edge set. Must be decided before the erasure guarantee is stated unconditionally to users |

---

## 5. Process pins (bind every later slice)

### 5.1 TC-RUBRIC-7 — codex §9 transcript path (**PINNED**)

The rubric requires "a durable release-namespaced path" but names none. **Pinned for 0.8.20:**

```text
dev/plans/runs/codex/0.8.20/<slice>-<UTC-timestamp>.log
```

`.log`, not `.md` — matches the existing transcript convention (`0.8.16-slice-*-codex-review-*.log`) and keeps
raw transcripts out of markdownlint scope.

e.g. `dev/plans/runs/codex/0.8.20/slice-0-20260719T033434Z.log`. **Every slice persists its §9 transcript here**,
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

### 6.3 Captured numbers — **CAPTURE BLOCKED (root-caused)**

**No baseline was captured at `4ca70ba6`. Do not fabricate one, and do not carry the historical GA-signoff
figures forward as if they were measured here.**

**The eu7 harness cannot complete on CPU on this box.** Root-caused by bisecting N (three runs, real exit codes):

| Run | N | Real exit | Outcome |
|---|---|---|---|
| full | 7667 (batched 256) | **101** | panic `eu7_real_corpus_ac.rs:414` — `seed drain (batch): Scheduler` |
| probe | 200 | **101** | identical panic, same line |
| minimal | 20 | **0** | **PASSED** — vector-stage recall@10 = 1.0000, 258.54 s |

`drain(600_000)` → `wait_for_idle` timeout → `EngineError::Scheduler`.

**The worker is NOT wedged — it is throughput.** n=20 passed cleanly, so the embed/projection path is
functionally correct. That run reports **`seed_ms=111670` for 20 docs = 0.179 docs/sec**, about **7.3× slower**
than the **1.3 docs/sec** the harness docstring assumes (`:97-99`). At 0.179 docs/s a **`BATCH = 256`** needs
**~1430 s**, so it can **never** drain inside the hardcoded **600 s** — the harness is **structurally unable to
run here at any N**, because it fails on the **first batch**. A full 7667-doc seed would need **~11.9 hours**
even with the timeout raised.

Excluded causes: weights cache is **complete** (`config.json` + `tokenizer.json` + `model.safetensors`);
CPU load was **4.5 of 24 cores**.

**The tension that must be resolved before Slice 40:** §6.1 forbids GPU for comparability, and CPU cannot
finish ⇒ **R-20-EU7 currently has no runnable path.**

**Options (Slice 40):** **(a)** reduce `BATCH` 256 → 64 (358 s, fits inside 600 s) or make `BATCH`/timeout
env-tunable — minimal, surgical, **does not change measurement semantics**; **(b)** raise the drain timeout and
accept a ~12 h CPU run; **(c)** investigate the 7.3× shortfall, which may itself be a real CPU-embed regression.
**Recommend (a) + (c).**

**Side-effect hazard.** The harness **writes `dev/plans/runs/eu7-latest-measurements.json` into the repo on every
run**, so a reduced-N scouting run silently produces a file that *looks* authoritative — the n=20 run wrote
`recall=1.0000` there. It was **deleted, not committed**. Never commit it from a scouting run. *(TC-19)*

## 7. Outstanding worktrees

| Path | Branch | Purpose | State |
|---|---|---|---|
| `fathomdb-worktrees/orch-0.8.20` | `orch-0.8.20` | orchestration + Slice-0 docs (TC-RUBRIC-5) | **active** |
| `fathomdb-worktrees/slice-0-preflight-landing` | `slice-0-preflight-landing` | `preflight.sh --landing` guardrail | Slice 0 landed — **reclaimable** |
| `fathomdb-worktrees/orch-0.8.20-s5` | `orch-0.8.20-s5` | Slice 5 erasure completeness | **active** — holds `9c87d758`+fix-5, **do not remove before land** |

Clean up per `orchestration.md` §11 — **one destructive op per Bash call**; never `find -delete`.

---

## 8. Recent decisions (newest first)

- **2026-07-20 — Slice 5 RE-CLOSED after a post-closure REGRESSION** (fix-4, `9c87d758`). Independent Steward
  verification — **fresh clone, isolated venv, A/B against `origin/main`** — found multi-document
  `ingest_with_extractor` failing with `ExtractorError`. **The engine was not the defect and was not changed**
  (`engine/src/lib.rs` byte-identical across fix-4); the **extractor protocol** was, in never requiring
  per-entity attribution. **Behavioral contract change:** multi-doc extractor batches now require **per-entity
  attribution**; a caller who cannot attribute must submit **single-document batches**. Also shipped the TC-20
  eu7 hard gate and the TC-27 `maturin` opt-in guard. New: **TC-31**, **TC-32** (§4 #13/#14). **Lesson: a codex
  PASS plus four green on-branch gate runs did not substitute for one honest execution.** (§11.7/§11.8)
- **2026-07-20 — Slice 5 CODE-COMPLETE** on `orch-0.8.20-s5` @ `8e09b950`; **codex §9 terminal PASS** after three
  fix rounds (§11). Proved the **`logical_id IS NULL ONLY` backfill rule wrong for EDGES** (TC-26); shipped the
  HITL-ruled erasure-audit retention exemption (§4 #3). **Six HITL items owed** (§4 #7–#12) — AC-079 is **NOT
  signed** and **blocks publish**; main-tree Python X1 is a **landing blocker**.
- **2026-07-19 — Slice-0 HITL-SIGNED and landed** at `403eb254`. X0 gate open; slices 5+ authorized.
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

---

## 10. Slice-0 findings the Steward must reconcile into the master

Slice 0 proved several things in `plan-0.8.20.md` / the v4 design wrong or under-specified. **The orchestrator did
not edit the master plan** — these are handed up for reconciliation.

| # | Where | Finding | Ledger |
|---|---|---|---|
| 1 | plan §3 | **"AC ceiling = AC-077, continue from it"** is a reserved-id collision. AC-077 is a *reserved placeholder* for IR-1/IR-2; **AC-078 is conditionally reserved to the same initiative**. Highest defined non-reserved AC = **AC-076**. Recommend minting from **AC-079**. | **TC-14** |
| 2 | plan §8 / v4 §3.6 | **"`source_id` retained permanently in `excise_source_audit`" is FALSE.** `enforce_provenance_retention` (`:10070`) sweeps `operational_mutations` with **no collection filter** (`:10083`), so audit rows are swept like any other. The erasure **audit trail is destructible**, and it shares a retention pool with the op-store payloads work-item 9 must erase. | **TC-15** |
| 3 | plan §0.1 / v4 §2.2 | Registry model too coarse. The write path **enqueues** vector work (`_fathomdb_projection_state`, **`kind TEXT PRIMARY KEY`** — verified) rather than projecting it. Registry must split **row-owned** (`write_cursor`-keyed) from **kind-owned**, or the guard demands a per-cursor delete on a kind-keyed table. | — |
| 4 | v4 §1/§2.2/§6 | The registry consumer is **`rebuild_shadow_state` (:6515)**, not `rebuild_projections` (:5949, the public entry). Taking v4 literally patches the wrong function. | — |
| 5 | plan §0 / v4 §3.4 | `derive_logical_id` **lowercases** its inputs (`:11156`). Strengthens the dictionary-attack rationale; the stated derivation is incomplete. | — |
| 6 | plan §7 prereq 4 | **"Baseline captured" was listed as an assumed precondition — no baseline existed.** Capture attempted at Slice 0 and is **BLOCKED, root-caused** (§6.3): the harness's `BATCH = 256` cannot drain inside its hardcoded 600 s at the measured **0.179 docs/s** (~7.3× below the documented rate), so it fails on the **first batch at any N**. Combined with §6.1 (GPU forbidden for comparability), **R-20-EU7 has no runnable path today.** | **TC-13**, **TC-19** |
| 7 | R-20-PUB | **The publish dry-run guard is DEAD and has been red since 0.8.14.** `test_actionlint_fixture.sh:53` greps `release.yml` for `cargo publish --dry-run -p`, but the job now delegates to `cargo-publish-if-new.sh --dry-run`. **Behavior is intact** (the helper forwards correctly) — but `./scripts/agent-test.sh` exits 1 wholesale, so a **real** publish-wiring regression would be invisible **in the first release that publishes for real**. | **TC-16** |
| 8 | v4 §3.2 | **Slice 5's `SourceId` newtype will break the eu7 harness** (`eu7_real_corpus_ac.rs:405` builds `PreparedWrite` with `source_id: None`). v4 enumerated only two internal callers and missed the test-side ones. Sweep `src/` **and** `tests/`. | **TC-17** |
| 9 | TC-RUBRIC-7 | Committing a §9 transcript **into the reviewed range** pollutes the next review's diff (codex re-read its own prior findings as if unfixed). Recommend committing transcripts **after** the final review round. | **TC-18** |

**Also carried:** the eu7 basis and `embed_batch_cls` decisions (§4 #1/#2) remain **HITL calls**, recorded with
recommendations, not decided here.

---

## 11. Slice 5 close — erasure completeness (R-20-E1…E8)

**Branch `orch-0.8.20-s5`, terminal HEAD `9c87d758` + fix-5** — cut from `origin/main` `19b568e2`, rebased onto
`30ad3524`. **Not landed.** The Steward lands it.

**⚠ The first closure at `8e09b950` was premature.** Post-`8e09b950` history:

| Commit | Content |
|---|---|
| `9898fd8e` | ledger TC-30 + Slice-5 docs closure artifact |
| `ff2d641c` | **fix-4** — RED: multi-document extractor batches require per-entity attribution |
| `9550bcde` | **fix-4** — GREEN: ELPS harness must attribute ENTITIES, not just edges |
| `265c54c0` | **fix-4** — tooling guards: TC-20 eu7 hard gate, TC-27 `maturin` opt-in |
| `9c87d758` | **fix-4** — docs: the per-entity-attribution contract |
| `93eca45a` | **fix-5** — `cfg`-gate `is_erasure_bookkeeping_collection` (non-`operator` `dead_code`) |

### 11.1 What shipped

| Sub-slice | Head | Content |
|---|---|---|
| **5a** | `bdd8750e` | **R-20-E1** — row-owned projection registry + **total** node/edge projectors; the five hand-rolled projection lists deleted |
| **5b** | `18197495` | **R-20-E5/E6/E7** — WAL truncation on erasure · selective telemetry redaction · record-level op-store erasure · erasure-audit durability |
| **5c** | `875017a2` | **R-20-E2/E3/E8** — `SourceId` newtype (**BREAKING**) · reserved `_engine:` / `_legacy:` provenance · **caller-grounded** ingest provenance |
| **5d** | `4b78658d` | **R-20-E4** — `erase_source` SDK verb (Py + TS + Rust) · `doctor orphan-provenance` · governed-surface delta · user docs |
| fix-1 | `00b46b84` | codex **P1** legacy-edge backfill (via `989fd7ef`) + **P2** durable pending-redaction queue |
| fix-2 | `7be20ec3` | codex **P2** doctor edge accounting + **P2** drain-before-freeze |
| fix-3 | `8e09b950` | codex **P1** refuse excising erasure bookkeeping + **P2** rotated-sink ⇒ `ErasureIncomplete` |
| **fix-4** | `9c87d758` | **REGRESSION** — multi-doc extractor batches require **per-entity attribution** (§11.7) · TC-20 eu7 hard gate · TC-27 `maturin` opt-in guard |
| fix-5 | `93eca45a` | `cfg`-gate `is_erasure_bookkeeping_collection` — the fix-3 guard lacked the `#[cfg(feature = "operator")]` its only call site carries, warning `dead_code` on every non-`operator` build. Behavior unchanged |

The central defect this slice closes: **`search_index_v2` stores the body**, so before R-20-E1 an excised body
**survived erasure** in that table. It never surfaced in results (both read paths gate on `canonical_nodes`),
which is exactly why it went unnoticed — a data-at-rest leak, invisible to any result-level assertion.

### 11.2 codex §9 — four rounds, terminal PASS

Transcripts under `dev/plans/runs/codex/0.8.20/` (TC-RUBRIC-7 path), committed **after** the final round per
TC-18.

| Round | Transcript | Verdict |
|---|---|---|
| 1 | `slice-5-20260719T231341Z.log` | **P1** legacy edges erasable by **no verb**; **P2** telemetry-redaction retry falsely reports success |
| 2 | `slice-5-fix-1-rereview-20260719T234803Z.log` | P1 cleared; **P2** doctor gives false assurance on unerasable edges; **P2** freeze-before-drain timeout |
| 3 | `slice-5-fix-2-rereview-20260720T001616Z.log` | **P1** `excise_collection_record` could delete the pending-redaction queue; **P2** rotated sink treated as redacted |
| 4 | `slice-5-fix-3-rereview-20260720T005056Z.log` | **TERMINAL PASS** — *"No actionable correctness issues were found in the reviewed diff. The added erasure/provenance paths appear consistently wired through Rust, Python, TypeScript, CLI, schema migration, and tests."* |

### 11.3 Gates — re-verified on the terminal HEAD (real exit codes)

Re-run at **fix-5** (`93eca45a`). Read via `$?` / `PIPESTATUS`, never a trailing `echo`.

| Gate | Result |
|---|---|
| `cargo clippy --workspace --all-targets` | **0** — and **zero `dead_code`** on a non-`operator` build (fix-5) |
| `cargo check --workspace --all-targets` | **0** |
| `erasure_projection_registry` | **4/4** |
| `provenance_mandatory` | **3/3** |
| `multidoc_extractor_provenance` (**fix-4**) | **5/5** |
| `erasure_completeness` (AC-080) | **10/10** |
| `sdk_only_erasure` | **3/3** — via **explicit non-operator invocation** (TC-25: it is `#![cfg(not(feature = "operator"))]`, so any feature-unified run compiles it to **zero** tests and reports success having asserted nothing) |
| `no_recovery_surface` (**AC-041**) | **1/1** — denylist unchanged at five |
| `governed_surface` | **3/3** — *against an unsigned proposal*, see §3 |
| `fathomdb-schema`, all targets | green, incl. new `step21_migration.rs` **5/5** |
| `fathomdb-cli`, all targets | green |
| TypeScript | **170/170**, `tsc` **0** |
| **Python** | **`2 failed, 754 passed, 7 skipped`** in an **isolated fresh clone** — failure set **identical to `origin/main`**, both pre-existing (**TC-31**). See **§11.8**; do **not** read this row without it |
| `SCHEMA_VERSION` | **20 → 21** |

### 11.4 What Slice 5 proved WRONG

1. **The `logical_id IS NULL ONLY` backfill rule is correct for NODES and WRONG for EDGES** *(codex P1; ledger
   **TC-26**)*. `purge_inner` resolves its target **exclusively** via
   `SELECT state FROM canonical_nodes WHERE logical_id = ?1`, then erases edges by **endpoint**
   (`from_id`/`to_id`). It **never** resolves an edge by edge `logical_id` — an edge's `logical_id` is only a
   **supersession identity** and confers **no purge-addressability whatsoever**. So a legacy edge with
   `source_id IS NULL AND logical_id IS NOT NULL` was unreachable by `excise_source`/`erase_source` (no
   provenance) **and** unreachable by `purge` (not addressable) — **erasable by no verb at all**, disappearing
   only incidentally when a connected node happened to be purged. That defeats R-20-E8's entire purpose.
   **Shipped step 21 is deliberately asymmetric:** nodes keep the `logical_id IS NULL` gate; edges back-fill on
   `source_id IS NULL` alone. **TC-11's pin is NOT affected** — the statement *reads* `logical_id` as its
   predicate and **never writes one**; no row transitions `logical_id` NULL → NOT NULL and no stored row's
   id-space is re-derived (`s21_backfill_populates_no_logical_id` asserts both).
   ⚠ **`plan-0.8.20.md` R-20-E8 (`:197`) and the v4/v5 design prose still state the unqualified rule and must be
   corrected** (§4 #8). The code is right; the design of record is not.
2. **v4 §3.6's "the audit retains `source_id` permanently — by design"** was already known false at Slice 0
   (§10 #2 / TC-15): `enforce_provenance_retention` swept `operational_mutations` with **no collection filter**,
   so the erasure audit trail was destructible. Slice 5 implements the **HITL-ruled** fix (§4 #3) — the
   erasure-audit collections are **exempt** from the sweep, and so is the new pending-redaction queue.
   Consequence, and it is a **behaviour change to a shipped knob** (**TC-24**): `cap` now bounds **sweepable**
   rows, not physical rows. An operator who sized `cap` against a physical row count will see the table exceed
   it. Changelogged.

### 11.5 Owed to the HITL / Steward

In §4 as decisions **#7–#14**, with ledger ids: **AC-079 sign-off** (blocks publish, **still NOT SIGNED**) ·
**design-text correction** TC-26 · **eu7 guard shape** TC-20 (guard now **shipped**) · **Python X1** TC-22
(**discharged**, §11.8) · **`maturin develop` conftest guard** TC-27 (**shipped**) · **pending-redaction
structural hardening** TC-28 · **write/read provenance asymmetry** TC-31 · **entity-dedupe erasure gap** TC-32.
Also logged by this slice and **not** requiring a decision: **TC-21** (`pr_g10_reranker_ce` has not compiled
under `--features default-reranker` since 0.8.19 — **pre-existing**, file byte-identical to baseline; it survived
because the release-DoD full-workspace gate does **not** fan out over feature combinations), **TC-23**
(untracked closure `output.json` artifacts are destructible by routine git hygiene — it happened **twice** in
this slice; implementers should **commit** their closure witness), **TC-25** (the `sdk_only_erasure`
vacuous-green hazard above — **CI must carry the explicit invocation** or the R-20-E4 guarantee is untested),
and **TC-29** (`run_rebuild` is the last remaining freeze-before-drain instance, unaudited; and
`operator_cli::t_s34_dump_mutations_lock_held_exits_71` is flaky under cross-binary lock contention — touches no
erasure path).

### 11.6 Closure artifacts

`dev/plans/runs/0.8.20-slice-5{a,b,c,d}-output.json` and
`dev/plans/runs/0.8.20-slice-5-fix-{1,2,3,4,5}-output.json` (nine), plus the four §9 transcripts in §11.2.
Committed with this close **(TC-23** — an untracked closure witness is destructible by routine git hygiene, and
**two implementers in this slice destroyed work exactly that way**; the witness gets committed, not left loose).

### 11.7 fix-4 — the REGRESSION found AFTER the first closure

**Found by independent Steward verification** — a **fresh clone with its own isolated venv**, run **A/B against
`origin/main`**. Not by codex (four rounds, terminal PASS), and not by any on-branch gate.

**What broke.** Multi-document `ingest_with_extractor` failed with `ExtractorError`.

**Mechanism.** `resolve_provenance` (`engine/src/lib.rs:3933-3943`) admits the model's echo **only as a
SELECTOR** among the caller-supplied batch ids: a single-document batch short-circuits to the one caller id, but
on a **multi-document** batch the echo **must name one of them** or the ingest fails loudly. Meanwhile
`src/python/eval/elps_live_harness.py` backfilled `source_doc_id` onto **edges only**, and the stub entities
carried none — so **every multi-doc batch failed at the entity loop** (`lib.rs:3972`, resolving at `:3979`).

**The engine was NOT the defect and was NOT changed.** `engine/src/lib.rs` is **byte-identical across all of
fix-4** (verified by `git diff 9898fd8e..9c87d758 -- src/rust/crates/fathomdb-engine/src/lib.rs`, empty).
Accepting the echo as a **value** rather than a selector is precisely the **R-20-E2 defect this slice exists to
fix** — provenance must be **caller-grounded**, never taken from the LLM's own echo. **Failing loudly is
correct.** The defect was in the **extractor protocol**, which never required per-entity attribution.

**The fix (contract side, `9550bcde`).**

1. Entities are now backfilled **symmetrically with edges** (`elps_live_harness.py:233-237`).
2. `_STUB_ENTITIES` (a module-level list) became **`_stub_entities(doc_id)`** returning **fresh dicts** (`:99`).
   The module-level list was a **latent aliasing bug**: backfilling it in place would have let the **last**
   document overwrite **every earlier document's** provenance — silently mis-attributing, which for an erasure
   slice is worse than the loud failure.
3. The per-entity requirement is now **explicit in the extractor prompt and schema** (`:42`, `:46`, `:70`).
4. A new **engine-level multi-doc test target** was added — `multidoc_extractor_provenance` (**5/5**).

**⚠ BEHAVIORAL CONTRACT CHANGE — record it.** Multi-document extractor batches now require **per-entity
attribution**. **A caller whose extractor cannot attribute must submit single-document batches.**

**The coverage gap that let it through: every existing extractor test was SINGLE-DOC only** — and a single-doc
batch takes the short-circuit path that never consults the echo at all. The regression was invisible by
construction.

### 11.8 Python verification — the honest number

Full suite, **isolated fresh clone with its own venv** (never the shared `.venv`):

```text
2 failed, 754 passed, 7 skipped   ·   exit 1
```

**This is NOT a regression, and this board says why.**

- **The identical two tests fail on `origin/main` `f22e4947`** in the same isolated clone (targeted main run:
  `2 failed, 8 passed`). **The branch's failure set is identical to main's.**
- An expected **"755 passed / 1 failed" was NOT REACHABLE.** It assumed
  `test_option2_elps_pipeline::test_build_fathomdb_elps_path_uses_ingest_with_extractor` would go green once the
  ingest regression was fixed — but **that test also fails on main**, at a later point.
- **The regression IS fixed.** The ELPS test now gets **all the way past ingest** (`blocker is None`,
  `adapter is not None`, `_use_graph_arm is True`, `db.exists()` all pass) and dies at `adapter.retrieve(...)`
  — **the same pre-existing failure point as main**.
- **Both remaining failures share ONE pre-existing root cause** (**TC-31**, §4 #13): `PySearchHit.source_id` is
  populated **only for graph-arm hits** (`fathomdb-py/src/lib.rs:537-539`), so `_doc_id_of` falls through to
  `int(sh.id)`, which has raised `TypeError` since 0.8.19 made `SearchHit.id` an `IdSpace` (C-2). `_doc_id_of`
  is **byte-identical on main**. **One read-side fix likely closes both.** Scheduled separately —
  **out of scope for Slice 5.**
- **7 skips, all environmental/opt-in**: musique corpus absent (`data/corpus-data/` is gitignored),
  `RELEASE_SURFACE_TESTS != 1`, `FDB_S15A_INTEGRATION` opt-in. **No skip masked a pass; no skip came from a
  missing binding.**
- **Shared `.venv` integrity verified intact:** `/home/coreyt/projects/fathomdb/.venv/.../fathomdb.pth` mtime
  still **2026-07-09**, still pointing at the **main** repo.

**TC-20 eu7 hard gate — verified by INSPECTION ONLY, with ZERO eu7 runs.** `eu7_real_corpus_ac` now carries
`#[ignore]` (`:760`) and `scripts/agent-test.sh` can no longer invoke it; a **control** was run to prove the
check was not vacuous. The prohibition on running eu7 was honored in the course of enforcing it.
