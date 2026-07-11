---
title: Dedicated-checkout-per-orchestration guard — proposal
status: ADOPTED (HITL 2026-07-11) — TC-RUBRIC-5 authorized; wiring folds into the 0.8.20 X0. Proposed by Steward 2026-07-10.
decider: HITL
owner: Program Steward
refs:
  - steward-ledger seq 74 (the incident), seq 76/77
  - rubric-eval v3 pilot on 0.8.19 — findings A6 / TC-RUBRIC-5
  - dev/plans/prompts/0.8.x-RELEASE-ORCHESTRATOR-HANDOFF.md §0
  - dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md §5
  - scripts/preflight.sh
---

# Dedicated-checkout-per-orchestration guard — proposal

> **PROPOSED — not yet adopted.** This is the Steward's diff-ready fix for the shared-checkout hazard the
> HITL asked to have proposed after 0.8.19. It specifies an **enforceable, witness-first guard** (not a
> "be careful" note), per `[[guardrail-failures-fix-tooling-not-people]]`. Applying it (the `preflight.sh`
> function + the handoff wiring) is a **process/record change → HITL-ratified**; this doc is the proposal.

## 1. The hazard (concrete, not hypothetical)

During the **0.8.19 Slice-5/Slice-15 landing** (steward-ledger **seq 74**), a **second `claude` session** (the
rubric-eval experiment) was using the **same working checkout** `/home/coreyt/projects/fathomdb` and **switched
its branch** (`main → rubric-eval-v3-terminal`) **mid-landing**. The orchestrator's cherry-pick then landed on
the **wrong branch**. It was recovered clean — `main` was never corrupted or wrong-pushed — but a load-bearing
invariant ("branch verified before every commit/push") was **violated in the moment**.

The rubric-eval v3 pilot independently flagged this as **A6** (high) and tracks the durable fix as
**TC-RUBRIC-5**.

## 2. Why discipline alone is insufficient

The orchestrator handoff §0 already mandates `git rev-parse --abbrev-ref HEAD` **before every commit**. That
discipline is necessary but **not sufficient against a concurrent writer**: the check is **TOCTOU-racy** — the
orchestrator can verify `main`, and a *different session* can switch the shared working tree's branch in the
window before the commit/cherry-pick executes. A single working tree has **one HEAD and one index**; two
agents sharing it can always thrash each other. The only robust fix is **isolation**, enforced — not a tighter
human/agent check.

## 3. The fix — dedicated linked worktree per orchestration/landing, enforced

**Principle:** release orchestration and any release/landing **git-write** run in a **dedicated linked git
worktree** (own HEAD + index), **never the shared primary checkout**. Read/analysis in the primary checkout
stays fine; only orchestration/landing **writes** are gated.

### 3a. Enforceable guard (witness-first, path-free)

Add a mode to `scripts/preflight.sh` — e.g. `--landing` (alias `--orchestration`) — that **HARD-fails when the
cwd is the primary (non-linked) checkout**. Git distinguishes them natively, so **no hardcoded path** is needed:

```sh
# a linked worktree has --git-dir != --git-common-dir; the primary checkout has them equal
GIT_DIR="$(git rev-parse --absolute-git-dir)"
COMMON="$(git rev-parse --path-format=absolute --git-common-dir)"
if [ "$GIT_DIR" = "$COMMON" ]; then
  hard "landing/orchestration must run in a DEDICATED linked worktree, not the shared primary checkout.
        Cut one:  git fetch origin && git worktree add <dir> origin/main   then re-run from <dir>."
fi
```

This is robust across machines/paths and against renames, and matches `preflight.sh`'s existing
witness-first / one-line-JSON / non-zero-on-HARD-fail contract. **Mechanism verified 2026-07-10:** in a linked
worktree `--git-dir` = `.git/worktrees/<name>` ≠ `--git-common-dir` = `.git` (guard passes); in the primary
checkout the two are equal (guard HARD-fails) — exactly the discrimination we want.

### 3b. Wiring (the record changes that need HITL ratification)

1. **Orchestrator handoff §0** — add a step 0: *"Before ANY orchestration/landing git-write, run
   `scripts/preflight.sh --landing`; it MUST print `preflight: pass`. If it fails, cut a dedicated worktree
   (`git worktree add <dir> origin/main`) and re-run from there."*
2. **Steward handoff §5** (commit cadence / "verify the branch") — add: *"Release/landing/program commits run
   from a dedicated worktree on `main`, gated by `preflight.sh --landing`; never the shared primary checkout."*
   (Dogfooded this session: `fathomdb-worktrees/steward-main`.)
3. **`dev/design/orchestration.md`** (§11 worktrees) — record the guard as the standing rule.

### 3c. Low-friction helper (optional, recommended)

A one-liner wrapper `dev/agent-tools/new-landing-worktree.sh <name>` that does
`git fetch origin && git worktree add "$ROOT-worktrees/<name>" origin/main` and prints the cd path — so the
"cut a dedicated worktree" step is a single command, keeping the guard cheap to satisfy.

### 3d. Belt (optional, lower priority)

A `pre-commit`/`pre-push` hook that **WARNs** (not blocks — avoids false positives on legit primary-checkout
docs edits) when committing to `main` or a `*-slice-*` branch **from the primary checkout**. The HARD gate is
the preflight `--landing` check invoked by the orchestrator/steward; the hook is a passive backstop.

## 4. Scope / non-goals

- **Does NOT forbid** a second session or the primary checkout for **read/analysis** — only orchestration/landing
  **writes** must be isolated.
- **Does NOT prevent** two sessions from running — it prevents them **colliding on one working tree**.
- **No product/engine impact** — pure orchestration hygiene (tooling + docs). Footprint-invariant-neutral.

## 5. Rollout

Small tooling change = **TC-RUBRIC-5**: one `preflight.sh` function + the §3b handoff/orchestration.md wiring
(+ optional 3c/3d). Land as a **label-only pico** or fold into the **0.8.20 X0 setup** (the next orchestration,
which should run under this guard from the start). Evidence it already works in practice: this session, the
rubric-eval agent **self-isolated to its own worktree** and the Steward's release/landing/docs commits ran from
dedicated worktrees (`0.8.19-landing`, then `steward-main`) — zero collisions after isolation.

## 6. Recommendation

**Adopt.** It converts a discipline-only invariant that already failed once into a physical, witness-first
guard, at near-zero friction (one command to cut the worktree; one preflight flag). Ratify §3b + authorize the
small tooling slice (TC-RUBRIC-5); I will commission it (or fold it into the 0.8.20 X0).
