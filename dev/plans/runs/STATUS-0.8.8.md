# 0.8.8 — status (Observability & telemetry)

> Live state for the 0.8.8 release. Plan → `dev/plans/plan-0.8.8.md`.
> Theme: make retrieval *legible* (EXP-OBS `explain` surface + telemetry/real-gold capture).
> Carries an OOB security drop-in: the **pyo3 0.24.1 → 0.29.0** bump (RUSTSEC-2026-0176/0177)
> as reserved-gap **Slice 1**, landing before the EXP-OBS Py surface (Slice 5).

## Verdict — Slices 0/1/5 DONE; 10/15/20/40 remain

Slice 1 (pyo3 security bump) is on `origin/main` (`8c938bb7`). Slices 0 (ADR) + 5 (EXP-OBS engine
keystone) are landing on `main` via this clean `0.8.8-slice5-close` worktree (the two commits had
leaked onto the `0.8.9-ci-integrity-micro` branch; re-applied here off `origin/main` per the
worktree-base preflight). The **`Explanation` field set is HITL-RATIFIED** (6-owner negotiation,
`dev/plans/runs/0.8.8-explanation-fieldset-ratification.md`): landed shape confirmed architecturally
correct, ADR amended to the code, only added engine delta = `#[non_exhaustive]` ×4. Slices 10
(SDK parity) / 15 (telemetry) / 20 (gold) / 40 (verify) remain, with the ratification artifact as
their input spec.

## Slice ladder

| Slice | Title | State |
|------:|-------|-------|
| 0 | Setup + ADR / scope freeze | ✅ ADR `dev/design/0.8.8-explain-and-telemetry-adr.md` **RATIFIED** (6-owner negotiation); Part A field set + §A.4 Q1–Q3 closed; Part B telemetry/gold amendments folded for Slice 15/20 |
| 1 *(reserved-gap)* | **pyo3 0.24.1 → 0.29.0 security bump** | ✅ on `origin/main` (`8c938bb7`); gated GREEN + codex §9 clean (see R-SEC-1) |
| 5 | EXP-OBS KEYSTONE (`explain=True`) | ✅ **DONE** — `Engine::search_explained` + `Explanation`/`QueryTrace`/`PerHitExplain` (all `#[non_exhaustive]`); reader-protocol 5-tuple; byte-stable default path. R-OBS-1 golden + R-OBS-2-COV (depth>0, graph_arm) tests; governed-surface allowlist 29; clippy clean; codex §9 clean. Landing on `main` via clean worktree. SDK wiring = Slice 10 |
| 10 | EXP-OBS SDK parity + zero-cost bench | ⏳ depends on 5 |
| 15 | Telemetry capture | ⏳ not started |
| 20 | Real-gold pipeline | ⏳ depends on 15 |
| 40 | Verification + release readiness | ⏳ depends on 5,10,15,20 |

## Acceptance criteria

| ID | Requirement | Result |
|----|-------------|--------|
| R-SEC-1 | pyo3 → 0.29.0 off the HIGH/moderate advisories; binding migrated (4 renames) + `#[pymodule(gil_used = true)]`; byte-stability + eu7 recall re-clear; build+import smoke; advisories no longer reported | ✅ on `origin/main` (`8c938bb7`) |
| R-OBS-1 | per-hit arm-provenance + score-breakdown + query trace behind `explain=True` | ✅ engine: golden `r_obs_1_golden_field_fidelity_at_rerank_depth_gt0`; SDK parity → Slice 10 |
| R-OBS-2 | `explain` zero-cost when off | ✅ engine: `None`/no-alloc default path; `r_obs_2_cov_*` byte-identity at depth>0 + graph_arm; bench → Slice 10 |
| R-OBS-3 | reuses existing seams (`fuse_three_arms`/`ce_rerank`/`GraphFrontierStats` side-channel) | ✅ codex §9 confirmed no parallel machinery |
| R-OBS-4 | Py + TS SDK parity | ⏳ Slice 10 (X1 harness) |
| R-TEL-1..3 | Telemetry + real-gold | ⏳ Slice 15/20 (ADR §B amendments folded) |

## Slice 1 — pyo3 0.24.1 → 0.29.0 (R-SEC-1) detail

Change surface = ONE file (`src/rust/crates/fathomdb-py/src/lib.rs`) + `Cargo.toml` + `Cargo.lock`.

**Applied edits (all compiler-verified):**
- `Cargo.toml` `pyo3 = "0.24"` → `"0.29"`; `Cargo.lock` pyo3 0.24.1 → **0.29.0** (+ ffi/macros/build-config).
- `Python::with_gil` → `Python::attach` — **5 sites**.
- `py.allow_threads` → `py.detach` — **3 call sites** (291, 608, 1313) + **2 doc comments** updated for accuracy.
- `.downcast::<PyDict>()` → `.cast::<PyDict>()` — **7 sites**.
- `PyObject` → `Py<PyAny>` — **2 sites** (`embedder_events` field + `embedder_event_to_py` return).
- `#[pymodule]` → `#[pymodule(gil_used = true)]` — preserves GIL semantics (abi3-py310; FFI assumes GIL held; free-threading explicitly out of scope, see `dev/design/free-threaded-python-value-lift-and-experiments.md`).
- **Unanticipated 0.29 deprecation handled:** Clone-deriving `#[pyclass]` types auto-derive `FromPyObject`, which 0.29 makes opt-in (deprecation = build failure on the `-D warnings` gate). Verified all **12** affected types are output-only DTOs (Receipt/Hit/Result/Record/Row/Snapshot/Report/Identity/Node) — **none extracted from Python** as input args (grep-confirmed) — so added `skip_from_py_object` (drops the dormant, never-invoked derive; no observable behavior change). `OpenReport` untouched (no `Clone`, no warning).

**Gates re-cleared:**
- `cargo clippy -p fathomdb-py -- -D warnings` → **GREEN** (no deprecation warnings).
- `cargo build`/`check` → GREEN.
- `maturin develop` (abi3) build → GREEN; **import smoke** (`import fathomdb._fathomdb`) → GREEN.
- Binding in-file Rust tests → **4/4 pass**.
- Python suite (`src/python/tests`, full feature build incl. `default-reranker`) → **639 passed, 8 skipped**. Two transient failures were diagnosed as NON-pyo3: (a) `test_fused_rerank_*` failed only when the binary was built without `default-reranker` (conftest skips its rebuild when test-hooks already present) → passes with the correct full-feature build; (b) `test_prereg_083_lint_*` is a pure-Python design-doc lint (doc says `status: SIGNED`, test wants `decision-ready`) — pre-existing baseline failure, untouched by this binding-only diff.
- **Security:** `cargo audit` → exit 0; **RUSTSEC-2026-0176 / RUSTSEC-2026-0177 (pyo3) no longer reported**. Only 3 pre-existing allowlisted warnings remain (async-std, paste, memmap2 — none pyo3).

**Honest gaps:**
- **eu7 real-corpus recall** gate: NOT runnable locally (no pre-embedded eu7 DB; $0-API constraint) and not part of per-push CI either. This is a pure FFI-layer change (no engine/embedder/quantization/search code touched) — the renames are exact semantic equivalents, so engine recall computation is provably unaffected. The end-to-end Python suite exercises embed/search/extract through the FFI and passed.
- **byte-stability** at the binding surface is covered by the passing Python extract/embed/search suite; the OPP-8 extract-byte-identical engine gate is unaffected (engine crate unchanged).
- **GPU maturin smoke** (0.8.7 R-GPU-3 deferred): the `maturin develop --features ...,embed-cuda` + `cuda:0` confirmation should run on the shared MAIN tree *after* this bump (both touch the shared `.venv`).
- codex §9 review: pending (no auto-merge per plan).
