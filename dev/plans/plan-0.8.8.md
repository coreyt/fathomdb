# FathomDB 0.8.8 — Plan (state-machine ladder) · **Observability & telemetry**

> **Plan-as-state-machine.** Mod-5 ladder + reserved-gap policy + "Immediate Next Slice". Authoritative
> contracts → `0.8.8-implementation.md`; live state → `runs/STATUS-0.8.8.md`; deps/decision record →
> `0.8.6-0.8.16-PROGRAM-SEQUENCING.md`. Run via `/goal complete 0.8.8` as an **orchestrator** session.
>
> **Theme.** Make retrieval *legible*. Today retrieval is a black box at the result level — `SearchHit`
> carries only the blended `score` plus the just-shipped `ce_score` (the lone retrieval-explainability
> field). This release builds (#1) the retrieval `EXPLAIN` surface and (#10) telemetry + real-gold
> capture — the two sibling observability/legibility capabilities that every later feature wants to be
> seen through, that every M-experiment wants for attribution, and that a transparent router cannot
> exist without.
>
> **Footprint.** Both IN-LIBRARY; `explain` is opt-in and **zero-cost when off** (hot path unchanged).
> Telemetry capture is local, deterministic, no network. Real-gold *capture* is in-library; gold
> *curation* is EVAL-ONLY.

---

## 1. Goal & scope

- **#1 — Retrieval `EXPLAIN` (EXP-OBS).** Opt-in `search(..., explain=True) → Explanation`: per-hit
  arm-provenance (vector-ANN / FTS-BM25 / graph, with per-arm rank), per-hit score breakdown
  (`rrf_norm`, `ce_score`, blended `score`, filter exclusions), and a query-level trace
  (`k, pool_n, α, MMR, embedder identity`, timings via existing `counters()`/profiling). Generalize the
  existing graph-`EXPLAIN` seam (`explain_graph_neighbors_for_test`) + `TraceReport` rather than
  inventing new machinery. `ce_score` (0.8.5 EXP-0) is increment #1.
- **#10 — Telemetry + real-gold capture (OPP-9, AGREED).** A local, opt-in telemetry channel that
  records real query→result→(agent feedback) events, and a capture path that turns them into a
  **test/train real-gold** set — FathomDB's exit from synthetic-gold purgatory and the head of the
  virtuous loop. Capture is in-library; the gold pipeline is eval-side.

*Why paired:* both answer "what actually happened, and was it right?" — EXP-OBS surfaces the *mechanism*
trace, telemetry captures the *outcome* trace; together they are the legibility substrate for M-work
attribution and (downstream, out of scope here) learned routing.

---

## 2. Requirements + acceptance criteria (release DoD — frozen at Slice 0)

| ID | Requirement | Acceptance signal |
|----|-------------|-------------------|
| R-OBS-1 | `explain=True` returns per-hit arm-provenance + score-breakdown + query trace | Golden test: a known hybrid query yields the correct arm attribution + ranks; fields match the computed RRF/CE values |
| R-OBS-2 | `explain` is zero-cost when off | Bench: hot-path latency (AC-013 envelope) **unchanged** vs baseline within noise; no allocation when `explain=False` |
| R-OBS-3 | Reuses existing seams | Built on graph-`EXPLAIN` + `TraceReport`/`counters()`; codex §9 confirms no parallel machinery |
| R-OBS-4 | Py + TS SDK parity | X1 harness exercises `explain` and asserts cross-binding-equal payloads |
| R-TEL-1 | Opt-in local telemetry: query→result→feedback events | Events recorded deterministically; **off by default**; no network egress (footprint test) |
| R-TEL-2 | Real-gold capture pipeline | Telemetry → a labeled gold record schema; a fixture run produces a valid gold row consumable by the eval harness |
| R-TEL-3 | Privacy/footprint honesty | Telemetry payload documented; no content leaves the box; agent-supplied relevance labels are the only exogenous signal |

New ACs: candidates at Slice 0 (explain-contract) and the telemetry/real-gold gate.

---

## 3. Slice ladder (mod-5)

```
0 → 5 → 10 → 15 → 20 → 40
```

| Slice | Title | Work-type | Depends-on |
|------:|-------|-----------|-----------|
| **0** | Setup + ADR — `Explanation` payload schema ADR (the §6 `initial-arch` spec); telemetry-event + real-gold schema ADR; privacy/footprint contract | design-adr | — |
| **5** | **EXP-OBS KEYSTONE** — per-hit arm-provenance + score-breakdown + query trace behind `explain=True`; generalize the graph-`EXPLAIN`/`TraceReport` seam | implementation | 0 |
| **10** | **EXP-OBS SDK + zero-cost proof** — Py+TS `explain` parity harness; hot-path no-cost bench (RED if regression) | implementation | 5 |
| **15** | **Telemetry capture** — opt-in local query→result→feedback event channel (deterministic, no egress) | implementation | 0 |
| **20** | **Real-gold pipeline** — telemetry → labeled gold schema → eval-harness ingestion; fixture-validated | implementation (eval) | 15 |
| **40** | **Verification + Release Readiness (0.8.8)** — X1/X2/X3 + R-OBS/R-TEL AC gate + zero-cost bench | verification | 5,10,15,20 |

**Keystones / hard gates.** Slice 5 (EXP-OBS) is the keystone. **R-OBS-2 zero-cost is a hard gate** —
the explain path must not perturb the AC-013 hot-path latency envelope; a regression BLOCKS. Telemetry
**off-by-default + no-egress** is a hard footprint gate at Slice 15.

**Tracks (parallelizable).** EXP-OBS track **5 → 10** ∥ telemetry track **15 → 20**, both off Slice 0.

---

## 4. Reserved-gap policy

Carried unchanged (`0.8.1-plan.md` §Numbering): mod-5 planned slices; gaps are fully-orchestrated
insertion slices off a fresh `main` baseline; HALT to HITL on band overflow.

## 5. Cross-cutting DoD (X1/X2/X3 — bind EVERY slice)

X1 SDK parity + functional harnesses · X2 `mkdocs build` green · X3 docs + DOC-INDEX per slice. Slice 40
enforces X2 as a release gate. `runs/STATUS-0.8.8.md` carries the per-slice X column.

## 6. Acceptance-criteria policy

`dev/acceptance.md` locked; track by OPP-id/G-gap + TDD names; new ACs only at gated slices (Slice 0,
Slice 40), HITL-decided.

## 7. Prerequisites

1. **0.8.6 closed** — the release machinery (DoD-shippable) and provider protocol exist. (EXP-OBS itself
   only hard-depends on the shipped `ce_score`, but the program runs 0.8.6 first.)
2. Worktrees off `$(git rev-parse main)`; GPU/maturin on MAIN tree only.

## 8. Out-of-band / parallel notes

- **Coordinate with the experiment program:** EXP-OBS and telemetry touch the **retrieval + eval
  hot-path** the M-experiments share. Sequence these slices with the active M-work owner so the explain
  fields and gold schema match what the experiments consume (this is the one release in the line that is
  *not* fully decoupled from the experiment track).
- **0.8.9 (OOB) — CI integrity (#12/#14)** may run in parallel; no file overlap.

## 9. Immediate next slice

**Slice 0 — `Explanation` + telemetry/real-gold schema ADRs.** Ratify the §6 `initial-arch` explain
payload shape and the gold-record schema with the M-work owner before code; stand up
`runs/STATUS-0.8.8.md`. Then fan out Slices 5 ∥ 15.
