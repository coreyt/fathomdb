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

## Slice scoreboard
| Slice | Title | State | Base SHA | Branch | output.json | codex | Cherry-pick/merge |
|------:|-------|-------|----------|--------|-------------|-------|-------------------|
| 0 | Setup + ADR — F9 ranking/lifecycle (honor Slice-35 ADR) + OPP-12 `rankable` forward-compat; ONNX-backend design + equivalence-measurement plan; stand up this board | **CLOSED (HITL SIGNED 2026-07-08)** | 36024585 | (docs on main) | n/a (design slice) | n/a | docs commit on main |
| 5 | **F9 importance/confidence KEYSTONE** — `canonical_nodes.importance` step-18 (SCHEMA 17→18) + 3-way sentinel + edge-confidence ranking; surfaced via `PerHitExplain` | NOT STARTED (dep: 0) | — | — | — | — | — |
| 10 | **ONNX embedder backend** — `OrtBgeEmbedder` behind the trait via `EmbedderChoice::Caller`; `onnx-embedder` feature; config/env device select; zero engine diff | NOT STARTED (dep: 0) | — | — | — | — | — |
| 15 | **ONNX equivalence measurement** — candle↔ONNX numeric Δ + 1-bit flip rate on a probe set; document interim same-backend discipline (feeds 0.8.18 #5) | NOT STARTED (dep: 10) | — | — | — | — | — |
| 40 | **Verification + Release Readiness** — X1/X2/X3 + R-F9/R-ONNX AC gate + eu7 gate | NOT STARTED (dep: 5,10,15) | — | — | — | — | — |

**Tracks (parallelizable):** F9 track **5** ∥ ONNX track **10 → 15**, off Slice 0. Canary the first real
launch through the full cycle before any parallelism; max 3 concurrent worktrees.

## Requirements / AC status (DoD frozen at Slice 0)
| ID | Requirement | State |
|----|-------------|-------|
| R-F9-1 | Importance column (REAL) + 3-way sentinel + edge confidence | ⏳ Slice 5 (schema step-18) |
| R-F9-2 | Importance/confidence influences ranking AND is observable (`explain`) | ⏳ Slice 5 |
| R-F9-3 | Slice-35 deferred-F9 ADR gate honored (no scope beyond mechanism) | ✅ §3 supersession CONFIRMED (HITL 2026-07-08); mechanism-only, no eval claim |
| R-F9-4 (minted) | F9 is OPP-12 `rankable`-forward-compatible (Q6a) | ✅ AC MINTED (HITL 2026-07-08); mapping done (design §4); verify at Slice 5/40 |
| R-ONNX-1 | `OrtBgeEmbedder` produces BGE-small via the trait, within documented Δ | ⏳ Slice 10 |
| R-ONNX-2 | Backend selected at `Engine::open` via config/env, not compile-only | ⏳ Slice 10 |
| R-ONNX-3 | candle↔ONNX Δ measured + recorded; same-backend discipline documented | ⏳ Slice 15 (feeds 0.8.18 #5) |
| R-X-1 | Py + TS SDK parity for the F9 surface (X1) | ⏳ Slice 40 |
| R-GATE | eu7 ≥ 0.90 (one-sided CI) on any embedder/index change | ⏳ Default paths no-op; verify at Slice 40 |

## Cross-cutting DoD (X1/X2/X3 — bind every slice)
X1 SDK parity + harnesses · X2 `mkdocs build --strict` green · X3 docs + DOC-INDEX per slice.
| Slice | X1 (SDK parity) | X2 (mkdocs) | X3 (docs+DOC-INDEX) |
|------:|-----------------|-------------|---------------------|
| 0 | n/a (design) | pending | this board + ADRs (post-sign-off) |
| 5 | ⏳ | ⏳ | ⏳ |
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
- **SCHEMA_VERSION migration (step-18)** = engine/schema migration → HITL-gated; ADR ratifies the plan.
- **R-ONNX-3 is a feed-forward gate** — the candle↔ONNX Δ measured at Slice 15 calibrates 0.8.18 #5;
  record it precisely.

## AC candidates (to mint at the Slice-0 gate — `dev/acceptance.md` locked; HITL-decided)
1. **R-F9-4** — F9 is `rankable`-forward-compatible per OPP-12 Q6a (design §4 mapping).
2. **F9 ranking-contract AC** — importance/confidence weighting formula + 3-way sentinel semantics.
3. **ONNX-equivalence AC** — R-ONNX-1 documented Δ tolerance + R-ONNX-3 recorded flip rate.

## Outstanding worktrees
- None open.

## Recent decisions (newest first)
- 2026-07-07 — **Slice-0 package DRAFTED** (F9 ADR + ONNX ADR + design doc + this board). DoD frozen; AC
  candidates listed; F9→OPP-12-`rankable` mapping complete; equivalence-measurement plan specified. NOT
  committed (HITL-gated). Reported up to the Steward for the Slice-0 sign-off gate.

## Next action
**HITL Slice-0 DoD sign-off (relayed via the Steward)** — ratify the 3-way sentinel + weighting formula,
confirm the deferred-ADR §3 supersession, authorize the step-18 migration, and mint the AC candidates.
Then commit the Slice-0 docs (one docs commit on `main`) and fan out Slice 5 ∥ Slice 10 (canary first).
