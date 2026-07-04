# FathomDB 0.8.14 — Plan (state-machine ladder) · **Substrate & recall features**

> **Plan-as-state-machine.** Mod-5 ladder + reserved-gap policy + "Immediate Next Slice". Authoritative
> contracts → `0.8.14-implementation.md`; live state → `runs/STATUS-0.8.14.md`; deps/decision record →
> `0.8.6-0.8.16-PROGRAM-SEQUENCING.md`. Run via `/goal complete 0.8.14` as an **orchestrator** session.
>
> **Theme.** The schema-migration release. Land the **kind-tagged coexisting-index substrate** (#2,
> EXP-S) and the **fielded FTS / BM25F recall lever** (#16, F5) **in one release so the engine pays one
> coordinated migration cost**. All IN-LIBRARY; CPU-only/deterministic query path preserved.
>
> **Reconciled 2026-07-02 (Steward):** #17 filter-grammar G4↔G10 unification, originally co-scoped here,
> **already SHIPPED in 0.8.11** (F-10 re-sequencing; PR #122, commit `ab3b4466`) — struck from this plan.
> The typed-constraint surface (`Filter`/`SearchFilter`) the 0.8.15 router leans on is on `main`. This
> plan's spine = EXP-S + F5.
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
- **#17 — Filter-grammar G4↔G10 unification — DONE in 0.8.11 (struck from 0.8.14).** Shipped as 0.8.11
  Slice 40 (`ab3b4466`, PR #122): one unified `Filter` contract over the G10 `SearchFilter`. The
  typed-constraint surface the 0.8.15 router's `constraints` block leans on is already on `main` — it is
  no longer 0.8.14 scope.

*Why this release / why coordinated:* #2 and #16 are both engine/schema migrations on the index write
path — doing them in one release lets EXP-S land the kind-tag/field columns that F5 then rides, paying
one `SCHEMA_VERSION` bump and one re-index rather than two. (#17, which would have composed the
typed-constraint surface with the new substrate, already shipped in 0.8.11 — see above.)

---

## 2. Requirements + acceptance criteria (release DoD — frozen at Slice 0)

| ID | Requirement | Acceptance signal |
|----|-------------|-------------------|
| R-SUB-1 | Row-kinds (leaf/coverage/graph) coexist in one store | Schema migration lands; a fixture writes ≥2 kinds; queries select by kind |
| R-SUB-2 | Incremental multi-index write is deterministic | Determinism test: same input → byte-identical index state across runs/backends-on-same-CPU |
| R-SUB-3 | Migration is forward-only + guarded | `SCHEMA_VERSION` bump; migration test (old DB → new) green; eu7 re-clear if vectors are touched |
| R-F5-1 | Fielded BM25F with tunable `b`/field weights | RED→GREEN: a field-weighted query outranks an unweighted baseline on a known fixture |
| R-F5-2 | F5 ships per **HITL Option-C override** (NOT gate-clearance) | **The R-F5-2 pre-registered gate did NOT clear** (only a synthetic n=16 15b smoke passed; full at-power run deferred; 0.8.3 shipped at marginal parity, no measured Mem0 gap). F5 ships this release by **conscious HITL override** (decider: coreyt, 2026-07-03) on intrinsic recall-lever merit + one-`SCHEMA_VERSION`-bump economics. KILL path retained. See ADR-0.8.14 §D8. |
| ~~R-FIL-1~~ | ~~G4 grammar unified with G10 `SearchFilter`~~ — **SHIPPED in 0.8.11 (struck)** | Satisfied by 0.8.11 Slice 40 (`ab3b4466`); not a 0.8.14 gate |
| R-X-1 | Py + TS SDK parity for EXP-S + F5 | X1 cross-binding harness green |
| R-GATE | eu7 ANN fidelity ≥ 0.90 (one-sided CI) holds after any re-embed | `recall_gate.rs`: ci_hi ≥ 0.90 PASS; a breach BLOCKS→HITL |

New ACs: candidates at Slice 0 (substrate determinism) and at the F5/filter gates.

---

## 3. Slice ladder (mod-5)

```text
0 → 5 → 10 → 20 → 25 → 40      (15 = void reserved gap — #17 shipped in 0.8.11)
```

| Slice | Title | Work-type | Depends-on |
|------:|-------|-----------|-----------|
| **0** | Setup + ADR — EXP-S substrate migration ADR (row-kinds, determinism check, KILL path); confirm the F5 ADR pre-conditions | design-adr | — |
| **5** | **EXP-S substrate KEYSTONE** — row-kinds + plural coexisting indexes + incremental multi-index write + determinism check; `SCHEMA_VERSION` bump | implementation (schema) | 0 |
| **10** | **F5 fielded BM25F** — field-weighted FTS + tunable `b`, riding the EXP-S field columns | implementation (schema) | 5 |
| **15** | *(void reserved gap)* — #17 filter-grammar **SHIPPED in 0.8.11** (F-10; PR #122, `ab3b4466`) | — | — |
| **20** | **eu7 re-clear + migration verify** — if Slices 5/10 touched vectors, re-clear the one-sided fidelity gate; old→new migration test | verification | 5,10 |
| **25** | *(reserved gap)* **Merge `0.8.14-gpu-rerank`** — rebase branch `d9e61c66` onto then-current base, full `agent-verify.sh` (py/ts/security); opt-in `rerank-cuda` GPU CE + `embed_batch_cls`, default-CPU-unchanged (EXP-S GPU sub-part; `0.8.x-remaining-todos §1`) | integration | 5 |
| **40** | **Verification + Release Readiness (0.8.14)** — X1/X2/X3 + R-SUB/R-F5 AC gate + eu7 gate | verification | 5,10,20,25 |

**Keystones / hard gates.** **Slice 5 (EXP-S) is the keystone** — F5 (10) rides its field columns, so
5 → 10 is a hard sequence (do EXP-S first within the release). **eu7 ≥ 0.90 (one-sided CI) is a hard
BLOCK→HITL gate** at Slice 20 if any re-embed occurs (`fathomdb-recall-fidelity-vs-relevance`). **F5 ships in 0.8.14 by conscious HITL
override** (decider: coreyt, 2026-07-03) — the R-F5-2 pre-registered gate did NOT clear; it ships on
intrinsic recall-lever merit + one-`SCHEMA_VERSION`-bump economics, KILL path retained (ADR-0.8.14 §D8).

**Tracks.** Substrate→F5 track **5 → 10 → 20 → 25** (single spine; the former parallel filter track is void — #17 shipped in 0.8.11).

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
   defers and the release ships EXP-S only.
3. **Frozen corpus + eu7 harness** reproduced locally for the fidelity re-clear.
4. Worktrees off `$(git rev-parse main)`; maturin/GPU build on the MAIN tree only.

## 8. Out-of-band / parallel notes

- **Coordinate with the M-work + router-design owners:** EXP-S is the router's physical substrate and
  F5 is an M2/M5 recall lever — align the row-kind taxonomy and field weights with what the experiments
  and the router design expect, so the schema is migrated once for both.
- This is the **heaviest engine release** in the line; sequence it when the experiment program can
  tolerate a coordinated schema migration (it bumps `SCHEMA_VERSION` and may trigger a re-index).

## 9. Immediate next slice

**Slice 0 — CLOSED (2026-07-03).** ADR `dev/adr/ADR-0.8.14-exp-s-kind-tagged-coexisting-index-substrate.md`
ratified by HITL (checkpoint approved): D1 separate `row_kind` column; D2 per-kind index-target dispatch;
D3 flush-then-byte-compare determinism check; D4 one coordinated migration (`SCHEMA_VERSION` 15→16 EXP-S,
16→17 F5); D5 discharges **TC-1** (OPP-12 projection-registry forward-compat seam); D6 eu7 no-op unless
vec0 rewritten; D7 KILL paths. **D8 = Option C:** F5 ships by conscious HITL override, NOT gate-clearance
(see R-F5-2). Board stood up at `runs/STATUS-0.8.14.md`.

**Slice 5 (EXP-S KEYSTONE) — CLOSED (2026-07-04).** Cherry-picked `ba15e176` (step-16 migration:
`SCHEMA_VERSION` 15→16, `canonical_nodes.row_kind` DEFAULT 'leaf', accretion-exempt) + `718cfe94`
(engine `RowKind{Leaf,Coverage,Graph}`, `index_targets_for_row_kind` per-kind dispatch seam, R-SUB-2
determinism check) onto `main`. codex §9 **PASS** (no findings; determinism test non-vacuous, doc-type
`kind` sites untouched, D6 no vec0 rewrite verified) — `runs/0.8.14-slice-5-review-20260704T001710Z.md`.
Full-workspace clippy+check both exit 0. R-SUB-1/R-SUB-2/R-SUB-3 GREEN; D5/TC-1 discharged. D6:
vec0 NOT rewritten → eu7 at Slice 20 is a documented no-op.

**Slice 10 (F5 fielded BM25F) — CLOSED (2026-07-04).** Cherry-picked `b145754f` (step-17 migration:
`SCHEMA_VERSION` 16→17, `search_index_v2` multi-column FTS5 over kind/body/status, O(N) rebuild) +
`c57e4e99` (in-engine BM25F scorer, tunable field weights + `b`/`k1`) + `9d8e368b` (fix-1: scorer
tokenization-faithful via `fts5_tokenize`) + `a7c3c145` (fix-2: comment correction). codex §9: CONCERN →
fix-1 (resolved the substantive tokenization finding) → CONCERN → fix-2 (comment-only) → landed;
re-review confirmed the medium finding resolved, sole residual a LOW comment now fixed. Full-workspace
clippy+check both exit 0 on the landing head. R-F5-1 GREEN (falsifiable ranking-flip + tokenization-faithful
tests). **Ships per the D8 Option-C HITL override** (recorded as override, NOT gate-clearance).
**Justified deviation from ADR-0.8.1:** FTS5's `bm25()` pins `k1`/`b`, so scoring is in-engine (BM25F over
`search_index_v2 MATCH`-recalled candidates) to honor the ADR's tunable-`b` requirement; field set +
weighting per ADR. D6: no vector touch → eu7 at Slice 20 is a no-op.

**Next — Slice 20 (eu7 re-clear + migration verify).** Per D6, Slices 5/10 touched NO vectors (pure
additive schema + FTS), so the eu7 gate is a **documented no-op regression check**; also verify old→new
(v15→v17) migration. Any eu7 breach = BLOCK→HITL. Then 25 (gpu-rerank merge) → 40 (release readiness).
Off a fresh `origin/main` baseline.
