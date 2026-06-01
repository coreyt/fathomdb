# Changelog

Release notes for the FathomDB engine, Python SDK, and TypeScript SDK.
Cuts follow the version tagged on `0.6.0-rewrite`. Each released section
MUST list every removed public symbol under a `### Removed` heading;
the removal-detect linter (`scripts/security/check-removal-changelog.sh`,
AC-050c) gates merges against this invariant.

## [Unreleased]

### Removed

(none yet)

## 0.7.2 - unreleased (IN PROGRESS)

RELEASE-HARDENING. The campaign that takes the held 0.7.0 + 0.7.1 work to a
pushable release: a doc-drift sweep, embedder concurrency hardening, a recall-
floor reframe (the "recall gap" was a measurement artifact, not a defect), and
a tiered real-corpus latency budget. **Not released. In progress** — Phase A
slices PR-1, PR-2(family), PR-9, PR-3 have landed on local `main` (unpushed);
**PR-4 (the slice that writes the final release notes, creates the `v0.7.1`
tag, and pushes `main` + both tags) has NOT run.** PR-5/6/7/8 (test/perf
hardening + campaign closure) are NOT started. Both `v0.7.0` and `v0.7.1` remain
held locally until PR-4. Ledger: `dev/plans/runs/STATUS-release-hardening.md`.

### Added

- **Embedder concurrency hardening (PR-9, `21f4df6`).** Closes the two
  robustness items EU-5f surfaced on the projection embed path:
  - **Invariant-5 per-embed watchdog.** `embed_with_watchdog` runs each embed
    on a detached thread under a deadline (30 s default, configurable); a hung
    embed surfaces `RuntimeEmbedderError::Timeout` into the existing retry/
    failure path instead of parking a projection worker. Panic-transparent
    (preserves the EU-5f `ProjectionPanic` path).
  - **Engine-side embed serialization.** An `embed_serialize` guard invokes the
    shared `Arc<dyn Embedder>` one call at a time. Justified on **safety**
    (caller-supplied pyo3/napi embedders are `Sync`-only by contract and may not
    be concurrency-safe), not throughput — it is throughput-neutral on the
    candle default (candle uses one process-wide rayon pool; the earlier "~13×"
    throughput rationale was withdrawn).
  - **Circuit breaker.** `live_embed_threads` counts watchdog threads currently
    alive; at `embed_circuit_threshold` (8) the breaker latches and jobs fail
    fast without spawning, bounding the abandoned-thread leak that Invariant-5's
    no-abort rule makes unavoidable. Keyed on *concurrent* live threads (not a
    consecutive-timeout streak) so it bounds intermittent hangs and self-clears
    for slow-but-returning embeds.
  - codex: 5 passes, BLOCK→PASS (the pass-4 BLOCK on the original consecutive-
    timeout breaker design was not overridden — it was redesigned to the live-
    thread-count form above).
- **Per-push read-path smoke (PR-3, `d9f9b65`).** New always-on,
  fixture-independent canary `perf_gates::ac_013_vector_read_path_smoke`: an
  exact-match sentinel must rank 1 through the two-phase bit-KNN + f32 rerank
  path. No `AGENT_LONG`, `default-embedder` feature, or corpus needed — this is
  the CI guard that replaces the infeasible canonical N=1M perf run.

### Changed

- **Recall floor reframe — 0.90 HOLDS, now correctly validated (PR-2 family,
  `78164b9`/`a154037`).** The AC-013b recall@10 "gap" measured at 0.7.1
  (0.828) was a **measurement artifact** — exclude-after-top-10 plus
  body-string ground truth over a corpus with ~5.6 % duplicate bodies — **not**
  an engine deficiency. The corrected ANN-fidelity measurement (exclude the
  query-source doc *before* top-10; dedup-by-id GT) on the real default embedder
  (bge-small, N=7,667, K=192, mean-centering) is **recall@10 = 0.937 (CI
  0.913–0.957, σ=0.0116)**; the full CI clears the 0.90 floor, so the floor is
  **kept at 0.90, not raised**. This number is **ANN/quantization fidelity**
  (how faithfully the 1-bit sign-quant index reproduces the same model's exact
  f32 top-10), **not IR relevance**. The separate embedder IR-relevance ceiling
  (recall@10 ≈ 0.571, CI 0.530–0.614, on 301 labeled queries) is **not a gate**.
  `ADR-0.7.0-vector-binary-quant.md` § 2 point 4 was amended to cite the
  corrected measurement; a fast sentinel `ac_013b_floor_matches_adr` pins the
  test constant to the ADR. Evidence: `dev/plans/runs/0.7.2-PR-2c-recall-rootcause.md`,
  `dev/plans/runs/0.7.2-PR-3-perf-data.md`.
- **Tiered AC-013 / AC-019 latency budget (PR-3, `d9f9b65`; HITL 2026-06-01).**
  The vec0 bit-KNN candidate stage is a per-query **O(N) linear scan** (no ANN
  index), so a single N-independent latency budget is not meaningful. The budget
  is now **tiered**: the **10,000-row tier is the binding release gate for the
  0.x and 1.x lines** (`AC013_GATE_N = 10000`; AC-013 asserts 80/300 ms only at
  `n ≤ gate`, reports above it). The **100k and 1M tiers are tracked targets**
  for post-1.0 (pre-2.1) ANN-index work, not gated. `ADR-0.7.0-text-query-
  latency-gates-revised.md` was amended with the tiered table and HITL-locked.
- **Latency/recall measurement is now LOCAL once-per-release** (PR-3). Real-
  embedder canonical N=1M is infeasible on CI (~166 h seed at the PR-9-measured
  1.67 docs/s vs a 240-min workflow timeout; the synthetic 1M seed also did not
  drain in 3 h locally). Heavy measurement runs locally; CI carries only the
  read-path smoke above.
- **Synthetic AC-019 perf gate is now REPORT-ONLY** (PR-3;
  `perf_gates::ac_019_mixed_retrieval_stress_workload_tail`,
  `AC019_REPORT_ONLY`). The synthetic isotropic fixture cannot meet the
  `max(baseline_p99 × 10, 150 ms)` bound — a property of the synthetic data
  (instant embed → unrealistically fast baseline → too-tight 10× bound), not the
  engine. The asserting AC-019 signal lives in the real-corpus harness
  `eu7_real_corpus_ac.rs`, which PASSES at the 10k tier (clean run 343 ms <
  405 ms bound). AC-013 keeps its hard 10k-tier gate.
- **Architecture/design/ADR docs aligned to shipped 0.7.x reality (PR-1,
  `aebf959`).** Doc-drift sweep: 10 HITL-approved corrections across design,
  architecture, and ADR docs. Docs-only.

### Deferred

- **Automatic in-ingest mean-drift detector → 0.8.x** (PR-2 family,
  `64f72e0`/`2ef8c3d`). The adaptive mean-recompute drift detector was built and
  ratified, then **carved out** because its sole justification (recall) collapsed
  with the measurement-artifact finding and its benefit is unmeasured. It is
  parked for 0.8.x behind a RED guard (`dev/plans/prompts/0.8.x-auto-mean-drift-
  DEFERRED.md`), not silently dropped. The **manual doctor verb** (operator-
  triggered mean recompute) ships.
- **AC-013/AC-019 at 100k and 1M corpus** — tracked, not gated, pending a
  post-1.0 ANN index (HNSW/IVF/DiskANN) on the vec0 table to take per-query cost
  from O(N) to O(log N)/O(√N). (See Known limitations.)

### Removed

(none)

### Known limitations

- **No ANN index — vector search is O(N).** The vec0 bit-KNN candidate stage is
  a per-query linear scan over all N rows; there is no HNSW/IVF/DiskANN index.
  The 10k latency tier is met (real bge p50 36 / p99 49 ms at N≈7,667); 100k is
  ~147 ms p50 (synthetic 384-d) and 1M extrapolates to ~1.5 s — i.e. the 80 ms
  p50 budget is not met above ~50k. The ANN index is the named post-1.0,
  pre-2.1 follow-up. (unverified: the exact post-1.0 milestone for the ANN index
  is named as "post-1.0 / pre-2.1" in the ADRs but no dated milestone exists.)
- **1M real-corpus recall/latency not freshly measured.** ~166 h seed makes it
  infeasible on this hardware; 0.937 @ N=7,667 is treated as an upper-ish bound
  (recall decreases slowly with N) and the 1M latency tier is an O(N)
  extrapolation off the 0.7.0 W4.1 anchor, not a fresh run.

## 0.7.1 - unreleased

EMBEDDER-UNDEFER: the default embedder is no longer deferred. fathomdb now
ships a real in-process default embedder (`BAAI/bge-small-en-v1.5`, 384d, via
`candle-transformers`), opt-in per binding, feeding the 0.7.0 sign-bit +
f32-rerank retrieval pipeline with mean-centering. Date/tag intentionally
unset: the real-corpus recall floor (`AC013B_RECALL_FLOOR`) re-derivation and
canonical N=1M acceptance validation are owned by the 0.7.2 RELEASE-HARDENING
campaign; the `v0.7.1` tag/push is cut there. See
`dev/plans/0.7.1-implementation.md`.

### Added

- **Default embedder (opt-in).** `BAAI/bge-small-en-v1.5` (384d) runs in
  process via `candle-transformers`; WordPiece tokenization (truncated to 512
  tokens) → mean-pool → L2-norm → mean-centering → sign-bit quantization →
  bit-KNN (K=192) → f32 rerank → top-10. Opt in with Python
  `Engine.open(path, use_default_embedder=True)` or TypeScript
  `engineOpen(path, { useDefaultEmbedder: true })`; default is OFF (no embedder
  configured, vector writes fail with `EmbedderNotConfigured` as before).
  Caller-supplied embedders remain available in Rust; custom Python/TS embedder
  bridges are deferred to 0.8.x.
- **First-use weight download (visible, verified).** With the default embedder
  enabled, first use downloads the pinned weight set from a fixed Hugging Face
  URL set, caches it under the platform cache directory, and verifies every
  file by sha256 (no trust-on-first-use). The activity is surfaced in
  `OpenReport.embedder_events` (per-file url + bytes + sha256 + cache path) and
  `OpenReport.embedder_download_ms`. `HF_TOKEN` is honored for token-gated
  mirrors; public bge-small needs none. This is the scoped opt-in exception to
  NEED-017 / REQ-033 (see `ADR-0.7.1-default-embedder-weight-fetch`).
- **Mean-centering.** A per-workspace corpus-mean f32 vector is stored in
  `_fathomdb_embedder_profiles.mean_vec` and subtracted before sign-bit
  quantization (the f32 rerank stays un-centered). It is pinned once at the
  first 256 ingested vectors and never silently recomputed.
- **Bindings.** Python `use_default_embedder` / TypeScript `useDefaultEmbedder`
  open flags; `OpenReport` gains `embedder_download_ms`, `embedder_events`
  (typed union), `embedder_mean_centering_required`, `embedder_mean_vec_pinned`.
- **Docs.** `docs/embedder.md` user guide (opt-in, first-use download, cache,
  offline notes, caveats, migration).

### Changed

- `OpenReport.default_embedder` now reports the real bge-small identity
  (`fathomdb-bge-small-en-v1.5` / HF snapshot `5c38ec7c...` / dim 384) instead
  of the `fathomdb-noop` scaffold identity.
- Schema migration step 10 adds the nullable `mean_vec BLOB` column.
- Wheel / `.node` binary size: the `default-embedder` feature pulls in candle +
  tokenizers and the ~133 MB weight set is fetched at first use (not bundled).
  The feature is opt-in, so builds without it pay no size cost; a per-platform
  wheel-size gate guards regressions.

### Fixed

- **Mean-centering is now applied on the production write path.** Prior to this
  release the corpus-mean pin/apply only ran via an internal test seam; the
  async `engine.write` → projection path never pinned, so real ingests were
  sign-quantized un-centered. The pin + re-quantize now run inside the
  projection commit (serialized for cross-worker correctness), with an
  open-time recovery pin for crash-before-pin workspaces.
- **Embedder inputs over 512 tokens are truncated** instead of erroring. Long
  documents previously failed the BGE forward pass
  (`index-select invalid index 512`).
- **Projection workers are fault-isolated.** A panic inside an embedder no
  longer wedges the projection scheduler (`drain` could previously hang into a
  scheduler timeout); the faulted batch is recorded and the worker recovers.

### Removed

(none)

### Known limitations

- **Real-corpus recall caveat (SUPERSEDED in 0.7.2 — see below).** At 0.7.1
  measurement time, dev-box scouting over the 7,667-doc corpus measured
  recall@10 ~0.828 (95% CI ~0.80–0.86) — below the 0.90 floor — and the floor
  re-derivation + canonical validation were deferred to 0.7.2 (PR-2/PR-3).
  **This 0.828 was later shown to be a measurement artifact** (exclude-after +
  body-string ground truth). The corrected ANN-fidelity number is **0.937 (CI
  0.913–0.957)** and the 0.90 floor HOLDS; see the 0.7.2 "Changed" section and
  `dev/plans/runs/0.7.2-PR-2c-recall-rootcause.md`. Do not cite the 0.828
  number; it is superseded.
- **Topic-drift mean.** Because the mean is pinned on the first 256 ingested
  docs and never recomputed, a workspace whose first 256 docs are
  unrepresentative may under-center. Remedy is reindex (a later campaign).
- **Migration.** Workspaces previously opened with the `fathomdb-noop` profile
  fail closed when re-opened with the default embedder (identity mismatch, by
  design). The remedy is wipe-and-rewrite; there is no in-place swap.

## 0.7.0 - unreleased

PERF-VECTOR-QUANT (PVQ). A perf-focused release line whose load-bearing change
is **binary vector quantization + f32 rerank** to bring vector retrieval latency
(AC-013) within budget. The workspace was bumped to `0.7.0` (`38d5f4f`) and a
local `v0.7.0` tag was cut, **but the tag is HELD locally and not pushed** —
0.7.2 PR-4 pushes `main` and both the `v0.7.0` and `v0.7.1` tags together. Do
not treat 0.7.0 as shipped. AC-013b recall was held OPEN at 0.7.0 ship
(synthetic isotropic fixture cannot reach the floor; real-embedder validation
deferred to 0.7.1). Ledger: `dev/plans/runs/STATUS-perf-vector-quant.md`;
decision: `ADR-0.7.0-vector-binary-quant.md`.

### Added

- **Binary vector quantization (Pack 1).** `vector_default` gains a sibling
  `embedding_bin bit[768]` column computed via sqlite-vec `vec_quantize_binary`
  inside the same writer transaction as the f32 insert (double-write at both
  insert sites). The f32 `embedding` column is retained for the rerank phase and
  the recall ground-truth pass. Schema migration step 9
  (`migrations/009_vector_binary_quant.sql`) with an unknown-kind preflight
  CHECK; dim-aware in-place reshape (`migrate_vector_partition_to_pack1`).
  Commits: `9b9f840`, `f5da3e4`, `7d4aa2c`, `d96c4b0` (RED).
- **`source_type` partition key + metadata columns (Pack 1).** vec0
  partition_key `source_type` (cardinality ~6: email/article/paper/meeting/
  note/todo) mapped from vector kind at write time via `resolve_source_type`
  (6-value HITL lock, `doc→article` coercion), plus `kind`/`created_at`/`tags`/
  `project_mentions` metadata within the vec0 16-column budget. This is the
  correct shape for real workloads (single-kind AC-013 fixture sees no benefit;
  bundled to avoid a second migration).
- **Two-phase query path (Pack 2).** `read_search_in_tx` replaced the
  single-phase f32 brute-force scan with two-phase **bit-KNN
  (`TOP_K_BIT_CANDIDATES`, K=64 at 0.7.0; raised to 192 in 0.7.1) + f32 rerank
  via `vec_distance_l2`**, in a single Deferred read transaction. Commit
  `26ef3dc`.
- **Real-corpus test corpora (CORPUS-1..4).** ~7,667-doc multi-source corpus
  under `data/corpus-data/` (CNN/DailyMail, Enron, QMSum, EnronQA, synthetic
  notes/todos/daily-logs) + cross-doc chain generator + ingest harness +
  search-validation gates. Commits across `5c1e92a`..`d9a219d`.
- **New perf-gates recall test.** `ac_013b_recall_at_10_floor` asserts
  recall@10 ≥ 0.90 against in-test f32 brute-force ground truth (`d468999`).

### Changed

- **AC-013 latency budget re-pinned to 80 / 300 ms** (`AC013_BUDGET_P50/P99`,
  `d468999`), superseding the 50 / 200 ms unindexed-path values. Tracked in
  `ADR-0.7.0-text-query-latency-gates-revised.md`.
- **Projection scanner throughput fix** (`53a270d`): `PROJECTION_INFLIGHT_LIMIT`
  raised 8→32 and the dispatcher now fills the full inflight budget per scan
  cycle (was one job per cycle). Dev-box AC-013 seed dropped ~11× (28.1 s → 2.5 s
  at N=10K).

### Fixed

- **`engine.write` batch-collapse bug** (`4a95cfd`). `write_inner` now allocates
  one write cursor per row in a batch, so a batch of N produces N distinct vec0
  rows (previously collapsed to ~1 unique row, which masked the recall/scanner
  issues with a degenerate recall=1.0). Regression test
  `tests/batch_write_per_row_cursor.rs`.

### Deferred / known gaps at 0.7.0 ship

- **AC-013b recall@10 ≥ 0.90 — held OPEN, not retconned.** The synthetic
  `VaryingEmbedder` fixture cannot reach the floor: sparse (6 of 768 coords)
  scored 0.1572; dense isotropic (`38f5e3a`) scored 0.5124 — the isotropic-noise
  floor, since random vectors carry no semantic structure for sign-bit ANN. Only
  real embeddings can validate the floor, deferred to 0.7.1 EMBEDDER-UNDEFER
  EU-7. 0.7.0 ships the **latency** win (the load-bearing AC-013 closure) with
  recall surfaced as a known gap.
- **Canonical N=1M validation + numeric budget lock** deferred (the seed cost is
  itself the gate). Later reframed in 0.7.2 PR-3 to a tiered, local-measurement
  posture.
- **AC-020 architectural lever** (`ADR-0.7.0-ac020-architectural-lever`,
  status `draft, HITL-required`) — PCACHE2 remains the named 0.7.0 architectural
  lever; the binary-quant change is explicitly a data-encoding change, not a
  second lever.

### Removed

(none)

## 0.6.1 - 2026-05-25

Promotion of `0.6.1-rc.1` to GA following V-slice fresh-install
verification (GREEN on all three bindings — see
`dev/plans/runs/0.6.1-V-transcript.txt`). Scope identical to RC1
below; no code or interface change between RC1 and GA. Axis E
(`fathomdb-embedder-api`) remains at `0.6.0` per Wake decision
`d-001`.

## 0.6.1-rc.1 - 2026-05-25

Patch release. Closes three 0.6.0 deferred items (Python and TypeScript
`OpenReport` surfacing, plus the axis-E independence demonstration),
resolves three Dependabot advisories, and carries the AC-012 canonical-
runner re-measurement as documented evidence (verdict RED; Pack 7 perf
work escalates to 0.7.0 per HITL 2026-05-24).

Axis-E (`fathomdb-embedder-api`) stays at `0.6.0` per Wake decision
`d-001`: no trait-surface change in this release, so axis-E does not
bump in lockstep with axis-W. This is the first post-GA exercise of
the two-axis discipline.

### Fixed

- `OpenReport` is now surfaced from both bindings via an engine-attached
  accessor (closes 12-TX-OPENREPORT carry-over from 0.6.0 GA):
  - Python: `engine.open_report()` returns the native `OpenReport`
    fields verbatim under snake_case identifiers
    (`schema_version_before`, `schema_version_after`,
    `migration_steps`, `embedder_warmup_ms`, `query_backend`,
    `default_embedder`). Idempotent — repeat calls return identical
    data (snapshot, not live state). Closes **AC-068c**.
  - TypeScript: `engine.openReport()` returns the camelCase mirror
    (`schemaVersionBefore`, `schemaVersionAfter`, `migrationSteps`,
    `embedderWarmupMs`, `queryBackend`, `defaultEmbedder`). Sync
    return — data lives in the napi engine struct after `open`.
    Closes **AC-068d**.
  - `Engine.open(...)` signatures are unchanged from 0.6.0 in both
    bindings (additive accessor; no return-shape regression).
- `scripts/security/check_removal_changelog.py` and its bash wrapper
  point their `--base` default at `v0.6.1` (was `v0.6.0`), advancing
  the "removals since last released API" anchor as 0.6.1 becomes the
  new GA reference. AC-050c regression-sentinel test #4 will be
  transiently RED in the BUMP → RC1 → GA window until the `v0.6.1`
  tag is pushed.

### Security

- **RUSTSEC-2025-0020** — bump `pyo3` `0.22.6` → `0.24.1` across the
  workspace; rename `*_bound` PyO3 APIs (24 callsites) to drop the
  deprecation warnings under `-D warnings`.
- **GHSA-mh29-5h37-fv8m** — bump `js-yaml` `4.1.0` → `4.1.1` via
  `markdownlint-cli2` `0.18` → `0.22.1` (transitive).
- **CVE-2024-3651 (idna)** — confirmed false-positive against
  fathomdb (not in the Python dependency graph after lock-file
  audit); `src/python/uv.lock` checked in to make the audit
  reproducible.

### Changed

- `scripts/set-version.sh --workspace 0.6.1` exercises the axis-E
  independence invariant for the first time post-GA: axis-W
  (`Cargo.toml`, `pyproject.toml`, `package.json`, and the five
  workspace.dependencies pins for `fathomdb`, `fathomdb-embedder`,
  `fathomdb-engine`, `fathomdb-query`, `fathomdb-schema`) advances
  to `0.6.1`; axis-E (`fathomdb-embedder-api` `[package].version`
  and its `workspace.dependencies` pin) stays at `0.6.0`.
  Regression sentinel codified in
  `scripts/tests/test_set_version.sh` test #13.
- B-001 forward-retag — `scripts/security/check_removal_changelog.py`
  and `scripts/security/check-removal-changelog.sh` default `--base`
  advanced from `v0.6.0` to `v0.6.1`.

### Removed

(none — patch release, no public symbol removals.)

### Deferred (carry-over)

- **AC-012** text-query latency on FTS5 (p50 ≤ 20 ms / p99 ≤ 150 ms):
  re-measured 2026-05-23 on canonical x86_64 tier-1 CI (AMD EPYC
  9V74, 4 cores, Ubuntu 24.04.4, rustc 1.95.0, SQLite 3.45.x via
  `libsqlite3-sys` 0.28.0) at N=1,000,000. Verdict **RED**:
  p50 = 140.95 ms (7.05× over budget), p99 = 458 ms (3.05× over
  budget). Pack 7 un-defer trigger fires; AC-012 closure target
  moved to **0.7.0** (perf-only release; budget revision + tuning).
  Evidence: `dev/notes/perf-canonical-runner-2026-MM.md` and
  `dev/plans/runs/0.6.1-AC012-measure-output.json` (workflow run
  26346417896). 0.6.1 carries this measurement as evidence and
  does NOT claim AC-012 closure.
- **AC-013** vector retrieval latency, **AC-019** mixed-retrieval
  stress tail, **AC-020** N=8 concurrent reader scaling: stay
  deferred per Pack 7 trigger evaluation (Pack 7 escalated to
  0.7.0 alongside AC-012).
- **Logical-id verbs** (`purge_logical_id` / `restore_logical_id`)
  stay deferred to **0.8.0** per HITL 2026-05-24 rescope.

## 0.6.0 - 2026-05-19

First stable release of FathomDB 0.6.0 — local-first retrieval
engine on SQLite (FTS5 + `sqlite-vec`) with Rust, Python, and
TypeScript SDKs.

### Added

- Local-first retrieval engine on SQLite (FTS5 + `sqlite-vec`):
  canonical writes, vector projections, scheduler, op-store, reader
  pool with thread-affine workers.
- Five-verb runtime SDK surface: `Engine.open`, `engine.write`,
  `engine.search`, `engine.close`, `admin.configure`.
- Typed error hierarchy under `EngineError` (`StorageError`,
  `ProjectionError`, `VectorError`, `EmbedderError`,
  `EmbedderNotConfiguredError`, `KindNotVectorIndexedError`,
  `SchedulerError`, `OpStoreError`, `WriteValidationError`,
  `SchemaValidationError`, `OverloadedError`, `ClosingError`,
  `DatabaseLockedError`, `CorruptionError`,
  `IncompatibleSchemaVersionError`).
- Engine-attached instrumentation: `engine.drain`,
  `engine.counters`, `engine.set_profiling`,
  `engine.set_slow_threshold_ms`, host logging subscriber attach.
- Python SDK (`fathomdb`) — PyO3 binding with type stubs.
- TypeScript SDK (`fathomdb`) — napi-rs binding, Promise API,
  handoff pool, typed exception envelope (TS milestone 1; not yet
  Python-parity — see Deferred).
- Rust facade crate (`fathomdb`) re-exporting runtime verbs from
  `fathomdb-engine`.
- CLI (`fathomdb-cli`) — `doctor` and `recover` verbs (Phase 10a).
- Two-axis versioning (Axis W workspace lockstep + Axis E
  independent embedder-api semver) with `scripts/set-version.sh
--check-files` enforcement and pre-push hook integration.
- 8-tier topological publish workflow `.github/workflows/release.yml`
  with crates.io index-propagation sleeps, post-publish smoke
  against fresh registry installs, co-tagging assert.
- actionlint v1.7.7 wired as canonical workflow validator.
- External user docs: install + quickstart + reference + concepts
  - compatibility (Phase 12-DX).

### Changed

- Release workflow: napi build matrix uses the canonical
  `win32-x64-msvc` target label for npm `optionalDependencies`
  resolution.
- Release workflow: `publish-rust` dry-run cascade restored via
  the rc.1 bootstrap publish that seeded sibling-dep versions on
  crates.io.
- Release workflow: npm `publish` passes `--tag next` for
  prerelease versions so the `latest` dist-tag stays pinned to
  the most recent stable.
- Release workflow: post-publish gates (`assert-co-tagging.sh`
  and the three `smoke-{crates,pypi-wheel,npm}.sh` scripts)
  accept `MAJOR.MINOR.PATCH(-PRERELEASE)?` SemVer.
- Release workflow: `smoke-pypi-wheel.sh` normalizes SemVer to
  PEP 440 (e.g. `0.6.0-rc.4` → `0.6.0rc4`) before `pip install`
  so the wheel resolves under pip's normalized version index.
- Release workflow: `assert-co-tagging.sh` sends a `User-Agent`
  header on crates.io API calls (the registry returns HTTP 403
  without one).
- Release workflow: PyPI + npm smoke scripts write a minimal
  valid record (`{"kind":"doc","body":"{}"}`) instead of an
  empty batch that the engine rejects per the 5-verb invariant.
- Release workflow: new `src/ts/tsconfig.build.json` emits
  `dist/index.js` at the path `package.json "main"` points to.
- Release workflow: `github-release` job explicitly sets
  `prerelease: ${{ contains(github.ref_name, '-') }}` so future
  RC tags are flagged as prereleases on GitHub.

### Deferred

- **Performance gates AC-012 / AC-013 / AC-019 / AC-020** deferred
  to 0.6.1 + Pack 7 (HITL re-confirmed 2026-05-17, Phase 12-P).
  AC-020 N=8 concurrent reader scaling is an architectural gap
  requiring vendor-SQLite work; AC-012 expected to close on
  canonical-runner re-measurement; AC-013/AC-019 close via Pack 7
  batched-insert vec0 API. See `dev/test-plan.md` § Current Perf
  Attribution.
- **`Engine.open` structured open report** dropped by both Python
  and TypeScript bindings in 0.6.0; populated native-side but not
  surfaced. Closes in 0.6.1 (slice `12-TX-OPENREPORT`). Symmetric,
  not a parity gap.
- **Logical-id verbs** (`purge_logical_id`, `restore_logical_id`)
  deferred to 0.8.0 (originally deferred to 0.7.x at Phase 12-V-VERBS
  2026-05-17; re-targeted to 0.8.0 per HITL 2026-05-24 alongside the
  canonical-identity substrate and Memex knowledge-store work — see
  `dev/roadmap/0.8.0.md`). Canonical-identity substrate design-only
  in 0.6.0. Client workaround: `fathomdb recover --excise-source <id>`.
- **TypeScript SDK Python-parity** — TS milestone 1 shipped
  2026-04-07; full Python-parity did NOT land at 0.6.0 GA and
  carries forward as a post-GA deliverable. Prefer Python SDK
  for production pilots until parity ships.

### Removed

(none — 0.6.0 is a rewrite; no 0.5.x→0.6.0 deprecation shims)
