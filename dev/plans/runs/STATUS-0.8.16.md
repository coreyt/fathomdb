# STATUS — 0.8.16 (Ranking signal & embedder reach: F9 + cross-vendor ONNX)

> Live state board (source of truth = git witnesses per orchestration.md §1.5; this is a cache).
> Plan: `dev/plans/plan-0.8.16.md` · Slice-0 design: `dev/design/0.8.16-slice-0-f9-onnx-design.md` ·
> ADRs: `dev/adr/ADR-0.8.16-f9-importance-confidence-ranking.md`,
> `dev/adr/ADR-0.8.16-onnx-embedder-backend.md`.
> Build: **TBD at Slice 40** (label-only unless HITL rules a publishable even micro). Push scope: fathomdb-only.
> Commission tip (live `main`) = `36024585`. Cut every worktree from `$(git rev-parse origin/main)`.

## Current state
- **Slice 0 (design/ADR) — CLOSED (HITL coreyt SIGNED 2026-07-08 — all 5 gate decisions ratified).** DoD
  frozen; 3 ACs minted (R-F9-4 + F9 ranking-contract + ONNX-equivalence); §3 supersession confirmed;
  sentinel `NULL`=absent; OFF-by-default multiplicative-on-fused weighting; step-18 migration (SCHEMA 17→18)
  authorized. Docs committed on `main`. **Slices 5 ∥ 10 RELEASED** (canary-first; worktrees off live `main`).
- **Slice 5 (F9 KEYSTONE) — CLOSED (HITL landing SIGNED 2026-07-08).** Canary. Cherry-picked
  `6462b511`+`74987f80`+`3c172131` onto `main` (base `61c9e09a`, clean; `cargo check --workspace` rc=0 on
  main). codex §9 **PASS** after CONCERN→fix-1→CONCERN→fix-2 (both real: edge-confidence-on-graph-arm
  correctness + F9 explain propagation through native bindings AND public Py/TS wrappers). SCHEMA_VERSION
  17→18 (step-18 `canonical_nodes.importance REAL`); F9 ships OFF-by-default (no eval claim); eu7 no-op
  basis (pure ADD COLUMN, no vector rewrite). Worktree cleaned; preflight re-run pass.

## Slice scoreboard
| Slice | Title | State | Base SHA | Branch | output.json | codex | Cherry-pick/merge |
|------:|-------|-------|----------|--------|-------------|-------|-------------------|
| 0 | Setup + ADR — F9 ranking/lifecycle (honor Slice-35 ADR) + OPP-12 `rankable` forward-compat; ONNX-backend design + equivalence-measurement plan; stand up this board | **CLOSED (HITL SIGNED 2026-07-08)** | 36024585 | (docs on main) | n/a (design slice) | n/a | docs commit on main |
| 5 | **F9 importance/confidence KEYSTONE** — `canonical_nodes.importance` step-18 (SCHEMA 17→18) + 3-way sentinel + edge-confidence ranking; surfaced via `PerHitExplain` | **CLOSED** (2026-07-08) | 61c9e09a | slice-5-20260708T114929Z | ✅ | **PASS** (CONCERN→fix1→CONCERN→fix2→PASS) | `6462b511`+`74987f80`+`3c172131` on main |
| 10 | **ONNX embedder backend** — `OrtBgeEmbedder` behind the trait via `EmbedderChoice::Caller`; `onnx-embedder` feature; config/env device select; zero engine diff | NOT STARTED (dep: 0) — **NEXT** | — | — | — | — | — |
| 15 | **ONNX equivalence measurement** — candle↔ONNX numeric Δ + 1-bit flip rate on a probe set; document interim same-backend discipline (feeds 0.8.18 #5) | NOT STARTED (dep: 10) | — | — | — | — | — |
| 40 | **Verification + Release Readiness** — X1/X2/X3 + R-F9/R-ONNX AC gate + eu7 gate | NOT STARTED (dep: 5,10,15) | — | — | — | — | — |

**Tracks (parallelizable):** F9 track **5** (DONE) ∥ ONNX track **10 → 15**. Canary (Slice 5) proved the
loop. Max 3 concurrent worktrees; per-worktree distinct `TMPDIR` before any parallelism (ac_002/t_s34 flakes).

## Requirements / AC status (DoD frozen at Slice 0)
| ID | Requirement | State |
|----|-------------|-------|
| R-F9-1 | Importance column (REAL) + 3-way sentinel + edge confidence | ✅ Slice 5 — step-18 (SCHEMA 17→18); round-trip {NULL,0.0,0.5,1.0}; out-of-[0,1] rejected; 3-way sentinel tested |
| R-F9-2 | Importance/confidence influences ranking AND is observable (`explain`) | ✅ Slice 5 — OFF-by-default reweight reorders vs OFF; edge confidence applied on graph-arm hits (real-path e2e); `PerHitExplain` importance/confidence surfaced through engine + native bindings + public Py/TS wrappers |
| R-F9-3 | Slice-35 deferred-F9 ADR gate honored (no scope beyond mechanism) | ✅ §3 supersession CONFIRMED (HITL 2026-07-08); mechanism-only, no eval claim |
| R-F9-4 (minted) | F9 is OPP-12 `rankable`-forward-compatible (Q6a) | ✅ Slice 5 — graceful-neutral identity (all-NULL reweight-ON == OFF) proven e2e + pure-fn; additive REAL, opt-in add/drop-idempotent, no break-if-later field |
| R-ONNX-1 | `OrtBgeEmbedder` produces BGE-small via the trait, within documented Δ | ⏳ Slice 10 |
| R-ONNX-2 | Backend selected at `Engine::open` via config/env, not compile-only | ⏳ Slice 10 |
| R-ONNX-3 | candle↔ONNX Δ measured + recorded; same-backend discipline documented | ⏳ Slice 15 (feeds 0.8.18 #5) |
| R-X-1 | Py + TS SDK parity for the F9 surface (X1) | ◑ Slice 5 wired native+public wrappers (Py test 3/3; TS mapping test authored); **compiled-module e2e parity = Slice-40 MAIN-tree gate** |
| R-GATE | eu7 ≥ 0.90 (one-sided CI) on any embedder/index change | ✅ Slice 5 no-op basis (pure ADD COLUMN, no re-embed); full verify at Slice 40 |

## Cross-cutting DoD (X1/X2/X3 — bind every slice)
X1 SDK parity + harnesses · X2 `mkdocs build --strict` green · X3 docs + DOC-INDEX per slice.
| Slice | X1 (SDK parity) | X2 (mkdocs) | X3 (docs+DOC-INDEX) |
|------:|-----------------|-------------|---------------------|
| 0 | n/a (design) | n/a | board + ADRs committed |
| 5 | ◑ native+public wrappers wired (Py/TS); e2e parity → Slice 40 | ⏳ Slice 40 | closure in this docs commit |
| 10 | ⏳ | ⏳ | ⏳ |
| 15 | ⏳ | ⏳ | ⏳ |
| 40 | ⏳ | ⏳ | ⏳ |

## Hard gates
- **eu7 ≥ 0.90 one-sided CI** on any embedder/index touch — runs **CPU same-backend** (policy `649a8d45`).
  Default paths (F9 no re-embed; ONNX behind the trait, default candle CPU unchanged) are **no-op**; breach
  BLOCKS→HITL.
- **Full-workspace clippy+check** — `cargo clippy --workspace --all-targets` AND `cargo check --workspace
  --all-targets`, both exit 0, before ANY green claim.
- **codex §9** review gate on every slice's output.json (adversarial-subagent fallback only if codex down).
- **SCHEMA_VERSION migration (step-18)** = engine/schema migration → HITL-gated (LANDED under HITL sign-off
  2026-07-08). Future 0.8.16 slices (10/15/40) are expected no-migration → land under Steward authority on
  a clean §9 PASS (standing landing mandate); escalate if any unexpectedly needs a migration/override/BLOCK.
- **R-ONNX-3 is a feed-forward gate** — the candle↔ONNX Δ measured at Slice 15 calibrates 0.8.18 #5;
  record it precisely.

## ACs minted (Slice-0 gate, HITL 2026-07-08 — `dev/acceptance.md` tracked by F-id/G-gap + TDD names)
1. **R-F9-4** — F9 is `rankable`-forward-compatible per OPP-12 Q6a (design §4 mapping). ✅ Slice 5.
2. **F9 ranking-contract** — OFF-by-default multiplicative-on-fused; 3-way sentinel semantics. ✅ Slice 5.
3. **ONNX-equivalence** — R-ONNX-1 documented Δ tolerance + R-ONNX-3 recorded flip rate. ⏳ Slice 10/15.

## Outstanding worktrees
- None open (`slice-5-20260708T114929Z` cleaned after landing; preflight re-run pass).

## Recent decisions (newest first)
- 2026-07-08 — **Slice 5 (F9 KEYSTONE) CLOSED / LANDED (HITL landing SIGNED).** Cherry-picked
  `6462b511`+`74987f80`+`3c172131` onto `main` (clean; `cargo check --workspace` rc=0). codex §9
  CONCERN→fix1→CONCERN→fix2→**PASS**. Two review rounds caught real gaps: (1) edge confidence wasn't
  applied on graph-arm node hits (+ a vacuously-green test) → fix-1 threads the traversing edge's
  confidence + de-vacuoused the test to a real-path e2e; (2) F9 explain fields dropped by native bindings
  then by the public Py/TS wrappers → fix-1 + fix-2 wired both layers. SCHEMA 17→18 (step-18); F9
  OFF-by-default, no eval claim; eu7 no-op basis. Env flakes (ac_002/t_s34) = concurrent-cargo-test
  artifacts, non-blocking. Slice-40 follow-up: compiled-module e2e SDK parity on the MAIN tree.
- 2026-07-08 — **Slice-0 CLOSED (HITL SIGNED).** 5 gate decisions ratified; 3 ACs minted; step-18 authorized.
- 2026-07-07 — **Slice-0 package DRAFTED** (F9 ADR + ONNX ADR + design doc + this board).

## Next action
**Fan out Slice 10 (ONNX embedder backend)** — canary proven. Cut a worktree off live `origin/main` with a
distinct `TMPDIR`, preflight pass, spawn the implementer, drive the §9 loop to a terminal verdict in-turn.
Slice 10 is expected no-migration → lands under Steward authority on a clean §9 PASS (standing mandate).
