# Phase 11b-fix-1 — Reviewer remediation pass

Targeted fix for the two codex `gpt-5.4` findings on Phase 11b
(verdict `CONCERN`, see
`dev/plans/runs/11b-review-20260516T212745Z.md`).

Operates in the **existing 11b worktree**
`/tmp/fdb-11b-napi-binding-20260516T210327Z` on branch
`phase-11b-napi-binding-20260516T210327Z`. Builds new commits on top
of `bb4f64f`.

## Model + effort

Opus 4.7, intent: medium. Spawn from main thread:

```bash
PHASE=11b-fix-1
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plans/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-11b-napi-binding-20260516T210327Z
PREAMBLE=$(cat <<'EOF'
YOU ARE THE IMPLEMENTER. Not the orchestrator. Do the work in this
worktree. Do NOT re-spawn yourself. Do NOT spawn other agents. Use
--disallowedTools Task Agent as a hard guard. Write code, run tests,
commit. Done.
EOF
)
( cd "$WT" && \
  ( echo "$PREAMBLE"; cat /home/coreyt/projects/fathomdb/dev/plans/prompts/11b-fix-1.md ) \
  | claude -p --model claude-opus-4-7 --effort medium \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --disallowedTools Task Agent \
      --permission-mode bypassPermissions \
      --output-format stream-json --include-partial-messages --verbose \
  > "$LOG" 2>&1 )
```

## Required reading

- `dev/plans/runs/11b-review-20260516T212745Z.md` — reviewer verdict.
- `dev/plans/prompts/11b-napi-binding.md` § FFI safety contract (the
  contract you are tightening).
- `src/rust/crates/fathomdb-napi/src/lib.rs` (HEAD: `bb4f64f`) — the
  binding you are patching. The existing `call_engine` /
  `Engine::open` panic-catch helper is the pattern to mirror.

## Scope — two findings

### Finding 1 (`medium`) — Uniform panic catching on every `#[napi]` entry point

Wrap each of these synchronous `#[napi]` methods in
`std::panic::catch_unwind` and map any unwind to `FDB_PANIC` /
`FathomDbPanicError` using the same path `call_engine` already uses:

- `Engine::counters()`
- `Engine::set_profiling()`
- `Engine::set_slow_threshold_ms()`
- `Engine::attach_subscriber()`

Pattern: extract the existing async unwind-to-error helper if it is
shaped reusably; otherwise factor a small sync sibling
`call_engine_sync<T>(f: impl FnOnce() -> Result<T, napi::Error> + UnwindSafe) -> napi::Result<T>`
in the same module and route every entry point through it. Document
the choice with a single-line `// why:` comment if non-obvious. Keep
the no-WHAT comment policy.

Tighten the panic-surfacing test in `src/ts/tests/ffi-safety.test.ts`:
add at least one case that triggers a panic through a sync accessor
(or a documented test-hook on a sync entry point) and asserts the
thrown class is `FathomDbPanicError` with code `FDB_PANIC`. If no
existing test hook covers a sync path, gate a new
`forcePanicInAccessorForTest()` `#[napi]` function behind
`#[cfg(any(test, feature = "test-hooks"))]` mirroring
`force_panic_for_test()`.

### Finding 2 (`low`) — Cite AC-057a in `src/ts/tests/surface.test.ts`

Add the citation to the module docstring at the top of the file:

```ts
// Binds AC-057a (REQ-053): five-verb runtime SDK surface.
```

Match the comment style already in `errors.test.ts` /
`ffi-safety.test.ts` (whichever the existing pattern is).

## Required commands

```bash
cd /tmp/fdb-11b-napi-binding-20260516T210327Z
cargo test -p fathomdb-napi
cd src/ts && npm run build && npm test && cd ../..
./scripts/agent-verify.sh
```

All must pass. If `agent-verify.sh` flakes on
`ac_029_canonical_writes_complete_under_projection_stall` or
`t_safe_export_engine_error_exits_export_failure_66`, rerun once —
both are known timing-sensitive flakes unrelated to this slice.

## Discipline

- One commit acceptable; two (one per finding) is also fine. Last
  commit message must include the closure summary.
- No scope creep into 11c / 11d.
- Comment policy unchanged: no WHAT, only non-obvious WHY. No
  "added in 11b-fix-1" markers.

## Output

After all commands pass, write
`dev/plans/runs/11b-fix-1-output.json`:

```json
{
  "phase": "11b-fix-1",
  "baseline_sha": "bb4f64f",
  "branch": "phase-11b-napi-binding-20260516T210327Z",
  "head_sha": "<HEAD after final commit>",
  "findings_addressed": [
    "1: catch_unwind wrapped around every sync #[napi] entry point (counters, set_profiling, set_slow_threshold_ms, attach_subscriber); test asserts FathomDbPanicError on sync path",
    "2: AC-057a (REQ-053) citation added to surface.test.ts module doc"
  ],
  "tests_added_or_tightened": ["<test names>"],
  "agent_verify_result": "pass | fail (+ tail)",
  "next_step_for_orchestrator": "promote to 0.6.0-rewrite; respawn codex reviewer for clean PASS"
}
```

Then stop. Do not advance to 11c. Do not run the reviewer yourself.
