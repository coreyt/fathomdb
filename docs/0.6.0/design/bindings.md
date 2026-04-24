---
title: Bindings Subsystem Design
date: 2026-04-24
target_release: 0.6.0
desc: Cross-language binding strategy (python, typescript, cli) — written first to test distinct role vs. interfaces/
blast_radius: TBD
status: not-started
---

# Bindings — Design

TBD — written first per HITL direction. May fill a role distinct from per-surface
`interfaces/{python,ts,cli}.md` (e.g. shared lifecycle, error propagation, marshalling
strategy, identity invariants across languages). If content fully duplicates the
interface files, fold + delete; otherwise keep as cross-cutting design.

Required (per done-def):
- AC ids owned
- Applicable ADRs
- Interface surface enumerated (fns, types, errors)
- Invariants
- Failure modes + recovery
- No speculative knobs

Critic = `architecture-inspector`.
