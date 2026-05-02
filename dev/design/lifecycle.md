---
title: Lifecycle Subsystem Design
date: 2026-05-01
target_release: 0.6.0
desc: Lifecycle feedback phases, host-routed diagnostics, counters, and profiling ownership
blast_radius: observability surfaces; REQ-001..REQ-007; AC-001..AC-009; bindings subscriber adapters; engine feedback emission sites
status: draft
---

# Lifecycle Design

This file owns the 0.6.0 observability surface that tells an operator what the
engine is doing while it is doing it.

The lifecycle subsystem is not one monolithic stream. It owns three distinct
but cooperating surfaces:

| Surface | Purpose | Primary consumer |
|---|---|---|
| Response-cycle feedback | Liveness and phase attribution for one public operation | application code, operator UX |
| Structured diagnostics | Engine and SQLite-originated events routed through the host subscriber | operators, debugging, incident triage |
| Pull / opt-in telemetry | Cumulative counters and per-statement profile records | operators, capacity planning, query tuning |

These surfaces must not be collapsed into one free-form log stream. A caller
must be able to consume lifecycle phase, diagnostics, and telemetry without
parsing English text.

## Ownership boundaries

This file owns:

- lifecycle phase semantics and the response-cycle event contract
- slow-statement and heartbeat semantics
- diagnostic routing through the host subscriber
- the public counter snapshot shape
- the public per-statement profile-record shape
- the stress-failure context payload required by REQ-007

This file does not own:

- `Engine.open` / `Engine.close` resource lifetime and shutdown ordering
  (`design/engine.md`)
- migration-step payload schema and per-step migration durations
  (`design/migrations.md`)
- cross-language subscriber registration protocol (`design/bindings.md`)
- CLI machine-readable verb output (`design/recovery.md` and
  `interfaces/cli.md`)

The migration-progress ownership split is intentional: lifecycle owns the phase
envelope and subscriber routing; `design/migrations.md` owns the migration
step-specific payload because that payload is about schema-transition semantics,
not lifecycle taxonomy.

## Response-cycle feedback

Response-cycle feedback is the stable surface for answering "is this operation
alive, and if it is delayed, what phase is it in?"

### Phase enum

The lifecycle phase enum in 0.6.0 has exactly five values:

- `Started`
- `Slow`
- `Heartbeat`
- `Finished`
- `Failed`

`AC-001` tests the non-slow members `{Started, Heartbeat, Finished, Failed}`;
`AC-008` separately locks the `Slow` transition. The full 0.6.0 lifecycle enum
still includes all five values above.

### Phase semantics

- `Started` is emitted exactly once when the observed operation is accepted and
  before the engine begins the operation's substantive work.
- `Slow` is emitted when the operation crosses the configured slow threshold.
  It is an advisory transition, not a terminal state.
- `Heartbeat` is emitted only while the operation is still in progress. It is
  periodic liveness evidence, not a new phase of work.
- `Finished` is emitted exactly once on successful completion and is terminal.
- `Failed` is emitted exactly once on operation failure and is terminal.

`Finished` and `Failed` are mutually exclusive. No response-cycle event may be
emitted after a terminal phase for the same operation.

### Public event contract

The only response-cycle field shape that 0.6.0 standardizes here is the typed
`phase` field required by REQ-001 / AC-001 / AC-008.

Producing subsystems may attach additional structured operation identity or
timing context, but lifecycle does not add more required public response-cycle
fields in 0.6.0. The exact non-phase envelope for `open`, `write`, `search`,
admin, recovery, or migration-originated events remains owned by the producing
surface or binding contract that emits it.

### Slow and heartbeat policy

The public slow threshold is runtime-configurable. The 0.6.0 default is
`100 ms` per REQ-006a / AC-007a.

The stable contract is:

- if measured wall-clock duration exceeds the configured threshold, a slow
  signal must surface and identify the statement that crossed the threshold
- that slow signal must contribute to the lifecycle `Slow` transition
- setting the threshold at runtime changes subsequent detection behavior
  without restart

The engine may use additional internal evidence such as statement counters to
enrich diagnostics, but those heuristics must not suppress threshold-based slow
emission.

Crossing the threshold therefore produces two correlated observability facts:

- a slow-statement signal for the statement-level diagnostic surface
- at least one lifecycle event whose `phase` is `Slow` while the operation is
  still in flight

Heartbeat cadence is configurable as part of the feedback configuration
surface. The lifecycle contract requires periodic liveness while an operation
remains in flight; per-binding spelling and default interval are owned by the
interface and binding surfaces rather than by this file.

### Terminal-delivery posture

Lifecycle feedback is guaranteed for in-process observed operations, not for
process-abort scenarios such as `SIGKILL`. A hard process abort may prevent a
terminal `Finished` or `Failed` event from being emitted; this does not weaken
the engine-lifetime cleanup invariants owned by `design/engine.md`.

## Host-routed diagnostics

REQ-002 and REQ-005 require one routing rule: the host owns subscriber
configuration, and both fathomdb-originated diagnostics and SQLite-originated
diagnostics use that same route.

### No private sink

When no subscriber is registered, the engine writes nothing of its own:

- no log files
- no private telemetry spool
- no best-effort stderr fallback masquerading as a stable API

This is load-bearing for AC-002. The engine may retain internal state needed to
serve snapshots and profiles, but it must not create a side-channel output
artifact on its own.

### Diagnostic source and category

Structured diagnostics carry a typed `source` tag and typed `category` tag.

Stable `source` values:

- `Engine`
- `SqliteInternal`

Stable engine-source categories used by AC-003*:

- `writer`
- `search`
- `admin`
- `error`

Stable SQLite-internal categories used by AC-006:

- `corruption`
- `recovery`
- `io`

`SqliteInternal` events are not allowed to bypass the host subscriber path.
They are routed through the same observer / adapter channel as engine events,
with source preserved so operators can distinguish origin without message
parsing.

## Counter snapshot

The public counter snapshot is a pull surface: read on demand, cumulative since
engine open, and non-perturbing to the counters themselves.

### Public key set

The 0.6.0 public snapshot exposes exactly these keys:

- `queries`
- `writes`
- `write_rows`
- `errors_by_code`
- `admin_ops`
- `cache_hit`
- `cache_miss`

AC-004a locks this set. Internal counters beyond these may exist, but they are
not part of the 0.6.0 public contract unless and until they are accepted in
requirements and acceptance.

### Semantics

- `queries`, `writes`, `write_rows`, and `admin_ops` are exact cumulative
  counts for accepted work since engine open.
- `errors_by_code` is a cumulative mapping keyed by stable machine-readable
  error code, not by rendered message text.
- `cache_hit` and `cache_miss` are cumulative SQLite cache counters surfaced in
  the snapshot's public naming scheme.
- Reading a snapshot must not itself increment any public counter.

The lifecycle subsystem owns the public shape and semantics above. The exact
internal aggregation mechanics across writer and reader connections are an
implementation concern so long as the snapshot remains cumulative and
machine-readable.

### Public access-path boundary

Lifecycle owns the snapshot payload shape, not the binding-visible method name.
The read operation is an `Engine`-attached instrumentation call in 0.6.0, not
a sixth top-level SDK verb. `interfaces/{rust,python,typescript}.md` own the
exact instance-method spelling.

## Per-statement profiling

Per-statement profiling is a separate opt-in surface from lifecycle feedback.
It answers "what did this statement cost?" rather than "is this operation
alive?" It is not implied by lifecycle subscription alone.

### Toggle and scope

- profiling is runtime-toggleable without engine rebuild
- profiling may be disabled on a running engine
- when disabled, normal lifecycle feedback still exists

### Public record shape

Each public profile record carries these typed numeric fields:

- `wall_clock_ms`
- `step_count`
- `cache_delta`

AC-005b locks this record shape. Additional internal counters may be collected,
but the public 0.6.0 surface must provide at least these fields and must not
require message parsing to obtain them.

Profile records may feed operator surfaces such as subscriber-delivered
diagnostics or CLI dump tooling, but those transport details are owned outside
this file.

The runtime profiling toggle is likewise an `Engine`-attached instrumentation
call, not a top-level SDK verb. `interfaces/{rust,python,typescript}.md` own
the concrete method name and parameter spelling.

### Runtime reconfiguration ownership

The following controls exist in 0.6.0 but are not named here:

- slow-threshold setter on the engine instrumentation surface
- subscriber/feedback registration surface that may carry heartbeat cadence

This file owns the semantics of those controls. The interface docs own the
binding-visible names.

## Stress-failure context

Stress / robustness failures require a dedicated structured payload rather than
generic `error` diagnostics with ad hoc metadata.

The exact public field set is the AC-009 schema, and lifecycle owns the public
payload type that carries it. Those required fields are:

- `thread_group_id`
- `op_kind`
- `last_error_chain`
- `projection_state`

REQ-007 delegates field-set enumeration to acceptance, and AC-009 fixes that
enumeration above. Lifecycle owns the fact that this payload is structured,
subscriber-routable observability data rather than ad hoc free-text metadata.

## Relation to other subsystem events

Lifecycle phase attribution and subsystem-specific payloads can coexist on the
same host route without merging ownership.

Examples:

- a migration step event may be routed through the host subscriber while its
  step payload remains owned by `design/migrations.md`
- a corruption-on-open failure may emit lifecycle `Failed` while the structured
  error payload remains owned by `design/errors.md`
- a slow query may emit both a lifecycle `Slow` event and a profile record

This separation prevents owner drift: lifecycle owns liveness / routing /
public observability schema, while producing subsystems own their domain
payloads.

## Traceability

REQ coverage owned here:

- REQ-001 — lifecycle phase attribution
- REQ-002 — host logging integration
- REQ-003 — cumulative engine counters
- REQ-004 — per-statement profiling opt-in
- REQ-005 — SQLite-internal events surfaced
- REQ-006a — slow-statement signal
- REQ-006b — slow signal feeds lifecycle attribution
- REQ-007 — stress-failure context sufficiency

Primary AC coverage owned here:

- AC-001
- AC-002
- AC-003a/b/c/d
- AC-004a/b/c
- AC-005a/b
- AC-006
- AC-007a/b
- AC-008
- AC-009
