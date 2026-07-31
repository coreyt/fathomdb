# Steward cold-start budget — RATIFIED

> **Status: RATIFIED by the HITL 2026-07-31** (steward ledger `seq-226`). All six steps approved.
> ✅ **Steps 1–3 are DONE** (`e27f4939` + this commit). Steps 4–6 are approved and sequenced, not started.
>
> **Ratified order.** Deviating from it needs a new ruling, because two of the constraints are hard:
>
> | # | Step | State |
> |---|---|---|
> | 1 | Push `5d135bee` · `b842d417` · `88be45a3` | ✅ **DONE** |
> | 2 | `DOC-INDEX.md` row + board pointer to the Slice 40 provenance doc | ✅ **DONE** |
> | 3 | **Phase 1** — §3 liveness-aware; ceiling **180,000**, not 60,000 | ✅ **LANDED** — 374,473 → 153,188 tok |
> | 4 | Commission **Slice 40** — **not gated on anything**; its item B6 *is* TC-139, so the slice clears the metric's blocker as its own work | approved |
> | 5 | **Phase 2** — **during** Slice 40's Windows-clock window, not before it | approved |
> | 6 | **Phase 3** + tighten ceiling to 60,000 — **only after 0.8.20 publishes** | approved |
>
> **Why 5 is placed, not merely deferred.** Phase 2 is not technically blocked by Slice 40 — it touches
> `steward-orient.sh`, not `context-clarity.sh`, and no board. But landing it on `main` mid-flight forces
> the slice branch to rebase, which restarts the **N=5 consecutive-green `rust-windows` accrual** (a
> 60–75 min serial floor), and trap 13 requires re-running every gate after a rebase. §4.9 of the Slice 40
> brief says that clock accrues while checkpoints A–E proceed and not to idle on it. Phase 2 fits that
> window and cannot touch the slice branch.
>
> **Why 6 is a hard dependency.** Phase 3 splits `STATUS-0.8.20.md` — the live board of the release in
> flight. Five `generated_views` anchor into it, `check-board-currency.sh` requires Slice 40's merge SHA
> **literally inside** it, and `preflight.sh --landing` §7 enforces the same. Restructuring it during its
> own release risks a red `main` at the worst moment.
>
> ⚠ **Consequence accepted at ratification: Phases 1+2 reach ~125,000, not 60,000.** The live board
> (48,713) and `plan-0.8.20.md` (22,389) dominate the remainder and neither is touchable mid-release.
> **60,000 is a post-publish number**, and §3's stated target should be read that way.

**Problem.** `/steward` → `dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md` §3. Read as written, that list is
**~366,800 tokens across 45 files**. Read charitably (item 3's parenthetical says "*the* board",
singular) it is still **~238,200**. Roughly **216,000 of it is closed releases** — `STATUS-0.8.0.md`
alone is 48,824 tokens for a release closed months ago, larger than the live board.

**Target.** Cold start ≤ **60,000 tokens** with no loss of decision-relevant fact, pinned by a test so it
cannot drift back. Stretch: 40,000.

⚠ **60,000 is the POST-PUBLISH end state, not the first ceiling.** The ratified ceiling at Phase 1 is
**180,000** — post-Phase-1 measures ~153,200, and a gate that is red the day it lands gets switched off.
That is exactly how repo-prune's own metrics ended up unratcheted (§1a Finding 3). The ceiling tightens
as each phase lands; it does not start at the destination.

---

## 1. What the measurement found

| | ~tokens | note |
|---|---|---|
| `steward-orient.sh` output | **955** | already excellent; covers items 2–3, front-loads 7 |
| item 1 — master | 42,891 | **60 % is the §6 findings ledger** (F-1…F-34) |
| item 3 — `STATUS-0.8.*.md` glob | 178,264 | 19 files; **18 are CLOSED** |
| item 4 — `plan-0.8.*.md` glob | 115,525 | 20 files; **17 are `status: COMPLETE`** |
| items 5–7 | ~29,000 | orchestration, orchestrator handoff, MEMORY, last report |

Two further facts decide the shape of this plan:

1. **The liveness split already exists, machine-readably, on both axes.**
   - Boards: `scripts/lib/board-closed.sh` exposes `board_is_closed()` — the canonical predicate, already
     shared by `check-board-currency.sh` and `steward-orient.sh`, already tested. It classifies 18
     CLOSED / 1 LIVE today.
   - Ladders: every `plan-0.8.z.md` carries `status:` frontmatter (`COMPLETE` / `ACTIVE` / `PROPOSED`),
     already linted by `lint-plans-status.sh` and `test_plans_status_frontmatter.sh`.

   So the largest win needs **no new predicate, no file moves, and no rewriting** — only a §3 that
   consults predicates the repo already trusts.

2. **The live documents are themselves mostly historical.**
   - `STATUS-0.8.20.md` — **67 %** is §6 (whose own heading reads *"CLOSED BY DECISION; this whole
     section is HISTORICAL"*) plus **nine** `## N. Slice N close` records. Live orientation is ~15,958
     of its 48,713 tokens.
   - The master — **60 %** is the §6 integration-findings ledger. Live sequencing is ~17,000 of 42,891.

---

## 1a. Prior art — `scripts/repo-prune/` (probed 2026-07-31)

A doc/memory prune ran **2026-06-26** with real results: `dev/` −53 % bytes, live `.md` tokens **−51 %**,
`runs/` **670→168** files. (repo-prune's `README.md` says 660; its own `baseline.json` and
`DELTA-2026-06-26.md` both say 670 — **the README is wrong**, and that defect should be fixed there.)
It left durable outputs still in place today: `dev/archive/` (50 files),
`dev/DOC-INDEX.md`, `dev/experiments-ledger.md`. **This plan should extend it, not duplicate it** — see
§5 for the verdict. Three probe findings decide how.

**Finding 1 — it measures a different cold start, and the two sets are disjoint.**
`context-clarity.sh` already emits a `cold_start_orient_set` metric. Its `orient_list()` is
`DOC-INDEX + dev READMEs + root contracts + dev/interfaces`. That is the *understand-the-codebase* set.
The steward §3 set is boards, ladders, the master and the hand-offs.

| set | files | overlap |
|---|---|---|
| `repo-prune` `orient_list()` | 45 | **0** |
| steward §3 reading list | 43 | **0** |

**Finding 2 — its own instrument shows the tree-wide prune did not help this axis.**

| metric | baseline `25541d88` | post `bb64a2d4` (per `post.json`) |
|---|---|---|
| `dev_tree.md_tokens_est` | 3,499,334 | 1,883,557 (**−46 %**) |
| `cold_start_orient_set.tokens_est` | 85,187 | 86,736 (**+1.8 %**) |

⚠ **Two repo-prune artifacts disagree about the post state and neither is obviously authoritative.**
`post.json` records sha `bb64a2d4` / 1,883,557 tokens; `DELTA-2026-06-26.md`'s header records sha
`fe2734e9` / 1,895,292. The numbers above are `post.json`'s. The **direction and magnitude are
unaffected** (−46 % either way), and the `cold_start_orient_set` rise is present in both. Reconciling
the two artifacts is a repo-prune housekeeping item, not a blocker for this plan.

Halving the tree left the orient set slightly *worse*. A tree-wide prune does not fix a reading-list
problem, because the reading list is a **named path through the tree**, not a size property of it.

**Finding 3 — there is no ratchet, and the zone it cleaned has more than fully re-drifted.**

| `dev/plans/runs/` | post-prune 2026-06-26 | today |
|---|---|---|
| all files | 168 | **359** |
| `.md` files | 88 | **187** |

(`find dev/plans/runs -maxdepth 1 -type f | wc -l`, 2026-07-31. Recursive is 485; quote the method
with the number — an unqualified count here is not reproducible.)

Nothing enforces the gains: no test and no CI job consults `context-clarity.sh`'s metrics. It is a
**campaign instrument** — run by hand, before and after a prune — not a standing gate. Five weeks
un-ratcheted returned the zone to worse than double its post-prune size.

**Why it could not have solved this anyway:** every predicate this plan leans on postdates the prune by
a month — `lint-plans-status.sh` 2026-07-24; `board-closed.sh`, `steward-orient.sh` and
`release-state-0.8.20.json` all 2026-07-25. The liveness split did not exist on 2026-06-26.

---

## 2. The plan — four phases, each independently shippable

### Phase 0 — pin the budget before changing content

⇒ **Extend `scripts/repo-prune/bin/context-clarity.sh` with a second metric,
`steward_cold_start_set`.** Do **not** add a parallel `steward-coldstart-cost.sh`: the harness, the
`ceil(bytes/4)` convention, the baseline/post/DELTA layout and the JSON contract already exist and are
already correct. A second tool would be a second index — the thing repo-prune's own design note (S3)
forbids.

The new metric expands §3's list as an agent would, and reports per-item and total cost alongside the
existing `cold_start_orient_set`. Then add the piece repo-prune never had: a **ratchet** —
`scripts/tests/test_steward_coldstart_cost.sh`, exiting non-zero above the ceiling, wired into CI at
Phase 4. That is the one genuinely new contribution to the tooling, and Finding 3 is the argument for
it.

⚠ **The metric must expand the list from §3 itself, not from a hardcoded copy.** A hardcoded list would
pass while §3 grew — a vacuous gate, the exact trap `lint-plan-anchors.sh` was written for. Note that
the existing `orient_list()` *is* hardcoded, which is consistent with a campaign tool but not with a
gate.

**Cost:** small. **Risk:** none — additive to a tested script.

### Phase 1 — make §3 liveness-aware

One edit to §3. No content moved, no files renamed, no archive.

- **Item 3** — `dev/plans/runs/STATUS-0.8.*.md` → *"the LIVE board, per `board_is_closed()`. Closed
  boards are the frozen record; open one only when a specific close record is in question."*
- **Item 4** — *"the `plan-0.8.z.md` ladders that exist"* → *"the ladders with `status: ACTIVE` or
  `PROPOSED`. `COMPLETE` ladders are historical."*

**Saves ~199,820 tokens** — 82 % of the total problem, for one section edit. Both predicates already
exist and are already under test. Fully reversible.

**Cost:** trivial. **Risk:** near-zero.

### Phase 2 — let `steward-orient.sh` discharge item 1

`orient.sh` already proves the model: 955 tokens beats 178k because it derives from
`dev/plans/release-state-<version>.json`, the single writer.

The lever is that **master §4 allocation is already a generated view** — `release-state-0.8.20.json`
declares `generated_views[0] = {"id": "master-ladder-progress", "file": ".../PROGRAM-SEQUENCING.md",
"location": "§4 Release allocation …"}`. The allocation the steward is told to "confirm" is already
machine-derived from the same source `orient.sh` reads.

So: extend `orient.sh` to emit the §4 allocation block, and demote item 1 from *"read the master"* to
*"§2a edges and §5 by-when on demand; §4 comes from orient."*

**Saves ~42,000.** Requires updating `scripts/tests/test_steward_orient.sh`.

**Cost:** medium. **Risk:** low — additive to a script with an existing test and a hard-fail-on-empty
contract.

### Phase 3 — split the live documents

⇒ **Run this as a `repo-prune` pass, under its existing contract**, not as a bespoke edit. Specifically
adopt:

- **R3 two-phase HITL gate** — Phase 1 emits a classification map and **stops** for sign-off; Phase 2
  executes only after approval. The map goes in `scripts/repo-prune/runs/` beside
  `doc-prune-CLEANUP-MAP.md`.
- **R1 verdicts** — CURRENT / REFERENCE / ARCHIVE / DELETE, one per section moved.
- **S1–S5 safety invariants**, and in particular **S3: no second index**, and **S4: rewrite inbound
  links in the same change**.
- ⭐ **The archive-mode rule, which already solves this plan's largest coupling risk.** repo-prune's
  design note: *"for trees cross-referenced by path (e.g. `dev/plans/prompts/`, ~120 refs) archive
  **in place** and record stale status in the existing index — never a second index."* The board, the
  master and `plan-0.8.20.md` are exactly such a tree (§4 risk 4). **Archive in place** — split the
  file, leave both halves where they are, record status in `DOC-INDEX.md` — rather than relocating into
  `dev/archive/`.
- **Distill-before-delete**, and its recorded learnings: never `git add -A` over staged deletions; scan
  inbound refs before moving anything; one regex is not enough.

Apply the same content pattern used on the Slice 40 brief (`5d135bee`): live instructions in one file,
historical record in a sibling, **split not deleted**.

| from | live remainder | to |
|---|---|---|
| `STATUS-0.8.20.md` §6 + the 9 `Slice N close` sections | ~15,958 | `STATUS-0.8.20-close-records.md` |
| master §6 findings ledger (F-1…F-34) | ~17,000 | `PROGRAM-FINDINGS.md` |

Also rename the master: **`0.8.6-0.8.16-PROGRAM-SEQUENCING.md` discusses 0.8.17 through 0.8.25.** An
agent told to read it meets a filename that misdescribes its scope before reading a word.

**Saves ~59,000.** This is the phase that needs HITL sign-off — it moves records, and the mandate rule
makes record changes yours.

⛔ **Split, do not delete.** The retained-negation prose is partly a deliberate audit trail: the
`seq-207` correction on the live board exists *because* a false claim was previously acted on. Deleting
it would destroy the evidence that a guardrail fired. The brief split preserved every retracted claim
for exactly this reason.

**Cost:** large. **Risk:** highest — see §3.

### Phase 4 — stop new drift at the source

- Close records land in `STATUS-<v>-close-records.md` **from the start** — an orchestrator-contract
  change, so §11-style sections never accrete on a live board again.
- Wire the Phase-0 ratchet (`scripts/tests/test_steward_coldstart_cost.sh`, reading
  `steward_cold_start_set.over_ceiling`) into CI so the ceiling is enforced, not aspirational.
  ⛔ **No `steward-coldstart-cost.sh`** — the metric lives in `context-clarity.sh` (Phase 0); a second
  script would be the second index S3 forbids.

**Cost:** small once Phase 3 lands. **Risk:** low.

---

## 3. Budget trajectory

| stage | ~cold-start tokens |
|---|---|
| today, literal | 366,800 |
| today, charitable | 238,200 |
| after Phase 1 | **~167,000** |
| after Phase 2 | ~125,000 |
| after Phase 3 | **~55,000** |
| ceiling at Phase 1 (ratified) | **180,000** |
| ceiling after Phase 3 | 60,000 |

Reaching the 40,000 stretch needs `plan-0.8.20.md` (22,389, 31 superseded/stale markers) split the same
way. Proposed as optional Phase 3b, not load-bearing.

---

## 4. Risks and unknowns — Phase 3 only

Phases 0–2 carry essentially no risk. Phase 3 touches four checked couplings, and they must move
atomically with the content:

1. ⛔ **Five `generated_views` in `release-state-0.8.20.json` carry `location` strings pointing at
   §-anchors** — two into `STATUS-0.8.20.md`, one into master §4, one into `plan-0.8.20.md`, one into a
   dated steward hand-off. Splitting either file invalidates its anchor. `check-release-state-views.sh
   --write` is the tool; the JSON is the single writer and must be updated in the same commit.
2. ⚠ **`check-board-currency.sh`** requires every `merge(<version>): Slice <N>` short SHA to appear
   **literally inside the STATUS board**. If close records move out, that check must follow them or it
   goes vacuous — and a vacuous board-currency check is how a stale board would stop being detectable.
3. ⚠ **`lint-plan-anchors.sh`** enforces the line-anchor ban (DOC-HYGIENE-2). A split tempts
   reintroducing `file:line` pointers between the two halves. Use greppable headings.
4. ⚠ **12+ scripts reference `STATUS-0.8.*` / `plan-0.8.*` paths** (`preflight.sh`,
   `commission-manifest.sh`, `check-c1-conformance.sh`, `lint-plans-status.sh`, `corpus-freeze.yml`,
   and the test suites). **This is why Phase 1 filters rather than archives** — filtering touches no
   path, and captures 82 % of the win with none of this exposure.

**Unknown, worth one cheap probe before Phase 3:** whether `scripts/repo-prune/` (which carries
`prune-docs.md`, a `doc-prune-CLEANUP-MAP.md`, and an acceptance-test file) already encodes a
doc-archival policy this plan should extend rather than duplicate.

---

## 5. Verdict on merging with `repo-prune`

**MERGE — clearly, and in one direction: this work becomes a second axis inside `repo-prune`, not a
sibling project.** Going it alone would rebuild five things that already exist and are proven
(measurement harness, before/after layout, classification verdicts, safety invariants, two-phase HITL
gate) and would violate repo-prune's own S3 by creating a second index of the same documents.

The split of contributions is clean, which is what makes the merge worth doing rather than merely
tidy:

| `repo-prune` already has | this plan adds |
|---|---|
| measurement harness + baseline/post/DELTA layout | the **steward reading-list axis** — zero overlap with its existing orient set (Finding 1) |
| R1 verdicts · R3 two-phase HITL gate · S1–S5 | the **liveness predicates** — `board_is_closed()`, `status:` frontmatter, `release-state` — none of which existed when it ran (a month later) |
| archive-in-place rule for path-referenced trees | a **ratchet**: a test + CI gate, so gains survive (Finding 3: `runs/` went 168 → 359 in five weeks, un-gated) |
| distill-before-delete + recorded learnings | |

**The strongest argument for merging is that repo-prune's own instrument makes this plan's case.** It
recorded a 46 % tree-wide token cut alongside a **+1.8 %** move on its cold-start metric. That is not a
failure of the prune — it is evidence that *tree size and reading-list cost are different variables*,
measured by the same tool, and that only one of them was ever targeted.

**One thing not to inherit:** `orient_list()` is a hardcoded file list. Acceptable for a campaign
instrument; not acceptable for a gate. The new metric must derive its list from §3.

---

## 6. Recommended sequencing

**Phase 0 + Phase 1 together, now.** One new script, one new test, one §3 edit — and cold start drops
below 200k immediately, with a gate that keeps it there. Neither moves a record, so neither needs a
mandate.

**Phase 2 next**, as ordinary tooling work.

**Phase 3 only on an explicit HITL ruling**, because it is a record change, and ideally after the
`repo-prune` probe above.
