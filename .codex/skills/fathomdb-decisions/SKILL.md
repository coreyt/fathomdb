---
name: fathomdb-decisions
description: Enumerate and close FathomDB HITL decisions. Use for "what decisions are open", release decision triage, or recording an explicit FathomDB HITL ruling.
---

# FathomDB Decisions

Use the repository's existing decisions command as the sole workflow definition.

1. Confirm the working directory is the FathomDB repository and read `AGENTS.md`.
2. Read `.claude/commands/decisions.md` in full and follow it literally.
3. Start with the live release-state JSON decision arrays, then gather only the required supporting evidence and explicit session-only questions.
4. Never re-open a ruled decision or manufacture a decision to pad the report. Do not act on a listed decision while enumerating it.
5. Record an answer only after an explicit HITL ruling, using the command's ledger and single-writer release-state procedure.
