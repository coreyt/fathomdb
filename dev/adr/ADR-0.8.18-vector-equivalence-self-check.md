# ADR-0.8.18 — Vector-equivalence self-check (#5)

- **Status: ACCEPTED (HITL SIGNED 2026-07-09).** Design review CLEAN after 4 codex rounds (BLOCKs resolved +
  re-confirmed, not overridden); Steward-verified. R-VEQ-5 = additive-only (confirmed); D4 floor set after the
  U3 measurement. The full requirements + RED-testable ACs + design are in
  `dev/design/0.8.18-slice-0-vector-equivalence-publish-design.md` §U1.
- **Supersedes-in-part:** the `EmbedderIdentity` pre-filter (`check_embedder_profile`, engine `lib.rs:2806`) —
  identity proves the backend *claims* the same embedder; the probe *proves equivalent 1-bit codes*.

## Decision (proposed — rulings applied)

1. **R-VEQ-1** Store the 45 committed probes + f32 references in a new internal `_fathomdb_embed_probe` at first
   vector-kind registration. **Schema migration SCHEMA_VERSION 18 → 19.** Slice-5 landing is **HITL-gated** (schema bump).
2. **R-VEQ-2/3 — assert BOTH stages of the dense pipeline** (D4 trace, engine-source-verified). On `Engine::open`
   (after `check_embedder_profile`), re-embed each probe and assert: **(P1)** the **mean-centered** Phase-1
   binary-code flip count over `embedding_bin` = `vec_quantize_binary(sign(x − mean_vec))` (`build_vector_phase1_sql`,
   engine `lib.rs` ~6525); AND **(P2)** a **Phase-2 L2 tolerance** on the **UN-centered** float `embedding`
   (`vec_distance_l2`, **not cosine**). Asserting only one lets the other silently drift (correctness trap).
   Mean-centering is gated by `identity_requires_mean_centering(identity)` ∧ mean-pinned (true only for the default
   `fathomdb-bge-small-en-v1.5`; NoopEmbedder no-op), symmetric ingest+query, un-centered fallback if no pin.
3. **R-VEQ-3 tolerance floor — D4 ⏳ PENDING, NOT frozen; TWO components.** The floor is a **design-review/HITL
   parameter** = a **Phase-1 binary-flip count** + a **Phase-2 L2 ε** (lean 0 flips / small ε — not ruled),
   calibrated against the measured 0-flip legs + the U3 canary measurement (which now measures both P1 and P2).
4. **R-VEQ-4/6** On divergence: **degraded-open** — `Engine::open` SUCCEEDS with `dense_disabled=true`, surfaced on
   the **`OpenReport` (Rust/Py/TS)** + telemetry (codex R2 U1-4). The refusal is a new **`EngineError::VectorEquivalenceMismatch`**
   (NOT `EngineOpenError` — queries surface `EngineError`; codex R2 U1-1), raised at the single choke point
   **`search_inner_with_stats`** before embedding/vector-SQL/graph/CE, covering every vector-dependent arm (codex R2
   U1-3); an explicit **text-only/FTS-only path stays serviceable** (codex R2 U1-2). Py/TS get leaf error classes/codes.
   Probe check runs **after** open-time mean-recovery (U1-b); references stored **UN-centered f32 only** (U1-d).
5. **R-VEQ-5 — RESOLVED: #5 is ADDITIVE-ONLY** (codex U1-e): the distinct-identity cross-vendor refusal STAYS; a
   45-probe PASS does not prove tokenizer/model/pooling/EP equivalence for arbitrary/truncated/future-drift text.
   Portability-relaxation deferred to a future dedicated ADR. (HITL confirms.)

## Consequences

Footprint invariant intact (open-time, CPU-only). L3 ONNX-GPU-EP Δ is measured OOB (D3, non-blocking); the ONNX
GPU EP ships **un-probe-verified** until L3 lands (documented gap + TC-track). Open items: D4 floor, R-VEQ-5.
