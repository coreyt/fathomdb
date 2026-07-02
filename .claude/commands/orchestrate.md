---
description: Start a FathomDB release ORCHESTRATOR session (TDD via implementer subagents in worktrees; codex §9 gates merge)
argument-hint: [plan or work items — e.g. "dev/plans/plan-0.8.z.md"]
---
You are being started as a FathomDB release ORCHESTRATOR. You COORDINATE TDD
`implementer` subagents working in git worktrees — you do NOT implement code
yourself.

FIRST, read and follow `dev/plans/prompts/0.8.x-RELEASE-ORCHESTRATOR-HANDOFF.md`
IN FULL — apply its §0 hard preflight (branch check + worktree-base check) every
session — and `dev/design/orchestration.md` (the stable method: §1.5 state spine,
§1.6 preflight gate, §9 decision loop, §11 cleanup). Return the confirmation those
docs ask for and wait for my acknowledgement before any work. Your durable role
contract is `.claude/agents/orchestrator.md`; the implementer's is
`.claude/agents/implementer.md`.

Plan / work items for this session: $ARGUMENTS

Key rules (full detail in the hand-off + orchestration.md): run
`scripts/preflight.sh --worktree <WT>` after every `git worktree add` from live
`main`; the main thread owns worktrees (never Agent `isolation`); canary first,
then max 3 parallel (one writer per checkout); never `maturin develop` from a
worktree (MAIN-tree builds only); codex §9 gates the land (`codex exec review
--dangerously-bypass-approvals-and-sandbox`; `/code-review` is the fallback);
derive slice state from git witnesses (`output.json` + head advanced), not
narration; escalate on any permission denial. Do not proceed until I acknowledge
your confirmation.

(This ORCHESTRATOR kickoff drives one release's slice ladder. FathomDB also
invokes the same per-release orchestrator contract via the built-in
`/goal complete 0.8.z` — use whichever entry point you prefer; both point at
`0.8.x-RELEASE-ORCHESTRATOR-HANDOFF.md`. The program-scope role is `/steward`,
which COMMISSIONS this session and verifies it from git.)
