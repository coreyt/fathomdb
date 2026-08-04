# File-size heuristic violations

Scanned 1369 classified files. 66 violate a threshold (16 hard, 50 soft).
556 files excluded by rule; 44 unclassified.

## shell — soft 200 / hard 500 (10 violators)

| LOC | severity | band | file |
|---:|---:|:--|:--|
| 806 | 1.61 | hard | `scripts/commission-manifest.sh` |
| 464 | 0.93 | soft | `scripts/check-release-state-views.sh` |
| 456 | 0.91 | soft | `scripts/set-version.sh` |
| 422 | 0.84 | soft | `scripts/steward-orient.sh` |
| 397 | 0.79 | soft | `scripts/check-governed-surface-pin.sh` |
| 283 | 0.57 | soft | `scripts/preflight.sh` |
| 278 | 0.56 | soft | `scripts/lint-plan-anchors.sh` |
| 273 | 0.55 | soft | `scripts/check-ledgers.sh` |
| 241 | 0.48 | soft | `scripts/release/cargo-publish-if-new.sh` |
| 207 | 0.41 | soft | `scripts/release/verify-embedder-api-no-drift.sh` |

## rust-test — soft 800 / hard 1200 (10 violators)

| LOC | severity | band | file |
|---:|---:|:--|:--|
| 1581 | 1.32 | hard | `src/rust/crates/fathomdb-engine/tests/perf_gates.rs` |
| 1277 | 1.06 | hard | `src/rust/crates/fathomdb-engine/tests/slice15d_projection_registry.rs` |
| 1189 | 0.99 | soft | `src/rust/crates/fathomdb-engine/tests/slice15_byo_llm_ingest.rs` |
| 1138 | 0.95 | soft | `src/rust/crates/fathomdb-engine/tests/eu7_real_corpus_ac.rs` |
| 1052 | 0.88 | soft | `src/rust/crates/fathomdb-engine/tests/slice15e_prekn_filterable.rs` |
| 1014 | 0.84 | soft | `src/rust/crates/fathomdb-engine/tests/vector_equivalence_probe.rs` |
| 901 | 0.75 | soft | `src/rust/crates/fathomdb-engine/tests/ir_c_gold_diagnostics.rs` |
| 861 | 0.72 | soft | `src/rust/crates/fathomdb-engine/tests/support/corpus_harness.rs` |
| 826 | 0.69 | soft | `src/rust/crates/fathomdb-engine/tests/support/ir_eval.rs` |
| 824 | 0.69 | soft | `src/rust/crates/fathomdb-embedder/tests/cross_backend_calibration.rs` |

## py-eval — soft 800 / hard 1200 (7 violators)

| LOC | severity | band | file |
|---:|---:|:--|:--|
| 1562 | 1.30 | hard | `src/python/eval/s15a_embedder_probe.py` |
| 1494 | 1.25 | hard | `src/python/eval/r2_parity_eval.py` |
| 1093 | 0.91 | soft | `src/python/eval/gap_decomposition_run.py` |
| 948 | 0.79 | soft | `src/python/eval/m1_baseline.py` |
| 918 | 0.77 | soft | `src/python/eval/fracc_voi_run.py` |
| 873 | 0.73 | soft | `src/python/eval/p0a_base_retrieval.py` |
| 870 | 0.72 | soft | `src/python/eval/autoe_pilot_run.py` |

## design-doc — soft 700 / hard 1100 (6 violators)

| LOC | severity | band | file |
|---:|---:|:--|:--|
| 1665 | 1.51 | hard | `dev/notes/performance-whitepaper-notes.md` |
| 854 | 0.78 | soft | `dev/design/embedder.md` |
| 760 | 0.69 | soft | `dev/design/0.7.0-vector-quant-pack1.md` |
| 746 | 0.68 | soft | `dev/design/orchestration.md` |
| 726 | 0.66 | soft | `dev/design/0.8.1-graph-experiment-plan.md` |
| 712 | 0.65 | soft | `dev/design/gpu-device-allocation-policy.md` |

## py-src — soft 500 / hard 800 (6 violators)

| LOC | severity | band | file |
|---:|---:|:--|:--|
| 878 | 1.10 | hard | `dev/agent-tools/ledgerwatch/ledgerwatch.py` |
| 692 | 0.86 | soft | `src/python/fathomdb/engine.py` |
| 678 | 0.85 | soft | `tests/corpus/scripts/generate_chain_corpus.py` |
| 645 | 0.81 | soft | `dev/plans/runs/0.8.4-gating-rerun.py` |
| 571 | 0.71 | soft | `src/python/fathomdb/types.py` |
| 500 | 0.62 | soft | `experiments/_lib.py` |

## py-test — soft 600 / hard 1000 (6 violators)

| LOC | severity | band | file |
|---:|---:|:--|:--|
| 1061 | 1.06 | hard | `dev/agent-tools/ledgerwatch/test_ledgerwatch.py` |
| 967 | 0.97 | soft | `src/python/tests/test_s15a_embedder_probe.py` |
| 834 | 0.83 | soft | `src/python/tests/test_gap_decomposition_run.py` |
| 688 | 0.69 | soft | `src/python/tests/test_m1_verdict.py` |
| 675 | 0.68 | soft | `src/python/tests/test_slice10_read_view.py` |
| 607 | 0.61 | soft | `src/python/tests/test_r2_harness.py` |

## rust-src — soft 1000 / hard 2000 (4 violators)

| LOC | severity | band | file |
|---:|---:|:--|:--|
| 17910 | 8.96 | hard | `src/rust/crates/fathomdb-engine/src/lib.rs` |
| 2678 | 1.34 | hard | `src/rust/crates/fathomdb-py/src/lib.rs` |
| 2561 | 1.28 | hard | `src/rust/crates/fathomdb-napi/src/lib.rs` |
| 1050 | 0.53 | soft | `src/rust/crates/fathomdb-cli/src/lib.rs` |

## plan-status-doc — soft 1100 / hard 1800 (4 violators)

| LOC | severity | band | file |
|---:|---:|:--|:--|
| 2087 | 1.16 | hard | `dev/plans/runs/STATUS-0.8.0.md` |
| 1650 | 0.92 | soft | `dev/plans/0.8.0-implementation.md` |
| 1162 | 0.65 | soft | `dev/plans/runs/STATUS-0.8.1.md` |
| 1120 | 0.62 | soft | `dev/plans/0.6.0-implementation.md` |

## shell-test — soft 500 / hard 800 (4 violators)

| LOC | severity | band | file |
|---:|---:|:--|:--|
| 882 | 1.10 | hard | `scripts/tests/test_commission_manifest.sh` |
| 716 | 0.90 | soft | `scripts/tests/test_check_release_state_views.sh` |
| 645 | 0.81 | soft | `scripts/tests/test_steward_orient.sh` |
| 558 | 0.70 | soft | `scripts/tests/test_check_governed_surface_pin.sh` |

## ci-workflow — soft 300 / hard 600 (3 violators)

| LOC | severity | band | file |
|---:|---:|:--|:--|
| 602 | 1.00 | hard | `.github/workflows/release.yml` |
| 580 | 0.97 | soft | `.github/workflows/ci.yml` |
| 340 | 0.57 | soft | `.github/workflows/perf-canonical.yml` |

## ts-src — soft 500 / hard 1000 (2 violators)

| LOC | severity | band | file |
|---:|---:|:--|:--|
| 1300 | 1.30 | hard | `src/ts/src/index.ts` |
| 518 | 0.52 | soft | `src/ts/src/binding.ts` |

## ts-test — soft 500 / hard 750 (2 violators)

| LOC | severity | band | file |
|---:|---:|:--|:--|
| 755 | 1.01 | hard | `src/ts/tests/slice10-read-view.test.ts` |
| 630 | 0.84 | soft | `src/ts/tests/slice15d-projection-registry.test.ts` |

## doc-other — soft 700 / hard 1100 (1 violators)

| LOC | severity | band | file |
|---:|---:|:--|:--|
| 758 | 0.69 | soft | `dev/corpus-creation/architecture.md` |

## py-stub — soft 500 / hard 800 (1 violators)

| LOC | severity | band | file |
|---:|---:|:--|:--|
| 536 | 0.67 | soft | `src/python/fathomdb/_fathomdb.pyi` |

## Over threshold but EXCLUDED by rule

These exceed their class threshold and are deliberately not violators. Listed so the exemption is visible rather than silent.

| LOC | would have been | file | why exempt |
|---:|:--|:--|:--|
| 1305 | hard | `dev/acceptance.md` | governance-locked specification: recorded as locked with no per-feature ACs. A file that cannot be edited by policy cannot be a length violation; splitting it is a governance decision, not a hygiene finding |
| 1151 | soft | `dev/archive/hitl-queue.md` | archived/frozen: Nygard's ADR practice is to KEEP superseded records and mark them, not edit them |

## Unclassified (no rule matched — review the ruleset)

- `.github/dependabot.yml`
- `.gitignore`
- `.markdownlint-cli2.jsonc`
- `.markdownlint.jsonc`
- `.prettierignore`
- `Cargo.toml`
- `LICENSE`
- `deny.toml`
- `dev/experiments/ruff.toml`
- `dev/plans/runs/0.8.0-slice-plan.wf.js`
- `dev/plans/runs/agent-memory-impl-strategy.wf.js`
- `dev/plans/runs/v05-feature-triage.wf.js`
- `dev/tools/mermaid/.gitignore`
- `lychee.toml`
- `mkdocs.yml`
- `rust-toolchain.toml`
- `rustfmt.toml`
- `scripts/hooks/pre-commit`
- `scripts/hooks/pre-push`
- `scripts/repo-prune/backups/.gitignore`
- `src/python/pyproject.toml`
- `src/rust/crates/fathomdb-cli/Cargo.toml`
- `src/rust/crates/fathomdb-embedder-api/Cargo.toml`
- `src/rust/crates/fathomdb-embedder/Cargo.toml`
- `src/rust/crates/fathomdb-engine/Cargo.toml`
- `src/rust/crates/fathomdb-napi/Cargo.toml`
- `src/rust/crates/fathomdb-py/Cargo.toml`
- `src/rust/crates/fathomdb-query/Cargo.toml`
- `src/rust/crates/fathomdb-schema/Cargo.toml`
- `src/rust/crates/fathomdb-schema/migrations/001_bootstrap.sql`
- `src/rust/crates/fathomdb-schema/migrations/002_canonical.sql`
- `src/rust/crates/fathomdb-schema/migrations/003_embedder_profiles.sql`
- `src/rust/crates/fathomdb-schema/migrations/004_op_store.sql`
- `src/rust/crates/fathomdb-schema/migrations/005_search_index.sql`
- `src/rust/crates/fathomdb-schema/migrations/006_vector_runtime.sql`
- `src/rust/crates/fathomdb-schema/migrations/007_projection_terminal.sql`
- `src/rust/crates/fathomdb-schema/migrations/008_source_id.sql`
- `src/rust/crates/fathomdb-schema/migrations/009_vector_binary_quant.sql`
- `src/rust/crates/fathomdb-schema/migrations/011_search_index_tokenizer.sql`
- `src/rust/crates/fathomdb-schema/migrations/013_op_store_collection_index.sql`
- `src/rust/crates/fathomdb/Cargo.toml`
- `tests/corpus/scripts/configs/acquire-compmix.yaml`
- `tests/corpus/scripts/configs/acquire-wec-eng.yaml`
- `tools/docs/.gitignore`
