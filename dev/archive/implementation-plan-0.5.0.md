# FathomDB 0.5.0 Implementation Plan

Items 1, 2, 3 already shipped. Remaining: items 4, 5, 6, 7.

## State

| Item | Status | Notes |
|------|--------|-------|
| 1. TextQuery AST | DONE | text_query.rs complete |
| 2. Adaptive search response shape | DONE | SearchRows/SearchHit complete |
| 3. FTS Level 3 | DONE | extract_property_fts handles objects |
| 4. TypeScript embedding adapters | NOT_STARTED | Design: design-typescript-embedding-adapters-0.5.0.md |
| 5. QueryEmbedder::max_tokens() | NOT_STARTED | Rust trait change, breaking |
| 6. Per-kind vec tables | NOT_STARTED | Schema + coordinator change |
| 7. Builtin write-time path | PARTIAL | regen exists; needs BatchEmbedder + in-process fn |

## Parallel tracks

Three independent tracks. Spawn as separate worktrees.

---

### Track A: Embedder traits (items 5 + 7)

Items 5 and 7 both modify `crates/fathomdb-engine/src/embedder/`. Run sequentially
in one worktree.

**Item 5: `QueryEmbedder::max_tokens()`**

File: `crates/fathomdb-engine/src/embedder/mod.rs`

1. Add `fn max_tokens(&self) -> usize;` to `QueryEmbedder` trait (after `identity()`).
2. Implement for every existing impl — audit all:
   - `BuiltinBgeSmallEmbedder` → 512
   - Any test stubs or mock impls
   - Subprocess-based embedder wrapper (Python FFI callback wrapper) → needs a
     way to call back; default to 512 if no override.
3. Write-time chunker in `admin.rs` (`regenerate_vector_embeddings`) reads
   `embedder.max_tokens()` instead of any hardcoded constant.
4. Python FFI: add `max_tokens()` to `QueryEmbedder` Python wrapper so Python
   subclasses can override; default impl returns 512.
5. Tests: integration test — stub embedder returning `max_tokens()=8192`, write
   a document > 512 tokens, verify stored as 1 chunk not 2.

**Item 7: BatchEmbedder + in-process regeneration**

File: `crates/fathomdb-engine/src/embedder/mod.rs`, `admin.rs`

1. Define new trait alongside `QueryEmbedder`:
   ```rust
   pub trait BatchEmbedder: Send + Sync {
       fn batch_embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedderError>;
       fn identity(&self) -> QueryEmbedderIdentity;
       fn max_tokens(&self) -> usize;
   }
   ```
2. Implement `BatchEmbedder` on `BuiltinBgeSmallEmbedder`:
   - `batch_embed`: tokenize all texts in one call, run BERT forward pass, return vecs.
3. Add `regenerate_vector_embeddings_in_process(embedder: &dyn BatchEmbedder, config: &VectorRegenerationConfig)` to `AdminService` (or top-level admin fn). Reuse existing contract-writing logic from `regenerate_vector_embeddings`.
4. Python admin surface: `AdminClient.regenerate_vector_embeddings_in_process(config)` — calls in-process path (requires builtin feature enabled).
5. TypeScript admin surface: same.
6. Tests: integration test — register schema, write nodes, call in-process regen, verify contract row + cosine-match query vectors for same text.

**TDD checkpoints:**
- Red: test for max_tokens() on BgeSmall → 512 (trait doesn't have it yet)
- Green: add trait method + impl
- Red: test for chunker respecting max_tokens
- Green: wire chunker
- Red: test for in-process regen round-trip
- Green: BatchEmbedder + in-process fn

---

### Track B: Per-kind vec tables (item 6)

File: `crates/fathomdb-schema/src/bootstrap.rs`, `crates/fathomdb-engine/src/coordinator.rs`, `crates/fathomdb-engine/src/admin.rs`

**Schema changes:**
1. `ensure_vector_profile` already accepts `table_name`. Change callers to
   pass `vec_<sanitized_kind>` instead of `"vec_nodes_active"`.
2. Kind-name sanitization: reuse existing `fts_kind_table_name` pattern
   (`kind.to_lowercase().replace(|c: char| !c.is_alphanumeric(), "_")`).
3. Schema migration: on open, for each kind with existing vec rows in
   `vec_nodes_active`, create `vec_<kind>` table. No data migration —
   breaking change; callers must re-run regeneration.
4. Drop the `('*', 'vec')` projection_profiles key; use `(kind, 'vec')`.
5. `VecProfile` read/write uses `(kind, 'vec')` key.

**Coordinator changes:**
1. `coordinator.rs` vector search targets `vec_<kind>` table derived from
   query's root kind.
2. If `vec_<kind>` table doesn't exist, return `was_degraded=true` (same as
   current capability-miss path).

**Admin changes:**
1. `regenerate_vector_embeddings(embedder, config)` writes to
   `vec_<kind>` from `config.kind` (add `kind` field to `VectorRegenerationConfig`).

**TDD checkpoints:**
- Red: test that open on 0.5.0 schema creates vec_<kind> tables
- Green: schema bootstrap creates per-kind tables
- Red: test that regen writes to vec_<kind>
- Green: wire admin regen
- Red: test that vector search hits vec_<kind>
- Green: coordinator targets per-kind table
- Red: test that legacy vec_nodes_active rows don't appear in new search
- Green: verify isolation

---

### Track C: TypeScript embedding adapters (item 4)

Design: `dev/notes/design-typescript-embedding-adapters-0.5.0.md`

**Files to create:**
- `typescript/packages/fathomdb/src/embedders/index.ts`
- `typescript/packages/fathomdb/src/embedders/openai.ts`
- `typescript/packages/fathomdb/src/embedders/jina.ts`
- `typescript/packages/fathomdb/src/embedders/stella.ts`
- `typescript/packages/fathomdb/src/embedders/subprocess.ts`

**Files to modify:**
- `typescript/packages/fathomdb/src/index.ts` — add barrel export

**Test files:**
- `typescript/packages/fathomdb/test/embedders/openai.test.ts`
- `typescript/packages/fathomdb/test/embedders/subprocess.test.ts`

**TDD checkpoints:**
1. Red: interface test — object implements QueryEmbedder shape check
2. Green: define interface + OpenAIEmbedder stub
3. Red: OpenAIEmbedder.embed() with mocked fetch returns correct shape
4. Green: implement embed() with fetch + cache
5. Red: SubprocessEmbedder.embed() round-trips known vector via fixture script
6. Green: implement subprocess protocol
7. Red: JinaEmbedder.embed() with mocked fetch
8. Green: implement JinaEmbedder
9. Red: StellaEmbedder.embed() with mocked fetch
10. Green: implement StellaEmbedder
11. Verify: SDK harness baseline unchanged

---

## Sequencing within a track

Each track proceeds independently. Merge order when all done:
1. Track A (blocking if Python/TS FFI max_tokens needed by other tracks)
2. Track B + Track C (order doesn't matter)

No merge conflicts expected across tracks — they touch disjoint files.

## Ship criteria (combined)

- All 7 items pass their individual ship criteria from `0.5.0-scope.md`.
- Rust test suite: `cargo test -p fathomdb-engine -p fathomdb-query -p fathomdb-schema`
- Python SDK harness: baseline + vector scenarios pass
- TypeScript SDK harness: baseline + vector scenarios pass
- No regression in existing stress tests
