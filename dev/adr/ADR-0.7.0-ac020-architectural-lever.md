---
title: ADR-0.7.0-ac020-architectural-lever
date: 2026-05-25
target_release: 0.7.0
desc: Architectural lever for AC-020 (single-reader concurrency ratio). Compares PCACHE2 / WAL2 / reader-writer pool split / vendor-SQLite swap; recommends one; encodes diagnostic-first stop-rule.
blast_radius: src/rust/crates/fathomdb-engine/src/lib.rs (reader pool + sqlite handle layer); src/rust/crates/fathomdb-engine/Cargo.toml (potentially: libsqlite3-sys version / vendor); src/rust/crates/fathomdb-engine/tests/perf_gates.rs (AC-020 bound); dev/design/recovery.md (if WAL2 / vendor swap); dev/notes/performance-whitepaper-notes.md (closure narrative)
status: draft, HITL-required
---

# ADR-0.7.0 — AC-020 architectural lever

**Status:** draft, HITL-required.

This ADR picks the **one** architectural lever that closes AC-020
in 0.7.0. AC-020's read-path concurrency ratio (currently 3.530×
median; required ≥ 5.33× per `tests/perf_gates.rs:245`) cannot be
closed by canonical-SQLite engine-side levers alone — Pack 5 / 6 /
6.G exhausted the canonical-SQLite knob surface and falsified each
one. The residual gap is in code surfaces that canonical SQLite
does not expose (custom page-cache allocator) or that require
substantial architectural change (separate readers from writer at
the SQLite layer, or replace SQLite with a derivative).

The four candidates below are **mutually exclusive** for the 0.7.0
release. A second lever fires only if the first underdelivers AND
HITL re-opens this ADR. Stacking two structural levers in a single
release would violate `feedback_reliability_principles` (single
load-bearing change per release for any high-blast-radius surface).

## Status / context

AC-020 contract (`dev/test-plan.md` § Current Perf Attribution
L143-190; `src/rust/crates/fathomdb-engine/tests/perf_gates.rs:211`):
single-reader connection pool of 8 workers must achieve concurrent
speedup ≥ 5.33× over the sequential baseline on the AC-020 fixture
(50 rounds × 4 mixed-vector / FTS / hybrid queries per thread).

Current measurement (`dev/notes/performance-whitepaper-notes.md`
§ 2 and § 12, plus `dev/plans/runs/G3_5-cache-pressure-telemetry-
output.json`): seq 563 ms, conc 161 ms, speedup **3.530×** (Pack 6.G
close 2026-05-04, aarch64 Tegra dev host). The canonical-runner
re-measurement of AC-020 has not been taken at 0.6.1 close (only
AC-012 was re-measured); the diagnostic-first phase below
(`0.7.0-PERF-DIAG`) captures the AC-020 canonical-runner baseline
before the architectural lever fires.

The whitepaper notes
(`dev/notes/performance-whitepaper-notes.md` § 6 hypothesis
hierarchy + § 11 Pack 5 close narrative + § 12 Pack 6 + Pack 6.G
synthesis) classify the residual as one of three remaining causes:

1. `pcache1` mutex acquires on every hit-path page-fetch — canonical
   SQLite cannot eliminate without a custom page-cache allocator.
2. WAL shared-memory atomics (frame counters, page references,
   checkpoint sequence) — survive `SQLITE_THREADSAFE=2` because
   they are not threading-mode mutexes.
3. rusqlite-side internal Mutex or `ReaderPool::borrow`
   `Mutex<Vec<Connection>>` — upstream of any SQLite-side flag.

The diagnostic-first phase below disambiguates which of these
dominates at canonical scale before the architectural lever
commits to its specific blast radius.

## Options compared (four)

### Option 1 — `SQLITE_CONFIG_PCACHE2` custom page-cache allocator

**Mechanism:** install a custom `sqlite3_pcache_methods2` via
`sqlite3_config(SQLITE_CONFIG_PCACHE2, ...)` at process start
(before any `sqlite3_initialize()` call, mirroring the B.1
ordering constraint from
`dev/notes/performance-whitepaper-notes.md` § 5 / § 7.3). Custom
allocator side-steps the global `pcache1` mutex by providing
per-handle (or per-reader-pool-slot) page-cache slabs that do not
contend.

**Blast radius:** medium. Pure FFI extension on top of canonical
SQLite; no SQLite version change; no schema impact; no durability
contract change. New `pcache2` allocator implementation +
process-start init wiring + per-slot accounting code. Roughly
~400-600 LOC of unsafe FFI + safe Rust wrapping.

**Expected payoff:** high if § 6 hypothesis #1 (`pcache1` mutex
hit-path contention) is the dominant residual; medium otherwise.
The whitepaper's `cache_used` = 3.35% and `delta_miss_rate` =
0.023% from G.3.5 (page-cache lever falsified at canonical-SQLite
PRAGMA layer) means the residual is in mutex acquires, not miss
rate — exactly the surface PCACHE2 targets.

**Prerequisite diagnostic (per § 7.1 of the whitepaper, made
machine-checkable here):**

- `perf record -g --call-graph dwarf` of the AC-020 binary in
  sequential and concurrent modes, on the canonical runner.
- `perf report` aggregator must show `pcache1Fetch` + `pcache1Pin`
  - their mutex-acquire siblings growing super-linearly under
    concurrency (concurrent share ≥ 2× sequential share).

If the diagnostic falsifies this hypothesis (mutex share is in
WAL atomics or rusqlite-side instead), PCACHE2 is **skipped**
without revert; the diagnostic data points at WAL2 or R-W split
instead.

**Cross-check against do-not-retry ledger
(`dev/notes/performance-whitepaper-notes.md` § 5):** ledger
contains `sqlite3_config(SQLITE_CONFIG_MULTITHREAD)` (B.1) and
compile-time `THREADSAFE=2` (C.1) but **does not contain**
`SQLITE_CONFIG_PCACHE2`. PCACHE2 is genuinely untried and is
explicitly listed in whitepaper § 7.5 as a "per-connection
lookaside and page cache" follow-on. **Not on the ledger.**

### Option 2 — WAL2 (SQLite WAL2 patchset)

**Mechanism:** swap the bundled SQLite for the WAL2 fork of
SQLite (one of the upstream patchsets — see `sqlite.org` WAL2
branch). WAL2 maintains two WAL files alternately, allowing
readers to operate on one while writers move to the other; the
shared-memory atomics surface contention can be reduced because
readers do not have to serialize on the same WAL frame counter
the writer is incrementing.

**Blast radius:** large. Touches the durability + recovery
surface: WAL2 changes the on-disk WAL file layout (two `*-wal`
files instead of one, plus a control file). `dev/design/recovery.md`
needs an updated section explaining the multi-file WAL recovery
shape. The 0.6.0 schema version stays the same (no row-format
change) but the WAL-file invariants used by external backup tools
no longer hold. `libsqlite3-sys` must be replaced with a custom
build or a different crate.

**Expected payoff:** high if § 6 hypothesis #2 (WAL shared-memory
atomics) is the dominant residual; low otherwise. Pack 5 evidence
(C.1 REVERT with `mutex_atomic` symbols surviving `THREADSAFE=2`)
points at this hypothesis but did not isolate it from #1 — that
isolation is the DIAG-slice's job.

**Prerequisite diagnostic:**

- Same `perf record` pass as Option 1.
- WAL2 is justified only if `walShmGet` / `walReadFrame` mutex
  acquires (or their aarch64 atomic equivalents
  `__aarch64_swp4_rel` / `__aarch64_cas4_acq` per whitepaper § 6)
  show super-linear growth AND PCACHE2's prerequisite is not met.

**Cross-check against do-not-retry ledger:** ledger contains no
WAL2 entry (Pack 5 / 6 / 6.G did not try it because it was always
classified Pack 7 territory per § 13 of the Pack 5 plan and
§ 14-15 of the whitepaper). **Not on the ledger.**

### Option 3 — Reader / writer physical separation (R-W pool split)

**Mechanism:** replace the current single-database-file shape with
two SQLite handles per logical operation: a writer connection on
the canonical DB file, and a reader pool of 8 connections on a
**read-only snapshot** maintained via SQLite's `sqlite3_snapshot_*`
API or by serving readers from a checkpointed read-replica file.
Readers and writer no longer contend on the same WAL or page
cache; the only synchronization is the snapshot-handoff at
projection-runtime commit boundaries.

**Blast radius:** large. Changes the writer/reader architecture
inside `fathomdb-engine`: `lib.rs:158` (`ReaderPool` declaration)

- `lib.rs:707` (constructor) + `lib.rs:773` (pool fill) + the
  projection-runtime commit path. Touches the snapshot semantics
  that AC-059b reader-tx snapshot contract relies on
  (`dev/notes/performance-whitepaper-notes.md` § 4 KEPT entries).
  Requires a snapshot-freshness ADR (because the snapshot version
  becomes part of the user-observable correctness contract).

**Expected payoff:** medium-high if § 6 hypothesis #3
(rusqlite-side / `ReaderPool::borrow` Mutex) is the dominant
residual; the borrow Mutex disappears when readers and writer
hold structurally separate handles.

**Prerequisite diagnostic:**

- Same `perf record` pass.
- R-W split is justified only if Rust-side mutex symbols
  (`std::sync::Mutex::lock` on the `ReaderPool::borrow` path or
  rusqlite's internal Mutex) dominate over SQLite-side mutex
  symbols.

**Cross-check against do-not-retry ledger:** ledger does not
contain R-W split. Pack 5 / 6 stayed inside the
single-file-single-pool topology. The closest entry is F.0
(thread-affine readers; KEPT) which removed the borrow-pool Mutex
on the dispatch path but kept readers and writer on the same
database file. **Not on the ledger.**

### Option 4 — Vendor-SQLite swap (libSQL or sqlite3mc)

**Mechanism:** replace canonical SQLite entirely with a vendor
fork that ships different concurrency primitives. libSQL (Turso's
fork) ships per-connection page-cache allocators by default and
has a different WAL implementation. sqlite3mc adds multi-cipher
support but reuses canonical SQLite's concurrency surface — only
attractive if encryption is also needed.

**Blast radius:** very large. Replaces the SQL engine that the
entire 0.6.0 design rests on. Affects durability, recovery,
crash-safety, extension compatibility (vec0 / FTS5 are SQLite
extensions; libSQL ships these but version-skewed). Changes
deployment surface: host-system SQLite paths
(`dev/notes/performance-whitepaper-notes.md` § 8 cross-references)
are no longer relevant; commits the project to bundled-only
deployment. ADR-class because durability + recovery contracts
shift.

**Expected payoff:** high but undifferentiated. libSQL claims
better concurrency but Pack 5 / 6 evidence cannot be replayed
against it without doing the swap. Cannot be justified by
diagnostic data alone — it's a "different bottleneck shape" bet,
not a "this specific mutex" fix.

**Cross-check against do-not-retry ledger:** ledger does not
contain vendor-SQLite swap (Pack 5 / 6 stayed bundled per
constraint). **Not on the ledger.**

## Diagnostic-first stop-rule

**Precondition for any of the four levers landing in 0.7.0:** the
`0.7.0-PERF-DIAG` slice must publish a `perf record` + classified
report on the canonical runner (`ubuntu-latest` x86_64, AMD EPYC,
4-core, glibc 2.39, SQLite 3.45.x bundled). The slice's output
JSON must include:

- Sequential per-symbol cycle aggregate (top 10 frames by share).
- Concurrent per-symbol cycle aggregate (same top 10 frames + any
  frame whose concurrent share is > 2× its sequential share).
- Classification of each super-linear-grower into one of the
  three hypothesis buckets:
  - **H1 = pcache1 / page-cache mutex** (PCACHE2 territory).
  - **H2 = WAL atomics / shm protocol** (WAL2 territory).
  - **H3 = rusqlite or ReaderPool-borrow Mutex** (R-W split
    territory).
- A **single primary hypothesis** call with the cycle-share
  evidence that supports it.

The architectural-lever slice does NOT spawn until the DIAG output
JSON exists and HITL Q2 picks the lever consistent with the DIAG
primary hypothesis.

If DIAG returns no super-linear-grower (concurrent shares within
1.2× of sequential shares across the top 10 frames), AC-020 may
be **closed by re-measurement** — the canonical runner's 4-core
slice may not exhibit the contention the aarch64 12-core dev host
exhibited. In that case, this ADR is closed `status: locked,
intervention: none` and AC-074 (the revised AC-020 budget) is
asserted by the existing harness on the canonical runner without
any lever landing.

## Recommendation

> **Drafter's recommendation (HITL Q2 picks):**
>
> **Option 1 — PCACHE2** as the default lever, contingent on the
> DIAG slice confirming H1 (page-cache mutex contention) as the
> primary hypothesis.
>
> Fallback ranking if DIAG points elsewhere:
>
> - H2 dominant → Option 2 (WAL2).
> - H3 dominant → Option 3 (R-W split).
> - DIAG inconclusive AND HITL judges the cycle-share evidence
>   weighs toward H1 → Option 1 (PCACHE2; the smallest blast
>   radius among the three structural levers).
> - DIAG falsifies H1/H2/H3 (no super-linear grower) → no lever;
>   close by re-measurement per the stop-rule above.
>
> Option 4 (vendor-SQLite swap) is **last resort**: justified only
> if all three single-cause levers underdeliver and HITL re-opens
> this ADR. It is recorded here for completeness because the
> whitepaper § 13 lists it; it is not the 0.7.0-default lever.

**Rationale for PCACHE2 as default:**

- Smallest blast radius among the three structural levers (no
  on-disk format change, no schema impact, no recovery-contract
  change, no SQLite vendor change).
- Pack 5 G.3.5 telemetry already implicates `pcache1` mutex
  acquires (whitepaper § 12: residual `page_cache` 6.29% concurrent
  share from hit-path page-fetch mutex acquires).
- Recoverable: if PCACHE2 lands and AC-020 still misses the revised
  budget by < 1×stddev, HITL can stack a second knob (lookaside
  sizing already LANDED INCONCLUSIVE at G.1) without re-opening
  the whole ADR.
- Independent of the workload: PCACHE2 helps any read-heavy
  workload that exercises page-cache hit-path concurrency, not
  just AC-020.

## Rejected alternatives

**R1 — Stack two levers in 0.7.0 (PCACHE2 + WAL2; or any pair).**
Violates `feedback_reliability_principles` (one load-bearing
change per release on a high-blast-radius surface). If lever-1
underdelivers, HITL re-opens this ADR after 0.7.0 ships; lever-2
lands in 0.7.1 or 0.8.0. Stacking would also confound attribution:
if AC-020 closes with both landed, the project cannot tell which
one moved the needle, and the next AC-020-class regression has no
hypothesis hierarchy to fall back on.

**R2 — Defer to 0.8.0.** 0.8.0's anchor (knowledge-store +
retrieval for Memex per `dev/roadmap/0.8.0.md`) consumes the
retrieval surface that AC-020 gates. Deferring AC-020 either
blocks 0.8.0 or ships 0.8.0 on a perpetually-RED concurrency gate.

**R3 — Retry a do-not-retry-ledger lever
(`dev/notes/performance-whitepaper-notes.md` § 5).** Explicitly
forbidden by the planning prompt and the handoff. The § 5 levers
were falsified with clean methodology; re-trying them without a
new mechanism is a process failure. Any ledger lever that
reappears here must carry an explicit HITL override pointing at
the new mechanism — none of the four options above is a ledger
lever.

**R4 — Lift the AC-020 contract.** Loosening `1.5 /
AC020_THREADS` to a more achievable ratio is a contract change,
not a perf fix. Rejected by the same reasoning as
ADR-0.7.0-text-query-latency-gates-revised § R1: the contract is
the user-visible promise, and a project that can only meet its
contract by weakening it has not closed the gate.

**R5 — Drop AC-020 (delete the test).** Rejected; the test is the
load-bearing gate against future regressions on the read-path
concurrency surface, which is REQ-053's `search` underlay. The
project's reliability principles forbid deleting a load-bearing
gate to make CI green.

## Acceptance criterion

**AC-074 (proposed, per
`dev/plans/0.7.0-implementation.md` § Per-AC scoreboard):**
AC-020 sequential + concurrent + speedup measurements on the
canonical runner at the AC-020 fixture meet the revised envelope
recorded in `ADR-0.7.0-text-query-latency-gates-revised` § AC-020
**after** the chosen architectural lever lands. If DIAG closes
the gate without a lever, AC-074 is asserted by re-measurement
alone (no source change to the engine; the AC-020 test bound
stays unchanged).

HITL confirms AC-074's wording when locking this ADR. The slice
that closes AC-074 (`0.7.0-PERF-AC020`) does not spawn until both
this ADR and `ADR-0.7.0-text-query-latency-gates-revised` read
`status: locked`.

## Consequences

- `dev/plans/0.7.0-implementation.md` § Slice sequence orders
  PERF-DIAG before PERF-AC020 unconditionally.
- `dev/notes/performance-whitepaper-notes.md` is updated by the
  closing slice's docs commit: § 5 ledger gets one new "kept" or
  "reverted" entry per the slice's verdict; § 14 gets a closing
  narrative paragraph.
- `dev/design/recovery.md` may add a § "WAL2 layout" section
  (Option 2 only) or a § "Reader snapshot lifecycle" section
  (Option 3 only). The closing slice's prompt skeleton enumerates
  the doc updates per chosen option.
- `src/rust/crates/fathomdb-engine/tests/perf_gates.rs:245` bound
  expression is unchanged; the budget numbers in the test
  constants section update per the revised-budgets ADR.

## Citations

- HITL 2026-05-24 (release rescope; 0.7.0 → perf-only).
- HITL Q2 lock (date pending; this ADR draft is the input).
- `ADR-0.7.0-text-query-latency-gates-revised` (parallel; owns
  the numeric envelope).
- `dev/notes/performance-whitepaper-notes.md` § 2 / § 4 / § 5 /
  § 6 / § 7 / § 11 / § 12 (AC-020 evidence chain).
- `dev/plans/0.6.0-Phase-9-Pack-5-performance-diagnostics.md`
  § 13 (Pack 7 proposed work; PCACHE2 / WAL2 / vendor-swap as
  the next-tier levers).
- `dev/test-plan.md` § Current Perf Attribution L143-190.
- `src/rust/crates/fathomdb-engine/tests/perf_gates.rs:211, 245`.
- MEMORY: `feedback_reliability_principles` (one load-bearing
  change per release on a high-blast-radius surface),
  `feedback_tdd` (RED test asserting the revised budget on the
  canonical runner before the lever lands).
