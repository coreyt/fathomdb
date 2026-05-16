# Phase 11b — napi-rs binding: wire src/ts to fathomdb-engine

Phase 11 second slice. Mirrors the FFI safety contract locked in 11a
(`dev/plans/prompts/11a-pyo3-binding.md` § FFI safety contract,
closed at `e950b5a`). Replaces the pure-TypeScript
`src/ts/src/index.ts` stub with a real napi-rs binding crate that
calls `fathomdb-engine` directly.

Out of scope: PyO3 (already landed in 11a), `set-version.sh` rewrite
(11c), `release.yml` restoration (11d).

## Model + effort

Opus 4.7, intent: high. Spawn from main thread:

```bash
PHASE=11b-napi-binding
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
  ( echo "$PREAMBLE"; cat /home/coreyt/projects/fathomdb/dev/plans/prompts/11b-napi-binding.md ) \
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
(`codex --model gpt-5.4`) MANDATORY: this slice introduces a new
public Rust crate (`fathomdb-napi`), changes the TypeScript package
build path, and must mirror the 11a FFI safety contract — reviewer
will cross-check parity against `fathomdb-py`.

## Log destination

- stdout/stderr: `dev/plans/runs/11b-napi-binding-<ts>.log`
- structured output: `dev/plans/runs/11b-napi-binding-output.json`
- reviewer verdict: `dev/plans/runs/11b-review-<ts>.md`

## Required reading

- `AGENTS.md` (§1, §3, §4, §5, §7).
- `MEMORY.md` and especially:
  - `feedback_tdd.md` (red-green-refactor)
  - `feedback_reliability_principles.md` (no soak, no scope creep, no
    backward-compat shims)
  - `feedback_cross_platform_rust.md` (`c_char`, never hardcoded
    `i8`/`u8`)
- `dev/plans/0.6.0-implementation.md` § Phase 11 and § Immediate
  Next Slice.
- `dev/interfaces/typescript.md` (entire file). Locked TS public
  surface (camelCase, Promise-returning).
- `dev/design/release.md` § Version axes + § Tiered publish order
  (Tier T8 binds the TypeScript package to Axis W; can publish in
  parallel with the Python wheel after T4).
- `dev/design/bindings.md` § 3 (error hierarchy parity contract).
- `dev/design/errors.md` § Binding-facing class matrix — note the
  2026-05-16 amendment adding `EmbedderNotConfiguredError` and
  `KindNotVectorIndexedError` (the matrix already enumerates the TS
  class stems for both, so no further matrix work is needed in 11b).
- `dev/acceptance.md` AC-057a, AC-060a, AC-060b, AC-067, AC-068a,
  AC-068b, AC-041 (recovery non-presence).
- Phase 11a landed example — STUDY THIS for parity:
  - `src/rust/crates/fathomdb-py/src/lib.rs` (the binding pattern)
  - `src/rust/crates/fathomdb-py/Cargo.toml` (feature gating)
  - `src/python/pyproject.toml` (build-backend wiring)
  - `src/python/fathomdb/errors.py` (leaf-class re-export pattern)
- Existing TS scaffold:
  - `src/ts/src/index.ts`, `src/ts/src/errors.ts`
  - `src/ts/tests/surface.test.ts`, `errors.test.ts`,
    `no-recovery-surface.test.ts`
  - `src/ts/package.json`, `tsconfig.json`
- Pre-0.6.0 napi-rs pattern (layout reference only; do NOT copy code
  blindly): `git show 39ee271^ -- typescript/ | head -200`.

## Scope

One new Rust crate, replacement TS package build path, behavior
wiring for the existing surface, and tests that mirror 11a parity.

### 1. New crate: `fathomdb-napi`

Location: `src/rust/crates/fathomdb-napi/`.

Cargo manifest:

- `[lib] crate-type = ["cdylib"]`
- depend on `fathomdb-engine` (workspace path),
  `napi = { version = "2", features = ["napi8", "async"] }`,
  `napi-derive = "2"`. Add `napi-build = "2"` to `[build-dependencies]`
  and create a `build.rs` that calls `napi_build::setup()`.
- workspace member: add to `src/rust/Cargo.toml [workspace] members`.
- Axis W version (lockstep). `publish = false` — this crate ships only
  inside the npm package.

Module exports (via `#[napi]`):

- `Engine` (`#[napi]` struct) wrapping `Arc<fathomdb_engine::Engine>`.
  Methods (all `#[napi]`, async or sync per blocking-vs-pure rule):
  - `open(path: String, options?: EngineOpenOptions): Promise<Engine>`
    — async, runs on libuv pool.
  - `write(batch: Buffer): Promise<WriteReceipt>` — async. The
    parameter shape needs care; the Python side accepts a `list` of
    typed-write dicts. Use the same JSON-as-payload representation
    here unless the engine surface dictates otherwise. Mirror what
    `fathomdb-py` does. If you have to invent a richer napi shape,
    document why in the binding entry module's `// why:` comment.
  - `search(query: String): Promise<SearchResult>` — async.
  - `close(): Promise<void>` — async.
  - `drain(timeoutMs: u32): Promise<void>` — async.
  - `counters(): CounterSnapshot` — sync, GC-friendly accessor.
  - `setProfiling(enabled: bool): void` — sync.
  - `setSlowThresholdMs(value: u32): void` — sync.
  - `attachSubscriber(callback: JsFunction, options?: AttachSubscriberOptions): void`
    — sync; subscriber uses `ThreadsafeFunction` (napi-rs's tsfn
    abstraction).
- `adminConfigure(engine: &Engine, options: AdminConfigureOptions): Promise<WriteReceipt>`
  — free `#[napi]` function. TS-side `admin.configure` thin-wraps it
  to preserve the namespace verb shape locked in
  `dev/interfaces/typescript.md` § Runtime surface.
- One `#[napi(custom_finalize, js_name = "...")]` per concrete error
  class in `src/ts/src/errors.ts` (16 base classes plus the two
  amendment leaves `EmbedderNotConfiguredError`,
  `KindNotVectorIndexedError` = 18 total). All extend `FathomDbError`.
- Data classes: `WriteReceipt`, `SearchResult`, `SoftFallback`,
  `CounterSnapshot`, `SubscriberEvent` — `#[napi(object)]` with the
  camelCase field names from `dev/interfaces/typescript.md`
  § Caller-visible data shapes.

### 2. FFI safety contract (MUST match 11a)

This contract is locked from 11a — do not re-litigate, mirror it.

**Async / thread offload.** Every method that calls into
`fathomdb-engine` and may block (`open`, `write`, `search`, `close`,
`drain`) MUST be `#[napi]` async (which runs the body on the libuv
worker pool, off the main JS thread). napi-rs's async support is the
TS analogue of PyO3's `py.allow_threads`. Pure accessors (`counters`,
`setProfiling`, `setSlowThresholdMs`) may be sync.

**Panic catching (AC-067).** napi-rs catches Rust panics by default
and turns them into thrown JS `Error` instances. Verify that behavior
on napi-rs 2.x; if the default-thrown class is not the napi-rs
`PanicException` analogue (some napi-rs versions throw a generic
`Error` with `name: "RustPanic"`), add explicit `catch_unwind` in
every `#[napi]` entry point and rethrow as a distinct
`FathomDbPanicError` class. Document the choice in a single-line
`// why:` comment on the binding entry module.

Add a debug-only `forcePanicForTest()` `#[napi]` function gated
behind `#[cfg(any(test, feature = "test-hooks"))]`. Mirror 11a
exactly: `default = []` in `Cargo.toml`, dev build opts in via the
build script invocation, release npm publish does NOT ship the hook.

Per 11a Cargo.toml:

```toml
[features]
default = []
test-hooks = []
```

**String validation (AC-068a/b).** Mirror `validate_ffi_string` from
11a, ported to napi-rs idioms.

- Embedded NUL (`\0`) → throw `WriteValidationError`.
- Unpaired UTF-16 surrogates (lone high surrogate `0xD800..=0xDBFF`
  or lone low surrogate `0xDC00..=0xDFFF`) → throw
  `WriteValidationError`. NOTE: JavaScript strings are UTF-16 by
  spec, so lone surrogates are representable on the JS side
  (`String.fromCharCode(0xD800)`). napi-rs converts to Rust `String`
  via `&str` round-trip which may already reject some malformed
  cases — verify behavior and add explicit validation on top.

Centralize in a single `validate_ffi_string(s: &str) -> Result<(), napi::Error>`
helper. Call on every string field of every typed write and op-store
payload, BEFORE writer transaction opens.

**Typed error mapping.** Single `engine_error_to_napi(err: fathomdb_engine::EngineError) -> napi::Error`
switch. NO catch-all arm — if the engine variant set drifts from the
TS class set, the build fails at the match. Mirror 11a's
`engine_error_to_py` exactly, including the two amendment leaves
(`EmbedderNotConfiguredError` → distinct class;
`KindNotVectorIndexedError` → distinct class).

### 3. TypeScript package re-wire

`src/ts/package.json`:

- Add napi-rs as a dependency and as the binding load surface:

  ```jsonc
  "dependencies": {
    "@napi-rs/cli": "^2"   // dev-only? or runtime? — see below
  },
  "scripts": {
    "build": "napi build --platform --release --js src/_napi.js --dts src/_napi.d.ts && tsc -p tsconfig.json",
    "build:debug": "napi build --platform --js src/_napi.js --dts src/_napi.d.ts && tsc -p tsconfig.json",
    "typecheck": "tsc --noEmit -p tsconfig.json",
    "test": "npm run build:debug && node --test dist/tests/*.test.js"
  },
  "napi": {
    "name": "fathomdb",
    "triples": {
      "defaults": true,
      "additional": []
    }
  }
  ```

  Actual flag set depends on napi-rs 2.x CLI; verify and adapt. Goal:
  `npm run build` produces `src/_napi.js` (and a platform-tagged
  `.node` binary alongside) that the TS layer can `import`.

- Move `@napi-rs/cli` to `devDependencies` if it is build-only;
  runtime users only need the platform-specific `.node` binary plus
  the loader stub.

- Keep `version = "0.6.0"` (Axis W lockstep).

`src/ts/src/index.ts`: replace the pure-TS `Engine` class body with
imports from `./_napi.js`. Re-export the napi-side classes and
helpers. `admin.configure` stays as a thin TS wrapper around the
underlying `adminConfigure` napi free function so the namespace shape
locked in `dev/interfaces/typescript.md` is preserved.

`src/ts/src/errors.ts`: replace each class body with a thin
re-export of the napi-side class so `instanceof` checks against the
imported class match the actual thrown class. Pattern:

```ts
import { FathomDbError as _FathomDbError } from "./_napi.js";
export const FathomDbError = _FathomDbError;
```

(Or `export { FathomDbError } from "./_napi.js"` if napi-rs exports
the symbols at top level.)

Add `EmbedderNotConfiguredError` and `KindNotVectorIndexedError` to
the exports. Update `errors.test.ts` to include them in the
`LEAF_CLASSES` list and to assert their inheritance chain
(`EmbedderNotConfiguredError extends EmbedderError`,
`KindNotVectorIndexedError extends VectorError`).

### 4. Tests

TDD: red-green-refactor.

- **Existing tests stay green.** `surface.test.ts`, `errors.test.ts`,
  `no-recovery-surface.test.ts` already pin shape and the rooted
  hierarchy. They must pass against the napi binding with import
  paths unchanged.

- **New: `src/ts/tests/ffi-safety.test.ts`.** Mirrors
  `src/python/tests/test_ffi_safety.py`. Covers AC-067, AC-068a,
  AC-068b:
  - `panic surfaces as JS exception, process unchanged` — call
    `_napi.forcePanicForTest()`; assert the thrown class is the
    documented panic class; assert `process.pid` unchanged before
    and after; assert subsequent `engine.counters()` works.
  - `embedded NUL rejected as WriteValidationError` — `engine.write([{ field: "a b" }])`
    (or whatever the write-batch shape is); assert
    `WriteValidationError` thrown; assert engine cursor unchanged.
  - `unpaired surrogate rejected as WriteValidationError` — use
    `String.fromCharCode(0xD800) + "x"` in a text field; same
    assertions.

- **New: `src/ts/tests/typed-errors.test.ts`.** Mirrors
  `src/python/tests/test_typed_errors.py`. AC-060a coverage. At
  minimum:
  - `EmbedderDimensionMismatchError` carries `stored` + `supplied`
    number fields.
  - `CorruptionError` carries `kind`, `stage`, `recoveryHintCode`,
    `docAnchor` string fields.
  - `DatabaseLockedError` carries `holderPid` number-or-undefined.
  - `EmbedderNotConfiguredError instanceof EmbedderError` and
    `instanceof FathomDbError`.
  - `KindNotVectorIndexedError instanceof VectorError` and
    `instanceof FathomDbError`.

- **New: `src/ts/tests/save-time-validation.test.ts`.** AC-060b
  coverage: submit an op-store write whose payload violates the
  registered schema; assert `SchemaValidationError`; assert no row
  in the op-store table.

- **Rust unit tests** in `fathomdb-napi/src/lib.rs` `#[cfg(test)]`:
  the `validate_ffi_string` helper — NUL, surrogate, valid string.
  Pure-Rust; no Node runtime required.

### 5. Bootstrap + agent-verify

`scripts/bootstrap.sh`: extend to run `npm install` and the napi
build in `src/ts/` if the script does not already. Same idempotent
pattern as the 11a maturin/pip integration.

If `scripts/agent-verify.sh` does not already exercise
`npm run build && npm test` in `src/ts/`, the Phase 11 exit gate
cannot pass — surface as a blocker, do NOT silently bypass.

## Required commands

Run inside the worktree (`$WT`):

```bash
cd "$WT"
# Rust binding compile + unit tests.
cargo test -p fathomdb-napi
# TS-side feedback loop.
cd src/ts && npm install && npm test && cd ../..
# Canonical local gate.
./scripts/agent-verify.sh
```

`agent-verify.sh` is the canonical gate per `AGENTS.md`. If it
surfaces a failure not caused by this slice, stop and report — do
NOT fix unrelated breakage.

Known flake: `ac_029_canonical_writes_complete_under_projection_stall`
in `fathomdb-engine` is timing-sensitive in debug mode and may flake
under host load. Rerun once if it flakes; passes clean in release.
Not 11b code.

## Discipline

- TDD per `feedback_tdd.md`.
- No soak / no scope creep into 11c (set-version) or 11d
  (release.yml). Surface those as out-of-scope if tempted.
- `feedback_cross_platform_rust.md`: any `c_char`-shaped interop
  uses `std::os::raw::c_char`, never hardcoded `i8`/`u8`.
- Comment policy per `AGENTS.md`: no WHAT comments, only non-obvious
  WHY. No "added in 11b" / "for napi parity" markers.
- Cite acceptance ids in test names / module docs: `AC-057a`,
  `AC-060a`, `AC-060b`, `AC-067`, `AC-068a`, `AC-068b`.
- Public surface change: every `#[napi]` export is a locked-symbol
  change. Reviewer cross-checks against `dev/interfaces/typescript.md`
  AND against `fathomdb-py` parity.
- One commit per logical step. Last commit message includes a
  Phase-11b closure summary line.

## Blockers — surface before writing migration code

If any of these are true, STOP and write the blocker report:

- `fathomdb-engine` is missing a public method matching any TS verb.
  (11a already exercised the engine surface; this should not happen.)
- napi-rs 2.x does not expose the `ThreadsafeFunction` shape needed
  for `attachSubscriber`, and a substitute that preserves the
  binding-side callback contract requires substrate not in this slice.
- `scripts/agent-verify.sh` cannot be extended to exercise
  `npm run build && npm test` without rewriting `agent-verify.sh`
  itself (out of scope).

Blocker report shape: same as 10b-B
(`dev/plans/runs/10b-B-purge-restore-output.json`).

## Output

After all commands pass, write
`dev/plans/runs/11b-napi-binding-output.json` with:

```json
{
  "phase": "11b",
  "baseline_sha": "<HEAD at spawn time>",
  "branch": "phase-11b-napi-binding-<ts>",
  "head_sha": "<HEAD after final commit>",
  "new_crate": "fathomdb-napi",
  "ffi_contract_parity_with_11a": [
    "async #[napi] off main thread (analogue of py.allow_threads)",
    "panic catch -> documented panic class (default napi-rs behavior or explicit FathomDbPanicError)",
    "validate_ffi_string rejects NUL + unpaired surrogate -> WriteValidationError",
    "engine_error_to_napi: single-switch typed mapping, no catch-all arm",
    "EmbedderNotConfiguredError + KindNotVectorIndexedError leaf classes mirror 11a"
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
  "test_results": {
    "cargo_test_fathomdb_napi": "<pass count>",
    "ts_npm_test": "<pass count>",
    "agent_verify": "pass | fail (+ tail)"
  },
  "next_step_for_orchestrator": "spawn 11b reviewer; then 11c set-version"
}
```

Then stop. Do not advance to 11c. Do not run the reviewer yourself.
