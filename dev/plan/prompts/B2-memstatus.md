# Phase B.2 — `SQLITE_CONFIG_MEMSTATUS=0` (composes with B.1)

## Model + effort

Sonnet 4.6, intent: medium.

```bash
PHASE=B2-memstatus
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plan/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-pack5-${PHASE}-${TS}
git -C /home/coreyt/projects/fathomdb worktree add "$WT" -b "pack5-${PHASE}-${TS}" <B1_KEPT_COMMIT_SHA>
( cd "$WT" && \
  cat /home/coreyt/projects/fathomdb/dev/plan/prompts/B2-memstatus.md \
  | claude -p --model claude-sonnet-4-6 --effort medium \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --permission-mode bypassPermissions \
      --output-format json \
  > "$LOG" 2>&1 )
```

`<B1_KEPT_COMMIT_SHA>`: only spawn this if B.1 KEPT and B.1 alone did
not pass the AC-020 bound. If B.1 already passed, skip B.2 entirely.

Reviewer pass: optional but recommended (one-line FFI add). Use the
same review template as B.1.

## Log destination

- `dev/plan/runs/B2-memstatus-<ts>.log`
- `dev/plan/runs/B2-memstatus-output.json`
- `dev/plan/runs/B2-review-<ts>.md` (if reviewer run)

## Required reading + discipline

- **Read `AGENTS.md` first** — canonical agent operating manual.
  Especially §1 (TDD mandatory), §3 (`agent-verify.sh`), §5
  (failing test first).
- **Read `MEMORY.md` + `feedback_*.md`** — especially
  `feedback_tdd.md`, `feedback_cross_platform_rust.md` (c_int for
  FFI), `feedback_reliability_principles.md`.
- **TDD**: B.1's `sqlite3_threadsafe == 2` test should still pass.
  Add a parallel assertion that `sqlite3_config(MEMSTATUS=0)`
  returned `SQLITE_OK` (e.g. expose the rc via a `#[cfg(test)]`
  hook or by panic-on-init-failure). Failing test first.
- **Run `./scripts/agent-verify.sh`** before declaring success.

## Context

- Plan §5 B.2.
- Whitepaper §7.4 (cheap composes-with-7.3).
- Code anchor: `init_sqlite_runtime` (added by B.1; in
  `src/rust/crates/fathomdb-engine/src/lib.rs`, near the head of
  `Engine::open_locked` at line 740).

## Mandate

In the same `init_sqlite_runtime` `Once` block added by B.1, after
the `sqlite3_config(SQLITE_CONFIG_MULTITHREAD)` call but **before**
`sqlite3_initialize`, add:

```rust
let rc = unsafe {
    rusqlite::ffi::sqlite3_config(
        rusqlite::ffi::SQLITE_CONFIG_MEMSTATUS,
        0_i32,
    )
};
if rc != rusqlite::ffi::SQLITE_OK as i32 {
    return Err(EngineOpenError::Io {
        message: format!("sqlite3_config(MEMSTATUS=0) failed: {rc}"),
    });
}
```

Use the exact constant names from `rusqlite::ffi`. Match the type
discipline from B.1 (`std::os::raw::c_int` etc.).

## Acceptance criteria

- `cargo test -p fathomdb-engine --release` green.
- AC-018 green (re-check, MEMSTATUS removal touches the writer path
  too).
- AC-020 long-run: B.1+B.2 combined drops concurrent ms by **≥ 5%
  more** than B.1 alone (decision rule per plan §5 B.2). Otherwise
  REVERT B.2 and leave B.1 in place.
- Reviewer verdict not BLOCK.

## Files allowed to touch

- `src/rust/crates/fathomdb-engine/src/lib.rs` (extend
  `init_sqlite_runtime` only).
- §12 + whitepaper update.

## Files NOT to touch

- Anything outside `init_sqlite_runtime`.
- `Cargo.toml`.
- Schema, migrations, other crates.

## Verification commands

```bash
cargo test -p fathomdb-engine --release
AGENT_LONG=1 cargo test -p fathomdb-engine --release --test perf_gates \
    ac_020_reads_do_not_serialize_on_a_single_reader_connection \
    -- --nocapture  # x5
./scripts/agent-verify.sh
```

## Required output to orchestrator

```json
{
  "phase": "B2",
  "decision": "KEEP|REVERT|INCONCLUSIVE",
  "before_b1_alone": {
    "raw_runs": [{"sequential_ms": <n>, "concurrent_ms": <n>}, ...],
    "sequential_ms": <n>, "concurrent_ms": <n>, "bound_ms": <n>, "speedup": <f>, "stddev_concurrent": <n>, "n": 5
  },
  "after_b1_b2": {
    "raw_runs": [{"sequential_ms": <n>, "concurrent_ms": <n>}, ...],
    "sequential_ms": <n>, "concurrent_ms": <n>, "bound_ms": <n>, "speedup": <f>, "stddev_concurrent": <n>, "n": 5
  },
  "delta_concurrent_pct": <f>,
  "delta_sequential_pct": <f>,
  "delta_speedup_pct": <f>,
  "memstatus_config_return_code": <integer>,
  "ac017_status": "green|red:<numbers>",
  "ac018_status": "green|red:<numbers>",
  "ac018_drain_ms_after": <n>,
  "ac020_passes_5_33x": true|false,
  "decision_rule_met": true|false,
  "reviewer_verdict": "PASS|CONCERN|BLOCK|skipped",
  "loc_added": <n>, "loc_removed": <n>,
  "commit_sha": "<sha if KEEP>",
  "data_for_pivot": "<if delta < threshold: is allocator-stats genuinely cheap on this build, or did MULTITHREAD already remove the relevant lock? Implies B.3 lookaside is the next lever vs. abandoning the runtime-config track.>",
  "unexpected_observations": "<free text>",
  "next_phase_recommendation": "verification-gate|B3|C1|D1|done"
}
```

## Required output to downstream agents

- B.3 baseline = B.2 KEPT numbers (or B.1 KEPT numbers if B.2
  reverted).

## Update log

_(append B.1 KEPT numbers + decision rule before spawn)_
