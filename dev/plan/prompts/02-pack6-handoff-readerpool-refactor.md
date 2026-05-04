# Pack 6 handoff — Architectural read-path refactor (thread-affine readers)

Phase 9 Pack 6 continues AC-020 closure after Pack 5 closed ESCALATE on
2026-05-04. This prompt is for the intervention most likely to close the gap
by itself, not for another diagnostic-only pass.

This prompt supersedes the earlier "smallest-radius" `ArrayQueue` pool swap as
the starting point. Pack 5 left three surviving residuals:

- `ReaderPool::Mutex<Vec<Connection>>` borrow/release contention.
- Cross-thread connection handoff / wrapper-side synchronization above SQLite.
- WAL shared-memory atomics.

An `ArrayQueue` swap can only remove the first item cleanly. A thread-affine
reader topology attacks the first two in one change. If it still fails, the
remaining suspect is largely WAL-side and AC-020 is unlikely to close without a
deeper SQLite usage change or formal deferral.

## 1. Read order on resume

1. `dev/plan/runs/STATUS.md` — final Pack 5 state + branch tip.
2. `dev/plan/runs/final-synthesis-output.json` — Pack 5 close decision +
   `data_for_pivot` / `next_packet_pointer`.
3. `dev/notes/performance-whitepaper-notes.md` §11 — final Pack 5 narrative.
4. `dev/notes/performance-whitepaper-notes.md` §5 — reverted experiment ledger
   for B.1 / C.1 / E.1.
5. `dev/plan/prompts/02a-pack6-diagnostics-recapture.md` — separate read-only
   recapture prompt. Not part of this handoff. Use only if the human asks for a
   fresh attribution pass before implementation.
6. `dev/plan/prompts/01-orchestrator-resume.md` §4 — anti-chaining defenses.

## 2. State at this handoff

- Branch: `0.6.0-rewrite`. Tip at handoff creation: `46f693a`.
- AC-020 final Pack 5 medians: seq `184.7 ms`, conc `124.0 ms`, bound
  `34.6 ms`, speedup `1.487x`. Required speedup remains `>= 5.33x`.
- AC-017 and AC-018 green throughout Pack 5.
- Pack 5 falsified:
  - runtime `sqlite3_config(SQLITE_CONFIG_MULTITHREAD)`,
  - compile-time `SQLITE_THREADSAFE=2`,
  - read-path `prepare_cached`.
- Whitepaper closeout says residual contention is architectural:
  `ReaderPool` Rust-side, wrapper-side / handoff-side, or WAL shared-memory.

## 3. Pack 5 falsifications you do not retry

Per the Pack 5 record:

- Do not retry runtime `CONFIG_MULTITHREAD`.
- Do not retry compile-time `THREADSAFE=2`.
- Do not retry read-path `prepare_cached`.
- Do not start Pack 6 with the `ArrayQueue`-only pool swap. That change is
  still a valid fallback experiment, but by itself it is not the strongest
  closure attempt: it removes only one of the two most plausible Rust-side
  residuals.

## 4. Authorized intervention

**F.0 — Replace borrow/release pooling with thread-affine reader workers.**

Design target:

- Keep `READER_POOL_SIZE = 8`.
- Spawn 8 long-lived reader worker threads at `Engine::open`.
- Each worker owns exactly one read-only SQLite `Connection` for its lifetime.
- `Engine::search` dispatches a request to one worker via channel.
- Worker executes the full search on its own thread, inside the same
  `BEGIN DEFERRED ... COMMIT` snapshot pattern already required by AC-059b.
- Result returns over a response channel; no `Connection` object crosses thread
  boundaries after startup.

Why this is the best single-shot closure attempt:

- It removes `ReaderPool::Mutex<Vec<Connection>>` and its `Condvar` from every
  query.
- It removes per-query cross-thread connection transfer, which is the other
  strongest surviving Rust-side suspect after Pack 5.
- It preserves the existing SQLite/WAL/search logic instead of mixing topology
  change with SQL or schema change.
- If this does not move AC-020 materially, the residual is likely WAL shared
  memory or an uncloseable contract for this design envelope.

## 5. Required implementation shape

1. Add a private reader-runtime type, for example:
   - `ReaderWorkerPool`
   - `ReaderWorkerHandle`
   - `ReaderRequest`
   - `ReaderResponse`
2. Use bounded channels. A simple round-robin dispatcher is sufficient. Avoid a
   global mutex on the hot path.
3. Move the current borrowed-connection search path into a worker-owned search
   path. The worker must still call the same snapshot-preserving read helper or
   an equivalent private refactor of it.
4. Preserve all current contracts:
   - same-snapshot `projection_cursor`,
   - lifecycle/profiling/slow-signal behavior,
   - clean shutdown / `Engine::close`,
   - AC-021 / AC-022 behavior,
   - no public Rust API expansion without interface-doc / ADR work.
5. Keep statement caching out of scope for the first landing unless it falls out
   naturally from the worker-owned connection shape without extra surface area.
   Pack 6 is topology-first, not parse-cost revisit.

## 6. Test discipline

Red-green-refactor remains mandatory.

Required tests:

1. A shutdown/integrity test proving all reader workers exit on close and all
   owned connections drop.
2. A routing/concurrency stress test proving N concurrent searches complete and
   no request is lost or duplicated.
3. Existing AC-059b cursor/read snapshot coverage must stay green unchanged.
4. Existing AC-021 / AC-022 / AC-018 tests must stay green unchanged.

Do not write a synthetic microbenchmark as the acceptance oracle. AC-020
remains the decision metric.

## 7. Decision rule

- **KEEP** iff:
  - `concurrent_median_ms <= 80`,
  - `speedup >= 5.0x`,
  - AC-018 green,
  - no lifecycle/close regression.
- **INCONCLUSIVE** iff:
  - concurrent improves materially from the `124 ms` Pack 5 final median but
    stays in `81..100 ms`, and
  - post-change perf capture shows the Rust-side mutex/handoff share collapsed.
- **REVERT** iff:
  - `concurrent_median_ms > 100`,
  - or AC-018 turns red,
  - or worker shutdown / delivery invariants fail,
  - or new hot symbols still point primarily at Rust-side pool/handoff logic.

Rationale: this packet is not trying to win a small percentage. It is trying to
remove the strongest remaining architectural bottleneck by itself.

## 8. Reviewer requirements

Reviewer (`codex gpt-5.4`) remains mandatory on KEEP or INCONCLUSIVE.

Reviewer must confirm:

- no hidden mutex on request dispatch hot path,
- clean worker shutdown and connection drop,
- no public-surface expansion,
- same-snapshot cursor contract preserved,
- AC-018 unchanged,
- no new cross-thread unsafety around SQLite handles.

## 9. Out of scope for this handoff

- Read-only perf re-capture before code change. That lives in
  `dev/plan/prompts/02a-pack6-diagnostics-recapture.md`.
- Schema/index work such as `canonical_nodes(write_cursor)`.
- WAL2 upgrade.
- Replacing `rusqlite` with raw `libsqlite3-sys`.
- AC-020 contract rewrite / formal deferral. Those are next decisions only if
  F.0 fails.

## 10. Failure interpretation

If F.0 fails after a clean implementation and clean perf recapture, treat that
as strong evidence that WAL shared-memory atomics or SQLite’s broader runtime
model are the remaining ceiling. At that point the next packet is not
`ArrayQueue`; it is either:

- formal AC-020 deferral,
- WAL2 / vendor-SQLite path,
- or a larger redesign such as reader/writer physical separation.

## 11. Spawning F.0

Use the resume §4 spawn block with anti-chaining defenses enabled.

- Implementer model: Opus 4.7 high.
- Reviewer: codex `gpt-5.4` high on KEEP / INCONCLUSIVE.
- Worktree pattern: `/tmp/fdb-pack6-F0-thread-affine-readers-<ts>`.
- Branch: `pack6-F0-thread-affine-readers-<ts>`.
- Baseline: `0.6.0-rewrite` tip.

## 12. Pause points

- After red tests are written.
- After implementer returns.
- After reviewer verdict.
- After packet close.

## 13. Success definition

- AC-020 passes over 5 consecutive `AGENT_LONG=1` runs, or
- AC-020 is formally deferred with enough evidence that no further small-radius
  experiment remains justified.

## Update log

- 2026-05-04 — Rewritten from the earlier lock-free `ArrayQueue` handoff. Main
  Pack 6 intervention is now thread-affine reader workers because it is the
  strongest single change that can remove both surviving Rust-side suspects in
  one step. The read-only recapture step moved to
  `02a-pack6-diagnostics-recapture.md`.
