# STATUS — 0.8.16 (Ranking signal & embedder reach: F9 + cross-vendor ONNX)

> Live state board (source of truth = git witnesses per orchestration.md §1.5; this is a cache).
> Plan: `dev/plans/plan-0.8.16.md` · Slice-0 design: `dev/design/0.8.16-slice-0-f9-onnx-design.md` ·
> ADRs: `dev/adr/ADR-0.8.16-f9-importance-confidence-ranking.md`,
> `dev/adr/ADR-0.8.16-onnx-embedder-backend.md`.
> Build: **label-only to date (manifests `0.8.9`)**; the 0.8.16 version-bump / tag / publish is a
> SEPARATE HITL call (fed by the release-readiness verdict below). Push scope: fathomdb-only.

## Current state — **0.8.16 CLOSED / RELEASE-READY (all mod-5 slices closed; 2026-07-08)**
- **Slice 0 (design/ADR) — CLOSED (HITL SIGNED).** DoD frozen; 3 ACs minted; §3 supersession confirmed;
  `NULL`=absent sentinel; OFF-by-default multiplicative-on-fused; step-18 migration authorized.
- **Slice 5 (F9 KEYSTONE) — CLOSED / LANDED (HITL landing SIGNED).** codex §9 PASS after fix-1/fix-2;
  SCHEMA 17→18 (step-18 `canonical_nodes.importance REAL`); F9 OFF-by-default; eu7 no-op basis.
- **Slice 10 (ONNX backend) + Slice 15 (equivalence) — CLOSED / LANDED TOGETHER (standing mandate).**
  9-commit chain `ece15629`..`77b35e0b`; codex §9 PASS after 6 fix rounds; **zero engine diff**;
  `OrtBgeEmbedder` behind non-default `onnx-embedder` feature; `ort =2.0.0-rc.10` (TC-9 for the stable bump);
  offline `safetensors→ONNX` export tooling; equivalence cosine≡1.0 / 1-bit flip-rate 0.0.
- **Slice 40 (Verification + Release Readiness) — CLOSED (Steward authority, standing mandate).** Worktree
  verification §9 PASS (`5a7cb89a`); MAIN-tree compiled-module verification GREEN. **0.8.16 RELEASE-READY.**

## Slice scoreboard
| Slice | Title | State | Base SHA | Branch | codex | Cherry-pick/merge |
|------:|-------|-------|----------|--------|-------|-------------------|
| 0 | Setup + ADR — F9 + OPP-12 `rankable` forward-compat; ONNX design + equivalence plan; board | **CLOSED (HITL SIGNED)** | 36024585 | (docs on main) | n/a | docs commit on main |
| 5 | **F9 KEYSTONE** — `canonical_nodes.importance` step-18 (SCHEMA 17→18) + 3-way sentinel + edge-confidence ranking; `PerHitExplain` | **CLOSED / LANDED** | 61c9e09a | slice-5-…114929Z | **PASS** (fix1→fix2) | `6462b511`+`74987f80`+`3c172131` |
| 10 | **ONNX embedder backend** — `OrtBgeEmbedder` via `EmbedderChoice::Caller`; `onnx-embedder` feature; runtime device select; zero engine diff | **CLOSED / LANDED** | 146eecca | slice-10-…192441Z | **PASS** (fix1→5) | `ece15629`..`dfc0f6ec`+`77b35e0b` |
| 15 | **ONNX equivalence measurement** — candle↔ONNX Δ + 1-bit flip rate; same-backend discipline (feeds 0.8.18 #5) | **CLOSED / LANDED** | 146eecca | slice-10-…192441Z | **PASS** (combined) | `70c2dad6` (in the 10+15 chain) |
| 40 | **Verification + Release Readiness** — X1/X2/X3 + R-F9/R-ONNX AC gate + eu7 gate | **CLOSED** | 616698ea | slice-40-…215328Z | **PASS** | `5a7cb89a` on main |

**Tracks:** F9 track **5** · ONNX track **10 → 15** · Verification **40**. All CLOSED.

## Requirements / AC status (DoD frozen at Slice 0 — ALL GREEN)
| ID | Requirement | State |
|----|-------------|-------|
| R-F9-1 | Importance column (REAL) + 3-way sentinel + edge confidence | ✅ Slice 5 — step-18 (SCHEMA 17→18); round-trip + range-reject + sentinel tested (`f9_importance_ranking.rs`, `step18_migration.rs`) |
| R-F9-2 | Importance/confidence influences ranking AND is observable (`explain`) | ✅ Slice 5 — OFF-by-default reweight reorders; edge confidence real-path e2e; surfaced engine + native + public Py/TS (`test_f9_importance_confidence_survive_the_ffi`, `exp-obs-explain.test.ts`) |
| R-F9-3 | Slice-35 deferred-F9 ADR gate honored (no scope beyond mechanism) | ✅ Slice 5 — §3 supersession; mechanism-only, no eval claim |
| R-F9-4 (minted) | F9 is OPP-12 `rankable`-forward-compatible (Q6a) | ✅ Slice 5 — graceful-neutral identity proven e2e + pure-fn |
| R-ONNX-1 | `OrtBgeEmbedder` produces BGE-small via the trait, within documented Δ | ✅ Slice 10 — real 384-dim vector (`ort_bge_embeds_384_dim_finite_deterministic_vector`); asset-digest identity |
| R-ONNX-2 | Backend selected via config/env, not compile-only | ✅ Slice 10 — runtime `FATHOMDB_EMBED_DEVICE`→ORT provider + loud CPU fallback |
| R-ONNX-3 | candle↔ONNX Δ measured + recorded; same-backend discipline documented | ✅ Slice 15 — **cosine ≡ 1.0, 1-bit sign-flip 0.0 (0/17280), max-abs Δ ≤ 1.86e-7** (45 probes, CPU-vs-CPU); NOT enforced (feeds 0.8.18 #5). Doc: `runs/0.8.16-slice-15-candle-onnx-equivalence.md` |
| R-X-1 | Py + TS SDK parity for the F9 surface (X1) | ✅ Slice 40 — compiled-module F9 explain parity GREEN both bindings (py 24/24, ts 134/134); surface parity `test_surface_parity_py_matches_ts`; no new governed verb (allowlist byte-identical); `tsc --noEmit` rc=0 |
| R-GATE | eu7 ≥ 0.90 (one-sided CI) on any embedder/index change | ✅ Slice 40 — **no-op basis (grounded):** default embedder path (`candle_bge.rs`/`device.rs`) byte-unchanged since `05755e10`; no embed/quant/vec0-write change; reweight OFF-by-default + post-fusion (outside eu7 vector-stage SUT); `default = []` (no `ort`); step-18 = ADD COLUMN (no re-embed). Fidelity gate not triggered (policy `649a8d45`; as 0.8.14 D6) |

## Cross-cutting DoD (X1/X2/X3) — ALL GREEN
| Slice | X1 (SDK parity) | X2 (mkdocs) | X3 (docs+DOC-INDEX) |
|------:|-----------------|-------------|---------------------|
| 0 | n/a | n/a | board + ADRs committed |
| 5 | ✅ (verified at 40) | ✅ (40) | closed |
| 10 | n/a (embedder-internal) | ✅ (40) | ADR §6 + `dev/tools/onnx/README.md` |
| 15 | n/a | ✅ (40) | `runs/0.8.16-slice-15-candle-onnx-equivalence.md` |
| 40 | ✅ compiled-module F9 explain parity (py 24/24 · ts 134/134) + no-new-verb | ✅ `mkdocs build --strict` rc=0 | ✅ DOC-INDEX + 5 new docs |

## Release-readiness verdict (2026-07-08)
**0.8.16 is RELEASE-READY.** All mod-5 slices (0·5·10·15·40) CLOSED. X1/X2/X3 green. Full AC gate green
(R-F9-1/2/3/4, R-ONNX-1/2/3, 3 minted ACs). eu7 R-GATE satisfied on the grounded no-op basis. Full-workspace
`cargo clippy/check/test --workspace --all-targets` rc=0; MAIN-tree pyo3 + napi builds rc=0; compiled-module
F9 explain parity GREEN both bindings. Zero engine diff for ONNX; footprint invariant intact (default build
gains no `ort`).

## Publish-scope (SEPARATE HITL decision — fed by this verdict, NOT decided here)
- **Manifests remain `0.8.9` (label-only to date).** The 0.8.16 version-bump / `v*` tag / crates+PyPI+npm
  publish is the HITL's separate call. 0.8.16 is an even (publishable) micro.
- **`ort =2.0.0-rc.10`** — RC dep behind the opt-in `onnx-embedder` feature (default build unaffected);
  the 2.0-stable bump is tracked as **TC-9**.
- **No user-facing `docs/` nav pages** for F9/ONNX (F9 is OFF-by-default mechanism; ONNX is opt-in EVAL) —
  a publish-scope call; DOC-INDEX (X3) is satisfied.
- **0.8.18 #5 follow-up:** R-ONNX-3's Δ is same-arch (ONNX-CPU vs candle-CPU) = 0 flips; the **cross-backend**
  (GPU ONNX EP vs CPU) divergence is the real 0.8.18 #5 target and is NOT yet measured (no GPU ONNX EP asset).

## Hard gates (all satisfied)
- eu7 ≥ 0.90 one-sided CI — no-op basis (grounded). Full-workspace clippy+check both 0. codex §9 on every
  slice. step-18 migration LANDED under HITL sign-off (Slice 5). R-ONNX-3 feed-forward recorded.

## Outstanding worktrees
- None (all slice worktrees cleaned after landing; throwaway `/tmp/onnx-export-venv` removed; preflight pass).

## Recent decisions (newest first)
- 2026-07-08 — **Slice 40 CLOSED → 0.8.16 RELEASE-READY (Steward authority, standing mandate).** Worktree
  §9 PASS (`5a7cb89a`, 3 files); MAIN-tree verification GREEN (pyo3+napi builds rc=0; compiled-module F9
  explain parity py 24/24 · ts 134/134; surface parity; `tsc --noEmit` 0; `mkdocs --strict` 0). eu7 no-op
  basis grounded (default vector path byte-unchanged since `05755e10`). Full AC gate green. Publish = separate
  HITL call.
- 2026-07-08 — **Slices 10 + 15 CLOSED / LANDED TOGETHER.** 9-commit chain; codex §9 PASS; zero engine diff;
  equivalence cosine≡1.0 / flip-rate 0.0; cross-backend Δ → 0.8.18 #5.
- 2026-07-08 — **Slice 5 (F9 KEYSTONE) CLOSED / LANDED.** SCHEMA 17→18; F9 OFF-by-default; eu7 no-op basis.
- 2026-07-08 — **Slice-0 CLOSED (HITL SIGNED).** 5 gate decisions; 3 ACs minted; step-18 authorized.

## Next action
**0.8.16 is CLOSED / RELEASE-READY.** No further slices. The version-bump / tag / publish is the HITL's
separate decision (Steward relaying the verdict). Steward to reconcile 0.8.16 into the master schedule.
