---
title: 0.6.0 Dependency Audit — Index
date: 2026-04-24
target_release: 0.6.0
desc: Per-dep audit verdicts (keep|drop|replace) + alternatives
blast_radius: TBD
status: living
---

# Dependency Audit

One file per third-party dep at `deps/<dep-name>.md`. Living folder during 0.6.0
(individual files are lockable).

Per-file content (template — see any existing file as example):
- Current usage: where + why
- Verdict: keep | drop | replace
- If replace: replacement + migration cost
- Alternatives considered (≥1 beyond status quo)
- License + maintenance status

## Verdict summary

| Dep | Ecosystem | Verdict | Replacement | Notes |
|-----|-----------|---------|-------------|-------|

TBD — populate in Phase 1a.i. Critic = `architecture-inspector`.
