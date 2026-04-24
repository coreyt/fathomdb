---
title: 0.6.0 Test Plan
date: 2026-04-24
target_release: 0.6.0
desc: AC id → test id → layer mapping; specialized subagent writes scaffolds
blast_radius: TBD
status: not-started
---

# Test Plan

TBD — draft in Phase 3f.

Layers: `unit | integration | soak | perf`.

Mapping table:

| AC id | Test id | Layer | Owning crate | Fixtures | Scaffold path |
|-------|---------|-------|--------------|----------|---------------|

Rules:
- Every AC id has ≥1 test id.
- No test id without AC back-reference.
- Perf/soak tests use absolute gates (values from acceptance.md).
- Test **scaffolds must exist** before lock (specialized subagent writes; >1 iteration expected).
