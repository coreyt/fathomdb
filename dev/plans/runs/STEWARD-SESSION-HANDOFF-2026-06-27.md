# Steward session hand-off — 2026-06-27 (live state)

> **You are the FathomDB 0.8.x Program Steward. Read `dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md` first** —
> that is your role, decision-rights, the mandate rule, transparency rules, the resident-subagent cost
> model, and your process. **This note is only the live state you are inheriting**; it does not restate
> the role. Keep it tight — verify everything below from git before acting.

## 0. Orient before acting (cold-start)

- **Trust git, not this note.** First thing, every session: `git rev-parse --abbrev-ref HEAD` — the shared
  single checkout is frequently left on a **feature branch** while the env reports "main"; **never assume
  main.** Then `git fetch origin`, `git log --oneline -15 origin/main`, `git worktree list`. **Land all
  your commits on `main` via a clean worktree cut from `origin/main`** (do not commit in the shared
  checkout's current branch — that is exactly how the contamination below happened).
- Schedule of record: `dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md`. Boards: `dev/plans/runs/STATUS-0.8.*.md`.

## 1. The work in front of you — four live tracks

**A — 0.8.8 EXP-OBS ratification (IN FLIGHT; commissioned to the 0.8.8 release orchestrator).**
The orchestrator is applying the HITL-ratified Explanation field-set resolution on a clean worktree off
main: cherry-pick the two stranded 0.8.8 commits (`65f9b3a8` Slice-0 ADR, `cc3a91a8` Slice-5 engine) →
apply the additive ratification (`#[non_exhaustive]`×4, ADR §A.3 **PROPOSED→RATIFIED** + §A.4 Q1–Q3
closed, `rust.md:86`→29 with `len()==29`, R-OBS-1 golden + R-OBS-2-COV, **Axis-W minor** bump) → codex §9
+ HITL → push to main. Spec: `dev/plans/runs/0.8.8-explanation-fieldset-ratification.md`.
**Your action:** when HITL signals "0.8.8 on main," **verify from git** (`65f9b3a8`+`cc3a91a8` are ancestors
of main; the `#[non_exhaustive]` + ADR + tests applied; gated) → then run **B**.

**B — 0.8.9 branch de-contamination (BLOCKED on A).** PR #93 (branch `0.8.9-ci-integrity-micro`) carries 3
non-0.8.9 commits: `cc3a91a8` + `65f9b3a8` (0.8.8 — land via A) and `77634256` (a Steward-handoff dup whose
content is already on main as `8156c769`). **Your action:** once A is on main, **rebase
`0.8.9-ci-integrity-micro` onto main** → all three drop → PR #93 reduces to its two real commits
(`d5a68d17` ci, `4880f0d3` board). Force-push touches the 0.8.9 session's branch (paused per its note,
`dev/plans/runs/NOTE-0.8.9-to-steward-2026-06-27.md`) — **coordinate + HITL OK before the force-push.**

**C — bootstrap CI failure (DIAGNOSED; folded into 0.8.9 — no separate steward landing).** Root cause:
two unguarded `httpx` imports (`eval/graph_arm_recall.py:36`, `eval/p0a_batch_e2e.py:287`; not in `[dev]`
extras) fail `pyright` in a clean CI venv, **masked** by `>/dev/null` + `--quiet` in `scripts/bootstrap.sh`
→ `verify`/`security` abort before any gate runs, on **all branches incl. main**. Fix = **A** (`# type:
ignore[import-not-found]` on the 2 imports) + **C** (drop the masking). **Folded into the 0.8.9 release**
as reserved-gap **Slice 1** in `plan-0.8.9.md` (a hard prerequisite). The 0.8.9 orchestrator implements it
on PR #93's branch when it resumes (post-B); main goes green when #93 merges. **You do not land it
separately.**

**#5 — record the decision (PENDING; do at the A/B reconciliation point, per orchestration.md §12.4).**
Record into the master §6 / the boards: the field-set ratification (amend-ADR-to-code) and that 0.8.8 now
carries the governed-surface **len 26→29** expansion (AC-074; HITL-blessed public sub-types). Point the
master / DOC-INDEX at the ratification artifact.

## 2. Additional next-work (after #93 merges + 0.8.8 lands)

- **Resume the 0.8.x program per the master:** 0.8.8 remaining slices (10 bindings, 15/20 telemetry/gold —
  contracts are in the ratification artifact), then the even line (0.8.10+) and the odd planner-router
  line; **EXP-S@0.8.12 is the long pole; EXP-FT folded at 0.8.15** (master §6 F-5).
- **`exp/subagent-persistence` branch reconciliation (deferred):** it carries program commits not on main
  (a 0.8.8 Slice-0 ADR variant, 0.8.9 CI work). Reconcile when convenient.

## 3. Minimal pertinent history (only what you need)

- **The contamination episode (why the guards exist).** The shared single checkout (no per-session
  worktrees) was on `0.8.9-ci-integrity-micro` while the env reported "main" → 0.8.8 + Steward commits
  leaked onto the 0.8.9 branch. **Guards landed (`245095e8`):** branch-check before every commit +
  worktree-base check, in **both** `0.8.x-STEWARD-HANDOFF.md` and `0.8.x-RELEASE-ORCHESTRATOR-HANDOFF.md`.
  Honor them.
- **pyo3 0.24→0.29** (0.8.8 Slice 1 security bump) is **on main** (`8c938bb7`) — Dependabot Need #1 resolved.
- **Field-set ratification:** a 6-owner negotiation (`fathom-resolve-explanation-field-set`) ratified the
  *landed* Explanation field set as architecturally correct; HITL approved amend-ADR-to-code + public
  sub-types. The field content is unchanged; the only engine delta is `#[non_exhaustive]`.
- **Role/file identity:** program-scope steward = `0.8.x-STEWARD-HANDOFF.md`; per-release orchestrator
  (`/goal complete 0.8.z`) = `0.8.x-RELEASE-ORCHESTRATOR-HANDOFF.md` (renamed from the old
  `PROGRAM-STEWARD-HANDOFF.md`; its body is **stale 0.8.3/0.8.4** — only its §0 preflight and §5/§6
  mechanics are current).

## 4. Open HITL waits

- 0.8.8 sign-off + the **"0.8.8 on main"** signal → triggers your **B**.
- **Coordinate + HITL OK before the 0.8.9 force-push** (B).
- Bootstrap Fix A+C is approved and assigned to the 0.8.9 lane (Slice 1) — no further decision needed.

## 5. Boundaries (full rules in the role prompt — this is the reminder)

Commission release orchestrators; never hand-drive a `plan-0.8.z` ladder or write slice code. Mutations
(commit/push, PRs, settings) are HITL-gated unless under a standing mandate; **direction/record changes
are always explicit HITL.** Verify your branch before every commit. Delegate mechanical/bulky work to
subagents (warmth→overlap→size); keep your context for decisions. Trust git, not narration.
