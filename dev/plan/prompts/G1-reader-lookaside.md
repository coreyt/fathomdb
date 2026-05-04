# Phase 9 Pack 6.G — G.1 Reader-worker lookaside tuning (implementer)

You are the **implementer** for Pack 6.G phase G.1. The orchestrator
(main thread) spawned you. Do the work below, write the output JSON,
commit inside this worktree, then exit. Do **not** spawn other
agents.

Authoritative spec:
`dev/plan/prompts/04-pack6G-handoff-canonical-sqlite-tuning.md`
sections §4 (G.1 row) + §6 + §7. If anything conflicts, the handoff
wins.

## 1. Read order before coding

1. `dev/plan/prompts/04-pack6G-handoff-canonical-sqlite-tuning.md`
   (full).
2. `dev/plan/runs/G0-wal-checkpoint-telemetry-output.json` —
   especially `recommendation`, `top_concurrent_mutex_atomic_call_sites`,
   and `next_phase_input.for_orchestrator` (sweep grid + sizing
   guidance from the SQLite docs).
3. `dev/plan/runs/F0-thread-affine-readers-output.json` — F.0
   topology summary (worker count, dispatch, connection lifetime).
4. `dev/notes/performance-whitepaper-notes.md` §5 (do-not-retry).
5. SQLite reference: `https://www.sqlite.org/malloc.html` §3
   "Lookaside Memory Allocator" — interface contract for
   `sqlite3_db_config(SQLITE_DBCONFIG_LOOKASIDE, ...)`.
6. Engine code:
   - `crates/fathomdb-engine/src/lib.rs` — F.0 reader worker
     `Connection::open` paths, profile-callback installation,
     existing PRAGMAs applied per reader connection.
   - `tests/perf_gates.rs`, `tests/reader_pool.rs`.

## 2. Mandate (handoff §4 G.1 row)

Configure SQLite per-connection lookaside on every F.0 reader
worker connection, **immediately after open**, before any
statement is prepared on that connection. Do **not** apply
lookaside to the writer connection in this packet (writer is on
the AC-018 / drain path, out of G.1 scope).

Required call shape (per SQLite docs):

```c
sqlite3_db_config(db, SQLITE_DBCONFIG_LOOKASIDE, NULL, slot_size, slot_count);
```

Notes:

- The middle pointer arg is a buffer the application supplies; pass
  `NULL` to let SQLite allocate the lookaside backing memory itself.
- Must be invoked before any other allocations on the connection,
  per the SQLite docs. Place this call inside the worker's open
  helper, ahead of the existing PRAGMAs that already run there.
- `SQLITE_DBCONFIG_LOOKASIDE` is in `sqlite3.h` and exposed by
  `libsqlite3-sys` via `ffi::SQLITE_DBCONFIG_LOOKASIDE` (or the
  numeric value `1005` if the symbol is not surfaced — prefer the
  named symbol when available).

Use rusqlite's safe wrapper if it exists, else
`unsafe { rusqlite::ffi::sqlite3_db_config(...) }` with the
`Connection::handle()` raw pointer — rusqlite's `db_config`
helper has historically been incomplete for the lookaside variant.
Check rusqlite's API in this repo's `Cargo.lock` first; document
which API path you used in the output JSON.

Initial slot size / slot count: **1200 bytes × 500 slots** per
worker (G.0 telemetry recommendation). If a quick sanity sweep is
needed before committing, try the canonical grid below and pick
the winner; do NOT bake the sweep into the production code path.

| slot_size (bytes) | slot_count               |
| ----------------- | ------------------------ |
| 512               | 128 (default)            |
| 512               | 500                      |
| 1200              | 250                      |
| 1200              | 500 (G.0 recommendation) |
| 1200              | 1000                     |

## 3. Required implementation shape

1. Lookaside configuration is a private function on the reader
   worker startup path. **No public Rust API expansion.** No new
   `pub fn ..._for_test` accessors unless `#[cfg(debug_assertions)]`
   gated and explicitly justified — F.0 reviewer block #1 standard.
2. Preserve all current contracts: same-snapshot
   `projection_cursor`, lifecycle / profiling / slow-signal
   behavior, clean shutdown, AC-021 / AC-022, AC-018 drain.
3. The lookaside call **must** complete before the worker installs
   the profile callback or runs any PRAGMA. If the worker open
   sequence is not ordered correctly, `SQLITE_DBCONFIG_LOOKASIDE`
   will be silently ignored — capture the rc into a debug
   assertion.
4. FFI: `c_int` / `c_char` per memory `feedback_cross_platform_rust.md`.
5. Use `./scripts/agent-verify.sh` (not `dev/agent-verify.sh`) per
   F.0 reviewer block #3.

## 4. Test discipline

Red-green-refactor.

Required new tests:

1. Lookaside-applied test: an integration test that opens an
   `Engine`, runs a small read, and asserts (via
   `sqlite3_db_status(... SQLITE_DBSTATUS_LOOKASIDE_USED, ...)` per
   `https://www.sqlite.org/c3ref/c_dbstatus_options.html`) that at
   least one lookaside slot was consumed on each reader worker
   connection. Use a `#[cfg(debug_assertions)]`-gated test helper
   if needed; do not expand the public surface.
2. Lookaside-ordering test: assert that
   `sqlite3_db_config(LOOKASIDE)` was called before any
   `prepare_*` / `execute_*` on the worker connection (otherwise
   the configuration is silently ignored). The cleanest form is a
   counter assertion: at the moment lookaside is configured,
   `SQLITE_DBSTATUS_LOOKASIDE_USED` is 0; after the first read it
   is > 0; after the test the rc of the original config call was
   `SQLITE_OK`.
3. AC-017 / AC-018 / AC-021 / AC-022 / AC-059b cursor / read-
   snapshot coverage stays green unchanged.

Workflow:

1. Write failing tests for (1) and (2); verify red.
2. Note the failure mode in `red_tests_written` of the output
   JSON.
3. Implement lookaside configuration on the worker open path.
4. Run AC-017 + AC-018 standalone, then `./scripts/agent-verify.sh`.
5. Capture AC-020 N=5 with `AGENT_LONG=1`.
6. `cargo clippy --all-targets -- -D warnings` and `cargo fmt
--check`.

## 5. Hard rules

1. Do not weaken AC-020 bound formula.
2. Snapshot / cursor contract sacred (REQ-013 / AC-059b / REQ-055).
3. AC-018 must stay green at every commit.
4. No retry of `dev/notes/performance-whitepaper-notes.md` §5
   experiments. Lookaside is **not** on the §5 list.
5. No destructive git.
6. No data migration.
7. Use `c_char` / `c_int` for FFI.
8. Do not chain subagents.
9. **Use `./scripts/agent-verify.sh`** (not `dev/agent-verify.sh`).
10. **No public Rust API expansion** without ADR / interface-doc
    work; `_for_test` helpers gated under `#[cfg(debug_assertions)]`.

## 6. Output JSON (mandatory)

Path (absolute):
`/home/coreyt/projects/fathomdb/dev/plan/runs/G1-reader-lookaside-output.json`

Required fields:

- `phase`: `"G.1"`.
- `decision_self`: `"KEEP"` / `"REVERT"` / `"INCONCLUSIVE"` per
  handoff §6.
- `commit_sha`: green commit SHA inside the worktree.
- `branch`: this worktree's branch.
- `red_tests_written`: list of test names + why each failed before
  the green refactor.
- `lookaside_config`: `{slot_size_bytes, slot_count, applied_via,
ordering_assertion_rc}` — `applied_via` records whether you
  used a rusqlite safe wrapper or a raw `ffi::sqlite3_db_config`
  call.
- `lookaside_used_evidence`: post-warmup
  `SQLITE_DBSTATUS_LOOKASIDE_USED` per worker (or aggregated) +
  any `LOOKASIDE_HIT` / `LOOKASIDE_MISS_*` counters that are
  reachable.
- `ac017_runs`, `ac018_runs`, `ac020_runs` (N=5),
  `ac020_summary`: same shape as F.0 / G.0 outputs.
- `comparison_to_g0_baseline`: deltas vs the G.0 baseline-of-
  record (seq_median 552 ms, conc_median 168 ms, speedup_median
  3.339× on this host).
- `optional_sweep_results` (if you ran the sweep grid): array of
  `{slot_size, slot_count, conc_median_ms, speedup_median}`.
- `unexpected_observations`.
- `alternative_hypothesis_if_revert`.
- `data_for_pivot` — if G.1 lands KEEP, what's the next G.4 / G.2
  candidate per evidence? G.0 telemetry already calls out
  `page_cache` (pcache1 global mutex) as the second-largest
  growth path.
- `agent_verify_status`.
- `do_not_retry_cross_check`: same shape as G.0 (B.1 / C.1 / E.1 /
  cache_size / NOMUTEX attestations).

## 7. Commit policy

- Commit at green only. One commit preferred (
  `perf(G1):` or `refactor(G1):` prefix).
- Do not push.
- Do not edit `dev/plan/runs/STATUS.md`.

## 8. Decision rule (handoff §6 / handoff §4 G.1 row)

- KEEP iff AC-017 + AC-018 stay green AND
  `concurrent_median_ms < G0_baseline.concurrent_median_ms - 1×stddev`
  (i.e. > ~12 ms improvement on this host) AND speedup_median ≥
  G0_baseline.speedup_median (no ratio worsening).
- INCONCLUSIVE iff numerics flat within 1× stddev. Implementer
  should still record the lookaside-applied evidence so a follow-
  up phase has a clean baseline.
- REVERT iff AC-018 red OR concurrent_median worsens beyond 1×
  stddev OR ratio worsens OR a contract / public-surface
  violation lands in the diff.

## 9. Stop rule

Exit after writing the output JSON and committing. Orchestrator
runs the codex `gpt-5.4` high reviewer next on KEEP / INCONCLUSIVE.
