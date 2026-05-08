# Plans

`dev/plans/` is the home for **0.6.0 implementation planning and execution
artifacts**.

Canonical files:

- `0.6.0-implementation.md` — single source of truth for the 0.6.0
  implementation sequence, current execution posture, and phase/pack status
- `../progress/0.6.0.md` — chronological work log for the 0.6.0 rewrite

Supporting 0.6.0 plan artifacts:

- `0.6.0-Phase-9-Pack-*.md` — packet- or pack-specific plans
- `prompts/` — execution prompts used for packet work
- `runs/` — packet logs, outputs, reviewer notes, and status boards

Directory split:

- `dev/plans/` = 0.6.0 execution planning
- `dev/roadmap/` = 0.7.0+ future-release planning

Rules:

- if a document is the active 0.6.0 implementation source of truth, keep it in
  this directory
- if a document is an intermediate packet plan, prompt, or run artifact for
  0.6.0, keep it in this directory
- if a document is a future-release backlog or deliberate deferral beyond 0.6.0,
  move it to `dev/roadmap/`
