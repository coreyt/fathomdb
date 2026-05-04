# Phase 9 Pack 5 — status board

Single up-to-date progress file for the AC-020 perf packet. Orchestrator
(main thread) updates this file at every plan §0.1 step-5 decision
point. Implementer subagents do **not** edit this file — they write
`<phase>-output.json` instead, which the orchestrator reads.

Last updated: 2026-05-03 (B.1 attempt #1 BLOCKER — sqlite3_threadsafe()==2 spec impossible; prompt re-framed to config_rc==SQLITE_OK; ready to re-spawn B.1).

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
- Active phase: **none** — A.0 / A.1 / A.2 / A.3 / A.4 closed;
  B.1 attempt #1 BLOCKER (no commit, worktree cleaned). B.1 prompt
  re-framed (`sqlite_runtime_config_rc() == 0` replaces
  `sqlite3_threadsafe() == 2`); ready to re-spawn.
- Active worktrees: none.

## Acceptance scoreboard

| Gate   | Required                                  | Latest reading                                | Status |
| ------ | ----------------------------------------- | --------------------------------------------- | ------ |
| AC-017 | green                                     | green (whitepaper §10)                        | green  |
| AC-018 | green; no regression > 10 % vs baseline   | green (whitepaper §10)                        | green  |
| AC-020 | `concurrent <= sequential * 1.25 / 8`, x5 | seq 456 / conc 127 / bound 85 / speedup 3.59x | red    |

Bound for AC-020 in this packet is the §1 20%-margin form
(`1.25 / 8` ≈ 0.156), tighter than the test's literal `1.5 / 8`. The
test bound stays untouched (hard rule §4.1); this score reflects the
packet's acceptance criterion.

## Phase results

| Phase | Spawned | Decision | Reviewer | Worktree | Commit | Notes / log             |
| ----- | ------- | -------- | -------- | -------- | ------ | ----------------------- |
| A.0   | 2026-05-03 | KEEP  | n/a (test-only) | cleaned | `fec71a0` | harness split; smoke seq=184/conc=117 N=1; output JSON `dev/plan/runs/A0-harness-split-output.json` |
| A.1   | 2026-05-03 | KEEP  | n/a (capture)   | cleaned | `ca0d8f0` | perf record N=5; seq median 182ms / conc 115ms / speedup 1.58×; flamegraphs in `dev/notes/perf/ac020-*-fec71a0.{svg,folded}`; phase JSON self-marked INCONCLUSIVE per A.1 capture-only mandate, orchestrator KEPT |
| A.2   | 2026-05-03 | PICK_B1 | self (main thread Opus) | n/a   | (no code) | mutex_atomic 6.45%→36.98% (5.73× growth, +262M cycles) — dominant; allocator 2× secondary; rest flat/shrinking. Output JSON `dev/plan/runs/A2-symbol-focus-output.json`. |
| A.3   | 2026-05-03 | PARTIAL_KEEP | n/a (diag) | cleaned | `edb0c84` | counters search_us=542/query, embedder=0; THREADSAFE=1 (MUTEX_PTHREADS) confirms A.2; strace skipped (no sudo); EXPLAIN no regressions, latent canonical_nodes missing-index flagged out-of-scope |
| A.4   | 2026-05-03 | PICK_B1 | self (main thread Opus) | n/a   | (no code) | §5 OVERRIDE on prior MULTITHREAD revert (pre-init placement + return-code validation + threadsafe()==2 assertion test required); rule conc≤80ms AND speedup≥5×; alt-on-fail=B.3; kill: B.1+B.3 stacked <10% drop ⇒ promote D.1. Output `dev/plan/runs/A4-decision-record-output.json`. |
| B.1   | 2026-05-03 (#1) | BLOCKER | n/a | cleaned | none | spec assertion `sqlite3_threadsafe()==2` impossible (compile-time constant per `sqlite3.h:249-252`); `config_rc=SQLITE_OK=0` proven (vs §5's `SQLITE_MISUSE=21`); implementer reverted per STOP-and-report; prompt re-framed for re-spawn |
| B.2   | -       | -        | -        | -        | -      | conditional on B.1 KEEP |
| B.3   | -       | -        | -        | -        | -      | conditional             |
| C.1   | -       | -        | -        | -        | -      | conditional             |
| D.1   | -       | -        | -        | -        | -      | parallel track          |
| final | -       | -        | -        | -        | -      | -                       |

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

  | Category      | Seq %  | Conc % | Ratio |
  | ------------- | ------ | ------ | ----- |
  | mutex_atomic  | 6.45   | 36.98  | 5.73× |
  | allocator     | 1.60   | 3.20   | 2.00× |
  | page_cache    | 1.64   | 1.46   | 0.89× |
  | vec0_fts      | 24.12  | 11.43  | 0.47× |
  | sql_parse     | 10.08  | 7.07   | 0.70× |
  | our_code      | 0.52   | 0.17   | 0.33× |

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

## Outstanding worktrees

_(none — populate when `git worktree add` succeeds; remove on cleanup)_

## Open concerns / overrides

_(none yet — anything CONCERN-severity from reviewer goes here with §12 ref)_

## Next action

Pre-write all phase prompt files (plan §10 step 1) → **DONE**.
Land Phase 9 Pack 1-4 baseline → **DONE** (`1980bf6`).
Phase A.0 KEEP `fec71a0` → A.1 KEEP `ca0d8f0` → A.2 PICK_B1 → A.3
PARTIAL_KEEP `edb0c84` → A.4 PICK_B1 OVERRIDE — all DONE.

B.1 attempt #1 returned BLOCKER on the `sqlite3_threadsafe()==2`
spec assertion (impossible per SQLite header `sqlite3.h:249-252`).
Prompt + A.4 mandate + whitepaper §7.3/§11 corrected: gate is now
`sqlite_runtime_config_rc() == 0` (real §5 differentiator).

**Re-spawn B.1** (Opus high, reviewer codex `gpt-5.4` mandatory)
from `0.6.0-rewrite` tip after this bookkeeping commit lands.
Pause for human confirmation per §8 A.4 gate before re-spawn.

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
