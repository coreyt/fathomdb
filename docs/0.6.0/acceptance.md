---
title: 0.6.0 Acceptance Criteria
date: 2026-04-27
target_release: 0.6.0
desc: Testable AC-NNN criteria; each maps to a REQ + test id
blast_radius: test-plan.md (every AC → ≥1 test); requirements.md (every REQ → ≥1 AC); CI gate definitions; release-checklist.md
status: draft
---

# Acceptance Criteria

Format:

```
## AC-NNN: <short title>

**Requirement ref:** REQ-NNN
**Test id:** T-NNN (placeholder; bound by test-plan.md)
**Assertion:** <single observable, measurable, falsifiable statement>
**Measurement:** <how it's checked>
**Fixture:** <name of fixture, or "test-plan.md fixture spec — pending">
```

Rules:
- Unique `AC-NNN` id; numbering stable; suffixes a/b/c when an outcome
  splits.
- One assertion per AC (no compounds — no AND chains, no comma-list of
  observables).
- No "should / ideally / reasonable" — binary outcomes only.
- Every REQ in `requirements.md` has ≥1 AC.
- Every AC has a placeholder T-NNN; `test-plan.md` (Phase 3f) binds
  T-NNN to real test scaffolds.
- Every AC names its fixture, or explicitly marks the fixture as
  pending `test-plan.md`. ACs whose fixture is pending are
  **lock-blocking** on `test-plan.md`.
- Numerical gates restate the cited accepted ADR. AC must not
  introduce numbers absent from the ADR — if a measurement parameter
  (warmup, sample count, tolerance) is needed beyond the ADR, the
  parameter is owned by `test-plan.md`, not invented inline.

T-NNN ids are placeholders until `test-plan.md` issues real ones.

## Parameter table

acceptance.md OWNS every numerical measurement parameter cited by an
AC. test-plan.md is the *measurer* — it executes the protocol but does
not own the threshold. Parameters with an ADR source restate the ADR.
Parameters without an ADR source are owned by this doc and bound at
acceptance lock; changing them post-lock follows the same critic + HITL
cycle as any other acceptance amendment.

Discoverability: this is the canonical home for human + machine lookup
of a parameter value. CI/test scripts consume parameters by `P-ID` from
this table.

| P-ID | Used by AC | Description | Value | Source |
|---|---|---|---|---|
| P-WTP-WARMUP | AC-011a, AC-011b | Write-throughput pre-measurement warmup window | 5 s | acceptance.md (this doc) |
| P-WTP-RUN | AC-011a, AC-011b | Write-throughput steady-state measurement window | 60 s | acceptance.md |
| P-PERF-SAMPLES | AC-012, AC-013, AC-017, AC-019 | Minimum measured samples per percentile calculation | 1,000 | ADR-0.6.0-text-query-latency-gates (sets ≥ 1,000 for text); applied uniformly to all latency ACs |
| P-STRESS-MULT | AC-019 | Mixed-retrieval stress tail-latency multiplier vs baseline_p99 | 10× | acceptance.md |
| P-STRESS-FLOOR | AC-019 | Mixed-retrieval stress tail-latency floor (max(mult × baseline, floor)) | 150 ms | acceptance.md |
| P-PARALLEL-TOL | AC-020 | Concurrent-read wall-clock tolerance vs `T_seq / N` | 1.5× | acceptance.md |
| P-FD-TOL | AC-022b | Post-close FD-count tolerance vs pre-open count | +0 (engine FDs) plus runtime-tolerance counted as `≤ +5` for runtime/GC FDs | acceptance.md |
| P-LOCK-BOUND | AC-024a | Second-open `DatabaseLocked` rejection wall-clock bound | 1 s | acceptance.md |
| P-TAU | AC-027d | Per-query Kendall tau threshold for post-recovery vector top-k vs pre-corruption baseline | ≥ 0.9 | ADR-0.6.0-recovery-rank-correlation |
| P-TAU-PASS | AC-027d | Aggregate gate across the AC-027d query suite | 100% of queries meet P-TAU | ADR-0.6.0-recovery-rank-correlation |
| P-STALL-TOL | AC-029 | Projection-stall vs unstalled write throughput tolerance | 1.5× wall-clock (i.e. stalled ≤ 1.5 × unstalled) | acceptance.md |
| P-DRAIN-TOL | AC-032b | Drain-timeout overshoot tolerance — typed timeout returned within `tolerance × T` | 1.5× | acceptance.md |
| P-RETENTION-CAP | AC-033 | Default provenance row-count cap | 1,000,000 rows | ADR-0.6.0-provenance-retention |
| P-RETENTION-SLACK | AC-033 | Slack between cap and enforced upper bound (eviction batching headroom) | 5% (i.e. row count enforced as `≤ cap × 1.05`) | ADR-0.6.0-provenance-retention |
| P-RETENTION-EVICT | AC-033 | Eviction policy | Oldest-first by primary key | ADR-0.6.0-provenance-retention |
| P-AC033-WORKLOAD | AC-033 | Compressed-runtime workload write rate × duration (compressed for CI) | 10,000 writes/sec × 14 minutes (≈ 8.4 M writes; well past P-RETENTION-CAP × eviction cycles) | acceptance.md |
| P-AC033-SAMPLE | AC-033 | Row-count sampling cadence during AC-033 | every 30 s | acceptance.md |
| P-PWR-TRIALS | AC-034a, AC-034b | Power-cut harness trial count | 100 | acceptance.md |
| P-OS-TRIALS | AC-034c | OS-crash harness trial count | 50 | acceptance.md |
| P-RECOV-N | AC-035 | Recovery-time worst-of-N N value for 1 GB DB | 10 | acceptance.md |
| P-AC036-CYCLE | AC-036 | Open + write + search + close cycle iterations under no-listen syscall capture | 1 (single full cycle sufficient — assertion is binary) | acceptance.md |
| P-AC044-SENTINEL-LEN | AC-044 | Random-per-test sentinel byte length for shadow-table corruption detection | 16 bytes | acceptance.md |
| P-AC046-K | AC-046a, AC-046b, AC-046c | Migration step count (k) for n-to-n+k migration fixture | 3 | acceptance.md |

Parameters used inline by their assertion (e.g. AC-007a's `100 ms`
slow-statement default threshold; AC-022c's `5 s` close-to-exit) are
already in the AC text and not duplicated here — they restate
`requirements.md` REQ-006a / REQ-020b which are themselves anchored.

## Traceability matrix

REQ → AC → P-ID coverage. Every numeric AC parameter resolves through
this table to either an ADR or to an acceptance.md self-owned bullet.

| AC | Owning REQ | Parameters consumed | Authoritative source(s) |
|---|---|---|---|
| AC-011a/b | REQ-009a/b | P-WTP-WARMUP, P-WTP-RUN | ADR-0.6.0-write-throughput-sli (gate); acceptance.md (protocol) |
| AC-012 | REQ-010 | P-PERF-SAMPLES | ADR-0.6.0-text-query-latency-gates |
| AC-013 | REQ-011 | P-PERF-SAMPLES | ADR-0.6.0-retrieval-latency-gates |
| AC-017 | REQ-015 | P-PERF-SAMPLES | ADR-0.6.0-projection-freshness-sli |
| AC-019 | REQ-017 | P-PERF-SAMPLES, P-STRESS-MULT, P-STRESS-FLOOR | acceptance.md |
| AC-020 | REQ-018 | P-PARALLEL-TOL | acceptance.md |
| AC-022b | REQ-020a | P-FD-TOL | acceptance.md |
| AC-024a | REQ-022a | P-LOCK-BOUND | acceptance.md |
| AC-027d | REQ-025c | P-TAU, P-TAU-PASS | ADR-0.6.0-recovery-rank-correlation |
| AC-029 | REQ-027 | P-STALL-TOL | acceptance.md |
| AC-032b | REQ-030 | P-DRAIN-TOL | acceptance.md |
| AC-033 | REQ-031 | P-RETENTION-CAP, P-RETENTION-SLACK, P-RETENTION-EVICT, P-AC033-WORKLOAD, P-AC033-SAMPLE | ADR-0.6.0-provenance-retention (cap/slack/policy); acceptance.md (workload/sample) |
| AC-034a/b | REQ-031b | P-PWR-TRIALS | acceptance.md |
| AC-034c | REQ-031b | P-OS-TRIALS | acceptance.md |
| AC-035 | REQ-031c | P-RECOV-N | acceptance.md |
| AC-036 | REQ-032 | P-AC036-CYCLE | acceptance.md |
| AC-044 | REQ-040 | P-AC044-SENTINEL-LEN | acceptance.md |
| AC-046a/b/c | REQ-042 | P-AC046-K | acceptance.md |

ACs not listed here have no quantitative parameter (purely structural
or boolean assertions).

---

## Observability

## AC-001: Lifecycle phase tag is a typed enum
**Requirement ref:** REQ-001
**Test id:** T-001
**Assertion:** Every lifecycle event carries a `phase` field whose value is one of the typed constants `{Started, Heartbeat, Finished, Failed}`, programmatically retrievable as the typed value (not as a substring of a free-text field). (Slow-phase coverage: AC-008.)
**Measurement:** Subscribe to lifecycle events for an open + 10-write + 10-search + close sequence; assert each event's `phase` field deserializes to one of the four constants; assert zero events require string parsing to extract the phase.
**Fixture:** standard-mixed-workload (test-plan.md fixture spec — pending).

## AC-002: No log files written without subscriber
**Requirement ref:** REQ-002
**Test id:** T-002
**Assertion:** With no host subscriber registered, an open + write + search + close cycle creates no new files outside the documented allow-list (DB file, `.lock`, WAL, SHM, optional rollback `.journal`).
**Measurement:** Snapshot recursive directory tree of `$PWD`, `$HOME`, `$XDG_*`, `$TMPDIR` pre+post; assert diff = subset of allow-list paths.
**Fixture:** clean-temp-root (test-plan.md fixture spec — pending).

## AC-003a: Writer events flow to host subscriber
**Requirement ref:** REQ-002
**Test id:** T-003a
**Assertion:** A write operation produces ≥ 1 event delivered to the host's idiomatic logging hook before the write call returns to the caller.
**Measurement:** Register binding-idiomatic logging hook; capture events; perform 1 write; assert ≥ 1 captured event with `category=writer` whose capture-ordinal precedes the write's return.
**Fixture:** single-write fixture (test-plan.md fixture spec — pending).

## AC-003b: Search events flow to host subscriber
**Requirement ref:** REQ-002
**Test id:** T-003b
**Assertion:** A search operation produces ≥ 1 `category=search` event delivered to the host hook before the call returns.
**Measurement:** As AC-003a with search.
**Fixture:** single-search fixture.

## AC-003c: Admin events flow to host subscriber
**Requirement ref:** REQ-002
**Test id:** T-003c
**Assertion:** An admin operation produces ≥ 1 `category=admin` event delivered to the host hook before the call returns.
**Measurement:** As AC-003a with admin.configure.
**Fixture:** single-admin fixture.

## AC-003d: Error events flow to host subscriber
**Requirement ref:** REQ-002
**Test id:** T-003d
**Assertion:** A failing operation produces ≥ 1 `category=error` event delivered to the host hook before the failure is raised to the caller.
**Measurement:** Trigger a deterministic failure (poison fixture); assert ≥ 1 `category=error` event with capture-ordinal < raise-ordinal.
**Fixture:** poison-fixture (test-plan.md fixture spec — pending).

## AC-004a: Counter snapshot exposes documented key set
**Requirement ref:** REQ-003
**Test id:** T-004a
**Assertion:** A counter snapshot contains the keys: `queries`, `writes`, `write_rows`, `errors_by_code`, `admin_ops`, `cache_hit`, `cache_miss`.
**Measurement:** Read snapshot on a fresh engine; assert exact key-set equality.
**Fixture:** fresh-engine.

## AC-004b: Counter delta exact for write/query keys
**Requirement ref:** REQ-003
**Test id:** T-004b
**Assertion:** Snapshot delta over N=1,000 mixed ops equals issued op counts exactly for `queries`, `writes`, `write_rows`, `admin_ops`. `cache_hit` / `cache_miss` are monotonic non-decreasing.
**Measurement:** Snapshot at t0; run fixture; snapshot at t1; assert per-key arithmetic.
**Fixture:** mixed-1000-ops fixture (test-plan.md fixture spec — pending).

## AC-004c: Counter snapshot read does not perturb counters
**Requirement ref:** REQ-003
**Test id:** T-004c
**Assertion:** Reading a counter snapshot increments no counter on the snapshot itself.
**Measurement:** Snapshot S0; snapshot S1 immediately after; assert S0 == S1 for every key.
**Fixture:** quiescent-engine.

## AC-005a: Per-statement profiling toggleable at runtime
**Requirement ref:** REQ-004
**Test id:** T-005a
**Assertion:** A documented API call enables per-statement profiling on a running engine without restart and without rebuild.
**Measurement:** Open engine; assert profiling disabled (no profile records on a fixture query); call enable-profiling API; assert subsequent fixture query emits ≥ 1 profile record.
**Fixture:** non-trivial-select fixture (test-plan.md fixture spec — pending — must scan ≥ 1 row).

## AC-005b: Profile record schema
**Requirement ref:** REQ-004
**Test id:** T-005b
**Assertion:** A profile record exposes fields `wall_clock_ms`, `step_count`, `cache_delta` as typed numeric values.
**Measurement:** Emit one profile record via AC-005a; deserialize; assert all three fields present and numeric.
**Fixture:** as AC-005a.

## AC-006: SQLite-internal events surfaced with typed source tag
**Requirement ref:** REQ-005
**Test id:** T-006
**Assertion:** SQLite-internal corruption / recovery / I/O events carry a `source` field equal to the typed constant `SqliteInternal` and a `category` field equal to a value from the documented SQLite-internal category set.
**Measurement:** Inject corruption via the documented corruption-injection harness; reopen; assert ≥ 1 captured event with `source == SqliteInternal` and `category` ∈ documented set.
**Fixture:** corrupt-page harness (test-plan.md fixture spec — pending; must include a documented page-corruption tool).

## AC-007a: Slow-statement event at default threshold
**Requirement ref:** REQ-006a
**Test id:** T-007a
**Assertion:** A statement whose wall-clock duration exceeds 100 ms emits exactly one slow-statement event identifying the statement.
**Measurement:** Run the deterministic-slow fixture (≥ 200 ms guaranteed by recursive-CTE counter); assert exactly one slow-statement event with the matching statement id.
**Fixture:** deterministic-slow-cte fixture (test-plan.md fixture spec — pending).

## AC-007b: Slow threshold reconfigurable at runtime
**Requirement ref:** REQ-006a
**Test id:** T-007b
**Assertion:** Setting threshold to N ms via documented API causes statements with measured duration ≥ N ms to emit a slow event and statements with measured duration < N ms not to emit.
**Measurement:** Set N=500; run fast-fixture (≤ 200 ms guaranteed) → assert no slow event; run slow-fixture (≥ 600 ms guaranteed) → assert one slow event.
**Fixture:** fast-fixture + slow-fixture (test-plan.md fixture spec — pending).

## AC-008: Slow signal participates in lifecycle attribution
**Requirement ref:** REQ-006b
**Test id:** T-008
**Assertion:** A statement crossing the slow threshold causes the lifecycle phase tag to take the value `Slow` for ≥ 1 event during the statement's wall-clock window.
**Measurement:** Subscribe to lifecycle stream; run 1 fast + 1 slow + 1 fast statement; assert the slow statement's wall-clock window contains ≥ 1 event with `phase == Slow` (subsequence, not contiguous order).
**Fixture:** as AC-007a.

## AC-009: Stress-failure event field schema
**Requirement ref:** REQ-007
**Test id:** T-009
**Assertion:** A stress-test failure event deserializes into a typed payload with fields `thread_group_id`, `op_kind`, `last_error_chain`, `projection_state`, each non-empty for the failing scenario.
**Measurement:** Run robustness suite with one-thread poison fixture; deserialize the failure event payload using the documented serde-typed schema; assert all four fields populated.
**Fixture:** one-thread-poison robustness fixture (test-plan.md fixture spec — pending).

## AC-010: Projection-status enum coverage
**Requirement ref:** REQ-008
**Test id:** T-010
**Assertion:** Projection-status query returns a value from the typed enum `{Pending, Failed, UpToDate}` for every kind with vector indexing enabled.
**Measurement:** Three named fixtures (pending — frozen scheduler; failed — poison embedder; up-to-date — quiescent); assert returned enum value matches expected.
**Fixture:** projection-status-three-state fixture (test-plan.md fixture spec — pending).

## Performance

(Numerical gates restate ADR thresholds; measurement parameters
— warmup, sample count, runner pinning, tolerances — are owned by the
**Parameter table** above (cited by P-ID). `test-plan.md` is the
*measurer* that executes the protocol; it does not own thresholds.
Fixture data corpora at scale (1M-row, 1GB-DB, harness binaries) are
the only test-plan.md responsibility for this section.)

## AC-011a: Write throughput @ 1 KB ≥ 1,000 commits/sec
**Requirement ref:** REQ-009a
**Test id:** T-011a
**Assertion:** Sequential `WriteTx` commits with 1 KB payload sustain ≥ 1,000 commits/sec.
**Measurement:** P-WTP-WARMUP warmup → P-WTP-RUN steady-state measurement window; commits/sec computed over the run window; CI gate fails if value < 1,000.
**Fixture:** write-throughput-1kb (test-plan.md fixture spec — pending).

## AC-011b: Write throughput @ 100 KB ≥ 100 commits/sec
**Requirement ref:** REQ-009b
**Test id:** T-011b
**Assertion:** Sequential `WriteTx` commits with 100 KB payload sustain ≥ 100 commits/sec, measured per the same protocol.
**Measurement:** As AC-011a with 100 KB payload.
**Fixture:** write-throughput-100kb (test-plan.md fixture spec — pending).

## AC-012: Text query latency on FTS5 path
**Requirement ref:** REQ-010
**Test id:** T-012
**Assertion:** Text-only query latency on the documented FTS5 fixture meets p50 ≤ 20 ms AND p99 ≤ 150 ms over ≥ P-PERF-SAMPLES samples on a single distribution.
**Measurement:** Per ADR-0.6.0-text-query-latency-gates workload (warmup discard + second-pass measurement, QPS=1, 50–90th percentile token-frequency band); CI gate fails if either percentile exceeds.
**Fixture:** text-query-1m-chunk (test-plan.md fixture spec — pending).

## AC-013: Vector retrieval latency
**Requirement ref:** REQ-011
**Test id:** T-013
**Assertion:** Vector retrieval on the documented vector fixture meets p50 ≤ 50 ms AND p99 ≤ 200 ms over ≥ P-PERF-SAMPLES samples.
**Measurement:** Per ADR-0.6.0-retrieval-latency-gates workload (warmup discard + second-pass, QPS=1); CI gate fails if either percentile exceeds.
**Fixture:** vector-1m-768d (test-plan.md fixture spec — pending).

## AC-014: `safe_export` ≤ 500 ms on seeded dataset
**Requirement ref:** REQ-012
**Test id:** T-014
**Assertion:** `safe_export` completes within 500 ms wall-clock on the seeded benchmark dataset.
**Measurement:** Single execution against the seeded fixture; CI gate fails if wall-clock > 500 ms. (Single-sample assertion sufficient — gate is a hard ceiling, not a percentile.)
**Fixture:** seeded-benchmark-dataset (test-plan.md fixture spec — pending).

## AC-015: Canonical-read freshness within write tx
**Requirement ref:** REQ-013
**Test id:** T-015
**Assertion:** A canonical-row read issued immediately after `write` returns reflects the just-written row on the first call (no retry, no poll).
**Measurement:** Single-thread test: write row R, immediately query R by id without intervening operation; assert R returned on first call; per-call wall-clock ≤ 50 ms; repeat 1,000 times; assert 100% first-call success.
**Fixture:** canonical-write-read fixture.

## AC-016: FTS-search freshness within write tx
**Requirement ref:** REQ-014
**Test id:** T-016
**Assertion:** An FTS5 query for a token unique to a just-written row returns that row on the first call after `write` returns.
**Measurement:** Same protocol as AC-015 with FTS5 query for a unique token; per-call wall-clock ≤ 50 ms; 1,000 iterations; 100% first-call success.
**Fixture:** unique-token fixture.

## AC-017: Vector-projection freshness p99 ≤ 5 s
**Requirement ref:** REQ-015
**Test id:** T-017
**Assertion:** Latency from write commit to projection-cursor reaching the commit's cursor value has p99 ≤ 5,000 ms over ≥ P-PERF-SAMPLES samples.
**Measurement:** Per write: capture commit-cursor `c_w` (REQ-055 surface); poll read-tx cursor until `c_r >= c_w`; record polling-completion time minus commit time; report p99; CI gate fails if > 5,000 ms.
**Fixture:** projection-freshness fixture (test-plan.md fixture spec — pending sample-count).

## AC-018: Drain of 100 vectors ≤ 2 s
**Requirement ref:** REQ-016
**Test id:** T-018
**Assertion:** The bounded-completion verb (per REQ-030) called with 100 pending deterministic-embedder vectors returns within 2 s wall-clock.
**Measurement:** Enqueue 100 writes against deterministic embedder; immediately call drain with 5 s timeout; assert returns within 2 s with all 100 vectors materialized.
**Fixture:** deterministic-embedder-100-vector fixture (test-plan.md fixture spec — pending).

## AC-019: Mixed-retrieval stress workload tail
**Requirement ref:** REQ-017
**Test id:** T-019
**Assertion:** Under the documented mixed-retrieval stress workload, read p99 ≤ `max(P-STRESS-MULT × baseline_p99, P-STRESS-FLOOR)` over ≥ P-PERF-SAMPLES samples, where `baseline_p99` is captured by re-running AC-013's protocol immediately preceding this AC in the same CI job.
**Measurement:** Run baseline first; freeze workload; run stress; assert bound.
**Fixture:** mixed-retrieval-stress (test-plan.md fixture spec — pending).

## AC-020: Reads do not serialize on a single reader connection
**Requirement ref:** REQ-018
**Test id:** T-020
**Assertion:** N=8 concurrent reader threads each running the documented read-mix complete in wall-clock ≤ P-PARALLEL-TOL × `(T_seq / N)`, where `T_seq` is the sequential N-iteration wall-clock.
**Measurement:** Run sequential and concurrent variants; assert the bound; fail CI if exceeded.
**Fixture:** interactive-read-mix (test-plan.md fixture spec — pending — must specify per-query-type ratios + tolerance).

## Reliability

## AC-021: Zero `SQLITE_SCHEMA` warnings under concurrent reads + admin DDL
**Requirement ref:** REQ-019
**Test id:** T-021
**Assertion:** A workload mixing 8 concurrent reader threads with 1 admin DDL operation/sec for 60 s emits zero events with `code == SQLITE_SCHEMA`.
**Measurement:** Subscribe to error stream; run fixture (DDL operations enumerated: `admin.configure_kind` add + remove cycle, schema-projection rebuild); assert event count = 0.
**Fixture:** schema-flood fixture (test-plan.md fixture spec — pending — must enumerate DDL operations under test).

## AC-022a: Engine close releases lock
**Requirement ref:** REQ-020a
**Test id:** T-022a
**Assertion:** After `Engine.close()` returns, the database file's exclusive lock is released and a sibling process can acquire it.
**Measurement:** Sibling process attempts open-and-acquire-lock immediately after close-return in parent; assert sibling succeeds within 1 s.
**Fixture:** parent-child-process fixture.

## AC-022b: Engine close does not leak FDs
**Requirement ref:** REQ-020a
**Test id:** T-022b
**Assertion:** Post-close FD count for the host process is ≤ pre-open FD count + P-FD-TOL.
**Measurement:** Capture pre-open + post-close FD count; assert bound.
**Fixture:** open-close fixture.

## AC-022c: Host process exits ≤ 5 s of close
**Requirement ref:** REQ-020b
**Test id:** T-022c
**Assertion:** A host process whose only work is `Engine.open(); Engine.close()` exits within 5 s of `close()` returning.
**Measurement:** Spawn subprocess; time from close-return to process-exit; assert ≤ 5 s.
**Fixture:** open-close subprocess.

## AC-023a: Bounded process exit ≤ 5 s on main-return without explicit close
**Requirement ref:** REQ-021
**Test id:** T-023a
**Assertion:** A subprocess that opens an engine, drops the local handle, and returns from main exits within 5 s.
**Measurement:** Time from main-return to process-exit; assert ≤ 5 s.
**Fixture:** open-no-close-handle-dropped subprocess.

## AC-023b: Bounded process exit ≤ 5 s on main-return with engine in module-level global
**Requirement ref:** REQ-021
**Test id:** T-023b
**Assertion:** A subprocess that opens an engine bound to a module-level global (handle never explicitly dropped) and returns from main exits within 5 s.
**Measurement:** Time from main-return to process-exit; assert ≤ 5 s.
**Fixture:** open-no-close-global-held subprocess.

## AC-024a: `DatabaseLocked` rejection on second open
**Requirement ref:** REQ-022a
**Test id:** T-024a
**Assertion:** Opening a second engine on a database file held by a first engine raises a typed `DatabaseLocked` error within P-LOCK-BOUND, including while the first engine has pending vector work.
**Measurement:** Open A; enqueue 100 vector writes; attempt second open from sibling process; assert typed exception within P-LOCK-BOUND; repeat 10× for smoke.
**Fixture:** second-open-with-pending-vector fixture.

## AC-024b: Rejected second open never modifies file
**Requirement ref:** REQ-022b
**Test id:** T-024b
**Assertion:** A rejected second-open attempt leaves the database file byte-identical to its pre-attempt state.
**Measurement:** SHA-256 pre-attempt; perform AC-024a sequence; SHA-256 post-attempt; assert equal.
**Fixture:** as AC-024a.

## AC-025: No hang on engine drop with pending vector work
**Requirement ref:** REQ-023
**Test id:** T-025
**Assertion:** Dropping an engine with 1,000 pending vector projection jobs returns control to the caller within 30 s wall-clock (no-hang proxy for deadlock-freedom).
**Measurement:** Open engine; enqueue 1,000 deterministic-embedder writes; immediately drop without explicit drain; assert drop returns within 30 s.
**Fixture:** drop-with-pending-vector fixture.

## AC-026: `safe_export` covers WAL-only commits
**Requirement ref:** REQ-024
**Test id:** T-026
**Assertion:** A `safe_export` artifact captured immediately after a write committed only into the WAL (no checkpoint) contains that write when restored to a fresh DB.
**Measurement:** Disable auto-checkpoint; write row R; safe_export; restore artifact; query R; assert present.
**Fixture:** wal-only-commit fixture.

## AC-027a: Recovery preserves canonical rows
**Requirement ref:** REQ-025a
**Test id:** T-027a
**Assertion:** After recovery from a corrupted-shadow-table state, every canonical row committed pre-corruption is queryable by id post-recovery.
**Measurement:** Seed N=10,000 canonical rows; corrupt FTS5 + vec0 shadow tables via the documented corruption harness; run recovery; assert all 10,000 canonical rows queryable by id.
**Fixture:** seeded-10k-canonical + shadow-corruption harness (test-plan.md fixture spec — pending).

## AC-027b: Recovery restores FTS query result equality
**Requirement ref:** REQ-025b
**Test id:** T-027b
**Assertion:** Pre-corruption FTS5 query result row-id sets equal post-recovery FTS5 query result row-id sets for the documented 100-query suite.
**Measurement:** Capture pre-corruption result row-id sets; perform AC-027a corruption + recovery; re-run; assert per-query set equality.
**Fixture:** fts-100-query suite (test-plan.md fixture spec — pending).

## AC-027c: Recovery preserves vector profile metadata bit-equal
**Requirement ref:** REQ-025c
**Test id:** T-027c
**Assertion:** Post-recovery vector profile metadata (embedder identity, dimension) equals pre-corruption metadata bit-for-bit.
**Measurement:** Snapshot metadata pre-corruption; perform corruption + recovery; re-snapshot; assert equality.
**Fixture:** as AC-027a.

## AC-027d: Recovery preserves vector top-k rank-correlation
**Requirement ref:** REQ-025c
**Test id:** T-027d
**Assertion:** Post-recovery top-k vector query results have per-query Kendall tau ≥ P-TAU vs pre-corruption results, with P-TAU-PASS aggregate gate, for the documented 100-query suite.
**Measurement:** Snapshot pre-corruption top-10; perform corruption + recovery; re-snapshot; compute Kendall tau per query; assert per-query tau ≥ P-TAU; assert P-TAU-PASS satisfied.
**Fixture:** vector-100-query suite (test-plan.md fixture spec — pending).

## AC-028a: `excise_source` writes audit row
**Requirement ref:** REQ-026
**Test id:** T-028a
**Assertion:** After `excise_source(<id>)`, an audit-trail row exists naming the excised source id and the operation timestamp.
**Measurement:** Seed source S1; excise; query audit table for `source_id == S1`; assert ≥ 1 row.
**Fixture:** two-source seed.

## AC-028b: `excise_source` removes residue from projections
**Requirement ref:** REQ-026
**Test id:** T-028b
**Assertion:** After `excise_source(S1)`, FTS5 + vector projections contain zero rows attributable to S1.
**Measurement:** Query projections for tokens/vectors known to come only from S1's rows; assert empty.
**Fixture:** as AC-028a.

## AC-028c: `excise_source` does not perturb non-excised projections
**Requirement ref:** REQ-026
**Test id:** T-028c
**Assertion:** Pre-excise projection result sets for non-excised sources equal post-excise result sets.
**Measurement:** Capture S2 result sets pre-excise; excise S1; re-capture S2; assert equality.
**Fixture:** as AC-028a.

## AC-029: Canonical writes complete under projection stall
**Requirement ref:** REQ-027
**Test id:** T-029
**Assertion:** With FTS5 and vector projection schedulers frozen, 1,000 sequential canonical writes complete with stalled-projection wall-clock ≤ P-STALL-TOL × unstalled-projection wall-clock.
**Measurement:** Capture baseline 1,000-write wall-clock; freeze projection schedulers; capture stalled wall-clock; assert ratio ≤ P-STALL-TOL.
**Fixture:** projection-stall fixture.

## AC-030a: Misconfig — no embedder wired
**Requirement ref:** REQ-028a
**Test id:** T-030a
**Assertion:** Calling a vector-requiring operation on an engine with no embedder configured raises typed `EmbedderNotConfigured` at the call boundary.
**Measurement:** Open engine without embedder config; call vector write; assert exception type matches; assert no row inserted in any vector table.
**Fixture:** no-embedder-config fixture.

## AC-030b: Misconfig — kind not vector-indexed
**Requirement ref:** REQ-028b
**Test id:** T-030b
**Assertion:** Calling a vector operation against a kind not configured for vector indexing raises typed `KindNotVectorIndexed` at the call boundary.
**Measurement:** Configure kind K1 without vector; vector-search K1; assert exception; assert projection tables untouched.
**Fixture:** non-vector-kind fixture.

## AC-030c: Misconfig — embedder dimension mismatch at call boundary
**Requirement ref:** REQ-028c
**Test id:** T-030c
**Assertion:** A vector operation submitted with an embedder whose runtime-produced dimension differs from the stored profile raises typed `EmbedderDimensionMismatch` at the call boundary, naming both expected and actual dimensions. (Re-open boundary covered by AC-048.)
**Measurement:** Configure stored profile dim=768; submit a vector from a dim=384 embedder via the call API; assert typed exception with `expected: 768`, `actual: 384` populated.
**Fixture:** dim-mismatch-call fixture (distinct from AC-048's reopen scenario).

## AC-031: Hybrid retrieval surfaces soft-fallback signal
**Requirement ref:** REQ-029
**Test id:** T-031
**Assertion:** A hybrid retrieval call that loses one branch returns a result AND a typed soft-fallback record naming the missed branch. (Field name owned by binding-interface ADRs — assertion testable on the typed record's presence + branch-name field.)
**Measurement:** Hybrid query; freeze vector scheduler so vector branch returns no fresh data; assert result returned; assert response carries a soft-fallback record whose `branch` field == `Vector`.
**Fixture:** hybrid-fallback-vector fixture.

## AC-032a: Bounded background-work — completes within timeout
**Requirement ref:** REQ-030
**Test id:** T-032a
**Assertion:** Calling the bounded-completion verb with N pending jobs and a timeout T sufficient to complete N jobs returns success within T.
**Measurement:** Enqueue 10 deterministic jobs; call drain(timeout=10s); assert returns success within 10s.
**Fixture:** small-batch-drain fixture.

## AC-032b: Bounded background-work — typed timeout error
**Requirement ref:** REQ-030
**Test id:** T-032b
**Assertion:** Calling the bounded-completion verb with timeout T smaller than completion time returns a typed timeout error within P-DRAIN-TOL × T.
**Measurement:** Enqueue 10,000 jobs; call drain(timeout=1s); assert typed timeout returned within P-DRAIN-TOL × 1s.
**Fixture:** large-batch-drain fixture.

## AC-033: Bounded provenance growth (compressed runtime)
**Requirement ref:** REQ-031
**Test id:** T-033
**Assertion:** Under the P-AC033-WORKLOAD compressed-runtime workload, provenance table row count stops growing once P-RETENTION-CAP is reached and remains ≤ `P-RETENTION-CAP × (1 + P-RETENTION-SLACK)`. Eviction obeys P-RETENTION-EVICT.
**Measurement:** Configure retention cap = P-RETENTION-CAP; run P-AC033-WORKLOAD; sample row count every P-AC033-SAMPLE; assert row-count bound after first crossing; assert evicted rows are oldest by primary key.
**Fixture:** compressed-runtime-write fixture.

## AC-034a: Zero corruption on power-cut
**Requirement ref:** REQ-031b
**Test id:** T-034a
**Assertion:** Power-cut simulation per the documented power-cut harness, repeated P-PWR-TRIALS times, leaves `PRAGMA integrity_check = ok` on every reopen.
**Measurement:** Per harness invocation: `kill -9` mid-commit at randomized times; reopen; run integrity_check; assert `ok` on every trial.
**Fixture:** power-cut harness (test-plan.md owns harness path + tooling; trial count = P-PWR-TRIALS).

## AC-034b: Power-cut final-commit-loss bound
**Requirement ref:** REQ-031b
**Test id:** T-034b
**Assertion:** Across the AC-034a P-PWR-TRIALS trial set, lost-commit duration p99 ≤ 100 ms.
**Measurement:** Per trial: record last-surviving-commit timestamp + kill timestamp; report p99 across P-PWR-TRIALS trials.
**Fixture:** as AC-034a.

## AC-034c: Zero commit loss on OS-crash
**Requirement ref:** REQ-031b
**Test id:** T-034c
**Assertion:** OS-crash simulation per the documented OS-crash harness (block-device sync barrier preserved), repeated P-OS-TRIALS times, loses zero committed transactions per trial.
**Measurement:** Per trial: write workload in VM; trigger crash via documented mechanism; reopen; assert zero committed-tx loss; sum across P-OS-TRIALS trials = 0.
**Fixture:** OS-crash harness (test-plan.md owns VM image + trigger mechanism, e.g. `echo c > /proc/sysrq-trigger` inside KVM with sync barrier preserved).

## AC-035: Recovery time ≤ 2 s for 1 GB DB (worst-of-10)
**Requirement ref:** REQ-031c
**Test id:** T-035
**Assertion:** Worst-of-P-RECOV-N measured `Engine.open` time (process-start → first-write-accept) on a 1 GB seeded DB after unclean shutdown is ≤ 2 s.
**Measurement:** Seed 1 GB DB; `kill -9` mid-write; time open + first-write-accept; repeat P-RECOV-N times; report worst; assert ≤ 2 s.
**Fixture:** 1gb-unclean-shutdown fixture (test-plan.md fixture spec — pending).

## AC-035a: Engine.open refuses on detected corruption
**Requirement ref:** REQ-031d
**Test id:** T-035a
**Assertion:** For each documented open-path corruption fixture in the 0.6.0 matrix `{WalReplayFailure, HeaderMalformed, SchemaInconsistent, EmbedderIdentityDrift}`, `Engine.open` returns `Err(EngineOpenError::Corruption(_))`. The engine never returns an `Engine` handle, never auto-truncates, never auto-rebuilds, never opens read-only.
**Measurement:** Run four fixtures, one per open-path `CorruptionKind`: WAL-replay corruption, header/page-1 corruption, schema-probe inconsistency, and corrupt stored embedder-profile row. Per fixture: invoke `Engine.open`; assert result is `Err`; downcast to `EngineOpenError::Corruption`; assert no `Engine` handle observable in caller scope; assert DB file mtime unchanged across the failed open (no truncation / no rebuild side effect); inspect process for absence of writer thread + scheduler.
**Fixture:** open-path corruption matrix (exactly four fixtures: `WalReplayFailure`, `HeaderMalformed`, `SchemaInconsistent`, `EmbedderIdentityDrift`; test-plan.md fixture spec — pending).

## AC-035b: CorruptionDetail shape
**Requirement ref:** REQ-031d
**Test id:** T-035b
**Assertion:** Every `EngineOpenError::Corruption(detail)` returned by AC-035a fixtures carries: (1) `kind: CorruptionKind` in `{WalReplayFailure, HeaderMalformed, SchemaInconsistent, EmbedderIdentityDrift}`, (2) `stage: OpenStage` in `{WalReplay, HeaderProbe, SchemaProbe, EmbedderIdentity}` and never `LockAcquisition`, (3) `locator: CorruptionLocator` with no free-form `Unspecified` and opaque-SQLite paths surfaced as `OpaqueSqliteError { sqlite_extended_code: i32 }`, (4) `recovery_hint: RecoveryHint { code: &'static str, doc_anchor: &'static str }` with stable machine-readable `code`.
**Measurement:** Per AC-035a fixture: extract the four fields; assert presence + variant correctness; assert `(kind, stage, recovery_hint.code)` matches the documented rows `{WalReplayFailure, WalReplay, E_CORRUPT_WAL_REPLAY}`, `{HeaderMalformed, HeaderProbe, E_CORRUPT_HEADER}`, `{SchemaInconsistent, SchemaProbe, E_CORRUPT_SCHEMA}`, `{EmbedderIdentityDrift, EmbedderIdentity, E_CORRUPT_EMBEDDER_IDENTITY}`; assert `code` stability by re-running fixture and asserting bit-equal `code` string.
**Fixture:** as AC-035a.

## AC-035c: Lock released + no SQLite connection retained on Corruption error
**Requirement ref:** REQ-031d
**Test id:** T-035c
**Assertion:** After `Engine.open` returns `Corruption`, the exclusive WAL lock on `{database_path}.lock` is released (a fresh `Engine.open` from a sibling process succeeds against the same path, modulo the corruption surfacing again); no SQLite connection to the database is observably retained by the failed-open process; no fathomdb writer thread or scheduler runtime is running in the failed-open process.
**Measurement:** Trigger AC-035a fixture in process A; from sibling process B attempt `flock` (or equivalent) on the lock file → assert acquirable; in process A inspect open file descriptors → assert no fd points at the database file; inspect threads → assert no thread named per fathomdb writer / scheduler conventions.
**Fixture:** sibling-lock + fd-introspection fixture.

## AC-035d: Recovery reachable only via CLI
**Requirement ref:** REQ-031d
**Test id:** T-035d
**Assertion:** `fathomdb recover` is invocable via the CLI with `--help` properties per AC-040a / AC-040b; no recovery verb is reachable from the runtime SDK (Python / TypeScript) — no public symbol named `recover`, `restore_*`, `repair`, or equivalent is exposed by the five-verb application surface (REQ-053 / AC-057a).
**Measurement:** (1) Invoke `fathomdb recover --help`; assert per AC-040a / AC-040b. (2) Per binding: enumerate the public surface per AC-057a's surface definition; assert none of `{recover, restore, repair, fix, rebuild}` are members.
**Fixture:** as AC-040a + as AC-057a.

## Security

## AC-036: No listening sockets opened
**Requirement ref:** REQ-032
**Test id:** T-036
**Assertion:** During a full open + write + search + close cycle, fathomdb makes zero successful `listen(2)` syscalls.
**Measurement:** Run cycle under `bpftrace` / `auditd` capture of `socket()` + `listen()` syscalls scoped to fathomdb's pid + threads; assert zero `listen` calls reaching LISTEN state.
**Fixture:** standard cycle.

## AC-037: No outbound network requests on open with embedder configured
**Requirement ref:** REQ-033
**Test id:** T-037
**Assertion:** `Engine.open` on a fresh database, with the default embedder configured by the caller, triggers zero outbound network requests.
**Measurement:** Run `Engine.open` inside a network namespace with default-deny egress; assert open succeeds and no `connect()` syscalls outside loopback.
**Fixture:** netns-deny-egress fixture (test-plan.md fixture spec — pending).

## AC-038: FTS5-injection-safe text query
**Requirement ref:** REQ-034
**Test id:** T-038
**Assertion:** A query containing FTS5 control syntax submitted via `search` returns a result set equivalent to the safe-grammar parser's literal-token interpretation, and raises zero `SQLITE_ERROR` (malformed MATCH expression) regardless of input.
**Measurement:** 100 fixture queries containing FTS5 syntax characters (`"`, `*`, `^`, `NEAR`, `AND`, `OR`); for each, assert result set matches the safe-grammar reference output and zero `SQLITE_ERROR` raised.
**Fixture:** fts5-injection-100-query suite (test-plan.md fixture spec — pending; reference output pending).

## AC-039a: `safe_export` artifact ships SHA-256 manifest matching contents
**Requirement ref:** REQ-035
**Test id:** T-039a
**Assertion:** Every `safe_export` artifact has a SHA-256 manifest whose digest equals a fresh recomputation over the artifact bytes.
**Measurement:** Run `safe_export`; recompute SHA-256; assert equal to manifest.
**Fixture:** standard safe-export.

## AC-039b: Tampered artifact detected by verifier
**Requirement ref:** REQ-035
**Test id:** T-039b
**Assertion:** The documented verifier tool reports mismatch when a single byte of a `safe_export` artifact is altered.
**Measurement:** Tamper one byte; run verifier; assert non-zero exit + named-mismatch output.
**Fixture:** as AC-039a + 1-byte tamper.

## Operability

## AC-040a: Every `fathomdb doctor` verb invocable
**Requirement ref:** REQ-036
**Test id:** T-040a
**Assertion:** For each verb in `{check-integrity, safe-export, verify-embedder, trace, dump-schema, dump-row-counts, dump-profile}`, `fathomdb doctor <verb> --help` exits 0.
**Measurement:** Loop the verb set; assert exit 0 each.
**Fixture:** built CLI binary.

## AC-040b: Every `fathomdb doctor` verb has usage section in help
**Requirement ref:** REQ-036
**Test id:** T-040b
**Assertion:** For each verb above, `--help` output contains a `Usage:` section.
**Measurement:** Loop; grep `^Usage:` in output; assert match.
**Fixture:** as AC-040a.

## AC-041: Recovery tooling unreachable from runtime SDK
**Requirement ref:** REQ-037
**Test id:** T-041
**Assertion:** The Python and TypeScript runtime SDK public top-level surface (default + named exports excluding `_`-prefixed names and type-only exports) contains zero of the recovery-verb names enumerated by REQ-054.
**Measurement:** Per binding: enumerate the public top-level surface using the binding's documented introspection (`dir(fathomdb)` minus `_`-prefixed for Python; `Object.keys(require('fathomdb'))` for TS); assert empty intersection with the canonical recovery-verb set.
**Fixture:** REQ-054 canonical recovery-verb list.

## AC-042: Source-ref blast-radius enumeration exact
**Requirement ref:** REQ-038
**Test id:** T-042
**Assertion:** `fathomdb doctor trace --source-ref <id>` returns exactly the canonical-row id set produced by `<id>` — no extra rows, no missing rows.
**Measurement:** Seed sources S1 (10 rows), S2 (15 rows); run `trace --source-ref S1`; assert returned row-id set == S1's 10 row ids exactly.
**Fixture:** two-source-trace fixture.

## AC-043a: `check-integrity` produces structured report with three sections
**Requirement ref:** REQ-039
**Test id:** T-043a
**Assertion:** `fathomdb doctor check-integrity` JSON output contains exactly the top-level keys `physical`, `logical`, `semantic`.
**Measurement:** Parse output as JSON; assert key set equality.
**Fixture:** healthy-seeded DB.

## AC-043b: `check-integrity` populates each section
**Requirement ref:** REQ-039
**Test id:** T-043b
**Assertion:** Each top-level section in AC-043a holds either a finding list (possibly empty) or an explicit `clean: true` marker.
**Measurement:** Parse output; per section, assert either `findings: [...]` present or `clean: true` present.
**Fixture:** as AC-043a.

## AC-043c: `check-integrity --full` findings carry stable report fields
**Requirement ref:** REQ-039
**Test id:** T-043c
**Assertion:** A `doctor check-integrity --full` finding record includes `code`, `doc_anchor`, `stage`, `locator`, and `detail`, and the emitted `code` set may include doctor-only `E_CORRUPT_INTEGRITY_CHECK`.
**Measurement:** Run `fathomdb doctor check-integrity --full --json` against a fixture with deterministic page damage; parse the finding record(s); assert each emitted finding includes the five fields; assert at least one emitted finding has `code == E_CORRUPT_INTEGRITY_CHECK`; assert the code is surfaced without requiring a corresponding `Engine.open` `CorruptionKind`.
**Fixture:** page-damage integrity fixture (test-plan.md fixture spec — pending).

## AC-044: Physical recovery rebuilds projections from canonical state
**Requirement ref:** REQ-040
**Test id:** T-044
**Assertion:** Physical recovery from a DB whose FTS5 + vec0 shadow tables have been corrupted with a P-AC044-SENTINEL-LEN random per-test sentinel produces correct FTS5 + vector results AND post-recovery shadow-table page bytes contain zero occurrences of the sentinel.
**Measurement:** Seed DB; corrupt shadow tables with 16-byte random sentinel; run physical recovery; assert correct query results; grep raw shadow-table pages for sentinel; assert zero matches.
**Fixture:** sentinel-corruption fixture.

## AC-045: Single-file deploy
**Requirement ref:** REQ-041
**Test id:** T-045
**Assertion:** A fresh container with only the fathomdb binary + one `.sqlite` path on disk + network egress denied performs open + write + search + close end-to-end with exit 0 and creates no files outside the documented allow-list (DB + .lock + WAL + SHM + optional .journal).
**Measurement:** Per AC-002 file-system snapshot; per AC-037 network-egress harness; run end-to-end script; assert exit 0; assert allow-list-only files created.
**Fixture:** fresh-container fixture.

## Upgrade / compatibility

## AC-046a: Auto schema migration applied at open
**Requirement ref:** REQ-042
**Test id:** T-046a
**Assertion:** Opening a DB at schema version N when the engine supports N+P-AC046-K applies all P-AC046-K migrations transparently and post-open `PRAGMA user_version` reads N+P-AC046-K.
**Measurement:** Use the `n-to-nplusk` migration fixture; open with current engine; assert `PRAGMA user_version` == N + P-AC046-K.
**Fixture:** n-to-nplusk migration fixture.

## AC-046b: Migration emits per-step duration event on success
**Requirement ref:** REQ-042
**Test id:** T-046b
**Assertion:** A successful migration emits one structured event per applied step containing `step_id` and `duration_ms` fields.
**Measurement:** Open DB requiring P-AC046-K migrations; capture migration events; assert exactly P-AC046-K events each with both fields populated.
**Fixture:** as AC-046a.

## AC-046c: Migration emits per-step duration event on failure
**Requirement ref:** REQ-042
**Test id:** T-046c
**Assertion:** A migration that fails mid-step emits a structured event for the failed step with `failed: true` and `duration_ms` populated, and the open call returns a typed `MigrationError`.
**Measurement:** Open DB through poison-migration fixture; assert typed exception; assert event captured with both fields.
**Fixture:** poison-migration fixture (test-plan.md fixture spec — pending).

## AC-047: Hard-error on 0.5.x-shaped DB
**Requirement ref:** REQ-043
**Test id:** T-047
**Assertion:** Opening a checked-in 0.5.x-shaped DB fixture with the 0.6.0 engine raises typed `IncompatibleSchemaVersion` whose message contains the seen schema-version string, before any read or write proceeds.
**Measurement:** Use checked-in 0.5.x DB fixture; attempt `Engine.open`; assert typed exception; assert message contains the version string.
**Fixture:** v0.5.x DB fixture (committed to test corpus).

## AC-048: Hard-error on embedder mismatch at re-open (identity)
**Requirement ref:** REQ-044
**Test id:** T-048
**Assertion:** Re-opening a store with an embedder whose identity differs from the stored profile raises typed `EmbedderIdentityMismatch` naming both stored and supplied identities, before any read or write proceeds. (Dimension mismatch covered by AC-048b; call-boundary by AC-030c.)
**Measurement:** Open with embedder A (id=X); close. Reopen with embedder B (id=Y); assert typed exception with `stored: X`, `supplied: Y` populated.
**Fixture:** identity-swap fixture.

## AC-048b: Hard-error on embedder mismatch at re-open (dimension)
**Requirement ref:** REQ-044
**Test id:** T-048b
**Assertion:** Re-opening with an embedder whose dimension differs from the stored profile raises typed `EmbedderDimensionMismatch` naming both dimensions, before any read or write proceeds.
**Measurement:** Open with embedder A (id=X, dim=768); close. Reopen with embedder A' (id=X, dim=384); assert typed exception with `stored: 768`, `supplied: 384`.
**Fixture:** dim-swap fixture.

## AC-049: Schema-migration accretion guard
**Requirement ref:** REQ-045
**Test id:** T-049
**Assertion:** A CI linter parses every post-v1 migration file and rejects any migration that adds a table or column without naming a removed table/column or without containing the exact comment marker `-- MIGRATION-ACCRETION-EXEMPTION: <reason>`.
**Measurement:** Run linter against actual repo migrations; assert exit 0. Add a fixture migration violating the rule; assert linter exits non-zero naming the offender.
**Fixture:** accretion-violator fixture migration.

## AC-050a: No 0.5.x → 0.6.0 deprecation shims (AST-scoped)
**Requirement ref:** REQ-046a
**Test id:** T-050a
**Assertion:** AST analysis (Rust: rust-analyzer / syn pass; Python: ast module; TypeScript: ts-morph) over `crates/`, `python/`, `ts/` source code finds zero `legacy_*` modules, zero `compat_v0_5*` features, zero `#[allow(deprecated)]` attributes in crate roots, zero re-route stubs from 0.5.x verb names. (Comments and docs are excluded from the scan to avoid false positives.)
**Measurement:** Run AST scanner; assert zero matches in code-only scope.
**Fixture:** AST scanner script (test-plan.md fixture spec — pending).

## AC-050b: Within-0.6.x changelog discipline
**Requirement ref:** REQ-046b
**Test id:** T-050b
**Assertion:** The release-checklist script rejects any release whose changelog contains a `Deprecated` section that does not list every deprecated item also under `Removed` for the same release.
**Measurement:** Run release-checklist against synthetic changelog with deprecation-but-no-removal; assert non-zero exit + named violation. Run against valid pair; assert exit 0.
**Fixture:** synthetic-changelog fixtures.

## AC-050c: Within-0.6.x removal scenario end-to-end
**Requirement ref:** REQ-046b
**Test id:** T-050c
**Assertion:** A within-0.6.x release that removes a previously-public API documents the removal in the same release where it was last present (no soft-removal-then-hard-removal pattern).
**Measurement:** Release-checklist scans the release's diff for removed public API symbols; for each, asserts the removed symbol's removal is announced in the same release's changelog `Removed` section.
**Fixture:** removal-detect linter (test-plan.md fixture spec — pending).

## Supply chain

## AC-051a: Cargo version-skew detected at resolve time
**Requirement ref:** REQ-047
**Test id:** T-051a
**Assertion:** A Cargo.toml requesting `fathomdb = X` and `fathomdb-embedder = Y` whose `fathomdb-embedder-api` ranges do not overlap fails `cargo update` with a resolver error.
**Measurement:** Construct fixture Cargo.toml; run `cargo update`; assert non-zero exit naming the conflict.
**Fixture:** cargo-skew fixture.

## AC-051b: Pip version-skew detected at resolve time
**Requirement ref:** REQ-047
**Test id:** T-051b
**Assertion:** A pip constraint file requesting `fathomdb==X` and `fathomdb-embedder==Y` whose transitive `fathomdb-embedder-api` ranges do not overlap fails `pip install` with a resolver error.
**Measurement:** Construct fixture constraint file; run `pip install -c constraints.txt fathomdb fathomdb-embedder`; assert non-zero exit.
**Fixture:** pip-skew fixture.

## AC-052: Co-tagged sibling releases
**Requirement ref:** REQ-048
**Test id:** T-052
**Assertion:** For every published release in the registry set, the three sibling packages `fathomdb`, `fathomdb-embedder`, `fathomdb-embedder-api` exist at the same version.
**Measurement:** Query crates.io / PyPI for all releases (or last 5, whichever is fewer); assert all three packages present at each version.
**Fixture:** registry query script.

## AC-053: Single source of truth for version
**Requirement ref:** REQ-049
**Test id:** T-053
**Assertion:** A pre-publish version-consistency check rejects any release where `Cargo.toml` workspace version and `python/pyproject.toml` version disagree.
**Measurement:** Run version-consistency check against synthetic mismatch; assert non-zero exit + named files. Run against match; assert exit 0.
**Fixture:** version-consistency fixtures.

## AC-054: Atomic multi-registry publish
**Requirement ref:** REQ-050
**Test id:** T-054
**Assertion:** The release-finalize script (named in `release-policy.md`) refuses to mark a release done while any one of the configured registry publishes (PyPI, crates.io, npm, GitHub Release) is in failed state.
**Measurement:** Inject a publish failure on one registry in a release-dry-run; assert release-finalize refuses to mark complete; assert a recorded failed-publish artifact exists.
**Fixture:** dry-run-with-injected-failure (test-plan.md fixture spec — pending; release-finalize script name pending in release-policy.md).

## AC-055: `sqlite-vec` validated at open with vector rows present
**Requirement ref:** REQ-051
**Test id:** T-055
**Assertion:** Opening a DB containing ≥ 1 vector row with `sqlite-vec` extension unavailable raises typed `VectorExtensionUnavailable` at `Engine.open` and aborts open before any read or write.
**Measurement:** Seed DB with 1 vector row; close; remove `sqlite-vec` shared library from load path; reopen; assert typed exception at open call (not at first vector query).
**Fixture:** vec-extension-removal fixture.

## AC-056: Registry-installed wheel is the release gate
**Requirement ref:** REQ-052
**Test id:** T-056
**Assertion:** The release-checklist script requires evidence (a recorded artifact path) of `pip install fathomdb==<version>` from PyPI in a fresh venv followed by an end-to-end open + write + search + close + process-exit script returning success, before marking the release done.
**Measurement:** Inspect release-checklist script source; assert it contains the install-from-registry step + the end-to-end smoke step + the recorded-artifact check; remove the smoke step in a fixture; assert release-checklist refuses to mark done.
**Fixture:** checklist-bypass-attempt fixture.

## Public surface

## AC-057a: Five-verb application runtime SDK surface
**Requirement ref:** REQ-053
**Test id:** T-057a
**Assertion:** The Python and TypeScript runtime SDK public top-level surface (defined as: names returned by `dir(fathomdb)` minus `_`-prefixed minus type-only exports for Python; default + named exports minus type-only for TS) is exactly the canonical five-verb set in bindings-idiomatic casing: `Engine.open`, `admin.configure`, `write`, `search`, `close`.
**Measurement:** Per binding: enumerate per the surface definition; assert set equality with the canonical five.
**Fixture:** binding-introspection fixture.

## AC-058: Recovery verbs CLI-reachable
**Requirement ref:** REQ-054
**Test id:** T-058
**Assertion:** The lossy recovery surface is reachable only via `fathomdb recover --accept-data-loss ...`; `fathomdb recover --help` documents every 0.6.0 recovery sub-flag in `{--truncate-wal, --rebuild-vec0, --rebuild-projections, --excise-source, --purge-logical-id, --restore-logical-id}`.
**Measurement:** Invoke `fathomdb recover --help`; assert each sub-flag name appears exactly once in the help output and that `--accept-data-loss` is documented on the root command.
**Fixture:** built CLI binary.

## AC-059a: `projection_cursor` exposed on read tx; monotonic non-decreasing
**Requirement ref:** REQ-055
**Test id:** T-059a
**Assertion:** Successive read-tx `projection_cursor` values across 1,000 sequential read-tx (with interleaved writes from a sibling thread) are monotonic non-decreasing.
**Measurement:** Run 1,000 sequential read-tx with interleaved writer thread; collect cursor values; assert `cursor[i+1] >= cursor[i]` for all i.
**Fixture:** interleaved-write-cursor fixture.

## AC-059b: Write commit returns write cursor satisfiable by `projection_cursor`
**Requirement ref:** REQ-055
**Test id:** T-059b
**Assertion:** A write commit returns a monotonic write cursor `c_w` such that the write's projection becomes queryable at the moment a read-tx exposing `projection_cursor >= c_w` is observable.
**Measurement:** Issue write W; capture `c_w`; poll read-tx until `c_r >= c_w`; immediately query for W's projection; assert present.
**Fixture:** write-cursor-projection fixture.

## AC-060a: Engine errors as typed language-idiomatic exceptions
**Requirement ref:** REQ-056
**Test id:** T-060a
**Assertion:** Every variant in the variant table of `ADR-0.6.0-error-taxonomy` § Decision maps to a distinct typed exception class in Python and a distinct typed error class in TypeScript; clients dispatch on the typed class without parsing error message strings.
**Measurement:** Enumerate variants from the ADR variant table; per variant, trigger via fixture; per binding: assert `except <SpecificError>` (Python) / `instanceof <SpecificError>` (TS) catches it; assert no message-string parsing required to distinguish.
**Fixture:** error-taxonomy-trigger suite (test-plan.md fixture spec — pending — one trigger per variant).

## AC-060b: JSON-Schema validation fires save-time, pre-commit; no open-time re-validation
**Requirement ref:** REQ-056
**Test id:** T-060b
**Assertion:** A `PreparedWrite::OpStore` whose payload fails its `schema_id`'s JSON Schema is rejected save-time with `SchemaValidationError` BEFORE any row is written or committed; the writer transaction is not opened, no partial state is observable post-rejection. Re-opening the database with `Engine.open` on a DB containing historical op-store rows whose payloads no longer satisfy the current `schema_id`'s schema (e.g. schema tightened in-repo between releases) does NOT trigger validation; open succeeds.
**Measurement:** (1) Submit `PreparedWrite::OpStore` with payload violating its schema; assert `SchemaValidationError` raised; assert sqlite tx counter unchanged + zero rows added to op-store table. (2) Seed DB with historical op-store row, tighten in-repo schema for that `schema_id` so the row would now fail, restart engine via `Engine.open`; assert open succeeds without error and no validation runs.
**Fixture:** json-schema-validation-cadence fixture (save-time-reject + open-time-skip).

## AC-061a: `append_only_log` writes preserve authoritative history
**Requirement ref:** REQ-057
**Test id:** T-061a
**Assertion:** Two accepted writes to the same `append_only_log` collection and `record_key` produce two distinct authoritative rows in `operational_mutations`; neither write overwrites the earlier row.
**Measurement:** Declare an `append_only_log` collection in fixture metadata; submit two `PreparedWrite::OpStore` writes with the same logical key and distinct payloads; query `operational_mutations`; assert row count increases by 2 and both payloads remain present in commit order.
**Fixture:** op-store-append-log fixture (test-plan.md fixture spec — pending).

## AC-061b: `latest_state` stores one authoritative current row per key
**Requirement ref:** REQ-057
**Test id:** T-061b
**Assertion:** Two accepted writes to the same `latest_state` collection and `record_key` leave exactly one row in `operational_state` for that key, and that row's payload equals the second write's payload.
**Measurement:** Declare a `latest_state` collection; submit two writes with the same key and distinct payloads; query `operational_state`; assert exactly one row for that key and payload equality with the later write.
**Fixture:** op-store-latest-state fixture (test-plan.md fixture spec — pending).

## AC-061c: No derived `operational_current` table exists
**Requirement ref:** REQ-057
**Test id:** T-061c
**Assertion:** A migrated 0.6.0 database schema contains `operational_collections`, `operational_mutations`, and `operational_state`, and contains no table named `operational_current`.
**Measurement:** Open a fresh 0.6.0 DB and inspect `sqlite_schema`; assert the three accepted table names exist and `operational_current` does not.
**Fixture:** fresh-migrated-db fixture.

## AC-062: Collection registry schema exposes the accepted narrow lifecycle
**Requirement ref:** REQ-058
**Test id:** T-062
**Assertion:** `operational_collections` exposes exactly the lifecycle-bearing columns `name`, `kind`, `schema_json`, `retention_json`, `format_version`, `created_at` and exposes no `disabled_at` or equivalent status / rename column.
**Measurement:** Inspect `PRAGMA table_info(operational_collections)`; assert the documented columns are present and assert absence of `disabled_at`, `renamed_from`, `retired_at`, and `status`.
**Fixture:** fresh-migrated-db fixture.

## AC-063a: Exhausted projection failure is recorded durably
**Requirement ref:** REQ-059
**Test id:** T-063a
**Assertion:** A projection batch that exhausts the fixed retry policy records exactly one durable failure row in the `projection_failures` op-store collection and leaves the corresponding vector projection absent.
**Measurement:** Use a deterministic failing embedder fixture; submit one vector-producing write; wait for retries to exhaust; query `operational_mutations` for collection `projection_failures`; assert exactly one new failure row for the batch and assert no vector row materialized for that batch's canonical row.
**Fixture:** projection-failure fixture (test-plan.md fixture spec — pending).

## AC-063b: Restart does not silently clear terminal projection failures
**Requirement ref:** REQ-059
**Test id:** T-063b
**Assertion:** After AC-063a, closing and reopening the engine leaves the durable `projection_failures` row present and does not materialize the missing vector projection before any explicit regenerate workflow is invoked.
**Measurement:** Produce AC-063a failure; record failure-row identity; close and reopen; assert the same failure row remains in `operational_mutations`; query for the vector projection before any recovery command; assert still absent.
**Fixture:** as AC-063a.

## AC-063c: `recover --rebuild-projections` performs the explicit regenerate workflow
**Requirement ref:** REQ-059
**Test id:** T-063c
**Assertion:** Running `fathomdb recover --accept-data-loss --rebuild-projections` against the AC-063a fixture materializes the missing projection from canonical rows without requiring a second application write.
**Measurement:** Produce AC-063a failure; run the documented recovery command; reopen if required by implementation; query for the previously-missing vector projection; assert present and queryable.
**Fixture:** as AC-063a + built CLI binary.

---

## Coverage trace

Every REQ in `requirements.md` has ≥1 AC:

| REQ | AC(s) |
|---|---|
| REQ-001 | AC-001 |
| REQ-002 | AC-002, AC-003a/b/c/d |
| REQ-003 | AC-004a/b/c |
| REQ-004 | AC-005a/b |
| REQ-005 | AC-006 |
| REQ-006a | AC-007a/b |
| REQ-006b | AC-008 |
| REQ-007 | AC-009 |
| REQ-008 | AC-010 |
| REQ-009a | AC-011a |
| REQ-009b | AC-011b |
| REQ-010 | AC-012 |
| REQ-011 | AC-013 |
| REQ-012 | AC-014 |
| REQ-013 | AC-015 |
| REQ-014 | AC-016 |
| REQ-015 | AC-017 |
| REQ-016 | AC-018 |
| REQ-017 | AC-019 |
| REQ-018 | AC-020 |
| REQ-019 | AC-021 |
| REQ-020a | AC-022a/b |
| REQ-020b | AC-022c |
| REQ-021 | AC-023a/b |
| REQ-022a | AC-024a |
| REQ-022b | AC-024b |
| REQ-023 | AC-025 |
| REQ-024 | AC-026 |
| REQ-025a | AC-027a |
| REQ-025b | AC-027b |
| REQ-025c | AC-027c/d |
| REQ-026 | AC-028a/b/c |
| REQ-027 | AC-029 |
| REQ-028a | AC-030a |
| REQ-028b | AC-030b |
| REQ-028c | AC-030c |
| REQ-029 | AC-031 |
| REQ-030 | AC-032a/b |
| REQ-031 | AC-033 |
| REQ-031b | AC-034a/b/c |
| REQ-031c | AC-035 |
| REQ-031d | AC-035a/b/c/d |
| REQ-032 | AC-036 |
| REQ-033 | AC-037 |
| REQ-034 | AC-038 |
| REQ-035 | AC-039a/b |
| REQ-036 | AC-040a/b |
| REQ-037 | AC-041 |
| REQ-038 | AC-042 |
| REQ-039 | AC-043a/b/c |
| REQ-040 | AC-044 |
| REQ-041 | AC-045 |
| REQ-042 | AC-046a/b/c |
| REQ-043 | AC-047 |
| REQ-044 | AC-048, AC-048b |
| REQ-045 | AC-049 |
| REQ-046a | AC-050a |
| REQ-046b | AC-050b/c |
| REQ-047 | AC-051a/b |
| REQ-048 | AC-052 |
| REQ-049 | AC-053 |
| REQ-050 | AC-054 |
| REQ-051 | AC-055 |
| REQ-052 | AC-056 |
| REQ-053 | AC-057a |
| REQ-054 | AC-058 |
| REQ-055 | AC-059a/b |
| REQ-056 | AC-060a/b |
| REQ-057 | AC-061a/b/c |
| REQ-058 | AC-062 |
| REQ-059 | AC-063a/b/c |

## Lock-blocking dependencies

acceptance.md OWNS every numerical threshold and tolerance via the
**Parameter table** above. Acceptance.md does not block on test-plan.md
for thresholds.

acceptance.md does block on test-plan.md for **fixture corpora and
harnesses** that an AC's measurement protocol invokes — these are
build-once test artifacts, not threshold decisions:

| Test-plan.md owes | Used by AC |
|---|---|
| 1 M chunk-row corpus + FTS5 + `vec0` indexes | AC-012, AC-013, AC-019 |
| 1 GB seeded DB | AC-035 |
| Open-path corruption matrix (4 fixtures: WAL replay, header probe, schema probe, embedder-profile corruption) | AC-035a/b/c |
| Power-cut harness (kill -9 mid-commit timing strategy + reopen loop) | AC-034a, AC-034b |
| OS-crash harness (VM image + sysrq trigger with sync barrier preserved) | AC-034c |
| Shadow-table corruption injection tool | AC-006, AC-027a/b/c/d, AC-044 |
| Page-corruption tool (for SQLite-internal events) | AC-006 |
| Page-damage integrity fixture for `doctor check-integrity --full` | AC-043c |
| Deterministic-slow CTE fixture (≥ 200 ms guaranteed) + fast / slow pair | AC-007a, AC-007b |
| Poison-fixture (deterministic op failure) | AC-003d, AC-009 |
| Mixed-retrieval stress workload generator | AC-019 |
| Interactive read-mix definition (per-query-type ratios) | AC-020 |
| Compressed-runtime write fixture (10k writes/sec × 14 min harness) | AC-033 |
| Vector-100-query suite + FTS-100-query suite | AC-027b/d |
| AST scanner script (Rust + Python + TS code-only scope) | AC-050a |
| Removal-detect linter | AC-050c |
| Cargo-skew + pip-skew constraint fixtures | AC-051a/b |
| Synthetic-changelog fixtures | AC-050b |
| netns-deny-egress + bpftrace harnesses | AC-036, AC-037 |

Test-plan.md does NOT decide thresholds. If a fixture / harness
generates a number (e.g. baseline_p99 in AC-019), that number is a
*measured* value, not a *threshold* — thresholds are compared against
measurements per the parameter table.
