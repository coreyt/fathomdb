---
status: ACTIVE
---

# FathomDB — Steward Session Hand-off (2026-07-30-B)

> **Boot:** run **`/steward`**, do its §3 cold-start (**start with `scripts/steward-orient.sh`**), then read
> THIS doc, return a short orientation, and **WAIT for the HITL** before mutating anything.
>
> **Supersedes `STEWARD-SESSION-HANDOFF-2026-07-30-A.md`.** That document is heavily annotated with 🕮
> supersession banners from `seq-207`–`seq-211` and is now **history**. Read it only if you need the
> reasoning behind a superseded item; **this file is the current truth**. ⚠ Do **not** delete `-A`, and do
> **not** delete `STEWARD-SESSION-HANDOFF-2026-07-24-A.md` — the latter is a **LIVE RENDER TARGET**
> (`generated_views` id `handoff-next-step`).

## 0. Why this hand-off exists

The prior session ran long and its context filled with a morning of reconciliation, three retracted claims
and two false alarms. **Everything load-bearing from it is on disk** — `release-state-0.8.20.json`, the two
ledgers, `STATUS-0.8.20.md` §22, and three adversarially-reviewed briefs. Boot clean and read the deltas;
do not try to reconstruct the narrative.

## 1. State — do NOT copy numbers out of here

`scripts/steward-orient.sh` prints the live picture in <4 KB from the single writer. **Run it and trust it
over this section.**

| | at hand-off |
|---|---|
| `main` | **`fc4c9032`**, clean |
| Steward ledger | **`seq-215`** |
| Todos ledger | **`seq-200`** (tip id `TC-135`; 93 open) |
| SCHEMA | **24** |
| `decisions.unruled` | three rows — `publish`, `npm-dist-tag`, and `platform-publish-schedule` (**parked**, ruled at `seq-203`) |
| Manifests | Axis-W **`0.8.9`** everywhere; Axis-E `fathomdb-embedder-api` **`0.6.1`**, version **undecided** |
| CI on `main` | **RED** — gate (i) not met (§5) |

## 2. ▶ IMMEDIATE: Slice 39.5 is IN FLIGHT

**A background orchestrator was commissioned at `seq-215`** against
`dev/plans/runs/0.8.20-slice-39.5-commission-brief.md` (v2), in worktree
`~/projects/fathomdb-worktrees/slice-39-5`, branch `0.8.20-slice-39.5-harness`, base **`fc4c9032`**.

> ### ⚠ YOU CANNOT RESUME THAT AGENT — it belongs to the previous session
>
> **Poll it from git**: branch tip, `git rev-list --count main..0.8.20-slice-39.5-harness`, and worktree
> mtimes. A commissioned orchestrator **returns once and does NOT notify on stall**; one stalled **36 h**
> unnoticed in this program.
>
> **If it is dead or has produced nothing after a long silence, re-commission from the same brief** — it is
> v2, adversarially reviewed, and stands on its own. Do not rewrite it.
>
> ⚠ **A commit-only stall detector FALSE-POSITIVES on this workload.** The units here work in 13-minute
> silent stretches. Key any monitor on **worktree write age** (25 min) **and** `pgrep -x codex|cargo`, not
> on commits alone. The prior session tripped its own detector twice on healthy agents.

**When it lands:** verify from git (never the report), land **fast-forward ref-to-ref** (§7), reconcile the
board + state file **in the same landing action**, write a close record, then Slice 40.

## 3. Slice 40 — ONE unit, structured internally (HITL: do NOT split)

> **HITL ruling 2026-07-30: Slice 40 is NOT to be split.** There have been too many splits already and the
> slice-naming convention is strained (`seq-202`/`seq-204` de-laddered a fractional id precisely because it
> corrupted five tools). **Structure it internally — clear phases, explicit checkpoints — not as 40a/40b/40c
> and not as new ids.**

**Brief:** `dev/plans/runs/0.8.20-slice-40-commission-brief.md` — a **SUBSTANCE DRAFT**, adversarially
reviewed (10 corrections applied). ⛔ **It MUST be regenerated with `scripts/commission-manifest.sh 0.8.20
40` at commission time**: its base SHA comes from a generator that was itself defective until `2008f529`.

### 3a. The internal structure to impose

Give the orchestrator **explicit checkpoints, and require it to report and pause at each** rather than
running eight phases as one undifferentiated push. That is how you get split-like blast-radius control
without split-like naming.

| Checkpoint | Content | Gate before proceeding |
|---|---|---|
| **A — determination** | PHASE 0 (TC-16/F-30) + PHASE 0b (`seq-198` dispatch guard) | the TC-16 determination **stated with evidence**; arm 10 re-pointed into two arms, not deleted |
| **B — versions** | PHASE 1 manifests + **the Axis-E call** | ⛔ **STOP and escalate** — Axis-E's version is undecided and is the HITL's |
| **C — mechanics** | PHASE 2 local dry-run · PHASE 3 parity + the broken smokes | both smokes write no `source_id` and will fail on the real publish |
| **D — gates + ACs** | PHASE 4 workspace gate · PHASE 5 AC-079/AC-080 mint | ⛔ mint **from AC-079 upward** — AC-077 reserved, AC-078 conditionally |
| **E — cut obligations** | PHASE 6 (R-20-H7 free via preflight, TC-42, TC-25, adoption arms, OOS-12) | |
| **F — land + rehearse** | PHASE 7 land, then `-f dry_run=true` **from `main`** | ⛔ **STOPS before any tag** |

**Why F is last:** `verify-release-gates.sh` check 3 requires HEAD reachable from `main`, and
`workflow_dispatch` inputs are read from the **default branch only** — so the new confirmation input is not
dispatchable until after the land. A pre-land dispatch that does not offer it is **not** evidence the guard
is broken.

### 3b. ⛔ The two irreversible publish paths (`seq-196`)

1. **Pushing a `v*` tag.**
2. **`workflow_dispatch` with `dry_run` UNCHECKED** — the else-branch publishes for real with the registry
   token in env, and `publish-pypi` / `post-publish-smoke` are `if: inputs.dry_run != true`, so they run.

⚠ **`dry_run` is `default: true`** — the hazard is a human *unchecking* it. ⛔ **The `|| 'false'` fallback
at `release.yml:20` is CORRECT and must stay** — it governs the tag-push event; flipping it would make a
pushed tag silently never publish. `seq-198`'s confirmation input is a **second factor**, and it **cannot**
be `required: true` (no conditional-required inputs) — declare it optional, enforce it in
`verify-release-gates.sh` with **exit 1**, not a warning.

### 3c. Slice 40's known traps

- ⛔ **`fathomdb/src/lib.rs`'s `DenseReadiness` "PROPOSED / NOT SIGNED" marker is CORRECT — do not touch
  it** (`seq-197`); changing it publishes a false sign-off to docs.rs.
- ⛔ **The allowlist `_comment` re-pin is DONE, not owed** (`seq-208`, `c239908b`). ⚠ The separate
  `InvalidArgument` vs `WriteValidation` claim is **unmeasured** — a determination duty, not a fact.
- ⛔ **TC-16: the determination is Slice 40's to make** (`seq-211`). The surfacing failure is the
  `cargo publish --dry-run -p` literal (**0 occurrences** in `release.yml`), not the tier-name swap.
  **One outcome is forbidden regardless: the resolution may not delete or bypass
  `cargo-publish-if-new.sh`** — that guard is one step from an irreversible three-registry publish.
- **SCHEMA stays 24. No `categories`. No `#[non_exhaustive]`.** `set-version.sh` skips Axis-E and never
  touches `src/ts/package-lock.json`; `src/python/fathomdb/__init__.py` has an ungated `__version__`.

## 4. Open HITL decisions

Read `release-state-0.8.20.json` `decisions.unruled` — **not** board §4, which is a historical queue.

| # | Decision | State |
|---|---|---|
| 1 | **PUBLISH** the breaking pair | **`halts_run: true`**, **PENDING** by HITL answer 2026-07-30. Two gates, §5. |
| 2 | **npm dist-tag** | **PENDING, and explicitly DEFERRED to the publish gate.** Defaults `next`. Decided *with* #1 — do not surface separately. |
| 3 | `platform-publish-schedule` | **RULED `seq-203`.** Parked tracking row; remove when the 0.8.21 plan exists. |

**Unplaced, and owed to the HITL when there is a home:**

- **TC-134** — the "Requirement traceability" work is **per-note triage** (`seq-212`, ruled): strip the
  linkage sentence, **keep any audit finding**, re-head the remainder. Blanket removal is ruled **out**.
  *Placement* still unplaced. ⚠ There are **twelve** occurrences, not the eight the record says.
- **TC-135** — see §6. Needs a ruling.

## 5. Publish gate — amended

**Two independent gates, neither sufficient** (`seq-202`, **as amended by `seq-211`**):
**(i)** every `ci.yml` job **THAT EXECUTED** concludes `success` on the landed commit, **AND (ii)** explicit
HITL approval.

⚠ The "that executed" wording is the `seq-211` amendment: `markdownlint` is
`if: docs_only == 'true'` (`ci.yml:379`) and is **skipped on every non-docs push**, so the original wording
was unmeetable by construction. **A skipped job is a distinct third state** — neither pass nor failure.

**(i) is NOT met.** The last completed runs concluded `failure` with four jobs red: `verify` and `security`
(seven pre-existing pyright errors, both dying in **bootstrap** so their real work has never run),
`commission-manifest`, and `rust-windows` (`tc57_worker_commit_pressure`). **All four are 39.5's to
REPORT, not to fix** (`seq-206`) — disposition is **one HITL decision taken once the full list exists**.

## 6. ⚠ Two live tooling defects, both opened today, both needing rulings

- **TC-135 — `TC-RUBRIC-7` collides with the orphan-marker scan.** Persisting a codex §9 transcript at its
  **required tracked path** reddens `check-release-state-views.sh`, because the scanner walks the tree
  (**including untracked files**) and **cannot tell a generated-region marker that DELIMITS a region from
  one merely QUOTED**. Measured rc=1 on 2 of 3 transcripts. **Landing one would redden `main` permanently.**
  ⚠ **The prior Steward reproduced this within minutes** by quoting the marker in a close record *while
  describing the hazard*. **Consequence: §9 transcripts are currently held OUT of the tree and TC-RUBRIC-7
  is knowingly unmet.** Do not invent a redaction rule and do not weaken the scanner without a ruling.
- **TC-121, third instance** — `seat-path-guard.sh` matched a path string appearing as a **JSON value** in a
  closure payload and blocked the write. Work around it (write to scratch); **never soften the guard**.

## 7. Landing (TC-110)

**Detached-HEAD / ref-to-ref push. ⛔ NEVER `git checkout -B main`** — it corrupts the primary checkout's
index and `preflight --landing` does not catch it.

```text
git fetch origin && git rebase origin/main      # in the worktree
bash scripts/preflight.sh --landing             # FROM THE WORKTREE; hard-fails in the primary by design
git push origin <branch>:main                   # fast-forward
```

⚠ **Prefer a fast-forward.** A `merge(<version>): Slice N` subject requires its short SHA to appear in the
board, which an orchestrator may not edit — so a merge commit turns `main` RED until you reconcile.
⚠ **A clean rebase is not a correct rebase — re-run every gate after.**
⚠ **The harness may DENY the push.** It denied Slice 39's; the HITL pushed it. **If denied: STOP and hand
back with the exact command.** Do not retry or reshape it.
⚠ **You own the board's LANDED row and the next-slice pointer AT LAND TIME** — same action, not a
follow-up.

## 8. Owed edits

- ⚠ **`plan-0.8.20.md:492` says "three latent" and is FALSIFIED** — the real answer is **eight bypasses
  over six state fields, plus a ninth**. Reported by `SLICE-ID-HARDENING`, not edited by it. **Yours.**
- **`STATUS-0.8.20.md` §22** is `SLICE-ID-HARDENING`'s close record — read it before commissioning Slice 40;
  it carries the `[DETERMINE]` outcomes and the two live defects that unit fixed.

## 9. What actually bit, this session — read before trusting a green

1. **⛔ Never quote a generated-region marker verbatim** in any document (TC-135, §6). It reddens a gate.
2. **`steward-orient.sh` has a 4096-byte budget and hard-fails over it.** The prior Steward blew it
   **twice** by writing a rich "Immediate next action" cell. Keep that cell tight; run the script after
   editing it.
3. **⚠ When two records disagree on a COUNT, reconcile the counting CONVENTIONS before declaring either
   wrong.** The prior Steward "corrected" a design doc that was right: `agent-test.sh` has 35 `run_capped`
   call sites and 34 labels, but **31 are unconditional** and 4 sit in `if`/`else` guards — so the doc's 31
   and 23 were both exact. Retracted at `seq-209`.
4. **⚠ Identical committer timestamps across every commit on a branch mean a REBASE, not a loop.** And
   **worktree file mtimes reflect worktree CREATION**, not agent activity. The prior Steward raised both as
   evidence an agent was looping and contaminated; **both were false alarms** and had to be retracted in
   front of the HITL.
5. **The adversarial-review-per-brief control is now 13-for-13** against Steward drafts — it caught a
   fabricated site count, a counterfactual inherited claim, a non-vacuity requirement that was
   *unsatisfiable* (which would have shipped vacuous test arms), an instruction pointing at an irreversible
   publish path, and a measurement that **could not fail**. **Run it on every brief. It is the only control
   on this class until TC-131 exists at 0.8.21.**
6. **Every count in the record about the fractional-id defects was wrong and LOW.** Do not trust a count
   you have not measured — including the ones in this document.
7. **`agent-test.sh`'s aggregate exit is VACUOUS (TC-16)** — until 39.5 lands, run suites individually and
   quote each `rc`. **Capture `rc=$?` on the very next line**, before any pipe.
8. **`dev/plans/runs/**` is excluded from markdownlint (TC-130)** — lint a copy elsewhere.
9. **Never `git add -A`** (TC-132). **`pgrep -x`, never `-f`** (TC-83).
10. **⚠ A STALE NATIVE MODULE MAKES `test-python` LOOK CATASTROPHICALLY RED WHEN IT IS MERELY UNRUNNABLE
    (`TC-136`).** Read-only, it gives **40 collection errors**, every one the same
    `ImportError: cannot import name 'IllegalTransitionError' from 'fathomdb._fathomdb'` — the compiled
    `.so` predates the 0.8.19 lifecycle symbols. **None is a test failure.** Ruled out as TC-97:
    `-o pythonpath=` from a neutral cwd gives the identical 40. `agent-test.sh:231-236`'s
    `FATHOMDB_TESTS_ALLOW_REBUILD` dance exists because for the Python suite the rebuild is a
    **prerequisite, not a convenience**. **A red list therefore needs FOUR states:** PASSED · FAILED ·
    SKIPPED-prerequisite-absent · **UNKNOWN-requires-the-sanctioned-rebuild**. ⛔ Do not resolve it by
    rebuilding blind: in a worktree that rebinds the shared venv (the stale-base trap). **Report it; do not
    file 40 phantom failures.**
11. **Steward-supplied primary-checkout results, 2026-07-30** (hand these to 39.5 rather than re-running):
    `test-ledgerwatch` **rc=0** (96 passed) · `test-ts` **rc=0** (~177 s, slow — budget for it) ·
    `test-check-ledgers` **rc=0** · `test-python` **UNKNOWN** per item 10.
12. **⚠ A guard can block LEGITIMATE PROVISIONING.** `seat-path-guard.sh` denied the 39.5 orchestrator a
    `src/ts/node_modules` symlink purely because the path starts with `src/` — but that directory is
    **gitignored with zero tracked files**, a dependency dir, not source. **The Steward provisions it**
    (worktree setup is the Steward's job). ⛔ **`.venv` is the exception and must NEVER be symlinked into a
    worktree** — it would satisfy `agent-test.sh:231`'s ownership check and fire a `maturin develop` that
    rebinds the shared venv.
13. **✅ An implementer REFUSED a permission-laundering request and was RIGHT.** The 39.5 orchestrator, blocked
    by its own seat guard, asked an implementer to perform the write for it; the implementer refused,
    correctly, without needing to judge whether the action was benign. **A blocked action escalates to the
    Steward — it never routes sideways.** Reinforce this; the guardrail worked.

## 10. Standing rules

- **Trust git, not narration.** Verify every "closed / landed / green" against the diff and real exit codes.
- **You COMMISSION and VERIFY; you do not implement.** ⚠ The prior Steward hand-wrote a guard hook, a
  snapshot script, **a test source** and a `.gitignore` edit instead of commissioning them — recorded as a
  boundary breach at **`seq-213`**. `orchestration.md` §1.2 reserves `src/**`, `engine/**` and **test
  sources** for the implementer seat. That breach then blocked an orchestrator's closure write (TC-121).
- **The mandate rule.** Direction and record changes — a release slot, moving an item between releases,
  re-sequencing — are **always** explicit HITL, never inside an implied mandate.
- **You cannot launder authority downward.** A message to an orchestrator is peer-level.
- **Escalate a pin trip; never clear it.** Widening the allowlist or re-pinning to pass a gate is forbidden.
- **Push scope is fathomdb-only.** Never push memex without a specific per-push directive each time.
- **Two-tier numbering.** `x.y.z` real · `x.y.z.p` pico label-only · **`13` forbidden** · publish is a
  separate explicit HITL gate. ⚠ **No more fractional slice ids** — `seq-202`/`seq-204`, and the HITL has
  now ruled Slice 40 must **not** be split.
- **Verify the branch before EVERY commit or push.** **Never open a ledger by hand** — `ledgerwrite` to
  append, `ledgerwatch` to read; stage the `.seq` sidecar with the `.jsonl` (TC-88).
- **Only the Steward edits `release-state-0.8.20.json`, the master, `STATUS-0.8.20.md` and the ledgers.**
- **Delegate; don't hand-do.** Spend Steward context on judgement and on verifying from git.
- **Surface your own errors in the open.** The prior session recorded five of its own, including two false
  alarms raised to the HITL and one boundary breach. **That transparency is the standard, not an anomaly.**
