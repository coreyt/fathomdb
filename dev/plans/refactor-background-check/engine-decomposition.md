# One Module Tree for `fathomdb-engine/src/lib.rs` (17,910 LOC → ~20 files, largest ~1,100)

## 0. The enabling fact this whole plan rests on

Rust lets inherent `impl` blocks for a type live in **any module of the defining crate**, and private fields are visible in the defining module **and all its descendants**. So if `struct Engine` stays at the crate root (`lib.rs`), every child module can write `impl Engine { … }` and touch `self.connection`, `self.reader_pool`, `self.dense_disabled` etc. with **zero visibility churn, zero facade, zero API change** — `Engine::search()` remains `Engine::search()` for every consumer.

That is why the answer is "one `Engine` type, ~16 `impl Engine` blocks in ~16 capability modules", not "split `Engine` into 16 types". Nobody should attempt the latter.

**Hard constraint that follows:** `struct Engine` must stay in `lib.rs`. If someone "tidies" it into `engine/mod.rs` and makes the capability modules siblings, private-field access breaks everywhere and the plan collapses into a `pub(crate)`-field rewrite. Write this down in the first commit.

---

## 1. Findings that are NOT "split the file" (do these regardless)

Per doctrine #3, these are worth more than the split and are independent of it:

| Finding | Location | Action |
|---|---|---|
| `sqlite_extended_code_name` / `sqlite_extended_code_name_from_int` are near-verbatim duplicate ~20-arm matches | L17195–17250 | Real duplication defect. Make the `&rusqlite::Error` form delegate to the `i32` form. ~20 lines deleted. Do this today, independent of everything below. |
| `read_search_in_tx` — 748 lines, 6 comment-delimited phases, 16 params | L10377–11124 | The only function in the file that clears the "over 200 lines with real internal seams" bar. But see §7 — the phases share one `Transaction` and one `FrozenView`; only split into **private helpers in the same file**, never across modules, and only if each phase can take `(&FrozenView, &Transaction) -> Vec<SearchHit>` without a 16-field context struct. |
| `Engine` is the privileged owner of every subsystem's test seam | ~46 `*_for_test` methods, L6766–7647 | This is the actual "one type carries 140 responsibilities" defect, and it is the cheapest to fix. It is step 1 below. |
| Four unrelated things are all called "projection"; two unrelated things are both called "lifecycle" | see §3 | Naming, not size. Resolved by the tree below. |
| `bfs_graph_arm_candidates` (338 lines) | L11125–11462 | **LEAVE.** One coherent algorithm (seed phase + traversal phase), not several concerns bolted together. Region 4 got this right. |

---

## 2. The module tree

All paths relative to `src/rust/crates/fathomdb-engine/src/`. "impl-Engine LOC" is the part of the 5008-line block that moves.

| Module | Owns | Line ranges moved | ≈LOC | impl-Engine LOC / methods |
|---|---|---|---|---|
| `lib.rs` (root) | `struct Engine`, `Debug`/`Drop`, imports, crate-wide consts, engine controls (`close`/`drain`/`counters`/`subscribe`/`set_profiling`/`set_slow_threshold_ms`/`path`/`ensure_open`), `emit_event`/`detect_slow`/`emit_sqlite_internal_error`, and **all `pub use` re-exports** | L3–223, 274–346, 525–543, 4104–4109, 4663–4666, 6096–6142, 6686–6765, 9109–9117 | **~600** | ~140 / 11 |
| `error.rs` | `EngineError` + `Display` + `stable_code()` + `Error` | L3529–3736 | 208 | — |
| `id.rs` | `IdSpaceKind`/`IdSpace`, `SourceId` (whole, incl. its trait tail), `resolve_source_type`, `derive_logical_id`, `derive_stable_id` | L1458–1558, 2924–3010, 14867–14955 | 277 | — |
| `temporal.rs` | `ReadView`/`FrozenView`/`BoundaryCrossing`, `CLOCK_READS`, `current_epoch_seconds`, `edge_validity_sql`, the whole ISO-8601⇄epoch boundary (`is_iso8601_shape`, `normalize_extractor_timestamp`, …) | L1659–2253 | 595 | — |
| `existence.rs` | `LifecycleState`, `InitialState`, `is_legal_transition_move`, `resolve_lifecycle_target`, `transition`, `purge`, `purge_inner` | L3169–3305, 7783–7930, 7986–8153 | 455 | 316 / 4 |
| `filter.rs` | `ScalarValue`/`ComparisonOp`/`Predicate`, `SearchFilter`, `Filter`/`FilterTerm` + lowering, **and** all three arms' compilation/enforcement (`vector_filter_clause`, `build_vector_phase1_sql`, `text_hit_passes_filter`, `edge_fts_hit_passes_filter`) | L2381–2547, 2594–2803, 10022–10376 | 732 | — |
| `reader_pool.rs` | `ReaderWorkerPool`, `ReaderRequest`, `SearchReaderError`, `CacheStatusReply`, `reader_worker_loop`, `reader_search_hook` | L558–1144 | 590 | — |
| `search/mod.rs` | `SearchHit`, `SoftFallback(Branch)`, `GraphFrontierStats`, `SearchResult`/`Explanation`/`QueryTrace`/`PerHitExplain`, `branch_str`, the 13 public `search*` methods, `search_inner(_with_stats)` | L1386–1457, 1559–1629, 2272–2380, 5615–5961, 6143–6344, 9401–9411 | 810 | 549 / 16 |
| `search/exec.rs` | `read_search_in_tx` + retrieval tuning consts | L9130–9143, 10377–11124 | 770 | — |
| `search/ranking.rs` | RRF consts, `fuse_rrf`/`fuse_three_arms`, recency/importance reweight, `rerank_fused`/`rerank_passages`/`ce_rerank`/`CandleCrossEncoder` | L9357–9397, 9531–10021 | 535 | — |
| `search/graph_arm.rs` | `bfs_graph_arm_candidates` | L11125–11462 | 340 | — |
| `read/mod.rs` | `NodeRecord`, `OpStoreRow`, the 10 `read_*`/`graph_neighbors`/`search_expand`/`crossed_boundary_since` verbs, `read_get_by_id_in_tx`, `read_collection_in_tx`, `read_list_in_tx` | L1630–1658, 2254–2271, 6345–6685, 11463–11696 | 670 | 341 / 10 |
| `read/graph.rs` | `TraversalDirection`, `SearchExpandResult`, `build_bfs_sql`, `build_bfs_with_depth_sql`, `graph_neighbors_in_tx`, `crossed_boundary_since_in_tx`, `search_expand_in_tx`, `explain_graph_neighbors_in_tx` | L2548–2593, 11697–12070 | 420 | — |
| `write/mod.rs` | `PreparedWrite`, `RowKind`, `WriteReceipt`, `Engine::write`/`write_inner`, `write_node_importance`/`node_importance`, `batch_is_admin`, `WritePlan` + all validation | L1362–1385, 3011–3168, 4667–4779, 7507–7553, 9118–9149, 15300–15480 | 620 | 160 / 4 |
| `write/commit.rs` | `prior_*_cursors_by_*`, `commit_batch`, `load_next_cursor`/`reserved_write_cursor`/`max_cursor` | L15481–15530, 16817–17194 | 430 | — |
| `projection/runtime.rs` | `ProjectionJob`/`ProjectionRuntimeState`/`ProjectionRuntimeShared`/`ProjectionRuntime` + `impl`, dispatcher/worker loops, watchdog, circuit breaker, `run_projection_job` | L228–252, 356–524, 1145–1292, 12071–12618 | 890 | — |
| `projection/commit.rs` | `open_runtime_connection`, cursor load/store, `record_projection_terminal`, `commit_projection_outcomes` | L13044–13336 | 293 | — |
| `projection/registry.rs` | `ProjectionRole`/`Fts`/`Vector`/`Spec`/`Delta`, `ROW_OWNED_PROJECTIONS`, `erase_row_projections` family, `StoredProjection`, EAV attribute projection, `apply_projection_config`, per-`row_kind` projector dispatch, `Engine::configure_projections`/`read_projections` | L3843–3946, 7931–7985, 15531–16625 | 1,110 | 55 / 2 |
| `projection/maintenance.rs` | Boot repair (tokenizer reproject, orphan edge-vector prune, `rederive_projections_on_boot`) + `RebuildKind`/`RebuildReport`, `rebuild_projections`/`rebuild_vec0`/`run_rebuild`/`rebuild_shadow_state` | L3807–3829, 7707–7725, 8998–9108, 12619–12907, 16397–16434 | 490 | 130 / 4 |
| `vector/mod.rs` | blob encode/decode, `subtract_mean`, `read_pinned_mean_vec`, `identity_requires_mean_centering`, `embed_text`, `drain_embedder_events` | L7226–7233, 7399–7423, 14392–14451 | 130 | 33 / 2 |
| `vector/partition.rs` | Everything about `vector_default`'s vec0 *shape*: create, Pack1↔Pack2 migration, `attr_<hex>` column reconcile/reshape | L13921–14391 | 471 | — |
| `vector/mean.rs` | `MeanAccumulator`, `cosine_similarity`, pin-and-requantize, `recover_mean_vec_pin`, `recompute_mean_in_tx*`, `Engine::recompute_mean`, `MeanRecomputeReport` | L4075–4087, 7424–7472, 9150–9400, 13337–13454 | 435 | 49 / 1 |
| `vector/equivalence.rs` | The whole 0.8.18 vector-equivalence self-check (`run_vector_equivalence_probe` and friends, `l2_distance`, `hamming_bytes`) | L14452–14866 | 415 | — |
| `extract/session.rs` | `ProviderTask`, `ProviderSession` (NDJSON-over-stdio subprocess transport), `extractor_io_timeout`, `recv_extractor_line`, `map_runtime_embedder_error`, `Engine::provider_session`, `dedup_prepared_by_logical_id` | L4803–4881, 14956–15150 | 260 | 79 / 1 |
| `extract/ingest.rs` | `ExtractDocument`, `IngestWithExtractorReceipt`, `ingest_with_extractor`, `run_extract_session` | L2804–2830, 4780–4802, 4882–5261 | 430 | 403 / 2 |
| `extract/consolidate.rs` | `ConsolidateAxis`/`CandidateEdge`/`Receipt`, `consolidate_with_provider`, `run_consolidate_session`, `assemble_consolidate_cluster`, `apply_consolidate_verdicts`, `active_edge_write_cursor`, `prune_edge_projection_shadows` | L2831–2923, 5262–5614 | 450 | 353 / 6 |
| `erasure/mod.rs` | `is_erasure_bookkeeping_collection` + erasure consts, `ExciseReport`/`ExciseRecordReport`/`OrphanProvenance*`, the 13-method GDPR verb family, `wal_checkpoint_truncate_once`, `enforce_provenance_retention`, `collect_erased_stable_ids`, `digest_record_identity` | L144–273, 3830–3842, 3947–3957, 4014–4060, 8154–8225, 8308–8393, 8444–8997, 13455–13579 | 930 | 712 / 13 |
| `erasure/redaction.rs` | `append_jsonl`, `redact_jsonl_*`, pending-redaction enqueue/load/clear | L9412–9530, 13543–13571 | 250 | — |
| `open/mod.rs` | `OpenReport`/`OpenedEngine`/`LoaderInfo`, all 12 `open*` methods incl. `open_locked` | L1293–1361, 4111–4662 | 630 | 556 / 12 |
| `open/errors.rs` | `CorruptionDetail`/`Kind`/`Locator`, `OpenStage`, `RecoveryHint`, `EngineOpenError` + `Display`, `EmbedderChoice`, `sqlite_extended_code_name*`, `map_open_sqlite_error`, `emit_open_error_event` | L3369–3528, 17195–17320 | 290 | — |
| `open/probe.rs` | Lock acquisition, header/WAL probes, legacy-shape rejection, `map_migration_error`, perf-experiment/extension init, `check_embedder_profile` | L13580–13920, 15151–15299 | 460 | — |
| `admin.rs` (`feature = "operator"`) | The ~14 one-verb-one-DTO doctor report types **as one flat catalog**, `check_integrity`/`safe_export`/`trace_source_ref`/`verify_embedder`/`dump_*`/`truncate_wal`, the `*_section` helpers, `hex_encode` | L3737–3806, 3958–4013, 4061–4103, 7648–7706, 7726–7782, 8226–8307, 8394–8443, 12908–13043, 13886–13920 | 590 | 248 / 8 |
| `bm25f.rs` | `Bm25fFieldWeights`/`Bm25fQueryPlan`, `fts5_tokenize`, `bm25f_score_doc`, `bm25f_search_inner`, `Engine::bm25f_search` | L3306–3351, 7201–7225, 16626–16816 | 262 | 25 / 1 |
| `telemetry.rs` | `TelemetrySink`, `CounterSnapshot`, `enable_telemetry`, `capture_telemetry`, `record_feedback`, `last_telemetry_query_id` | L347–355, 3352–3368, 5962–6095 | 165 | 134 / 4 |
| `sqlite_ffi.rs` | `ProfileContext`, `install`/`uninstall_profile_callback`, the FFI trampoline, perf pragmas, lookaside/cache-status `unsafe` | L544–557, 17321–17620 | 315 | — |
| `test_hooks.rs` | One `#[cfg(…)] impl Engine` holding the ~46 `*_for_test` seams | L6766–7200, 7234–7398, 7473–7506, 7554–7647 | 730 | 730 / 46 |
| (existing) `lifecycle.rs`, `pcache2.rs` | unchanged | — | — | — |

**Totals:** ~17,800 accounted for. Largest new file ~1,110 (`projection/registry.rs`). No file under 130. Root `lib.rs` ends at ~600 lines. Largest single `impl Engine` block ends at ~730 (`test_hooks`), then extract-ingest+consolidate ~780 across two files, `open` 556, `search` 549.

`telemetry.rs` (165) and `vector/mod.rs` (130) are below the Hatton floor. That's deliberate and defensible: `vector/mod.rs` is a `mod.rs` of shared primitives, and `telemetry.rs` is a privacy-scoped surface whose whole value is being auditable in isolation. If you disagree, fold `telemetry.rs` into the root — do **not** fold it into `erasure/redaction.rs` (they are the writer and the eraser of the same file and must be reviewable separately).

---

## 3. Conflicts between regions, resolved

1. **Filter grammar (R1 c16/18-19 vs R4 c7).** R1 claimed the types (L2381–2803), R4 claimed the SQL builders and the three per-arm enforcement functions (L10022–10376). One capability — "what can be filtered, and how each arm compiles and enforces it". → single `filter.rs` (732). Separating them would break the documented D3/D4 lowering contract.

2. **Four things named "projection"** (R1 c3 runtime state; R2 c9 declarative config types; R5 c2/c5 loops+commit; R6 c6/c7 registry+per-row projector; R3 c13 the `Engine` config verbs). → one `projection/` directory with four files whose names say which is which: `runtime.rs` (scheduler/threads), `commit.rs` (durable write of outcomes), `registry.rs` (declarative config + EAV + row projector), `maintenance.rs` (boot repair + rebuild). This naming fix is a real deliverable, not incidental.

3. **Two things named "lifecycle"** (`pub mod lifecycle` = subscribers/events, already a published module path; vs `LifecycleState` = the OPP-12 existence axis). → keep `lifecycle.rs` untouched (public API), name the new one **`existence.rs`**. Never `lifecycle_state.rs` — too close.

4. **Erasure claimed four ways** (R3 c14 impl methods; R4 c4 JSONL redaction; R5 c7 provenance retention; R6 c6 the registry-driven `erase_row_projections`). → `erasure/` takes the first three. `erase_row_projections` / `purge_row_projections_for_cursor_in` / `truncate_*_row_projections` **stay in `projection/registry.rs`**, because they are registry-walk primitives whose correctness is guaranteed by the `guard_row_owned_registry` test that also lives there. `erasure`, `existence::purge_inner`, and `projection::maintenance` all call into them. This is the one deliberate cross-module edge in the plan; document it at the top of `erasure/mod.rs`.

5. **`SourceId` split across the R1/R2 seam** (L2924–3010) — one type, → `id.rs` whole.

6. **`derive_logical_id` / `derive_stable_id` split across the R5/R6 seam** (L14898–14955) — one cluster, → `id.rs`.

7. **`search_expand_in_tx` split across the R4/R5 seam** (L11904–12040) — continues the BFS-CTE theme, → `read/graph.rs` with `build_bfs_with_depth_sql`, which it reuses.

8. **`check_embedder_profile`** (R6 c3 argued it belongs with the vector/open integrity cluster, not where it sits) — agreed → `open/probe.rs`, **not** `vector/`. It produces `EngineOpenError`, only `open_locked` calls it.

9. **Mean-centering split** (R4 c3 accumulator/requantize; R5 c6 recompute/recover) → `vector/mean.rs`, both halves.

10. **Rebuild** (R3 c15 grouped it with erasure/lifecycle on the shared drain-freeze-mutate pattern) → I overrule that: the shared *pattern* is not a shared *capability*. Rebuild regenerates derived projection state → `projection/maintenance.rs`. Extract the drain-then-freeze sequence as one `pub(crate) fn` in the root if the three call sites drift.

---

## 4. How `impl Engine` decomposes — the core of the answer

**Basis for clustering (in priority order):**

1. **Which subsystem's state the method mutates** (reader pool / projection runtime / vec0 partition / provenance / subprocess) — not what it returns.
2. **Which free functions it calls.** Every one of the 140 methods is a thin funnel into free functions that currently sit 3,000–12,000 lines away *in the same file*. The dominant finding across all six regions is identical: **the `impl Engine` / free-function boundary is not capability-aligned.** Almost every capability has a "public API" half in the impl block and an "engine room" half in the free-function tail. The decomposition is: **co-locate each capability's two halves; the impl-block boundary is an artifact of file convention and carries no information.**
3. Cargo feature / `cfg` gate, as a tiebreaker (`operator` → `admin.rs`; `default-reranker` → `search/ranking.rs`; `_for_test` → `test_hooks.rs`).

**Resulting 16 `impl Engine` blocks:**

| Block | Methods | LOC | Why these cluster |
|---|---|---|---|
| `test_hooks.rs` | ~46 | 730 | All `*_for_test`. The one real cohesion defect: these let `Engine` reach into reader-pool, projection-runtime, mean-centering, and provenance-retention private state directly, diluting every production cluster they sit between. |
| `extract/` (3 files) | 9 | 835 | Extract and consolidate are **siblings over one shared subprocess transport** (`ProviderSession`), not independent — hence one directory, transport in its own file. |
| `open/mod.rs` | 12 | 556 | Eleven thin funnels into `open_locked` (237 lines), which wires every other subsystem's boot hook. The most cross-cutting block in the file, and the reason it goes late in the sequence. |
| `search/mod.rs` | 16 | 549 | Public search surface + query-vector prep + `dense_disabled` latch. Dispatches through `ReaderRequest::Search`; the SQL is `search/exec.rs`. |
| `read/mod.rs` | 10 | 341 | Uniform shape: build a `ReaderRequest` variant, dispatch, unwrap. Pure API surface over `*_in_tx`. |
| `existence.rs` | 4 | 316 | The governed pending/active/deleted/purged state machine; only these can produce `Deleted`/`Purged`. |
| `erasure/mod.rs` | 13 | 712 | Region 3's assessment holds: "almost no coupling into the other clusters beyond `drain`/`ensure_open`/`connection`". The cleanest large extraction in the impl block. |
| `admin.rs` | 8 | 248 | All `feature = "operator"`, all read-mostly report generators. |
| `write/mod.rs` | 4 | 160 | `write`/`write_inner` + the two importance accessors that write `canonical_nodes.importance`. |
| `projection/maintenance.rs` | 4 | 130 | Rebuild wrappers + the two rebuild workers. |
| `projection/registry.rs` | 2 | 55 | `configure_projections`/`read_projections` — thin wrappers, put them next to what they wrap. |
| `lib.rs` (root) | 11 | 140 | Engine-wide operational controls + the shared event-emission primitives used by every other block. Correctly small; do not split further. |
| `telemetry.rs` | 4 | 134 | Privacy-scoped by design (never captures query text or `source_id`) — worth being separately auditable. |
| `vector/mean.rs` | 1 | 49 | `recompute_mean` next to the accumulator it drives. |
| `vector/mod.rs` | 2 | 33 | `embed_text`, `drain_embedder_events`. |
| `bm25f.rs` | 1 | 25 | `bm25f_search` next to `bm25f_search_inner`. |

Note the last five: five one-to-four-method `impl Engine` blocks. That is fine and is the point — a method goes where the thing it delegates to lives, even when that's a block of one.

---

## 5. Sequencing — this matters more than the end state

Do **not** attempt this as one change. Each step below is one PR, compiles green, changes no public path, and is independently abandonable.

**Step 0 (today, unrelated to the split):** collapse the duplicated `sqlite_extended_code_name*` match. ~20 lines. Proves the review produced something usable before any restructuring lands.

**Step 1 — `test_hooks.rs` (−730, impl block 5008→~4,280). Do this first.**
Why this one:

- **Zero production risk.** Every method moved is `*_for_test`/`#[doc(hidden)]`/`cfg(debug_assertions)`. If it compiles, it is correct.
- **It is the one named defect** in the impl block, not just an attention signal.
- **It proves the mechanism** under the crate's real constraints — split `impl Engine` across files, private field access from a child module, `cfg`/feature gates surviving the move, and the pyo3/napi/integration-test consumers unchanged. Everything after depends on that proof.
- Biggest single cut of the impl block available at zero risk (15%).
- Guardrail it earns: once these 46 methods are in one place, add a lint/test that no new `*_for_test` method may be added to any other `impl Engine` block.

**Step 2 — `erasure/` (−1,176; impl → ~3,570).**
Second because it's the largest genuinely self-contained *production* capability, all six regions independently identified it as such, and it is the area under the most active HITL/GDPR scrutiny — the capability that most benefits from being one reviewable module. Also forces the `erase_row_projections` cross-edge decision (conflict 4) early, while it's cheap to revisit.

**Step 3 — `extract/` (−1,150; impl → ~2,735).**
A whole subprocess protocol with a single entry point. After this, the impl block is under 3,000 and the "one type, 140 responsibilities" finding is already half-discharged. **This is a legitimate stopping point** — see §8.

**Step 4 — `open/` (−1,366; impl → ~2,180).**
Deliberately *not* earlier despite being large. `open_locked` calls into a dozen subsystems; if you extract it first, its call sites become a wall of `crate::…` paths to code that hasn't moved yet, and you'll want to redo them. After steps 5–6 those calls read `vector::partition::ensure_vector_partition(…)`, `projection::maintenance::rederive_on_boot(…)` — which is the actual discoverability win. If you'd rather not reorder, do step 4 *last* instead of fourth.

**Step 5 — `projection/` + `vector/` (−4,120).** The largest block of work, and the one that needs the most care because `commit_projection_outcomes` and `commit_batch` share transaction ordering. Split into two PRs (`vector/` first — `equivalence.rs` and `partition.rs` are near-zero-coupling warmups).

**Step 6 — the retrieval spine: `search/` + `filter.rs` + `read/` + `reader_pool.rs` (−4,819).** Last of the majors, because it's the most interconnected and the most performance-sensitive. Do `filter.rs` first (it's a leaf both `search/exec.rs` and `read/mod.rs` depend on).

**Step 7 — leaves, any order, opportunistically:** `id.rs`, `temporal.rs`, `error.rs`, `existence.rs`, `bm25f.rs`, `sqlite_ffi.rs`, `admin.rs`, `telemetry.rs`, `write/`. Each is small, self-contained, and can ride along with whatever slice touches it.

**Mechanical rules for every step:**

- Move code **verbatim**. No renames, no signature changes, no "while I'm here" cleanups. One PR = one `git mv`-shaped diff. Reviewability of the diff is the entire point.
- Move each item's tests out of `mod tests` (L17621–17910) **with** the item, in the same PR. `guard_row_owned_registry` moves with `projection/registry.rs`.
- Every PR ends with a public-symbol diff (rustdoc JSON or `cargo public-api`) proving the exported surface is byte-identical.
- Full-workspace `cargo clippy --workspace --all-targets` + `cargo check --workspace --all-targets`, both exit 0 — per the release-DoD memory note, per-crate verify masks cross-crate breaks, and this refactor is exactly the shape that produces them.

---

## 6. What genuinely should stay together

Being honest about the parts where LEAVE is the right verdict:

- **`read_search_in_tx` (748 lines).** Its six phases share one `Transaction`, one `FrozenView` (the type-level guarantee that `:now` resolves exactly once), and ~16 locals. It stays in `search/exec.rs` as one function unless someone demonstrates the phases decompose to `(&FrozenView, &Transaction) -> Vec<SearchHit>` without threading a 16-field context struct. A context struct would be strictly worse than the current shape.
- **`bfs_graph_arm_candidates` (338).** One algorithm. LEAVE.
- **`commit_batch` (321).** It is the connective tissue for validation, prior-cursor lookup, the row projector, and the G8 dangling-edge pass — all inside one transaction. It is long *because* it is one transaction. LEAVE as one function; it moves to `write/commit.rs` intact.
- **The doctor report DTO catalog** (~14 one-verb-one-DTO pairs, L3737–4103). Zero coupling between them; the flatness is appropriate, not a defect. Move as **one unit** into `admin.rs`; never decompose further. Splitting these into 14 files is the textbook doctrine-#2 violation.
- **`EngineError` (208 lines) and `PreparedWrite` (125).** Cross-cutting by definition — `EngineError` is returned by essentially all 140 methods. `error.rs` is a leaf every module imports; that's correct, not a smell. Do not try to distribute the variants.
- **`sqlite_ffi.rs` unsafe blocks** stay adjacent to the trampoline they configure; their `// SAFETY:` comments reference each other's pointer lifetimes.
- **`mod tests` (290 lines).** Per the test doctrine, nothing to flag — no per-test-length or duplicated-fixture problem. It dissolves only as a side effect of steps 1–7.

---

## 7. What would make this plan wrong

1. **`struct Engine` moves out of the crate root.** Then private fields stop being visible to the capability modules and the whole plan degenerates into a `pub(crate)`-field rewrite with a much larger blast radius. This is the single load-bearing assumption.
2. **A consumer imports an internal path.** If `fathomdb-py`, `fathomdb-node`, `fathomdb-cli`, or any integration test does `use fathomdb_engine::some_free_function`, moving it breaks them. The plan assumes `pub use` re-exports from `lib.rs` cover 100% of the current exported surface. **Verify this with a symbol diff before step 1, not after step 6.** If the surface turns out to be wide and undocumented, the cost of every step roughly doubles.
3. **0.8.20 is in flight.** A 17,910-line reshuffle conflicts with every concurrent slice touching this file. If slices are landing weekly against `lib.rs`, this must be sequenced *between* releases, and steps 5–6 in particular need a quiet window. Doing this concurrently converts a mechanical refactor into a merge-conflict tax on everyone else.
4. **The review model depends on reading the ordering invariants together.** `commit_batch`, `commit_projection_outcomes`, `purge_inner`, and `excise_source_inner` all encode a drain→freeze→mutate ordering that is currently reviewable by scrolling one file. If the codex §9 gate or HITL sign-off depends on that co-location, steps 2 and 5 make review *harder*, not easier. Mitigation: extract the shared sequence into one named `pub(crate)` helper at the root so the ordering has a single definition instead of four. If that mitigation isn't taken, downgrade steps 2 and 5.
5. **There is no defect-risk justification here and the plan should never claim one.** El Emam found no threshold effect of class size; Yamashita found larger files have *lower* defect density. Splitting this file has **no predicted defect benefit**. The entire justification is navigability and ownership — being able to say "erasure lives here" and "the vec0 partition shape lives here". If the team does not currently feel that pain, **LEAVE the file alone** and take only §1 (the duplicate match arms) and step 1 (`test_hooks.rs`, which is a cohesion defect independent of file size). That is a complete and defensible outcome.

---

## 8. Legitimate stopping points

- **After step 1:** the one real cohesion defect is fixed. Cost: one small PR. If you do nothing else, do this.
- **After step 3:** `lib.rs` is ~14,850 and `impl Engine` is ~2,735. The "one type carries 140 responsibilities" finding is materially discharged; the three most self-contained capabilities own their own files. This is where the marginal value per unit of risk drops sharply.
- **After step 7:** ~20 files, largest ~1,110, root ~600. Nice, but the delta from step 3 to step 7 is navigability only, bought with the most interconnected and performance-sensitive code in the crate.
