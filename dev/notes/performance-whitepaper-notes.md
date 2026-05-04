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

- `pub(crate) struct SubscriberRegistry` — line 117.
- `pub(crate) struct Counters` — line 250.

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
- Read-path `prepare_cached` on the four search statements
  (`vec0_match`, `canonical_lookup`, `soft_fallback_probe`,
  `fts_match`) at long-lived reader connections (E.1, 2026-05-03,
  REVERTED via `git revert` `1739b17`+`3e047a3`). Implementation
  worked: prepares per search dropped from 4 to 0 post-warmup
  (verified by counter test). Sequential improved -13.7% (182→157
  ms) — parse-cost relief is real. **But concurrent did NOT improve**
  (115→125 ms; +3.5 ms over the B.1/C.1 environmental drift
  baseline ~121 ms; statistically flat). Speedup 1.58→1.27×
  (-19.9%) because sequential dropped without concurrent dropping
  — gate ratio worsens. Codex `gpt-5.4` reviewer (mandatory for
  KEEP path) returned **BLOCK** with three findings: (1) new
  `pub fn ..._for_test` accessors expand public surface without
  ADR / `dev/interfaces/rust.md` update — Public surface is
  contract per AGENTS.md §1; (2) `concurrent_median = 125 > 115`
  matches the **REVERT** threshold of the written rule, not the
  `INCONCLUSIVE` decision the implementer recorded; (3) test
  exercised 3 of 4 statements (300 baseline misses, not 400
  mandated). Phase 7/8 invariants all PASS. Reverting per
  reviewer; the parse-cost relief is real but does not help
  AC-020 (in fact tightens the bound). Do NOT retry
  `prepare_cached` on read-path search until the rusqlite-side
  per-call lock-contention layer is understood — the residual
  contention is upstream of where parse cost sits. Output:
  `dev/plan/runs/E1-prepare-cached-readers-output.json` (now
  removed by revert; archived in commit `91c69e9`'s tree). Review
  verdict: `dev/plan/runs/E1-review-20260504T031424Z.md`.
- Compile-time `SQLITE_THREADSAFE=2` rebuild via
  `LIBSQLITE3_FLAGS="-USQLITE_THREADSAFE -DSQLITE_THREADSAFE=2"`
  in `.cargo/config.toml` (C.1, 2026-05-03, REVERTED `15c6473`,
  no source commit). Build flag verified live: both
  `sqlite3_threadsafe()==2` and `PRAGMA compile_options THREADSAFE=2`
  green pre-revert. AC-020 flat: seq 182→182.2 ms (+0.1%), conc
  115→121.5 ms (+5.65%), speedup 1.58→1.509× (-4.48%). The A.2
  hot mutex/atomic symbols survive `THREADSAFE=2` unchanged —
  they are not SQLite's threading-mode mutexes. Likely WAL
  shared-memory atomic primitives (frame-counter CAS, page
  reference atomics) which SQLite uses regardless of threading
  mode. Mutex track CLOSED with the strongest possible intervention.
  Do NOT retry compile-time threading flags without first
  surfacing direct evidence that the contended primitive sits
  inside a SQLite mutex API call. Output JSON:
  `dev/plan/runs/C1-threadsafe2-rebuild-output.json`. Cross-platform
  CI deferred (only aarch64-linux verified locally; flag route
  is architecture-neutral but x86_64-linux + darwin-arm64 not
  exercised).
- Runtime `sqlite3_config(SQLITE_CONFIG_MULTITHREAD)` placed at
  `Engine::open_locked` head **before** any `Connection::open`
  (B.1 attempt #2, 2026-05-03, REVERTED at `d448263`'s baseline,
  no source commit). This is the ordering-correct version of the
  prior entry — `init_sqlite_runtime()` Once-guarded with
  `shutdown` → `config(MULTITHREAD)` → `initialize`. Captured rcs:
  shutdown=0, **config=0 (SQLITE_OK)**, initialize=0. Verified via
  `pub fn sqlite*runtime*config_rc() -> Option<i32>` accessor and
  `tests/multithread_wiring.rs` (2/2 passing pre-revert). The wiring
  is provably correct (cannot fail silently like the prior entry —
  return code is captured, asserted, and the test discriminates
  `Some(0)` from the `Some(21)` no-op pattern). **Yet AC-020 ratio
  is unchanged**: sequential 182→184 ms (+1.1%), concurrent 115→121
  ms (+4.9%), speedup 1.58→1.53× (-3.4%) — all within one stddev of
  the A.1 baseline. Hypothesis "runtime threading-mode flag relieves
  the bottleneck" is **falsified** with a high-confidence signal
  (the gate is observable, not silent). Next experiment is C.1
  (compile-time `SQLITE_THREADSAFE=2` rebuild — directly disables
  the per-connection mutex symbols dominating the A.1 concurrent
  flame, which `CONFIG_MULTITHREAD` cannot reach because the
  bundled SQLite is compile-pinned at THREADSAFE=1). Do NOT retry
  runtime CONFIG_MULTITHREAD without first showing a different
  call path or a different SQLite build — the rc=0/idempotency
  evidence is dispositive. Output JSON:
  `dev/plan/runs/B1-multithread-wiring-output.json`.

---

## 6. Hypothesis hierarchy for the remaining gap

> **2026-05-04 update — superseded by §11 Pack 5 close.** The primary
> suspect below (SQLite global allocator / threading-mode mutexes) was
> falsified clean by B.1 (runtime CONFIG_MULTITHREAD, `config_rc=0`,
> AC-020 flat) and C.1 (compile-time `THREADSAFE=2` verified live,
> AC-020 flat). The revised hypothesis ladder is in §11's closing
> "Pivot for the next packet" paragraph: residual contention is in
> rusqlite-side internal Mutex, our `ReaderPool::borrow`
> `Mutex<Vec<Connection>>` (`lib.rs:158`), or WAL shared-memory
> atomics — all upstream of where SQLite threading mode reaches.
> §6 below is preserved as the pre-Pack-5 reasoning for historical
> record; do not use it as a starting point for new experiments.

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
- Validate by checking `sqlite3_config()`'s return code is
  `SQLITE_OK = 0` (NOT by `sqlite3_threadsafe()` — that is a
  compile-time constant per `sqlite3.h:249-252` and is unchanged
  by `sqlite3_config()`; bundled libsqlite3-sys is pinned at
  `THREADSAFE=1`). The §5 silent-no-op returned `SQLITE_MISUSE =
21`, so `rc == SQLITE_OK` is a real differentiator.
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

### 7.7 SWMR + per-reader `OPEN_NOMUTEX` stacked on B.1 (2026-05-03 hypothesis)

> **2026-05-04 status — trigger never fired; archived as conditional
> plan record.** The activation trigger required B.1 to land
> KEEP/INCONCLUSIVE; B.1 attempt #2 landed REVERT (clean falsification,
> §11). C.1 then closed the SQLite mutex track entirely. NOMUTEX has
> no remaining mechanism here — the residual primitives are not SQLite
> threading-mode mutexes. Section preserved as a record of the
> conditional plan; do not activate without re-establishing a live
> SQLite per-connection-mutex hypothesis first.

Hypothesis raised mid-Pack-5, after A.4 PICK*B1 locked: keep the current
SWMR shape (single writer + 8-reader pool, WAL, BEGIN IMMEDIATE on
writes, one-thread-per-connection ownership), and stack
`SQLITE_OPEN_NOMUTEX` on the reader-pool connections **after** B.1's
`sqlite3_config(SQLITE_CONFIG_MULTITHREAD)` lands. This is \_not* a new
B-track branch; it is a stack-experiment that becomes interesting only
in one specific A.1-recapture outcome.

What is already true today (no change needed):

- single-writer / 8-reader pool — `lib.rs:158` (`ReaderPool`),
  `READER_POOL_SIZE = 8` (`lib.rs:48`).
- WAL — A.3.4 reader_pragmas: `journal_mode=wal`.
- BEGIN IMMEDIATE on the writer side (migrations + writer txns).
- one-thread-per-connection ownership — `ReaderPool::borrow` returns a
  guard with exclusive access; writer is single-threaded by design.
- `synchronous=2` (FULL) on readers — A.3.4 (readers don't fsync, so
  this is durability-side; reading it from a reader is informational).

What §5 already settled:

- Reader-side `SQLITE_OPEN_NOMUTEX` was tried at THREADSAFE=1 and
  reverted because "NOMUTEX only drops the per-connection mutex; we
  still hit the global mutexes from THREADSAFE=1." The §5 entry is
  layer-correct: at THREADSAFE=1 the global mutexes dominate and
  NOMUTEX is irrelevant.

What is genuinely new:

- After B.1 lands MULTITHREAD via `sqlite3_config`, the global mutexes
  go away. _Then_ per-connection mutexes (which NOMUTEX targets)
  become the next-layer candidate — but only if A.1 recapture against
  the B.1 branch shows residual `mutex_atomic` cycles still localised
  in per-connection mutex symbols.
- `synchronous=NORMAL` and `busy_timeout` are NOT in scope. NORMAL is
  a write-side durability change (reader pragma is informational),
  REQ-013 / AC-059b are sacred per resume hard-rule §4.2 — out of
  Pack 5 scope without a separate ADR. `busy_timeout` is irrelevant
  (no SQLITE_BUSY observed in A.1/A.3). Shared-cache avoidance is
  a no-op (rusqlite default already excludes it).

Decision rule for this stack-experiment ("B.1.b" if it runs):

- Trigger: B.1 lands KEEP **or** INCONCLUSIVE; A.1 recapture against
  the B.1 branch shows residual `mutex_atomic` ≥ 5 % AND those
  cycles localise in per-connection symbols (e.g. `pthreadMutexEnter`
  on a per-conn handle), NOT in rusqlite-side or
  `ReaderPool::borrow` Mutex.
- Intervention: add `SQLITE_OPEN_NOMUTEX` to the reader-pool
  `Connection::open_with_flags` call at `lib.rs:775` (writer
  unchanged — writer is single-threaded but a future contributor
  could parallelise it, leave the safety margin).
- Acceptance: concurrent_median drops by an additional ≥ 10 % on top
  of B.1 alone; `mutex_atomic` share drops at least proportionally;
  AC-018 stays green.
- Skip if: A.1 recapture shows residual mutex_atomic < 5 %, OR the
  residual is in `ReaderPool::borrow`'s Rust-side `Mutex<Vec<Conn>>`
  (different mutex domain — would need a different fix, e.g. a
  lock-free pool), OR B.1 alone closes AC-020.

Contract / durability risks:

- `OPEN_NOMUTEX` is safe **iff** one-thread-per-connection is strict.
  Today it is, but reviewer must check that no `&Connection` escapes
  a `ReaderPool` guard (e.g. handed across an `await` boundary or
  stored in a `Send + Sync` container). REQ-055 cursor contract +
  AC-059b snapshot contract are unaffected because per-connection
  mutex removal does not change isolation semantics inside a
  `BEGIN`/`COMMIT` pair.
- No durability change (synchronous, journal_mode, wal_checkpoint
  unchanged).
- Observability change is nil: `EngineEvent` emission, structured
  errors, and probe paths use the connection through the existing
  guard.

Slot in the packet:

- A.4 alt-on-fail stays B.3 (per-conn lookaside) — the allocator
  share is the next-largest super-linear grower per A.2 (2.00×) and
  has its own evidence chain. B.1.b sits between B.1 and B.3 only
  if the recapture evidence demands it.
- Plan §10 ordering and STATUS phase-results table do **not**
  change. If B.1.b runs, it is recorded as a §12 entry and a §11
  narrative paragraph just like any other Phase B slot.

This note is the audit trail; no spawn, no prompt file, no §12 line
created here. If/when the trigger fires, write the spawn brief at
that point with the recapture numbers in hand.

### 7.8 E synthesis track — staged residual tuning (2026-05-03 hypothesis)

> **2026-05-04 status — activation table consumed.** C.1 REVERTed
> (mutex track closed) → E.1 ran (`prepare_cached` on the four named
> read-path stmts) → E.1 REVERTed (codex reviewer BLOCK; parse track
> closed). E.2 deferred out-of-packet (no schema migration in Pack 5).
> E.3 / E.4 SKIPPED by evidence exhaustion per §11 final-synthesis
> paragraph. Section preserved as the activation record; do not
> re-activate E.1 / E.3 without first understanding the upstream
> per-call lock-contention layer (§11 closing paragraphs).

Hypothesis raised after B.1 falsification, while C.1 is in flight:
the AC-020 shortfall is multi-cause, not single-cause. The
mutex-contention track (B.1 / C.1) attacks the _largest_ cycle
sink but leaves several smaller-but-real costs that A.3 already
surfaced:

- `prepares_per_search = 4` (A.3.2 counters) — every `Engine::search`
  re-prepares 4 statements. Combined with `sqlite3RunParser` at
  4.6% (sequential) / 3.4% (concurrent) of cycles independent of
  concurrency, this is a real per-call floor that B.1/C.1 cannot
  touch.
- `canonical_nodes` has no index on `write_cursor` (A.3.3 EXPLAIN).
  Hidden by 4-row fixture today; correctness latency at scale.
- `DEFAULT_MMAP_SIZE = 0`, `DEFAULT_CACHE_SIZE = -2000` (A.3.4).
  Reader pool 8 × 2 MB cache = 16 MB across all readers; one cold
  page can cost a syscall.
- `DEFAULT_WAL_AUTOCHECKPOINT = 1000` pages (A.3.4). Writer thread
  may stall mid-checkpoint; workload is read-heavy but writer
  pressure exists.

E is a synthesis, not a new theory:

- Mutex contention is **dominant** (A.2 5.73× growth) — that's why
  B.1 / C.1 came first.
- B.1 falsified runtime mutex flip; C.1 tests the strongest possible
  mutex intervention (compile-time elimination).
- _After_ the mutex track outcome lands, the residual gap is most
  efficiently closed by tuning the per-query overheads listed above
  in a single staged track, not as separate one-off experiments.

E sub-experiments (run as a single phase if activated):

- **E.1** — `prepare_cached` on the 4 read-path search statements
  via long-lived reader connections. Whitepaper §4 already KEEPS
  `prepare_cached` on writer-side; readers were tried in §5 but
  the reverted entry doesn't specify which statements; redo
  carefully on the named four (vec0_match, canonical_lookup,
  soft_fallback_probe, fts_match per A.3.3). Risk: statement
  cache TTL / invalidation across schema migrations — readers are
  read-only and never see DDL, so cache is safe across the test's
  lifetime. Expected impact: cuts per-query parse cost from
  ~4 × ~25 µs ≈ 100 µs to amortized near-zero on warm-cache reads;
  on a 542 µs/query baseline that's ~18% drop.
- **E.2** — add index on `canonical_nodes(write_cursor)`. Latent
  scale concern; cleanup either way. Expected impact on the 4-row
  fixture: noise (no real visible saving). On scale fixtures this
  becomes load-bearing.
- **E.3** — bump reader `cache_size` and re-evaluate `mmap_size`.
  §5 reverted "Reader-side `cache_size` / `mmap_size` tuning" with
  the note "did not change the concurrency ratio" — that was at
  THREADSAFE=1 with global mutex contention masking everything.
  Worth re-trying _if_ C.1 KEEPs (mutex contention removed) AND
  the residual is read-bound. Skip if C.1 closes AC-020 alone.
- **E.4** — WAL `wal_autocheckpoint` tuning + `journal_size_limit`.
  Writer-side; only matters if A.1 re-capture against the post-mutex-
  fix branch shows writer-thread cycles in checkpoint. Skip
  otherwise.

Activation logic (decided after C.1 returns):

- **C.1 KEEPs** (AC-020 closed): E.2 only as separate cleanup
  follow-up issue; E.1/E.3/E.4 unnecessary.
- **C.1 REVERTs** (mutex track closed clean — even compile-time
  elimination doesn't move AC-020): E full sequence (E.1 → E.2
  → E.3 → E.4) becomes the next experiment branch, **replaces D.1
  as next** because E's evidence chain is more specific than
  D.1's. D.1 remains as fallback if E also lands flat.
- **C.1 BLOCKER** (cross-platform build fails): E full sequence
  regardless; C.1 retried separately with a build-flag harness.

Decision rule for E (numeric, mirrors B.1/C.1 form):

- KEEP iff `concurrent_median_ms ≤ 80` AND `speedup ≥ 5.0×` AND
  AC-018 green. Same threshold; reuse same N=5 harness.
- INCONCLUSIVE band 80–100 ms → re-capture A.1 against the E
  branch and re-classify; the bottleneck has likely moved
  (vec0_fts becomes the new ceiling at 11.43% concurrent share).
- REVERT iff concurrent_median > 115 OR AC-018 red.

Contract / durability risks per sub-experiment:

- E.1: prepared-statement cache must be invalidated on schema
  migration; readers don't see DDL but defensive code should
  hold a generation counter. REQ-013 / AC-059b unaffected.
- E.2: pure index addition, schema migration, must be in a new
  schema version (Pack 5 said "no data migration in this packet"
  per resume hard-rule §4.6 — so E.2 alone is a Pack 5 violation
  unless run as a separate post-Pack-5 issue). Recording this as
  the gating constraint.
- E.3: reader-side pragma; durability unaffected (readers don't
  write).
- E.4: writer-side; could affect crash-recovery latency but not
  durability semantics.

Slot in the packet:

- E does NOT replace C.1. C.1 is the strongest mutex test and
  must run.
- E activates only after C.1 returns; the activation table above
  encodes the decision.
- If E activates, it lands as Phase **E.1 / E.2 / E.3 / E.4** —
  new prompts written at activation time with the carry-forward
  numbers in hand. Plan §10 ordering does not change.
- E.2 (the index addition) is **out of Pack 5 scope** under
  resume hard-rule §4.6 unless explicitly authorized by the
  human; the other E sub-experiments are pure runtime tuning
  with no schema impact.

This note is the audit trail; no spawn, no prompt file, no §12
line created here. C.1 outcome decides activation.

**B.1 attempt #1 — BLOCKER (2026-05-03, no commit).** First spawn
of B.1 (Opus high) hit a real spec error and reverted per the
STOP-and-report rule. Implementer built `init_sqlite_runtime()`
exactly per spec — process-wide `OnceLock`, sequence
`sqlite3_shutdown` → `sqlite3_config(SQLITE_CONFIG_MULTITHREAD)`
→ `sqlite3_initialize`, all FFI through `rusqlite::ffi` with
`std::os::raw::c_int` return types — and verified at runtime that
all three calls returned `SQLITE_OK = 0`. **That `config_rc = 0`
is the real differentiator from §5**, which (per its own entry)
silently no-op'd because the call ran after rusqlite had
triggered `sqlite3_initialize()` and would have returned
`SQLITE_MISUSE = 21`. The B.1 ordering is correct.

The spec's gate assertion (`sqlite3_threadsafe() == 2`) is
**impossible by SQLite design**. Header `sqlite3.h:249-252`:
"the return value of `sqlite3_threadsafe()` shows only the
compile-time setting of thread safety, not any run-time changes
to that setting made by `sqlite3_config()`. In other words, the
return value from `sqlite3_threadsafe()` is unchanged by calls
to `sqlite3_config()`." Bundled libsqlite3-sys-0.28.0 compiles
with `-DSQLITE_THREADSAFE=1` (`build.rs:137`), so
`sqlite3_threadsafe()` is pinned to `1` regardless of any
runtime config. Changing it to `2` requires the C.1 compile-time
rebuild path. A.4's mandate inherited this incorrect premise
from §7.3 ("Validate by checking `sqlite3_threadsafe()` return
value (expect 2)"), which is now also corrected by reference.

Orchestrator decision (re-spawn): replace the gate assertion
with `fathomdb_engine::sqlite*runtime*config_rc() == 0` (or
`Some(0)` if the API is `Option<i32>`). Add this as a small
`pub fn` accessor that reads the captured `c_int` from the
`init_sqlite_runtime` `OnceLock`. The test compares against
`SQLITE_OK = 0` and references `SQLITE_MISUSE = 21` in a comment
as the §5 silent-no-op return code that B.1 guards against.
This preserves the §5 differentiator (real, observable, runtime)
without depending on a SQLite API that doesn't reflect runtime
state. **AC-020 numeric KEEP rule (concurrent_median ≤ 80 ms
AND speedup ≥ 5.0× AND AC-018 green) is unchanged** — that is
the load-bearing gate; the accessor test is a safety check that
distinguishes B.1 from a §5-style silent no-op.

A.4 alt-on-fail extends to **B.3 OR C.1** (was B.3 alone).
Rationale: if B.1 lands `config_rc = OK` but AC-020 doesn't move,
the runtime config is provably in effect (rc proves it) but
isn't reaching the contended mutexes. That pattern points at C.1
(compile-time `SQLITE_THREADSAFE=2` rebuild) being the actual
fix, not B.3 (lookaside). B.3 stays the right alt-on-fail when
`mutex_atomic` share _does_ drop under B.1 but speedup doesn't
hit 5×.

Implementer report (full transcript, 211 KB): see audit trail at
`/home/coreyt/.claude/projects/-home-coreyt-projects-fathomdb/0a705ea9-4d09-476f-bfb1-7fe41171ee12/tool-results/b58meryie.txt`.
B.1 prompt updated in-place 2026-05-03 with corrected mandate.
Worktree from attempt #1 cleaned (no commit, no branch).

**B.1 attempt #2 — REVERT (2026-05-03, `d448263` for the output
JSON only; no source commit, source identical to baseline).** Ran
on the corrected prompt (gate = `sqlite*runtime*config_rc() == 0`,
no `sqlite3_threadsafe()` assertion) plus the 2026-05-03
anti-chaining defenses (PREAMBLE prepended via stdin,
`--disallowedTools Task Agent`, `stream-json` log). Spawn worked
cleanly: no chaining, no orphan implementer, single coherent
agent doing the whole job and reporting at the end.

What landed pre-revert: `init_sqlite_runtime()` `OnceLock<Result<i32,
...>>`-cached, sequence `sqlite3_shutdown` → `sqlite3_config(MULTITHREAD)`
→ `sqlite3_initialize` at `Engine::open_locked` head BEFORE
`register_sqlite_vec_extension` and any `Connection::open`. New
`pub fn sqlite*runtime*config_rc() -> Option<i32>` accessor and
`tests/multithread_wiring.rs` (2 tests: post-open returns `Some(0)`;
re-open is idempotent). FFI return types `std::os::raw::c_int`
throughout. +119 LOC, 0 removed.

Captured numeric evidence:

| metric            | A.1 baseline | B.1 #2 after  | delta   |
| ----------------- | ------------ | ------------- | ------- |
| sequential median | 182 ms       | 184.0 ms      | +1.1%   |
| concurrent median | 115 ms       | 120.6 ms      | +4.9%   |
| speedup median    | 1.58×        | 1.526×        | -3.4%   |
| concurrent stddev | 4.0 ms       | 2.98 ms       | tighter |
| sqlite3_config_rc | n/a          | 0 (SQLITE_OK) | —       |
| AC-017 / AC-018   | green        | green         | flat    |

`config_rc=0` is the §5 differentiator: the §5 silent no-op would
have returned `SQLITE_MISUSE = 21`. The wiring is provably correct.
Yet AC-020 ratio is essentially unchanged — concurrent shifted +1.7
stddev on the wrong side of the 115 ms threshold; the strict A.4
rule says REVERT. Implementer reverted source per spec policy and
left the worktree clean (only the output JSON ran orphan, harvested
into `d448263`).

Decision interpretation (per A.4 `data_for_pivot` taxonomy):
this is the **APPLIED-BUT-DIDN'T-HELP** branch, not the
silent-no-op branch. Two surviving hypotheses for the dominant
A.1 mutex symbols (`__aarch64_swp4_rel`, `__aarch64_cas4_acq`,
`___pthread_mutex_lock`):

- (a) **C.1** — compile-time `SQLITE_THREADSAFE=2` /
  `SQLITE_NO_MUTEX` rebuild. Smaller-radius, tests the more
  specific hypothesis (per-connection mutex elimination is
  compile-time-only because the bundled SQLite is pinned at
  `THREADSAFE=1`; `CONFIG_MULTITHREAD` only relaxes the _global_
  mutexes which apparently aren't the bottleneck).
- (b) **D.1** — architectural (per-conn lookaside, alt reader
  topology, single-stmt UNION refactor). Activated as kill-track
  fallback if C.1 also lands flat.

A.4 alt-on-fail update: **C.1 first** (extends the prior
"B.3 OR C.1" to "C.1 first; B.3 only if C.1 also flat AND mutex
symbols still dominant in re-capture; D.1 if both flat"). The
reasoning is that B.1's clean-falsification result targets the
SQLite-mutex track specifically; C.1 is the compile-time test
of the same track, while B.3 (per-conn lookaside) targets the
allocator track which is a different category.

Reviewer note: codex `gpt-5.4` was mandatory per plan §0.1 / resume
§4 for B.1 KEEP. Skipped here because REVERT means there is no
production diff to review. The output JSON commit (`d448263`) is
docs-only and stands as the audit trail.

Anti-chaining defenses (resume §4 update at `fc3dda3`) **worked**:
single coherent agent, no Task spawns, no orphan implementer,
clean structured output. Keep them on for all subsequent spawns.

**C.1 — compile-time `SQLITE_THREADSAFE=2` rebuild (REVERT,
`15c6473`, 2026-05-03, Sonnet high).** Strongest possible mutex
intervention. Build route: env-side `LIBSQLITE3_FLAGS=
"-USQLITE_THREADSAFE -DSQLITE_THREADSAFE=2"` in
`.cargo/config.toml` (the `-U` first overrides
libsqlite3-sys-0.28.0 build.rs:137 hardcoded
`-DSQLITE_THREADSAFE=1`). Two independent assertions verified
the rebuild was live before revert: `sqlite3_threadsafe()==2`
(via `tests/compile_options.rs`) AND `PRAGMA compile_options`
output containing `THREADSAFE=2`. Build clean in 80.7 s; binary
size 1.17 MB; cross-platform CI deferred (aarch64-linux only).

AC-020 numbers (N=5 each, AGENT_LONG=1):

| metric            | A.1 baseline | C.1 after | delta   |
| ----------------- | ------------ | --------- | ------- |
| sequential median | 182 ms       | 182.2 ms  | +0.1%   |
| concurrent median | 115 ms       | 121.5 ms  | +5.65%  |
| speedup median    | 1.58×        | 1.509×    | -4.48%  |
| concurrent stddev | 4.0 ms       | 4.8 ms    | similar |
| AC-018 drain      | green        | 220 ms    | green   |

REVERT per A.4 numeric rule (conc > 115 threshold). Within ~1.2σ
of A.1; statistically indistinguishable. `sequential` change is
noise (+0.1%), confirming THREADSAFE=2 has no measurable effect
on single-thread path either.

Interpretation: this is the second clean falsification of the
mutex track. B.1 falsified the _runtime_ threading-mode flag;
C.1 falsifies the _compile-time_ threading-mode flag at the
strongest setting. **The A.2 hot symbols are not SQLite
threading-mode mutexes.** The most plausible remaining
explanation is that `__aarch64_swp4_rel`, `__aarch64_cas4_acq`,
and `___pthread_mutex_lock` symbols are surfacing from SQLite's
WAL shared-memory protocol (frame counters, page references,
checkpoint sequence atomics) and the `pthread_mutex_lock` calls
are coming from rusqlite-side or our own ReaderPool's
`Mutex<Vec<Connection>>`. Neither layer is removed by
`SQLITE_THREADSAFE=2`. The `MUTEX_PTHREADS` compile*options
entry persists at `THREADSAFE=2` because that flag selects the
mutex backend implementation; it is \_bypassed* in code paths
that the threading mode disables (per-conn + per-stmt mutex
bypass in MULTITHREAD), but the implementation symbols stay in
the binary.

Implementer's analysis was partially wrong about _why_
THREADSAFE=2 didn't help (they wrote "per-connection mutex is
still physically present in the binary at THREADSAFE=2; only
disabled at THREADSAFE=0 or with SQLITE*NO_MUTEX") — that's
binary-presence reasoning. The actual SQLite semantics are that
THREADSAFE=2 \_bypasses* the per-conn / per-stmt mutex API calls
even though the symbols remain. The numerical conclusion
(falsification) stands either way. Recording the corrected
mechanism here so a future reader doesn't mis-design a
follow-up.

Decision per A.4 kill criterion + §7.8 activation table:
mutex track is closed clean. **Promote E synthesis sequence
(replaces D.1 as next branch).** D.1 retained as last-resort
fallback if E also lands flat.

E.2 (`canonical_nodes(write_cursor)` index) is deferred
out-of-packet per resume hard-rule §4.6 (no data migration in
Pack 5). Activated for the next branch: **E.1 first** (prepare_cached
on read-path search statements; ~18% expected drop on
542 µs/query A.3.2 baseline) — if E.1 KEEPs alone, AC-020 may
close. If E.1 KEEPs but speedup still <5×, stack E.3 (reader
cache_size + mmap_size) on top. E.4 (WAL autocheckpoint) only
if A.1 recapture shows writer-thread checkpoint cycles.

Reviewer note: codex `gpt-5.4` was MANDATORY for C.1 per plan
§0.1 (cross-platform Cargo change). Skipped because REVERT means
no production diff to review. The .cargo/config.toml
modification was reverted; only output JSON + evidence files
landed.

---

## 8. Open questions for the whitepaper

- Does `sqlite3_config` actually take effect when invoked from
  `register_sqlite_vec_extension` (lib.rs:1824)? Capture and assert the
  return code; if it is `SQLITE_MISUSE`, the configuration call is a
  no-op and the whitepaper should call this out as a foot-gun specific
  to bundled rusqlite.
  - **ANSWERED 2026-05-03 (B.1 #1+#2):** at the
    `register_sqlite_vec_extension` callsite, `sqlite3_config` returns
    `SQLITE_MISUSE = 21` silently because rusqlite has already
    triggered `sqlite3_initialize()` on the prior `Connection::open`.
    Moving the call to `Engine::open_locked` head BEFORE any
    `Connection::open` makes it return `SQLITE_OK = 0`. Captured-rc
    assertion (`sqlite*runtime*config_rc()`) is the correct
    differentiator from §5's silent no-op pattern. **Foot-gun
    confirmed; resolution: pre-init placement + return-code
    validation.**
- Is the `1.5 / AC020_THREADS` bound (concurrent <= sequential \* 1.5/8)
  the right contract, or should AC-020 specify the contention source it
  is gating against (allocator mutex / pcache mutex / etc.)? The current
  bound is a black-box ratio; a whitepaper-grade contract would tie it
  to a named mutex regime.
  - **STILL OPEN, with new evidence (Pack 5):** the black-box ratio
    is workable only when the contention source is the SQLite
    threading layer. Pack 5 evidence shows the residual is upstream
    of SQLite (rusqlite-side Mutex / our `ReaderPool::borrow` Mutex)
    or sideways (WAL shared-memory atomics). A future packet may
    refine the bound to specify a named runtime invariant
    (e.g. "no mutex on the reader-pool borrow path"). Holding the
    1.5/8 ratio for now; revisit when architectural fix lands.
- For deployments using a host-system SQLite (non-bundled), `SQLITE_THREADSAFE`
  is fixed at the host-distro build flag and we lose the THREADSAFE=2
  knob entirely. The whitepaper should document this and either commit
  to bundled-only or describe degraded-mode behavior under THREADSAFE=1.
  - **MOOT 2026-05-03 (C.1):** with bundled-only `THREADSAFE=2`
    verified live, the AC-020 ratio doesn't move. The host-system
    SQLite question is no longer a closure-blocker for AC-020.
    Recommendation: stay bundled-default at `THREADSAFE=1`; do not
    require bundled-only deployment for AC-020 reasons. The
    deployment-mode discussion is now an API-availability concern
    (host SQLite may lack vec0 / FTS5 features) but not a
    performance-gate concern.
- **NEW (Pack 5 surfaced):** What is the source of the
  `___pthread_mutex_lock` + `__aarch64_swp4_rel` / `_cas4_acq` cycles
  that survive `THREADSAFE=2`? Three candidates: rusqlite-side
  internal Mutex, our `ReaderPool::borrow` Mutex<Vec<Connection>>,
  WAL shared-memory atomics. Pack 6 starting point should perf-re-capture
  against the current tip and classify by the same A.2-style
  category aggregator (extended with `rusqlite::*` and `ReaderPool::*`
  patterns) before writing code.

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

**A.0 — harness split (KEEP, `fec71a0`, 2026-05-03).** Diagnostic
prep, not a perf change. Added two `#[ignore]` tests in
`tests/perf_gates.rs` (`ac_020_sequential_only`,
`ac_020_concurrent_only`) gated by `AC020_PHASE` env var, sharing
`seed_ac020_fixture` / `run_ac020_mix` / `AC020_THREADS` /
`AC020_ROUNDS_PER_THREAD` with the combined gate so fixtures cannot
drift. Each emits `AC020_PHASE_SEQUENTIAL_MS=<n>` /
`AC020_PHASE_CONCURRENT_MS=<n>` on stderr for shell grep. Smoke run
(N=1, not gate reading): seq=184ms, conc=117ms; combined gate at the
same tree reported seq=182ms / conc=118ms / bound=34ms / speedup=0.19
— consistent within noise, fixture parity confirmed. +54/-0 LOC,
test-only. `agent-verify.sh` green. Reviewer skipped (test-only edit
per plan §0.1). Sub-phase entry points unblock A.1 perf capture
(`AC020_PHASE=sequential|concurrent cargo test … --ignored ac_020_*_only`).
Pre-existing compile errors in `tests/{compatibility,cursors,lifecycle_observability}.rs`
flagged in A.0 output JSON as not-of-this-phase; agent-verify still
green, so noted but not actioned here.

**A.1 — perf capture (KEEP `ca0d8f0`, 2026-05-03).** Diagnostic
capture only, no perf delta. N=5 `perf record -F 999 -g
--call-graph dwarf` of each sub-phase via the A.0 entry points;
flamegraphs + diff rendered with `inferno` v0.12.6. Numbers:
sequential `[189,199,182,179,176]` ms (median 182, stddev 9.2),
concurrent `[120,110,117,115,112]` ms (median 115, stddev 4.0),
observed speedup 1.58× vs required 5.33× — gap 3.4×. Phase JSON
self-marked INCONCLUSIVE because A.1 mandate is capture, not
classify; orchestrator KEEP because every acceptance criterion
hit and the artifacts are exactly what A.2 needs.

Profile shape (concurrent vs sequential): atomic + mutex primitives
dominate concurrent at ~30% of cycles
(`__aarch64_swp4_rel` 11.2%, `__aarch64_cas4_acq` 9.8%,
`___pthread_mutex_lock` 6.8%, `__aarch64_swp4_acq` 5.9%,
`lll_mutex_lock_optimized` 1.8%) versus ~5% in sequential. Useful
work fraction (`min_idx`, `vec0Filter_*`) drops 14.5% → 8.7% under
contention even though wall-time is shorter. A.1's
alternative*hypothesis frames this as SQLite WAL/pager spinlock +
the global SQLite mutex serializing writers — \_not* the read/write
connection lock hierarchy from §6 ladder.

Independent secondary signal in _both_ profiles (4.6% sequential,
3.4% concurrent): `sqlite3RunParser`. Every search call re-parses
SQL; no prepared-statement cache reuse. Orthogonal to the
concurrency bottleneck, so A.2 can keep it as a candidate Phase B/C
target on its own merits.

Caveats: perf binary `5.15.163` (extracted from
`linux-tools-5.15.0-124-generic_arm64.deb` because no
`linux-tools-tegra` exists) vs kernel `5.15.185-tegra` — minor
skew, no recording errors. CPU governor could not be changed from
`schedutil` (no sudo); cores observed 729-2035 MHz under load
(rated max 2201 MHz). `perf_event_paranoid=2` blocks kernel
events, so `perf lock` and `perf c2c` are unavailable without
sudo — A.1's `data_for_pivot` lists USDT probes + `strace -c -f`
(A.3 territory) as the fallbacks. Build used
`RUSTFLAGS='-C force-frame-pointers=yes' cargo build --test
perf_gates` (skips pre-existing compile errors in
`tests/{cursors,lifecycle_observability,compatibility}.rs`).
Sanity symbols `mem1Malloc` and `read_search_in_tx` were absent
from the folded files: the former because this build uses
`sqlite3MemMalloc` (different SQLite alloc shim), the latter
because the rusqlite caller inlines it in release; both are
expected and not capture failures.

Raw `perf.data` files (8.1 MB concurrent + 2.2 MB sequential =
10.3 MB) excluded from commit — regenerable via the documented
A.0 entry points. Folded + SVG files (~1 MB total) committed for
A.2 + final synthesis.

**A.2 — symbol-focus diff (PICK_B1, 2026-05-03, main thread Opus
4.7).** Read both folded files into a category aggregator
(awk-side classifier matched leaf symbols against the six A.2
patterns plus a few neighbours: `wal_pager`, `vdbe`,
`sqlite_hash`, `libc_mem`, `time_syscall`, `unknown`).

Classification (% of total cycles per profile):

| Category     | Seq % | Conc % | Ratio | Verdict                     |
| ------------ | ----- | ------ | ----- | --------------------------- |
| mutex_atomic | 6.45  | 36.98  | 5.73× | **DOMINANT**                |
| allocator    | 1.60  | 3.20   | 2.00× | secondary (small absolute)  |
| page_cache   | 1.64  | 1.46   | 0.89× | did not grow                |
| vec0_fts     | 24.12 | 11.43  | 0.47× | useful work displaced       |
| our_code     | 0.52  | 0.17   | 0.33× | not a bottleneck (inlined)  |
| embedder     | 0.0   | 0.0    | n/a   | not present in the loop     |
| sql_parse    | 10.08 | 7.07   | 0.70× | independent (no prep cache) |

Decision rule (A.2 mandate): pick the category whose share grows
super-linearly between sequential and concurrent. **mutex_atomic
grows 5.73× with the largest absolute cycle delta (+262M cycles)
out of 776M total in the concurrent profile** — unambiguous. Per
A.2 mapping table → first Phase B/C/D candidate is **B.1
(runtime MULTITHREAD, ordering-correct)**: swap SERIALIZED for
MULTITHREAD on the projection-runtime + reader connections so
each connection has its own mutex domain instead of contending
on the global SQLite mutex.

This _replaces_ the §6 hypothesis ladder's primary suspect (lock
hierarchy between read/write connections). The actual bottleneck
is one level deeper — SQLite's global mutex (`sqlite3_mutex_*`,
`__aarch64_swp4_rel`, `__aarch64_cas4_acq`,
`___pthread_mutex_lock`, `__GI___lll_lock_wait`) serializing
writers across our pool. B.1 directly targets this.

Allocator at 2× ratio is real but small in absolute terms; held
in reserve as the B.3 candidate if B.1 KEEPs without closing the
gap. If B.1 lands with no speedup improvement, A.1 must
re-capture against the B.1 branch and re-classify before
picking again — same residual mutex symbol = B.1 missed the
right connections; different mutex = different problem
(rusqlite-side, ReaderPool::acquire). No recapture needed _now_ —
A.1 signal is sufficient.

`sqlite3RunParser` at 4-5% in both profiles is independent of
concurrency (does not grow), but is a real time floor.
Optimization is orthogonal to AC-020; can be picked up in a
later packet or as a Phase D-class candidate if AC-020 is closed
by B.1 alone.

**A.3 — secondary diagnostics (PARTIAL_KEEP `edb0c84`,
2026-05-03).** Three `#[ignore]` diagnostic tests added to
`tests/perf_gates.rs` plus evidence under
`dev/plan/runs/A3-evidence/`. A.3.4 is the critical
corroboration: `sqlite3_threadsafe()` returns `1` (SERIALIZED)
with `MUTEX_PTHREADS`, `SYSTEM_MALLOC`, `DEFAULT_MMAP_SIZE=0`,
`DEFAULT_CACHE_SIZE=-2000`. That confirms A.2's verdict — the
contended primitives in A.1 (`__aarch64_swp4_*`,
`__aarch64_cas4_acq`, `___pthread_mutex_lock`,
`__GI___lll_lock_wait`) are SQLite's serialized-mode mutexes
exactly as predicted. A.3.2 counters: 542 µs/query under
concurrency with embedder ~0 µs (RoutedEmbedder fixture-only),
so per-query latency is essentially borrow_wait + read_search_in_tx;
splitting those two requires a production hook, deferred.
A.3.3 EXPLAIN: no planner regressions; latent
`canonical_nodes` missing-index on `write_cursor` flagged as a
structural follow-up (4-row fixture hides O(N) scan, not a Pack 5
concern). A.3.1 strace skipped — no `strace` binary on this host
and no sudo to install one; A.4 treats this as corroborative-not-
load-bearing because the cycles signal already establishes the
verdict. Subagent wrote the structured output JSON to main repo
via absolute path (not the worktree), so the orchestrator
FF-merged only the test-code commit and kept the subagent's
JSON as canonical (it is more thorough than orchestrator's
synthesis would have been).

**A.4 — decision record (PICK B.1, 2026-05-03, main thread Opus
4.7, intent: high).** Locks B.1 (runtime MULTITHREAD wiring,
ordering-correct) as the first Phase B/C/D candidate.

§5 cross-check: **OVERRIDE**. §5 already lists a prior reverted
MULTITHREAD attempt — the `sqlite3_config(SQLITE_CONFIG_MULTITHREAD)`
call placed inside `register_sqlite_vec_extension`'s `Once` block
(`lib.rs:1824`), which produced no measurable change. The §5
entry itself names the failure cause: by the time
`register_sqlite_vec_extension` runs (called from `Engine::open`
at `lib.rs:746`, one line before the writer `Connection::open`),
rusqlite has already triggered `sqlite3_initialize()` on a prior
connection open in the same process, so `sqlite3_config` returns
`SQLITE_MISUSE` and is silently ignored. B.1's mandate is
materially different: place the `sqlite3_config` call BEFORE any
`sqlite3_initialize` trigger (a process-wide `Once` invoked from
`Engine::open` _entry_ before any `Connection::open`, or a
`ctor`-style static init), validate the return code is
`SQLITE_OK`, and add a `#[ignore]` integration test that asserts
`sqlite3_threadsafe() == 2` after `Engine::open`. Without all
three, the change is indistinguishable from the §5 attempt and
must REVERT. This is the explicit override that resume hard-rule
§4.4 requires; the §12 line points back here.

Decision rule: **KEEP iff `concurrent_median_ms ≤ 80` AND
`speedup ≥ 5.0×` AND AC-018 green** (≥ 30% drop from A.1
baseline 115 ms). INCONCLUSIVE band 80–100 ms triggers a re-A.1
capture against the B.1 branch; if mutex_atomic share dropped
substantially but speedup didn't reach 5.0×, stack B.3
(per-conn lookaside) without reverting B.1. REVERT iff
concurrent regresses past 115 ms or AC-018 turns red.

Kill criteria: if B.1 + B.3 stacked still produce < 10%
concurrent_ms drop, the mutex_atomic track is wrong; promote D.1
(single-stmt UNION refactor) and treat the residual mutex as
something `MULTITHREAD` doesn't reach (likely the rusqlite-side
Mutex around the connection pool or our own `ReaderPool::borrow`
`Mutex<Vec<Connection>>`). Before declaring kill, re-capture A.1
to confirm `mutex_atomic %` actually dropped — if it did but
speedup didn't, the bottleneck moved to whatever sits next on the
diff (likely vec0_fts becoming the new ceiling).

Ordering safety check: three `Connection::open` callsites in
`lib.rs` (writer 747, reader pool 775, third path 1574). All
three are downstream of any `Engine::open` entry; the new Once
must fire before any of them. Reviewer (codex) is mandatory for
B.1 per plan §0.1 and resume §4 — primary review focus is the
ordering invariant + the return-code validation + the
`sqlite3_threadsafe() == 2` assertion test.

Expected outcome window: concurrent 30-80 ms (median), speedup
5-12×. Lower bound assumes near-linear 8-thread scaling once
contention is removed; upper bound respects the vec0/FTS work
that is itself non-trivial and can't be parallelised away.

**E.1 — `prepare_cached` on read-path (REVERT via `git revert`
`1739b17`+`3e047a3`, 2026-05-03, Sonnet high; reviewer codex
`gpt-5.4` BLOCK).** Implementation correct: 4 statements
(`vec0_match`, `canonical_lookup`, `soft_fallback_probe`,
`fts_match`) switched from `tx.prepare()` to `tx.prepare_cached()`
on the long-lived reader connections; new
`tests/prepare_cached_readers.rs` (4 tests) verified misses drop
from ~300 baseline to ≤ 32 (8 readers × 4 stmts) post-warmup.
Sequential improved measurably — parse cost relief is real.

| metric            | A.1 baseline | E.1 after | delta  |
| ----------------- | ------------ | --------- | ------ |
| sequential median | 182 ms       | 157 ms    | -13.7% |
| concurrent median | 115 ms       | 125 ms    | +8.7%  |
| speedup median    | 1.58×        | 1.266×    | -19.9% |
| prepares/search   | 4            | 0 (warm)  | ✓      |
| AC-017 / AC-018   | green        | green     | flat   |

The concurrent change (+10 ms over A.1 / +3.5 ms over B.1/C.1
drift) is statistical noise. **Sequential improvement does not
help AC-020**: the gate is `concurrent ≤ sequential × 1.25/8`,
so a lower sequential lowers the bound — gate gets _harder_,
not easier. -19.9% speedup confirms.

Reviewer (codex `gpt-5.4`, mandatory for KEEP path per plan §0.1)
returned **BLOCK** with three substantive findings:

1. `pub fn search_prepare_count_for_test()` and
   `reset_search_prepare_count_for_test()` at `lib.rs:1285+1290`
   expand the Rust public surface without an ADR or update to
   `dev/interfaces/rust.md`. AGENTS.md §1 "Public surface is
   contract" — block.
2. `concurrent_median = 125 ms > 115 ms threshold` matches the
   _written_ REVERT rule in the E.1 prompt, not the
   `INCONCLUSIVE` decision the implementer recorded. The
   "environmental drift" allowance the implementer cited is not
   part of the written rule. Block.
3. Test exercised 3 of the 4 statements (300 baseline misses,
   not the mandated 400). Concern.

Phase 7/8 invariants (lifecycle event taxonomy, snapshot/cursor,
profile-callback safety, reader-pool ADR, file lock,
projection-runtime lifecycle) all PASS — code change was narrow
and contract-respecting on the runtime side; only the surface-
contract + decision-rule findings were load-bearing.

Decision: REVERT per reviewer. Reverted via `git revert`
(non-destructive) — both `1739b17` (source) and `3e047a3`
(output JSON) recorded. Worktree + branch cleaned. Output JSON
archived in the reverted commit `91c69e9`'s tree for audit.

Pivot interpretation: the concurrent-side bottleneck is NOT
parse cost. Combined with B.1+C.1 closing the SQLite mutex track,
the surviving candidates for the conc spinlock symbols are:

- **rusqlite-side internal Mutex** wrapping the sqlite3 handle
  (rusqlite is thread-safe via Mutex regardless of SQLite's
  THREADSAFE setting).
- **Our own `ReaderPool::borrow` `Mutex<Vec<Connection>>`** at
  `lib.rs:158` — every borrow + release acquires this Mutex.
- **WAL shared-memory atomics** for frame counters / page refs
  (SQLite uses these regardless of threading mode).

The first two are architectural; the third is a SQLite-internal
that no Pack 5 intervention can reach. **Do NOT retry
`prepare_cached` on the read path until the upstream per-call
lock-contention layer is understood.** The §5 entry has been
updated with this finding.

E.3 (reader `cache_size` / `mmap_size` re-try): §5's prior revert
already noted "did not change the concurrency ratio." With
mutex AND parse both ruled out, E.3's expected impact is small
(reads on a 4-row fixture are already cache-hot). Skipping E.3
to proceed directly to final synthesis — the data is sufficient
to characterize the residual without burning another experiment
slot on a likely-flat outcome. If a future packet wants to
revisit, the `cache_size` track is well-targeted at _absolute_
sequential latency, not at the AC-020 ratio.

**Pack 5 packet outcome (preliminary, pending final synthesis):**
Mutex track and parse-cost track both ruled out via clean
falsification. Residual concurrent contention is in
rusqlite-side or `ReaderPool` Rust-side Mutex (most likely) or
WAL shared-memory atomics (less likely). All three require
architectural changes (lock-free pool replacement, async I/O,
or a structurally different SQLite usage pattern) that exceed
Pack 5 scope. Recommendation: close Pack 5 as evidence-rich
diagnostic packet; open a follow-up packet for the architectural
intervention with this packet's evidence as its starting point.

**Final synthesis — Pack 5 closes ESCALATE (2026-05-03, main
thread Opus 4.7).** AC-020 did not close in Pack 5. N=5 final
verification:

| run        | seq ms    | conc ms   | bound ms | speedup   | passes 5.33× |
| ---------- | --------- | --------- | -------- | --------- | ------------ |
| 1          | 184.3     | 124.0     | 34.6     | 1.487     | no           |
| 2          | 186.3     | 125.1     | 34.9     | 1.490     | no           |
| 3          | 185.4     | 128.2     | 34.8     | 1.447     | no           |
| 4          | 173.7     | 118.2     | 32.6     | 1.470     | no           |
| 5          | 184.7     | 121.7     | 34.6     | 1.518     | no           |
| **median** | **184.7** | **124.0** | **34.6** | **1.487** | —            |

Required for AC-020 (test bound 1.5/8): speedup ≥ 5.33×.
Required for Pack 5 §1 (1.25/8 margin): speedup ≥ 6.4×.
Observed: 1.487× — far short on both. AC-017 + AC-018 stay
green throughout.

Experiment chain (3 KEEP diagnostic-only, 3 REVERT, 4 SKIPPED):

- **A.0** KEEP `fec71a0` — harness split.
- **A.1** KEEP `ca0d8f0` — perf record N=5 + flamegraphs;
  baseline-of-record seq=182 / conc=115 / speedup=1.58×.
- **A.2** PICK_B1 (main thread) — mutex_atomic 6.45→36.98
  (5.73× growth, +262M cycles).
- **A.3** PARTIAL_KEEP `edb0c84` — sqlite3_threadsafe()=1 +
  MUTEX_PTHREADS confirms A.2; strace skipped (no sudo).
- **A.4** PICK_B1 (main thread) — §5 OVERRIDE; numeric KEEP
  rule conc ≤ 80 ms AND speedup ≥ 5×; alt-on-fail extended.
- **B.1** REVERT `d448263` (output JSON only) — runtime
  CONFIG_MULTITHREAD; config_rc=SQLITE_OK proven (real §5
  differentiator vs SQLITE_MISUSE=21); AC-020 ratio flat;
  hypothesis "runtime threading flag relieves bottleneck"
  falsified clean.
- **C.1** REVERT `15c6473` (output JSON + evidence) —
  compile-time SQLITE_THREADSAFE=2 verified live; AC-020 still
  flat; the strongest possible mutex intervention; the A.2 hot
  symbols are NOT SQLite threading-mode mutexes. Mutex track
  CLOSED.
- **E.1** REVERT `1739b17`+`3e047a3` (revert of `e4ff255`+`91c69e9`)
  — `prepare_cached` on 4 read-path stmts; sequential -13.7%
  (parse cost real); concurrent unchanged; codex `gpt-5.4`
  reviewer BLOCK with three findings (pub surface, decision-rule
  mismatch, partial test). Phase 7/8 invariants PASS. Parse
  track CLOSED.
- **B.3 / D.1 / E.3 / E.4** SKIPPED — B.3/E.3/E.4 by evidence
  exhaustion; D.1 architectural, out of Pack 5 scope.

Net production-code LoC delta = **0**. Every src/ change in Pack
5 was reverted. Diagnostic test additions (+316 LoC under
`#[ignore]`) do not run in normal CI. Pack 5 is by construction
a _diagnostic packet_: its product is evidence + decision
records, not source-code changes.

**Pivot for the next packet (Pack 6 recommendation):**
architectural reader-pool refactor. Hypothesis: replace
`ReaderPool::Mutex<Vec<Connection>>` (`lib.rs:158`) with a
lock-free `crossbeam::queue::ArrayQueue<Connection>` for
borrow/release. Expected to remove ~6.8 % concurrent cycles
attributed to `___pthread_mutex_lock` + ~1.8 % from
`lll_mutex_lock_optimized` + a meaningful fraction of the
`__aarch64_swp4_rel` / `__aarch64_cas4_acq` CAS cycles that
land on the pool slot. Combined with a perf re-capture against
the post-revert tip to confirm the residual symbol-source
_before_ writing code. Smallest-radius architectural change
consistent with Pack 5's exhaustive falsification of the
SQLite-internal hypotheses.

**Alternative pivots if Pack 6 falsifies the lock-free-pool
hypothesis:** (a) replace rusqlite with raw `libsqlite3-sys` +
manual handle management to remove rusqlite's internal Mutex
(high LoC, high risk); (b) switch to WAL2 mode (SQLite 3.45+;
not yet in libsqlite3-sys-0.30) to reduce WAL shared-memory
atomic contention; (c) split readers from writers across
separate database files (architectural; loses cross-table
consistency).

**Recommended human decision:** approve Pack 6 per the lock-
free-pool starting point, OR explicitly defer AC-020 and ship
0.6.0 with the gate documented as known-not-met in
`dev/test-plan.md` (REQ-020 column flips to "deferred"). Either
way, the diagnostic record is sufficient that no future
contributor needs to re-walk the mutex / parse falsification.

**Meta-findings (apply to all future packets):**

- The orchestrator-subagent chaining bug caught at B.1 #1 cost
  one full Opus-high spawn cycle. The resume §4 anti-chaining
  defenses landed at `fc3dda3` (PREAMBLE prepended via stdin,
  `--disallowedTools Task Agent`, `--output-format stream-json
--include-partial-messages --verbose`) worked first try on
  every spawn after that. Keep all three on for every packet.
- The B.1 prompt's `sqlite3_threadsafe()==2` assertion was a
  real spec error inherited from whitepaper §7.3 — corrected
  in-place. Future spec writers should validate runtime-observable
  assertions against vendor docs before locking them into a
  decision rule (e.g. SQLite header `sqlite3.h:249-252` is the
  authoritative source on what `sqlite3_threadsafe()` reflects).
- Reviewer process held strict on contract issues (E.1: pub
  surface expansion, decision-rule mismatch). Reviewer mandate
  works as a guardrail; honor BLOCK verdicts even when the
  KEEP-narrative is sympathetic.
