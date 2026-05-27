# STATUS — 0.7.0 PERF-VECTOR-QUANT

_Last updated: 2026-05-27 — Pack 1 + Pack 2 implementation CLOSED. P2-CANONICAL awaits user authorization._

Orchestrator: main thread (Claude Code session). Pattern per `dev/design/orchestration.md`.

## Handoff

- Plan: `dev/plans/prompts/0.7.0-PERF-VECTOR-QUANT-HANDOFF.md`
- Research: `dev/notes/0.7.0-vector-cost-research.md`
- HITL: `dev/plans/0.7.0-HITL-recommendations.md`
- Parallel work: `dev/plans/prompts/0.7.0-CORPUS-BUILD-HANDOFF.md`

## Baseline

- Branch: `main`
- Pre-campaign HEAD: `68c6339`
- Post-campaign HEAD: `d500d66`

## Slice scoreboard

| ID | Subject | Status | Cherry-pick(s) | Codex |
|---|---|---|---|---|
| S0 | ADR draft | **CLOSED** | `79cea9f`, `277fa4c` (inline fixup) | CONCERN → fixed inline |
| P1-DESIGN | Pack 1 design memo | **CLOSED** | `340502e`, `83869d6` (fix-1), `0fa6710` (fix-2), `adcbbfe` (fix-3), `8213b56` (closure) | BLOCK / BLOCK / BLOCK / **PASS** |
| P1-IMPL | Pack 1 schema + ingest | **CLOSED** | `d96c4b0`, `9b9f840`, `f5da3e4`, `7d4aa2c`, `b533f61`, `cc5d15e` (inline fixup) | CONCERN → fixed inline |
| P2-RED | Pack 2 RED tests | **CLOSED** | `d468999`, `4060a54` (closure) | **PASS** |
| P2-IMPL | Pack 2 query rewrite | **CLOSED** | `26ef3dc`, `28c2d6d`, `d500d66` (closure) | CONCERN (scope-precision nit, override) |
| P2-CANONICAL | canonical-CI dispatch + lock-flip | **AWAITS USER** | — | — |

## Per-AC scoreboard

| AC | Pre-campaign | Dev-box smoke N=10K | Canonical N=1M target | Status |
|---|---|---|---|---|
| AC-013 p50 | 2048 ms @ N=1M (W4.1) | 6 ms (post-P2) | ≤ 80 ms | GREEN at dev-box; awaits canonical-CI |
| AC-013 p99 | 2327 ms @ N=1M (W4.1) | 12 ms (post-P2) | ≤ 300 ms | GREEN at dev-box; awaits canonical-CI |
| AC-013b recall@10 | n/a | 1.0 (post-P2) | ≥ 0.90 | GREEN at dev-box; awaits canonical-CI |
| AC-019 stress p99 | 8388 ms @ N=1M (W4.1) | 131 ms (post-P2) | improve | GREEN at dev-box; awaits canonical-CI |
| AC-012 / AC-017 / AC-018 | GREEN | GREEN | GREEN | unchanged |
| AC-020 | GREEN at canonical | flaky dev-box (pre-existing, stash-sandwich confirmed) | GREEN | unchanged |

## What landed

**ADR**: `dev/adr/ADR-0.7.0-vector-binary-quant.md` — status `draft, HITL-required`. Locks the architectural decision (binary quant + f32 rerank as a data-encoding change, not a second architectural lever).

**Design memo**: `dev/design/0.7.0-vector-quant-pack1.md` (fix-3 PASS) — D1-D8 resolved against the actual code anchors.

**Pack 1 code** (writer + schema):
- New `fathomdb-schema` migration step 9 (`migrations/009_vector_binary_quant.sql`) — preflight CHECK for unknown kinds.
- `migrate_vector_partition_to_pack1` in `lib.rs` — dim-aware in-place reshape (DROP+CREATE same name + staged copy + `vec_quantize_binary` SQL-side + D3 `KIND_TO_SOURCE_TYPE_CASE_SQL` + `strftime('%s','now')`).
- `ensure_vector_partition` updated: detects existing shape (none / old / Pack 1) and routes.
- Writer double-write at both sites (`commit_projection_outcomes`, `write_vector_for_test`) populates `embedding`, `embedding_bin`, `source_type`, `kind`, `created_at` inside the existing single transaction.
- `resolve_source_type` helper enforces 6-value HITL lock with `doc→article` coercion. Drift-detection unit test executes the SQL CASE against in-memory SQLite and asserts byte-equal output with the Rust helper.

**Pack 2 code** (reader):
- `read_search_in_tx` (lib.rs:2307-2370): replaced single-phase f32 brute-force with two-phase bit-KNN (`TOP_K_BIT_CANDIDATES=64`) + f32 rerank via `vec_distance_l2`. Single Deferred read transaction preserved; `?1` bound once and reused.

**Pack 2 tests**:
- `AC013_BUDGET_P50/P99` re-pinned to 80/300 ms.
- `ac_013b_recall_at_10_floor` — recall ≥ 0.90 against in-test f32 brute-force ground truth.

## Open HITL items (awaits user)

1. **Canonical-CI dispatch** — run the perf-canonical workflow with `targets="ac013 ac019"` and the locked W4.1-stacked-O1 env knobs. Confirm:
   - AC-013 p50 ≤ 80 ms, p99 ≤ 300 ms at N=1M.
   - AC-013b recall@10 ≥ 0.90 at N=1M.
   - AC-019 GREEN.
2. **Numeric budget lock** — fill the placeholder AC-013/AC-019 budget rows in `dev/adr/ADR-0.7.0-text-query-latency-gates-revised.md` with canonical-CI measurements + ~10% headroom (per ADR-vector-binary-quant § 6 open question).
3. **ADR lock-flip** — both ADRs from `draft, HITL-required` → `locked`.
4. **Update `dev/notes/pcache2-followups.md`** — reference AC-013/019 closure under the new lever.
5. **Push to origin** — explicit user OK required (per handoff constraint).

## Compaction-resume checklist

1. Read this file.
2. `git log --oneline 68c6339..HEAD` for the campaign commit arc.
3. `dev/adr/ADR-0.7.0-vector-binary-quant.md` for the decision.
4. `dev/design/0.7.0-vector-quant-pack1.md` for design rationale.
5. `dev/plans/runs/*PVQ*review*.md` for each codex verdict.
6. `git worktree list` — no PVQ worktrees should remain (all cleaned).

## Codex iteration cost (campaign retro)

- S0: 1 codex (CONCERN → inline fixup).
- P1-DESIGN: 4 codex (BLOCK/BLOCK/BLOCK/PASS). Sources of churn: HITL-lock vocab; vec0 ALTER RENAME unsupported in 0.1.7; rebuild_vec0 async semantics; preflight CHECK + strftime mechanics. Each fix-N constrained vs prior verdict.
- P1-IMPL: 1 codex (CONCERN → inline drift-test strengthen + comment fix).
- P2-RED: 1 codex (PASS).
- P2-IMPL: 1 codex (CONCERN, scope-precision nit, override).

Total: 8 codex passes; 3 BLOCKs (all on the same slice, P1-DESIGN); design memo doubled as the constraint-discovery vehicle.
