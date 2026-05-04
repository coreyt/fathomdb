# Phase 9 Pack 6.G — G.0 WAL / checkpoint telemetry pass (implementer)

You are the **implementer** for Pack 6.G phase G.0. Read-only
diagnostics on the F.0 baseline. The orchestrator (main thread)
spawned you. Do the work below, write the output JSON, commit
inside this worktree if you land an `#[ignore]`-gated counter
test, then exit. Do **not** spawn other agents.

Authoritative spec:
`dev/plan/prompts/04-pack6G-handoff-canonical-sqlite-tuning.md`
sections §4 (G.0 row) + §6 + §7. If anything conflicts, the
handoff wins.

## 1. Read order before measuring

1. `dev/plan/prompts/04-pack6G-handoff-canonical-sqlite-tuning.md`
   (full).
2. `dev/plan/runs/F0-thread-affine-readers-output.json` — F.0
   numeric baseline + worker pool topology.
3. `dev/plan/runs/A1-perf-capture-output.json` +
   `dev/plan/runs/A2-symbol-focus-output.json` — pre-F.0 perf
   capture shape; reuse the same fold/grouping pattern where it
   still applies, extend where the topology changed.
4. `dev/notes/performance-whitepaper-notes.md` §5 — do-not-retry
   list. G.0 measures only; it does not retry §5 levers in
   production.
5. Engine code:
   - `crates/fathomdb-engine/src/lib.rs` — F.0 worker pool
     dispatch (`AtomicUsize::fetch_add` round-robin), worker
     thread bodies, `read_search_in_tx`, profile callback path,
     `Engine::open` reader connection pragmas.
   - `tests/perf_gates.rs` — AC-017 / AC-018 / AC-020 split
     harness (`ac_020_*` sub-phase entry points).

## 2. Mandate (handoff §4 G.0 row)

Re-capture AC-020 perf evidence on the F.0 tip, classify residual
hot symbols, and pick the strongest single lever among G.1 / G.2 /
G.3 for the orchestrator's next phase.

Required deliverables:

- AC-020 N=5 medians on the current tip (`AGENT_LONG=1`, release).
  These are the **post-F.0 baseline-of-record** for G.1 / G.2 /
  G.3.
- `perf record -F 999 -g --call-graph dwarf` capture for at least
  one sequential and one concurrent AC-020 run. Folded stack
  output under `dev/notes/perf/ac020-{seq,conc}-<commit-sha>.{svg,
folded}`.
- Symbol grouping JSON with these category buckets (extend pattern
  from `A2-symbol-focus-output.json`):
  - `wal_atomics` — `walIndexMap`, `walFrames`, `walRead*`,
    `walTryBeginRead`, `walRestartLog`, low-level CAS / atomic
    primitives reachable from `pager.c` / `wal.c`.
  - `checkpoint` — `sqlite3WalCheckpoint`,
    `sqlite3PagerCheckpoint`, `walCheckpoint`, busy-handler
    interaction.
  - `allocator_lookaside` — `sqlite3MallocSize`,
    `sqlite3DbMallocRawNN`, `sqlite3_release_memory`, lookaside
    bucket fast paths if visible at the symbol level.
  - `parse_compile` — `sqlite3RunParser`, `sqlite3VdbePrepare`,
    `sqlite3LockAndPrepare`. Should be visible if E.1's parse-cost
    hypothesis still applies at F.0 thread-affine connections.
  - `vec0_fts` — `vec0Filter_*`, `min_idx`, FTS hot symbols
    (carry from A.2).
  - `our_code` — engine module, worker dispatch, channel send /
    recv frames.
  - `mutex_atomic` — Rust-side or libc-side mutex / atomic
    primitives. Should be much smaller than Pack 5 A.2's 36.98%
    if F.0 did its job.
  - `other` — everything else.

For each bucket: `seq_pct`, `conc_pct`, `ratio`, `delta_cycles`.

- Pick a strongest-lever recommendation:
  - If `wal_atomics + checkpoint > 25%` of conc cycles: pick G.3
    or escalate to Pack 7 (handoff §9 path).
  - If `allocator_lookaside > 8%` of conc cycles or shows the
    documented allocator pressure pattern: pick G.1 first.
  - If `parse_compile > 5%` of conc cycles or scales with query
    count: pick G.2 first.
  - Otherwise: pick the bucket with the largest absolute conc-vs-
    seq delta and explain.

## 3. Optional `#[ignore]`-gated counter test

If a tiny diagnostic counter test (e.g. count
`sqlite3_release_memory` invocations, or assert lookaside is
disabled by default) materially helps the orchestrator pick the
next phase, you may land it under `#[ignore]` in
`tests/perf_gates.rs` or a new `tests/g0_diag.rs`. Same hard
rules apply (no public surface expansion; use
`#[cfg(debug_assertions)]` for any helper).

If unsure, skip the test — G.0 deliverable is the JSON.

## 4. Hard rules

1. **No production code changes.** This phase is read-only.
2. **Do not weaken AC-020 bound formula** (`tests/perf_gates.rs`).
3. Snapshot / cursor contract sacred (REQ-013 / AC-059b / REQ-055).
4. AC-018 must stay green at every commit.
5. No retry of `dev/notes/performance-whitepaper-notes.md` §5
   experiments.
6. No destructive git.
7. No data migration.
8. FFI uses `c_char` / `c_int`.
9. Do not chain subagents. You ARE the implementer.
10. **Use `./scripts/agent-verify.sh`**, not `dev/agent-verify.sh`
    (F.0 reviewer block #3).

## 5. Output JSON (mandatory)

Path (absolute):
`/home/coreyt/projects/fathomdb/dev/plan/runs/G0-wal-checkpoint-telemetry-output.json`

Required fields:

- `phase`: `"G.0"`.
- `decision_self`: `"PICK_G1"` / `"PICK_G2"` / `"PICK_G3"` /
  `"ESCALATE_PACK7"` / `"INCONCLUSIVE"`.
- `commit_sha`: tip at measurement time (handoff baseline
  `a00cd13` if no commit).
- `branch`: this worktree's branch.
- `ac017_runs`, `ac018_runs`: 1+ runs with `passed`.
- `ac020_runs`: N=5, same shape as F.0 output JSON.
- `ac020_summary`: median / min / max / stddev for sequential /
  concurrent / speedup.
- `symbol_classification`: array of buckets with
  `{name, seq_pct, conc_pct, ratio, delta_cycles_M}`.
- `flamegraphs`: paths to the produced SVG / folded files.
- `recommendation`: structured object `{lever: "G.1"|"G.2"|"G.3"|
"Pack7", rationale: "...", expected_impact_class: "small" |
"medium" | "large"}`.
- `unexpected_observations`: anything surprising, esp. anything
  that contradicts the F.0 worker pool summary.
- `f0_topology_holds`: bool — is the worker pool still
  observably collapsed (mutex_atomic share materially below
  Pack 5's 36.98%)? If false, this is a regression to flag.
- `agent_verify_status`: pass / fail / not-run + reason. **Use
  `./scripts/agent-verify.sh`.**
- `do_not_retry_cross_check`: confirm this phase did not retry
  any §5 lever. List the §5 levers + a one-line "did not
  re-run" attestation per item.

## 6. Commit policy

- No source commits unless an `#[ignore]`-gated diagnostic test
  is added; in that case use `diag(G0):` prefix.
- Do not edit `dev/plan/runs/STATUS.md`.
- Output JSON + flamegraph artifacts are written via absolute
  paths; orchestrator commits them on return.

## 7. Stop rule

Exit after writing the output JSON and producing the flamegraph
artifacts. Orchestrator will read JSON, decide next phase, and
spawn the corresponding implementer prompt with a §3 update-log
amendment carrying the new baseline numbers forward.
