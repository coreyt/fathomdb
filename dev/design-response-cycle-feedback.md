# Design: Response-Cycle Feedback For All `fathomdb` Operations

## Purpose

Define a cross-cutting response-cycle feedback system for `fathomdb` so callers
can distinguish:

- normal short operations
- slow but healthy operations
- failed or stalled operations

This design is explicitly **not** limited to repair or admin functions. It
covers:

- user-space reads / queries
- user-space writes
- admin / recovery operations
- bridge-backed operations
- future compound workflows built on top of those primitives

The main user-facing goal is simple:

- a delayed response should not be mistaken for a processing error just because
  the system is silent

## Core Correctness Rule

Timer-based response-cycle feedback is allowed to report only:

- lifecycle state
- elapsed time
- liveness

Timer-based response-cycle feedback is **not** allowed to report:

- percent complete
- rows completed
- ETA
- forward progress
- “almost done” style claims

unless those claims come from a deeper engine or workflow-specific progress
source.

This rule is what keeps timer-based feedback meaningful and correct across all
operation types, including arbitrary reads and writes.

## Problem

Today the main public call paths are synchronous and mostly silent while work is
in progress.

Examples:

- Python query execution blocks until `execute_ast(...)` returns
- Python writes block until `submit_write(...)` returns
- Python admin calls block until `check_integrity`, `rebuild`, `rebuild_missing`,
  `trace_source`, `excise_source`, or `safe_export` returns
- Go bridge-backed calls wait for one final JSON response from the admin bridge

This creates one bad user experience across all operation types:

- if a call takes longer than expected, the caller cannot easily tell whether
  the operation is:
  - still running normally
  - blocked on I/O or lock contention
  - large but healthy
  - hung
  - failed but not yet surfaced

The problem is broader than progress bars for repair.

It is about **response-cycle feedback**:

- when work starts
- when it is still running
- when it has crossed a “slow” threshold
- when it completes
- when it fails

## Summary

Implement response-cycle feedback in two layers:

1. A universal, low-overhead lifecycle layer in the language libraries
2. Optional, richer liveness signals from the engine and bridge for long SQL

The universal layer should apply to every public operation.

It should emit:

- `started`
- `slow`
- `heartbeat`
- `finished`
- `failed`

This layer should be timer-based and should not require extra database queries
on the fast path.

Then add optional richer signals where they are worth the complexity:

- cheap preflight heuristics for “likely slow”
- engine-side SQLite progress callbacks for coarse heartbeats during long SQL
- bridge protocol support for streamed events rather than only one final
  response

## Design Boundary

This design covers:

- response-cycle feedback semantics
- the public event model
- scope and non-scope
- prioritization
- the implementation approach across:
  - Rust engine
  - Rust facade / Python binding
  - Python package
  - Go bridge client and integrity tooling

This design does **not** require:

- exact percent-complete reporting for every operation
- exact time prediction for every operation
- mandatory async APIs everywhere in v1
- invasive query planner changes
- a separate job queue or background task system

## Goals

1. Give users immediate evidence that work has started.
2. Give users periodic evidence that a long operation is still alive.
3. Surface “this is taking longer than normal” consistently across all
   operation types.
4. Keep overhead near zero for fast operations.
5. Avoid adding extra database scans to every request just to estimate runtime.
6. Allow richer telemetry later without invalidating the base API.
7. Keep the semantics consistent across Rust, Python, and Go bridge-backed use.

## Non-Goals

1. Do not promise exact percent-complete for arbitrary SQL queries.
2. Do not promise accurate time-to-completion estimates for all operations.
3. Do not add a polling query for every operation in order to decide whether it
   is slow.
4. Do not require all public APIs to become async in the first iteration.
5. Do not block progress on a streaming bridge protocol before shipping basic
   lifecycle feedback.
6. Do not turn response-cycle feedback into a second observability stack with
   metrics storage, tracing backends, or distributed telemetry.

## What Will Be Covered

### Covered In This Design

- A single feedback model for all operations
- A common event schema
- Timer-based lifecycle events in language libraries
- Slow-threshold detection
- Periodic heartbeat emission while an operation is still running
- Failure and completion events
- Optional operation metadata:
  - operation category
  - operation label
  - cheap shape hints
  - duration
- Cheap “likely slow” heuristics where they are already available from request
  shape or pre-existing metadata
- Future engine-side liveness hooks for long SQL
- Future bridge streaming support

### Covered Operation Types

- queries / reads
- writes
- admin operations
- recovery / repair operations
- bridge commands
- higher-level application workflows built on those primitives

## What Will Not Be Covered

### Explicitly Out Of Scope For Initial Implementation

- exact progress percentages for normal reads and writes
- exact row-remaining counts for arbitrary SQL
- per-row or per-chunk progress callbacks from SQLite for every operation
- automatic cancellation policies based on estimated runtime
- preflight “count the whole workload” queries for every request
- a universal ETA field that claims accuracy
- persistent historical telemetry storage inside the database
- UI design beyond the event contract exposed to callers

### Deferred Work

- streaming bridge protocol
- engine-generated heartbeats via SQLite progress handlers
- historical latency models keyed by operation shape
- richer stage models for complex multi-step workflows
- explicit cancellation APIs integrated with progress reporting

## Trade-Offs That Led To The Scope

### Chosen Trade-Off: Lifecycle Feedback Over Exact Forecasting

Why:

- users mostly need to know that work is happening
- exact forecasting is expensive and often inaccurate
- timer-based lifecycle feedback solves the main UX problem immediately

Rejected:

- attempting exact runtime forecasting for all operations before shipping any
  response-cycle feedback

### Chosen Trade-Off: No Extra Query Overhead On The Fast Path

Why:

- adding count/preflight queries to every operation penalizes all calls,
  including fast ones
- many operations already expose enough cheap information from request shape
- timer-based slow detection is nearly free

Rejected:

- mandatory preflight scans for every query or write

### Chosen Trade-Off: Coarse Heartbeats Before Fine-Grained Progress

Why:

- “still alive” is much easier to implement than meaningful percent-complete
- coarse heartbeats work across all operation types
- fine-grained progress is operation-specific and often impossible for arbitrary
  SQL

Rejected:

- requiring fine-grained progress accounting before exposing any user feedback

### Chosen Trade-Off: Shared Event Model Across Languages

Why:

- callers should not learn a different mental model in Rust, Python, and Go
- a common model allows one implementation strategy per language binding
- later bridge streaming can map into the same event model

Rejected:

- ad hoc one-off status fields only for repair or only for Python

### Chosen Trade-Off: Heuristics Are Allowed, But Optional

Why:

- operation shape often gives useful hints cheaply
- exact predictions do not justify full-cost preflight scans on every request
- heuristics can improve the UX without becoming a correctness dependency

Rejected:

- no prediction hints at all
- exact prediction as a hard requirement

## Priorities

### Priority 0: Universal Lifecycle Feedback

Ship for every public operation:

- `started`
- `slow`
- `heartbeat`
- `finished`
- `failed`

This is the first priority because it solves the user-visible silence problem
without waiting for deep engine changes.

### Priority 1: Cheap Shape-Based Hints

Add optional metadata that can be computed without extra SQL, such as:

- operation class: `query`, `write`, `admin`, `recovery`
- query shape:
  - traversal present
  - vector search present
  - text search present
  - final limit
  - shape hash when available
- write shape:
  - row counts by table
  - chunk byte totals
  - vec insert count
- admin command type

These are useful for caller logs and future latency heuristics.

### Priority 2: Long-SQL Liveness Hooks

Use SQLite progress callbacks or chunked execution for operations where:

- long runtimes are common
- timer-only heartbeats are too weak
- the engine can emit meaningful “still executing” signals cheaply

Examples:

- large projection rebuilds
- large exports
- large text/vector maintenance operations

### Priority 3: Bridge Streaming

Extend bridge-backed operations from single final response to streamed events so
callers can receive:

- `started`
- `stage_changed`
- `heartbeat`
- `finished`
- `failed`

### Priority 4: Historical Runtime Heuristics

Use local, in-memory historical latency summaries keyed by operation shape to
improve “likely slow” classification.

This is valuable, but lower priority than simply providing live feedback.

## Definitions

### Response-Cycle Feedback

The stream of user-visible state changes for one operation from invocation to
completion or failure.

### Operation

Any public call that may do meaningful work and block a caller:

- query
- write
- admin operation
- bridge command
- compound workflow

### Slow Threshold

A wall-clock threshold after which the system should explicitly inform the
caller that the operation is still running and slower than the fast path.

### Heartbeat

A periodic signal that confirms the operation is still live, even if no deeper
progress metric is available.

### Forecasting

A best-effort prediction that an operation is likely to be long-running before
or shortly after it starts.

## Common Event Model

Define one cross-language event schema.

```text
OperationEvent
  operation_id: string
  phase: started | slow | heartbeat | finished | failed
  category: query | write | admin | recovery | bridge | workflow
  name: short operation name
  started_at_ms: int
  now_ms: int
  elapsed_ms: int
  message: optional human-readable status
  metadata: map<string, value>
```

### Required Fields

- `operation_id`
- `phase`
- `category`
- `started_at_ms`
- `now_ms`
- `elapsed_ms`

### Recommended Metadata

- `query_shape_hash`
- `root_kind`
- `has_traversal`
- `has_text_search`
- `has_vector_search`
- `final_limit`
- `write_counts`
- `chunk_bytes`
- `vec_insert_count`
- `admin_target`
- `bridge_command`
- `db_path_basename`

Do not expose secrets or large payload bodies in the event metadata.

## Implementation

## Layer 1: Language-Library Lifecycle Feedback

### Decision

Every public library API should accept an optional observer / callback.

Examples:

- Rust facade:
  - optional `OperationObserver`
- Python package:
  - optional `progress_callback` or `operation_observer`
- Go integrity CLI / library:
  - optional writer / callback for status events

### Behavior

For every operation:

1. emit `started` immediately
2. schedule a slow-threshold timer
3. if the call is still running when the threshold fires, emit `slow`
4. while still running, emit `heartbeat` periodically
5. emit `finished` or `failed` when done

### Threshold Defaults

Suggested initial defaults:

- `slow_threshold_ms = 500`
- `heartbeat_interval_ms = 2000`

These are policy defaults, not protocol requirements.

### Why This Works

- no extra SQL needed
- works for all operation types
- immediately solves “silence looks like failure”

### Required Correctness Guarantees

The timer-based lifecycle implementation must satisfy these guarantees.

State machine:

```text
            +---------+
            | started |
            +---------+
                 |
                 v
            +---------+
            | running |
            +---------+
             |     |
    threshold |     | terminal success
      crossed |     v
             v   +----------+
         +------+| finished |
         | slow |+----------+
         +------+
             |
             v
       +-----------+
       | heartbeat |
       +-----------+
             |
             +--------------------+
             |                    |
             | still running      | terminal failure
             v                    v
       +-----------+         +--------+
       | heartbeat |  ...    | failed |
       +-----------+         +--------+
```

Rules:

- `started` is emitted once
- `slow` is emitted at most once
- `heartbeat` may repeat only while the operation remains non-terminal
- `finished` and `failed` are mutually exclusive terminal states
- no events may occur after a terminal state

1. Exactly one terminal event per operation
   - if `started` is emitted, the operation must eventually emit exactly one of:
     - `finished`
     - `failed`
   - unless the hosting process itself terminates unexpectedly

2. No post-terminal events
   - after `finished` or `failed`, no further `slow` or `heartbeat` events may
     be emitted for that operation

3. Heartbeats are liveness-only
   - a `heartbeat` event means only:
     - the operation has not yet reached a terminal state
     - the caller-side lifecycle wrapper is still active
   - it does not mean:
     - measurable forward progress
     - successful lock acquisition
     - row-by-row advancement

4. `slow` is threshold-only
   - a `slow` event means only that the operation exceeded the configured
     wall-clock threshold
   - it must not imply abnormality or imminent failure

5. Elapsed time must use a monotonic clock
   - durations and thresholds must be computed from monotonic time, not wall
     clock time
   - system clock changes must not distort lifecycle timing

6. Timer lifetime is bound to operation lifetime
   - timers must be started when the lifecycle wrapper starts
   - timers must be canceled immediately on terminal state
   - a timer must never outlive the operation it is reporting on

7. Terminal emission must live in cleanup paths
   - the implementation must emit `finished` / `failed` from `defer`,
     `finally`, RAII-drop, or equivalent cleanup paths so that ordinary errors
     do not leave an operation stuck in a non-terminal state

### Meaningful Feedback Standard

The timer-based system is considered meaningful if:

- users see immediate confirmation that work started
- users see explicit notice when the operation crossed a slow threshold
- users continue to receive periodic evidence that the operation is still alive
- users are never shown fabricated progress precision

This is intentionally weaker than exact progress reporting, but it is also much
more broadly correct.

## Layer 2: Cheap “Likely Slow” Heuristics

### Query Heuristics

Use request/plan metadata already available in memory:

- query shape hash
- driving table
- traversal present
- traversal max depth
- text/vector search present
- final limit

This does not predict exact runtime, but it supports messages like:

- “complex traversal query started”
- “vector query started”

### Write Heuristics

Use request shape:

- counts of nodes, edges, chunks, runs, steps, actions, vec inserts
- total chunk text bytes
- number of upserts / retires

This supports messages like:

- “large write batch started”
- “write includes chunk replacement and vector inserts”

### Admin Heuristics

Use operation name plus optional cheap size signals:

- DB file size
- `PRAGMA page_count`

Only use those signals when they are already acceptable for that code path.

## Layer 3: Engine-Side Liveness For Long SQL

### Decision

For operations known to run long, support coarse liveness from inside the
engine using SQLite progress hooks or staged execution.

### Feasible Mechanisms

- `rusqlite` `progress_handler`
- operation-stage wrappers around multi-step workflows
- chunked rebuild loops when exact SQL atomicity is not required

### Use Cases

- full projection rebuild
- rebuild missing projections on large databases
- safe export
- large vector cleanup / repair

### Event Semantics

Engine-side signals should remain coarse:

- “executing SQL”
- “building FTS”
- “restoring vector profile”
- “exporting database”

Do not promise precise percent complete unless the operation is explicitly
restructured to support it.

## Layer 4: Bridge Streaming

### Current Limitation

The current bridge client waits for one final JSON response.

That means:

- no intermediate status reaches the caller
- timer-based heartbeats must be generated by the caller, not the engine

### Future Decision

Add a streamed event mode, for example NDJSON over stdout:

- one event per line
- final line is the terminal success/failure payload

Example phases:

- `started`
- `stage_changed`
- `heartbeat`
- `finished`
- `failed`

### Why This Is Deferred

- it changes the bridge protocol
- it requires compatibility/versioning work
- timer-based lifecycle feedback can ship earlier with much less disruption

## Public API Shape

### Python

Possible direction:

```python
def on_event(event: OperationEvent) -> None:
    ...

rows = (
    db.nodes("Meeting")
      .text_search("budget", limit=10)
      .execute(progress_callback=on_event)
)

receipt = db.write(request, progress_callback=on_event)
report = db.admin.rebuild_missing(progress_callback=on_event)
```

Or:

```python
with db.observe(on_event):
    db.admin.rebuild_missing()
```

### Rust

Possible direction:

```rust
pub trait OperationObserver: Send + Sync {
    fn on_event(&self, event: OperationEvent);
}
```

Operations can then accept:

- explicit observer parameter
- session-scoped observer
- engine-scoped observer

### Go

Possible direction:

- callback function
- event channel
- writer of structured status events

The Go CLI can then print human-readable lines such as:

- `started projection rebuild`
- `still working after 5.0s`
- `still working after 15.0s`

## Failure Semantics

The feedback system must distinguish:

- operation returned an error
- operation exceeded the slow threshold but is still running
- operation was canceled
- operation was interrupted by context cancellation or process termination

`slow` must never imply failure.

`heartbeat` must never imply forward progress beyond liveness.

If the process crashes or is externally killed, the lifecycle may end without a
terminal event. This is the only expected case where an operation can remain
observably unterminated from the caller’s perspective.

## Security And Privacy Constraints

Do not emit:

- raw query text if it may contain sensitive user data
- chunk text bodies
- full JSON properties
- filesystem secrets or environment variables

Allowed:

- operation category
- counts and sizes
- shape hashes
- non-sensitive labels

## Testing Strategy

### Unit Tests

- timer lifecycle emits `started` then `finished`
- slow calls emit `slow` and `heartbeat`
- failed calls emit `failed`
- fast calls do not emit unnecessary heartbeat noise

### Integration Tests

- query callback receives lifecycle events
- write callback receives lifecycle events
- admin callback receives lifecycle events
- bridge-backed operation emits lifecycle feedback in caller code

### Future Tests

- engine-side heartbeat fires during long rebuilds
- streamed bridge mode preserves event ordering and terminal payload rules

## Open Questions

### Questions That Can Be Answered By The Software Engineer

These are implementation questions, not product questions.

1. Where should the observer be attached?
   - per call
   - per session
   - per engine
   - all three

2. What is the smallest event type that stays stable across Rust, Python, and
   Go?

3. What default timing thresholds give useful feedback without noisy churn?

4. Should fast-path events be emitted synchronously on the caller thread, or
   buffered onto a lightweight internal dispatcher?

5. For Python, is a plain callback sufficient, or is a richer observer object
   needed?

6. Should the Rust engine emit timer-based events only in the binding layer, or
   should the engine own a canonical lifecycle emitter?

7. Which operations justify engine-side progress hooks first?

8. Is a streamed bridge protocol worth doing before historical latency
   heuristics?

9. What metadata is safe and useful by default?

10. How should cancellation interact with progress callbacks and terminal
    events?

### Questions That Need Product Or UX Direction

1. What user-facing wording should be shown for slow operations?
2. How verbose should heartbeat messages be in interactive clients?
3. Should callers be able to suppress all feedback by default or opt in?
4. Should libraries expose raw structured events only, or also human-readable
   default messages?

## Recommended Rollout

### Slice 1

- define the common event schema
- add library-level timer-based lifecycle feedback
- cover queries, writes, and admin operations

### Slice 2

- add cheap shape metadata
- standardize thresholds and event formatting

### Slice 3

- add engine-side liveness hooks for known long operations

### Slice 4

- add streamed bridge support

### Slice 5

- add optional historical latency heuristics

## Decision

Ship response-cycle feedback as a universal operation-lifecycle layer first.

That is the highest-value / lowest-overhead solution.

Then incrementally add:

- cheap heuristics
- engine liveness hooks
- streamed bridge events

Do **not** make exact forecasting or exact progress a prerequisite for giving
users feedback that the system is alive.
