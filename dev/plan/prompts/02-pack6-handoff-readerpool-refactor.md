# Pack 6 handoff — Architectural ReaderPool refactor (lock-free)

Phase 9 Pack 6 continues the AC-020 closure work after Pack 5
closed ESCALATE on 2026-05-04. The human authorized the
architectural refactor in this handoff; no further approval
needed for the lock-free pool intervention itself.

## 1. Read order on resume

1. `dev/plan/runs/STATUS.md` — final Pack 5 state + branch tip.
2. `dev/plan/runs/final-synthesis-output.json` — Pack 5 close
   decision + Pack 6 starting point (`next_packet_pointer` field).
3. `dev/notes/performance-whitepaper-notes.md` §11 — Pack 5
   narrative incl. final synthesis paragraph + alternative pivots.
   §5 — full reverted-experiment ledger (B.1, C.1, E.1).
4. `dev/plan/0.6.0-Phase-9-Pack-5-performance-diagnostics.md`
   §12 — append-only experiment log.
5. `dev/plan/runs/A1-perf-capture-output.json` — load-bearing
   perf baseline (top symbols + folded paths).
6. `dev/plan/runs/A2-symbol-focus-output.json` — category
   classification (mutex_atomic 5.73× growth).
7. `dev/plan/prompts/01-orchestrator-resume.md` §4 — anti-chaining
   defenses (PREAMBLE + `--disallowedTools Task Agent` +
   `stream-json` log). **Apply on every spawn.**

## 2. State at this hand-off

- Branch: `0.6.0-rewrite`. Tip: `46f693a` (Pack 5 final synthesis).
  16 commits ahead of `origin/0.6.0-rewrite`.
- AC-020 N=5 medians (final Pack 5 reading): seq 184.7 ms,
  conc 124.0 ms, bound 34.6 ms, speedup 1.487× (required ≥ 5.33×
  test bound, ≥ 6.4× Pack 5 §1 1.25/8 margin).
- AC-017 + AC-018 green (note pre-existing flake under
  `cargo test --workspace` concurrent execution; standalone runs
  green — not Pack 6 territory).
- All worktrees clean (`git worktree list` shows main +
  `agent-phase10` only).
- Net production-code LoC delta from Pack 5 = 0. Diagnostic
  test additions (perf_gates.rs +316 LoC) under `#[ignore]`.

## 3. Pack 5 falsifications you do NOT retry

Per resume hard-rule §4.4 (no §5 retry without explicit override):

- **Runtime `sqlite3_config(SQLITE_CONFIG_MULTITHREAD)`** —
  ordering-correct (rc=SQLITE_OK proven); AC-020 flat. §5 +
  §11 B.1 #2 entry.
- **Compile-time `SQLITE_THREADSAFE=2`** rebuild —
  `sqlite3_threadsafe()==2` verified; AC-020 still flat. §5 +
  §11 C.1 entry. Strongest possible mutex intervention; both
  runtime and compile-time paths exhausted.
- **Read-path `prepare_cached`** on the 4 search statements —
  parse cost relief was real (seq −13.7%) but concurrent
  unchanged; codex reviewer BLOCK on contract + decision-rule
  mismatch. §5 + §11 E.1 entry.

Pack 6 attacks a different layer — the Rust-side / pool-side
contention.

## 4. Authorized intervention

**E.0 — Architectural reader-pool refactor (lock-free
`crossbeam::queue::ArrayQueue<Connection>`).** Replaces
`ReaderPool::Mutex<Vec<Connection>>` at
`src/rust/crates/fathomdb-engine/src/lib.rs:154`. Single-threaded
borrow + release become wait-free.

Hypothesis (carried from Pack 5):

- A.2 mutex_atomic 36.98% concurrent share. C.1 ruled out the
  SQLite-internal mutex. The pthread / aarch64-CAS cycles must
  come from rusqlite-side or our pool-side Mutex (or WAL
  atomics, which is the last-resort fallback).
- `Mutex<Vec<Connection>>` adds 2× CAS per query (borrow +
  release). At ~1600 queries × 8 threads = 12 800 acquire/release
  pairs in the AC-020 workload; under contention each pair can
  cost > 100 ns of CAS spin.
- Replacing with `ArrayQueue` (single-producer-single-consumer
  free-slot ring) makes the borrow path wait-free. Expected to
  drop the `___pthread_mutex_lock` 6.8% + `lll_mutex_lock_optimized`
  1.8% concurrent share to near-zero, and to reduce the
  `__aarch64_swp4_rel` / `_cas4_acq` share by however much of
  it sits on the pool slot.

Required ordering (do not skip):

1. **A.1 re-capture against current tip (no source change)** —
   confirm the conc spinlock symbols still look like the A.1
   classification (mutex_atomic dominant). If the symbols have
   shifted, the pool-mutex hypothesis is weakened; re-classify
   before writing code.
2. **Read pool implementation** at `lib.rs:154` —
   `ReaderPool::borrow` + `release`. Trace every callsite that
   holds the pool Mutex; confirm the Mutex is ONLY held for
   slot-pop / slot-push (no work under the lock).
3. **Replacement design**: `crossbeam::queue::ArrayQueue<Box<Connection>>`
   with capacity = `READER_POOL_SIZE` (= 8). Borrow = `pop()`;
   release = `push()`. Connection ownership is by `Box<Connection>`
   to match the `Vec<Connection>` move-semantics today. If the
   queue is empty (impossible under invariant N borrowers ≤ N
   slots but defensively), block on a `CondVar` or just spin —
   the existing impl spins or signals; replicate equivalent
   semantics.
4. **Cargo.toml change**: add `crossbeam-queue = "0.3"`
   (workspace dep if other crates need it; otherwise crate-local).
   Reviewer must verify cross-platform compatibility (aarch64-Linux,
   x86_64-Linux, darwin-arm64).
5. **Test** (red-green-refactor):
   - Red: write a stress test that runs 8 threads × 200 borrow/
     release pairs with `assertEq!(pool_size_after, READER_POOL_SIZE)`.
     This passes today by chance under Mutex; under ArrayQueue
     it must continue to pass without the Mutex.
   - Green: implement the refactor.
   - Refactor: ensure `Connection::Drop` is still called on
     pool teardown (pool's `Drop` must drain the queue and drop
     each Connection).
6. **Decision rule (numeric, mirrors B.1/C.1/E.1 form)**:
   - **KEEP** iff `concurrent_median_ms ≤ 80` AND `speedup ≥ 5.0×`
     AND AC-018 green (= ≥ 30% drop from A.1 baseline 115 ms).
   - **INCONCLUSIVE** band 80–100 ms → re-capture A.1 against
     the E.0 branch and re-classify; remaining options are WAL
     atomics (last-resort) or rusqlite-side Mutex (replace
     rusqlite with raw libsqlite3-sys + manual handle, large
     refactor).
   - **REVERT** iff `concurrent_median_ms > 115` OR AC-018 red
     OR ArrayQueue invariant violated (drop count < pool size).
7. **Reviewer (codex `gpt-5.4`) MANDATORY.** Cross-platform
   Cargo.toml change + ownership-semantics swap. Reviewer must
   confirm: ownership transfer correctness, Connection drop on
   pool teardown, no new `pub` Rust public-surface expansion
   without `dev/interfaces/rust.md` update + ADR per
   AGENTS.md §1, no `c_char` / `c_int` hardcoding, and the AC-018
   drain timing is unchanged.

Kill criterion (per A.4 lineage extended): if E.0 lands
config-rc-equivalent proof (queue invariant + cycle-share
drop in re-capture) but AC-020 still > 100 ms concurrent, the
last-surviving hypothesis is **WAL shared-memory atomics**
unaffected by any pool / connection-side change. At that point
either: (a) accept AC-020 as not closable in 0.6.0 and defer;
(b) escalate to a SQLite WAL2 upgrade (libsqlite3-sys-0.30
doesn't carry SQLite 3.45+; would need vendor patch); or (c)
split readers/writers across separate database files
(architectural and breaks cross-table consistency).

## 5. Spawning E.0

Use the resume §4 spawn block (PREAMBLE + `--disallowedTools
Task Agent` + `stream-json` log). Implementer model: Opus 4.7
high. Reviewer (codex `gpt-5.4` high) mandatory after E.0
returns KEEP/INCONCLUSIVE. Skip reviewer on REVERT (no diff).

Worktree pattern: `/tmp/fdb-pack6-E0-readerpool-arrayqueue-<ts>`.
Branch: `pack6-E0-readerpool-arrayqueue-<ts>`. Baseline:
`0.6.0-rewrite` tip (currently `46f693a`).

After spawn, follow resume §3 decision loop (read output JSON
→ read reviewer verdict → KEEP/REVERT/INCONCLUSIVE → update
STATUS.md / plan §12 / whitepaper §11 / next-prompt update log
→ stage staged commits via FF-merge after rebase).

## 6. Out-of-scope for Pack 6 (defer to later packet)

- E.2 (`canonical_nodes(write_cursor)` index) — schema migration,
  out of scope per resume §4.6 unless Pack 6 expands scope
  explicitly.
- D.1 (single-statement UNION refactor) — only if E.0 falsifies
  the pool-mutex hypothesis AND the human re-authorizes a larger
  refactor.
- Replacing rusqlite with raw `libsqlite3-sys` — last-resort
  scope-blowup; do not pursue without a separate ADR.
- AC-020 bound contract refinement (whitepaper §8 q2) — only
  worth it after E.0 closure or formal AC-020 deferral; either
  outcome unblocks the discussion.

## 7. Pause points

- After A.1 re-capture (read-only). Confirm classification before
  code change. **Pause for human ack here.**
- After E.0 implementer returns. Run reviewer if KEEP/INCONCLUSIVE.
- After reviewer verdict. KEEP/REVERT decision with bookkeeping.
- After packet close (Pack 6 final synthesis).

Auto-mode continuation past A.1 re-capture allowed only with
explicit human re-authorization (the "approval for architectural
refactor" in this handoff covers the *intervention* but not the
A.1 ↔ implementer auto-spawn cadence).

## 8. Success definition

- AC-020 passes `concurrent ≤ sequential × 1.25/8` over 5
  consecutive `AGENT_LONG=1` runs (20% margin); OR
- AC-020 formally deferred with documented evidence (REQ-020
  marked "deferred" in `dev/test-plan.md` + whitepaper §11
  Pack 6 narrative explaining the architectural ceiling).
- AC-017 + AC-018 green on the same runs.
- Net production LoC delta: prefer net-negative or net-zero per
  `feedback_reliability_principles.md`. The pool refactor itself
  is roughly LoC-neutral; if E.0 KEEPs, delete `init_sqlite_runtime()`
  scaffolding from Pack 5 attempt B.1 (it's already reverted, so
  this is a clean state — confirm).
- Whitepaper §11 closing paragraph reads as a self-contained
  explanation a future paper could lift.

## Update log

- 2026-05-04 — Handoff written immediately after Pack 5 final
  synthesis (`46f693a`). Pack 6 baseline = `46f693a` (= current
  `0.6.0-rewrite` tip). The "complete the work" + Pack 6
  authorization came from the same human session that ran Pack 5.
- A.1 re-capture is the first action in Pack 6, **not** E.0
  implementation. Do not skip the re-capture: Pack 5 evidence
  is from `fec71a0` and may have drifted (B.1/C.1/E.1 all
  reverted but environmental drift was observed at +5–6 ms).
