# ADR-0.8.14 — EXP-S kind-tagged coexisting-index substrate migration (+ F5 co-land)

- **Status:** RATIFIED (HITL checkpoint approved, decider: coreyt, 2026-07-03).
- **Release:** 0.8.14 — "the schema-migration release" (Substrate & recall: EXP-S #2 + F5 #16).
- **Supersedes/relates:** builds on `ADR-0.8.1-deferred-f5-fielded-fts-bm25f.md` (F5 deferral);
  discharges ledger `TC-1` (OPP-12 projection-registry forward-compat).
- **Plan:** `dev/plans/plan-0.8.14.md`; board `dev/plans/runs/STATUS-0.8.14.md`.

## Context (grounded in code, not architecture)

The portfolio's "one store, many indexes" is **already partly real** — EXP-S is a *delta +
generalization*, not net-new architecture. Verified at baseline (`fathomdb-schema`,
`fathomdb-engine`):

- `SCHEMA_VERSION = 15` (`fathomdb-schema/src/lib.rs:6`); forward-only contiguous `MIGRATIONS`
  applied by `migrate_with_event_sink`; per-step atomic `PRAGMA user_version` bump; new
  tables/columns require a `-- MIGRATION-ACCRETION-EXEMPTION:` marker. Next step = **16**.
- Doc-type `kind` vocabulary is **hard-locked in three sites** — `resolve_source_type`
  (engine:9389), `KIND_TO_SOURCE_TYPE_CASE_SQL` (engine:9257), migration-9 preflight CHECK
  (schema:240) — to `email/article/paper/meeting/note/todo` (+`doc`,`edge_fact`).
- Two-path multi-index write already exists: FTS written **synchronously** in `commit_batch`
  (engine:9999); vector written **asynchronously** by a projection worker pool
  (`PROJECTION_WORKERS=2`) → `commit_projection_outcomes` (engine:8480), serialized by a
  `commit_gate` total order. Durable readiness = `_fathomdb_projection_terminal` +
  `ProjectionStatus{Pending,UpToDate,Failed}`.
- Determinism substrate: `rowid == write_cursor == cursor` identity throughout; `commit_gate`
  total order; mean-pin re-quantize uses DELETE+INSERT to preserve the vec0 BIT subtype.
- eu7 gate: `recall_gate::recall_ci_clears_floor(ci_hi, 0.90)`
  (`fathomdb-engine/tests/support/recall_gate.rs`), AGENT_LONG-gated, re-embeds the corpus.

## Decisions

### D1 — Row-kinds via a SEPARATE structural tag, not the doc-type `kind`

The plan's row-kinds (`leaf`/`coverage`/`graph`) are a **structural-role** axis, orthogonal to
the doc-type `kind` (email/article/…). Add a new `row_kind TEXT NOT NULL DEFAULT 'leaf'` column
(accretion-exempt) rather than overload the hard-locked doc-type vocabulary. `leaf` = normal
record (default preserves back-compat), `coverage` = coverage/summary rows, `graph` = graph
structural rows. Rationale: avoids colliding with the three hard-locked doc-type sites + the
migration-9 CHECK, and keeps doc-type semantics stable for the (out-of-scope) router/eval.
**Taxonomy coordination:** the exact `leaf/coverage/graph` names should be aligned with the
router/M-work design owners before Slice 5 locks them (plan §8); the separate-column decision is
robust to whatever the final names are.

### D2 — Multi-index write = per-kind index-target dispatch

Generalize the write path so each `row_kind` declares **which** indexes it projects into (a
`kind → index-target set` lookup), preserving the existing sync-FTS / async-vector split. Current
behavior becomes the `leaf` default (FTS + optional vector). This is the extensibility that makes
"one store, many indexes" genuine — a new kind slots in by declaring its targets, no bespoke write
code.

### D3 — Determinism check (R-SUB-2)

Write a fixed fixture into two fresh DBs, **flush projections to quiescence**, then serialize each
index (FTS content-rows, vec0 rows, `_fathomdb_vector_rows`, `row_kind` tags, projection-terminal
cursors) and assert **byte-identical**. Leans on the existing `commit_gate` total order +
rowid==cursor identity. The flush is mandatory: the FTS-sync/vector-async split otherwise races the
compare.

### D4 — SCHEMA_VERSION plan (one coordinated migration event)

- **EXP-S** = migration step **16** (`SCHEMA_VERSION` 15→16): adds `row_kind`; old rows default to
  `leaf` (== current behavior). Forward-only.
- **F5** = migration step **17** (16→17): `search_index_v2` multi-column FTS
  (`kind`/`body`/`status`) + per-column BM25F weights, in the **same release**. An old DB migrates
  15→17 in one open — satisfying "pay once" (one release, one re-index window).
- Two clean, independently-revertable steps. If F5 were pulled, only step 16 lands
  (`SCHEMA_VERSION=16`). No vec0 embedding/quant/pooling change (see D6).

### D5 — TC-1 forward-compat (OPP-12 projection registry) — DISCHARGES TC-1

The engine **already owns** the async substrate a future OPP-12 projection registry needs
(projection workers + `_fathomdb_projection_terminal` readiness = the durable equivalent of
`dense_readiness`; per-kind `kind_is_vector_indexed` decision). EXP-S forward-compat obligations:

- (a) Make the D2 per-kind index-target set the **seam** a later declarative projection spec wraps
  — keep it a per-kind lookup, not inline hard-coding.
- (b) Keep FTS/filter writes **same-transaction** and vector writes **async-via-worker** (do NOT
  collapse to sync) — that IS OPP-12's `filterable`/`searchable→FTS` same-txn vs `searchable→vector`
  async split.
- (c) Keep the terminal-cursor readiness **per-kind-extensible** so a later
  `dense_readiness ∈ {ready, embedding}` maps on without a substrate reshape.
Hygiene only — implement **no** OPP-12 surface here. OPP-12 lands ≥0.9.x; re-check at its scheduling.
(Ledger `TC-1`, refs OPP-12 / `projection-registry-and-async-embed.md`.)

### D6 — eu7 gate scoping

EXP-S (`row_kind` + multi-index dispatch) and F5 (multi-column FTS) change **no** embedder, pooling,
or quant → ANN fidelity is mathematically invariant. Slice 20 runs
`recall_gate::recall_ci_clears_floor(ci_hi, 0.90)` as a **regression check only if** a migration step
rewrites vec0 rows (e.g. a vec0 metadata reshape via `ensure_vector_partition`, which DELETE+INSERTs
the *same* embeddings). If no vec0 rewrite occurs, the re-clear is a **documented no-op**. Any breach
(which would indicate an unintended embedding change) is **BLOCK→HITL**.

### D7 — KILL paths

- **EXP-S KILL** (determinism unworkable / substrate collision): revert `row_kind` + multi-index
  dispatch; the engine keeps its current fixed FTS-sync/vector-async paths; router stays agent-side,
  indexes stay eval-side (plan §1).
- **F5 KILL** (retained even though F5 ships per D8): record-and-defer to 0.8.5+ per ADR-0.8.1.

### D8 — F5 ships by conscious HITL override (NOT gate-clearance)

`ADR-0.8.1` gates F5 (`R-F5-2`) on **(a)** the 15b at-power proxy passing **and** **(b)** Slice-20
leaving a measured Mem0 gap. **The pre-registered gate did NOT clear** (verified from the 0.8.3
record): only a **synthetic n=16 smoke** passed (`probe_15b_pass: true`,
`dev/plans/runs/0.8.3-s15b-smoke.json`); the full at-power 15b eligibility run was
**deferred/never ran** (`plan-0.8.3.md:253`, `STATUS-0.8.3.md:156`); 0.8.3 shipped at **marginal
parity via CE-rerank** (`STATUS-0.8.3.md:143-151`), not an F5-closeable gap; Slice 25 (D2/F5 build)
was **NOT RUN** (`STATUS-0.8.3.md:159`).

**Ruling (decider: coreyt, 2026-07-03):** F5 **ships in 0.8.14 by conscious HITL override**,
**decoupled from the R-F5-2 parity gate**, justified by (i) F5's intrinsic recall-lever merit and
(ii) the release's own "pay once" economics — the multi-column FTS schema co-lands while
`SCHEMA_VERSION` is already bumping (16→17). This is **an override, not a gate-pass**: the
pre-registered condition is recorded as NOT met, and the F5 KILL path (D7) is retained. This ADR and
`plan-0.8.14.md` (R-F5-2) both record it as an override so the honesty prior is preserved (no
goalpost moved).

## Consequences

- Slice 5 (keystone) implements D1–D3 + the step-16 migration; Slice 10 implements F5 (step 17);
  Slice 20 runs D6; Slice 25 merges the parked `0.8.14-gpu-rerank`; Slice 40 gates release readiness.
- Cross-binding (Py/TS) parity (X1) must cover `row_kind` + BM25F surfaces.
- Label-only build (manifests stay `0.8.9`; no tag/publish).
