# NOTE → Orchestrator (0.8.11 / EXP-AF) — mandatory governance-reclassification review of `record_feedback`

**From:** 0.8.8 Release Orchestrator · **Date:** 2026-06-28 · **HITL decision:** ACCEPT now + mandatory review at 0.8.11.

## What 0.8.8 did (HITL-ratified)

Slice 15 added three telemetry methods to the SDK surfaces (`enable_telemetry`,
`last_telemetry_query_id`, `record_feedback`) and classified all three as **observability
instrumentation** — excluded from the governed-surface *application-command* allowlist (alongside
`counters` / `setProfiling` / `attachSubscriber`), in `src/python/tests/test_surface.py` and
`src/ts/tests/surface.test.ts`. This is correct for 0.8.8's scope: telemetry is **local, opt-in,
off-by-default, no-egress**, and `record_feedback` only appends agent labels to a local JSONL sink.

## HITL ruling (2026-06-28)

ACCEPT the instrumentation classification for all three **now**. **BUT** `record_feedback` is the
one that *mutates state with exogenous (agent-supplied) input* — so it is tagged for a **mandatory
reclassification review at 0.8.11 (EXP-AF)**: decide whether `record_feedback` should become a
**first-class governed application command** (moved into
`src/conformance/governed-surface-allowlist.json` + the Rust facade allowlist + the X1 surface
suites) rather than instrumentation. The trigger is when agent-feedback becomes a load-bearing input
to a learned/active-feedback (AF) loop rather than passive local telemetry.

## Action for the 0.8.11 / EXP-AF orchestrator

- [ ] Review `record_feedback` governance classification (instrumentation vs governed command).
- [ ] If reclassified: move it into the governed allowlist(s) + add the X1 cross-binding surface
      assertions + bump the governed-surface counts; update `dev/interfaces/*.md`.
- [ ] `enable_telemetry` / `last_telemetry_query_id` stay instrumentation unless EXP-AF says otherwise.

Reference: 0.8.8 Slice 15 (`dev/design/0.8.8-telemetry-design.md`), `runs/STATUS-0.8.8.md`.
