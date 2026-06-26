---
name: eu5f-hitl-signoff
description: HITL signed off on the EU-5f engine-fix slice (writer/projection-path changes) on 2026-05-30
metadata: 
  node_type: memory
  type: project
  originSessionId: 4eca2edc-b621-412e-88a3-c3d0fbb9d25c
---

HITL (coreyt) signed off on the **EU-5f** engine-fix slice on 2026-05-30. EU-5f
modifies the writer/projection path that the EU-7 launch prompt told the
implementer to preserve, so it required explicit HITL approval beyond EU-7's
scope. EU-5f = three engine fixes surfaced while building the EU-7 harness:
(A) projection-worker `catch_unwind` fault isolation, (B) production
mean-centering pin in `commit_projection_outcomes` (commit-gate-serialized) +
open-time recovery pin, (C) `CandleBgeEmbedder` 512-token truncation.

**Why:** the locked "mean-centering ON" feature was inert on the production
`engine.write` path (only the `write_vector_for_test` seam pinned), and the real
corpus could not seed past ~512 docs (long-doc embed errors). Without EU-5f,
EU-7 could not validly measure the locked config.

**How to apply:** EU-5f is approved to land; remaining gates are codex review
(EU-5f + EU-7) and then EU-8. Do NOT push to origin without a separate explicit
OK. The `AC013B_RECALL_FLOOR` re-pin stays 0.7.2 PR-2 (EU-7 measured recall
~0.828 < 0.90, surfaced honestly; anchor in
`dev/plans/runs/0.7.1-EU-7-output.json`).
