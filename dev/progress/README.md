# dev/progress

Per-release multi-session work logs.

Convention: one file per release (`<version>.md`). Date-stamped entries. Sections per entry: **Done**, **In progress**, **Blocked**, **Decisions**, **Next**.

Purpose: durable state that survives compaction and session boundaries (per `dev/tmp/context-research-agentic-best-practices.md` finding F2 — externalize durable state to disk).

Active log: [`0.6.0.md`](0.6.0.md).
