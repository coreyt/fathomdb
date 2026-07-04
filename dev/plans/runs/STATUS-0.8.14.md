# STATUS — 0.8.14 (Substrate & recall: EXP-S + F5) · the schema-migration release

> Live state board (source of truth = git witnesses per orchestration.md §1.5; this is a cache).
> Plan: `dev/plans/plan-0.8.14.md` · ADR: `dev/adr/ADR-0.8.14-exp-s-kind-tagged-coexisting-index-substrate.md`.
> Build: **label-only** (manifests stay `0.8.9`; NO `v*` tag, NO publish). Push scope: fathomdb-only.

## Current state
- **Slice 0 (ADR) — CLOSED** (2026-07-03). HITL approved D1–D8; F5 per Option C; TC-1 discharged.
- **Slice 5 (EXP-S keystone) — CLOSED** (2026-07-04). Cherry-picked `ba15e176`+`718cfe94` to `main`;
  codex §9 **PASS** (no findings); full-workspace gate both exit 0; SCHEMA_VERSION 15→16; D6 no vec0 rewrite.
- **Slice 10 (F5 BM25F) — CLOSED** (2026-07-04). Cherry-picked `b145754f`+`c57e4e99`+`9d8e368b`+`a7c3c145`;
  codex §9 CONCERN→fix-1→CONCERN→fix-2→land; substantive tokenization finding resolved; gate both exit 0;
  SCHEMA_VERSION 16→17; ships per D8 Option-C override; in-engine BM25F (justified ADR-0.8.1 deviation for tunable b).
- **Slice 25 (gpu-rerank merge) — CLOSED** (2026-07-04). `3c98b35b`+`813e525a`+`9187de26`+`e311aadf` on
  origin/main; codex §9 BLOCK→fix-1→PASS (finding-1 refuted empirically; findings 2+3 fixed). Rebased clean
  onto `ce8e1eef`; opt-in `rerank-cuda`/`FATHOMDB_RERANK_DEVICE` (default `[]`), default-CPU unchanged.
  MAIN-tree maturin build OK + `embed_batch_cls` importable; full-workspace gate 0/0; agent-security PASS.
- **Next:** Slice 20 (eu7 no-op regression per D6 + v15→v17 migration verify) — off a fresh `origin/main` baseline.

## Slice scoreboard
| Slice | Title | State | Base SHA | Branch | output.json | codex | Cherry-pick/merge |
|------:|-------|-------|----------|--------|-------------|-------|-------------------|
| 0 | Setup + ADR (row-kinds, determinism, KILL, SCHEMA_VERSION, TC-1, F5 ruling) | **CLOSED** | 0344a343 (ADR authored on main d7cad699) | (docs on main) | n/a (design slice) | n/a | docs commit on main |
| 5 | **EXP-S KEYSTONE** — row_kind + per-kind coexisting-index write + determinism check + `SCHEMA_VERSION` 15→16 | **CLOSED** | dff4830c | slice-5-…235950Z | ✅ | **PASS** (no findings) | `ba15e176`+`718cfe94` on main |
| 10 | **F5 fielded BM25F** — `search_index_v2` + in-engine BM25F (tunable weights/`b`), `SCHEMA_VERSION` 16→17 | **CLOSED** | be37dffd | slice-10-…002826Z | ✅ | CONCERN→fix1→CONCERN→fix2→**resolved** | `b145754f`+`c57e4e99`+`9d8e368b`+`a7c3c145` |
| 15 | *(void reserved gap — #17 shipped 0.8.11)* | VOID | — | — | — | — | — |
| 20 | eu7 re-clear + migration verify (D6) | not-started | — | — | — | — | — |
| 25 | *(reserved gap)* Merge `0.8.14-gpu-rerank` (opt-in GPU CE + `embed_batch_cls`, default-CPU-unchanged) | **CLOSED** | ce8e1eef | slice-25-…033943Z | ✅ | BLOCK→fix1→**PASS** | `3c98b35b`+`813e525a`+`9187de26`+`e311aadf` on main |
| 40 | Verification + Release Readiness (X1/X2/X3 + R-SUB/R-F5 AC gate + eu7 gate) | not-started | — | — | — | — | — |

## Requirements / AC status (DoD frozen at Slice 0)
| ID | Requirement | State |
|----|-------------|-------|
| R-SUB-1 | Row-kinds coexist in one store | ✅ Slice 5 (GREEN) |
| R-SUB-2 | Incremental multi-index write deterministic | ✅ Slice 5 (GREEN, non-vacuous) |
| R-SUB-3 | Migration forward-only + guarded (`SCHEMA_VERSION` bump) | ✅ step-16 (Slice 5) + step-17 (Slice 10); v15→v17 verify @ Slice 20 |
| R-F5-1 | Fielded BM25F, tunable `b`/field weights | ✅ Slice 10 (GREEN; tokenization-faithful) |
| R-F5-2 | F5 ships per HITL Option-C override (gate did NOT clear) | ✅ ruled (ADR §D8) — ships as override |
| R-X-1 | Py+TS SDK parity for EXP-S + F5 (X1) | ⏳ per slice (Slice 25 added `embed_batch_cls` py-only + `.pyi` stub) |
| R-GATE | eu7 ANN fidelity ≥ 0.90 (one-sided CI) after any re-embed | ⏳ Slice 20 (no-op unless vec0 rewritten, D6) |

## Hard gates
- **eu7 ≥ 0.90 one-sided CI** — BLOCK→HITL at Slice 20 if any re-embed/vec0 rewrite occurs (D6).
- **Full-workspace clippy+check** — `cargo clippy --workspace --all-targets` AND
  `cargo check --workspace --all-targets`, both exit 0, before ANY green claim.
- **codex §9** review gate on every slice's output.json.
- **SCHEMA_VERSION migration** = engine/schema migration → HITL-gated; ADR ratifies the plan.

## Outstanding worktrees
- None open (Slice-0 + Slice-5 worktrees removed after close).

## Concurrency
- **Library Sweep #2** runs on its own branch (only `.github/workflows/*` + JS/TS lockfiles) —
  disjoint from engine `src/`/`Cargo.lock`. Expect `main` to advance; rebase is trivial.

## Recent decisions (newest first)
- 2026-07-04 — Slice 10 (F5) CLOSED: 4 commits on main, codex CONCERN→fix1→CONCERN→fix2→resolved, gate green,
  SCHEMA_VERSION 16→17; in-engine BM25F (justified ADR-0.8.1 deviation for tunable b); D6 no vector touch.
- 2026-07-04 — Slice 5 (EXP-S keystone) CLOSED: `ba15e176`+`718cfe94` on main, codex §9 PASS, gate green,
  SCHEMA_VERSION 15→16, D6 no vec0 rewrite (eu7@20 = no-op).
- 2026-07-03 — Slice-0 ADR ratified; D8=Option C (F5 override); Slice 25 added; TC-1 discharged.

## Repo-health flags (pre-existing; NOT Slice-25-caused; surfaced by running agent-verify at merge)
- **md-lint mis-scoped:** `agent-lint-md` (markdownlint-cli2) scans the whole tree incl. `typescript/node_modules`,
  gitignored `data/corpus-data/**`, and `scripts/repo-prune/backups/**` → `agent-verify` fails at `lint`. Tooling fix (scope excludes).
- **pyright:** 1 pre-existing error `src/python/eval/exp_cov1_sweep.py:377` (`cache_file` possibly unbound).
- **release.yml test:** `publish-rust-t1-embedder-api dry-run` structure + actionlint-fixture fail — from the concurrent
  Library-Sweep/napi-3 workflow changes (Slice 25 touched no `.github/workflows`). Echoes the 0.8.9 embedder-api publish drift.
- These are flagged to the Steward as repo-health items (own fix/consideration), not Slice-25 blockers.

## Next action
Cut Slice-20 (eu7/migration verify) worktree off fresh `origin/main` → preflight `--expect-closed 10` →
run eu7 as a documented no-op regression (D6) + v15→v17 migration test → codex §9 → land → 25 (gpu-rerank) → 40.
