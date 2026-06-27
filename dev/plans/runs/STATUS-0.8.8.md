# 0.8.8 — status (Observability & telemetry)

> Live state for the 0.8.8 release. Plan → `dev/plans/plan-0.8.8.md`.
> Theme: make retrieval *legible* (EXP-OBS `explain` surface + telemetry/real-gold capture).
> Carries an OOB security drop-in: the **pyo3 0.24.1 → 0.29.0** bump (RUSTSEC-2026-0176/0177)
> as reserved-gap **Slice 1**, landing before the EXP-OBS Py surface (Slice 5).

## Verdict — IN PROGRESS

Slice 0 (scope freeze + this tracker) and **Slice 1 (pyo3 security bump) — DONE, pending
codex §9 + HITL sign-off / commit**. Feature slices 5/10/15/20 (EXP-OBS + telemetry) not yet
started; Slice 5 requires HITL ratification of the `Explanation` payload schema with the
M-work owner before code (plan §9).

## Slice ladder

| Slice | Title | State |
|------:|-------|-------|
| 0 | Setup + ADR / scope freeze | ✅ Migration approach + ACs frozen in `plan-0.8.8.md` §1/§2; this STATUS stood up |
| 1 *(reserved-gap)* | **pyo3 0.24.1 → 0.29.0 security bump** | ✅ migrated + gated GREEN (see R-SEC-1 below); **pending codex §9 + commit** |
| 5 | EXP-OBS KEYSTONE (`explain=True`) | ⏳ blocked on HITL schema ratification |
| 10 | EXP-OBS SDK parity + zero-cost bench | ⏳ depends on 5 |
| 15 | Telemetry capture | ⏳ not started |
| 20 | Real-gold pipeline | ⏳ depends on 15 |
| 40 | Verification + release readiness | ⏳ depends on 5,10,15,20 |

## Acceptance criteria

| ID | Requirement | Result |
|----|-------------|--------|
| R-SEC-1 | pyo3 → 0.29.0 off the HIGH/moderate advisories; binding migrated (4 renames) + `#[pymodule(gil_used = true)]`; byte-stability + eu7 recall re-clear; build+import smoke; advisories no longer reported | ✅ see below |
| R-OBS-1..4 | EXP-OBS `explain` surface | ⏳ not started |
| R-TEL-1..3 | Telemetry + real-gold | ⏳ not started |

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
