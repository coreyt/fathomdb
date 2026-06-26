---
name: c1-graph-arm-seeding-live
description: "C1 graph-arm seeding LANDED (commit 5da42b0) — graph arm seeds from entity/edge-fact FTS, now LIVE producing source-resolved candidates; recall-vs-BM25 measurement still pending (needs full-graph extraction)"
metadata: 
  node_type: memory
  type: project
  originSessionId: 54df08ae-c01b-4eae-8862-514eb2cfd198
---

C1 (the BLOCK-1 recall fix) landed on local main `5da42b0` (2026-06-15, unpushed).
`bfs_graph_arm_candidates` now seeds the BFS frontier from the query's own matched FTS
surfaces — **source A** edge-fact FTS (`search_index_edges` endpoints) + **source B**
entity-node FTS (`search_index` ⋈ `canonical_nodes`, `logical_id IS NOT NULL`) — instead of
doc-node hits (logical_id=NULL → empty frontier, the root cause of zero graph candidates).
Resolved seeds are EMITTED as depth-0 candidates carrying provenance source_id (edge's for
source A, node's for source B), not just BFS roots (codex §9 [P2] fix). Built on G0 Phase-2
part-1 (`017ad68`: meter + SearchHit.source_id carry + FFI parity).

**Proven LIVE on real extracted data** (`/tmp/r2-lme-s-v2.sqlite`, full 19,195-session
s_cleaned haystack + partial ~1408-edge graph): `search(q, use_graph_arm=True)` returns 49–50
`graph_arm` hits/query with real session source_ids (e.g. `sharegpt_yywfIrx_0`) — up from ZERO.

**Why:** HITL goal "make the graph arm beat BM25". C1 is the lever; it's done + LIVE.
**How to apply / NEXT:** the clean recall@K-vs-BM25 number still needs a FULL-graph extraction
(Tier-B ~$0.4 flash-lite via `elps_live_harness.py`) on a known gold haystack — the existing
DB is only ~7% graph coverage (measuring on it would understate the graph arm = misleading).
Then **PRE-3** (step-16 provenance schema, SCHEMA-GATE signed [[g0-phase2-schema-gate-signed]])
to complete G0 P2 for GA. Graph arm stays G2-blocked (use_graph_arm not flipped by default).
Related: [[fathomdb-recall-fidelity-vs-relevance]] (relevance ceiling), [[fathomdb-080-plan-approved]].
