# FathomDB → Memex: Near-term roadmap

Consolidated to-do list resulting from the 0.3.0 review round with Memex. Grouped by proposed release. Order within each release reflects current priority.

## 0.3.1 — Docs & small-surface wins

Fast-follow release. All items are docs-shaped or small-surface, parallelizable.

1. **`filter_json_*` footgun callout.** Prominent warning in `docs/guides/querying.md` and the `SearchBuilder` reference. Side-by-side wrong-shape vs right-shape over-fetch example showing how small `limit` + post-filter can silently return zero hits.
2. **Rerank recipe guide.** New section in `docs/guides/querying.md` showing how to rerank `SearchRows.hits` on top of block-ordered output. Worked example must cover the dominant Memex pattern:
   - Decay by `written_at`
   - Pin boost by `$.pinned` property
   - Reputation boost by `$.source_reputation` property
   - Using fields already on `SearchHit`: `vector_distance`, `written_at`, `match_mode`, `projection_row_id`, `source`, `modality`
   - Memex may contribute a working version for inclusion.
3. **Content-ref externalization worked example.** Worked example in `docs/guides/content-refs.md` (create if missing) for the "big audit payload, project on read" pattern. Targets the 20KB meetings audit case.
4. **Operational collection query guide.** New guide covering `OperationalAppend` / `OperationalPut` / `OperationalDelete` query surface. Unblocks Memex's 8th category and completes coverage of all eight categories on a single substrate. *Memex flagged this as the most-anticipated 0.3.1 item.*
5. **Grammar contract regression tests.** Pin lowercase `or`/`not` → literal, clause-leading `NOT` → literal, unsupported syntax → literal. Protects the load-bearing chat-→-`search()` property. No user-visible deliverable.

## 0.4.0 — Headline architectural fix

Single major commitment. Everything else in 0.4.x is gated on Memex input.

6. **Write-time embedder parity.** ✅ **SHIPPED in 0.4.0** (commit `a95c1c9`). Resolved by establishing a stronger architectural invariant than originally proposed: vector identity is the embedder's responsibility, not the regeneration config's. `VectorRegenerationConfig` lost the 5 identity-bearing fields (`model_identity`, `model_version`, `dimension`, `normalization_policy`, `generator_command`) entirely; `Engine::regenerate_vector_embeddings(config)` reads the open-time `EmbedderChoice` from the coordinator and errors `EmbedderNotConfigured` if the engine was opened without an embedder. The subprocess generator pattern is removed from fathomdb proper. See `dev/notes/project-vector-identity-invariant.md` and `docs/operations/vector-regeneration.md`.

## 0.4.x — Ranking & composition

Each item gated on a specific Memex input; order set by which inputs land first.

7. **Named fused JSON filters.** ✅ **PROMOTED FROM 0.4.x AND SHIPPED in 0.4.0** (commit `fd28e0d`). New builder methods that push `json_extract` predicates into the inner search CTE WHERE clause for kinds with a registered property-FTS schema, so the `LIMIT` applies after the filter (eliminating the small-limit-returns-zero trap on the post-filter `filter_json_*` family). Post-filter `filter_json_*` semantics are unchanged. Mirrored across Rust core + Python + TypeScript bindings.
   - **First cut shipped:** `filter_json_fused_text_eq` (status-like equality) + `filter_json_fused_timestamp_*` family (gt/gte/lt/lte). Covers ~90% of Memex's use.
   - **Contract enforced:** Raises `BuilderValidationError::MissingPropertyFtsSchema` (or `PathNotIndexed` / `KindRequiredForFusion`) at filter-add time when called against a kind with no registered FTS schema — no silent degrade. Preserves the "reviewer sees the choice in the diff" property.
   - **Still deferred:** Other fused variants until Memex asks.
   - See `docs/reference/query.md` and `docs/guides/querying.md` for the user-facing entry points.
8. **`SearchBuilder.expand(slot=...).execute_grouped()` — search→traverse bridge.** Grouped terminal that composes search with graph expansion in one round trip.
   - **Gate:** Memex `dev/notes/` doc with current Python call shapes and proposed replacements for three known use cases: (a) goal retrieval → active plan steps + commitments + recent execution records, (b) knowledge search → provenance drill-in to source Observations/ExecutionRecords, (c) meeting recall → extracted decisions + action items.
9. **Background recursive property-FTS rebuild.** Async/batched rebuild path so registering a recursive schema on a large kind doesn't stall the write path.
   - **Gate:** Memex advance-flag before `KnowledgeItem.payload` or `ExecutionRecord.trace` convert to recursive mode.
   - **Fallback:** If this slips past 0.4.0, ship a targeted batched-under-a-flag workaround scoped to those two kinds specifically.

## Post-0.4.0 — Larger rewrites

Acknowledged, uncommitted, no ETA.

10. **Per-field BM25 weighting / field-scoped property FTS.** Blocked on design decision: FTS5 per-field weights (doable, ugly) vs layered index (clean, larger rewrite). Interim workaround for Memex: register two property-FTS schemas on the same kind with different path sets and fuse in Python — documented as part of #2.
11. **Write-priority / foreground-read isolation.** Scheduler-burst vs conversation-stall story. Scope (write queueing in `fathomdb-engine` vs connection-pool priority lane vs caller-side semaphore) depends on profile data.
    - **Gate:** Memex scheduler-burst instrumentation — writes/sec, payload sizes, transaction shape, captured during representative burst (task runs + connector health + intake queue concurrent with a chat turn). ETA: "a few days" from Memex.

## Explicitly declined

- **Cross-modality unified score / `rerank_with(fn)` hook.** Held back pending Memex trying the #2 rerank recipe on real workloads. Revisit only if the recipe proves insufficient for goal/knowledge ranking. Memex has retracted the pushback and accepted this sequencing.

---

## Critical path

**0.3.1** (1–5, parallelizable, docs-shaped)
→ **0.4.0** (6, the one architectural commitment)
→ **0.4.x** (7 unblocked; 8 and 9 unblock as Memex inputs land)
→ **Post-0.4.0** (10, 11 — neither committed)

## Waiting on Memex

| # | Item | Gates |
|---|---|---|
| A | Scheduler-burst instrumentation data | #11 |
| B | `dev/notes/` composition use-case doc with call-site shapes | #8 |
| C | Advance flag before recursive conversion of `KnowledgeItem.payload` / `ExecutionRecord.trace` | #9 fallback |
| D | *Optional:* working rerank example contributed back | #2 quality |
