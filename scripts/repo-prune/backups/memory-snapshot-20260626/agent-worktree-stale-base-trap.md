---
name: agent-worktree-stale-base-trap
description: "Agent tool isolation:\"worktree\" cut worktrees from a 206-commit-stale base (not live main), losing two slices + breaking the shared venv binding; pre-create + verify from $(git rev-parse main) instead."
metadata: 
  node_type: memory
  type: feedback
  originSessionId: b7d95508-8f3d-42a9-9865-720392751700
---

In the FathomDB session, spawning an `implementer` with the Agent tool's
`isolation: "worktree"` created the worktree from a **stale fixed base**
(`900919f`, ~206 commits behind `main`) — NOT live `main` HEAD. This bit **twice**
(the P0-A scaffold and the G0 Phase-1 agent): the agent did ~11 min of work, then
discovered its tree lacked `src/python/eval/`, the graph arm, and SCHEMA_VERSION 15.
Worse, to run it maturin-developed `fathomdb` from the stale worktree, which
**repointed the shared `.venv` `fathomdb.pth` at the agent's worktree and broke the
canonical binding** for everyone.

**Why:** `isolation:"worktree"`'s base is whatever fixed ref the harness picked at
session start, not current HEAD. The Slice-25 agent, which self-created its worktree
from `BASE=$(git rev-parse main)` in its prompt, got the correct base — so the
mechanism, not the idea, was the problem.

**How to apply:**
- **Do NOT rely on `isolation:"worktree"` for current-main work.** Either (a)
  orchestrator **pre-creates** the worktree: `git worktree add "$WT" -b <branch>
  "$(git rev-parse main)"`, or (b) the prompt has the agent create it from
  `$(git rev-parse main)` (Slice-25 pattern).
- **Verify before spawning:** `HEAD == main`, key files present (here:
  `src/python/eval/*`, `bfs_graph_arm_candidates`, schema version).
- **Give the agent a fail-fast STEP-0 preflight** (same checks) → STOP, don't
  rebase/merge, if stale. A stale baseline is an orchestrator problem.
- **Forbid `maturin develop` / `pip install -e` from a worktree** unless you intend
  to repoint the shared `.venv`; after a legit rebuild, re-point the binding back to
  canonical. The binding is a shared mutable resource.

Extends [[orchestration-execution-traps]]. Also: an agent that self-pauses on a
"waiter" expecting auto-resume may be **terminated** instead — verify via
process/commits, don't trust the resume claim (see the P0-A stall).
