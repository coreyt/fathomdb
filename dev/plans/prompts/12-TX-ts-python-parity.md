# Phase 12-TX — TypeScript SDK → Python-Parity Audit + Close-Gaps

Phase 12 Wave 3 first slice. Audits TS SDK against Python SDK for
**genuine parity gaps** (NOT cross-language-idiom differences which
are spec'd intentionally) and closes any found. Per plan §
"TypeScript SDK Python-parity (12-TX)".

Out of scope:

- 12-D / 12-S / 12-P / 12-V-VERBS / 12-B (already closed or deferred).
- Cross-language idiom translation (camelCase vs snake_case, positional
  vs kwarg, Promise vs sync) — these are **intentional per
  `dev/interfaces/typescript.md` and `dev/interfaces/python.md`**.
- New runtime features (verb additions) — bindings parity is over the
  current locked surface, not new functionality.

## Model + effort

Opus 4.7, intent: high. Spawn per `dev/design/orchestration.md` § 2:

```bash
PHASE=12-TX-ts-python-parity
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plans/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-${PHASE}-${TS}
git -C /home/coreyt/projects/fathomdb worktree add "$WT" -b "phase-${PHASE}-${TS}" 0.6.0-rewrite
PREAMBLE=$(cat <<'EOF'
YOU ARE THE IMPLEMENTER. Not the orchestrator. Do the work in this
worktree. Do NOT re-spawn yourself. Do NOT spawn other agents. The
"## Model + effort" section in this prompt describes how YOU were
just launched (claude -p with the listed model/effort). Do NOT
re-run that block. Use --disallowedTools Task Agent as a hard
guard. Write code, run tests, commit. Done.
EOF
)
( cd "$WT" && \
  ( echo "$PREAMBLE"; cat /home/coreyt/projects/fathomdb/dev/plans/prompts/12-TX-ts-python-parity.md ) \
  | claude -p --model claude-opus-4-7 --effort high \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --disallowedTools Task Agent \
      --permission-mode bypassPermissions \
      --output-format stream-json --include-partial-messages --verbose \
  > "$LOG" 2>&1 )
```

## Log destination

- stdout/stderr: `dev/plans/runs/12-TX-ts-python-parity-<ts>.log`
- structured: `dev/plans/runs/12-TX-ts-python-parity-output.json`
- reviewer verdict: `dev/plans/runs/12-TX-review-<ts>.md`

## Required reading

- `AGENTS.md` § 1, § 3, § 4, § 5, § 7.
- `MEMORY.md`, especially:
  - `project_typescript_sdk` — TS milestone 1 shipped 2026-04-07;
    not yet Python-parity (this slice closes that gap).
  - `feedback_tdd` — red-green-refactor.
  - `feedback_reliability_principles` — net-negative LoC.
  - `feedback_release_verification` — registry-installed smoke is
    the release gate.
- `dev/design/orchestration.md` § 2, § 3, § 8.
- `dev/plans/0.6.0-implementation.md` § "TypeScript SDK
  Python-parity (12-TX)" — preliminary gap list.
- **`dev/interfaces/typescript.md` (locked)** — TS surface spec.
  This is the canonical source of TRUTH for what TS surface
  SHOULD look like. Camelcase, positional args, Promise-based,
  `attachSubscriber` are INTENTIONAL per this spec — do NOT
  "fix" them to match Python.
- **`dev/interfaces/python.md` (locked)** — Python surface spec.
  Snake_case, kwargs, sync, `attach_logging_subscriber` are
  INTENTIONAL per this spec.
- **`dev/design/bindings.md` § 1 Surface-set parity invariant** —
  the canonical PARITY DEFINITION. "Same five-verb canonical set
  - same error class taxonomy + same data shapes in language-
    idiomatic spelling."
- `dev/design/errors.md` — error class taxonomy (15 classes
  required across all SDKs per the matrix).

Existing surfaces to audit:

- `src/python/fathomdb/{__init__.py,engine.py,admin.py,errors.py,config.py,types.py}`
- `src/python/fathomdb/_fathomdb.pyi` (type stubs)
- `src/python/tests/test_*.py` (8 test files)
- `src/ts/src/{index.ts,errors.ts,binding.ts,validation.ts}`
- `src/ts/tests/*.test.ts` (6 test files + helpers)
- `src/rust/crates/fathomdb-py/src/lib.rs` (PyO3 native)
- `src/rust/crates/fathomdb-napi/src/lib.rs` (napi-rs native)

## Scope — audit + close, in order

**Inventory first.** Build a parity matrix BEFORE writing code.
Per `feedback_reliability_principles` net-negative-LoC: if a gap
turns out to be intentional per the locked interface spec, NOTE
IT in output JSON but do NOT change code to "fix" it.

### Sub-1: Build parity matrix

Produce a markdown table mapping every member of the Python
public surface to its TS counterpart per
`dev/design/bindings.md` § 1. Rows:

- Verb / method name + signature (per-language idiomatic OK)
- Argument shape (Python kwarg vs TS options-object — both per-spec OK)
- Return type shape (Python dataclass vs TS interface — both per-spec OK)
- Side effects + invariants
- Test counterparts (Python test_X.py vs TS X.test.ts)

For each row, mark:

- ✅ **parity-OK** (both surfaces present; spec-compliant idiom
  differences acknowledged)
- ❌ **gap** (TS missing what Python has, or vice versa, where
  the locked interface spec implies parity)
- 📋 **divergence-by-design** (locked spec explicitly diverges —
  e.g. Promise vs sync; camelCase vs snake_case; positional vs
  kwarg — these are NOT gaps, just documented)

Write the matrix into the prompt's output JSON as
`parity_matrix` field + into a markdown file
`dev/notes/12-TX-parity-matrix.md` (durable artifact).

### Sub-2: Close error-class taxonomy gaps

Python has 15 classes per `src/python/fathomdb/errors.py`:
`EngineError`, `StorageError`, `ProjectionError`, `VectorError`,
`KindNotVectorIndexedError`, `EmbedderError`,
`EmbedderNotConfiguredError`, `SchedulerError`, `OpStoreError`,
`WriteValidationError`, `SchemaValidationError`, `OverloadedError`,
`ClosingError`, `DatabaseLockedError`, `CorruptionError`,
`IncompatibleSchemaVersionError`.

Wait — that's 16 including `EngineError`. Verify exact count from
the Python source. Then count TS leaf classes in
`src/ts/src/errors.ts` (255 lines). Per
`dev/design/errors.md` matrix + `dev/interfaces/typescript.md` §
Errors: one TS leaf per canonical row.

If TS is missing any class:

1. Add the missing leaf to `src/ts/src/errors.ts` (extending
   `FathomDbError`).
2. Add the variant mapping in `rethrowTyped` (or equivalent
   dispatcher) so the napi-rs binding returns the right class.
3. If napi-rs side needs a corresponding `Error::new` call,
   surface as blocker (touches fathomdb-napi crate; bigger scope).

### Sub-3: Close data-shape gaps

For each caller-visible data shape (WriteReceipt, SearchResult,
SoftFallback, CounterSnapshot, EngineConfig), verify Python
fields map 1:1 to TS fields modulo casing.

- Python field `counter_snapshot.write_rows` ↔ TS field
  `counterSnapshot.writeRows`: OK (idiom).
- Python field `search_result.projection_cursor` ↔ TS
  `searchResult.projectionCursor`: OK (idiom).
- Any field present on one side but not the other = GAP.

Add missing fields per-side; update return-mapping logic
(`_native.<call>` → wrapper conversion in both engine.py + Engine.ts).

### Sub-4: Close test-counterpart gaps

Python tests (8): `test_errors.py`, `test_ffi_safety.py`,
`test_no_recovery_surface.py`, `test_property_template.py`,
`test_save_time_validation.py`, `test_scaffold.py`,
`test_surface.py`, `test_typed_errors.py`.

TS tests (6 + helpers): `errors.test.ts`, `ffi-safety.test.ts`,
`no-recovery-surface.test.ts`, `save-time-validation.test.ts`,
`surface.test.ts`, `typed-errors.test.ts`.

**Missing TS counterparts:** `test_property_template.py` +
`test_scaffold.py`. For each:

- Read the Python file. Determine: is the test exercising a
  language-agnostic invariant (port to TS) or a Python-specific
  thing (no TS counterpart needed)?
- If language-agnostic: write TS counterpart at
  `src/ts/tests/<name>.test.ts` using Node's built-in `node:test`
  per existing TS test pattern.
- If Python-specific: document in the parity matrix as
  📋 divergence-by-design with explicit rationale.

### Sub-5: Verify type-stub completeness

Python has `src/python/fathomdb/_fathomdb.pyi` carrying type stubs
for the native PyO3 module. TS uses `.d.ts` (auto-generated from
napi or hand-written in `binding.ts` / `index.ts`).

For each native method that exists in BOTH `fathomdb-py/src/lib.rs`
and `fathomdb-napi/src/lib.rs`:

- Verify the Python `.pyi` declares it.
- Verify the TS type definitions declare it (in `binding.ts`'s
  `NativeEngine` interface or equivalent).
- Any method in one stub set but not the other = GAP.

Add missing declarations. Run `pyright` (Python) and `tsc
--noEmit` (TS) to confirm both type-check clean after additions.

### Sub-6: Run both test suites green

After all parity gaps closed:

```bash
pytest src/python/
cd src/ts && npm test
```

Both must be GREEN. If either has flakes, rerun once.

## Required commands

```bash
cd /tmp/fdb-12-TX-ts-python-parity-<ts>
# Audit + matrix.
# (Manual inventory — write to dev/notes/12-TX-parity-matrix.md.)
# Python suite.
pytest src/python/ -v
# TS suite (rebuild native first via napi build).
cd src/ts && npm ci && npm test
# Type-check both.
pyright src/python/  # if pyright in bootstrap
cd src/ts && npx tsc --noEmit -p tsconfig.json
# Canonical local gate (existing slice regression).
bash scripts/agent-verify.sh
```

Known flakes: `ac_029`, `ac_017`, `t_safe_export_engine_error_exits_export_failure_66`.

## Discipline

- **Inventory before implementing.** The TS surface is locked per
  `dev/interfaces/typescript.md`. Many apparent gaps are
  intentional divergences. Document them, don't "fix" them.
- TDD: every new error class / data field / type stub lands with
  a failing test first.
- Net-negative LoC bias: prefer extending existing tests over
  writing new ones; prefer fixing missing-mapping over rewriting
  Engine.ts wholesale.
- Comment policy: WHY only.
- No new verbs. Parity is over the locked five-verb surface +
  instrumentation methods + admin verb.

## Blockers — surface before writing code

1. **napi-rs binding scope expansion.** If closing an error-class
   gap requires adding new variants to `fathomdb-napi/src/lib.rs`,
   that touches the Rust crate (not just TS wrapper). Surface +
   propose scope decision (in-slice OR follow-up slice).
2. **PyO3 binding scope expansion.** Same for `fathomdb-py` crate.
3. **napi-rs cannot emit a Promise-rejecting class with custom
   attributes** (e.g. `holder_pid` on DatabaseLockedError). If TS
   can't surface custom attrs the way Python's exception classes
   do, surface + propose a normalized shape (e.g. `error.detail
= { holderPid: number }`).
4. **Test counterpart is Python-specific.** If `test_scaffold.py`
   tests something like pytest plugin behavior or Python import
   semantics that doesn't map to TS, surface as
   📋 divergence-by-design with rationale.
5. **napi build fails on this host** (was an issue earlier in
   Phase 11). Surface; recommend Linux-only test scope.

## Output

After all commands pass, write
`dev/plans/runs/12-TX-ts-python-parity-output.json`:

```json
{
  "phase": "12-TX-ts-python-parity",
  "baseline_sha": "<HEAD of 0.6.0-rewrite at spawn>",
  "branch": "phase-12-TX-ts-python-parity-<ts>",
  "head_sha": "<HEAD after final commit>",
  "commits": ["<sha>: <subject>", "..."],
  "parity_matrix_summary": {
    "total_rows": <int>,
    "parity_ok": <int>,
    "gaps_closed": <int>,
    "divergence_by_design": <int>
  },
  "parity_matrix_file": "dev/notes/12-TX-parity-matrix.md",
  "error_classes_python": <int>,
  "error_classes_ts": <int>,
  "data_fields_added_ts": [...],
  "data_fields_added_python": [...],
  "tests_ported_to_ts": ["..."],
  "tests_marked_python_specific": ["..."],
  "type_stubs_updated": ["..."],
  "python_pytest_result": "pass | fail (+ tail)",
  "ts_npm_test_result": "pass | fail (+ tail)",
  "blockers_encountered": [{...}],
  "agent_verify_result": "pass | fail (+ tail)",
  "next_step_for_orchestrator": "promote to 0.6.0-rewrite; respawn codex reviewer for verdict"
}
```

Then stop. Do not advance to 12-DX. Do not run the reviewer
yourself.
