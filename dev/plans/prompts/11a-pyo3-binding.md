# Phase 11a — PyO3 binding: wire src/python to fathomdb-engine

Phase 11 first slice. Replaces the pure-Python `src/python/fathomdb/*.py`
stub with a real PyO3 binding crate that calls `fathomdb-engine` directly.
Locks the FFI safety contract (GIL release, panic catching, string
validation, typed error mapping) that Phase 11b napi-rs will mirror.

Out of scope: napi-rs / TS binding (11b), `set-version.sh` rewrite (11c),
`release.yml` restoration (11d).

## Model + effort

Opus 4.7, intent: high. Spawn from main thread:

```bash
PHASE=11a-pyo3-binding
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plans/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-${PHASE}-${TS}
git -C /home/coreyt/projects/fathomdb worktree add "$WT" -b "phase-${PHASE}-${TS}" 0.6.0-rewrite
PREAMBLE=$(cat <<'EOF'
YOU ARE THE IMPLEMENTER. Not the orchestrator. Do the work in this
worktree. Do NOT re-spawn yourself. Do NOT spawn other agents. The
"## Model + effort" section in this prompt describes how YOU were
just launched (claude -p with the listed model/effort). Do NOT re-run
that block. Use --disallowedTools Task Agent as a hard guard if you
forget. Write code, run tests, commit. Done.
EOF
)
( cd "$WT" && \
  ( echo "$PREAMBLE"; cat /home/coreyt/projects/fathomdb/dev/plans/prompts/11a-pyo3-binding.md ) \
  | claude -p --model claude-opus-4-7 --effort high \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --disallowedTools Task Agent \
      --permission-mode bypassPermissions \
      --output-format stream-json --include-partial-messages --verbose \
  > "$LOG" 2>&1 )
```

Anti-chaining: PREAMBLE prepended via stdin per
`dev/plans/prompts/01-orchestrator-resume.md` §4. Reviewer
(`codex --model gpt-5.4`) MANDATORY: this slice introduces a new public
Rust crate (`fathomdb-py`), changes the Python package build backend
(setuptools → maturin), and defines the FFI safety contract that 11b
will mirror.

## Log destination

- stdout/stderr: `dev/plans/runs/11a-pyo3-binding-<ts>.log`
- structured output: `dev/plans/runs/11a-pyo3-binding-output.json`
- reviewer verdict: `dev/plans/runs/11a-review-<ts>.md`

## Required reading

- `AGENTS.md` (§1, §3, §4, §5, §7).
- `MEMORY.md` and especially:
  - `feedback_tdd.md` (red-green-refactor)
  - `feedback_python_native_build.md` (`pip install -e src/python/`,
    never manual `cargo build + cp`)
  - `feedback_reliability_principles.md` (no soak, no scope creep, no
    backward-compat shims)
  - `feedback_cross_platform_rust.md` (`c_char`, never hardcoded `i8`/`u8`)
- `dev/plans/0.6.0-implementation.md` § Phase 11 (lines 429-467) and
  § Immediate Next Slice.
- `dev/interfaces/python.md` (entire file — 89 lines). This is the
  locked Python public surface.
- `dev/design/release.md` § Version axes (lines 16-43) and § Tiered
  publish order (lines 46-66). Tier T8 binds the Python wheel to Axis W.
- `dev/design/bindings.md` § 3 (error hierarchy parity contract).
- `dev/design/errors.md` § Binding-facing class matrix (single root +
  one concrete subclass per canonical row).
- `dev/acceptance.md` AC-057a (line 878), AC-060a (line 910), AC-060b
  (line 918), AC-067 (line 1019), AC-068a (line 1029), AC-068b (line
  1039), AC-041 (recovery surface non-presence).
- Existing scaffold: `src/python/fathomdb/__init__.py`,
  `errors.py`, `engine.py`, `types.py`, `config.py`, `admin.py`.
- Existing tests: `src/python/tests/test_surface.py`,
  `test_errors.py`, `test_recovery_unreachable.py`.
- Pre-0.6.0 PyO3 pattern (for layout reference only; do NOT copy code
  blindly): `git show 39ee271^ -- python/ | head -200`.

## Scope

One new Rust crate, replacement Python package build backend, behavior
wiring for the existing surface, and the FFI safety contract that 11b
will mirror.

### 1. New crate: `fathomdb-py`

Location: `src/rust/crates/fathomdb-py/`.

Cargo manifest:

- `[lib] crate-type = ["cdylib"]`
- depend on `fathomdb-engine` (workspace path), `pyo3 = { version = "0.22",
features = ["extension-module", "abi3-py310"] }`
- workspace member: add to `src/rust/Cargo.toml [workspace] members`
- Axis W version (lockstep with workspace). Do NOT publish to crates.io;
  this crate ships only inside the Python wheel.

Module name: `_fathomdb` (underscore prefix — it is the C extension that
the Python package imports; not a public PyPI name).

`#[pymodule]` surface:

- `Engine` (`#[pyclass]`) wrapping `Arc<fathomdb_engine::Engine>` (or
  whatever ownership shape matches `Engine::open` — read the engine
  source). Methods: `open` (classmethod / staticmethod returning `Engine`),
  `write`, `search`, `close`, `drain`, `counters`, `set_profiling`,
  `set_slow_threshold_ms`, `attach_logging_subscriber`.
- `admin_configure(engine: &Engine, name: &str, body: &str) -> WriteReceipt`
  free function — Python `admin.configure` wraps it.
- One `create_exception!` per concrete error class in
  `src/python/fathomdb/errors.py` (17 classes today). All inherit from
  `EngineError`, which inherits from `PyException`.
- Data classes: `WriteReceipt`, `SearchResult`, `SoftFallback`,
  `CounterSnapshot` — `#[pyclass(get_all, frozen)]` mirroring the
  attribute names from `dev/interfaces/python.md` § Caller-visible
  data shapes.

### 2. FFI safety contract

This is the contract 11b will mirror in napi-rs. Lock it now.

**GIL release.** Every method that calls into `fathomdb-engine` and may
block (open, write, search, close, drain) MUST wrap the engine call in
`py.allow_threads(|| ...)`. The only methods that may hold the GIL
through their body are pure accessors (`counters`, `set_profiling`,
`set_slow_threshold_ms`).

**Panic catching (AC-067).** Every `#[pyfunction]` / `#[pymethod]` entry
point MUST wrap engine calls in `std::panic::catch_unwind` (or rely on
PyO3's built-in `PanicException` translation — verify which path PyO3
0.22 takes by default and document the choice in a single-line `// why:`
comment on the binding entry module). A panic in the engine MUST surface
as a Python `pyo3::panic::PanicException` (or `EngineError` subclass —
your choice, document it) WITHOUT aborting the process. Add a debug-only
`force_panic_for_test()` `#[pyfunction]` gated behind `#[cfg(any(test,
feature = "test-hooks"))]`. Enable the `test-hooks` feature in the dev
build path; do NOT ship it in release wheels.

**String validation (AC-068a/b).** Every string argument crossing the
FFI MUST be checked for:

- embedded NUL (`\0`) → raise `WriteValidationError`
- unpaired UTF-16 surrogates (`\u{D800}`..=`\u{DFFF}` codepoints) →
  raise `WriteValidationError`

The check happens before any SQLite bind. Centralize in a single
`validate_ffi_string(&str) -> PyResult<()>` helper; call it on every
string field of every typed write and op-store payload. Python `str` is
already UTF-8, but `\u{D800}..\u{DFFF}` are valid Python codepoints that
must be rejected — Python permits them, SQLite does not. The
no-row-written assertion (AC-068a/b § Measurement) must hold: validate
BEFORE opening the writer transaction.

**Typed error mapping.** The engine returns `EngineError` (Rust enum).
The binding translates each variant to its Python counterpart by name,
preserving the typed payload fields documented in `errors.py`. Use a
single `engine_error_to_py(err: fathomdb_engine::EngineError, py: Python)
-> PyErr` switch; do NOT distribute the mapping across call sites. If
the engine variant set drifts from the Python class set, the build
should fail at the match — no catch-all arm.

### 3. Python package re-wire

`src/python/pyproject.toml`:

- Switch `[build-system]` to maturin:

  ```toml
  [build-system]
  requires = ["maturin>=1.7,<2"]
  build-backend = "maturin"

  [tool.maturin]
  manifest-path = "../rust/crates/fathomdb-py/Cargo.toml"
  module-name = "fathomdb._fathomdb"
  python-source = "."
  features = ["pyo3/extension-module"]
  ```

- Keep `version = "0.6.0"` (Axis W lockstep).
- Keep the existing `[tool.ruff]`, `[tool.pyright]` blocks.
- Drop `[tool.setuptools]` blocks — incompatible with maturin backend.

`src/python/fathomdb/__init__.py`: import the native module:

```python
from fathomdb import _fathomdb  # native PyO3 extension
```

Re-export `Engine` from `_fathomdb` instead of the pure-Python class.
Keep `EngineConfig` as a Python dataclass in `config.py` (it is a
kwargs-bag, not an engine handle).

`src/python/fathomdb/errors.py`: replace each class body with `pass` and
re-bind to the PyO3-created exception types:

```python
from fathomdb._fathomdb import (
    EngineError as _EngineError,
    StorageError as _StorageError,
    # ... etc
)

EngineError = _EngineError
StorageError = _StorageError
```

Typed payload accessors (`holder_pid`, `kind`, `stage`,
`recovery_hint_code`, `doc_anchor`, `stored_name`, `supplied_name`,
`stored`, `supplied`) MUST be readable on the PyO3-created class —
`#[pyclass(extends=EngineError)]` + `#[pyo3(get)]` on the struct fields.

`src/python/fathomdb/engine.py` and `admin.py`: thin wrappers calling
into `_fathomdb`. Engine.py owns the docstring and the type annotations
(pyright happiness); admin.py provides the `admin.configure(engine, ...)`
namespace verb per `dev/interfaces/python.md` § Runtime surface.

`src/python/fathomdb/types.py`: keep as pure-Python dataclasses ONLY if
the PyO3 classes are not directly importable for typechecking. Prefer
re-exporting the PyO3 `#[pyclass(frozen)]` types directly.

### 4. Tests

TDD: red-green-refactor. Write the failing test first.

- **Existing tests stay green.** `test_surface.py`, `test_errors.py`,
  `test_recovery_unreachable.py` already pin surface shape and error
  hierarchy. They must pass against the PyO3 binding without
  modification beyond import paths (and import paths SHOULD not need
  changing — that's the point of re-exporting from `__init__.py`).

- **New: `src/python/tests/test_ffi_safety.py`.** Covers AC-067,
  AC-068a, AC-068b:
  - `test_panic_surfaces_as_python_exception` — call
    `_fathomdb.force_panic_for_test()`; assert raises
    `PanicException` (or documented subclass); assert `os.getpid()`
    unchanged before/after; assert subsequent `engine.counters()`
    still works.
  - `test_embedded_nul_rejected` — call `engine.write([...])` with a
    payload containing `"a\x00b"` in any text field; assert
    `WriteValidationError`; assert engine cursor / counters unchanged.
  - `test_unpaired_surrogate_rejected` — same shape with `"a\ud800b"`;
    same assertions.

- **New: `src/python/tests/test_typed_errors.py`.** AC-060a coverage:
  one trigger per error variant (where feasible against an in-memory
  engine). At minimum:
  - `EmbedderDimensionMismatchError` carries `.stored` and `.supplied`
    integer attrs.
  - `CorruptionError` carries `.kind`, `.stage`, `.recovery_hint_code`,
    `.doc_anchor` string attrs.
  - `DatabaseLockedError` carries `.holder_pid` int-or-None attr.

- **New: `src/python/tests/test_save_time_validation.py`.** AC-060b
  coverage: submit `OpStore` write whose payload violates registered
  schema; assert `SchemaValidationError`; assert no row in op-store
  table.

- **Rust unit tests** in `fathomdb-py/src/lib.rs` `#[cfg(test)]`: the
  `validate_ffi_string` helper — NUL, surrogate, valid string. These
  are pure-Rust; they do not need a Python interpreter.

### 5. Bootstrap script

`scripts/bootstrap.sh` already exists. Read it. If it does NOT already
install maturin + run `pip install -e src/python/`, add those lines
(idempotent). Do NOT add a new bootstrap variant; extend the existing
one.

If `scripts/agent-verify.sh` does not already exercise the Python build
loop via `pip install -e src/python/` + `pytest src/python/tests`, the
Phase 11 exit gate cannot pass — surface that as a blocker, do NOT
silently bypass.

## Required commands

Run inside the worktree (`$WT`):

```bash
cd "$WT"
# Build the native module into the editable Python install.
pip install -e src/python/
# Python-side feedback loop.
pytest src/python/tests -x
# Pure-Rust unit tests for the binding.
cargo test -p fathomdb-py
# Canonical local gate.
./scripts/agent-verify.sh
```

`agent-verify.sh` is the canonical gate per `AGENTS.md`. If it surfaces
a failure not caused by this slice, stop and report — do not fix
unrelated breakage.

## Discipline

- TDD per `feedback_tdd.md`.
- No soak / no scope creep into 11b (napi-rs), 11c (set-version), 11d
  (release.yml). Surface those as out-of-scope if tempted.
- `feedback_python_native_build.md`: native module is built via
  `pip install -e src/python/`. Never manual `cargo build && cp .so`.
- `feedback_cross_platform_rust.md`: if any `c_char`-shaped interop is
  needed, use `std::os::raw::c_char`, never hardcoded `i8`/`u8`.
- Comment policy per `AGENTS.md`: no WHAT comments, only non-obvious
  WHY. No "added in 11a" markers.
- Cite acceptance ids in test names / module docs: `AC-057a`,
  `AC-060a`, `AC-060b`, `AC-067`, `AC-068a`, `AC-068b`.
- Public surface change: every `#[pyclass]` / `create_exception!` is a
  locked-symbol change. The reviewer cross-checks against
  `dev/interfaces/python.md`.
- One commit per logical step. Last commit message includes a
  Phase-11a closure summary line.

## Blockers — surface before writing migration code

If any of these are true, STOP and write the blocker report instead of
proceeding:

- `fathomdb-engine` does not expose a public method matching any
  Python verb (open, write, search, close, drain, counters,
  set_profiling, set_slow_threshold_ms, attach_logging_subscriber,
  admin.configure equivalent).
- `EngineError` Rust enum is missing a variant that the Python error
  hierarchy assumes (e.g. `WriteValidationError` has no Rust
  counterpart).
- `scripts/agent-verify.sh` cannot be made to exercise the Python
  build loop without rewriting `agent-verify.sh` itself (out of scope
  for 11a).

Blocker report shape: same as 10b-B
(`dev/plans/runs/10b-B-purge-restore-output.json`).

## Output

After all commands pass, write
`dev/plans/runs/11a-pyo3-binding-output.json` with:

```json
{
  "phase": "11a",
  "baseline_sha": "56699d8",
  "branch": "phase-11a-pyo3-binding-<ts>",
  "head_sha": "<HEAD after final commit>",
  "new_crate": "fathomdb-py",
  "build_backend_switch": "setuptools -> maturin",
  "ffi_contract_locked": [
    "py.allow_threads around blocking engine calls",
    "panic catch -> PanicException (or documented subclass)",
    "validate_ffi_string rejects NUL + unpaired surrogate -> WriteValidationError",
    "engine_error_to_py: single-switch typed mapping, no catch-all arm"
  ],
  "tests_added": ["<test names>"],
  "acceptance_ids_bound": [
    "AC-057a",
    "AC-060a",
    "AC-060b",
    "AC-067",
    "AC-068a",
    "AC-068b"
  ],
  "agent_verify_result": "pass | fail (+ tail)",
  "next_step_for_orchestrator": "spawn 11a reviewer, then 11b napi-rs binding"
}
```

Then stop. Do not advance to 11b. Do not run the reviewer yourself.
