# Commission brief — `GENVIEW`: end the reconciliation tax (evaluation unit)

> **DRAFT / EVALUATION UNIT.** This is **not** a 0.8.20 ladder slice, not a cross-cutting unit of any
> release, and **it does not land**. It builds on a branch, **measures whether it achieves the goal**, and
> **STOPS**. Placement and landing are separate, explicit HITL decisions taken after the measurement is in.
>
> **Written by the Steward 2026-07-30 at HITL request.** Everything marked **[V]** was measured at
> `494696b8`.

## 0. The problem, in one measured paragraph

**Today on `main`: 2,333 lines of documentation changed against 41 lines of scripts [V]** (`git diff --stat
91db34d8..494696b8`). Most of that was not new thinking — it was **reconciling the same facts across
documents that had drifted apart**. Every steward session opens with hours of it, and drift is the default
outcome rather than the exceptional one.

**The mechanism, measured on a real failure.** The previous session's commit `1f85ca2a`, subject *"Slice 39
reconciled — LANDED 91db34d8"*, changed **exactly one line** of `STATUS-0.8.20.md`, and that line was
**inside the `<!-- BEGIN GENERATED -->` markers [V]**. It updated the single writer, ran the regenerator,
watched the generated cell update, and `check-board-currency.sh` passed. Meanwhile **six hand-written
assertions stayed false** for hours:

| # | Site @ `46b9365e` | Said | Truth |
|---|---|---|---|
| 1 | `STATUS-0.8.20.md:76` | Slice 39 `NOT_STARTED — the immediate next slice` | LANDED `91db34d8` |
| 2 | `:46` | most recent land = Slice 33; *"Next: Slice 39"* | Slice 39 |
| 3 | `:37` | `Ladder remaining: 39 → 40` | `40` |
| 4 | `:79` | `Ladder remaining: 39 → 40` | `40` |
| 5 | `:47` | `decisions.unruled` holds **TWO** | three rows / two live |
| 6 | `:50` | *"Commission Slice 40 — the FINAL slice"* | `SLICE-ID-HARDENING` is next |

**That session did the mechanically correct thing and still shipped a false board**, because the correct
thing only covered the ~5 regions the generator owns. **The defect is coverage, not discipline.** No amount
of care fixes it; more care is what was already applied.

> ### The thesis this unit must prove or disprove
>
> **A fact that lives in the state file and is RENDERED cannot drift. A fact that is restated by hand
> will.** So: move every drift-prone fact under the single writer, and make the remaining hand-written
> prose mechanically detectable when it contradicts the state file.

## 1. What exists already — extend it, do not reinvent it

`scripts/check-release-state-views.sh` + `dev/plans/release-state-<version>.json` already implement exactly
this pattern, built and codex-reviewed under DOC-HYGIENE-2. **Five** generated blocks exist today **[V]**,
registered in the state file's `generated_views` array with an `id`, `file`, `location` and `renders`:

`master-ladder-progress` · `status-unblocks` · `status-live-open-count` · `handoff-next-step` ·
`plan-immediate-next`

The renderer hard-fails on drift or hand-editing, and `--write` regenerates. **The machinery is sound and
proven. This unit widens its coverage; it does not replace it.**

⚠ **`check-release-state-views.sh` is also being edited right now by the in-flight `SLICE-ID-HARDENING`
unit** (fractional-id fixes to `_slice_str`/`_by_slice` and their call sites). Your base is that unit's
branch tip, not `main` — see §7.

## 2. Scope — the four blocks that would have prevented all six failures

Add generated blocks covering exactly the facts that drifted. Each must be registered in `generated_views`
with the same `id`/`file`/`location`/`renders` shape as the existing five.

1. **`status-ladder-status`** — the **Status column of the §2 ladder table**, per slice: `COMPLETE — LANDED
   <sha>` / `not started` / the in-flight marker, derived from `landed` + `ladder[].sha` + `next_slice`.
   *(Kills sites 1.)* ⚠ The prose *describing* each slice's deliverables stays hand-written — only the
   **status cell** is rendered. Do not swallow the substance column; it is genuine editorial content and
   the state file has no business owning it.
2. **`status-in-flight`** — the §1 *"Slice in flight"* cell's factual half: most recent land + its SHA, and
   the forward sequence from `ladder_order`. *(Kills site 2.)*
3. **`status-ladder-remaining`** — **every** `Ladder remaining:` claim in the board. There were **two** and
   both were stale; a renderer that covers one is worse than useless. *(Kills sites 3 and 4.)*
4. **`status-decision-summary`** — the unruled/live-open sentence, rendered from `decisions.unruled` with
   its ruled-but-parked rows distinguished. *(Kills site 5.)*

**Site 6 (`:50`, "Immediate next action") is DELIBERATELY NOT fully generated.** That cell is mostly
judgment — which unit to commission and why — and generating it would either produce something useless or
smuggle editorial decisions into a state file. **Instead it is covered by §3's detector.** Say so
explicitly in your closure; a reader must not think it was overlooked.

## 3. The other half — a drift detector for prose that stays hand-written

Generation cannot cover everything, so the residue needs detection. Extend the guard so that **prose
contradicting the state file is caught**, not merely prose missing a SHA.

`SLICE-ID-HARDENING` has already built the first instance of this (TC-133: for every id in `landed`, the
board must not describe it with a not-started/in-flight/next marker, and `Ladder remaining:` prose must
agree with `remaining_ladder`). **Build on it; do not duplicate it.**

**[DETERMINE]** What else is mechanically checkable in hand-written prose without becoming a
false-positive generator: a stale `next_slice` mention, a slice described as "the final slice" when
`ladder_order` says otherwise, a decision described as open that sits in `decisions.ruled`. **State plainly
what your detector can and cannot catch** — an over-claimed detector is worse than none, because the next
reader will trust it.

## 4. ⛔ MEASUREMENT — this is the deliverable, not the code

The HITL commissioned this to find out **whether the change achieves the goal**. A unit that ships four
renderers and asserts success has failed. **Four measurements, and M1 is the one that matters.**

### M1 — HISTORICAL REPLAY against known ground truth (the headline)

We have something rare: **a known-answer control.** The board at `46b9365e` contained the six drift sites
in §0, all six silently passing every gate.

> **Acceptance:** reconstruct the repo state at `46b9365e`, apply your mechanism, and show that **all six
> sites are caught** — each either (a) **rendered** from the state file and therefore correct by
> construction, or (b) **flagged RED** by your detector. **Zero of the six may silently pass.** Report the
> disposition of each of the six individually, by line number, with the command and its `rc`.

This is non-vacuous *by construction*: the answer is known, was found by hand, and cost a morning. ⚠ Build
the replay as a **fixture** (copy the `46b9365e` documents and state file into a scratch dir under `/tmp`)
— do **not** check the primary out to an old commit, which you are sealed against anyway.

### M2 — DRIFT SURFACE (static, before/after)

Build `scripts/measure-drift-surface.sh`: for each fact the state file owns (`landed` + SHAs, `next_slice`,
`remaining_ladder`, `schema_version`, `decisions.unruled` ids and count), count **hand-written restatements
outside generated markers** across `STATUS-0.8.20.md`, `plan-0.8.20.md`, the master and the live hand-off.

Report the count **at `46b9365e`, at your base, and after your change**. The number should fall sharply for
covered facts.

> ⚠ **This metric is the easiest thing here to fake, in both directions.** A regex that matches nothing
> reports a beautiful zero. **Validate it against the ground truth first: run it on the `46b9365e` fixture
> and confirm it finds the six known sites.** A drift-surface scanner that cannot find six *known* drifts
> is measuring nothing, and its "improvement" is noise. State that validation before you state any number.

### M3 — RECONCILIATION COST (the thing the HITL actually feels)

The cost being paid is *hand-edits per land*. **Simulate a land** on a fixture: mark the next slice landed
in the fixture state file, run `check-release-state-views.sh --write`, then run every currency gate.

> **Acceptance: ZERO hand-edits to prose are required to make every document true.** Today the same
> operation took **six** hand-edits to the board alone, plus three more sites found later. Report the
> number you achieve; if it is not zero, report exactly which fact still needs a human and why.

### M4 — NON-VACUITY, per block, both directions

For **each** new generated block: (a) mutate the fixture state file → the rendered output **must change**
accordingly; (b) corrupt a rendered block by hand → the bare checker **must exit non-zero**. Quote both
`rc`s per block. A block that passes both ways in both directions is not wired to anything.

### M5 — NO REGRESSION

The five existing blocks render byte-identically for unchanged state; `check-release-state-views.sh` bare
**rc=0**; `check-board-currency.sh` **rc=0**; all four `SLICE-ID-HARDENING` suites **rc=0**; full-workspace
`cargo clippy --workspace --all-targets` **and** `cargo check --workspace --all-targets` both **exit 0**.

## 5. ⛔ THE SEAL — you may not touch the primary checkout

You run as `sealed-orchestrator`. `.claude/hooks/sealed-worktree-guard.sh` denies any `Edit`/`Write`/`Bash`
call naming the primary checkout — **reads included**, because your worktree is a full checkout and you
never need it.

**The guard is prevention; the proof is detection.** The Steward fingerprints the primary with
`scripts/snapshot-tree.sh` **before and after** your run and compares. **One byte of difference fails this
unit outright**, regardless of what the guard did or did not intercept, and regardless of how good the
renderers are.

- **Never `git add -A`** (TC-132). Stage explicit paths.
- **`git init`, `git worktree add|remove`, `--git-dir`, `GIT_DIR=` are all DENIED** (TC-128 — an unscrubbed
  `GIT_DIR` re-initialised the primary twice on 2026-07-29 and set `core.bare=true` on it). Build fixtures
  under `/tmp`.
- **If the guard denies you: STOP and hand back with the exact command.** Do not rephrase or route around
  it. ⛔ **Editing or weakening the guard is forbidden and invalidates the run.**

## 6. Guardrails

- ⛔ **Do not edit** `dev/plans/release-state-0.8.20.json`'s *facts*, `STATUS-0.8.20.md`'s hand-written
  substance, the master, or **any ledger**, except as required to convert a region into a generated block.
  **You are changing the MECHANISM, not the RECORD.** If your work implies a fact is wrong, **report it**;
  the Steward reconciles.
- ⛔ **Never hand-edit inside a `GENERATED` marker.** ⚠ One render target is a region inside
  `STEWARD-SESSION-HANDOFF-2026-07-24-A.md` (`generated_views` id `handoff-next-step`) — that old hand-off
  is a **LIVE RENDER TARGET**; do not delete or restructure it.
- **No source changes.** No `src/**`, no `engine/**`, no `.github/**` (Slice 40's exclusive territory).
  SCHEMA stays **24**. Governed surface byte-identical.
- **`dev/plans/runs/**` is excluded from markdownlint** (TC-130) — a green lint proves nothing for the
  board. Lint a copy outside that directory.
- **Capture `rc=$?` on the very next line**, before any pipe or command substitution.
- **`pgrep -x`, never `pgrep -f`** (TC-83).
- Do **not** run `scripts/agent-test.sh` as an aggregate (vacuous, TC-16), `cargo test --workspace`
  (unstable, TC-72), or anything eu7 (closed by decision).

## 7. Base, branch, worktree — and a moving base

- Worktree **`/home/coreyt/projects/fathomdb-worktrees/genview`**, branch **`genview-single-writer-eval`**,
  both pre-created and verified by the Steward. The path is baked into your seat's guard arguments.
- ⚠ **Your base is the `SLICE-ID-HARDENING` branch tip, not `main`** — that unit is editing
  `check-release-state-views.sh` right now and you would collide with it. **That base is therefore
  MOVING**: the unit is in codex fix rounds and has already been rebased once. Expect to rebase, and
  **re-run every gate afterwards — a clean rebase is not a correct rebase** (DOC-HYGIENE-3 rebased with
  zero conflicts and then failed six gates).

## 8. Definition of done

1. The four blocks of §2 implemented and registered in `generated_views`.
2. The §3 detector extended, with an explicit statement of what it cannot catch.
3. **M1 replay reported site-by-site for all six**, each with command and `rc`.
4. **M2 scanner validated against the ground truth before any number is quoted**, then baseline/after.
5. **M3 simulated land**, with the hand-edit count stated plainly.
6. **M4 per-block non-vacuity, both directions, both `rc`s quoted.**
7. **M5 no regression**, every gate quoted.
8. codex §9 terminal-clean (`codex exec review --dangerously-bypass-approvals-and-sandbox`; `/code-review`
   is the fallback). Never triage by grepping `[P1]`/`[P2]` markers (TC-87).
9. Closure `output.json` in `dev/plans/runs/`, **whose substance is the measurement table, not a narrative
   of what you built.**
10. ⛔ **STOP. Do not land, do not push to `main`, do not open a PR.**

> ### The honest-answer clause
>
> **If the measurement shows this does not achieve the goal, say so plainly and say why.** A negative
> result, clearly evidenced, is a complete and valuable deliverable — it saves the program from adopting a
> mechanism that does not pay. Do **not** tune the metric until it looks good; if M2 improves only because
> the scanner is narrow, **that is the finding**, and it is more useful than a flattering number.

## 9. ⛔ STOP conditions

Any guard denial · any HITL-gated decision · any implied change to a release-state **fact** · any pin trip ·
a board render diff you did not intend · **landing anything** · round 6 of the fix cap (3 rounds per
finding, Steward check-in at 6, HITL halt beyond 10).

## 10. ⚠ ANTI-STALL

You return **once** and do **not** notify on stall. One orchestrator in this program stalled **36 hours**
unnoticed. Emit a progress line after each bounded step naming the step and your branch's last commit SHA;
never wait silently on a long command without saying what you are waiting for; if blocked, **say so and
return** rather than idling. The Steward polls your branch tip and worktree mtimes from git.
