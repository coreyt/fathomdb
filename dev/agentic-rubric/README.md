# `dev/agentic-rubric/` — the operational agent-audit harness

This subtree is the **operational tool** that *runs* the agent-harness evaluation rubric — it
does not define a rubric. The rubric of record is the existing, TERMINAL
`dev/design/agent-harness-evaluation-rubric-v3.md` (v3.1). This subtree productizes that
instrument into a repeatable, HITL-driven harness that scores a run, proposes changes to the
audited repo's agents / CLI / prompts, applies approved changes in isolation, and re-measures
at milestones.

It is kept in one isolated directory (rather than scattered across `dev/`) so the operational
tool reads as one subsystem and could later audit another repo. It **reuses**, never
re-derives, the existing rubric, detectors, and ledger.

## What already exists (reused, not rebuilt)

| Piece | Where | Role here |
| --- | --- | --- |
| Rubric v3.1 (62 criteria, dims A–H, `[D]/[L]/[H]`, 12 HARD, Q-SEV) | `dev/design/agent-harness-evaluation-rubric-v3.md` | The instrument this tool executes. |
| Audit + revision method | `dev/design/rubric-audit-and-revision-method.md` | The measurement discipline (forward/reverse/discrimination). |
| First run (scorecard) | `dev/design/rubric-run-0.8.19-2026-07-10.md` | Ground-truth to reproduce; scorecard shape to emit. |
| Transcript parse + deterministic detectors | `dev/experiments/rubric-stress-test/{parse,detectors,run_detectors}.py` | The `[D]` layer + `[H]` evidence-gatherers. |
| Severity vector + adjudication + IRR | `dev/experiments/rubric-stress-test/audit/{severity_vector_v3.json,build_audit.py,compute_irr.py}` | Aggregation weights + judge↔human agreement. |
| Ledger | `dev/steward/agent-rubric-ledger.jsonl` (via `dev/agent-tools/{ledgerwrite,ledgerwatch}`) | Append-only decision/run trail; this tool adds `proposal`/`apply` kinds. |

## What this subtree adds (the missing capabilities)

The existing project stops at "score by hand from artifacts." This tool builds the four
capabilities that are still missing:

1. **Automated airlock `[L]`/`[H]` judge** (today done by a human/agent session).
2. **Propose** specific changes to the *audited repo's* agents / CLI / prompts (distinct from
   revising the rubric — the rubric is TERMINAL).
3. **HITL-gated apply** of approved changes, in an isolated worktree, behind `agent-verify`.
4. **Milestone re-measurement** of agent-performance deltas, with anti-Goodhart IRR tracking.

## Layout

| Path | Purpose |
| --- | --- |
| `design.md` | Design of record for the operational harness. |
| `requirements.md` | Requirements for the four new capabilities (traceable to v3 criteria + `TC-RUBRIC-N`). |
| `acceptance.md` | Falsifiable acceptance signals per requirement. |
| `prompts/{judge,proposer,decision-package}.md` | Prompt gists (cite v3 criteria + verification class). |
| `harness/` | Python package `harness` (added in Slice 5+; see `design.md`). |

## Status

Planning deliverables only (docs). Harness code lands per the slice ladder in `design.md`
(Slice 5 = `$0` no-network infra first, reproducing the 0.8.19 scorecard deterministically).
