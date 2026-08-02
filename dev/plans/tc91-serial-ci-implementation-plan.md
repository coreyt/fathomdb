---
status: PROPOSED
date: 2026-08-01
depends_on: TC-91 projection commit hardening design
---

# TC-91, then serial CI — implementation plan

## Purpose

Land the projection-worker commit correction first. Start the serial-CI work
only after its acceptance evidence is complete. This ordering prevents a test
gate change from masking or substituting for the engine correctness fix.

This plan assigns no release number and does not authorize a tag, registry
publication, or release-state change.

## Inputs and review record

- [TC-91 projection commit hardening design](../design/0.8.20-tc91-projection-commit-hardening.md)
  — design series `4f085de5`, `37011aee`, and `797ab392`. Its GPT-5.6 Sol
  review found and the two permitted fix rounds addressed: the panic-terminal
  commit path, error classification, restart/shutdown handshaking, and
  exhausted-timeout semantics. The last corrective pass is not followed by a
  third review, by the agreed two-fix-round cap.
- [Temporary serial Rust-workspace release-gate design](../design/temporary-serial-rust-workspace-release-gate.md)
  — design series `27e57f6b` and `9fcf2358`; independent review PASS.

## Sequence A — harden the projection-worker commit path

### A1. Red tests and test seams

Add the deterministic, test-only worker-commit fault controls specified in the
TC-91 design. Write the failing tests before production changes. They must
cover all of the following:

- a SQLite worker-commit failure is observable but creates no terminal,
  vector, or `projection_failures` record;
- durable redispatch eventually completes the pending row without silently
  dropping it;
- a failed threshold-crossing commit leaves mean-accumulator and pin-event
  state unchanged until the successful redispatch;
- the `ProjectionPanic` terminal-commit path reports and redispatches if that
  transaction fails;
- non-SQLite failure classification, and stop-before-redispatch then reopen
  behavior, use the specified deterministic handshakes;
- the TC-57 pressure suite preserves its caller-race purpose while requiring
  zero duplicate embeds under the corrected worker behavior.

### A2. Implement the narrow correction

Make `commit_projection_outcomes` acquire `BEGIN IMMEDIATE` writer intent
before its reads. Return and handle errors from both ordinary and panic worker
commit paths. Use the existing lifecycle diagnostics and durable missing
terminal as the redispatch mechanism; do not turn a commit failure into an
embed failure or create a new queue.

Keep mean-accumulator and `MeanVecPinned` publication commit-atomic. Do not
redesign worker scheduling, public bindings, counters, or the caller write
transaction.

### A3. Local acceptance before CI

In a fresh isolated clone at the candidate commit:

1. Run the focused deterministic TC-91 and TC-57 suites ten consecutive times,
   recording each command's immediate exit code.
2. Run the complete serial workspace gate five consecutive times, again
   recording each immediate exit code and log path.
3. Run `bash scripts/agent-verify.sh` once after the focused evidence is green.

Any failure returns to A1/A2. CI is confirmation of this local evidence, not
the first execution of the candidate script.

### A4. Independent review and landing gate

Require an independent code review of the implementation diff and a clean
full-workspace CI run on the exact landing commit. Only after that evidence is
recorded is Sequence B allowed to begin.

## Sequence B — make the serial workspace gate canonical

### B1. Red script and workflow tests

Add the red-first script tests described by the serial-gate design. They must
prove the exact `--serial` and `--parallel-report` Cargo invocations, argument
validation, unmasked exits, and parsed workflow semantics. The workflow tests
must reject direct gating `cargo test --workspace` calls and must execute the
reporter shell body with a failing fake runner to prove immediate return-code
capture and non-gating artifact ordering.

### B2. Implement the canonical runner

Add `scripts/test-rust-workspace.sh` with explicit required modes. The serial
mode must use both Cargo job serialization and libtest thread serialization;
the parallel mode preserves the current command and returns Cargo's unmodified
exit status.

Route `scripts/agent-test.sh` through serial mode. Route Linux `verify`,
`rust-windows`, and `rust-macos` through the same canonical serial path. Keep
`--no-fail-fast` for all three legs. No CI-only serial command and no direct
gating workspace Cargo command may remain.

### B3. Add the non-gating parallel reporter

Add the Linux parallel reporter as a separate finite-timeout job. It must save
complete Cargo stdout/stderr to `$RUNNER_TEMP`, capture the raw return code
immediately, publish the diagnostic summary and warning, upload the exact log
with the pinned artifact action under `if: always()`, and only then exit zero.
It is diagnostic evidence, never a pass/fail release gate.

### B4. Local-first and CI evidence

At the exact candidate commit, create three clean standalone clones. Bootstrap
each and run `AGENT_VERBOSE=1 bash scripts/agent-verify.sh` once, preserving
the immediate exit code, timestamps, and log path for every run. Stop on the
first non-zero result.

Only then open/update the CI candidate. Require the Linux, Windows, and macOS
serial Rust legs to pass. Retain the parallel reporter log and its return code
as diagnostic evidence regardless of its result.

## Cross-cutting constraints

- Use fresh worktrees from current `main`; never run editable Python builds in
  a worktree.
- Every implementation change follows red → green → refactor. Do not alter a
  failing test merely to clear it.
- Record immediate exit codes; do not let a trailing command, pipeline, retry,
  or artifact upload overwrite the tested command's status.
- Keep release state, tag movement, and registry publication outside this
  plan. They require their own evidence and explicit authorization.

## Completion evidence

The work is ready for subsequent release-gate consideration only when all of
the following exist:

1. TC-91 tests and five serial-workspace local repetitions pass at the landed
   TC-91 commit, with immediate return codes retained.
2. The TC-91 implementation has independent-review evidence and green CI.
3. The serial-runner tests, three clean-clone local `agent-verify` runs, and
   all three serial Rust CI legs pass at the serial-gate commit.
4. The parallel reporter preserves its complete log and raw return code without
   determining the gate result.
