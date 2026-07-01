# FathomDB 0.8.14 — Plan (state-machine ladder) · **Substrate & recall features**

> **Plan-as-state-machine.** Mod-5 ladder + reserved-gap policy + "Immediate Next Slice". Authoritative
> contracts → `0.8.14-implementation.md`; live state → `runs/STATUS-0.8.14.md`; deps/decision record →
> `0.8.6-0.8.16-PROGRAM-SEQUENCING.md`. Run via `/goal complete 0.8.14` as an **orchestrator** session.
>
> **Theme.** The schema-migration release. Land the **kind-tagged coexisting-index substrate** (#2,
> EXP-S) and the **fielded FTS / BM25F recall lever** (#16, F5) **in one release so the engine pays one
> coordinated migration cost**, plus the **filter-grammar G4↔G10 unification** (#17, the typed-constraint
> surface the router will lean on). All IN-LIBRARY; CPU-only/deterministic query path preserved.
>
> **Footprint.** All three IN-LIBRARY. Re-embeds/index rebuilds are far cheaper on GPU (0.8.7) but the
> shipped query path stays CPU-only/1-bit/deterministic.

---

## 1. Goal & scope

- **#2 — Kind-tagged coexisting-index substrate (EXP-S).** Today `kind` is doc-type-only and the
  portfolio's "one store, many indexes" is *asserted, not built*. Add row-kinds (leaf / coverage /
  graph), plural indexes coexisting in one store, incremental multi-index write, and a **determinism
  check**. This is the physical foundation the (out-of-scope) router needs; its **KILL path** = router
  stays agent-side, indexes stay eval-side.
- **#16 — Fielded FTS / BM25F (F5).** A genuine recall lever (field-weighted lexical scoring,
  tunable `b`) — already ADR-ratified (`ADR-0.8.1-deferred-f5-fielded-fts-bm25f.md`), conditional on the
  0.8.3 15b-proxy passing + a Slice-20 Mem0 gap. It is a **schema migration**, which is exactly why it
  is coordinated with EXP-S in one release rather than paid for twice.
- **#17 — Filter-grammar G4↔G10 unification (gap-37).** Unify the G4 filter grammar with the shipped G10
  `SearchFilter` (reserved-gap 37, NEEDED per Slice-35). Yields the typed-constraint surface the router's
  `constraints` block (PSD §I.B) and intelligent filtering both depend on.

*Why this release / why coordinated:* #2 and #16 are both engine/schema migrations on the index write
path — doing them in one release lets EXP-S land the kind-tag/field columns that F5 then rides, paying
one `SCHEMA_VERSION` bump and one re-index rather than two. #17 touches the shipped G10 surface and is
sequenced here (not as a blind OOB drop-in) because the typed-constraint surface composes with the new
substrate.

---

## 2. Requirements + acceptance criteria (release DoD — frozen at Slice 0)

| ID | Requirement | Acceptance signal |
|----|-------------|-------------------|
| R-SUB-1 | Row-kinds (leaf/coverage/graph) coexist in one store | Schema migration lands; a fixture writes ≥2 kinds; queries select by kind |
| R-SUB-2 | Incremental multi-index write is deterministic | Determinism test: same input → byte-identical index state across runs/backends-on-same-CPU |
| R-SUB-3 | Migration is forward-only + guarded | `SCHEMA_VERSION` bump; migration test (old DB → new) green; eu7 re-clear if vectors are touched |
| R-F5-1 | Fielded BM25F with tunable `b`/field weights | RED→GREEN: a field-weighted query outranks an unweighted baseline on a known fixture |
| R-F5-2 | F5 ADR pre-conditions are met | The 0.8.3 15b-proxy pass + Mem0-gap condition is confirmed before F5 ships (else record + defer) |
| R-FIL-1 | G4 grammar unified with G10 `SearchFilter` | One filter contract; the shipped G10 paths re-expressed on it without behavior change (parity test) |
| R-X-1 | Py + TS SDK parity for all three | X1 cross-binding harness green |
| R-GATE | eu7 ANN fidelity ≥ 0.90 (one-sided CI) holds after any re-embed | `recall_gate.rs`: ci_hi ≥ 0.90 PASS; a breach BLOCKS→HITL |

New ACs: candidates at Slice 0 (substrate determinism) and at the F5/filter gates.

---

## 3. Slice ladder (mod-5)

```text
0 → 5 → 10 → 15 → 20 → 40
```

| Slice | Title | Work-type | Depends-on |
|------:|-------|-----------|-----------|
| **0** | Setup + ADR — EXP-S substrate migration ADR (row-kinds, determinism check, KILL path); confirm the F5 ADR pre-conditions; G4↔G10 unification design | design-adr | — |
| **5** | **EXP-S substrate KEYSTONE** — row-kinds + plural coexisting indexes + incremental multi-index write + determinism check; `SCHEMA_VERSION` bump | implementation (schema) | 0 |
| **10** | **F5 fielded BM25F** — field-weighted FTS + tunable `b`, riding the EXP-S field columns | implementation (schema) | 5 |
| **15** | **G4↔G10 filter unification** — one typed filter contract; re-express shipped G10 paths (parity) | implementation | 0 |
| **20** | **eu7 re-clear + migration verify** — if Slices 5/10 touched vectors, re-clear the one-sided fidelity gate; old→new migration test | verification | 5,10 |
| **40** | **Verification + Release Readiness (0.8.14)** — X1/X2/X3 + R-SUB/R-F5/R-FIL AC gate + eu7 gate | verification | 5,10,15,20 |

**Keystones / hard gates.** **Slice 5 (EXP-S) is the keystone** — F5 (10) rides its field columns, so
5 → 10 is a hard sequence (do EXP-S first within the release). **eu7 ≥ 0.90 (one-sided CI) is a hard
BLOCK→HITL gate** at Slice 20 if any re-embed occurs (`fathomdb-recall-fidelity-vs-relevance`). **F5
ships only if its ADR pre-conditions hold**, else record + defer.

**Tracks (parallelizable).** Substrate→F5 track **5 → 10 → 20** ∥ filter track **15** (off Slice 0).

---

## 4. Reserved-gap policy

Carried unchanged (`0.8.1-plan.md` §Numbering). Schema-migration follow-on (e.g. a backfill the
migration reveals) is a fully-orchestrated reserved-gap slice off a fresh `main` baseline, never an
ad-hoc patch.

## 5. Cross-cutting DoD (X1/X2/X3 — bind EVERY slice)

X1 SDK parity + harnesses · X2 `mkdocs build` green · X3 docs + DOC-INDEX per slice. `runs/STATUS-0.8.14.md`
carries the per-slice X column.

## 6. Acceptance-criteria policy

`dev/acceptance.md` locked; track by G-gap/F-id + TDD names; new ACs only at gated slices, HITL-decided.

## 7. Prerequisites

1. **0.8.6 closed** (release machinery / DoD-shippable) and **0.8.7 OOB GPU recommended-landed** (the
   re-index here is re-embed-heavy; GPU makes it minutes not hours).
2. **F5 ADR pre-conditions** (0.8.3 15b-proxy + Mem0-gap) confirmed at Slice 0 — else F5 records-and-
   defers and the release ships EXP-S + #17 only.
3. **Frozen corpus + eu7 harness** reproduced locally for the fidelity re-clear.
4. Worktrees off `$(git rev-parse main)`; maturin/GPU build on the MAIN tree only.

## 8. Out-of-band / parallel notes

- **Coordinate with the M-work + router-design owners:** EXP-S is the router's physical substrate and
  F5 is an M2/M5 recall lever — align the row-kind taxonomy and field weights with what the experiments
  and the router design expect, so the schema is migrated once for both.
- This is the **heaviest engine release** in the line; sequence it when the experiment program can
  tolerate a coordinated schema migration (it bumps `SCHEMA_VERSION` and may trigger a re-index).

## 9. Immediate next slice

**Slice 0 — EXP-S + F5 + filter ADRs.** Ratify the row-kind taxonomy + determinism contract, confirm
F5's ADR pre-conditions, and design the G4↔G10 unification; stand up `runs/STATUS-0.8.14.md`. Then run
Slice 5 (EXP-S) before 10 (F5); 15 (filter) in parallel.
