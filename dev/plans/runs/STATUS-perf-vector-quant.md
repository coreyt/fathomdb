# STATUS — 0.7.0 PERF-VECTOR-QUANT

_Last updated: 2026-05-27_

Orchestrator: main thread (Claude Code session). Implementer: `claude -p` in worktree. Reviewer: `codex exec`. Pattern per `dev/design/orchestration.md`.

## Handoff

- Plan: `dev/plans/prompts/0.7.0-PERF-VECTOR-QUANT-HANDOFF.md`
- Research: `dev/notes/0.7.0-vector-cost-research.md`
- HITL: `dev/plans/0.7.0-HITL-recommendations.md`
- Parallel work: `dev/plans/prompts/0.7.0-CORPUS-BUILD-HANDOFF.md` (real-data corpus that will validate Pack 2 RED tests once both complete)

## Baseline

- Branch: `main`
- Baseline SHA: `68c6339` (docs(0.7.0): lock corpus HITL answers + corpus-build agent handoff)

## Slice scoreboard

| ID | Subject | Status | Branch | Cherry-pick |
|---|---|---|---|---|
| S0 | ADR draft (Design step) | pending | — | — |
| S0-review | Codex review of ADR | pending | — | — |
| P1-DESIGN | Pack 1 design memo | pending | — | — |
| P1-IMPL | Pack 1 schema + ingest (TDD) | pending | — | — |
| P1-REVIEW | Codex review of Pack 1 | pending | — | — |
| P2-RED | Pack 2 RED tests | pending | — | — |
| P2-IMPL | Pack 2 query-path rewrite | pending | — | — |
| P2-REVIEW | Codex review of Pack 2 | pending | — | — |
| P2-CANONICAL | Canonical-CI dispatch + lock-flip prep | pending | — | — |

## Per-AC scoreboard (target)

| AC | Current (canonical-CI, W4.1) | Target | Status |
|---|---|---|---|
| AC-013 p50 | 2048 ms | ≤ 80 ms | ⏳ |
| AC-013 p99 | 2327 ms | ≤ 300 ms | ⏳ |
| AC-019 tail | (existing 10× bound) | improve | ⏳ |
| recall@10 | n/a | ≥ 0.90 | ⏳ |

## Open HITL items

- ADR lock-flip — deferred to post-Pack-2 GREEN.
- Budget numeric values — to be filled with canonical-CI measurements + ~10% headroom after P2-CANONICAL.

## Next action

Spawn S0 implementer (ADR draft).

## Compaction-resume checklist

1. Read this file.
2. Read `dev/plans/prompts/0.7.0-PERF-VECTOR-QUANT-HANDOFF.md`.
3. Read `dev/design/orchestration.md` (mechanics).
4. `git worktree list` for outstanding implementer worktrees.
5. Resume from "Next action".
