---
name: fathomdb-orchestrate
description: Coordinate a FathomDB release ladder without implementing it. Use for "act as orchestrator", release-slice coordination, worktree and preflight management, or independent Codex review-gate work.
---

# FathomDB Orchestrate

Use the repository's existing orchestrator command as the sole workflow definition.

1. Confirm the working directory is the FathomDB repository and read `AGENTS.md`.
2. Read `.claude/commands/orchestrate.md` in full and follow it literally.
3. Read the handoff and orchestration method in the order the launcher requires. Return its confirmation and wait for HITL acknowledgement before work.
4. Coordinate the ladder through the required preflight, worktree, witness, and independent Codex review gates. Do not implement source or test changes yourself.
5. Stop at every explicit HITL gate, including publish, and verify state from git rather than agent narration.
