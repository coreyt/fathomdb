# Pack 6.G handoff — Canonical-SQLite tuning on the F.0 baseline

Phase 9 Pack 6.G continues AC-020 closure work after Pack 6 closed
F.0 (thread-affine reader workers KEPT, human override of REVERT
rule) on 2026-05-04. Pack 6.G stays inside canonical/release SQLite
— no WAL2, no `libsqlite3-sys` swap, no rusqlite replacement. Those
remain Pack 7+ research territory.

F.0 removed Rust-side ReaderPool / cross-thread-handoff cost
(speedup 1.49×→3.49×, +2.34×). The residual gap is now plausibly
SQLite-internal: WAL shared-memory atomics, checkpoint pressure,
per-connection allocator (lookaside), and / or parse cost on
sticky long-lived reader connections. Pack 6.G attacks each of
those with the smallest-radius, highest-evidence canonical-SQLite
levers, in the order the telemetry pass picks.

## 1. Read order on resume

1. `dev/plan/runs/STATUS.md` — Pack 6 closed state + Pack 6.G
   active phase.
2. `dev/plan/runs/F0-thread-affine-readers-output.json` — F.0
   numeric baseline + worker pool topology summary.
3. `dev/plan/runs/F0-review-20260504T122055Z.md` — codex BLOCK
   findings + override rationale; key for future spawn discipline.
4. `dev/notes/performance-whitepaper-notes.md` §4 (kept) + §5
   (do-not-retry) + §11 (Pack 5 narrative). §5 still binds — no
   retry of B.1 / C.1 / E.1 without explicit override.
5. `dev/plan/prompts/01-orchestrator-resume.md` §4 — anti-chaining
   defenses (PREAMBLE + `--disallowedTools Task Agent` + stream-
   json). Apply on every spawn.
6. The next phase prompt per "Next action" in STATUS.md.

## 2. State at this hand-off

- Branch: `0.6.0-rewrite`. Tip: `a00cd13` at handoff creation.
- F.0 commits already on tip: `4ebdd68` (refactor) + `29cdc6f`
  (cfg-gate fix) + `a00cd13` (KEEP bookkeeping).
- AC-020 N=5 numbers (post-F.0, this host): seq 531 ms / conc
  155 ms / speedup 3.49×. Required ≥ 5.0× KEEP / ≤ 80 ms conc.
  Gap: 75 ms above conc bound; 1.51× below speedup floor.
- Host caveat: this worktree machine runs ~3× slower in absolute
  ms than the Pack 5 reference machine. Cross-pack comparisons
  use the speedup ratio, not absolute ms.
- AC-017, AC-018 green; AC-018 drain 56 ms.
- Active worktrees: none.
- Anti-chaining defenses verified WORKING on F.0 spawn (single
  coherent agent; no Task spawns). Keep on for all G-phase spawns.

## 3. Pack 5 / Pack 6 falsifications you do not retry

Per the whitepaper §5 do-not-retry list:

- Runtime `sqlite3_config(SQLITE_CONFIG_MULTITHREAD)` (B.1).
- Compile-time `SQLITE_THREADSAFE=2` rebuild (C.1).
- Read-path `prepare_cached` on borrow/release pool connections
  (E.1). **Note:** G.2 in this packet revisits `prepare_cached` on
  the F.0 _thread-affine_ connections, which is a different
  topology and a different argument. Treat G.2 as new territory,
  not an §5 retry — see G.2 prompt for the explicit override
  rationale.
- Reader `cache_size` / `mmap_size` global tuning (Pack 5 reverted
  experiment ledger). G.0 may _measure_ mmap_size effects but does
  not change the production default.
- Reader open flags `READ_ONLY | NO_MUTEX`. G-phase keeps
  serialized open flags; thread-affine ownership is the safety
  argument, not the open-flag bits.

## 4. Authorized G-phase interventions (in execution order)

### G.0 — WAL / checkpoint telemetry pass (read-only)

Re-capture AC-020 perf evidence on the F.0 tip with extended
symbol grouping. Distinguish:

- WAL shared-memory atomics (`walIndexMap`, `walFrames`, `walRead*`
  function family, atomic / CAS primitives inside `pager.c` / `wal.c`).
- Checkpoint cost (`sqlite3WalCheckpoint`, busy-handler interaction,
  auto-checkpoint threshold hits).
- Per-connection allocator behavior (`sqlite3MallocSize`,
  `sqlite3_release_memory`, lookaside hit/miss if surfaceable).
- Parse cost on sticky reader connections (`sqlite3RunParser`,
  `sqlite3VdbePrepare`). Should be high if E.1 hypothesis applies
  to F.0 topology; should be low if F.0 already amortized it.
- Our code share (engine + worker dispatch).

Output: classification JSON + a short reviewer-ready note picking
the strongest single lever among G.1 / G.2 / G.3 for the next
phase. No production code change. No commit unless the diagnostic
adds a `#[ignore]`-gated counter test.

### G.1 — Reader-worker lookaside tuning

Configure SQLite per-connection lookaside immediately after
`Connection::open` for each F.0 reader worker. Use
`sqlite3_db_config(SQLITE_DBCONFIG_LOOKASIDE, ...)` with
documented sane defaults (per <https://www.sqlite.org/malloc.html>
§3 — typical 1200-byte slots, 100 slots, sized for the workload's
prepared-statement footprint; revise from G.0 evidence).

Expected impact (per SQLite docs): ~10–15% overall perf in
allocator-heavy workloads. Reader-side, expressed across N=8
workers with sticky connections, is exactly the documented happy
path. Sequential effect: small. Concurrent effect: small-to-
medium if the allocator is contended.

KEEP / REVERT decision rule lives in `G1-reader-lookaside.md`.
Numeric KEEP target: any clean, reproducible improvement on
concurrent_median_ms with no AC-017 / AC-018 regression. Lookaside
is too small a lever to require AC-020 closure alone — it is a
sequencing decision, not a closer.

### G.2 — Per-worker prepared-statement cache (sticky-connection revisit of E.1)

E.1 (Pack 5) added `prepare_cached` on the four read-path search
statements against borrow/release pool connections. Sequential
improved -13.7%, concurrent unchanged, ratio worsened, codex BLOCK
on public surface + decision-rule mismatch + partial test. REVERTED.

The F.0 topology _is_ the natural shape for prepared-statement
caching: each reader worker owns one connection for its lifetime,
so the cache amortizes across the worker's full lifetime, not just
the brief borrow window. G.2 re-runs that experiment with two
binding constraints from E.1's review:

- All four statements covered (no partial test).
- No public Rust API expansion. Use private worker-internal cache
  with an integration test that drives observable behavior, not a
  `pub fn ..._for_test` accessor unless it is `#[cfg(debug_assertions)]`-gated and explicitly justified in the diff.

KEEP target: AC-020 numerics improve cleanly (no ratio worsening).
The Pack 5 §5 entry binds: G.2 is not a retry of the same
experiment; it is the same SQL change applied to a strictly
different connection-lifetime topology. Whitepaper §12 carries the
explicit override note.

### G.3 — WAL checkpoint policy / tuning

Adjust auto-checkpoint threshold (PRAGMA `wal_autocheckpoint`),
busy timeout, or move to an explicit application-driven checkpoint
schedule on the writer side. If G.0 telemetry says checkpoint cost
is small or zero on the read-heavy fixture, G.3 may be SKIPPED in
this packet and queued for a real-write-load packet.

KEEP target: lower variance + lower max latency on AC-020
concurrent runs without regressing AC-017 / AC-018.

## 5. Phase ordering

1. G.0 telemetry first. Picks strongest lever or surfaces a non-
   listed bottleneck (orchestrator pivot point).
2. Run interventions in the order G.0 picks. Default order is
   G.1 → G.2 → G.3.
3. After each intervention: KEEP or REVERT, then re-measure
   AC-020 N=5 and update the chained baseline-of-record.
4. Final synthesis at packet close: cumulative speedup vs F.0
   baseline (3.49×) and vs the original Pack 5 baseline (1.49×).

## 6. Decision rules (per phase)

- KEEP iff AC-017 + AC-018 stay green AND AC-020 numerics improve
  cleanly (no ratio worsening, no concurrent_median_ms regression
  outside one stddev) AND no contract / public-surface violation.
- INCONCLUSIVE iff numerics flat within noise; record evidence and
  move on.
- REVERT iff any of the above fails.

The packet-close success definition is **not** "AC-020 closes
within 0.6.0". It is "every canonical-SQLite lever has been tried,
KEEPs are landed, and the residual gap is documented well enough
that Pack 7 (WAL2 / vendor-SQLite / reader-writer split) can be
scoped from evidence rather than guess."

## 7. Reviewer requirements

Codex `gpt-5.4` high reviewer **mandatory on KEEP / INCONCLUSIVE**
for G.1 / G.2 / G.3. G.0 is read-only diagnostics — reviewer
optional unless it lands a counter test.

Reviewer must confirm per phase:

- No retry of an §5 do-not-retry experiment without explicit
  override note in the prompt + whitepaper.
- No public Rust API expansion without ADR / interface-doc work,
  or `#[cfg(debug_assertions)]` gating with the existing convention.
- AC-018 unchanged; same-snapshot cursor contract (REQ-013 /
  AC-059b / REQ-055) preserved.
- `./scripts/agent-verify.sh` run after meaningful edits (not
  `dev/agent-verify.sh` — that path does not exist, F.0 reviewer
  block #3).

## 8. Out of scope for Pack 6.G

- WAL2 mode. Branch-only SQLite; Pack 7 territory.
- `libsqlite3-sys` replacement / vendor-SQLite path. Pack 7.
- Reader/writer physical separation. Pack 7.
- Schema/index changes (e.g. `canonical_nodes(write_cursor)`).
- Any change that re-opens borrow/release pooling. F.0 is the
  load-bearing topology for the entire G-phase.
- Public Rust API expansion. `_for_test` accessors only with
  `#[cfg(debug_assertions)]` per F.0 reviewer convention.

## 9. Failure interpretation

If G.0 + G.1 + G.2 + G.3 all land KEEP and AC-020 still does not
close: ship 0.6.0 with REQ-020 deferred (per the F.0 packet-close
position) and open Pack 7 with the cumulative G-phase evidence as
the input.

If telemetry instead surfaces a **new** bottleneck class (e.g. a
specific hot path in `vec0` that is amenable to a SQL or query-
plan tweak), the orchestrator may add a G.4 phase by appending to
this handoff and the STATUS phase-results table — same review +
TDD discipline applies.

## 10. Spawning interventions

Use the resume §4 spawn block with anti-chaining defenses. Per-
phase prompts live alongside this handoff:

- `dev/plan/prompts/G0-wal-checkpoint-telemetry.md`
- `dev/plan/prompts/G1-reader-lookaside.md` (authored after G.0)
- `dev/plan/prompts/G2-per-worker-stmt-cache.md` (authored after
  G.0 / G.1 KEEP)
- `dev/plan/prompts/G3-checkpoint-tuning.md` (authored after
  G.0 / G.1 / G.2)

Implementer model: Opus 4.7 high. Reviewer: codex `gpt-5.4` high.
Worktree pattern: `/tmp/fdb-pack6G-<phase>-<ts>`. Branch:
`pack6G-<phase>-<ts>`.

## 11. Pause points

- After G.0 returns. Orchestrator reviews telemetry JSON, picks
  next-phase ordering, and pauses for human review before
  spawning G.1.
- After each KEEP / REVERT decision (§3 update protocol in
  `01-orchestrator-resume.md`).
- After packet close (cumulative-vs-F.0 + cumulative-vs-Pack-5
  numbers in the final synthesis JSON).

## Update log

- 2026-05-04 — Authored from the user's "Best next opportunities
  on top of F.0" briefing. G.0 telemetry first; default ordering
  G.1 → G.2 → G.3; Pack 6 F.0 baseline `a00cd13` (seq 531 ms /
  conc 155 ms / speedup 3.49× / N=5).
