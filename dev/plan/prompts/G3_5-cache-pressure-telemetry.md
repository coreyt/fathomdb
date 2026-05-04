# Phase 9 Pack 6.G — G.3.5 Cache-pressure telemetry micro-step (implementer)

You are the **implementer** for Pack 6.G phase G.3.5. Read-only
diagnostics on the G.1-landed tip. The orchestrator (main thread)
spawned you. Do the work below, write the output JSON, commit
inside this worktree, then exit. Do **not** spawn other agents.

This is a screening step ahead of G.4 (per-worker reader
`PRAGMA cache_size` sweep). G.4 only justifies its run cost if the
current G.1 tip shows real cache pressure on the read path. G.3.5
gathers per-worker, delta-windowed `DBSTATUS_CACHE_*` evidence and
recommends one of: SKIP_G4, PROBE_G4_MINUS_8000, FULL_G4_SWEEP.

Authoritative spec:
`dev/plan/prompts/04-pack6G-handoff-canonical-sqlite-tuning.md` +
this prompt.

## 1. Read order before coding

1. `dev/plan/prompts/04-pack6G-handoff-canonical-sqlite-tuning.md`
   (full).
2. `dev/plan/runs/G0-wal-checkpoint-telemetry-output.json` —
   page_cache 6.29% conc / 4.01× ratio / +34.5M cycle delta.
3. `dev/plan/runs/G1-reader-lookaside-output.json` — G.1
   topology + lookaside-applied evidence pattern (the
   debug-only `LookasideStatus` request broadcast is the model
   for the G.3.5 `CacheStatus` broadcast).
4. `dev/notes/performance-whitepaper-notes.md` §5 — Pack 5
   `cache_size` global tuning REVERT is on this list. G.3.5 only
   **measures** cache state; it does not tune `cache_size`. G.4
   (if greenlit) will carry the explicit topology override.
5. SQLite reference:
   <https://www.sqlite.org/c3ref/c_dbstatus_options.html> for
   `SQLITE_DBSTATUS_CACHE_HIT` / `_CACHE_MISS` / `_CACHE_USED`.
6. Engine code:
   - `crates/fathomdb-engine/src/lib.rs` — F.0 reader-worker
     dispatch, G.1's `ReaderRequest::LookasideStatus` broadcast,
     `read_lookaside_used_hiwtr`. The `CacheStatus` broadcast
     mirrors that pattern.
   - `tests/perf_gates.rs` — AC-020 split harness
     (`ac_020_concurrent_only --ignored`).

## 2. Mandate

Add a **debug-only** `ReaderRequest::CacheStatus { snapshot_label }`
broadcast that asks each F.0 reader worker to read its own
`SQLITE_DBSTATUS_CACHE_HIT` / `_CACHE_MISS` / `_CACHE_USED` and
return them.

Run a single `#[ignore]`-gated diagnostic test that:

1. Opens an `Engine` against the AC-020 fixture.
2. Drives a warmup (e.g. 16 dispatched searches against the
   seeded fixture so each worker takes ≥ 1 hot-path hit and the
   page cache reaches steady state).
3. Snapshots `CacheStatus { "pre" }` per worker.
4. Runs the AC-020 `ac_020_concurrent_only` sub-phase entry point
   one time (or, if cleaner, drives the concurrent-loop body
   directly via a private helper that already exists for AC-020).
5. Snapshots `CacheStatus { "post" }` per worker.
6. For each worker computes:
   - `delta_hit = post.cache_hit - pre.cache_hit`
   - `delta_miss = post.cache_miss - pre.cache_miss`
   - `delta_total = delta_hit + delta_miss`
   - `delta_miss_rate = delta_miss / delta_total` (guard against
     zero division).
   - `cache_used_post_bytes = post.cache_used`
   - `cache_used_post_pct_of_limit = post.cache_used /
(configured_cache_size_bytes)` — note: SQLite's default
     cache_size is `-2000` (KiB), i.e. 2 MiB per connection. If
     no override is in place, treat the limit as 2 MiB (2048 \*
     1024 bytes).
7. Writes per-worker results plus aggregate min/median/max into
   the output JSON.

Decision matrix (apply in the implementer's `decision_self`):

| Signal across workers                                                                                                      | `decision_self`       |
| -------------------------------------------------------------------------------------------------------------------------- | --------------------- |
| All workers: `cache_used_post_pct_of_limit < 50%` AND `delta_miss_rate < 5%`                                               | `SKIP_G4`             |
| Any worker: `cache_used_post_pct_of_limit >= 90%` OR `delta_miss_rate > 15%`                                               | `FULL_G4_SWEEP`       |
| Worker variance: `max(per_worker_metric) > 2 * min(per_worker_metric)` for HIT, MISS, or USED (and not already FULL_SWEEP) | `DISPATCH_BIAS_FLAG`  |
| Anything else (between thresholds, no variance flag)                                                                       | `PROBE_G4_MINUS_8000` |

`DISPATCH_BIAS_FLAG` is a separate finding, not a G.4 go/no-go.
It signals the round-robin dispatcher is hitting a non-uniform
worker subset, which is its own diagnostic. Surface it; do not
let it block the G.4 recommendation.

## 3. Required implementation shape

1. `ReaderRequest::CacheStatus { snapshot_label: String, respond:
SyncSender<CacheStatusReply> }` — debug-only enum variant
   (`#[cfg(debug_assertions)]`).
2. Worker arm matches the variant, calls a new private helper
   `read_cache_status(&Connection) -> CacheStatusReply` that
   issues three `sqlite3_db_status` calls.
3. Engine helper `cache_status_per_worker_for_test(label: &str)
-> Vec<CacheStatusReply>` — debug-only,
   `#[cfg(debug_assertions)] #[doc(hidden)]`. Broadcasts the
   request to every worker (not round-robin), collects replies in
   worker-id order. Same shape as G.1's
   `reader_lookaside_used_per_worker_for_test`.
4. Use rusqlite raw FFI:
   `unsafe { rusqlite::ffi::sqlite3_db_status(handle, op, &mut current, &mut hiwtr, 0) }`.
   Reset flag = 0 — we want monotonic counters, the test does
   delta arithmetic explicitly.
5. **No production code path changes.** No PRAGMA changes. No
   open-flag changes. No public surface expansion outside the
   `#[cfg(debug_assertions)]` debug-only convention.
6. The diagnostic test is `#[ignore]`-gated so it does not run
   in normal CI; only the orchestrator invokes it via
   `cargo test ... -- --ignored g3_5_cache_pressure_telemetry`.

## 4. Test discipline

Red-green-refactor.

1. Write the failing diagnostic test first (it will fail to
   compile until `CacheStatus` request + `cache_status_per_worker_for_test`
   helper exist). Capture the red failure mode in
   `red_tests_written`.
2. Implement the request variant + worker arm + helper.
3. Run AC-017 + AC-018 standalone to confirm no regression
   (debug-only changes should not move them, but verify).
4. Run the new diagnostic test: `cargo test --release -p
fathomdb-engine -- --ignored g3_5_cache_pressure_telemetry --nocapture`.
5. `./scripts/agent-verify.sh`.
6. `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check`.

## 5. Hard rules

1. **No production code changes** outside debug-only diagnostic
   plumbing.
2. Snapshot / cursor contract sacred (REQ-013 / AC-059b /
   REQ-055).
3. AC-018 stays green at every commit.
4. **No retry of `dev/notes/performance-whitepaper-notes.md` §5
   experiments.** G.3.5 measures only; it does NOT change
   `cache_size`, `mmap_size`, or any other §5 lever.
5. No destructive git.
6. No data migration.
7. FFI: `c_int` / `c_char`; never hardcode `i8` / `u8`.
8. Do not chain subagents.
9. **Use `./scripts/agent-verify.sh`** (not `dev/agent-verify.sh`).
10. **No public Rust API expansion** without ADR / interface-doc
    work; debug helpers under `#[cfg(debug_assertions)]` per F.0
    - G.1 convention.

## 6. Output JSON (mandatory)

Path (absolute):
`/home/coreyt/projects/fathomdb/dev/plan/runs/G3_5-cache-pressure-telemetry-output.json`

Required fields:

- `phase`: `"G.3.5"`.
- `decision_self`: `"SKIP_G4"` / `"PROBE_G4_MINUS_8000"` /
  `"FULL_G4_SWEEP"` per the §2 decision matrix.
- `dispatch_bias_flag`: bool — independent finding per §2.
- `commit_sha`: green commit SHA inside the worktree.
- `branch`: this worktree's branch.
- `red_tests_written`: list with `name`, `file`,
  `red_failure_mode`.
- `cache_size_limit_bytes_assumed`: SQLite default is 2 MiB per
  connection unless an override is present; record the value
  used for `cache_used_post_pct_of_limit` math + the source of
  that value (PRAGMA readback, default assumption, etc).
- `per_worker_telemetry`: array of length 8 with
  `{worker_idx, pre_hit, pre_miss, pre_used_bytes, post_hit,
 post_miss, post_used_bytes, delta_hit, delta_miss,
 delta_total, delta_miss_rate, cache_used_post_pct_of_limit}`.
- `aggregate_summary`:
  `{delta_miss_rate_min, delta_miss_rate_median,
 delta_miss_rate_max, delta_miss_rate_stddev,
 cache_used_pct_min, cache_used_pct_median,
 cache_used_pct_max, cache_used_pct_stddev}`.
- `decision_matrix_evaluation`: free-form prose explaining which
  matrix row matched + why.
- `dispatch_bias_evidence`: if flagged, show the per-worker
  variance numbers that triggered it.
- `ac017_runs`, `ac018_runs`: 1+ runs with `passed`. (No AC-020
  N=5 here; G.3.5 is a screening step, not a perf packet.)
- `unexpected_observations`.
- `data_for_pivot`: what should the orchestrator do per
  `decision_self`?
- `agent_verify_status`.
- `do_not_retry_cross_check`: same shape as G.0 / G.1.

## 7. Commit policy

- Commit at green only: `diag(G3_5):` prefix.
- One commit preferred (debug-only plumbing + diagnostic test).
- Do not push.
- Do not edit `dev/plan/runs/STATUS.md`.

## 8. Stop rule

Exit after writing the output JSON. The orchestrator decides:

- `SKIP_G4` → defer G.4; reassess G.2 or formal AC-020 deferral
  per Pack 6.G §9.
- `PROBE_G4_MINUS_8000` → spawn G.4 with a single sweep point,
  not the full grid.
- `FULL_G4_SWEEP` → spawn G.4 with `{-2000, -8000, -32000}` KiB.

`DISPATCH_BIAS_FLAG` (whichever G.4 path is picked) gets its
own follow-up note in the orchestrator's STATUS update.
