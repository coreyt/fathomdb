# TODO: Response-Cycle Feedback

## Purpose

Track implementation of the response-cycle feedback design in
`dev/design-response-cycle-feedback.md`.

This tracker is execution-oriented. It is the to-do list and acceptance gate
for shipping the first production slice.

## Required Development Approach

TDD is required for this work.

Rules:

- Write or update failing tests before implementing behavior.
- Implement the smallest change that makes the next test pass.
- Refactor only after the behavior is covered.
- Do not treat "code executed" as sufficient proof of completeness.
- Tests must validate externally observable feature behavior and feature
  completeness across public surfaces.

## Definition Of Done

This work is done only when all current public blocking operation surfaces
expose optional response-cycle feedback with the same lifecycle semantics:

- `started`
- `slow`
- `heartbeat`
- `finished`
- `failed`

And when tests prove:

- the events are emitted correctly
- the events are ordered correctly
- the terminal guarantees hold
- the behavior is available across Rust, Python, and Go/CLI
- the feature works for queries, writes, and admin/recovery operations
- existing no-feedback call paths still work

## Global Constraints

- Timer-based feedback may report only lifecycle, elapsed time, and liveness.
- Timer-based feedback may not claim percent-complete, ETA, rows processed, or
  forward progress.
- All timing must use monotonic clocks.
- Exactly one terminal event is allowed per operation.
- No heartbeat may occur after terminal state.
- No extra database inspection queries may be added to the fast path in v1.
- Do not add bridge streaming, SQLite progress handlers, or forecasting queries
  in this slice.

## Default Event Policy

Unless explicitly overridden by caller configuration:

- `slow` threshold: `500ms`
- `heartbeat` interval: `2000ms`

## Operation Coverage Required In V1

- `engine.open`
- `query.compile`
- `query.explain`
- `query.execute`
- `write.submit`
- `admin.check_integrity`
- `admin.check_semantics`
- `admin.rebuild_projections`
- `admin.rebuild_missing_projections`
- `admin.trace_source`
- `admin.excise_source`
- `admin.safe_export`

## Work Items

### 1. Canonical Event Model

- [x] Define one canonical response-cycle event model shared across surfaces.
- [x] Define the operation taxonomy used by Rust, Python, and Go.
- [x] Define `FeedbackConfig` with default thresholds.
- [x] Document the terminal-state and monotonic-time guarantees in code comments
      and public docs.

Acceptance:

- Tests prove event ordering and one-terminal-event behavior using a controlled
  clock or deterministic timer hooks.

### 2. Rust Core Feedback Wrapper

- [x] Add a reusable internal lifecycle wrapper that owns:
  - operation id creation
  - monotonic timing
  - timer scheduling
  - terminal-state suppression
- [x] Add public Rust feedback types:
  - `ResponseCycleEvent`
  - `OperationObserver`
  - `FeedbackConfig`
- [x] Add additive `_with_feedback` Rust entrypoints for all covered
      operations.
- [x] Keep existing methods as wrappers with no feedback observer.

Acceptance:

- Rust tests fail first, then pass, for:
  - fast success
  - slow success
  - slow failure
  - no heartbeat after terminal state
  - source compatibility of existing no-feedback methods

### 3. Python Public API

- [x] Add `ResponseCycleEvent` and `FeedbackConfig` Python types.
- [x] Add optional `progress_callback` and `feedback_config` to:
  - `Engine.open`
  - `Engine.write` / `Engine.submit`
  - `Query.compile`
  - `Query.explain`
  - `Query.execute`
  - all `AdminClient` blocking methods
- [x] Route Python feedback through the Rust timer-based implementation.
- [x] Ensure callback failures do not fail the DB operation.

Acceptance:

- Python tests fail first, then pass, for:
  - correct lifecycle events on query, write, and admin calls
  - exactly one terminal event on success and failure
  - no feedback after terminal state
  - callback exceptions are contained
  - event objects are stable and fully populated

### 4. Go Bridge Client And CLI

- [x] Add additive `ExecuteWithFeedback(...)` to the Go bridge client.
- [x] Keep existing `Execute(...)` behavior unchanged.
- [x] Add timer-based lifecycle feedback around whole bridge request duration.
- [x] Add a CLI renderer for `slow` and `heartbeat` messages to stderr.
- [x] Use feedback-aware execution in user-visible command paths.

Acceptance:

- Go tests fail first, then pass, for:
  - correct lifecycle around a mocked slow bridge call
  - correct terminal behavior on context cancellation and bridge failure
  - CLI emits slow/heartbeat output only after threshold crossing
  - fast operations remain quiet

### 5. Feature-Completeness Coverage

- [x] Add end-to-end tests that prove the feature exists on all supported
      blocking operation classes:
  - query
  - write
  - admin/recovery
- [x] Add coverage proving feedback is optional and non-breaking.
- [x] Add coverage proving the same semantic contract across Rust, Python, and
      Go/CLI.

Acceptance:

- Tests validate feature completeness, not just code execution.
- The suite proves the user-visible contract, not merely internal helper
  behavior.
- Missing coverage for any public operation class blocks completion.

## Required Test Philosophy

These tests must be feature-completeness tests.

That means tests should answer questions like:

- Does every required public operation surface support feedback now?
- Do all surfaces emit the same lifecycle semantics?
- Does the user receive meaningful liveness feedback for slow operations?
- Do terminal guarantees hold under success, failure, and cancellation?
- Does feedback remain truthful and conservative?

These tests should not be limited to:

- helper function coverage
- branch coverage
- "callback was invoked once" style implementation checks
- internal-only timer tests without public API verification

## Suggested Test Layers

### Rust

- deterministic lifecycle tests for the shared wrapper
- public API tests for `_with_feedback` methods
- compatibility tests for existing no-feedback methods

### Python

- public callback contract tests
- exception containment tests
- query/write/admin behavior tests

### Go

- bridge client lifecycle tests
- cancellation/failure tests
- CLI stderr behavior tests

### Cross-Surface Acceptance

- one query scenario
- one write scenario
- one admin/recovery scenario

Each must prove:

- `started` emitted
- `slow` emitted only when threshold crossed
- `heartbeat` repeats only while running
- exactly one terminal event emitted
- no terminal mismatch or post-terminal heartbeat

## Explicit Non-Scope For This Tracker

Do not implement in this slice:

- streamed bridge events
- SQLite progress-handler integration
- percent-complete reporting
- ETA reporting
- historical telemetry storage
- predictive "likely slow" models
- extra SQL counting queries for forecasting

## Completion Gate

Do not mark this work complete until:

- all required TDD-first tests exist
- those tests prove feature completeness across Rust, Python, and Go/CLI
- all tests pass
- the implementation matches the design constraints in
  `dev/design-response-cycle-feedback.md`
