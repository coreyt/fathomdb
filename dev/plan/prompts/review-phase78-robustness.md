# Reviewer template — Phase 7/8 robustness check (codex)

## Model + effort

`codex exec --model gpt-5.4 -c model_reasoning_effort=high`. Stdin
closed (`< /dev/null`).

Read-only.

## Log destination

`dev/plan/runs/<phase>-review-phase78-<utc-ts>.md` (or appended to
the experiment review file from `review-experiment.md`).

## Required reading + discipline (reviewer)

- **Read `AGENTS.md`** — canonical agent operating manual. §1
  Public-surface-is-contract is the lens for this review: any
  weakening of a Phase 7/8 invariant is a contract regression.
- **Read `MEMORY.md` + `feedback_*.md`**.
- **Reviewer is read-only**. No file edits except the verdict file.

## Context

Phase 7 and Phase 8 of the 0.6.0 rewrite establish invariants that
later perf work must not weaken. This reviewer pass exists because
perf changes (especially FFI, mutex, and SQL refactors) are exactly
the kind of change that can silently break those invariants while
gates still go green on the perf side.

Plan §0.1 mandates this check for B.1 + D.1, optional but recommended
for B.2 / B.3 / C.1.

### Invariants to enforce

1. **Lifecycle event taxonomy** (REQ-006a / AC-005a / AC-007b):
   `Phase::Started`, `Finished`, `Failed`, with the correct
   `EventCategory` (Writer / Search / Admin / Error). The
   capture-ordinal-before-raise-ordinal contract for errors
   (AC-003d) — Failed/Error events fire **before** the
   `EngineError` returns to the caller.
   Code anchors:
   - `src/rust/crates/fathomdb-engine/src/lib.rs` write path:
     `write` at line 794, `write_inner` at line 831, error-emit
     ordering at line 818-826.
   - `Engine::search` at line 869, error-emit ordering at line
     880-895.
2. **Cursor-from-snapshot contract** (REQ-013 / AC-059b / REQ-055):
   `read_search_in_tx` derives `projection_cursor` from
   `load_projection_cursor` **inside** the `BEGIN DEFERRED` reader
   transaction (lib.rs:1283-1289). Result rows + cursor must be
   from the same snapshot.
3. **Profile-callback SAFETY** (commit `ba900a1`):
   - `ProfileContext` lives in `Box`es, not a `Vec<ProfileContext>`,
     because the FFI pointer must be stable
     (`Engine.profile_contexts: Mutex<Vec<Box<ProfileContext>>>` at
     lib.rs:77 — clippy-allowed `vec_box`).
   - `Box<ProfileContext>` outlives the connection that captured
     its pointer (drop order: connections drop first, then
     `profile_contexts`).
   - `install_profile_callback` (lib.rs:2401),
     `uninstall_profile_callback` (lib.rs:2436),
     `profile_callback_trampoline` (lib.rs:2457) — pointer
     lifetime invariants.
4. **Reader-pool ADR** (commits `09415b4`, `123a0d6`, `7852269`):
   - `READER_POOL_SIZE = 8` (lib.rs:48).
   - `ReaderPool` (lib.rs:158) must not serialize behind one mutex.
   - Each reader connection opens with `journal_mode=WAL` and
     `query_only=ON` (lib.rs:777-783).
5. **Database-lock-mechanism revision**:
   - Per-engine file lock (`Engine.lock: Mutex<Option<File>>`,
     lib.rs:54) — exclusive open contract preserved.
6. **Snapshot derivation in projection runtime**: the projection
   dispatcher / worker connections (`projection_dispatcher_loop` at
   lib.rs:1358, `projection_worker_loop` at lib.rs:1410) are
   long-lived and use `prepare_cached`; do not change their
   connection lifecycle without flagging.

### What to check on the diff

- For each invariant above, ask: did this diff weaken it? Specifically:
  - Did any callsite stop emitting Started/Finished events?
  - Did the read transaction's BEGIN/COMMIT boundary move?
  - Did `projection_cursor` move outside the reader-tx?
  - Did `Box<ProfileContext>` change to `ProfileContext` (or move
    into a non-stable container)?
  - Did `READER_POOL_SIZE` change?
  - Did any reader skip `query_only=ON` or `journal_mode=WAL`?
  - Did the file-lock acquisition path change?
- Even if the diff does not directly touch these anchors, ask: does
  the change affect their preconditions? (e.g. a mutex change in
  init order can affect file-lock observability.)

### Output shape (Markdown)

```markdown
# Phase 7/8 robustness verdict — <phase> — <ts>

Verdict: PASS | CONCERN | BLOCK

## Invariant cross-check
- Lifecycle event taxonomy (REQ-006a / AC-005a / AC-007b / AC-003d): PASS | <finding>
- Cursor-from-snapshot (REQ-013 / AC-059b / REQ-055): PASS | <finding>
- Profile-callback SAFETY (ba900a1): PASS | <finding>
- Reader-pool ADR (09415b4 / 123a0d6 / 7852269): PASS | <finding>
- Database-lock-mechanism: PASS | <finding>
- Projection-runtime connection lifecycle: PASS | <finding>

## Findings
1. <file:line> — <invariant> — <severity>
2. ...

## Recommended next step
<KEEP | REVERT | ESCALATE>
```

`BLOCK` if any invariant cross-check is non-PASS. `CONCERN` if a
preconditions question is raised but not directly broken.

## Acceptance criteria

- Six-line invariant cross-check fully populated.
- Every finding cites `file:line`.
- Verdict is one of PASS / CONCERN / BLOCK.

## Files allowed to touch

- The verdict Markdown file only.

## Files NOT to touch

- Everything else.

## Verification commands

```bash
test -f dev/plan/runs/<phase>-review-phase78-<ts>.md
grep -E "^Verdict: (PASS|CONCERN|BLOCK)$" dev/plan/runs/<phase>-review-phase78-<ts>.md
```

## Required output to orchestrator

`Verdict:` line + `Recommended next step:` line. Block-severity
findings drive a forced revert unless explicitly overridden in §12.

## Required output to downstream agents

- None.

## Update log

_(append the implementer's commit SHA + which invariants the diff
most plausibly affects before invoking codex)_
