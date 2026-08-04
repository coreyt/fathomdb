# Vertical review — per-file refactor proposals

66 violators reviewed by 38 agents. Verdicts: **59 LEAVE, 4 RESTRUCTURE-IN-PLACE, 2 SPLIT**.

An 89% LEAVE rate is the expected outcome, not a failure of the review. Phase 1's
verified research found no threshold effect of file size on defects, so most large
files here are long because their job is long. The signal is in the 7 exceptions.

## Actionable findings

### `scripts/commission-manifest.sh` — RESTRUCTURE-IN-PLACE (high confidence)

**Why it got big:** Embeds a 674-line Python program as a shell heredoc (lines 132–806), defeating IDE tooling support and preventing independent testing or reuse of the Python logic.

**Blast radius:** Low. Public API unchanged (callers invoke `scripts/commission-manifest.sh` with same arguments). Internal refactor only. Existing test suite (`scripts/tests/test_commission_manifest.sh`) exercises the script end-to-end and will verify correctness. No cross-language mirror or re-export risk.

| target | lines | ~LOC | what |
|:--|:--|--:|:--|
| `Extract Python logic to independent module` | 132–806 | 674 | Move the Python code block (currently a heredoc at lines 132–806) into a new file `scripts/commission-manifest.py`. This enables IDE syntax highlighting, linting, independent unit testing, and potential reuse by other Python code in the repo. |
| `Replace heredoc with thin wrapper call` | 132–806 | 3 | Replace the entire Python heredoc with a single call: `MODE="$MODE" RELEASE="$RELEASE" SLICE="$SLICE" exec python3 "$(dirname "$0")/commission-manifest.py"`. The shell wrapper (lines 100–131) retains responsibility for argument parsing and the python3 availability check; the extracted module handles all logic. |

### `tests/corpus/scripts/generate_chain_corpus.py` — RESTRUCTURE-IN-PLACE (high confidence)

**Why it got big:** Six chain builders (chain_email_note_todo through chain_paper_note_todo) repeat nearly identical templates: pick anchor → build synthetic docs → create queries → return dict, spanning ~350 LOC of systematic copy-paste.

**Blast radius:** Internal script; no external API. Tests depend on chain shapes (CHAIN_BUILDERS list), but shapes themselves are stable.

| target | lines | ~LOC | what |
|:--|:--|--:|:--|
| `Extract chain-builder factory` | L205-600 (the 6 chain_* functions) | 150 | Replace the 6 chain builders with a parameterized factory function that takes anchor spec, document sequence, and relation templates, then synthesizes docs and queries. Consolidate synth_doc call patterns and query-building logic into one place. This eliminates ~150 LOC of duplication while preserving the CHAIN_BUILDERS list. |

### `src/ts/tests/slice15d-projection-registry.test.ts` — RESTRUCTURE-IN-PLACE (high confidence)

**Why it got big:** 23 test methods (100% of tests) repeat an identical 4–5 line engine-lifecycle boilerplate pattern (freshDbPath, Engine.open, try/finally close), accounting for ~16% of file bulk.

**Blast radius:** None — internal test fixture, no public API or cross-file impact.

| target | lines | ~LOC | what |
|:--|:--|--:|:--|
| `Extract withEngine() fixture helper` | Add new helper after imports (ca. L24), before test suite begins | 8 | Lift the repeated try/finally engine-open/close sequence into a parameterized helper that accepts a test body function. Rewrite all 23 tests to call withEngine(async (engine, path) => { ... }). Reduces file size to ~430–450 LOC while maintaining test clarity and method lengths in the 10–25 LOC range. |

### `src/rust/crates/fathomdb-cli/src/lib.rs` — RESTRUCTURE-IN-PLACE (medium confidence)

**Why it got big:** Three genuinely distinct concerns are concatenated in one file: clap arg/subcommand struct definitions (~L26-395), verb dispatch + engine-error-to-exit-code mapping (~L396-830), and a long flat sequence of ~20 near-identical `*_report_json(report) -> Value` serializer functions (~L831-1025) that convert each typed engine report into its CLI JSON body.

**Blast radius:** none - internal move within the same crate; functions are private (fn, not pub fn) and only called from run_doctor/run_recover in the same crate, so a `mod report_json; use report_json::*;` keeps the public CLI surface identical.

| target | lines | ~LOC | what |
|:--|:--|--:|:--|
| `report_json.rs (new sibling module,`mod report_json;`)` | 817-1025 | 210 | Move the block of `*_report_json`/`section_json`/`finding_json`/`locator_json` functions (the report-to-JSON serialization layer) into their own module. Each function is small and self-contained with no interaction with the clap/dispatch code above it, so this is a clean topic boundary, not an arbitrary line cut. Leaves lib.rs at ~840 LOC (arg structs + dispatch + tests), comfortably under the soft threshold, and gives report-JSON shape changes an isolated, easy-to-diff home. |

### `src/rust/crates/fathomdb-py/src/lib.rs` — SPLIT (high confidence)

**Why it got big:** One PyO3 binding crate accreted eight distinct concerns (exception taxonomy + FFI validation, ~30 thin Py wrapper structs/getters, the PyEngine method surface, standalone lifecycle/read verbs, dict/batch write-translation, graph traversal + rerank, an embedder singleton, and module registration + tests) into a single file because #[pymodule] registration and #[pymethods] impls all had to live somewhere and nobody split them out as the surface grew feature-by-feature across 0.6.0-0.8.20.

**Blast radius:** Internal crate-private reorg only, no Python-facing API/ABI change: all types stay registered on the same `_fathomdb` pymodule, same class names. The `_fathomdb` pymodule-init fn (L2460-2548, stays in lib.rs) needs `use` imports across the new modules; the `mod tests` block (L2549-2678, stays in lib.rs) currently uses items in scope and needs qualified/re-exported paths after the split. The napi-rs mirror crate (src/rust/crates/fathomdb-napi/src/lib.rs, 2561 LOC) is a separate single-file crate the header comment says is kept in parity for FFI *semantics* (GIL-release, panic translation, string validation), not file layout, so this split does not need a matching napi-side split and does not desync anything documented.

| target | lines | ~LOC | what |
|:--|:--|--:|:--|
| `errors.rs` | 64-395 | 330 | The create_exception! taxonomy (root EngineError + ~20 concrete leaves), FFI string validation (validate_ffi_string, validate_ffi_string_py, extract_validated_str, extract_opt_validated_str), and Rust-error-to-PyErr conversion (engine_error_to_py, corruption_kind_str, open_stage_str, engine_open_error_to_py, corruption_to_py, call_engine). |
| `types.rs` | 396-1176 | 780 | All the small Py* data-wrapper structs and their #[pymethods]/getter impls: PyWriteReceipt, PyEraseReport, PyIngestWithExtractorReceipt, PyConsolidateReceipt, PySoftFallback, PyIdSpace, PySearchHit, PySearchResult, PyQueryTrace, PyPerHitExplain, PyExplanation, PyNodeRecord, PyReadView (+read_view_or_default), PyBoundaryCrossing, PyProjectionSpec, PyProjectionDelta, PyOpStoreRow, PyCounterSnapshot, PyMigrationStepReport, PyEmbedderIdentity, PyOpenReport, and embedder_event_to_py. |
| `engine.rs` | 1177-1568 | 392 | The PyEngine struct and its single #[pymethods] impl block (open/write/search/etc.) - the core engine method surface, already the single largest cohesive unit in the file. |
| `verbs.rs` | 1565-1895 | 330 | Standalone #[pyfunction]s hung off PyEngine: admin_configure, transition, purge, erase_source, configure_projections, read_projections, read_get, read_get_many, read_collection, read_mutations, read_collection_impl (+py_predicate_to_rust), read_list, read_list_filter (+py_filter_term_to_rust). |
| `translate.rs` | 1896-2096 | 200 | Write-batch translation from Python dicts to PreparedWrite: translate_batch, the dict_get/dict_str/dict_str_required/dict_source_id_required/dict_epoch_seconds helpers, translate_write_item, translate_node, translate_edge, translate_op_store, translate_admin_schema. |
| `graph.rs` | 2097-2361 | 265 | Graph-traversal and rerank surface: PyExpandedNode, PySearchExpandResult, parse_direction, graph_neighbors, crossed_boundary_since, search_expand (+dict_u64_required/dict_f64_required), rerank. |
| `embed.rs` | 2362-2459 | 98 | The CLS embedder singleton and its Python entry points: get_or_try_init, cls_embedder_singleton, embed_batch_cls, the two cfg-gated embed_batch_cls_impl variants, and force_panic_for_test. |

### `src/rust/crates/fathomdb-napi/src/lib.rs` — SPLIT (high confidence)

**Why it got big:** Sole source file of the fathomdb-napi crate accreted five structurally distinct FFI responsibilities — error-code translation, ~20 #[napi] DTO struct/impl conversions, the core Engine binding methods, the read/query surface, and JSON write-translation helpers — each independently understandable but bundled by history rather than by design.

**Blast radius:** None on the public API: napi-rs codegen (build.rs) scans every module in the crate regardless of file layout, so the generated index.d.ts / TS leaf-class surface (src/ts/src/binding.ts, index.ts, read.ts, release-surface.test.ts) is unaffected by an internal `mod` split as long as items stay pub(crate)/pub and are wired via `mod` declarations in lib.rs. No cross-language mirror (fathomdb-py, pyi) lives in this crate to keep in sync. Pure internal reorganization of a single-file crate; each target module is 300-790 lines, comfortably above the ~150-line floor, and each maps to one real responsibility (errors / DTOs / engine methods / read surface / write translation) — not over-splitting.

| target | lines | ~LOC | what |
|:--|:--|--:|:--|
| `errors.rs` | 58-103 (error-code consts) + 112-416 (TypedEnvelope/typed_error through call_engine_sync) | 360 | Error-code constants, TypedEnvelope/typed_error, validate_ffi_string(_napi), checked_ids_napi, engine_error_to_napi, corruption_kind_str/open_stage_str/corruption_to_napi, engine_open_error_to_napi, panic_error, call_engine/call_engine_sync — the FFI error-translation layer, self-contained and independently testable against the TS leaf-class table (AC-060a). |
| `types.rs` | 417-1207 | 790 | The repeated-pattern block of plain #[napi] DTO structs + impls converting Rust engine types to JS-visible shapes: WriteReceipt, EraseReport, ProjectionSpec/ProjectionDelta, IngestWithExtractorReceipt, ConsolidateReceipt, SoftFallback, IdSpace, SearchHit, NodeRecord, OpStoreRow, SearchFilterInput, SearchResult, QueryTrace, PerHitExplain (+ inline test mod 903-988), Explanation, MigrationStepReport, EmbedderIdentity, EmbedderEvent, OpenReport, CounterSnapshot, AttachSubscriberOptions, EngineConfig, EngineOpenOptions, AdminConfigureOptions. Homogeneous DTO-conversion responsibility, no engine-call logic. |
| `engine.rs` | 1208-1781 + 2477-2532 (test-support impl Engine) + 2533-2561 (mod tests) | 650 | struct Engine + the large impl Engine (519 lines, L1214-1732) holding the primary bound methods and admin_configure, plus force_panic_for_test / call_panicking_engine_for_test test-support additions and the crate's own unit test module — the actual engine-lifecycle binding surface. |
| `read.rs` | 1782-2176 | 395 | The read/query surface: ReadViewInput, BoundaryCrossing, ReadCollectionOptions, read_get/read_get_many/read_collection/read_mutations/read_collection_impl, PredicateInput + napi_predicate_to_rust, read_list, FilterTermInput + napi_filter_term_to_rust, read_list_filter, ExpandedNode, SearchExpandResult, parse_direction_napi, graph_neighbors, crossed_boundary_since, search_expand. |
| `translate.rs` | 2177-2476 | 300 | Pure JSON→PreparedWrite translation helpers: translate_batch, json_get/json_str(_required)/json_serialised(_required) and their _alt variants, json_source_id_required, translate_write_item, translate_node, json_i64(_alt), translate_edge, translate_op_store, translate_admin_schema. No napi-macro surface — ordinary Rust functions, easy to unit-test in isolation. |

## Cross-file observations

Surfaced by batching same-class files into one agent. No single-file review could see these.

### rust-test

eu7_real_corpus_ac.rs and corpus_harness.rs duplicate 76 LOC across four utility functions: SplitMix64 struct+impl (identical), synth_query() (identical), lead_sentence() (identical), and LEAD_MAX_CHARS constant. These should be extracted to a shared support module (e.g., support/query_synthesis.rs) and imported by both files. This is a single high-confidence cross-cutting extraction that reduces duplication without touching per-test granularity.

### ci-workflow

Both workflows duplicate identical TMPDIR configuration (4-line bash block), checkout action SHA, Rust toolchain setup, and rust-cache config. This boilerplate appears 7 times total across ci.yml (5 jobs) and perf-canonical.yml (2 jobs). Extracting into a reusable composite action `.github/actions/setup-fathomdb-env/action.yml` would reduce duplicated setup by ~50–60 LOC, establish a single source of truth for toolchain pinning, and improve maintenance. This cross-cutting extraction is worth more than any per-file split, since each file's length is inherent to its job (ci.yml = manifest of parallel gates; perf-canonical.yml = two substantial measurement workflows).

### py-test

These five test files target different systems (embedder probe, gap decomposition, M1 verdict, read view SDK, R2 harness) with zero cross-file setup duplication. Each file's length is inherent to its system's multi-scenario or cross-product test matrices. Per-test methods are measured-reasonable (max 69 LOC), and helper fixtures are system-specific (FakeEncoder, _RecClient, etc.) with no reuse across files. The high LEAVE rate is expected for soft-threshold advisory hits.

### shell — DOC-HYGIENE-2 gate scripts

These 7 files are DOC-HYGIENE-2 gates (T1b-T1e, T2a, T3a) created to mechanically prevent recurring failures by encoding measured remedies into tooling. They share patterns (cd to toplevel, tool-presence checks, structured error reporting, exit-code semantics) but NOT code. Each is independently deployable and deliberately self-contained: check-release-state-views, check-governed-surface-pin, and check-ledgers embed Python; lint-plan-anchors embeds Perl; preflight and steward-orient orchestrate multiple checks; set-version manages version axes. No shared harness, no copy-pasted logic, no duplication. The standing rule ("fix tooling so it cannot recur") applies: each script is a specialized gate that precisely documents its own failure mode and remedy, making it independently understandable and auditable. Extracting a common bash library would save 5-10 lines per script at the cost of added complexity and indirection with minimal gain.

### shell — release scripts

Both scripts redefine `read_crate_version()` (~12 LOC each) and `registry_has_version()` (~20–42 LOC each) with nearly identical curl+jq patterns and AWK parsing. This suggests a future extraction target: a shared `scripts/release/_common.sh` library. However, neither script is large enough to mandate it yet. The duplication is modest in absolute terms, and extracting introduces a cross-script dependency. This is an *architectural improvement* deferred, not a structural defect of either file.

### plan-status-doc

All three are historical release-planning documents already correctly implementing archive-and-link: each is marked CLOSED at the header, explicitly points to a newer current version (STATUS-0.8.20.md + 0.8.6-0.8.16-PROGRAM-SEQUENCING.md as master schedule-of-record), and serves as an authoritative reference contract for a closed release cycle. No shared duplication, copy-pasted harness, or cross-cutting extraction opportunity exists — they are distinct releases. The pattern is correct; these files need no restructuring.

### py-eval

All 5 files successfully reuse BudgetLedger and pricing infrastructure from gap_decomposition_run (working shared pattern). Three files (gap_decomposition, fracc_voi, autoe_pilot) implement similar checkpoint/resume patterns for resilience, but their checkpoint schemas are domain-specific: gap_decomposition tracks reader + mode, fracc_voi tracks cost dict separately, autoe_pilot persists judgment keys. Extracting a generic checkpoint framework would require config abstraction over these schemas, adding complexity for modest LOC savings (~30 per file). Per the py-eval doctrine, these are run-once experiment pipelines whose value is reproducibility of the specific execution; the checkpoint-pattern repetition is acceptable because: (1) each schema is slightly different, (2) total duplication is only ~30 LOC per file, (3) framework overhead would increase cognitive load when reading the run record. Recommendation: accept the duplication; the shared BudgetLedger infrastructure is the right level of extraction.

### shell-test

All three files duplicate lightweight test infrastructure (~40 lines per file): identical pass()/fail()/FAILED counter, cleanup() trap pattern, output-capture wrappers, and assertion helpers. Extraction to a shared lib would save ~90 lines but introduce an import dependency and cannot standardize custom assertions (expect_routes_to_hitl). Duplication is real but lightweight, stable, and unlikely to diverge — not a blocking issue. However, this pattern should be codified for future test suites to avoid recurring duplication.

### py-src

These files do not share duplication across them. They occupy different directories and serve distinct purposes (SDK wrapper, type catalog, corpus generation, experiment orchestration, experiment utilities). The only significant duplication is internal to generate_chain_corpus.py, where 6 chain builders repeat nearly identical templates. The other files' sizes are inherent to their responsibilities; splitting would harm discoverability (types.py), reproducibility (gating-rerun.py), or cohesion (engine.py, _lib.py).

### rust-src

Only one file was in scope this round, so no cross-file duplication could be directly observed. But the extracted concern (typed-engine-report -> serde_json::Value mapping, one function per report type, each emitting a "verb": "<name>" discriminator) is exactly the kind of mechanical, repeated shape that tends to recur wherever the CLI or an adjacent HTTP/RPC-facing crate re-serializes the same fathomdb engine report types (IntegrityReport, RebuildReport, ExciseReport, etc.) for a different transport. Worth a repo-wide grep for other *_report_json / report -> Value mappers before deciding whether report_json.rs should become a small shared crate rather than a cli-local module.

## LEAVE verdicts

| file | why it stays |
|:--|:--|
| `.github/workflows/ci.yml` | Manifests 17 independent parallel CI gates (filter, verify, security, tests, matrices, metadata jobs); the length reflects job count and variety, not any single over-long function. |
| `.github/workflows/perf-canonical.yml` | Two substantial workflow_dispatch jobs with complex setup; ac012-canonical-measure and perf-experiment are self-contained and serve distinct measurement roles. |
| `.github/workflows/release.yml` | This is a monolithic release pipeline (verify → build → publish-tiers → post-publish) where each job has a specific role in a sequential dependency chain; splitting fragments the narrative and doesn't reduce the inherent repetitio |
| `dev/agent-tools/ledgerwatch/ledgerwatch.py` | The file is a complete CLI tool that implements three interchangeable file-watching strategies and four command modes (validate, project, prune) sharing a unified state model and rendering layer; the longest functions (183-line CL |
| `dev/agent-tools/ledgerwatch/test_ledgerwatch.py` | Single tool's cohesive test suite organized by operational feature (100 tests across JSONL/Markdown/diff strategies, state handling, modes, validation, and project/fold modes); per-test length all healthy (5–24 LOC, median 10 LOC) |
| `dev/corpus-creation/architecture.md` | File documents 12 distinct data sources with individual pinning, license, and acquisition logic, plus 4 validation packs and sequencing rules — inherent complexity for a canonical operational reference. |
| `dev/design/0.7.0-vector-quant-pack1.md` | Design-decision memo for binary quantization Pack 1, structured as eight interrelated decisions; largest section documents complex SQL migration with atomicity and recovery. |
| `dev/design/0.8.1-graph-experiment-plan.md` | Experiment plan with methodology, program structure, statistical math, cost forecasting, and live state; math sections intertwined with design decisions. |
| `dev/design/embedder.md` | Comprehensive reference on embedder subsystem design covering ten distinct concerns (mean-centering, loader, HTTP, auth, cache, atomicity, verification, timing, endianness, failures, concurrency). |
| `dev/design/gpu-device-allocation-policy.md` | Design-on-spec proposal following standard design-evaluation pattern; research sections grounded in prior-art and genuine unknowns. |
| `dev/design/orchestration.md` | Canonical release-independent orchestration runbook; sections are tightly interdependent (roles require state spine, state spine requires decision loop). |
| `dev/notes/performance-whitepaper-notes.md` | Performance investigation whitepaper spanning 6+ experimental phases (Pack 4-6.G) with chronological evidence narrative that builds from research summary → hypothesis falsification → final synthesis. |
| `dev/plans/0.6.0-implementation.md` | Implementation plan for older release organized by phases (5–12) as logical units, with per-phase execution posture and a detailed slice-sequence table; phases are structural, not arbitrary. |
| `dev/plans/0.8.0-implementation.md` | Contains detailed contracts for 9 slices (0, 5, 10, 15, 20, 25, 30, 35, 40) with per-slice specifications (~140–170 LOC each) PLUS universal orchestration rules (§1–§12) that must be co-located for reference completeness. |
| `dev/plans/runs/0.8.4-gating-rerun.py` | Run-once experiment orchestration script with tightly coupled concerns (spending metering, answer generation, judging, result serialization); functions maintain shared state (CFG, _spent dicts) and are called sequentially. |
| `dev/plans/runs/STATUS-0.8.0.md` | Complete historical decision ledger (73 dated entries, 1593 LOC) for a ~8-day 0.8.0 release phase; size is inherent to the file's purpose as an append-only historical record. |
| `dev/plans/runs/STATUS-0.8.1.md` | Live status board (now archived) whose §7 'Recent decisions' ledger (lines 413–1162) is append-only, capturing the chronological spine of HITL decisions and experiment outcomes; earlier sections reference into this ledger. |
| `experiments/_lib.py` | Utility library for experiment tracking (mirrored from memex); three concerns (config schemas, record I/O, index management) all belong together; write_record (117 LOC) is a single end-to-end pipeline that cannot split without thr |
| `scripts/check-governed-surface-pin.sh` | Embeds Python validator checking pinned file against pre-signed spec on 4 axes: hash, member lists, counts, REQ-054 denylist. |
| `scripts/check-ledgers.sh` | Embeds Python gate validating ledger integrity (sidecar-agreement + seq contiguity) for all discovered *.jsonl.seq sidecars. |
| `scripts/check-release-state-views.sh` | Embeds complete Python state-file renderer + validator in here-doc with 4 renderers and orphan-marker scanning loop. |
| `scripts/lint-plan-anchors.sh` | Enforces line-anchor ban via RULE 1 (shape scan) + RULE 2 (symbol existence) plus wrap-aware citation extraction via embedded Perl. |
| `scripts/preflight.sh` | Orchestrator gate running 9 independent health checks (mid-merge, stale-base, disk, landing-mode, board-currency, ledger, surface, etc.) |
| `scripts/release/cargo-publish-if-new.sh` | Substance-driven growth: workspace/crate version resolution, registry idempotency check with split-brain guard, dry-run conditional for dependent crates, each load-bearing to the script's mission. |
| `scripts/release/verify-embedder-api-no-drift.sh` | Only 7 LOC over threshold; normalize_src (17 LOC) is domain-specific source normalization for diff, appropriate length and necessary. |
| `scripts/set-version.sh` | check_files() at 167 LOC is unified version-consistency check across workspace, Python, npm, platform packages as single semantic operation. |
| `scripts/steward-orient.sh` | Stateless briefing aggregating 6+ sources (release, worktrees, ledgers, todos) into unified output under 4 KB budget + strict no-write contract. |
| `scripts/tests/test_check_governed_surface_pin.sh` | Only 8% over soft threshold (558 vs 500 LOC); length justified by comprehensive edge-case coverage including three distinct pin-validation backstop attacks (omitted count, mistyped count, killer-drop-count combo). |
| `scripts/tests/test_check_release_state_views.sh` | Comprehensive RED-first specification suite with 25+ test arms (5–25 lines each) covering every error path of the release-state-views gate, not a single oversized component. |
| `scripts/tests/test_commission_manifest.sh` | Exhaustive fixture-based regression suite for commission-manifest.sh generator: 40+ test arms (each 8–29 lines) testing distinct failure modes, with a necessarily monolithic 228-line fixture setup that builds a complete fake repo. |
| `scripts/tests/test_steward_orient.sh` | Unified proof of the steward-orient briefing contract: ~150 lines of fixture infrastructure plus 12 test arms (5–30 lines each), with longer arms deliberately exercising .git/index locking and stale-stat-cache behavior. |
| `src/python/eval/autoe_pilot_run.py` | run_pilot (259 LOC) is the largest function: it orchestrates sampling, building answers, resilient judging, cost projection, and win-rate aggregation as a single pass. The phases are logically distinct but all feed one output repo |
| `src/python/eval/fracc_voi_run.py` | main() (142 LOC) orchestrates five named deliverables (ce_guard, stage_a, margins, vos, grid, asym) where each represents one experimental phase with distinct outputs. Phases are sequential but interdependent; extracting them woul |
| `src/python/eval/gap_decomposition_run.py` | run_gap_decomposition (243 LOC) is a single coherent orchestration loop iterating over queries with integrated oracle context retrieval, new-arm answering, delta computation, and checkpoint persistence — not multiple extractable s |
| `src/python/eval/m1_baseline.py` | BGEEncoder (94 LOC) and FusedPoolReranker (69 LOC) are essential classes required by the justified-deviation architectural decision: dense retrieval must be in-harness because the engine does not expose vector-kind registration, f |
| `src/python/eval/p0a_base_retrieval.py` | Comprises pure scorer functions (hit_at_k, reciprocal_rank, ndcg_at_k, aggregate) deliberately co-located with orchestration loops. Module docstring emphasizes these scorers are TDD'd separately in tests/test_p0a_scorer.py; co-loc |
| `src/python/eval/r2_parity_eval.py` | This is a complete py-eval pipeline (1494 LOC) integrating data models, answerer protocol, four retrieval adapters, scoring logic, and two CLI entry points into a single reproducible R2 parity evaluation run, with well-marked sect |
| `src/python/eval/s15a_embedder_probe.py` | File implements a complete frozen measurement pipeline (15a embedder-ceiling probe); size inherent to the job, not bloat |
| `src/python/fathomdb/_fathomdb.pyi` | .pyi stub files are inherently verbose because they faithfully mirror the Rust binding's entire public surface (27 exception types + 20+ Engine methods + 10+ data classes), and splitting would fragment the logical unit that type c |
| `src/python/fathomdb/engine.py` | The Engine class wraps the public five-verb API surface defined by dev/interfaces/python.md; all verbs (write, transition, purge, erase_source, configure_projections) plus read utilities must co-locate in one public-facing wrapper |
| `src/python/fathomdb/types.py` | Public API type catalog (~30 frozen dataclasses, TypedDicts, functions) defined in dev/interfaces/python.md; each type is short (10–45 LOC); cohesion is by role (all are public API types), not length. |
| `src/python/tests/test_gap_decomposition_run.py` | Tests gap-decomposition budget safety, distiller constraints, oracle completeness, checkpointing, and resumption across multiple modes. |
| `src/python/tests/test_m1_verdict.py` | Tests M1 verdict harness across 5-arm pipelines, endpoint computation, decision rules, stage-1/2 gates, and multi-run resume semantics. |
| `src/python/tests/test_r2_harness.py` | Tests R2 parity evaluation harness contract and corpus validity across multiple adapter integration paths; marginally over soft threshold. |
| `src/python/tests/test_s15a_embedder_probe.py` | Tests a complex multi-scenario embedder probe wiring across model selection, determinism, cache behavior, and integration markers. |
| `src/python/tests/test_slice10_read_view.py` | Tests R-20-RV read view across all 5 verbs and multiple orthogonal flags (include_superseded, include_inactive, valid_as_of) forming complex assertion matrices. |
| `src/rust/crates/fathomdb-embedder/tests/cross_backend_calibration.rs` | File is 824 LOC (24 above soft threshold), but this is a cohesive measurement instrument with properly-factored helpers and no duplicated setup; per-test methods (50–85 LOC) are individually short. |
| `src/rust/crates/fathomdb-engine/tests/eu7_real_corpus_ac.rs` | Single monolithic benchmark/measurement harness (307 LOC) that runs correlated phases sequentially; splitting breaks test coherence and measurement validity. |
| `src/rust/crates/fathomdb-engine/tests/ir_c_gold_diagnostics.rs` | Main test (173 LOC) plus nested module with 8 focused unit tests; structure is clean; no per-test threshold violation. |
| `src/rust/crates/fathomdb-engine/tests/perf_gates.rs` | The file organizes 15 semantically related performance gates (AC-012, AC-013, AC-013b, AC-017–AC-020, G.3.5, A.3.x) that share deterministic synthetic data generation, embedders, measurement utilities, and documented context acros |
| `src/rust/crates/fathomdb-engine/tests/slice15_byo_llm_ingest.rs` | 23 independent conformance tests covering 8 acceptance criteria; all per-test lengths <87 LOC; file size reflects appropriate test count, not per-test bloat. |
| `src/rust/crates/fathomdb-engine/tests/slice15d_projection_registry.rs` | Comprehensive correctness test suite for projection registry (24 tests × 3 feature areas: registry semantics, EAV/property-FTS, lifecycle gates) with shared fixture library; per-test methods all ≤91 LOC, well within measured cogni |
| `src/rust/crates/fathomdb-engine/tests/slice15e_prekn_filterable.rs` | 11 tests, one at 131 LOC documents four load-bearing reshape invariants; splitting would fragment a cohesive contract verifying byte-safe re-insertion. |
| `src/rust/crates/fathomdb-engine/tests/support/corpus_harness.rs` | Single reusable CorpusFixture type with cohesive impl (327 LOC); this is a library fixture, not a test file; impl methods are related behaviors of one logical entity. |
| `src/rust/crates/fathomdb-engine/tests/support/ir_eval.rs` | Public IR evaluation library (not test file); type defs + parsing + evaluation logic; no function exceeds ~80 LOC; appropriate scope for reference documentation + API. |
| `src/rust/crates/fathomdb-engine/tests/vector_equivalence_probe.rs` | 21 tests probing embedder identity + mean-centering paths; longest is 78 LOC; per-test granularity is sound. |
| `src/ts/src/binding.ts` | File bundles runtime binding resolution (9-11 LOC functions) with 39 FFI interface definitions (462 LOC) that document what the native napi-rs binding exports. |
| `src/ts/src/index.ts` | The file is the TypeScript SDK public surface, necessarily exporting ~300 LOC of type contracts + the monolithic Engine class facade (26 methods) + admin/graph verbs, with no internal fragmentation. |
| `src/ts/tests/slice10-read-view.test.ts` | Cross-binding parity test harness for R-20-RV/RV surface spanning 5 read verbs × 4 view permutations × 2 validity-window axes, with shared oracle helpers. Per-test length (20–106 LOC) is reasonable; longest test justified by nativ |
