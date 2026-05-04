# Phase 9 Pack 5 / Pack 6 — status board

Single up-to-date progress file for the AC-020 perf packet. Orchestrator
(main thread) updates this file at every plan §0.1 step-5 decision
point. Implementer subagents do **not** edit this file — they write
`<phase>-output.json` instead, which the orchestrator reads.

Last updated: 2026-05-04 (Pack 6 OPEN — F.0 thread-affine reader workers
queued; Pack 5 ESCALATE record below for history).

---

## Pack 6 current state

- Branch: `0.6.0-rewrite`. Tip: `de4810a` at F.0 spawn time
  (handoff + STATUS Pack 6 OPEN + F.0 implementer prompt landed).
- Active phase: **F.0 CLOSED — REVERT** (2026-05-04). Pack 6
  ESCALATE pending human decision between handoff §10 (a) defer,
  (b) WAL2/vendor-SQLite, (c) reader/writer physical separation.
- F.0 worktree: `/tmp/fdb-pack6-F0-thread-affine-readers-20260504T115216Z`
  (un-merged branch `pack6-F0-thread-affine-readers-20260504T115216Z`,
  worktree commit `07388cf`). Branch retained for audit; not merged.
- Decision rule met: conc_median 155 ms > 100 ms REVERT bound;
  speedup 3.49× < 5.0× KEEP threshold; AC-018 green (drain 56 ms);
  shutdown + routing invariants pass; no Rust-side hot-path mutex
  remaining.
- Reviewer skipped per handoff §8 (mandatory only on KEEP /
  INCONCLUSIVE; REVERT does not require codex pass).
- Pack 6 phase results:

| Phase | Spawned       | Decision | Reviewer  | Worktree                                         | Notes / log                                                                                                                              |
| ----- | ------------- | -------- | --------- | ------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------- |
| F.0   | 2026-05-04    | REVERT   | n/a       | un-merged branch retained, worktree dir to clean | thread-affine reader workers; speedup 1.49→3.49× (+2.34×); conc 155 ms > 100 ms bound. Output `dev/plan/runs/F0-thread-affine-readers-output.json`. Implementer log `dev/plan/runs/F0-thread-affine-readers-20260504T115216Z.log`. |

## Pack 6 latest measurements

- 2026-05-04 F.0 N=5 raw (release, AGENT_LONG=1):
  - sequential `[530, 549, 531, 541, 518]` ms — median 531, stddev 11.17.
  - concurrent `[157, 155, 152, 155, 164]` ms — median 155, stddev 4.27.
  - bound `[99, 102, 99, 101, 97]` ms — median 99.
  - speedup `[3.376, 3.542, 3.493, 3.490, 3.159]` — median 3.490, stddev 0.137.
  - AC-017 green; AC-018 drain 56 ms (green); 4 new
    `tests/reader_pool.rs` integration tests green; clippy + fmt
    clean; pre-existing release-build cargo-test compile gap on
    `compatibility/cursors/lifecycle_observability` (uses
    `#[cfg(debug_assertions)]` helpers absent in release) is
    pre-F.0 and not introduced by Pack 6.
  - Host caveat: sequential timings ~3× Pack 5's reported 184 ms on
    this worktree machine; compare speedup ratios across packs.

---

## Pack 5 closed state (history)

---

## Current state

- Branch: `0.6.0-rewrite`.
- Branch tip: `ca0d8f0` (diag(perf-gates): A.1 perf capture artifacts,
  FF-merged from `pack5-A1-perf-capture-20260504T003956Z` after
  rebase onto `522a88d`). Not yet pushed. Prior local commits:
  `522a88d` A.0 bookkeeping; `fec71a0` A.0 harness split;
  `fc8b8d8` docs alignment; `2dc2134` STATUS refresh;
  `1980bf6` Pack 1-4 production.
- A.0 spawn baseline (used): `0.6.0-rewrite` ref at spawn time =
  `fc8b8d8` (descendant of plan-recorded baseline `1980bf6`).
- A.1 spawn baseline (used): `fec71a0` (A.0 commit). FF applied
  after rebase onto `522a88d`.
- A.2 executed by main thread at baseline `ca0d8f0`; output JSON
  `dev/plan/runs/A2-symbol-focus-output.json`; no commit (docs +
  JSON only, bundled with bookkeeping commit).
- A.3 spawn baseline (used): `0.6.0-rewrite` at `3bbb9a1`. KEPT
  as `edb0c84` (test-code commit FF-merged); evidence + output JSON
  written to main repo by subagent via absolute path (kept as-is).
- A.4 executed by main thread at `0.6.0-rewrite` tip; output JSON
  `dev/plan/runs/A4-decision-record-output.json`.
- B.1 spawn baseline: **`0.6.0-rewrite`** tip (current after A.4
  bookkeeping commit). Reviewer (codex `gpt-5.4`) mandatory.
- Baseline drift note: original Pack 5 plan assumed a clean baseline
  with Pack 1-4 already committed, but those changes were sitting in
  the working tree. They were committed at `1980bf6` after running
  `agent-verify.sh` green at that tree. No production changes
  authored in this resume; the commit is a clerical land of existing
  WT state.
- Pre-flight: PASS — see `dev/plan/runs/preflight-summary.md`. (HEAD
  drifted from `da9ae05` to `1980bf6` since pre-flight; no preflight
  amendment required because none of the seven checks depend on the
  engine src state.)
- Prompts: PASS — 13 files under `dev/plan/prompts/`.
- Active phase: **none** — A.0 / A.1 / A.2 / A.3 / A.4 / B.1 / C.1 /
  E.1 all closed. Mutex track CLOSED (B.1+C.1 REVERT). Parse track
  CLOSED (E.1 REVERT per reviewer BLOCK; sequential improved but
  concurrent unchanged + bound tightened). Residual is
  architectural (rusqlite-side / ReaderPool Mutex / WAL atomics).
  Final synthesis next.
- Active worktrees: none.
- Anti-chaining defenses (resume §4 update at `fc3dda3`) verified
  WORKING on B.1 #2 + C.1: PREAMBLE prepended via stdin,
  `--disallowedTools Task Agent`, `--output-format stream-json
--include-partial-messages --verbose`. Single coherent agent;
  no Task spawns; mid-flight monitoring via stream events. Keep
  on for all subsequent spawns.

## Acceptance scoreboard

| Gate   | Required                                  | Latest reading                                                   | Status |
| ------ | ----------------------------------------- | ---------------------------------------------------------------- | ------ |
| AC-017 | green                                     | green (whitepaper §10)                                           | green  |
| AC-018 | green; no regression > 10 % vs baseline   | green (whitepaper §10)                                           | green  |
| AC-020 | `concurrent <= sequential * 1.25 / 8`, x5 | seq 184.7 / conc 124.0 / bound 34.6 / speedup 1.487× (N=5 final) | red    |

Bound for AC-020 in this packet is the §1 20%-margin form
(`1.25 / 8` ≈ 0.156), tighter than the test's literal `1.5 / 8`. The
test bound stays untouched (hard rule §4.1); this score reflects the
packet's acceptance criterion.

## Phase results

| Phase | Spawned            | Decision     | Reviewer                | Worktree | Commit                                                      | Notes / log                                                                                                                                                                                                                                                                                                              |
| ----- | ------------------ | ------------ | ----------------------- | -------- | ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| A.0   | 2026-05-03         | KEEP         | n/a (test-only)         | cleaned  | `fec71a0`                                                   | harness split; smoke seq=184/conc=117 N=1; output JSON `dev/plan/runs/A0-harness-split-output.json`                                                                                                                                                                                                                      |
| A.1   | 2026-05-03         | KEEP         | n/a (capture)           | cleaned  | `ca0d8f0`                                                   | perf record N=5; seq median 182ms / conc 115ms / speedup 1.58×; flamegraphs in `dev/notes/perf/ac020-*-fec71a0.{svg,folded}`; phase JSON self-marked INCONCLUSIVE per A.1 capture-only mandate, orchestrator KEPT                                                                                                        |
| A.2   | 2026-05-03         | PICK_B1      | self (main thread Opus) | n/a      | (no code)                                                   | mutex_atomic 6.45%→36.98% (5.73× growth, +262M cycles) — dominant; allocator 2× secondary; rest flat/shrinking. Output JSON `dev/plan/runs/A2-symbol-focus-output.json`.                                                                                                                                                 |
| A.3   | 2026-05-03         | PARTIAL_KEEP | n/a (diag)              | cleaned  | `edb0c84`                                                   | counters search_us=542/query, embedder=0; THREADSAFE=1 (MUTEX_PTHREADS) confirms A.2; strace skipped (no sudo); EXPLAIN no regressions, latent canonical_nodes missing-index flagged out-of-scope                                                                                                                        |
| A.4   | 2026-05-03         | PICK_B1      | self (main thread Opus) | n/a      | (no code)                                                   | §5 OVERRIDE on prior MULTITHREAD revert (pre-init placement + return-code validation + threadsafe()==2 assertion test required); rule conc≤80ms AND speedup≥5×; alt-on-fail=B.3; kill: B.1+B.3 stacked <10% drop ⇒ promote D.1. Output `dev/plan/runs/A4-decision-record-output.json`.                                   |
| B.1   | 2026-05-03 (#1+#2) | REVERT       | skipped (no diff)       | cleaned  | `d448263` (JSON only)                                       | #1 BLOCKER on impossible `sqlite3_threadsafe()==2` spec; #2 REVERT — `config_rc=SQLITE_OK` proven (vs §5's `21`), but AC-020 conc 115→120.6ms (+4.9%, +1.7σ), speedup 1.58→1.526× — runtime CONFIG_MULTITHREAD provably applied-but-didn't-help. Promotes C.1. Output `dev/plan/runs/B1-multithread-wiring-output.json`. |
| B.2   | -                  | -            | -                       | -        | -                                                           | conditional on B.1 KEEP                                                                                                                                                                                                                                                                                                  |
| B.3   | -                  | -            | -                       | -        | -                                                           | conditional                                                                                                                                                                                                                                                                                                              |
| C.1   | 2026-05-03         | REVERT       | skipped (no diff)       | cleaned  | `15c6473` (JSON+evidence)                                   | THREADSAFE=2 verified live (sqlite3_threadsafe()==2 + PRAGMA both green pre-revert), AC-020 conc 115→121.5ms (+5.65%, ~1.2σ), speedup 1.58→1.509× (-4.48%). Hot symbols are NOT SQLite threading-mode mutexes (likely WAL atomics). Mutex track CLOSED.                                                                  |
| E.1   | 2026-05-03         | REVERT       | BLOCK (codex gpt-5.4)   | cleaned  | reverted via `1739b17`+`3e047a3` (orig `e4ff255`+`91c69e9`) | `prepare_cached` on 4 read stmts; seq -13.7% (parse-cost relief real) but conc unchanged + bound tightened (speedup 1.58→1.266×); reviewer flagged surface-contract + decision-rule mismatch + partial test coverage. Pivot: residual conc contention is upstream of parse cost — architectural.                         |
| B.3   | -                  | -            | -                       | -        | -                                                           | SKIPPED (mutex track closed by C.1)                                                                                                                                                                                                                                                                                      |
| D.1   | -                  | -            | -                       | -        | -                                                           | SKIPPED in-packet (architectural; out of Pack 5 scope)                                                                                                                                                                                                                                                                   |
| E.3   | -                  | -            | -                       | -        | -                                                           | SKIPPED (§5 already noted cache_size doesn't move conc ratio)                                                                                                                                                                                                                                                            |
| E.4   | -                  | -            | -                       | -        | -                                                           | SKIPPED (writer-side; workload is read-heavy)                                                                                                                                                                                                                                                                            |
| final | 2026-05-03         | ESCALATE     | n/a                     | n/a      | this commit                                                 | Pack 5 CLOSED; AC-020 not met (speedup 1.487× vs req 5.33×); mutex+parse tracks exhausted; Pack 6 architectural reader-pool refactor recommended. Output `dev/plan/runs/final-synthesis-output.json`.                                                                                                                    |
| D.1   | -                  | -            | -                       | -        | -                                                           | parallel track                                                                                                                                                                                                                                                                                                           |
| final | -                  | -            | -                       | -        | -                                                           | -                                                                                                                                                                                                                                                                                                                        |

Decision values: `KEEP` / `REVERT` / `INCONCLUSIVE` / `RECAPTURE` /
`SKIPPED`. Reviewer values: `PASS` / `CONCERN` / `BLOCK` / `n/a`.

## Latest measurements (N=5 unless noted)

- 2026-05-03 A.0 smoke (N=1, NOT a gate reading): split harness
  seq=184ms, conc=117ms; combined gate at same tree
  seq=182ms / conc=118ms / bound=34ms / speedup=0.19. Numbers
  consistent within noise → fixture parity confirmed. Combined-gate
  bound failure was pre-existing (not introduced by A.0).
- 2026-05-03 A.1 N=5 (perf record `cycles:u`, `-F 999 -g
--call-graph dwarf`):
  - sequential: `[189,199,182,179,176]` ms — min 176, median 182,
    max 199, stddev 9.2.
  - concurrent: `[120,110,117,115,112]` ms — min 110, median 115,
    max 120, stddev 4.0.
  - speedup_observed = 1.58× (required 5.33×; gap 3.4×).
  - Concurrent profile cycle distribution: ~30% in atomic/mutex
    primitives (`__aarch64_swp4_rel` 11.2%, `__aarch64_cas4_acq`
    9.8%, `___pthread_mutex_lock` 6.8%, `__aarch64_swp4_acq` 5.9%,
    `lll_mutex_lock_optimized` 1.8%) vs ~5% in sequential.
  - Useful work fraction (`min_idx` + `vec0Filter_*`) drops
    14.5% → 8.7% under concurrency.
  - Independent finding both profiles: `sqlite3RunParser` 4.6%
    sequential / 3.4% concurrent — no prepared-statement cache.
- 2026-05-03 A.2 category-aggregated classification (% total cycles,
  same A.1 folded files):

  | Category     | Seq % | Conc % | Ratio |
  | ------------ | ----- | ------ | ----- |
  | mutex_atomic | 6.45  | 36.98  | 5.73× |
  | allocator    | 1.60  | 3.20   | 2.00× |
  | page_cache   | 1.64  | 1.46   | 0.89× |
  | vec0_fts     | 24.12 | 11.43  | 0.47× |
  | sql_parse    | 10.08 | 7.07   | 0.70× |
  | our_code     | 0.52  | 0.17   | 0.33× |

  mutex_atomic absolute delta = +262M cycles (largest of any
  category). Decision rule met → PICK_B1.

- 2026-05-03 A.3 counters (1600 concurrent queries, 8×50×4):
  - `search_us_per_query` = 542 µs
  - `embedder_us_per_query` = 0 µs (RoutedEmbedder fixture)
  - `proxy_borrow_plus_read_us_per_query` = 542 µs (split needs hook)
- 2026-05-03 A.3 SQLite config: `sqlite3_threadsafe()` = `1`
  (SERIALIZED, `MUTEX_PTHREADS`); `SYSTEM_MALLOC`;
  `DEFAULT_MMAP_SIZE=0`; `DEFAULT_CACHE_SIZE=-2000`. Confirms
  A.2 verdict.
- 2026-05-03 A.4 baseline-of-record for B.1: seq_median=182ms,
  conc_median=115ms, bound=34ms (combined-gate 1.5/8 form),
  speedup=1.58×, n=5. Decision rule numeric: KEEP iff
  conc_median≤80 AND speedup≥5.0 AND AC-018 green.
- 2026-05-03 B.1 #2 N=5 raw (release, AGENT_LONG=1):
  - sequential `[181.5, 178.1, 189.0, 184.0, 186.0]` ms — median
    184.0, stddev 3.73.
  - concurrent `[121.6, 115.1, 124.0, 120.6, 118.8]` ms — median
    120.6, stddev 2.98.
  - speedup `[1.493, 1.547, 1.524, 1.526, 1.566]` — median 1.526,
    stddev 0.025.
  - bound (combined-gate `1.5/8` form, recorded for parity)
    median 34.5 ms.
  - sqlite3_config rc=0 (SQLITE_OK), shutdown=0, initialize=0.
  - AC-017 + AC-018 green. AC-020 numeric REVERT.
- 2026-05-03 baseline-of-record for **C.1** = A.1 baseline directly
  (B.1 was REVERT, not KEPT — sequential 182, concurrent 115,
  speedup 1.58, n=5).
- 2026-05-03 C.1 N=5 raw (release, AGENT_LONG=1, THREADSAFE=2):
  - sequential `[185.8, 179.7, 182.2, 184.4, 181.1]` ms — median 182.2,
    stddev tight.
  - concurrent `[116.7, 126.8, 128.4, 121.5, 120.0]` ms — median 121.5,
    stddev 4.8.
  - speedup median 1.509×; AC-018 drain 220 ms (green).
  - Build: `LIBSQLITE3_FLAGS="-USQLITE_THREADSAFE -DSQLITE_THREADSAFE=2"`
    in `.cargo/config.toml`; clean build 80.7s; binary 1.17 MB.
- 2026-05-03 baseline-of-record for **E.1** = A.1 baseline directly
  (C.1 also REVERT, not KEPT). Same N=5 numbers as B.1/C.1 used.

## Outstanding worktrees

_(none — populate when `git worktree add` succeeds; remove on cleanup)_

## Open concerns / overrides

_(none yet — anything CONCERN-severity from reviewer goes here with §12 ref)_

## Next action

Pre-write all phase prompt files (plan §10 step 1) → **DONE**.
Land Phase 9 Pack 1-4 baseline → **DONE** (`1980bf6`).
Phase A.0 KEEP `fec71a0` → A.1 KEEP `ca0d8f0` → A.2 PICK_B1 → A.3
PARTIAL_KEEP `edb0c84` → A.4 PICK_B1 OVERRIDE — all DONE.

**Pack 5 CLOSED — ESCALATE.** AC-020 not met. Final-synthesis
output at `dev/plan/runs/final-synthesis-output.json`. Whitepaper
§11 carries the closing narrative + Pack 6 starting point.

**Pack 6 ESCALATE — F.0 REVERT.** Thread-affine reader workers
collapsed the Rust-side ReaderPool / cross-thread-handoff component
(speedup 1.49×→3.49×, +2.34×) but did not close AC-020 (conc
155 ms > 100 ms bound; speedup 3.49× < 5.0× KEEP threshold). Per
handoff §10 the residual ceiling is now WAL/SQLite-internal.

Recommended next action (human decision per handoff §10):

- (a) Formal AC-020 deferral with the Pack 5 + Pack 6 evidence
  chain attached; ship 0.6.0 with REQ-020 marked deferred in
  `dev/test-plan.md`. Implementer-recommended option.
- (b) WAL2 / vendor-SQLite path. Higher LoC + risk; not yet in
  `libsqlite3-sys-0.30`.
- (c) Reader/writer physical separation. Larger redesign.

The smallest-radius remaining experiment is NOT another pool change.
Pack 5 closed mutex+parse; Pack 6 F.0 closed pool topology.

---

## Update protocol

1. After implementer subagent returns: read its `<phase>-output.json`.
2. After reviewer (codex) returns: read its `<phase>-review-<ts>.md`.
3. Orchestrator decides KEEP / REVERT / INCONCLUSIVE.
4. **Edit this file**:
   - Update "Active phase" / "Current state".
   - Fill the matching row in "Phase results".
   - Append median / min / max numbers to "Latest measurements".
   - Update "Outstanding worktrees" (add on spawn, remove on cleanup).
   - Add any reviewer CONCERN to "Open concerns / overrides".
   - Update "Next action".
5. Append §12 line in the plan file (one-line audit trail).
6. Append §11 narrative in the whitepaper notes (only on KEEP).
7. Update next prompt's `## Update log` with the just-decided numbers.
