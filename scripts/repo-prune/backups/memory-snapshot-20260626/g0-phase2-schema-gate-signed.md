---
name: g0-phase2-schema-gate-signed
description: "HITL signed the G0 Phase-2 PRE-3 SCHEMA-GATE (step-16, SCHEMA_VERSION 15→16 provenance) on 2026-06-15; full G0 Phase-2 then C1 authorized"
metadata: 
  node_type: memory
  type: project
  originSessionId: 54df08ae-c01b-4eae-8862-514eb2cfd198
---

HITL (coreyt) on 2026-06-15 chose "full G0 Phase-2 (incl. PRE-3) → then C1" and thereby
SIGNED the PRE-3 SCHEMA-GATE: step-16 `SCHEMA_VERSION 15→16` adding nullable
`extractor_provenance` (nodes+edges) + `extractor_model_id` (nodes; edges already have it),
with the `-- MIGRATION-ACCRETION-EXEMPTION:` marker for the accretion guard. Design:
`dev/plans/runs/0.8.1-g0-phase2-design.md` (§F). 

**Why:** G0 Phase-2 is the unbuilt prerequisite for C1 — C1's seed phase returns G0's
`GraphFrontierStats` tuple, reuses the `_graph_frontier_stats_for_test` seam, and relies on
Phase-2's traversed-edge `source_id` carry. G0 Phase-2 was design-ready but NOT implemented
on main or the stale `g0-…` branch (which had only G0 *Phase-1* tracer).

**How to apply:** Build G0 Phase-2 TDD-first per design §G order (1 byte-stable → 10
ready-provenance), then C1 (`0.8.1-c1-seeding-slice-design.md`). Neither flips
`use_graph_arm` (stays G2-blocked). Does NOT need extract.v1/protocol/golden change.
Related: [[g0-identity-scope-logical-id-alone]], [[fathomdb-080-plan-approved]].
