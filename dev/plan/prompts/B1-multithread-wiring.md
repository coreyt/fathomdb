# Phase B.1 — Runtime MULTITHREAD wiring (Opus high; reviewer mandatory)

## Model + effort

Opus 4.7, intent: high. Spawn from main thread:

```bash
PHASE=B1-multithread-wiring
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plan/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-pack5-${PHASE}-${TS}
git -C /home/coreyt/projects/fathomdb worktree add "$WT" -b "pack5-${PHASE}-${TS}" <A0_COMMIT_SHA>
( cd "$WT" && \
  cat /home/coreyt/projects/fathomdb/dev/plan/prompts/B1-multithread-wiring.md \
  | claude -p --model claude-opus-4-7 --effort high \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --permission-mode bypassPermissions \
      --output-format json \
  > "$LOG" 2>&1 )
```

Reviewer pass after implementer (mandatory, FFI ordering risk):

```bash
RPHASE=B1-review
RTS=$(date -u +%Y%m%dT%H%M%SZ)
RLOG=/home/coreyt/projects/fathomdb/dev/plan/runs/${RPHASE}-${RTS}.md
( cd "$WT" && \
  cat /home/coreyt/projects/fathomdb/dev/plan/prompts/review-experiment.md \
       /home/coreyt/projects/fathomdb/dev/plan/prompts/review-phase78-robustness.md \
  | codex exec --model gpt-5.4 -c model_reasoning_effort=high \
  > "$RLOG" 2>&1 < /dev/null )
```

## Log destination

- stdout/stderr (impl): `dev/plan/runs/B1-multithread-wiring-<ts>.log`
- structured (impl): `dev/plan/runs/B1-multithread-wiring-output.json`
- reviewer verdict: `dev/plan/runs/B1-review-<ts>.md`

## Required reading + discipline

- **Read `AGENTS.md` first** — canonical agent operating manual.
  Especially §1 (TDD mandatory, ADRs authoritative, Public surface =
  contract), §3 (`agent-verify.sh` after every meaningful edit),
  §4 (verification ordering), §5 (failing test first; test files
  read-only during fix-to-spec; no agent-generated oracles).
- **Read `MEMORY.md` + `feedback_*.md`** — especially
  `feedback_tdd.md` (red-green-refactor),
  `feedback_cross_platform_rust.md` (c_char / c_int rules for FFI
  — load-bearing for this phase),
  `feedback_reliability_principles.md` (net-negative LoC, no punt).
- **TDD path is mandatory** for this phase (production code change
  with FFI). Mandate below makes red→green→refactor explicit; honor
  it.
- **Run `./scripts/agent-verify.sh`** before declaring success.

## Context

- Plan §5 B.1.
- Whitepaper §5 (the **earlier** B.1-shaped revert: placed inside
  `register_sqlite_vec_extension` Once block — silently no-op'd).
  This phase explicitly fixes ordering and asserts the return code so
  the previous failure mode is caught.
- Whitepaper §7.3 (correct sequence).
- Memory: `feedback_cross_platform_rust.md` — any new FFI uses
  `std::os::raw::c_char`, never hardcoded `i8`/`u8`.
- Memory: `feedback_tdd.md` — red-green-refactor.
- Code anchors:
  - `Engine::open_locked` — `src/rust/crates/fathomdb-engine/src/lib.rs:740`.
    Calls `register_sqlite_vec_extension` at line 746, then
    `Connection::open` at line 747.
  - `register_sqlite_vec_extension` — lib.rs:1824
    (`Once`-guarded; calls `sqlite3_auto_extension` which itself
    triggers `sqlite3_initialize`).
  - `READER_POOL_SIZE = 8` — lib.rs:48.
  - `ReaderPool` — lib.rs:158.
- Reader connections opened at lib.rs:775 (in the
  `for _ in 0..READER_POOL_SIZE` loop in `open_locked`).
- A.0 / A.1 / A.3 outputs (read these for baseline + evidence):
  - `dev/plan/runs/A1-perf-capture-output.json`
  - `dev/plan/runs/A3-secondary-diagnostics-output.json`

## Mandate

Wire `sqlite3_config(SQLITE_CONFIG_MULTITHREAD)` correctly so that
THREADSAFE drops from `1` (serialized) to `2` (multi-thread) at
runtime, **before any** `Connection::open` or
`sqlite3_auto_extension` call.

### Required behavior

1. New module-level `init_sqlite_runtime()` function:
   - `Once`-guarded (one-shot per process).
   - Sequence:
     1. `sqlite3_shutdown()` (idempotent if not initialized — capture
        return code; `SQLITE_OK` or `SQLITE_MISUSE` both acceptable).
     2. `sqlite3_config(SQLITE_CONFIG_MULTITHREAD)` — capture return
        code; **assert `SQLITE_OK`**, else surface
        `EngineOpenError::Io { message: "sqlite3_config(MULTITHREAD) failed: <code>" }`.
     3. `sqlite3_initialize()` — capture return code; assert `SQLITE_OK`.
   - All FFI uses `rusqlite::ffi` types and `std::os::raw::c_int`.

2. Call site: at the head of `Engine::open_locked` (lib.rs:740),
   **before** `register_sqlite_vec_extension()` (lib.rs:746). Order:
   - `init_sqlite_runtime()?;`
   - `register_sqlite_vec_extension();`
   - `Connection::open(&path)?;`

3. After init, capture and log
   `unsafe { rusqlite::ffi::sqlite3_threadsafe() }`. Expect `2`.
   Log it via lifecycle subscriber (or, if simpler for this phase,
   via `eprintln!` in a `#[cfg(debug_assertions)]` block — do not add
   a new lifecycle event for this).

4. Test (red-green-refactor):
   - **Red**: write a `#[test]` that opens an `Engine`, asserts
     `unsafe { rusqlite::ffi::sqlite3_threadsafe() } == 2`. Run on
     main first to confirm it fails (records baseline).
   - **Green**: implement the init function. Test passes.
   - **Refactor**: tighten error path; ensure init runs only once
     (re-open `Engine` second time does not call `sqlite3_shutdown`).

### Risk mitigation

- `sqlite3_shutdown` after another connection is open is a misuse.
  The `Once` guard plus call-site at `open_locked` head guarantees no
  prior connection in the same process.
- `sqlite3_initialize` is called implicitly by `Connection::open`, so
  if this ever runs after a connection was already opened (e.g. from
  a different code path) the `sqlite3_config` will return
  `SQLITE_MISUSE` — the assert catches it.

## Acceptance criteria

- `cargo test -p fathomdb-engine --release` is green.
- New test asserting `sqlite3_threadsafe() == 2` after `Engine::open`
  is green.
- AC-018 stays green (re-run; concurrent drain unchanged).
- AC-020 long-run improves: **decision rule from A.4** (typically
  concurrent drops by ≥ 30% vs A.1 baseline AND speedup ≥ 5.0x → KEEP).
- Reviewer verdict not BLOCK.
- No new FFI hardcodes `i8` or `u8` for `c_char` / `c_int`
  (memory: `feedback_cross_platform_rust.md`).
- §12 of the plan gets one line; whitepaper §4 (kept) or §5
  (reverted) gets a full entry with before/after numbers (N=5).

## Files allowed to touch

- `src/rust/crates/fathomdb-engine/src/lib.rs` (insert
  `init_sqlite_runtime` + call from `open_locked`).
- `src/rust/crates/fathomdb-engine/tests/lifecycle_observability.rs`
  (or a new test file) — add the `sqlite3_threadsafe == 2` test.
- `dev/plan/runs/B1-multithread-wiring-output.json` and `.log`.
- §12 + whitepaper update (only after KEEP decision).

## Files NOT to touch

- `Cargo.toml` (this is a runtime config, not a build flag — that is
  Phase C.1).
- Schema files / migrations.
- Other crates in `src/rust/crates/`.
- Reader-side `PRAGMA` calls — already in §5 reverted list, do not
  touch them.
- Test files outside the chosen one.

## Verification commands

```bash
cargo test -p fathomdb-engine --release \
    --test lifecycle_observability  # or whichever test file holds the new assertion
cargo test -p fathomdb-engine --release
AGENT_LONG=1 cargo test -p fathomdb-engine --release --test perf_gates \
    ac_020_reads_do_not_serialize_on_a_single_reader_connection \
    -- --nocapture
# Repeat the AGENT_LONG run 5 times back to back; record min/median/max.
./scripts/agent-verify.sh
```

## Required output to orchestrator

```json
{
  "phase": "B1",
  "decision": "KEEP|REVERT|INCONCLUSIVE",
  "before": {
    "sequential_ms": <n>, "concurrent_ms": <n>, "bound_ms": <n>, "speedup": <f>, "n": 5,
    "raw_runs": [{"sequential_ms": <n>, "concurrent_ms": <n>}, ...],
    "source": "A.1 baseline | re-measured at this commit"
  },
  "after": {
    "raw_runs": [{"sequential_ms": <n>, "concurrent_ms": <n>, "bound_ms": <n>, "speedup": <f>}, ...],
    "sequential_ms": {"min": <n>, "median": <n>, "max": <n>, "stddev": <n>},
    "concurrent_ms": {"min": <n>, "median": <n>, "max": <n>, "stddev": <n>},
    "bound_ms":      {"min": <n>, "median": <n>, "max": <n>},
    "speedup":       {"min": <f>, "median": <f>, "max": <f>, "stddev": <f>},
    "n": 5
  },
  "delta_concurrent_pct": <f>,
  "delta_sequential_pct": <f>,
  "delta_speedup_pct": <f>,
  "ac017_status": "green|red:<numbers>",
  "ac018_status": "green|red:<numbers>",
  "ac018_drain_ms_after": <n>,
  "ac020_passes_5_33x": true|false,
  "ac020_passes_packet_1_25_margin": true|false,
  "threadsafe_before_open": <integer>,
  "threadsafe_after_open": <integer>,
  "sqlite3_config_return_code": <integer>,
  "sqlite3_shutdown_return_code": <integer>,
  "sqlite3_initialize_return_code": <integer>,
  "init_runs_once_verified": true|false,
  "decision_rule": "<rule from A.4>",
  "decision_rule_met": true|false,
  "kill_criteria_met": true|false,
  "reviewer_verdict": "PASS|CONCERN|BLOCK",
  "reviewer_concerns": ["<text>", ...],
  "reviewer_log": "dev/plan/runs/B1-review-<ts>.md",
  "phase78_review_verdict": "PASS|CONCERN|BLOCK",
  "phase78_review_log": "dev/plan/runs/B1-review-phase78-<ts>.md",
  "loc_added": <n>, "loc_removed": <n>,
  "files_changed": ["src/rust/crates/fathomdb-engine/src/lib.rs", ...],
  "commit_sha": "<sha if KEEP>",
  "git_status_clean_after_revert": true|null,
  "data_for_pivot": "<if KEEP but bound still red: which next experiment is most promising and why; if REVERT: was the intervention silently no-op'd (config rc != OK), or applied-but-didn't-help (rc OK but threadsafe stayed at 1, or threadsafe == 2 but ratio unchanged)? Each answer points at a different next move — name it.>",
  "unexpected_observations": "<free text — e.g. one of the 5 runs was a clear outlier; AC-018 changed in an unexpected direction; sqlite3_config returned OK but threadsafe stayed at 1>",
  "next_phase_recommendation": "verification-gate|B2|B3|C1|D1|REVERT_AND_RECONSIDER"
}
```

## Required output to downstream agents

- B.2 (if needed) baseline becomes B.1's `after` numbers (composing).
- Verification gate: re-runs the 5x AGENT_LONG cycle and the full
  engine suite.
- Reviewer log path is consumed by the §8 verification gate before
  the orchestrator commits.

## Update log

- 2026-05-03 — A.2 PICK_B1 (main thread Opus 4.7, no recapture
  needed). Per `dev/plan/runs/A2-symbol-focus-output.json` and
  whitepaper §11 A.2 entry. A.3 / A.4 may still run; B.1 is
  pre-aligned with A.2's classification.
- A.1 baseline (carry numbers into the JSON `before` block):
  - sequential N=5 `[189,199,182,179,176]` ms; median 182, stddev 9.2
  - concurrent N=5 `[120,110,117,115,112]` ms; median 115, stddev 4.0
  - speedup_observed 1.58×; required 5.33×; gap 3.4×
- A.2 classification (`before` distribution):
  - mutex_atomic 6.45% seq → 36.98% conc (5.73× growth, +262M cycles)
  - allocator 1.60% seq → 3.20% conc (2.00× growth, +19M cycles)
  - vec0_fts 24.12% → 11.43% / sql_parse 10.08% → 7.07% / page_cache,
    vdbe, our_code all flat-or-shrinking
  - Top concurrent symbols to watch on `after`: `__aarch64_swp4_rel`
    11.2%, `__aarch64_cas4_acq` 9.8%, `___pthread_mutex_lock` 6.8%,
    `__aarch64_swp4_acq` 5.9%, `lll_mutex_lock_optimized` 1.8%,
    `__GI___lll_lock_wait` ~1.2%.
- B.1 spawn baseline = `ca0d8f0` (A.1 commit, current
  `0.6.0-rewrite` tip after orchestrator bookkeeping commit
  `1f89169`). Replace `<A0_COMMIT_SHA>` in spawn block with
  `0.6.0-rewrite` (resolves to the current tip — `1f89169` at
  spawn time, descendant of `ca0d8f0`).
- Decision rule for B.1 KEEP/REVERT (per AC §1):
  - KEEP iff concurrent median drops ≥ 30% vs A.1 baseline (115ms →
    ≤ 80.5ms) AND speedup ≥ 5.0× AND AC-018 stays green.
  - INCONCLUSIVE if speedup improves but does not reach 5.0× —
    proceed to A.1 recapture against B.1 branch + re-classify;
    candidate next is B.3 (per-conn lookaside) targeting the
    secondary allocator share.
  - REVERT if concurrent median regresses or AC-018 turns red.
- A.2 alternative-if-fails: if B.1 lands without speedup change,
  re-capture must show whether residual mutex_atomic is still in
  the SQLite global mutex (B.1 not applied to right connections)
  vs a different mutex (rusqlite-side or `ReaderPool::acquire`).
- Reviewer: codex with `gpt-5.4` mandatory per plan §0.1 / resume §4.
