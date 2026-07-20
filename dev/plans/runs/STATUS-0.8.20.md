# STATUS — FathomDB 0.8.20 · OPP-12 Phase-2 + erasure completeness + the breaking-pair publish

> **Board of record** for 0.8.20 (`orchestration.md` §12.5). Ladder: `dev/plans/plan-0.8.20.md`.
> Design of record: `dev/design/0.8.20-erasure-and-h-end-state-v4.md`.
> Slice-0 design (v5 addendum): `dev/design/0.8.20-slice0-erasure-design.md`.
> **Update at every slice close.** Verify state from git, never from narration.

**Release base:** `4ca70ba6` · **Orchestration worktree:** `/home/coreyt/projects/fathomdb-worktrees/orch-0.8.20`
(branch `orch-0.8.20`, dedicated linked worktree per **TC-RUBRIC-5**).
**Slice 5 is LANDED** at **`1f8ed8bf`** — it is in `origin/main`. *(This board previously described it as
"awaiting Steward land"; that was stale. §11 is retained as the historical close record.)*
**Slice-10 worktree:** `/home/coreyt/projects/fathomdb-worktrees/orch-0.8.20-s10` (branch `orch-0.8.20-s10`,
rebased onto `origin/main` `ae44770f`), terminal HEAD **`93a57b10`** — **COMPLETE on-branch, NOT landed.**
**Slice-15 worktree:** `/home/coreyt/projects/fathomdb-worktrees/orch-0.8.20-s15` (branch `orch-0.8.20-s15`,
based on `29eba153`), terminal HEAD **`a8087dfb`** (docs/artifacts `cd5620be`) — **NOT landed.**
**⚠ Slice 15 is PARTIAL: TC-34 closed; R-20-PR + R-20-EAV + TC-33 NOT STARTED; Slices 20/25 stay BLOCKED.**
**Last updated:** 2026-07-20 (Slice 15b closed on-branch; §13).

---

## 1. Current state

| | |
|---|---|
| **Slice in flight** | **Slice 15 — Phase-2 keystone** — **⚠ IN PROGRESS / PARTIAL.** Only **TC-34** (+ an unscoped search-validity coherence fix) has closed. **R-20-PR, R-20-EAV and TC-33 are NOT STARTED.** |
| **Status** | `orch-0.8.20-s15` @ **`a8087dfb`** (docs/artifacts `cd5620be`), based on `29eba153`. **TC-34 CLOSED** — node-validity write-side authoring as **optional fields on the existing node write item**, **zero new commands** — plus **search-validity coherence**: `ReadView` now governs `search`, across **five** hydration sites, filtering **before** the vector cutoff and binding **one instant per query**. codex §9 ran **four rounds to a TERMINAL PASS**, **no verdict overridden**. Gates green at `a8087dfb` (§13.3). **Not landed — the Steward lands it.** **Slice 15 itself remains OPEN.** |
| **Blocks** | **Slices 20 and 25 remain BLOCKED** — they depend on **R-20-PR (the projection registry), which does not exist**. TC-34 closing does **not** unblock them. Slice 30 (H7) depends on 10/15/20/25. **Publish remains blocked on AC-079**, which is **still unsigned**. |
| **Next action** | **Return to Steward: land `orch-0.8.20-s10` and `orch-0.8.20-s15`, then commission the REMAINDER of Slice 15 — R-20-PR + R-20-EAV + TC-33.** Nine HITL decisions are owed — **TC-33**, the **Slice-10** and **Slice-15b** governed-surface deltas, the carried **AC-079 sign-off**, and **§4 #18–#22**. |

**Slice 5 is COMPLETE and LANDED** at **`1f8ed8bf`** (in `origin/main`). Its close record is §11.

**Slice 0 is COMPLETE and HITL-SIGNED** (`403eb254`, 2026-07-19) — the X0 gate is open and slices 5+ are
authorized. eu7 baseline capture remains **BLOCKED** (§6.3); resolve before Slice 40.

---

## 2. Slice ladder

| Slice | Title | Depends-on | Status |
|------:|-------|-----------|--------|
| **0** | **X0 design gate** | — | **COMPLETE — HITL-SIGNED, landed `403eb254`** |
| **5** | **Erasure completeness (R-20-E1…E8, +E9a)** | 0 | **COMPLETE — LANDED `1f8ed8bf`** (in `origin/main`). Close record §11 |
| **10** | **`ReadView` / read-modes + node-validity (R-20-RV, R-20-NV)** | 0 | **COMPLETE on-branch @ `93a57b10`** — SCHEMA 21→22; **not landed** (§12) |
| **15** | **Projection registry (C-1) + EAV/property-FTS (R-20-PR, R-20-EAV) + TC-34 + TC-33** | 0, 10 | **⚠ IN PROGRESS / PARTIAL** — **TC-34 CLOSED** on-branch @ `a8087dfb` (+ search-validity coherence; §13). **R-20-PR, R-20-EAV and TC-33 NOT STARTED — no code.** **Slice 15 stays OPEN** |
| 20 | `dense_readiness` + `flush_embeddings()` (R-20-DR) | 15 | **BLOCKED on R-20-PR** (the registry does not exist) — not started |
| 25 | Surrogate minting — governed entities ONLY (R-20-SUR) | 15 | **BLOCKED on R-20-PR** (the registry does not exist) — not started |
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

**Slice 10 minted NO AC.** Its governed-surface delta is recorded as a **PROPOSAL, NOT SIGNED** (§12.5), the same
shape Slice 5d used. **`AC-079` remains available and unminted** — Slice 5's delta is still awaiting the sign-off
that would consume it, so Slice 10 did not mint over it. **AC-041 is GREEN on the Slice-10 branch too**, verified
**live in both bindings**: `test_no_recovery_surface.py` and `no-recovery-surface.test.ts` ran inside the
zero-failure suite runs of §12.3. **Denylist unchanged at exactly five.**

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
| 11 | **`maturin develop` fires AUTONOMOUSLY** from `src/python/tests/conftest.py::_ensure_test_hooks_binding` — merely running the Python suite from a worktree attempts to rebind the **shared** `.venv`. Observed live in fix-3. **GUARD SHIPPED in fix-4, then CORRECTED in fix-6** — fix-4's env-var guard raised at import time and made the *documented* default path permanently red; fix-6 restates the policy positively as a pure function returning `PROCEED`/`REBUILD`/`DEGRADED`/`CONTRADICTORY` (`src/python/tests/_test_hooks_gate.py`). The load-bearing check is **`venv_belongs_to_source_tree()`** — `maturin develop` may run **only** when the venv prefix lies **inside the repo root** — and **the opt-in env var CANNOT override it**. See **§11.9** | **TC-27** | **RESOLVED** (ledger **seq-48**), closed by tooling (fix-the-tooling, not a be-careful note), and closed **structurally** rather than by an env var. *No damage occurred:* the shared `.venv` was re-verified intact — `/home/coreyt/projects/fathomdb/.venv/.../fathomdb.pth` mtime still **2026-07-09**, still pointing at the **main** repo |
| 12 | **Pending-redaction queue hardening** — its "a row is removed ONLY when the obligation is discharged" invariant is upheld by **three correct call sites, not structurally**. codex found a defect in this one mechanism on **each** of rounds 1, 2 and 3 | **TC-28** | **Make it structural** (own table with no generic `DELETE` verb, or a trigger). Every known path is now closed but **nothing prevents a fifth.** Deliberately NOT attempted inside a fix round |

**Raised by Slice 5 fix-4** (details in §11.7/§11.8):

| # | Decision | Ledger | Recommendation |
|---|---|---|---|
| 13 | **✅ RESOLVED at Slice 10** (`63dfbc08` — `source_id` now populated on **every** hit path, not just the graph arm; RED test `f29f7d91`). **WRITE/READ PROVENANCE ASYMMETRY.** 0.8.20 makes provenance **mandatory on write** (R-20-E2, `SourceId`) but it is **unreadable on a text or vector hit**: `PySearchHit.source_id` is populated **only for graph-arm hits** and is `None` for every two-arm hit (`fathomdb-py/src/lib.rs:537-539`). Consumers therefore fall back to `int(sh.id)`, which has raised `TypeError` since 0.8.19 made `SearchHit.id` an `IdSpace` (C-2). This is the measured form of the known "NO SDK EXPOSURE" erasure gap — a caller cannot tell which document a hit came from, so it cannot audit or scope an erasure | **TC-31** | **Schedule a read-side fix** — populate `source_id` on every arm. **One fix likely closes BOTH** residual Python failures (§11.8). **OUT OF SCOPE for Slice 5**: `_doc_id_of` is **byte-identical on `main`** and both failures reproduce there |
| 14 | **ENTITY-DEDUPE ERASURE GAP, adjacent to R-20-E2 — found in fix-4, NOT fixed. ✅ RULED ON (HITL, 2026-07-20): ACCEPTED AS-IS, no behavior change** — annotated in code at Slice 10 (`e62309e1`). **Carry-forward caveat: the erasure guarantee MUST NOT be stated unconditionally to users while co-named-entity dedupe stands.** Entities dedupe **within a batch** by `logical_id` derived from `(kind, name)`, so two documents naming the same entity **collapse to one row** carrying the **FIRST** document's provenance. Erasing the second document therefore **leaves that entity behind**, still attributed to the first. An **erasure-completeness gap**: the slice's own guarantee ("erase every row owned by this source") does not hold for a co-named entity | **TC-32** | **Entity-identity design question, not a fix round.** Options: per-source entity rows, or a multi-valued provenance edge set. Must be decided before the erasure guarantee is stated unconditionally to users |

**Raised by Slice 10** (details in §12):

| # | Decision | Ledger | Recommendation |
|---|---|---|---|
| 15 | **Node validity has NO write-side authoring verb.** `valid_from`/`valid_until` are **queryable but not settable from any SDK** — the tests author windows via **direct SQL**. **Is R-20-NV met without it?** The read half is complete and closed; the write half does not exist on the governed surface | **TC-34** | **HITL call, not an implementer call.** Either (a) ratify R-20-NV as read-only for 0.8.20 and schedule the authoring verb, or (b) re-open Slice 10 to add it. Note the coupling: an authoring verb is a **governed-surface addition**, so it lands with a delta and a sign-off |
| 16 | **TEMPORAL-MODEL SPLIT.** Node validity is **INTEGER epoch**; the shipped edge `t_valid`/`t_invalid` are **ISO-8601 TEXT**. Edges were **deliberately untouched**, and the divergence is **pinned by two tests** so it cannot drift silently | **TC-33** | **Accept long-term, or schedule a unifying slice.** Recorded as a deliberate divergence with an explicit migration note in the step-22 SQL — **not** an accident. Unifying is a breaking migration and belongs in its own slice if wanted |
| 17 | **Slice-10 governed-surface delta — PROPOSED / NOT SIGNED.** Adds commands `read.crossed_boundary_since` / `read.crossedBoundarySince` and types `ReadView`, `BoundaryCrossing` | — | **Sign or amend before publish**, together with **AC-079** (#7). Recorded exactly as Slice 5d recorded its own. **Recovery denylist UNCHANGED at five; AC-041 GREEN** |

Also logged by Slice 10 and **not** requiring a decision: **TC-35** (napi `#[napi(object)]` **OMITS** the property
for `Option::None` rather than emitting `null` — **measured, not reasoned**; drove the `9a6e4896` shape fix) and
**TC-36** (the published API docs still declare `SearchHit.id` as `int`/`number` "write_cursor" — **stale since
0.8.19 C-2** made it an `IdSpace`; a docs defect, pre-existing, not introduced here).

**Raised by Slice 15b** (details in §13):

| # | Decision | Ledger | Recommendation |
|---|---|---|---|
| 18 | **Error-variant choice.** An unsatisfiable window raises **`EngineError::InvalidArgument`** (carrying **both** bounds) rather than the **message-less `WriteValidation`** its sibling checks use. Python therefore raises **`InvalidArgumentError`** for an inverted window but **`WriteValidationError`** for a non-integer bound | — | **Deliberate** — a semantic violation is not a type-shape violation, and the caller needs the bounds in the message. But it **is** a family inconsistency. **One line plus tests to reverse**; cheapest to settle now, before the surface is signed |
| 19 | **`search` view is scoped to the VALIDITY AXIS ONLY.** `include_superseded` / `include_inactive` on a **search** view are a **typed refusal** | — | **Accept.** Search hydrates from **projection indexes that are not version-complete**, so there is **no truthful answer** to give — a refusal is honest where a silent partial answer would not be. **Reversible via one guard function** if the indexes later become version-complete |
| 20 | **Vector-cutoff limitation** (§13.2). Recall is restored **only within the 192-candidate bit-KNN pool**; with >192 expired near-neighbours the result set can still be short | — | **Either** accept the bounded-192-pool behaviour as the pre-existing ANN bound, **or** schedule the `canonical_nodes(write_cursor)` index + the `EXISTS` general fix. The latter is a **schema step**, and was **deliberately not taken inside a fix commit** |
| 21 | **The five read verbs still call `view.now_param()` directly.** **Correct today** — they are single-arm queries — but it is the **same latent shape** as the fix-3 defect: an instant re-read per arm rather than bound once per query | — | **Follow-up, not scope creep.** ~**24 call sites**. Recommended as its own small slice rather than folded into a fix round |
| 22 | **⚠ SLICE 15 IS INCOMPLETE.** **R-20-PR, R-20-EAV and TC-33 are NOT STARTED** — design work exists, **no code**. **Slices 20 and 25 remain BLOCKED** on the registry | — | **Commission the remainder of Slice 15.** TC-34 closing does **not** unblock 20/25 — only **R-20-PR** does. The board must not be read as the keystone having landed |

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
| `fathomdb-worktrees/orch-0.8.20-s5` | `orch-0.8.20-s5` | Slice 5 erasure completeness | Slice 5 **landed** (`1f8ed8bf`) — **reclaimable** |
| `fathomdb-worktrees/orch-0.8.20-s10` | `orch-0.8.20-s10` | Slice 10 `ReadView` + node-validity | **active** — holds **`93a57b10`**, **do not remove before land** |

Clean up per `orchestration.md` §11 — **one destructive op per Bash call**; never `find -delete`.

---

## 8. Recent decisions (newest first)

- **2026-07-20 — Slice 10 COMPLETE on-branch** at **`93a57b10`**. **R-20-RV + R-20-NV closed**; **SCHEMA 21 → 22**
  (node validity window); **TC-31 RESOLVED** — `source_id` is now readable on **every** search-hit path, closing
  the measured "NO SDK EXPOSURE" erasure gap on the read side. **Two codex §9 terminal PASSes.** The Python
  failure Slice 5 attributed to TC-31 **now passes**. Opens **TC-33/TC-34** (§4 #15/#16) and logs **TC-35/TC-36**.
  Governed-surface delta **PROPOSED / NOT SIGNED**; **no AC minted**. **Zero eu7 runs.** (§12)
- **2026-07-20 — TC-32 ACCEPTED AS-IS, no behavior change** (HITL). Co-named-entity dedupe is **annotated, not
  fixed** (`e62309e1`). **The erasure guarantee must NOT be stated unconditionally to users while it stands.**
- **2026-07-20 — Slice 5 LANDED** at **`1f8ed8bf`**, in `origin/main`. **AC-079 is still UNSIGNED and still
  blocks publish** — landing the code did **not** discharge the sign-off.
- **2026-07-20 — fix-7: the test-hooks probe was NARROWER than the surface it gated** (`7c353ac5`). It checked
  one of three symbols, so a **partial** binding read as "hooks present" and a marked test **failed on a
  missing import instead of skipping**. Now probes all three, fails safe to DEGRADED, and carries a drift guard
  against `lib.rs`. Found by isolated-clone verification **after** codex's terminal PASS. (§11.10)
- **2026-07-20 — fix-6: codex found a P2 in our OWN TC-27 guard** (`5452016f`, `d710721a`). fix-4's env-var
  guard turned a silent-rebuild hazard into a **permanently red default pytest path**. TC-27 is now stated
  **positively** as a pure policy function, and the load-bearing check is `venv_belongs_to_source_tree()` —
  **the opt-in env var cannot override it**, so the shared `.venv` is protected **structurally**. codex
  returned a **terminal PASS** on the delta. **TC-27 RESOLVED** (ledger seq-48); **TC-16 corrected**
  (seq-49: the dead assertion is in `test_actionlint_fixture.sh`, aborting `agent-test.sh` at line 63 **before**
  the Rust and Python steps, so its exit code is not a suite verdict). **Lesson: a guard that breaks the
  documented default path is a worse defect than the hazard it closes.** (§11.9)
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
| 7 | R-20-PUB | **The publish dry-run guard is DEAD and has been red since 0.8.14.** `test_actionlint_fixture.sh:53` greps `release.yml` for `cargo publish --dry-run -p`, but the job now delegates to `cargo-publish-if-new.sh --dry-run`. **Behavior is intact** (the helper forwards correctly) — but `./scripts/agent-test.sh` exits 1 wholesale, so a **real** publish-wiring regression would be invisible **in the first release that publishes for real**. **⚠ CORRECTED (ledger seq-49):** the dead assertion is **NOT** in `test_pypi_publish_roundtrip.sh` (that script passes cleanly) — it is in **`scripts/tests/test_actionlint_fixture.sh`**, invoked at **`scripts/agent-test.sh` line 63**. Because `set -euo pipefail` aborts there, **`agent-test.sh` never reaches the Rust or Python steps**, so its aggregate exit code says **NOTHING** about whether those suites pass. Confirmed **pre-existing**: that script and `.github/workflows/release.yml` are byte-identical to `origin/main`. | **TC-16** |
| 8 | v4 §3.2 | **Slice 5's `SourceId` newtype will break the eu7 harness** (`eu7_real_corpus_ac.rs:405` builds `PreparedWrite` with `source_id: None`). v4 enumerated only two internal callers and missed the test-side ones. Sweep `src/` **and** `tests/`. | **TC-17** |
| 9 | TC-RUBRIC-7 | Committing a §9 transcript **into the reviewed range** pollutes the next review's diff (codex re-read its own prior findings as if unfixed). Recommend committing transcripts **after** the final review round. | **TC-18** |

**Also carried:** the eu7 basis and `embed_batch_cls` decisions (§4 #1/#2) remain **HITL calls**, recorded with
recommendations, not decided here.

---

## 11. Slice 5 close — erasure completeness (R-20-E1…E8)

**Branch `orch-0.8.20-s5`, terminal HEAD `d710721a` + fix-7** — cut from `origin/main` `19b568e2`, rebased onto
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
| `5452016f` | **fix-6** — codex **P2**: fix-4's TC-27 guard broke the default pytest path; restate the policy positively (§11.9) |
| `d710721a` | **fix-6** — docs/ledger for the above |
| `7c353ac5` | **fix-7** — probe **all three** test-hook symbols so a partial binding DEGRADES (§11.10) |

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
| **fix-6** | `d710721a` | **codex P2 in our OWN TC-27 guard** — fix-4 turned a silent-rebuild hazard into a **permanently red default pytest path**. Policy restated positively as a pure function; the ownership check, not the env var, is load-bearing (§11.9) |
| fix-7 | `7c353ac5` | The test-hooks probe checked **one** of the **three** symbols it gates, so a partial binding read as PROCEED and a marked test **failed instead of skipping**. Probe all three; fail safe to DEGRADED (§11.10) |

The central defect this slice closes: **`search_index_v2` stores the body**, so before R-20-E1 an excised body
**survived erasure** in that table. It never surfaced in results (both read paths gate on `canonical_nodes`),
which is exactly why it went unnoticed — a data-at-rest leak, invisible to any result-level assertion.

### 11.2 codex §9 — four rounds on the branch, then two delta rounds, terminal PASS

Transcripts under `dev/plans/runs/codex/0.8.20/` (TC-RUBRIC-7 path), committed **after** the final round per
TC-18.

| Round | Transcript | Verdict |
|---|---|---|
| 1 | `slice-5-20260719T231341Z.log` | **P1** legacy edges erasable by **no verb**; **P2** telemetry-redaction retry falsely reports success |
| 2 | `slice-5-fix-1-rereview-20260719T234803Z.log` | P1 cleared; **P2** doctor gives false assurance on unerasable edges; **P2** freeze-before-drain timeout |
| 3 | `slice-5-fix-2-rereview-20260720T001616Z.log` | **P1** `excise_collection_record` could delete the pending-redaction queue; **P2** rotated sink treated as redacted |
| 4 | `slice-5-fix-3-rereview-20260720T005056Z.log` | **TERMINAL PASS** — *"No actionable correctness issues were found in the reviewed diff. The added erasure/provenance paths appear consistently wired through Rust, Python, TypeScript, CLI, schema migration, and tests."* |
| 5 (fix-4/5 delta) | `slice-5-fix-4-5-delta-20260720T022544Z.log` | **P2** — the fix-4 TC-27 guard **broke the documented default pytest path** (import-time raise before collection). Fixed in fix-6 (§11.9) |
| 6 (fix-6 delta) | `slice-5-fix-6-rereview-20260720T024726Z.log` | **TERMINAL PASS on the delta** |

**Read the round count honestly:** rounds 1–4 reviewed the **full branch** (P1+P2 → P2+P2 → P1+P2 → PASS);
rounds 5–6 reviewed only the **fix-4/5 and fix-6 deltas**. **fix-7 has NOT been through codex** — it was found
by isolated-clone verification after the terminal PASS and is covered by §11.10's executed evidence.

### 11.3 Gates — re-verified on the terminal HEAD (real exit codes)

Re-run at **fix-7** (`7c353ac5`). Read via `$?` / `PIPESTATUS`, never a trailing `echo`.

**⚠ Invocation matters — a bare invocation of the first two is NOT a run.** `erasure_projection_registry` and
`provenance_mandatory` live in **`fathomdb-engine`** and **require `--features operator`** (without it, `cargo
test` exits **101**). `sdk_only_erasure` lives in **`fathomdb`** and needs the explicit
`cargo test -p fathomdb --test sdk_only_erasure` (TC-25).

| Gate | Result |
|---|---|
| `cargo clippy --workspace --all-targets` | **0** — and **zero `dead_code`** on a non-`operator` build (fix-5) |
| `cargo check --workspace --all-targets` | **0** |
| `erasure_projection_registry` | **4/4** — `-p fathomdb-engine --features operator` |
| `provenance_mandatory` | **3/3** — `-p fathomdb-engine --features operator` |
| `multidoc_extractor_provenance` (**fix-4**) | **5/5** |
| `erasure_completeness` (AC-080) | **10/10** |
| `sdk_only_erasure` | **3/3** — via **explicit non-operator invocation** (TC-25: it is `#![cfg(not(feature = "operator"))]`, so any feature-unified run compiles it to **zero** tests and reports success having asserted nothing) |
| `no_recovery_surface` (**AC-041**) | **1/1** — denylist unchanged at five |
| `governed_surface` | **3/3** — *against an unsigned proposal*, see §3 |
| `fathomdb-schema`, all targets | green, incl. new `step21_migration.rs` **5/5** |
| `fathomdb-cli`, all targets | green |
| TypeScript | **170/170**, `tsc` **0** |
| **Python** | **`2 failed, 766 passed, 7 skipped`** (hooks available) in an **isolated fresh clone** — failure set **identical to `origin/main`**, both pre-existing (**TC-31**). See **§11.8**; do **not** read this row without it |
| `ruff check src/python` | **0**; `py_compile` clean on every file fix-7 touched |
| `test_test_hooks_gate.py` (fix-7) | **20/20** — synthetic complete / partial / import-failure bindings; **no compiled extension required** |
| `SCHEMA_VERSION` | **20 → 21** |

**⚠ `scripts/agent-test.sh`'s aggregate exit code is NOT a suite verdict** — it aborts at line 63 on the
pre-existing dead publish assertion (**TC-16**, §10 #7) and never reaches the Rust or Python steps. Gate on the
individual commands above, not on that script. The **invocation** `agent-test.sh` uses for pytest was run
directly and is healthy (**`2 failed, 766 passed, 7 skipped`**); `cargo test --workspace --no-fail-fast` exits
**0** across 148 test binaries.

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

Full suite, **isolated fresh clone with its own venv** (never the shared `.venv`). **Re-executed at `d710721a`**
across all three environment states — the numbers below are **runs, not reasoning**:

```text
hooks available (in-tree venv)      2 failed, 766 passed,  7 skipped   ·   exit 1
default path (hook-less, no opt-in) 1 failed, 762 passed, 12 skipped   ·   exit 1
degraded (FATHOMDB_TESTS_NO_REBUILD=1) 1 failed, 762 passed, 12 skipped · exit 1
```

**What each state proves.**

- **Default path is NOT red-by-construction any more** (the fix-4 defect, §11.9): collection **succeeds**, 775
  items, **no import-time raise**, and the degraded banner is on screen before the first test.
- **The hook-dependent tests genuinely RAN and PASSED** when hooks were available — **verified three ways**,
  including an explicit verbose re-run. They did **not** skip. This is the check that distinguishes a real pass
  from a vacuous one.
- **Degraded is not a session-wide self-skip:** **exactly two** marker skips, each with a clear reason, and
  **762 tests still ran**. The extra skips vs the hooks-available run are the two markers plus three
  `test_verify_embed_db` tests whose module-scoped fixture cannot build a real embed DB without the hooks.
- **The ownership check holds:** in-clone venv → owned; **shared `/home/coreyt/projects/fathomdb/.venv` → NOT
  owned**; worktree venv → not owned; and `decide(allow_rebuild=True, venv_owned=False)` → **`degraded`, not
  `rebuild`**. **The opt-in env var cannot override it.**

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
  **out of scope for Slice 5.** Re-verified at fix-7: `git diff origin/main...HEAD` is **empty** over
  `src/python/tests/test_option2_elps_pipeline.py`, `src/python/tests/test_verify_embed_db.py` **and**
  `eval/r2_parity_eval.py` — this branch did not touch the failing surface at all.
- **7 skips, all environmental/opt-in**: musique corpus absent (`data/corpus-data/` is gitignored),
  `RELEASE_SURFACE_TESTS != 1`, `FDB_S15A_INTEGRATION` opt-in. **No skip masked a pass; no skip came from a
  missing binding.**
- **Shared `.venv` integrity verified intact:** `/home/coreyt/projects/fathomdb/.venv/.../fathomdb.pth` mtime
  still **2026-07-09**, content still `/home/coreyt/projects/fathomdb/src/python`; the shared `.so` untouched.
  Re-verified after **every** round through fix-7.

**TC-20 eu7 hard gate — verified by INSPECTION ONLY, with ZERO eu7 runs.** `eu7_real_corpus_ac` now carries
`#[ignore]` (`:760`) and `scripts/agent-test.sh` can no longer invoke it; a **control** was run to prove the
check was not vacuous. The prohibition on running eu7 was honored in the course of enforcing it.

### 11.9 fix-6 — codex found a P2 in our OWN TC-27 guard

**The guard for a hazard became a worse defect than the hazard.** fix-4 closed the autonomous-`maturin develop`
hole by **raising at import time** when a rebuild was not authorized. But `conftest.py` is imported before
collection, so a clean, **documented** checkout — `pip install -e 'src/python[dev]'`, whose
`[tool.maturin] features` deliberately ships **no** `test-hooks` surface — raised **before a single test was
collected**. The default path was **permanently red**, and the fix traded a silent-corruption risk for a
guaranteed outage.

**fix-6 restates TC-27 positively.** The policy is now a **pure function** —
`src/python/tests/_test_hooks_gate.py`, no I/O, no environment, no subprocess — returning one of
`PROCEED` / `REBUILD` / `DEGRADED` / `CONTRADICTORY`. It is unit-tested **without a binding, a venv, or a
build**, which is exactly the configuration the policy exists to handle.

**The load-bearing check is `venv_belongs_to_source_tree()`, NOT the env var.** `maturin develop` may run
**only** when the venv prefix lies **inside the repo root** that owns `src/python`. The opt-in env var
**cannot override it**: `decide(allow_rebuild=True, venv_owned=False)` → **`degraded`**. The shared `.venv` is
therefore protected **structurally**, by the shape of the filesystem, rather than by an environment variable
someone might export. `scripts/agent-test.sh` now sets the opt-in **itself**, when it has selected the in-tree
`.venv` — the authorization is issued by the thing that knows it is safe.

A missing surface **degrades**: the suite runs, and only the tests marked `@pytest.mark.requires_test_hooks`
skip — visibly, with the reason, plus a banner in the pytest header at any verbosity.

### 11.10 fix-7 — the probe was narrower than the surface it gated

**Found by isolated-clone verification on a real `.so`, after codex's terminal PASS.**

`_binding_has_test_hooks()` probed **one** symbol, `Engine._write_vector_for_test`, while the gate it drives
protects **three** (`src/rust/crates/fathomdb-py/src/lib.rs`, each behind
`#[cfg(any(test, feature = "test-hooks"))]`): `Engine._configure_vector_kind_for_test` (`:1239`),
`Engine._write_vector_for_test` (`:1247`), and module-level `force_panic_for_test` (`:2038`).

**The observed failure mode.** A binding can carry both `Engine` methods while module-level
`force_panic_for_test` is **absent** — reachable from a stale or interrupted build. The single-symbol probe
called that **"hooks present" ⇒ PROCEED**, so the `requires_test_hooks` skips did **not** apply and
`test_panic_surfaces_as_python_exception` **failed on a missing import instead of skipping cleanly**. The
narrow probe is the classic vacuous-gate shape: a check weaker than the thing it certifies.

**The fix.** The probed set lives next to the gate as `TEST_HOOK_SYMBOLS`, so it cannot drift from the surface
it gates, and the surface counts as present only if **all** of it is. A partial binding yields **DEGRADED** and
leads its reason with what is actually missing — *"built WITHOUT test-hooks"* is the wrong diagnosis for it. A
crashed or unparseable probe **fails safe** to "the whole surface is absent": DEGRADED, never PROCEED.

**Evidence is executed, not reasoned:** the probe was run against **synthetic** bindings — complete, partial
(both `Engine` methods, no `force_panic_for_test`), and import-failure — so the tests need **no compiled
extension**. `test_test_hooks_gate.py` **20/20**. A drift guard asserts each probed symbol is still
`test-hooks`-gated in `lib.rs`. The three fix-6 requirements were re-verified and hold: the default path still
collects and runs, the ownership check is untouched and still un-overridable, and a missing surface still
produces a visible skip.

**Left deliberately unfixed (cosmetic, and NOT worth the risk).** Three `test_verify_embed_db.py` tests depend
on the hook surface without carrying the marker, so in degraded mode they skip with an internal error string
(`'Engine' object has no attribute '_configure_vector_kind_for_test'`) rather than the gate's reason. **They do
skip visibly** — this is presentation only. It was left alone because `test_verify_embed_db.py` is currently
**byte-identical to `origin/main`**, and that identity is load-bearing evidence for the **TC-31**
pre-existing-failure attribution above. Editing it for cosmetics would destroy the proof.

---

## 12. Slice 10 close — `ReadView` / read-modes + node-validity (R-20-RV, R-20-NV)

**Branch `orch-0.8.20-s10`, terminal HEAD `93a57b10`** — rebased onto `origin/main` **`ae44770f`**.
**COMPLETE on-branch. NOT landed — the Steward lands it.**

**R-20-RV and R-20-NV are CLOSED. TC-31 is RESOLVED. TC-32 is ANNOTATED** per the HITL ruling (accepted, no
behavior change). **No AC was minted** — see §3.

### 12.1 What shipped

| Commit | Content |
|---|---|
| `f29f7d91` | **RED** — `source_id` must be readable on every search-hit path |
| `63dfbc08` | **TC-31 fix** — populate `SearchHit.source_id` on **every** hit path, not just the graph arm |
| `e62309e1` | **TC-32** — annotate the accepted single-provenance entity dedupe |
| `b90c9a0d` | **TC-31** — IdSpace-safe doc-id resolution at the two remaining eval sites |
| `9392dbc5` | **TC-31 fix-1** — correct the remaining stale "`source_id` is graph-arm-only" contract text |
| `43ae248f` | Slice-10a closure artifacts + the codex §9 PASS transcript |
| `9c6420e5` | **R-20-NV** — schema **step 22**, `canonical_nodes` validity window (**SCHEMA 21 → 22**) |
| `e3cc071b` | **R-20-RV/R-20-NV** — thread `ReadView` through **all five** read verbs + both bindings |
| `4524ffd2` | Read-mode + validity matrices; Py/TS parity; the surface delta |
| `e069e3a9` | Record the `ReadView` / `BoundaryCrossing` surface delta (Rust docs) |
| `c5e12da6` | Slice-10b closure artifact |
| `742a347e` | `BoundaryCrossing` boundaries are `number \| null`, not `?: number` — **superseded by fix-3** |
| `14d33bba` | **X1** — live Py + TS functional harnesses for the read-view surface |
| `073b2d3a` | Slice-10b fix-2 closure artifact — X1 binding-execution parity |
| `9a6e4896` | **fix-3 (TC-35)** — napi **OMITS** `None` `Option` object fields; **measured, not reasoned** |
| `a6c849ee` | Slice-10b fix-3 closure artifact — the measured napi object-field shape |
| `cf92d1c4` | codex **[P2]** — annotate the neighbors direction matrix as `TraversalDirection` |
| `93a57b10` | Annotate the `_doc_id_of` `getattr` result as `Any` (pyright **12 → 8**) |

**The five read verbs are `read_get`, `read_get_many`, `read_list`, `read_list_filter`, `graph_neighbors`.**
*(The plan's §3 shorthand "`get`/`list`/`neighbors`" named no real symbol; corrected there.)*
**`graph_neighbors` has THREE direction variants, not four** — `Outgoing` / `Incoming` / `Both`
(`engine/src/lib.rs:1948-1952`). The 4th CTE that made the brief say "four" is **`build_bfs_with_depth_sql`**,
which serves **`search_expand`** — **not one of the five read verbs**, and **deliberately left on the strict
path**.

### 12.2 Schema — 21 → 22

Step 22 adds `canonical_nodes.valid_from` / `valid_until`: **INTEGER epoch seconds, nullable**, half-open
**`[valid_from, valid_until)`**, **NULL = unbounded**. **Existing rows back-fill NULL/NULL ⇒ always valid ⇒
default-view visibility is unchanged.** The INTEGER choice **deliberately diverges** from the shipped
ISO-8601 TEXT `canonical_edges.t_valid`/`t_invalid`, which are **untouched** — the divergence is **pinned by two
tests** and carries a migration note in the step-22 SQL, so it cannot drift silently. **That divergence is
TC-33, and it is a decision owed to the HITL** (§4 #16) — it is recorded here as deliberate, not as settled.

### 12.3 Gates — ONE fresh clone at exactly `93a57b10`, everything SERIAL

The clone head was verified **equal to the branch head** before any gate ran. Real exit codes throughout.

| Gate | Result |
|---|---|
| `cargo clippy --workspace --all-targets` | **exit 0** |
| `cargo check --workspace --all-targets` | **exit 0** |
| `cargo test -p fathomdb-engine -p fathomdb-schema -- --test-threads=1` | **exit 0** — **540 passed / 0 failed** |
| `cargo test -p fathomdb --test governed_surface` | **exit 0** |
| **Python** | **787 passed / 12 skipped · exit 0** — fresh clone, **own venv**, `pip install -e "src/python[dev]"`; **never** the shared `.venv` |
| **TypeScript** | **186 pass / 0 fail · exit 0** |
| `pyright -p src/python` | **8 errors, exit 1** — **the pre-slice baseline is ALSO 8**; see below |
| **AC-041** (`test_no_recovery_surface.py`, `no-recovery-surface.test.ts`) | **GREEN, live in BOTH bindings** — inside the zero-failure runs above; denylist unchanged at five |
| **eu7** | **ZERO runs, any backend, any N.** `eu7_real_corpus_ac` is still `#[ignore]`d |

**pyright, stated honestly: the project gate was ALREADY RED before this slice, and is not made worse.** The
slice **introduced 4 errors and cleared all 4**; the residual **8 are the pre-existing baseline**. This is
**not** a green gate and is **not** claimed as one.

### 12.4 Python — the honest comparison

**Baseline at `c82feb80`, same method: `1 failed, 770 passed, 12 skipped`.** The single failure was
`test_option2_elps_pipeline.py::test_build_fathomdb_elps_path_uses_ingest_with_extractor` — the **TC-31**
`int(sh.id)` `TypeError`. **It now PASSES**, and the suite is **787 passed / 12 skipped, exit 0**.

**On the earlier "2 failed" figure — both numbers are real; neither disproves the other.** §11.8 row 1 measured
the **hooks-available** environment; the **hook-less default path** shows **1**. They are different environment
states of the same suite, and are recorded as such rather than one being retconned.

### 12.5 Governed-surface delta — **PROPOSED / NOT SIGNED**

Recorded in the same shape Slice 5d used, and for the same reason: the branch is not red, but **that is not an
approval**.

- **Commands added:** `read.crossed_boundary_since` / `read.crossedBoundarySince`
- **Types added:** `ReadView`, `BoundaryCrossing`
- **Allowlist:** the `allowlist` array goes **25 → 27** entries (the two command names above); `core` **unchanged
  at 5**; `recovery_denylist` **UNCHANGED at exactly five** — `["recover","restore","repair","fix","rebuild"]`
- **AC-041 GREEN** in both bindings (§12.3). **No AC minted — `AC-079` remains available and unminted**, since
  Slice 5's delta has not yet consumed it

### 12.6 codex §9 — two terminal PASSes

Transcripts under `dev/plans/runs/codex/0.8.20/` (TC-RUBRIC-7 path), committed with this close.

| Round | Transcript | Verdict |
|---|---|---|
| 10a | `slice-10-20260720T155459Z.log` | **PASS** |
| 10b initial | `slice-10b-20260720T175114Z.log` | **CONCERN** — one **[P2]** (pyright). **Fixed in `cf92d1c4`, NOT overridden** |
| 10b re-review | `slice-10b-rereview-20260720T180124Z.log` | **TERMINAL PASS** |

### 12.7 Owed to the HITL

**§4 #15 (TC-34)** node validity has **no write-side authoring verb** — queryable but not settable from any SDK;
the tests author windows **via direct SQL**. **Is R-20-NV met without it?** · **§4 #16 (TC-33)** the temporal-model
split · **§4 #17** the Slice-10 governed-surface delta · and the carried **§4 #7 AC-079 sign-off**, which still
**blocks publish**. **§4 #14 (TC-32)** is ruled and closed, but its **carry-forward caveat stands: do not state
the erasure guarantee unconditionally to users** while co-named-entity dedupe stands.

Logged, no decision needed: **TC-35** (napi `#[napi(object)]` omits `None` `Option` properties — measured) and
**TC-36** (published API docs still declare `SearchHit.id` as `int`/`number` "write_cursor", **stale since
0.8.19 C-2**).

### 12.8 Closure artifacts

`dev/plans/runs/0.8.20-slice-10a-output.json`, `0.8.20-slice-10a-fix-1-output.json`,
`0.8.20-slice-10b-output.json`, and `0.8.20-slice-10b-fix-{2,3,4,5}-output.json`, plus the three §9 transcripts
in §12.6. Committed with this close per **TC-23** — an untracked closure witness is destructible by routine git
hygiene.

---

## 13. Slice 15b close — TC-34 node-validity authoring + search-validity coherence

**Branch `orch-0.8.20-s15`, terminal HEAD `a8087dfb`** (docs/artifacts at **`cd5620be`**), based on **`29eba153`**.
**COMPLETE on-branch. NOT landed — the Steward lands it.**

> **⚠ SLICE 15 IS NOT COMPLETE. This close covers ONE of its four parts.**
> **TC-34 is CLOSED** (plus a search-validity coherence fix that was **not originally scoped**).
> **R-20-PR (projection registry, the C-1 co-land), R-20-EAV (EAV / property-FTS) and TC-33 (temporal
> harmonisation) are NOT STARTED — no code exists for any of them.** **Slice 15 remains OPEN**, and
> **Slices 20 and 25 remain BLOCKED** on the registry that does not yet exist. Do not read this section as
> the Phase-2 keystone landing.

### 13.1 What shipped

| Commit | Content |
|---|---|
| `f2ce7268` | **RED** — TC-34 node-validity write-side authoring path |
| `35523156` | **GREEN** — TC-34 authoring path (Rust + Py + TS) |
| `31f550a8` | Slice 15b closure witness |
| `ab790880` | **fix-1** — interface contracts for the node-validity write fields (codex **[P2]**) |
| `25943ae8` | **RED** — fix-2, validity window must govern search |
| `41044405` | **fix-2 Part 1** — validity governs every search hydration site |
| `0457908c` | **fix-2 Part 2** — `ReadView` on `search` across Py + TS |
| `83566058` | **RED** — fix-3, vector-cutoff recall + one instant per query |
| `a8087dfb` | **fix-3 GREEN** — validity filters before the vector cutoff; one instant per query |
| `cd5620be` | Closure artifacts — witnesses, 4 codex transcripts, TC-38…42 |

**The shape.** Authoring is via **optional fields on the existing node write batch item** (`valid_from` /
`valid_until`, **INTEGER epoch seconds**; TS accepts both `validFrom`/`validUntil` and the snake_case
spellings) — **not a new verb**. That is exactly symmetric with how `PreparedWrite::Edge` has always accepted
`t_valid`/`t_invalid`. **Zero new commands**; allowlist membership is **byte-unchanged**.

Validation lives in the **engine** (`validate_write`), so Rust, Python and TypeScript share **one** rule and
cannot drift: an **unsatisfiable** window (both bounds present, `valid_from >= valid_until`) is refused
**before any INSERT** and **rejects the whole batch**; a **one-sided** window is **never** refused; a
non-integer bound is a **typed refusal** — Python rejects `bool` **explicitly**, since `bool` subclasses `int`
and `True` must not silently become the instant `1`.

### 13.2 The defect codex found — and why it was in scope

**Slice 10 scoped `ReadView` to the five read verbs and deliberately left `search` out.** That was defensible
**only while no SDK caller could author a window** — raw SQL was the sole route, so the gap was **unreachable**.
**TC-34 made authoring reachable and thereby turned a latent gap into a live defect:** an SDK-authored
out-of-window node **still came back from `Engine::search`** while `read_get`/`read_list` correctly hid it. The
implementer **reproduced it at runtime on the unmodified engine** before fixing it.
`dev/design/record-lifecycle-protocol/api-surface.md:50` had **always** specified `ReadView` on **`search`**, so
the five-verb scope was a **narrowing of the contract, not the contract**.

The fix touched **five** node-hydration sites, not the one codex cited — `bfs_graph_arm_candidates` carried
**three more of the same class**, reachable from governed surface via `search_reranked(use_graph_arm=true)`.

Then **two further [P2]s**: validity was filtering **after** the vector KNN cutoff — a **recall** defect, whose
RED reproduced **ZERO** hits for a query with **two valid matches** — and the graph arm **re-read the clock**
independently of the other arms, a **determinism** defect. **Both fixed.**

**The honest limitation on the vector fix.** Recall is restored **only within the existing 192-candidate
bit-KNN pool** (`TOP_K_BIT_CANDIDATES = 192`, `engine/src/lib.rs:8095`). With **more than 192 expired
near-neighbours** the result set can still come back short. That is the **pre-existing ANN bound of the
two-stage design, not a new one** — but it is **not a fully general fix**, and it is recorded here as such.
The rejected alternative was an `EXISTS` join — **legal**, since phase 2 is an ordinary rowid JOIN and so
**ADR-0.8.11 D3 does not bite there** — rejected on **cost**: the only index on `canonical_nodes(write_cursor)`
is the **PARTIAL** `canonical_nodes_state_active_idx … WHERE state = 'active'` (migration step 20,
`fathomdb-schema/src/lib.rs:516-517`), which **cannot serve a general validity join**, so the `EXISTS` form
would impose a **full scan × the 192-row pool on EVERY search** to fix a degenerate case.

### 13.3 Gates — real exit codes, terminal HEAD `a8087dfb`

| Gate | Result |
|---|---|
| `cargo clippy --workspace --all-targets` | **exit 0** (0 warnings) |
| `cargo check --workspace --all-targets` | **exit 0** |
| `cargo test -p fathomdb-engine -p fathomdb-schema -- --test-threads=1` | **exit 0 — 560 passed / 0 failed** (baseline 540 ⇒ **+20, all new**) |
| `cargo test -p fathomdb --test governed_surface` | **exit 0** |
| `cargo test -p fathomdb --test compile_fail_provenance` | **exit 0** |
| **Python** | **exit 0 — 809 passed / 12 skipped** — **fresh clone at `a8087dfb`**, **own venv inside the clone**, clone head **verified == branch head**; baseline 787 ⇒ **+22 = exactly the new tests** |
| **TypeScript** | **exit 0 — 201 pass / 0 fail** (baseline 186) |
| **markdown lint** | Run with the **PRIMARY checkout's** binary — the worktree script is **vacuous** (**TC-37**). **0 errors in every file this slice touched.** The exit 1 is **entirely** the **9 pre-existing** MD025/MD001 errors in `dev/research/personal-agent-database-market-2026-07-02.md`, **untouched here** |
| **AC-041** | **GREEN**, both bindings; recovery denylist **UNCHANGED at exactly five** |
| **eu7** | **ZERO runs**, any backend, any N; `eu7_real_corpus_ac` still `#[ignore]`d, attribute untouched |

### 13.4 Governed-surface delta — **PROPOSED / NOT SIGNED**

- **Commands added: NONE** from TC-34 — it is **fields only**. **fix-2 Part 2** adds an **optional `view`
  argument to `search`** in **both** bindings.
- **Types:** `ReadView` **reused** — no new type. `recovery_denylist` **UNCHANGED at exactly five**.
  **AC-041 GREEN.**
- **`AC-079` remains available and UNMINTED.**
- Marked **`AWAITING HITL SIGN-OFF, NOT SIGNED`**.

### 13.5 codex §9 — four rounds, terminal PASS

Transcripts under `dev/plans/runs/codex/0.8.20/` (TC-RUBRIC-7 path), committed with this close.

| Round | Transcript | Verdict |
|---|---|---|
| initial | `slice-15b-20260720T195420Z.log` | **CONCERN** — **[P2]** missing interface-contract docs |
| fix-1 re-review | `slice-15b-fix-1-rereview-20260720T200434Z.log` | **CONCERN** — **[P2]** search ignores validity windows |
| fix-2 re-review | `slice-15b-fix-2-rereview-20260720T205344Z.log` | **CONCERN** — **2×[P2]** vector cutoff + clock re-read |
| fix-3 re-review | `slice-15b-fix-3-rereview-20260720T213603Z.log` | **TERMINAL PASS** |

**No verdict was overridden. Every [P2] was fixed.**

### 13.6 What Slice 15b proved WRONG

1. **A named RED test was never written, and the property it guarded then regressed.**
   `dev/design/0.8.20-slice0-erasure-design.md:308` names **three** RED tests for **R-20-NV**;
   **`valid_as_of_binds_now_once` has ZERO hits in `src/`** — it exists **only in that design table**. R-20-NV
   was nevertheless **CLOSED at Slice 10**, and the exact property it named (`:now` binds **once per query**)
   **regressed in fix-2**. Traceability from the Slice-0 acceptance tables to real tests is **UNENFORCED**.
   **Slice 40 should mechanically verify that every test named in those tables exists** — this was found **by
   accident** and is unlikely to be the only one. (**TC-42**)
2. **`ReadView` never covered `search`** — a five-verb **narrowing** of a contract that named `search`.
   (**TC-38**)
3. **`AGENTS.md:25`'s interface-doc obligation is routinely missed**, and `dev/DOC-INDEX.md` did not track
   `dev/interfaces/python.md`, `typescript.md`, `wire.md` or `README.md` **at all**. Rows were added for the
   **first two**; **`wire.md` and `README.md` remain gate-m debt.** (**TC-39**)
4. **Not a defect, but load-bearing for the unstarted work:** the plan's `roles: {filterable, rankable,
   searchable}` **cannot express the ratified C-1 contract** (**TC-40**), and `filterable` has **two
   incompatible backends** (**TC-41**).

### 13.7 Owed to the HITL

Recorded as **§4 #18–#22**.

### 13.8 Closure artifacts

`dev/plans/runs/0.8.20-slice-15b-output.json` and `0.8.20-slice-15b-fix-{1,2,3}-output.json`, plus the four §9
transcripts in §13.5. Ledger entries **TC-38…TC-42**. Committed with this close per **TC-23**.
