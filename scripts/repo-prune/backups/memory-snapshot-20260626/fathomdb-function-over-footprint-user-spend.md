---
name: fathomdb-function-over-footprint-user-spend
description: "FathomDB stance shift (HITL 2026-06-23) — function can outweigh the small/local/CPU-only goal; FathomDB may use local/cloud/frontier LLMs as USER-controlled opt-in \"spend\" knobs."
metadata: 
  node_type: memory
  type: project
  originSessionId: 1a66de90-c67e-434a-a0e5-9ae699d3289c
---

**HITL direction (2026-06-23):** FathomDB is in a very competitive space, and **function must be
weighed strongly — sometimes over the original "small, local, CPU-only" goal.** Local computers
routinely have access to LLMs (cloud, frontier, AND local). The design principle is to **let the USER
decide when to "spend"** (compute / $ / latency) to unlock function — FathomDB exposes the knobs;
the user picks the operating point for their machine/environment.

**Why:** this re-frames the "footprint invariant" stance that several memories + the 0.8.4 handoff §5
treat as binding (CPU-only / no-API / 1-bit at the query boundary). That invariant is no longer an
absolute gate — it's *one* operating point. LLM-heavy, GPU, or API-using features are **on the table**
as opt-in, as long as the user controls the spend. Concretely raised by the HITL for the GraphRAG gap:
FathomDB may (a) use a local LLM AND match GraphRAG's exact LLM/generation budget for fair comparison,
(b) accept a configurable top-K parameter (top-8 was self-inflicted), (c) run a rate-adaptive
"graphrag" ingest queue that builds graph/community structure at a rate appropriate to the host.

**How to apply:** stop treating "offline-build LLM / GPU / API" as a footprint violation that kills a
feature. Instead design it as a user-controlled spend tier (default cheap/local; opt-in to
frontier/cloud). Keep a cheap/local default path working, but don't refuse function on footprint
grounds alone. Supersedes the "binding footprint" framing in [[0.8.4-slice0-preregistered-awaiting-hitl]]
and the CPU-only absolutism in the handoff. Pairs with [[characterize-underperformance-then-hitl]].
