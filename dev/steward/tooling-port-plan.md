# Steward / orchestration tooling port — plan

> Recorded per the HITL-approved port directive. This is the **plan of record** for
> the memex→FathomDB Steward/orchestration **tooling** port. Principle:
> **converge with FathomDB's existing prose; do not fork a parallel system; zero
> memex domain content.** The tooling is generic (ledger CLIs, agent roles, thin
> command launchers); every command points at FathomDB's OWN kickoff docs, and
> every agent def encodes FathomDB's OWN rules.

## Workspace

- Sole writer: worktree `fathomdb-worktrees/tooling-steward-port`, branch
  `tooling/steward-orchestration-port`, cut off `origin/main` `20f53ffb`.
- `~/projects/memex` is the source, **read-only** — never written/committed/pushed.
- Commit on the branch; **do NOT push or merge** (merge to `main` is a separate
  Steward + HITL gate).

## Principle (non-negotiable)

- Converge with FathomDB's existing prose, don't create a parallel system.
- Zero memex domain content — no `src/memex`, no memex `dev/steward/*` text, no
  OPP/LEVERAGE vocabulary, no memex paths. Verified at close by grep over the new
  files.

## Tier 1 — Ledger CLIs (verbatim)

1. Confirm memex `dev/agent-tools/ledgerwrite/` + `ledgerwatch/` are
   domain-agnostic (grep the `.py` — expect nothing). Copy each dir **verbatim**
   (`.py` + `README.md` + `test_*.py`; skip `__pycache__`) into FathomDB
   `dev/agent-tools/`.
2. Run their tests in-repo → must be GREEN. Record the result.
3. Add `dev/steward/README.md` (the discipline: append via `ledgerwrite`, read
   deltas via `ledgerwatch`, keeps Steward context O(delta)); seed
   `dev/steward/steward-ledger.jsonl` with one bootstrap entry via `ledgerwrite`.

## Tier 2 — Roles as tooling (converged)

1. `.gitignore` convergence (HITL-approved): un-ignore the SHARED `.claude/`
   tooling (`agents/`, `commands/`) while per-user/local files
   (`settings.local.json`, etc.) stay ignored. Capture the existing local
   `implementer.md` into the worktree so all three agent defs are tracked.
2. Author `.claude/agents/steward.md` + `.claude/agents/orchestrator.md`.
   Tools: Read, Bash, Grep, Glob, Agent, Task — **NO Edit/Write** (the guard).
   Distil the rules from FathomDB's `dev/design/orchestration.md` +
   `dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md` +
   `0.8.x-RELEASE-ORCHESTRATOR-HANDOFF.md`. Model the SHAPE on memex's agent
   defs, rewrite the content for FathomDB.
3. Author `.claude/commands/steward.md`, `.claude/commands/orchestrate.md`,
   `.claude/commands/orch.md` as **thin launchers** pointing at FathomDB's
   EXISTING kickoff docs. `/orch` = alias to `/orchestrate`.
4. Scripts: inspect FathomDB `scripts/`. Add `preflight.sh` /
   `agent-permission-canary.sh` / `codex-review.sh` **only where not already
   covered**; prefer wiring `/orchestrate` to the EXISTING scripts. Do not
   duplicate; if covered, note and skip.

## Tier 3 — skip/defer (converge, don't duplicate)

- Skip `orchestrator-guard.sh` (wake `guard-check` PreToolUse hook already covers
  on-`main` edits — verify wired in `.claude/settings.json`).
- Skip session hooks/settings (already present).
- Defer `archive_ledger_items.py`.

## DoD + closeout

- Ledger CLI tests GREEN in-repo.
- `/steward`, `/orchestrate`, `/orch` files exist and reference REAL FathomDB
  docs (no dangling paths).
- Agent defs encode FathomDB rules; the memex-domain grep returns nothing.
- `.gitignore` change verified (tooling tracked, `settings.local.json` ignored).
- `dev/steward/tooling-port-reconciliation.md`: a table mapping each new
  command/agent-def → the FathomDB doc/rule it converges with (proves no parallel
  system).
- Independent **codex §9** review of the full diff; findings folded into fix-N.
- Constraints: label-only (no manifest/tag/publish) · fathomdb-only (no memex
  writes) · don't rewrite history · commit on the branch, **do NOT push/merge**.
- Report back to the PDS for verify-from-git + the merge-to-HITL gate.
