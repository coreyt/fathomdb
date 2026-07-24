# Status-board currency enforcement — keep project artifacts true to git at land-time

> **Status:** PROPOSAL (steward-authored 2026-07-24, HITL-directed). Rulings on items 1–3 = **do it**;
> item 4 optional/later; placement of the checks answered below. **First implementation effort scope = items
> 1–3.** Proposed to the next Steward: commission an orchestrator to implement 1–3 as part of the next slice.

## 1. The problem (the incident that motivated this)

After the 0.8.20 Slice-10 and Slice-15 keystone **landed** on `origin/main`, `dev/plans/runs/STATUS-0.8.20.md`
still said "Slice 15 PARTIAL / IN PROGRESS / not landed," last touched four days earlier. A doc a **fresh
Steward session boots from was lying about the repository** — the exact failure the Steward role exists to
prevent ("the master must never silently lie about the repo"). It was only caught because a human asked. The
plan's "immediate next slice" pointer was stale the same way.

## 2. Root cause — an ownership seam, not a discipline lapse

Responsibility for the board is **split**, and the split has a gap:

| Phase | Who holds the pen | Board state they own |
|---|---|---|
| Building a slice | **Orchestrator** (`orchestration.md` §12.5) | in-flight / partial / on-branch-complete |
| Landing a slice | **Steward** (does the merge + master reconcile, §12.4) | **LANDED — nobody's explicit job** |

The orchestrator cannot set **LANDED** — it does not land, and by land-time it has stopped. The Steward lands
and reconciles the master + ledger, but treats the board as "the orchestrator's," so skips it. The board's
**post-land truth falls in the gap between the two roles.** Blaming the actor ("the Steward should have
remembered") is the anti-pattern this repo already rejects (`guardrail-failures-fix-tooling-not-people`): when
something slips a guardrail, **fix the mechanism so it cannot recur for anyone.**

## 3. Design principle

**Artifact updates are part of the atomic unit of work, verified at a chokepoint that already exists — never a
trailing step that depends on someone remembering.** This is the same reasoning as the existing Release DoD
("a land is not done until full-workspace clippy + check are green"), extended to: **"…and until the board and
the next-slice pointer match git."**

## 4. The mechanisms (first effort = 1–3)

### (1) Assign the seam explicitly — contract fix

Amend `.claude/agents/steward.md` and `orchestration.md` §12.5 with one rule:

- the **Orchestrator** owns the board's *in-flight* rows;
- the **Steward** owns the **LANDED** row and the **"immediate next slice"** pointer, updated **in the same
  merge that lands the slice.**

A land is redefined as one atomic checklist: `merge → stamp STATUS row LANDED@<sha> + move the next-slice
pointer → reconcile master §4/§6 → ledger → push`. The board update is *inside* the land, not a follow-up.

### (2) Prevent at land-time — a gate in `scripts/preflight.sh --landing`

`preflight.sh --landing` is already the **mandatory** landing chokepoint (TC-RUBRIC-5; hard-fails in the
primary checkout). Add a board-currency assertion: given the slice/merge being landed, **refuse the land unless
`STATUS-0.8.z.md` marks that slice `LANDED@<sha>` and the plan's next-slice pointer is not the just-landed
slice.** The board then *cannot* stay stale through a land — the land will not complete. Ships with a test that
the check goes red on a deliberately-stale board and green on a current one (RED-first).

### (3) Detect on `main` — CI backstop

A cheap CI job (and/or a Steward watch-pass step) compares git reality to the board: for each slice, is its
merge-commit an ancestor of `origin/main`, and does the board claim the matching state? **Red if they
disagree.** This catches anything that slipped the preflight or predates it — precisely the four-day drift the
incident exhibited. Non-bypassable (runs in the shared pipeline), unlike a local hook.

### (4) OPTIONAL / LATER — machine-derive the LANDED table

Do not hand-maintain the LANDED column at all: generate it from git
(`git merge-base --is-ancestor <slice-branch> origin/main` per slice). Narrative prose stays hand-written; the
**status table is regenerated and cannot drift by construction.** More work and slightly less readable, so it
is a **later** phase — reach for it only if boards keep drifting *despite* (2)+(3). When built, its check is
"regenerate and assert no diff," homed in the same two places as below.

## 5. Where do the checks live? (CI vs preflight vs precommit/prepush)

**Not precommit.** During a build the board *legitimately* reads "in-flight," so a landed-currency check at
commit time false-positives on every slice commit. Precommit may only sanity-check that the board file is
**well-formed**, never that it is **current**.

**Split by prevent-vs-detect:**

| Layer | Home | Role | Why there |
|---|---|---|---|
| **Prevent (land-time)** | `preflight.sh --landing` | block a land that would leave the board stale | already the mandatory, deliberately-invoked land gate; stops the stale board *before* it reaches `main` |
| **Detect (backstop)** | **CI on `main`** | flag any drift that reached the shared branch | non-bypassable; catches what slipped preflight or predates it (this incident) |
| **Shift-left (optional)** | `prepush` hook | early local warning on push-to-main | nice-to-have; **not** a substitute — `--no-verify`-able and depends on each dev's local config |

**Bottom line for item 4's machinery:** preflight regenerates + asserts no diff before a land; CI asserts the
committed board equals the regenerated one. Same two homes. **CI is required as the backstop; preflight is
required as the gate; precommit/prepush cannot carry the currency guarantee alone.**

## 6. Scope & phasing

- **First effort (this proposal): items 1–3.** Contract line + preflight gate (with test) + CI drift job. Small,
  TDD-able, and it closes the exact hole permanently.
- **Later, only if needed: item 4** (machine-derived table).

## 7. Ownership & decision rights

- The **contract change (1)** is a Steward/HITL record change — land it with the tooling.
- The **preflight gate (2)** and **CI job (3)** are cross-cutting tooling; an **orchestrator/implementer** builds
  them TDD (RED: stale board fails the gate; GREEN: current board passes), Steward verifies from git and lands.
- Applies to **every** release board (`STATUS-0.8.z.md`) and the master, not just 0.8.20.

## 8. Implementation plan (for the commissioned orchestrator)

1. **RED** — a test asserting `preflight.sh --landing --slice N --sha <sha>` (or equivalent) fails when
   `STATUS-0.8.z.md` does not mark slice N `LANDED@<sha>`; and a CI-check script that exits non-zero on a
   board/git mismatch. Prove both fail on a crafted stale fixture.
2. **GREEN** — implement the preflight board-currency assertion + the CI check script; wire the CI job on `main`.
   Update `.claude/agents/steward.md` + `orchestration.md` §12.5 with the seam-ownership rule (mechanism (1)).
3. **Verify** — the gate blocks a stale-board land; the CI job reds a stale `main`; both green on a current
   board. Full-workspace clippy + check unaffected (docs/scripts only). codex §9 on the package.

*Guardrail alignment:* this is itself an instance of `guardrail-failures-fix-tooling-not-people` — a repo
mechanism so board-drift cannot recur for anyone, rather than a "be careful" note.
