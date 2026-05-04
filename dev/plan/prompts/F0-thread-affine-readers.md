# Phase 9 Pack 6 — F.0 Thread-affine reader workers (implementer)

You are the **implementer** for Pack 6 phase F.0. The orchestrator (main
thread) spawned you. Do the work below, write the output JSON, commit
inside this worktree, and exit. Do **not** spawn other agents.

Authoritative spec: `dev/plan/prompts/02-pack6-handoff-readerpool-refactor.md`
sections §4 through §10. This prompt restates the binding parts; if any
conflict, the handoff wins.

## 1. Read order before coding

1. `dev/plan/prompts/02-pack6-handoff-readerpool-refactor.md` (full).
2. `dev/notes/performance-whitepaper-notes.md` §5 (do-not-retry) + §11
   (Pack 5 narrative + final synthesis).
3. `dev/plan/runs/STATUS.md` Pack 6 section.
4. Engine code:
   - `crates/fathomdb-engine/src/lib.rs` (search, ReaderPool wiring).
   - `crates/fathomdb-engine/src/reader_pool.rs` (or wherever
     `ReaderPool::Mutex<Vec<Connection>>` lives — locate via grep).
   - `tests/perf_gates.rs` (AC-017 / AC-018 / AC-020 harness).
   - Snapshot/cursor read path that backs AC-059b.

## 2. Mandate (from handoff §4)

Replace borrow/release pooling with thread-affine reader workers:

- `READER_POOL_SIZE = 8` unchanged.
- 8 long-lived reader worker threads spawned at `Engine::open`.
- Each worker owns exactly one read-only SQLite `Connection` for its
  lifetime. Connections never cross threads after startup.
- `Engine::search` dispatches a request to a worker via channel.
- Worker executes the full search on its own thread inside the same
  `BEGIN DEFERRED ... COMMIT` snapshot pattern AC-059b requires.
- Result returns over a response channel.

Out of scope: statement caching (Pack 5 E.1 closed parse track).
Schema/index changes. WAL2. `rusqlite` replacement. AC-020 contract
rewrite or formal deferral.

## 3. Required implementation shape (from handoff §5)

1. Private types only — names suggested, not binding:
   `ReaderWorkerPool`, `ReaderWorkerHandle`, `ReaderRequest`,
   `ReaderResponse`.
2. Bounded channels. Round-robin dispatch is fine. **No global mutex
   on the hot dispatch path.**
3. Move the borrowed-connection search path into a worker-owned search
   path. The worker still calls the same snapshot-preserving read
   helper (or a private refactor of it that preserves the contract).
4. Preserve all current contracts:
   - same-snapshot `projection_cursor`,
   - lifecycle / profiling / slow-signal behavior,
   - clean shutdown / `Engine::close` (workers join, connections drop),
   - AC-021 / AC-022 behavior,
   - **no public Rust API expansion** without an interface-doc / ADR.
5. FFI: never hardcode `i8` / `u8` for C interop — use
   `std::os::raw::c_char` / `c_int` (memory:
   `feedback_cross_platform_rust.md`).

## 4. Test discipline (from handoff §6)

Red-green-refactor. **Mandatory.** Required new tests:

1. Shutdown / integrity: all reader workers exit on `Engine::close`,
   all owned connections drop. (Likely an integration test asserting
   thread count returns to baseline + a Drop counter.)
2. Routing / concurrency stress: N concurrent searches complete; no
   request is lost or duplicated. (Counter-based assertion across all
   responses; prefer a cheap deterministic shape over a perf test.)
3. AC-059b cursor / read-snapshot coverage stays green unchanged.
4. AC-021 / AC-022 / AC-018 stay green unchanged.

Do **not** write a synthetic microbenchmark as the acceptance oracle.
AC-020 (`tests/perf_gates.rs`) is the decision metric. Run it
`AGENT_LONG=1` after green, N=5 medians.

Workflow:

1. Write failing tests for (1) and (2) first. Verify they fail in a
   meaningful way against the existing code (red).
2. Pause and write a short note in your output JSON
   `red_tests_written` field describing what failed and why before you
   start the green refactor. (The orchestrator will inspect this.)
3. Implement the worker pool and the search-path refactor (green).
4. Run AC-017 + AC-018 + AC-020 standalone (`cargo test -- --ignored
--test-threads=1`-equivalent per existing harness) and a
   workspace `cargo test --release`.
5. Capture AC-020 N=5 with `AGENT_LONG=1`.
6. Run `cargo clippy --all-targets -- -D warnings` and `cargo fmt
--check`.
7. If `dev/agent-verify.sh` exists, run it.

## 5. Hard rules

1. Do not weaken AC-020 bound formula (`tests/perf_gates.rs:245`).
2. Snapshot / cursor contract sacred (REQ-013 / AC-059b / REQ-055).
3. AC-018 must stay green at every commit.
4. No retry of `dev/notes/performance-whitepaper-notes.md` §5
   experiments without an §12 override (irrelevant here — F.0 is
   topology, not the §5 list).
5. No destructive git (`--force`, `reset --hard`, `--no-verify`,
   `--no-gpg-sign`).
6. No data migration in this packet.
7. Use `c_char` / `c_int` for FFI.
8. Do not chain subagents. You ARE the implementer.

## 6. Output JSON (mandatory)

Path (absolute): `/home/coreyt/projects/fathomdb/dev/plan/runs/F0-thread-affine-readers-output.json`

Required fields:

- `phase`: `"F.0"`
- `decision_self`: `"KEEP" | "REVERT" | "INCONCLUSIVE"` —
  self-classification per handoff §7 (orchestrator may override).
- `commit_sha`: green commit SHA inside the worktree.
- `branch`: this worktree's branch.
- `red_tests_written`: list of test names + why each failed before
  the green refactor.
- `ac017_runs`: 1+ runs with `passed` boolean.
- `ac018_runs`: 1+ runs with `drain_ms` + `passed`.
- `ac020_runs`: N=5 runs each with
  `sequential_ms`, `concurrent_ms`, `bound_ms`, `speedup`,
  `passed_5_33x`, `passed_packet_1_25_margin`.
- `ac020_summary`: `{sequential_ms, concurrent_ms, speedup}` each
  with `min/median/max/stddev`.
- `worker_pool_summary`: `{worker_count, dispatch_topology,
connection_lifetime, hot_path_synchronization}` (free-form prose
  short).
- `shutdown_test_result`: pass/fail + details.
- `routing_stress_test_result`: pass/fail + details.
- `unexpected_observations`: anything surprising.
- `alternative_hypothesis_if_revert`: what to try next.
- `data_for_pivot`: what the orchestrator should know if F.0 fails.
- `agent_verify_status`: pass / fail / not-run + reason.

## 7. Commit policy

- Commit at green only (after AC-017/018/020 captured). One commit
  preferred; small follow-up fixups are OK if isolated.
- Conventional Commits prefix: `perf(F0):` or `refactor(F0):` as
  appropriate.
- Do **not** push.
- Do **not** edit `dev/plan/runs/STATUS.md` — orchestrator owns it.

## 8. Decision rule (handoff §7, restated)

- KEEP iff `concurrent_median_ms <= 80` AND `speedup >= 5.0` AND
  AC-018 green AND no lifecycle/close regression.
- INCONCLUSIVE iff conc improves materially from 124 ms but stays in
  `81..100 ms` AND post-change perf shows the Rust-side mutex/handoff
  share collapsed.
- REVERT iff `concurrent_median_ms > 100` OR AC-018 red OR worker
  shutdown / delivery invariants fail OR new hot symbols still point
  primarily at Rust-side pool/handoff logic.

## 9. Stop rule

Exit after writing the output JSON and committing. The orchestrator
runs the codex reviewer next on KEEP / INCONCLUSIVE.
