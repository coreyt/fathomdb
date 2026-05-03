---
title: Phase 9 implementer handoff
date: 2026-05-02
target_release: 0.6.0
desc: Things Phase 9 inherits from Phase 8 — contracts to honor, traps not to repeat, tools left in place
---

# Phase 9 handoff

Read `AGENTS.md`, `MEMORY.md`, `dev/plans/0.6.0-implementation.md` Phase 9, and the most recent entries of `dev/progress/0.6.0.md` first. This file is the gap between those and Phase 8's actual landed state.

## Honored contracts (do not break)

- **ADR-0.6.0-database-lock-mechanism is SUPERSEDED.** Use `ADR-0.6.0-database-lock-mechanism-reader-pool-revision`. Writer connection has NO `locking_mode=EXCLUSIVE`. `-shm` is a normal WAL artifact and is in AC-002 + AC-045 allow-lists. Do not re-add EXCLUSIVE — it deadlocks the reader pool (every reader hits 5 s `busy_timeout`; AC-004b's 1000 ops takes ~2500 s). If a Phase 9 feature needs writer exclusivity, find another mechanism (sidecar flock already covers cross- and same-process double-open).
- **Reader pool semantics.** 8 reader connections, `journal_mode=WAL`, `query_only=ON`. Searches MUST run inside `BEGIN DEFERRED` ... `COMMIT` to bind `projection_cursor` to the same WAL snapshot as the rows. Pattern: `read_search_in_tx` in `src/rust/crates/fathomdb-engine/src/lib.rs`. New retrieval paths (FTS5, vector) MUST follow the same tx pattern. Cursor source = `MAX(write_cursor) FROM canonical_nodes` inside the tx. Loading from `Engine::next_cursor` outside a tx reintroduces the AC-059b race — fixture caught it 77/1000 iterations.
- **`Engine` struct field order is load-bearing.** `connection` and `reader_pool` MUST be declared before `profile_contexts`. SAFETY invariant for `unsafe sqlite3_profile` FFI relies on Rust drop order: connections drop before `Box<ProfileContext>` userdata drops. Do not reorder. If you add a new SQLite-tied resource that the profile callback touches, declare it before `profile_contexts` too.
- **Vector identity belongs to the embedder.** Per ADR-0.6.0-vector-identity-embedder-owned and `MEMORY` `project_vector_identity_invariant`. Embedder owns name + revision + dimension. Phase 9 vector storage MUST persist what the embedder hands it; do NOT canonicalize or normalize.
- **No data migration.** Per `feedback_no_data_migration`. Schema v24+ adds tables only; no `INSERT … SELECT` from legacy 0.5 tables.
- **Lifecycle observability surface is matrix-bound.** Phase enum has exactly five values. EventSource has exactly two. EventCategory has seven. CounterSnapshot has seven keys. Adding a new Event field, new Phase variant, new EventSource, new EventCategory, or new CounterSnapshot key requires a successor ADR + interface-doc update in the same PR. Do NOT silently extend.
- **Subscriber trait extensions are default-no-op only.** New observability transports (e.g. corruption-injection events for AC-006) extend `Subscriber` with default-no-op trait methods, NOT new required methods. Existing subscribers compile unchanged.

## Tools left in place that you can use

- **Real `ProfileRecord` emission via `sqlite3_profile` FFI.** `Engine::open` installs the callback on every writer + reader connection. `set_profiling(true)` enables. Subscribers receive `on_profile(record)`. Phase 9's vector + FTS5 statements get profiled for free.
- **`step_count` and `cache_delta` are currently 0** — `sqlite3_profile` does not surface them. AC-005b accepts typed-numeric (zero is fine). If Phase 9 retrieval tuning needs real values, route via `sqlite3_db_status(SQLITE_DBSTATUS_CACHE_HIT_DELTA)` captured around statements. Document the change with a `// Why:` comment and update AC-005b's measurement protocol via ADR if you change the contract.
- **Statement-level slow signal.** `Subscriber::on_slow_statement(SlowStatement { statement, wall_clock_ms })` fires from the same profile callback when wall-clock exceeds `slow_threshold_ms`. Threshold default 100 ms per REQ-006a. Lifecycle `Phase::Slow` event also fires for the operation envelope. Both are required per `dev/design/lifecycle.md` § Slow and heartbeat policy.
- **Typed `Event.code: Option<&'static str>`.** Engine errors emit the stable `EngineError::stable_code` string; SQLite-internal failures emit the SQLite extended-code symbol name via `sqlite_extended_code_name`. AC-006 corruption-injection events should populate `code` with the matching extended code (`SQLITE_CORRUPT`, `SQLITE_NOTADB`, etc.) and `source = SqliteInternal`, `category = Corruption`.
- **`Engine::emit_sqlite_internal_error(&rusqlite::Error)` helper.** Use this from any new error path that bubbles a rusqlite error so subscribers see consistent `(SqliteInternal, Error, code)` events.
- **Profile callback's userdata holds `Arc<AtomicBool>` (profiling toggle) + `Arc<AtomicU64>` (slow threshold).** Both runtime-mutable; visible inside the FFI callback without restart. Same pattern works for any future runtime-toggleable knob you need to observe from the callback.
- **`Engine::execute_for_test(sql)`** (likely under `#[cfg(test)]`) — runs arbitrary SQL through the writer profiling path. Used by AC-007a/b deterministic-slow-cte fixtures. Reuse for any Phase 9 fixture that needs to exercise a specific slow path.

## Traps not to repeat (Phase 8 cost cycles on these)

- **Shape-green claims.** Do NOT mark an AC green if the test only constructs a typed payload or only toggles a flag. The test must bind the AC's measurement protocol. If you can't bind it now, `#[ignore = "AC-NNN: dep-reason"]` and exclude from the green ledger. Pattern from Phase 8: AC-005a, AC-007a, AC-007b, AC-009 were all initially "green (shape only)" and had to be retracted under audit. Do not annotate dishonesty — retract it.
- **`set_slow_threshold_ms(0)` + sub-millisecond op as the slow fixture.** Tests the 0 ms edge case, not the 100 ms threshold contract. Phase 8 retracted this and bound to deterministic-slow CTE. If your timing fixture passes only because the threshold is bogus, the test does not bind the AC.
- **Test-side baseline shifts to make a feature pass.** AC-022b's pre-open FD baseline was once shifted to post-open during the reader-pool wiring; the test then certified a weaker invariant than the AC mandated. AGENTS.md § 5: test files are read-only during fix-to-spec. If a test fails because of a real footprint change, fix the close path or amend the AC parameter via ADR — do not silently move the baseline.
- **`--no-verify`.** Hard ban (AGENTS.md § 10 + MEMORY). Phase 8 implementer used it once mid-session and reset; do not repeat. If a hook blocks, fix the underlying issue.
- **Editing locked design docs without an ADR.** `dev/design/lifecycle.md`, `dev/design/engine.md`, `dev/design/errors.md`, `dev/design/bindings.md`, `dev/interfaces/*.md` are locked. Successor ADR + same-PR interface update is the only valid mechanism. Phase 8 ADR successor (lock-mechanism reader-pool revision) is the worked example.
- **Markdown line-continuations starting with a literal plus + space at column 5.** markdownlint MD004/ul-style reads them as plus-bullets. Use the words `plus`, `and`, or rewrap the line. Phase 8 hit this in the progress log post-merge.
- **Stuck cargo-test processes from race fixtures.** Phase 8's RED test for the cursor race left a stuck `cargo test` process in the sandbox. If you write a long-iteration race fixture, ensure the inner loop is bounded and tested for clean termination before iteration count goes up.

## Specific Phase 9 work targets

### AC-006 corruption injection (currently `#[ignore]`)

- Surface already wired: `(SqliteInternal, Corruption, code = Some("SQLITE_…"))` events.
- Need: deterministic corruption-injection harness. Likely a tool that bit-flips a known page or shadow-table row in a closed DB file, then reopens. Per AC-006 fixture spec ("corrupt-page harness, must include a documented page-corruption tool").
- Test path: open → close → corrupt → reopen → assert `(SqliteInternal, Corruption, code)` event captured + typed `EngineOpenError::Corruption` returned.

### AC-009 robustness/poison fixture (currently `#[ignore]`)

- Surface already wired: `StressFailureContext { thread_group_id, op_kind, last_error_chain, projection_state }` typed payload + Subscriber path.
- Need: one-thread-poison fixture (deterministic op failure under stress; AGENTS-doc fixture spec naming). Stress runner produces the failure event with all four fields populated.
- Pure-type test `ac_009_stress_failure_context_constructs` exists as a compile-time shape lock; do not delete. Add a behavioral test that runs the poison fixture and asserts the deserialized payload.

### AC-020 reader-pool perf gate

> **Pack 5 owns this.** Pack 4 landed an honest harness + retained
> reader-side / projection-runtime changes that closed AC-018 but left
> AC-020 long-run red. Subsequent diagnosis + remediation is scoped in
> `dev/plan/0.6.0-Phase-9-Pack-5-performance-diagnostics.md`. Latest
> numbers, hypothesis ladder, and the kept/reverted experiment ledger
> live in `dev/notes/performance-whitepaper-notes.md`. Do **not** start
> AC-020 work from this file — read those two first and follow the
> Pack 5 plan's pre-flight + phase order.

- Reader pool is wired; AC-021 already binds correctness under concurrent reads + admin DDL.
- AC-020 is the perf gate: 8 reader threads complete the documented read-mix in `≤ P-PARALLEL-TOL × (T_seq / N)`. Per `dev/acceptance.md` AC-020. Pack 4 owns the read-mix fixture + the parallel measurement (already landed); Pack 5 owns the remediation.
- Lives in the long-run gate (`scripts/check.sh AGENT_LONG=1`), not `agent-verify.sh`. Documented in `dev/test-plan.md` alongside AC-021 + AC-059b.

### Retrieval, vector, projections, scheduler

- FTS5 + injection-safe query grammar (`fathomdb-query` crate); per Phase 9 plan, replaces the current `LIKE %query%` placeholder in `Engine::search`.
- Vector storage: vec0 + LE-f32 BLOB encoding + dimension checks. Embedder owns identity.
- Projection cursor + scheduler + projection failure records + restart semantics + explicit regenerate workflow hook for CLI.
- `engine.drain` bounded-completion control method (currently a stub at `Engine::drain` returning Ok(()) — Phase 9 wires it).

## Gate boundary (do not break)

- `./scripts/agent-verify.sh` = fast local loop. Runs lint → typecheck → unit/integration tests EXCLUDING long-run variants. Stays fast.
- `./scripts/check.sh AGENT_LONG=1` = full evidence gate. Runs `agent-verify` plus AC-021 60 s window + AC-059b ~1000-iter race + (Phase 9 should add) AC-020 perf gate + any other long-run variants.
- AGENT_LONG-gated tests MUST be documented in `dev/test-plan.md` § Implementation Order step 3.

## Pinned tunings

- AC-007a/b deterministic-slow CTE N values were pinned to **aarch64 Linux** (this dev runner): `FAST_CTE = 100_000` (~89 ms), `SLOW_CTE = 1_000_000` (~801 ms). Different CI hardware may require re-tuning. If CI fails AC-007a/b, do not relax the AC — re-measure on the canonical CI runner and pin a hardware-portable N or split into runner-specific constants gated by env.

## When in doubt

- Re-read `AGENTS.md` § 5, § 6, § 10. Most Phase 8 mistakes were rule violations the rules were already explicit about.
- Re-read `dev/progress/0.6.0.md` Phase 8 hardening entry. The retraction-and-redo cycle is the worked example of how to recover from shape-vs-contract dishonesty.
- ASK the orchestrator before improvising on a locked design doc, ADR, or interface contract.
