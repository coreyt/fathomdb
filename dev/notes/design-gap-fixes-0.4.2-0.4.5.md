> **Status: IMPLEMENTED** — All four gaps merged to main at commit 4f7a9cb (2026-04-15).
> See commits 72ce85c (Gap4), d891b6f (Gap1), 5ad551e (Gap2+3).

# Gap Fixes: 0.4.2 → 0.4.5

Four binding-layer gaps were identified between the Rust engine and the Python/TypeScript SDKs. All four are now closed.

## Gap 1 — `FtsPropertyPathSpec.weight` silently dropped

The `weight` field on `FtsPropertyPathSpec` was present in the Rust engine but not forwarded through the Python or TypeScript bindings.

**Fix:** `PyPropertyPathSpec` in `admin_ffi.rs` now includes `weight: Option<f32>`. The Python `FtsPropertyPathSpec` dataclass gained `weight: float | None = None`. The TypeScript `FtsPropertyPathSpec` type gained `weight?: number`.

## Gap 2+3 — TypeScript SDK missing projection profile + vec regen methods

The Python `AdminClient` had `get_fts_profile`, `get_vec_profile`, `preview_projection_impact`, `configure_fts`, `configure_vec`, `restore_vector_profiles`, and `regenerate_vector_embeddings`. None of these were present in the TypeScript `AdminClient`.

**Fix:** All seven methods were added to `typescript/packages/fathomdb/src/admin.ts`. New types `FtsProfile`, `VecProfile`, `ProjectionImpactReport`, `VecIdentity`, `VectorRegenerationConfig`, and `VectorRegenerationReport` were added to `types.ts`.

## Gap 4 — Python CLI `_resolve_embedder` wrong metadata

The CLI helper `_resolve_embedder` did not recognise `"bge-small-en-v1.5"` or `"BAAI/bge-small-en-v1.5"` as the built-in embedder, and stub embedders used the wrong `normalization_policy`.

**Fix:** Both aliases now return `BuiltinEmbedder()`. All stub embedders use `normalization_policy="l2"`.
