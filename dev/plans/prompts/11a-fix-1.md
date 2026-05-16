# Phase 11a-fix-1 — Reviewer remediation pass

Targeted fix for the four codex `gpt-5.4` findings on Phase 11a
(verdict `BLOCK`, see `dev/plans/runs/11a-review-20260516T184507Z.md`).

Operates in the **existing 11a worktree** `/tmp/fdb-11a-pyo3-binding-20260516T174849Z`
on branch `phase-11a-pyo3-binding-20260516T174849Z`. Builds new commits
on top of `e21708d`.

## Model + effort

Opus 4.7, intent: medium. Spawn from main thread:

```bash
PHASE=11a-fix-1
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plans/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-11a-pyo3-binding-20260516T174849Z
PREAMBLE=$(cat <<'EOF'
YOU ARE THE IMPLEMENTER. Not the orchestrator. Do the work in this
worktree. Do NOT re-spawn yourself. Do NOT spawn other agents. Use
--disallowedTools Task Agent as a hard guard. Write code, run tests,
commit. Done.
EOF
)
( cd "$WT" && \
  ( echo "$PREAMBLE"; cat /home/coreyt/projects/fathomdb/dev/plans/prompts/11a-fix-1.md ) \
  | claude -p --model claude-opus-4-7 --effort medium \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --disallowedTools Task Agent \
      --permission-mode bypassPermissions \
      --output-format stream-json --include-partial-messages --verbose \
  > "$LOG" 2>&1 )
```

## Required reading

- `dev/plans/runs/11a-review-20260516T184507Z.md` — reviewer verdict.
- `dev/plans/prompts/11a-pyo3-binding.md` § FFI safety contract (the
  contract you are tightening).
- `dev/design/errors.md` § Binding-facing class matrix (lines 92-115).
  You will EXTEND this matrix with two new rows.
- `src/rust/crates/fathomdb-py/src/lib.rs` (HEAD: `e21708d`) — the
  binding you are patching.

## Scope — four findings, one commit per finding (or one commit total)

### Finding 1 (`must-fix`) — Panic must surface as `PanicException`

Change `src/rust/crates/fathomdb-py/src/lib.rs` so every `#[pymethod]` /
`#[pyfunction]` panic-catch path raises
`pyo3::panic::PanicException::new_err(...)` (or a defined subclass of
it), NOT `EngineError::new_err(...)`. The current code lives around
lines 23, 241, 332 — verify and patch all sites.

Tighten the Python test at `src/python/tests/test_ffi_safety.py` (the
panic-surfacing case currently at line 19): assert
`isinstance(exc, _fathomdb.PanicException)` (or whichever exact PyO3
class you settled on). `except BaseException` is too loose.

If using PyO3's built-in `PanicException`, no `create_exception!` call
is needed — it's already exported by `pyo3::panic`. If you choose to
ship a subclass for binding ergonomics, define it once in the
`#[pymodule]` and add a single-line `// why:` comment explaining the
subclass choice.

### Finding 2 (`must-fix`) — `force_panic_for_test()` must not ship in release

`src/rust/crates/fathomdb-py/Cargo.toml:14` currently reads:

```toml
[features]
default = ["test-hooks"]
test-hooks = []
```

Change to:

```toml
[features]
default = []
test-hooks = []
```

Verify `src/rust/crates/fathomdb-py/src/lib.rs:531` (the
`force_panic_for_test()` definition + its `#[pymodule]` registration)
is gated behind `#[cfg(any(test, feature = "test-hooks"))]`. If the
pymodule registration is unconditional, gate it too.

Update the python test build path: tests that exercise
`force_panic_for_test()` rely on the `test-hooks` feature. Ensure
`pip install -e src/python/` builds with `--features test-hooks`
(either via `[tool.maturin] features = ["pyo3/extension-module",
"test-hooks"]` in `src/python/pyproject.toml`, or via a separate
test-build invocation documented in the prompt). The CI / dev install
gets the hook; release wheels do not.

If switching the pyproject `features` list breaks the production-wheel
contract, prefer a maturin `--features test-hooks` override invoked
explicitly by the test path — explain the choice in the commit message.

### Finding 3 (`must-fix`) — Distinct Python classes per Rust variant

Two Rust engine variants currently collapse to parent classes:

- `EngineError::EmbedderNotConfigured` → `EmbedderError` (Python)
- `EngineError::KindNotVectorIndexed` → `VectorError` (Python)

Per HITL 2026-05-16 (orchestrator decision), add distinct leaf classes
and extend the canonical matrix.

Steps:

1. **Amend `dev/design/errors.md` § Binding-facing class matrix**
   (currently lines 92-115). Add two new rows:

   | Rust-side surface                    | Python class stem            | TypeScript class stem        | CLI dispatch class |
   | ------------------------------------ | ---------------------------- | ---------------------------- | ------------------ |
   | `EngineError::EmbedderNotConfigured` | `EmbedderNotConfiguredError` | `EmbedderNotConfiguredError` | runtime failure    |
   | `EngineError::KindNotVectorIndexed`  | `KindNotVectorIndexedError`  | `KindNotVectorIndexedError`  | runtime failure    |

   Add an amendment note below the table: "2026-05-16 amendment:
   `EmbedderNotConfigured` and `KindNotVectorIndexed` leaf classes
   added per Phase 11a codex reviewer finding #3. Python and TS
   bindings expose them as distinct leaves; both descend from the
   single rooted base." Bump the file's frontmatter `date` field to
   `2026-05-16`.

   TS impl lands in Phase 11b — out of scope here, but the matrix row
   must enumerate the TS stem so 11b has nothing to invent.

2. **Add Python leaf classes** in `src/python/fathomdb/errors.py`:

   ```python
   class EmbedderNotConfiguredError(EmbedderError):
       """No embedder is configured for a vector-requiring operation."""


   class KindNotVectorIndexedError(VectorError):
       """A vector operation targeted a kind not configured for vector indexing."""
   ```

   Add both to `__all__`. Both descend from existing parent leaves
   (`EmbedderError` / `VectorError`) so the existing rooted-hierarchy
   tests still hold; the new leaves are strictly more specific.

3. **Add PyO3 `create_exception!` entries** in
   `src/rust/crates/fathomdb-py/src/lib.rs` mirroring the Python
   class hierarchy. Update `engine_error_to_py` (around lines 115-130
   per the review) to map directly:

   ```rust
   RustEngineError::EmbedderNotConfigured => {
       EmbedderNotConfiguredError::new_err(/* msg */)
   }
   RustEngineError::KindNotVectorIndexed => {
       KindNotVectorIndexedError::new_err(/* msg */)
   }
   ```

   The single-switch / no-catch-all rule still applies.

4. **Extend `src/python/tests/test_typed_errors.py`**: two new tests,
   each asserts the leaf type AND inheritance from its parent
   (`EmbedderNotConfiguredError` is-a `EmbedderError` is-a
   `EngineError`; same shape for `KindNotVectorIndexed`).

5. **Update `src/python/fathomdb/_fathomdb.pyi`** to declare the two
   new classes (mirroring the existing stub layout).

### Finding 4 (`should-fix`) — Cite AC-057a in `test_surface.py`

`src/python/tests/test_surface.py` should explicitly cite AC-057a in
its module docstring. Add one line in the existing docstring:
"Binds AC-057a (REQ-053): five-verb runtime SDK surface." Match the
citation style already in `test_errors.py` if present.

## Required commands

Run inside the worktree:

```bash
cd /tmp/fdb-11a-pyo3-binding-20260516T174849Z
cargo test -p fathomdb-py
pip install -e src/python/        # must rebuild with new features
pytest src/python/tests -x
./scripts/agent-verify.sh
```

All must pass. If `agent-verify.sh` flakes on
`ac_029_canonical_writes_complete_under_projection_stall` (debug-mode
timing test), rerun once — known flake unrelated to this slice.

## Discipline

- One commit per finding is acceptable; one combined commit is also
  fine. Last commit message must include the closure summary.
- No scope creep into 11b napi-rs or 11c set-version work.
- Comment policy unchanged: no WHAT, only non-obvious WHY. No
  "added in 11a-fix-1" markers.

## Output

After all commands pass, write
`dev/plans/runs/11a-fix-1-output.json`:

```json
{
  "phase": "11a-fix-1",
  "baseline_sha": "e21708d",
  "branch": "phase-11a-pyo3-binding-20260516T174849Z",
  "head_sha": "<HEAD after final commit>",
  "findings_addressed": [
    "1: panic -> PanicException (lib.rs + test_ffi_safety.py)",
    "2: default features no longer include test-hooks (Cargo.toml)",
    "3: EmbedderNotConfiguredError + KindNotVectorIndexedError leaf classes added; matrix extended in design/errors.md",
    "4: AC-057a citation added to test_surface.py module doc"
  ],
  "new_python_classes": [
    "EmbedderNotConfiguredError",
    "KindNotVectorIndexedError"
  ],
  "matrix_amendment": "design/errors.md § Binding-facing class matrix +2 rows",
  "tests_added_or_tightened": ["<test names>"],
  "agent_verify_result": "pass | fail (+ tail)",
  "next_step_for_orchestrator": "promote to 0.6.0-rewrite; respawn codex reviewer for clean PASS"
}
```

Then stop. Do not advance to 11b. Do not run the reviewer yourself.
