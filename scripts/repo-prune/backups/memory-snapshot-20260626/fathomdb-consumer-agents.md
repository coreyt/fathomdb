---
name: fathomdb-consumer-agents
description: "FathomDB's target consumers (Memex, Hermes, OpenClaw) are real public local-first agent-memory projects, not internal — their stacks define the requirement surface."
metadata: 
  node_type: memory
  type: project
  originSessionId: 482125fb-352f-45de-a97b-7961c5482408
---

FathomDB's intended consumers — **Memex, Hermes Agent, OpenClaw Agent** — are
real, public, shipping local-first AI-agent memory systems, NOT internal projects
(HITL corrected me on 2026-05-31 after I assumed they were internal because only
`~/projects/memex` exists locally).

- **OpenClaw**: markdown files as source-of-truth + **per-agent SQLite +
  sqlite-vec + BM25 hybrid + MMR rerank + exponential temporal decay**; tools
  `memory_search` (hybrid semantic) and `memory_get` (by-id/file read). No
  graph/entity model. Nearly identical substrate to FathomDB but adds by-id read,
  rerank, and decay that FathomDB lacks.
- **Hermes Agent** (Nous Research, Feb 2026, open source): **SQLite FTS5** session
  search + LLM summarization for cross-session recall + Honcho dialectic user
  modeling (longitudinal) + procedural "skills" memory. Local-first, no telemetry.
- **Mem0** (memory layer used by OpenClaw etc.): vector store + extraction layer
  (new facts stored / outdated updated / dupes merged); tool surface is literally
  search / store / **get (by-id)** / **list** / forget (delete) — exactly the
  read verbs FathomDB doesn't expose. Has a graph-memory variant too.

**Why:** These real stacks validate the four-pillar requirement (store / retrieve
/ world-model / reasoning) and show the missing-verb gaps (by-id, list, rerank,
decay, entity-graph, consolidation) are table-stakes in shipping peers — not
speculative. **How to apply:** treat the [[agent-memory-fit]] gap ladder (G0–G8 in
dev/design/agent-memory-fit.md) as consumer-driven, not hypothetical; the deep-
research run wf_4c04fd5a-530 covers the broader literature. Relates to
[[pr2a-go-recompute-split]] (0.7.2 line).
