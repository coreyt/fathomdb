# STATUS — 0.8.16 (Ranking signal & embedder reach: F9 + cross-vendor ONNX)

> Live state board (source of truth = git witnesses per orchestration.md §1.5; this is a cache).
> Plan: `dev/plans/plan-0.8.16.md` · Slice-0 design: `dev/design/0.8.16-slice-0-f9-onnx-design.md` ·
> ADRs: `dev/adr/ADR-0.8.16-f9-importance-confidence-ranking.md`,
> `dev/adr/ADR-0.8.16-onnx-embedder-backend.md`.
> Build: **TBD at Slice 40** (label-only unless HITL rules a publishable even micro). Push scope: fathomdb-only.
> Commission tip (live `main`) = `36024585`. Cut every worktree from `$(git rev-parse origin/main)`.

## Current state
- **Slice 0 (design/ADR) — CLOSED (HITL SIGNED 2026-07-08).** DoD frozen; 3 ACs minted; §3 supersession
  confirmed; `NULL`=absent sentinel; OFF-by-default multiplicative-on-fused; step-18 migration authorized.
- **Slice 5 (F9 KEYSTONE) — CLOSED / LANDED (HITL landing SIGNED 2026-07-08).** codex §9 PASS after
  fix-1/fix-2; SCHEMA 17→18 (step-18 `canonical_nodes.importance REAL`); F9 OFF-by-default; eu7 no-op basis.
- **Slice 10 (ONNX backend) + Slice 15 (equivalence) — CLOSED / LANDED TOGETHER (Steward authority, standing
  mandate, 2026-07-08).** 9-commit chain `ece15629`..`77b35e0b` cherry-picked onto `main` (base `146eecca`,
  clean; `.gitignore` 3-way auto-merged keeping both `/output.json` + `*.onnx`; `cargo check --workspace` rc=0;
  **zero engine diff**). codex §9 **PASS** after 6 fix rounds (5 on Slice 10: loud CPU fallback → CPU retry →
  `error_on_failure` root → untrack witness → asset-digest identity; 1 combined: export fail-closed on the
  pinned revision). `OrtBgeEmbedder` behind non-default `onnx-embedder` feature via `EmbedderChoice::Caller`;
  `ort =2.0.0-rc.10` (HITL-accepted; 2.0-stable = TC-9). Offline `safetensors→ONNX` export tooling committed;
  `.onnx` gitignored; ORT offline via on-host `libonnxruntime.so.1.26.0`. Footprint invariant intact.

## Slice scoreboard
| Slice | Title | State | Base SHA | Branch | output.json | codex | Cherry-pick/merge |
|------:|-------|-------|----------|--------|-------------|-------|-------------------|
| 0 | Setup + ADR — F9 ranking/lifecycle + OPP-12 `rankable` forward-compat; ONNX-backend design + equivalence plan; board | **CLOSED (HITL SIGNED)** | 36024585 | (docs on main) | n/a | n/a | docs commit on main |
| 5 | **F9 KEYSTONE** — `canonical_nodes.importance` step-18 (SCHEMA 17→18) + 3-way sentinel + edge-confidence ranking; `PerHitExplain` | **CLOSED / LANDED** | 61c9e09a | slice-5-…114929Z | ✅ | **PASS** (fix1→fix2) | `6462b511`+`74987f80`+`3c172131` |
| 10 | **ONNX embedder backend** — `OrtBgeEmbedder` behind trait via `EmbedderChoice::Caller`; `onnx-embedder` feature; runtime device select; zero engine diff | **CLOSED / LANDED** | 146eecca | slice-10-…192441Z | ✅ | **PASS** (fix1→5) | `ece15629`+`994a8bf3`+`8b4b4622`+`53c7aabf`+`31f8ca06`+`eaa8851a`+`dfc0f6ec`+`77b35e0b` |
| 15 | **ONNX equivalence measurement** — candle↔ONNX Δ + 1-bit flip rate; same-backend discipline (feeds 0.8.18 #5) | **CLOSED / LANDED** | 146eecca | slice-10-…192441Z | ✅ | **PASS** (combined) | `70c2dad6` (in the 10+15 chain) |
| 40 | **Verification + Release Readiness** — X1/X2/X3 + R-F9/R-ONNX AC gate + eu7 gate | NOT STARTED (dep: 5,10,15) — **NEXT / ONLY REMAINING** | — | — | — | — | — |

**Tracks:** F9 track **5** (DONE) · ONNX track **10 → 15** (DONE). Only **Slice 40** remains.

## Requirements / AC status (DoD frozen at Slice 0)
| ID | Requirement | State |
|----|-------------|-------|
| R-F9-1 | Importance column (REAL) + 3-way sentinel + edge confidence | ✅ Slice 5 |
| R-F9-2 | Importance/confidence influences ranking AND is observable (`explain`) | ✅ Slice 5 (edge confidence real-path e2e; surfaced engine + native + public Py/TS) |
| R-F9-3 | Slice-35 deferred-F9 ADR gate honored (no scope beyond mechanism) | ✅ Slice 5 (§3 supersession; mechanism-only) |
| R-F9-4 (minted) | F9 is OPP-12 `rankable`-forward-compatible (Q6a) | ✅ Slice 5 (graceful-neutral identity proven) |
| R-ONNX-1 | `OrtBgeEmbedder` produces BGE-small via the trait, within documented Δ | ✅ Slice 10 — real 384-dim finite deterministic L2-normed vector on ONNX CPU EP; asset-digest identity |
| R-ONNX-2 | Backend selected at `Engine::open` via config/env, not compile-only | ✅ Slice 10 — runtime `FATHOMDB_EMBED_DEVICE`→ORT provider (CPU/CUDA/ROCm/DirectML/OpenVINO), loud CPU fallback |
| R-ONNX-3 | candle↔ONNX Δ measured + recorded; same-backend discipline documented | ✅ Slice 15 — **cosine ≡ 1.0, 1-bit sign-flip rate = 0.0 (0/17280), max-abs Δ ≤ 1.86e-7** (45 probes, candle-CPU vs ONNX-CPU). Doc: `dev/plans/runs/0.8.16-slice-15-candle-onnx-equivalence.md`. **NOT enforced** (feeds 0.8.18 #5) |
| R-X-1 | Py + TS SDK parity for the F9 surface (X1) | ◑ Slice 5 wired native+public wrappers; compiled-module e2e parity = Slice-40 MAIN-tree gate |
| R-GATE | eu7 ≥ 0.90 (one-sided CI) on any embedder/index change | ✅ no-op basis (F9 no re-embed; ONNX default candle path unchanged); full verify at Slice 40 |

## Cross-cutting DoD (X1/X2/X3 — bind every slice)
| Slice | X1 (SDK parity) | X2 (mkdocs) | X3 (docs+DOC-INDEX) |
|------:|-----------------|-------------|---------------------|
| 0 | n/a | n/a | board + ADRs committed |
| 5 | ◑ wrappers wired; e2e → Slice 40 | ⏳ Slice 40 | closed |
| 10 | n/a (embedder crate-internal; no SDK verb) — verify at 40 | ⏳ Slice 40 | ADR §6 + `dev/tools/onnx/README.md` |
| 15 | n/a | ⏳ Slice 40 | `dev/plans/runs/0.8.16-slice-15-candle-onnx-equivalence.md` |
| 40 | ⏳ | ⏳ | ⏳ |

## Hard gates
- **eu7 ≥ 0.90 one-sided CI** — CPU same-backend (policy `649a8d45`). Default paths no-op; full verify at 40.
- **Full-workspace clippy+check** — both exit 0 before any green claim.
- **codex §9** review gate on every slice (adversarial-subagent fallback only if codex down).
- **step-18 migration** LANDED under HITL sign-off (Slice 5). Slices 10/15 = no-migration → landed under
  Steward authority on a clean §9 PASS (standing mandate). Slice 40 (verification) same lane.
- **R-ONNX-3 feed-forward** — Slice-15 Δ calibrates 0.8.18 #5; recorded (see below).

## ACs minted (Slice-0 gate, HITL 2026-07-08)
1. **R-F9-4** — `rankable`-forward-compatible (Q6a). ✅ Slice 5.
2. **F9 ranking-contract** — OFF-by-default multiplicative-on-fused; 3-way sentinel. ✅ Slice 5.
3. **ONNX-equivalence** — R-ONNX-1 documented Δ + R-ONNX-3 recorded flip rate. ✅ Slices 10/15.

## 0.8.18 #5 follow-up (recorded, NOT a 0.8.16 gap)
R-ONNX-3's measured Δ is **ONNX-CPU vs candle-CPU (same arch)** — numerically identical to 1-bit (0 flips).
The **cross-backend** divergence (GPU ONNX EP vs CPU) is the real 0.8.18 #5 target and is **not yet
measured** (no GPU ONNX EP asset provisioned). Explicit 0.8.18 #5 follow-up; treat any non-trivial future
flip rate as an export/backend regression, not benign.

## Outstanding worktrees
- None open (`slice-5-…` and `slice-10-…192441Z` cleaned after landing; throwaway `/tmp/onnx-export-venv`
  removed; preflight re-run pass).

## Recent decisions (newest first)
- 2026-07-08 — **Slices 10 + 15 CLOSED / LANDED TOGETHER (Steward authority, standing mandate).** 9-commit
  chain `ece15629`..`77b35e0b`; codex §9 PASS after 6 fix rounds; zero engine diff; footprint invariant
  intact; `ort =2.0.0-rc.10` (TC-9 for the stable bump). R-ONNX-1/2/3 green; equivalence cosine≡1.0 /
  flip-rate 0.0. Model offline (`local_files_only`, pinned rev, fail-closed); export TOOLCHAIN pip-installed
  into a throwaway venv (network for the Python toolchain only, NOT the weights). Cross-backend Δ → 0.8.18 #5.
- 2026-07-08 — **Slice 5 (F9 KEYSTONE) CLOSED / LANDED (HITL landing SIGNED).** codex §9 fix1→fix2→PASS;
  SCHEMA 17→18; F9 OFF-by-default; eu7 no-op basis.
- 2026-07-08 — **Slice-0 CLOSED (HITL SIGNED).** 5 gate decisions; 3 ACs minted; step-18 authorized.
- 2026-07-07 — **Slice-0 package DRAFTED.**

## Next action
**Slice 40 — Verification + Release Readiness (the ONLY remaining slice).** X1 (incl. the deferred
compiled-module e2e SDK parity for F9 explain on the MAIN tree), X2 `mkdocs build --strict`, X3 DOC-INDEX,
the R-F9 + R-ONNX AC gate, and the eu7 gate. Steward to commission/scope.
