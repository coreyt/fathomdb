# Performance Whitepaper Notes — Engine Read Path & AC-020 Concurrency Gate

Status as of 2026-05-03. Branch: `0.6.0-rewrite`. Durable notes for resuming
the AC-020 investigation later. Reference, do not duplicate, code already in
the repo.

---

## 1. Context / framing

### AC-020 contract

- Defined in `dev/test-plan.md` (perf gates section) and exercised by
  `src/rust/crates/fathomdb-engine/tests/perf_gates.rs`.
- Test of record:
  `ac_020_reads_do_not_serialize_on_a_single_reader_connection`
  (`tests/perf_gates.rs:211`).
- Bound (`tests/perf_gates.rs:245`):

  ```text
  bound = sequential * (1.5 / AC020_THREADS)
  ```

  with `AC020_THREADS = 8` (`tests/perf_gates.rs:11`). Allowed slowdown vs
  the ideal 8x parallel speedup is 1.5x. Equivalently: concurrent must
  achieve >= 5.33x speedup over sequential.

### Test workload

- `seed_ac020_fixture` (`tests/perf_gates.rs:102`): 4 vector-only docs +
  4 hybrid (vector + FTS) docs.
- `run_ac020_mix` (`tests/perf_gates.rs:121`): 50 rounds, 4 queries per
  round (mixed vector / hybrid / FTS), per thread.
- Embedder: `RoutedEmbedder` (`tests/perf_gates.rs:43`), 3 routes,
  deterministic.
- Sequential phase: `AC020_THREADS` serial runs of `run_ac020_mix` on a
  single engine (`tests/perf_gates.rs:222`).
- Concurrent phase: `AC020_THREADS` threads, each calling `run_ac020_mix`,
  synchronized on a `Barrier` (`tests/perf_gates.rs:228`).

### Hardware where the numbers below were taken

- ARMv8 12-core (Linux 5.15 Tegra), `nproc` = 12.
- Bundled SQLite via `rusqlite = { version = "0.31", features = ["bundled"] }`
  (`src/rust/crates/fathomdb-engine/Cargo.toml:13`). Default build flag is
  `SQLITE_THREADSAFE=1` (serialized — full per-call mutex around every
  SQLite entry point, plus global allocator/pcache/PRNG mutexes).

---

## 2. Current status of perf gates

| Gate   | State          | Notes                                     |
| ------ | -------------- | ----------------------------------------- |
| AC-017 | green          |                                           |
| AC-018 | green          | After persistent projection-runtime conns |
| AC-020 | red (long-run) | Best retained: 3.6x speedup, need 5.33x   |

### AC-020 retained run (best)

- sequential = 456 ms
- concurrent = 127 ms
- bound = 85 ms (= 456 \* 1.5 / 8)
- speedup = 3.59x; required = 5.33x

### Other observed runs from the investigation

- 473 ms / 135 ms (earlier, similar regime).
- 183 ms / 123 ms (smaller-seed configuration, not the gate fixture).
- 188 ms / 113 ms (after subscriber-empty fast-path; ~8% concurrent gain).

All four runs sit in the 1.5x–3.6x band — a signature consistent with a
shared mutex bottleneck rather than CPU saturation (12 cores, 8 threads).

---

## 3. Breadcrumbs — commits and code anchors

### Commits worth referencing

- `b4a3261` — docs(notes): Phase 9 implementer handoff.
- `ba900a1` — profile-callback SAFETY notes.
- `2ef0efb` — search projection_cursor read from reader-tx snapshot.
- `09415b4` — reader pool introduction.
- `c054487` — Subscriber registry.

### Engine code anchors (`src/rust/crates/fathomdb-engine/src/lib.rs`)

- `READER_POOL_SIZE = 8` — line 48.
- `struct ReaderPool` — line 158 (declared at 158; field at 56; constructor
  invocation at 707; pool fill loop at 773).
- `Engine::search` — line 869; calls `search_inner` at 872.
- `Engine::search_inner` — line 942; calls `read_search_in_tx` at 965.
- `read_search_in_tx` — line 1283.
- `register_sqlite_vec_extension` — line 1824 (Once-guarded; the natural
  hook for any global `sqlite3_config` call).
- `install_profile_callback` — line 2401.
- `uninstall_profile_callback` — line 2436.
- `profile_callback_trampoline` — line 2457.

### Lifecycle anchors (`src/rust/crates/fathomdb-engine/src/lifecycle.rs`)

- `pub(crate) struct SubscriberRegistry` — line 115.
- `pub(crate) struct Counters` — line 235.

### Test anchors (`src/rust/crates/fathomdb-engine/tests/perf_gates.rs`)

- `AC020_THREADS` — line 11.
- `RoutedEmbedder` — line 43.
- `seed_ac020_fixture` — line 102.
- `run_ac020_mix` — line 121.
- `ac_020_reads_do_not_serialize_on_a_single_reader_connection` — line 211.

---

## 4. Experiments tried — kept

Landed in `lib.rs`; moved AC-018 from red to green and shaved AC-020 latency
without closing the gap.

- Persistent projection-runtime SQLite connections (dispatcher + worker
  long-lived).
- `prepare_cached` on long-lived projection runtime connections.
- Batched projection commits (one commit per batch instead of per row).
- Compute query embedding vector before borrowing a pooled reader. Shorter
  pooled-reader hold time; snapshot semantics preserved because the read
  transaction still wraps the actual read.

---

## 5. Experiments tried — reverted (with reason)

- Single-statement vec0 + canonical join materialization. No measurable
  gain on the real gate; added query-plan complexity.
- Reader-side `cache_size` / `mmap_size` tuning. Per-connection page
  caches did not change the concurrency ratio.
- Read-path `prepare_cached` tweaks. Improved sequential latency but did
  not improve the concurrency ratio — pointing at a parallelism ceiling
  rather than a per-call cost.
- Reader open flags `READ_ONLY | NO_MUTEX`. NOMUTEX only drops the
  per-connection mutex; we still hit the global mutexes from THREADSAFE=1.
- Read-tx teardown by drop instead of explicit `commit()`. Equivalent.
- Increasing `READER_POOL_SIZE` above 8. No improvement; bottleneck is
  not pool capacity.
- `SubscriberRegistry` empty-subscribers AtomicUsize fast-path. Bought
  ~8% concurrent improvement only; not the bottleneck.
- Runtime `sqlite3_config(SQLITE_CONFIG_MULTITHREAD)` placed inside the
  existing `register_sqlite_vec_extension` Once block (lib.rs:1824). No
  measurable change. Hypothesis: by the time `register_sqlite_vec_extension`
  runs, `sqlite3_initialize()` has already been triggered (rusqlite calls
  it on first connection open), so `sqlite3_config` returns
  `SQLITE_MISUSE` and is silently ignored. Not yet validated by capturing
  the return code.

---

## 6. Hypothesis hierarchy for the remaining gap

We have a 12-core box, 8 worker threads, mostly read-only work on a tiny
fixture. CPU is not the bound. Speedup of 1.5x–3.6x is the classic
shape of contention on a single shared mutex.

### Primary: SQLite global allocator mutex (THREADSAFE=1)

- Bundled SQLite default is `SQLITE_THREADSAFE=1` ("serialized"), which
  enables both per-connection mutexes and three global mutexes:
  allocator (`mem1`/`mem3`), page cache (`pcache1`), PRNG.
- `SQLITE_OPEN_NOMUTEX` only drops the per-connection mutex; the global
  ones stay.
- 8 readers all malloc/free per query (statement prepare, row materialize,
  string/blob copies) → those callers serialize on the allocator mutex.
- Pattern fits the observed 1.5x–3.6x band.

### Secondary: pcache mutex; lookaside not configured per-connection

- Page cache lookups under contention also pass through a global mutex
  in serialized mode.
- Per-connection lookaside is not configured, so small allocations go to
  the global allocator.

### Tertiary: per-search prepare cost amplifies allocator traffic

- Each `Engine::search` runs ~4 prepares (vector route + FTS route +
  hybrid join + projection cursor read). More prepares → more malloc →
  more time inside the suspect mutex per query → worse contention.

---

## 7. Untried options ranked by likely payoff

### 7.1 Diagnostic first (do this before more reverts)

- `perf record -g --call-graph dwarf` of the AC-020 binary in two modes
  (sequential and concurrent), then `perf report` and diff the time spent
  in: `pthread_mutex_lock`, `pthreadMutexEnter`, `mem1Malloc`/`mem1Free`,
  `pcache1Fetch`/`pcache1Truncate`. Whichever frame grows
  super-linearly under concurrency is the bottleneck.
- Cost ~30 minutes. Prevents another revert cycle by confirming which
  mutex is the actual problem before structural rework.

### 7.2 Rebuild bundled SQLite with `SQLITE_THREADSAFE=2`

- "Multi-thread" mode: drops the global allocator/pcache mutexes and the
  per-connection mutex; caller is responsible for not sharing connections
  across threads.
- Our `ReaderPool` already guarantees single-thread use of any one
  connection. NOMUTEX is already set. THREADSAFE=2 is the missing
  piece.
- Highest-payoff fix if perf data confirms the allocator-mutex hypothesis.

### 7.3 Runtime `sqlite3_config(SQLITE_CONFIG_MULTITHREAD)` done correctly

- Sequence must be: `sqlite3_shutdown` → `sqlite3_config(MULTITHREAD)` →
  `sqlite3_initialize`, executed at process start, **before any
  `Engine::open`** or any other path that opens a Connection.
- Validate by checking `sqlite3_threadsafe()` return value (expect 2).
- This is the no-rebuild path to the same effect as 7.2 — but
  ordering-sensitive.

### 7.4 `SQLITE_CONFIG_MEMSTATUS=0`

- Cheap. Removes some allocator stats locking. Will not be sufficient
  on its own but composes with 7.3 / 7.5.

### 7.5 Per-connection lookaside and page cache

- `SQLITE_CONFIG_LOOKASIDE` sized lookaside per connection, plus
  `SQLITE_CONFIG_PAGECACHE` carving out per-connection page cache, can
  sidestep the global mutex without rebuild.

### 7.6 Structural: collapse search to one prepared statement

- One prepared statement combining vec0 + FTS5 via UNION inside the same
  read transaction. Fewer prepares → less malloc churn → less pressure
  on whichever mutex turns out to be the constraint. Preserves the
  reader-tx snapshot contract.
- Worth doing regardless of mutex outcome (reduces sequential latency
  too), but riskier than 7.2/7.3 — keep it as the structural follow-up.

---

## 8. Open questions for the whitepaper

- Does `sqlite3_config` actually take effect when invoked from
  `register_sqlite_vec_extension` (lib.rs:1824)? Capture and assert the
  return code; if it is `SQLITE_MISUSE`, the configuration call is a
  no-op and the whitepaper should call this out as a foot-gun specific
  to bundled rusqlite.
- Is the `1.5 / AC020_THREADS` bound (concurrent <= sequential \* 1.5/8)
  the right contract, or should AC-020 specify the contention source it
  is gating against (allocator mutex / pcache mutex / etc.)? The current
  bound is a black-box ratio; a whitepaper-grade contract would tie it
  to a named mutex regime.
- For deployments using a host-system SQLite (non-bundled), `SQLITE_THREADSAFE`
  is fixed at the host-distro build flag and we lose the THREADSAFE=2
  knob entirely. The whitepaper should document this and either commit
  to bundled-only or describe degraded-mode behavior under THREADSAFE=1.

---

## 9. Cross-references

- Test plan: `dev/test-plan.md` (AC-017 / AC-018 / AC-020 sections).
- Phase 9 implementer handoff (recent): commit `b4a3261`, plus the
  `dev/notes/` directory.
- Engine: `src/rust/crates/fathomdb-engine/src/lib.rs`,
  `src/rust/crates/fathomdb-engine/src/lifecycle.rs`.
- Perf gate: `src/rust/crates/fathomdb-engine/tests/perf_gates.rs`.
- Cargo manifest:
  `src/rust/crates/fathomdb-engine/Cargo.toml` (line 13 — rusqlite/bundled).

---

## 10. Phase 9 Packet 4 appendix — AC-018 / AC-020 notes

Status snapshot for later synthesis. Branch/worktree context:
`0.6.0-rewrite`. `git rev-parse --short HEAD` at note time: `b4a3261`.

### Context

- Packet 4 was the Phase 9 performance/concurrency evidence packet.
- Main focus was binding honest runtime evidence for:
  - `AC-018`: real `Engine::drain(100 vectors)` timing.
  - `AC-020`: long-run sequential-vs-8-reader comparison under
    `AGENT_LONG=1`.
- Functional precondition for honest Packet 4 work: the vector search
  branch was made real in `src/rust/crates/fathomdb-engine/src/lib.rs`,
  with regression coverage in
  `src/rust/crates/fathomdb-engine/tests/projection_runtime.rs`.

### Harness / setup notes

- `AC-018` is measured in
  `src/rust/crates/fathomdb-engine/tests/perf_gates.rs` using a real
  `Engine::drain(100 vectors)` path, not a shape-only substitute.
- `AC-020` is measured in the same file using the documented mixed
  read workload and a sequential-vs-8-reader comparison gated behind
  `AGENT_LONG=1`.
- Key files for reconstruction:
  - `src/rust/crates/fathomdb-engine/src/lib.rs`
  - `src/rust/crates/fathomdb-engine/tests/perf_gates.rs`
  - `src/rust/crates/fathomdb-engine/tests/projection_runtime.rs`
  - `dev/test-plan.md`
  - `dev/progress/0.6.0.md`

### Retained changes

- Persistent projection-runtime SQLite connections.
- Batched projection commits.
- Query vector computed before reader checkout to shorten pooled-reader
  occupancy without changing snapshot derivation.

Observed effect:

- `AC-018` was originally about `4.9s` and later went green after the
  retained runtime changes.
- `AC-020` improved materially but remained red on the accepted bound.

### Reverted / unsuccessful experiments

- Single-statement vec0 join materialization:
  no useful gain on the real gate.
- Reader statement-cache / `prepare_cached` attempt:
  helped some per-call cost, not the concurrency ratio.
- Reader `READ_ONLY` / `NOMUTEX` open flags:
  no gate-changing improvement.
- Read-tx teardown by drop instead of explicit commit:
  effectively equivalent.
- Reader pool size above `8`:
  no improvement; not a pool-capacity bottleneck.
- Reader cache / `mmap_size` / temp-store style tuning:
  no retained improvement from the reader-side tuning pass.

### Measured results to preserve

- `AC-018`:
  - originally about `4.9s`
  - later green after persistent runtime connections and batched
    projection commits
- `AC-020` red measurements:
  - early honest harness:
    `sequential=469.132706ms`, `concurrent=283.893694ms`,
    `bound=87.962382ms`
  - after persistent runtime connections:
    `sequential=502.985489ms`, `concurrent=130.327471ms`,
    `bound=94.309777ms`
  - after batching kept runtime changes:
    `sequential=455.510692ms`, `concurrent=131.502285ms`,
    `bound=85.408255ms`
  - later retained-ish reader-hold-time improvement:
    `sequential=456.016904ms`, `concurrent=126.626795ms`,
    `bound=85.503168ms`
  - latest rerun:
    `sequential=473.091827ms`, `concurrent=134.882609ms`,
    `bound=88.70472ms`

### Diagnostics and later-work breadcrumbs

- Next diagnostic priority: flamegraph / `perf record` on sequential vs
  concurrent `AC-020` runs, then compare stacks.
- Primary hypothesis to validate: allocator / mutex contention inside
  SQLite rather than reader-pool hold time.
- Candidate later experiments if diagnostics support that hypothesis:
  - SQLite threading/runtime-mode experiments
  - allocator / page-cache configuration experiments
  - deeper structural statement consolidation that still preserves the
    single borrowed-reader snapshot contract

### Worktree breadcrumbs

- Relevant `git status --short` entries at note time included:
  - modified:
    `src/rust/crates/fathomdb-engine/src/lib.rs`,
    `dev/test-plan.md`, `dev/progress/0.6.0.md`
  - untracked:
    `src/rust/crates/fathomdb-engine/tests/perf_gates.rs`,
    `src/rust/crates/fathomdb-engine/tests/projection_runtime.rs`

---

## 11. Phase 9 Pack 5 narrative log (append-only)

Per STATUS.md update protocol step 6 and several phase prompts
(A.2, B.1, B.2, B.3, C.1, D.1), each KEEP decision in Pack 5
appends a paragraph here summarising hypothesis → measurement →
decision. §4/§5 stay structured (kept / reverted experiments);
§11 is the prose companion that A.2 and the final synthesis can
quote from.

Preamble (not an experiment): on 2026-05-03 the Phase 9 Pack 1-4
production work — vector runtime, projection terminal, FTS search
index, and the AC-020 perf gate — was found uncommitted in the
working tree at orchestrator resume time. It was committed
clerically at `1980bf6` after `agent-verify.sh` green at that tree.
Pack 5 baseline = `1980bf6`. See `dev/plan/runs/STATUS.md`
"Baseline drift note" and the §11 preamble in
`dev/plan/0.6.0-Phase-9-Pack-5-performance-diagnostics.md`.

_(no experiment narratives yet — A.0 spawn pending)_
