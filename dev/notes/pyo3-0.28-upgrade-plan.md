# PyO3 0.23 → 0.28 Upgrade Plan

Investigation date: 2026-04-09
Target version: pyo3 0.28.3 (released 2026-04-02)
Trigger: RUSTSEC-2025-0020 (`PyString::from_object` info leak in pyo3 < 0.24.1)
Currently bypassed via ignore in `.github/workflows/ci.yml`

---

## Scope

Exactly **one file** contains pyo3 code:
`crates/fathomdb/src/python.rs` (938 lines)

`crates/fathomdb/src/python_types.rs` is pure serde — no pyo3 imports.

### API surface used

- **Macros**: `#[pyclass(frozen)]`, `#[pymethods]`, `#[pymodule(name = "_fathomdb")]`
  (already new-style, takes `&Bound<'_, PyModule>`), `#[pyfunction]`,
  `#[pyo3(signature = ...)]`, `create_exception!`, `wrap_pyfunction!`
- **Types**: `Python<'_>`, `Py<Self>`, `Bound<'_, PyAny>`, `Bound<'_, PyModule>`,
  `PyObject` (one use), `pyo3::types::PyDict`
- **GIL**: `Python::with_gil`, `py.allow_threads` (~30 call sites)
- **Errors**: `PyResult`, `PyErr`, `PyException`, `PyValueError`, custom
  `create_exception!` hierarchy
- **Conversions**: `dict.into()` for `Bound<PyDict>` → `PyObject`; no
  `IntoPy`/`ToPyObject`/`FromPyObject` trait usage
- **Not used**: `PyString`, `downcast`, `GILOnceCell`, `AsPyPointer`, raw pointer
  APIs, custom protocols, async

The code is **already on Bound-style API** — the old `Py<'py, T>` GIL-ref
migration was done in a previous release. RUSTSEC-2025-0020 affects
`PyString::from_object` which fathomdb does not call directly; the risk is
purely transitive through pyo3 internals.

---

## Target versions

| Crate | From | To | Notes |
|-------|------|----|----|
| pyo3 | 0.23.4 | 0.28.3 | Latest stable as of 2026-04-02, MSRV 1.83 |
| pyo3-log | 0.12 | 0.13.3 | Latest pairing with pyo3 0.28 |
| maturin | `>=1.8,<2` | `>=1.9,<2` | pyo3 0.26+ requires maturin ≥ 1.9 |

Verify pyo3-log 0.13.3's Cargo.toml pins pyo3 0.28 before finalizing.

---

## Breaking changes that touch fathomdb

| Change | Version | Impact |
|--------|---------|--------|
| `Python::with_gil` → `Python::attach` (deprecated, not removed) | 0.25 | ~5 sites in tests |
| `Python::allow_threads` → `Python::detach` (deprecated) | 0.25 | ~30 sites |
| `PyObject` deprecated in favor of `Py<PyAny>` | 0.25 | 1 site (`telemetry_snapshot` return type) |
| `#[pymodule]` multi-phase init | 0.26 | Already new-style — no action |
| `FromPyObject` lifetime/Error rework | 0.26/0.27 | Not used — no action |
| `.downcast()` → `.cast()`, `DowncastError` → `CastError` | 0.26 | Not used — no action |
| `#[pymodule] gil_used` default flip | 0.28 | **See concerns below** |
| `From<Bound<T>>`/`From<Py<T>>` for `PyClassInitializer` removed | 0.28 | Not used — no action |
| MSRV 1.83 | 0.28 | Verify against workspace MSRV |

No blockers. Every API fathomdb uses either survives unchanged or has a
trivial rename.

---

## Concrete change list

### 1. `Cargo.toml` (workspace dependencies)

```toml
pyo3 = { version = "0.28", features = ["extension-module"] }
pyo3-log = "0.13"
```

### 2. `crates/fathomdb/src/python.rs`

- `Python::with_gil(...)` → `Python::attach(...)` (~5 sites)
- `py.allow_threads(...)` → `py.detach(...)` (~30 sites)
- `telemetry_snapshot` return type: `PyObject` → `Py<PyAny>`
- Add `#[pymodule(gil_used = true)]` to preserve current invariants
  (see Concerns)

### 3. `python/pyproject.toml`

```toml
[build-system]
requires = ["maturin>=1.9,<2"]
```

### 4. `.github/workflows/ci.yml`

Remove the `RUSTSEC-2025-0020` ignore from the cargo-audit step.

### 5. Workspace MSRV

If `rust-version` is pinned, verify it's ≥ 1.83. (Currently fathomdb uses
edition 2024 which already requires Rust 1.94+, so MSRV is fine.)

---

## Concerns

### 1. Free-threaded default in 0.28 (highest priority)

`EngineCore::Drop` has a documented GIL deadlock invariant around pyo3-log
and the writer thread (commit history references D-096). Under free-threaded
mode the invariants may subtly change.

**Mitigation**: Explicitly set `#[pymodule(gil_used = true)]` and defer
free-threaded support to a follow-up release. Add a TODO comment referencing
this note.

### 2. Stepwise vs. one-shot

0.23 → 0.28 in one hop is viable given the small surface. If CI is nervous
or new failures appear, fall back to:
- Step 1: 0.23 → 0.25 (rename pass: `with_gil` → `attach`, `allow_threads`
  → `detach`, `PyObject` → `Py<PyAny>`)
- Step 2: 0.25 → 0.28 (gil_used annotation, version bump only)

### 3. pyo3-log version pairing

Confirmed pyo3-log 0.13.3 is the latest, but double-check its Cargo.toml
pins pyo3 0.28 before finalizing. If it lags behind, we may need to use
0.13.x or wait.

### 4. `gil_used` regression risk

If the writer thread or any background task takes the GIL re-entrantly,
the new default (`gil_used = false`) would change runtime behavior
silently. Tests should pass, but the fathomdb-specific GIL deadlock
patterns documented in the engine lifecycle deserve manual review.

---

## Estimated effort

**~1 hour, mechanical** — small, focused PR. Most of the time is spent
on the `attach`/`detach` rename pass and verifying tests pass under the
new gil_used semantics.

---

## Suggested release plan

- Defer until after 0.2.1 ships successfully to all three registries
- Land as 0.2.2 in a focused PR with the title "Upgrade pyo3 0.23 → 0.28"
- PR description should reference RUSTSEC-2025-0020 and remove the audit
  ignore in the same commit so the security fix is bisectable
