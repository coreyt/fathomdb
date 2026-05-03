# Phase B.3 — Per-connection lookaside (`SQLITE_CONFIG_LOOKASIDE`)

## Model + effort

Sonnet 4.6, intent: medium.

```bash
PHASE=B3-lookaside
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plan/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-pack5-${PHASE}-${TS}
git -C /home/coreyt/projects/fathomdb worktree add "$WT" -b "pack5-${PHASE}-${TS}" <PRIOR_KEPT_COMMIT_SHA>
( cd "$WT" && \
  cat /home/coreyt/projects/fathomdb/dev/plan/prompts/B3-lookaside.md \
  | claude -p --model claude-sonnet-4-6 --effort medium \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --permission-mode bypassPermissions \
      --output-format json \
  > "$LOG" 2>&1 )
```

`<PRIOR_KEPT_COMMIT_SHA>`: head of B.2 (KEPT) or B.1 (if B.2 reverted).

**Skip if** B.1 (or B.1+B.2) already passes the 5.33x bound.

Reviewer optional.

## Log destination

- `dev/plan/runs/B3-lookaside-<ts>.log`
- `dev/plan/runs/B3-lookaside-output.json`

## Required reading + discipline

- **Read `AGENTS.md` first** — canonical agent operating manual.
  Especially §1 (TDD mandatory), §3 (`agent-verify.sh`), §5
  (failing test first).
- **Read `MEMORY.md` + `feedback_*.md`** — especially
  `feedback_tdd.md`, `feedback_cross_platform_rust.md` (FFI types),
  `feedback_reliability_principles.md`.
- **TDD**: failing test first — assert that lookaside is configured
  on each reader (e.g. via `sqlite3_db_status(SQLITE_DBSTATUS_LOOKASIDE_USED)`
  > 0 after a known-allocating query, or rc check on the config call
  exposed via a `#[cfg(test)]` hook). Then implement.
- **Run `./scripts/agent-verify.sh`** before declaring success.

## Context

- Plan §5 B.3.
- Whitepaper §7.5.
- §5 reverted list **explicitly forbids** retrying per-connection
  `cache_size` / `mmap_size` tuning — do **NOT** touch
  `pragma_update("cache_size", ...)`. Lookaside is a different knob,
  via `sqlite3_db_config` per connection, not a PRAGMA.
- Code anchor: reader open loop at
  `src/rust/crates/fathomdb-engine/src/lib.rs:773-784`.

## Mandate

Configure a per-connection lookaside slab on each reader and on the
writer connection.

Two acceptable shapes; pick whichever fits cleaner:

1. **Global default via `SQLITE_CONFIG_LOOKASIDE`**: in
   `init_sqlite_runtime` (B.1), call
   `sqlite3_config(SQLITE_CONFIG_LOOKASIDE, slot_size, cnt)`. Affects
   all subsequent connections.
2. **Per-connection via `sqlite3_db_config(SQLITE_DBCONFIG_LOOKASIDE, ...)`**:
   right after each `Connection::open` (writer at lib.rs:747, readers
   at lib.rs:775).

Recommended starting values: `slot_size = 128`, `cnt = 500`. These are
the SQLite defaults written more aggressively (default is 1200/500;
small slots favor frequent small allocations from prepare/step,
which is exactly the workload here).

Use `c_int` from `std::os::raw` for any FFI integers.

## Acceptance criteria

- `cargo test -p fathomdb-engine --release` green.
- AC-018 green.
- AC-020 long-run: concurrent ms drops by **≥ 10%** vs prior KEPT
  baseline (B.1 or B.1+B.2). Otherwise REVERT.

## Files allowed to touch

- `src/rust/crates/fathomdb-engine/src/lib.rs` only.
- §12 + whitepaper update.

## Files NOT to touch

- `Cargo.toml`.
- Reader-side `PRAGMA cache_size` / `mmap_size` (forbidden).
- Other crates.

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
  "phase": "B3",
  "decision": "KEEP|REVERT|INCONCLUSIVE",
  "lookaside_route": "config_global|db_config_per_connection",
  "lookaside_slot_size": <n>,
  "lookaside_count": <n>,
  "before": {
    "raw_runs": [{"sequential_ms": <n>, "concurrent_ms": <n>}, ...],
    "sequential_ms": <n>, "concurrent_ms": <n>, "bound_ms": <n>, "speedup": <f>, "stddev_concurrent": <n>, "n": 5
  },
  "after": {
    "raw_runs": [{"sequential_ms": <n>, "concurrent_ms": <n>}, ...],
    "sequential_ms": <n>, "concurrent_ms": <n>, "bound_ms": <n>, "speedup": <f>, "stddev_concurrent": <n>, "n": 5
  },
  "delta_concurrent_pct": <f>,
  "delta_sequential_pct": <f>,
  "delta_speedup_pct": <f>,
  "lookaside_hit_stats": {"per_conn_hits": <n>, "per_conn_misses": <n>, "available": "yes|no:<reason>"},
  "decision_rule_met": true|false,
  "ac017_status": "green|red:<numbers>",
  "ac018_status": "green|red:<numbers>",
  "ac018_drain_ms_after": <n>,
  "ac020_passes_5_33x": true|false,
  "reviewer_verdict": "PASS|CONCERN|BLOCK|skipped",
  "loc_added": <n>, "loc_removed": <n>,
  "commit_sha": "<sha if KEEP>",
  "guard_check_no_cache_size_pragma_added": true,
  "data_for_pivot": "<if lookaside hits are high but concurrent ms barely moves, allocator was not the binding constraint — promote D.1 (statement-count reduction) or C.1 (rebuild). If hits stay low, lookaside slab is sized wrong; report the per-conn hit ratio and a suggested re-sizing before abandoning.>",
  "unexpected_observations": "<free text>",
  "next_phase_recommendation": "verification-gate|C1|D1|done"
}
```

## Required output to downstream agents

- C.1 (if needed) baseline = B-stack KEPT numbers.
- D.1 (parallel track) baseline = same.

## Update log

_(append prior KEPT baseline + cache_size guard reminder before spawn)_
