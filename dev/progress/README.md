# dev/progress

**Historical — frozen at 0.6.x.** Kept for the record only. Current per-release
state lives in `dev/plans/runs/` (boards and run results) and
`dev/plans/release-state-<version>.json`.

Per-release multi-session work logs.

Convention: one file per release (`<version>.md`). Date-stamped entries. Sections per entry: **Done**, **In progress**, **Blocked**, **Decisions**, **Next**.

Purpose: durable state that survives compaction and session boundaries (per `dev/notes/context-research-agentic-best-practices.md` finding F2 — externalize durable state to disk).

Last log: [`0.6.1.md`](0.6.1.md).
