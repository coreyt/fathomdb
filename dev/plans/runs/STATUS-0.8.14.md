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
- **Slice 20 (eu7 re-clear + v15→v17 migration verify) — CLOSED** (2026-07-05, D6 no-op basis; HITL-ruled (A)).
  Migration half landed `52f29fb9` (codex §9 PASS, R-SUB-3). eu7 half **closed on the D6 no-op basis**: D6
  (codex-verified @ Slice 5 — zero vec0 rewrite) conclusively excludes a 0.8.14 fidelity regression (R-GATE is
  conditional on a re-embed; none occurred). Empirical GPU run (mis-directed cross-backend, self-corrected in policy
  `649a8d45`): N=1000 PASS (0.950, ci[0.930,0.967]); N=7667 sub-floor (0.833, ci_hi 0.864) — a cross-backend
  quant-flip + corpus-growth artifact, NOT a 0.8.14 regression. Canonical eu7 = CPU same-backend (baseline 0.896);
  floor re-baseline for the grown 18,472-doc corpus → **TC-5**.
- **Next:** Slice 40 (release readiness) — off `649a8d45`.

## Slice scoreboard
| Slice | Title | State | Base SHA | Branch | output.json | codex | Cherry-pick/merge |
|------:|-------|-------|----------|--------|-------------|-------|-------------------|
| 0 | Setup + ADR (row-kinds, determinism, KILL, SCHEMA_VERSION, TC-1, F5 ruling) | **CLOSED** | 0344a343 (ADR authored on main d7cad699) | (docs on main) | n/a (design slice) | n/a | docs commit on main |
| 5 | **EXP-S KEYSTONE** — row_kind + per-kind coexisting-index write + determinism check + `SCHEMA_VERSION` 15→16 | **CLOSED** | dff4830c | slice-5-…235950Z | ✅ | **PASS** (no findings) | `ba15e176`+`718cfe94` on main |
| 10 | **F5 fielded BM25F** — `search_index_v2` + in-engine BM25F (tunable weights/`b`), `SCHEMA_VERSION` 16→17 | **CLOSED** | be37dffd | slice-10-…002826Z | ✅ | CONCERN→fix1→CONCERN→fix2→**resolved** | `b145754f`+`c57e4e99`+`9d8e368b`+`a7c3c145` |
| 15 | *(void reserved gap — #17 shipped 0.8.11)* | VOID | — | — | — | — | — |
| 20 | eu7 re-clear + v15→v17 migration verify (D6) | **CLOSED** | 4ca170a2 (mig) · 1c5ba1b6 (eu7) | slice-20-…121320Z (mig) | ✅ (mig) | **PASS** (mig §9) | mig `52f29fb9` on main; eu7 half **D6 no-op close** (HITL) |
| 25 | *(reserved gap)* Merge `0.8.14-gpu-rerank` (opt-in GPU CE + `embed_batch_cls`, default-CPU-unchanged) | **CLOSED** | ce8e1eef | slice-25-…033943Z | ✅ | BLOCK→fix1→**PASS** | `3c98b35b`+`813e525a`+`9187de26`+`e311aadf` on main |
| 40 | Verification + Release Readiness (X1/X2/X3 + R-SUB/R-F5 AC gate + eu7 gate) | not-started | — | — | — | — | — |

## Requirements / AC status (DoD frozen at Slice 0)
| ID | Requirement | State |
|----|-------------|-------|
| R-SUB-1 | Row-kinds coexist in one store | ✅ Slice 5 (GREEN) |
| R-SUB-2 | Incremental multi-index write deterministic | ✅ Slice 5 (GREEN, non-vacuous) |
| R-SUB-3 | Migration forward-only + guarded (`SCHEMA_VERSION` bump) | ✅ step-16 (Slice 5) + step-17 (Slice 10); v15→v17 full-path verify landed `52f29fb9` (Slice 20, codex PASS) |
| R-F5-1 | Fielded BM25F, tunable `b`/field weights | ✅ Slice 10 (GREEN; tokenization-faithful) |
| R-F5-2 | F5 ships per HITL Option-C override (gate did NOT clear) | ✅ ruled (ADR §D8) — ships as override |
| R-X-1 | Py+TS SDK parity for EXP-S + F5 (X1) | ✅ Slice 40 — EXP-S (`row_kind`) + F5 (`search_index_v2`/BM25F) are **engine-internal**: grep confirms they appear ONLY in `fathomdb-engine`/`fathomdb-schema` (Rust), NEVER in the Py/TS SDK bindings or the governed-surface allowlist → **NO new SDK verbs** (non-vacuously green). Py surface 16✅ / TS surface 131✅ (both read the one shared `governed-surface-allowlist.json`). **KNOWN py-first deferral (intentional, tracked):** the module-level embedder helper `embed_batch_cls` is Python-only (`fathomdb-py` + `__init__.__all__` + `_fathomdb.pyi`; ABSENT from `fathomdb-napi`/TS) — the parity harness is blind to module-level functions, so Slice 40 added `test_module_level_embedder_helper_asymmetry_is_tracked` to ASSERT the py-only set (was a silent blind spot). `Engine.embed` verb IS at Py↔TS parity. TS `embedBatchCls` binding = out-of-scope deferral |
| R-GATE | eu7 ANN fidelity ≥ 0.90 (one-sided CI) after any re-embed | ✅ Slice 20 — **satisfied on D6 no-op basis** (zero vec0 rewrite ⇒ no re-embed ⇒ gate not triggered; HITL-ruled). GPU run sub-floor @ N=7667 (0.833) = cross-backend+corpus artifact, not a regression; floor re-baseline → TC-5 |

## Hard gates
- **eu7 ≥ 0.90 one-sided CI** — Slice 20 CLOSED on D6 no-op basis (no re-embed occurred). The eu7 fidelity gate MUST run **CPU same-backend** (policy `649a8d45`); a GPU run cross-backends it. Floor re-baseline for the grown corpus → TC-5.
- **Full-workspace clippy+check** — `cargo clippy --workspace --all-targets` AND
  `cargo check --workspace --all-targets`, both exit 0, before ANY green claim.
- **codex §9** review gate on every slice's output.json.
- **SCHEMA_VERSION migration** = engine/schema migration → HITL-gated; ADR ratifies the plan.

## Outstanding worktrees
- None open (dead-run `slice-20-…121320Z` + `0.8.12-gpu-rerank` cleaned; eu7-verify worktree `slice-20-eu7-…195012Z` removed after the D6-no-op close).

## Concurrency
- **Library Sweep #2** runs on its own branch (only `.github/workflows/*` + JS/TS lockfiles) —
  disjoint from engine `src/`/`Cargo.lock`. Expect `main` to advance; rebase is trivial.

## Recent decisions (newest first)
- 2026-07-05 — **Slice 20 CLOSED (D6 no-op basis; HITL-ruled (A)).** Migration half `52f29fb9` (codex PASS, R-SUB-3).
  eu7 half: D6 (codex-verified zero vec0 rewrite @ Slice 5) excludes any 0.8.14 fidelity regression (R-GATE is
  conditional on a re-embed; none occurred). Empirical GPU eu7 (mis-directed cross-backend): N=1000 PASS 0.950;
  N=7667 sub-floor 0.833 (ci_hi 0.864) = cross-backend quant-flip + corpus-growth artifact. eu7 fidelity gate →
  CPU same-backend policy `649a8d45`; floor re-baseline → TC-5. Evidence:
  `runs/0.8.14-slice-20-eu7-gpu-run-20260705T205222Z.log`.
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
Cut Slice-40 (release readiness) worktree off `649a8d45` → preflight `--expect-closed 20` → X1/X2/X3 cross-binding
parity + R-SUB/R-F5 AC gate + eu7 gate (satisfied D6 no-op) + resolve-or-document the pre-existing agent-verify reds
→ codex §9 → land → close 0.8.14 (label-only).
