# STATUS — 0.8.11 (live)

> Orchestrator session `fathom-0.8.11-orchestrator-b`. Plan → `dev/plans/plan-0.8.11.md`;
> contracts → `dev/plans/0.8.11-implementation.md`; deps → `0.8.6-0.8.16-PROGRAM-SEQUENCING.md` (F-11).
> Branch: `0.8.11` (off `origin/main` @ `80c6b8b8` + F-11 plan rewrite). Goal: **complete Slices 0–40.**

## Headline

0.8.11 owns and discharges the missed planner-router experiment ladder (F-11) as **Track E** (eval
spine, Slices 5–35) plus **Track G** (#17 filter-grammar + F-8b, Slice 40), converging at Slice 45.
Slice 0 (this) freezes the pre-registrations + ADRs + HITL decisions. **Budget: ~$20 priced-LLM
ceiling** (raised from $0, HITL 2026-06-28); running tally below.

## Slice board

| Slice | Title | Track | State | Notes |
| ---: | --- | :---: | --- | --- |
| **0** | ADRs + ladder pre-registration + STATUS standup | — | **DONE** | contracts ✅; STATUS ✅; ADR-0.8.11 ✅ (`a9ba8a5a`); ledger scaffold ✅ (F-11 rows REGISTERED); 2 HITL Slice-0 decisions ✅ (A / conditional) |
| 5 | Gate-0 + Gate-2 (eval foundation) | E | pending | blocked-by 0 |
| 10 | EXP-A ‖ EXP-M4 | E | pending | blocked-by 5 |
| 15 | EXP-B′ joint tuning (KEYSTONE) | E | pending | blocked-by 10 (A∧M4) |
| 20 | EXP-Fr-acc base | E | pending | blocked-by 5 |
| 25 | EXP-Fr-acc/VoI finalize | E | pending | blocked-by 20 |
| 30 | EXP-AF value test (KILL/GO) | E | pending | blocked-by 25; HITL #3 |
| 35 | L2 router prototype + pre-stage | E | pending | blocked-by 15∧25∧30 |
| 40 | #17 filter-grammar + F-8b exec | G | pending | blocked-by 0 only (∥ spine) |
| 45 | Verification + release readiness | — | pending | blocked-by 5–40 |

## HITL decision points (four; owner: steward → HITL)

1. **F-8b** (Slice 0) — `record_feedback` instrumentation → governed command?
   **RULED 2026-06-28: conditional on EXP-AF** (stays instrumentation; promote at Slice 40 iff
   EXP-AF GO). `enable_telemetry`/`last_telemetry_query_id` stay instrumentation regardless.
2. **Filter-grammar shape** (Slice 0) — unified type vs thin adapter?
   **RULED 2026-06-28: Option A — one unified `Filter` + two backends**, with the anti-laziness
   mandate (reserved-gap only for a genuine field-resolution constraint). → ADR-0.8.11.
3. **EXP-M4 swap KILL/GO** (Slice 10 readout) — beat bge-small? Default keep; productized swap
   out-of-0.8.11, separately gated. — PENDING.
4. **EXP-AF KILL/GO** (Slice 30 readout) — agent signal beats `ce_score` net of round-trip?
   Gates the L2 feedback arm + F-8b promotion. — PENDING.

## Budget — running `$` tally (ceiling ~$20)

| Experiment | Ceiling | Spent | Status |
| --- | ---: | ---: | --- |
| Gate-0 (scoped labeling) | $1 | $0 | not started |
| EXP-B′ judge | $6 | $0 | not started |
| EXP-Fr-acc base | $3 | $0 | not started |
| EXP-Fr-acc/VoI | $3 | $0 | not started |
| EXP-AF | $5 | $0 | not started |
| Reserve | $2 | $0 | — |
| **Total** | **$20** | **$0** | — |

Gate-2 / EXP-A / EXP-M4 are $0 (local / GPU). No priced run starts before its pre-registration
(`0.8.11-implementation.md §1`) is committed; cheap-validate (gemini-flash-lite) before each spend.

## Cross-cutting DoD (X1/X2/X3)

- **X1** SDK parity (Py↔TS) — applies to any surface change (Slice 40 filter contract; `record_feedback`
  if promoted). Eval slices' "shippable" DoD = landed result doc + reproducible script + ledger row.
- **X2** `mkdocs build --strict` green.
- **X3** docs + DOC-INDEX entry per surface-touching slice.

## Verification log

- 2026-06-28: branch `0.8.11` cut off `origin/main` (`80c6b8b8`); plan rewrite + F-11 sequencing +
  BEIR manifest committed; merged `origin/main`. Working tree clean at Slice 0 start.
- 2026-06-28: Slice 0 closed. ADR-0.8.11 filter-grammar (`a9ba8a5a`) found **NO reserved-gap
  trigger** — all 4 G10 fields resolve in both stores (`source_type` constant-folds via
  `resolve_source_type(kind)`); arbitrary json-paths are typed-rejected by `search_filtered` (a
  defined dispatch outcome). **Correction:** G4 `read.list`/`Predicate` is **already built +
  SDK-exposed** (0.8.0) → Slice 40 is a type-unification + dispatch **refactor**, not greenfield.
  Unified type: `Filter { terms: Vec<FilterTerm> }` (implicit AND); one impl-level TBD (kind
  redundancy/constant-fold) left for Slice 40.

## Experiments-ledger (F-11 closure tracker)

Rows to be added to `dev/experiments-ledger.md` (zero planner-router rows today): Gate-0, Gate-2,
EXP-A, EXP-M4, EXP-B′ (+B′.5), EXP-Fr-acc base, EXP-Fr-acc/VoI, EXP-AF. Each lands at its slice.
