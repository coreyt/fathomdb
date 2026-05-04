# Phase A.1 — perf record + flamegraph capture

## Model + effort

Sonnet 4.6, intent: medium. Spawn from main thread:

```bash
PHASE=A1-perf-capture
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plan/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-pack5-${PHASE}-${TS}
git -C /home/coreyt/projects/fathomdb worktree add "$WT" -b "pack5-${PHASE}-${TS}" <A0_COMMIT_SHA>
( cd "$WT" && \
  cat /home/coreyt/projects/fathomdb/dev/plan/prompts/A1-perf-capture.md \
  | claude -p --model claude-sonnet-4-6 --effort medium \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --permission-mode bypassPermissions \
      --output-format json \
  > "$LOG" 2>&1 )
```

`<A0_COMMIT_SHA>` is the commit produced by Phase A.0 (see its
output JSON). A.1 must build on top of A.0 because it uses the
sequential-only / concurrent-only test entry points.

## Log destination

- stdout/stderr: `dev/plan/runs/A1-perf-capture-<utc-ts>.log`
- structured outputs: `dev/plan/runs/A1-perf-capture-output.json`
- Flamegraph SVGs: `dev/notes/perf/ac020-{sequential,concurrent}-<short-sha>.svg`
- Folded stack files: `dev/notes/perf/ac020-{sequential,concurrent}-<short-sha>.folded`

## Required reading + discipline

- **Read `AGENTS.md` first** — canonical agent operating manual.
  Especially §1 (Stale > missing — keep evidence files current or
  delete them), §3 (run `agent-verify.sh` after meaningful edits;
  perf capture is not an edit but verify must still pass at the end
  of the session for the worktree to be clean).
- **Read `MEMORY.md`** and the `feedback_*.md` files it indexes;
  especially `feedback_orchestrator_thread.md` (this is a subagent,
  not the orchestrator).
- **No production-code changes in this phase.** If you find yourself
  editing `src/`, stop — that is not the mandate.

## Context

- Plan §4 A.1.
- Whitepaper §7.1 (diagnostic-first principle).
- A.0 output JSON (sub-phase entry points + commit SHA).
- Hardware: ARMv8 12-core Linux 5.15 Tegra; CPU governor must be
  `performance` for the duration of capture.
- Tools required (verify present before doing anything else):
  - `perf` (linux-tools matching kernel 5.15).
  - `flamegraph.pl` or `inferno-flamegraph` (Rust crate `inferno`).
  - `inferno-collapse-perf` (or equivalent `stackcollapse-perf.pl`).

## Mandate

Capture two `perf record` profiles — sequential-only and
concurrent-only — using the A.0 harness entry points, then render
two flamegraphs and a diff.

1. **Pre-flight**:
   - Confirm `perf record --version`, `inferno-flamegraph --version`
     (or `flamegraph.pl` available), `cpupower` (or `tlp` /
     `/sys/devices/system/cpu/cpu*/cpufreq/scaling_governor` write).
   - Set CPU governor to `performance` if not already. Record what
     you changed; reset at end.
   - Build the test binary once with debuginfo:
     `RUSTFLAGS="-C force-frame-pointers=yes" cargo build -p fathomdb-engine --release --tests`.

2. **Capture sequential-only**:

   ```bash
   AC020_PHASE=sequential perf record -F 999 -g --call-graph dwarf \
       -o dev/plan/runs/perf-ac020-sequential-<ts>.data \
       -- cargo test -p fathomdb-engine --release --test perf_gates \
       -- --ignored ac_020_sequential_only --nocapture
   ```

   N = 5. Record elapsed times each run. Keep the perf.data file from
   the **median** elapsed run.

3. **Capture concurrent-only**:
   Same with `AC020_PHASE=concurrent` / `ac_020_concurrent_only`.

4. **Render flamegraphs**:

   ```bash
   perf script -i <perf.data> > <name>.script
   inferno-collapse-perf < <name>.script > dev/notes/perf/ac020-<phase>-<short-sha>.folded
   inferno-flamegraph < <name>.folded > dev/notes/perf/ac020-<phase>-<short-sha>.svg
   ```

5. **Diff**:

   ```bash
   inferno-diff-folded dev/notes/perf/ac020-sequential-<sha>.folded \
                       dev/notes/perf/ac020-concurrent-<sha>.folded \
                       > dev/plan/runs/A1-folded-diff.txt
   inferno-flamegraph --colors=red < dev/plan/runs/A1-folded-diff.txt \
       > dev/notes/perf/ac020-diff-<short-sha>.svg
   ```

6. **Sanity-check the captures**: open each SVG (or grep the .folded
   file) for `pthread_mutex_lock`, `mem1Malloc`, `pcache1Fetch`,
   `vec0_*`, `read_search_in_tx`. Each must appear at least once in
   the concurrent profile, otherwise the capture is broken.

Do **not** classify or pick a bottleneck — that is Phase A.2's job.

## Acceptance criteria

- N=5 elapsed times for each sub-phase recorded in the output JSON
  (min / median / max in ms).
- Two `.svg` flamegraphs + two `.folded` files produced and exist on
  disk at the documented paths.
- Diff folded file produced; non-empty.
- Sanity-check symbol set found in concurrent .folded output.
- CPU governor reset to original at the end (or noted in output JSON
  if it could not be reset, with reason).

## Files allowed to touch

- `dev/notes/perf/` (create new SVGs / folded files).
- `dev/plan/runs/` (logs + output JSON + diff text).
- `target/` (build artifacts).

## Files NOT to touch

- All `src/` directories (this is profile capture only).
- All `tests/` directories.
- All other plan / prompt / docs files.

## Verification commands

```bash
ls -lh dev/notes/perf/ac020-*-<short-sha>.svg
ls -lh dev/notes/perf/ac020-*-<short-sha>.folded
grep -c pthread_mutex_lock dev/notes/perf/ac020-concurrent-<short-sha>.folded
grep -c mem1Malloc dev/notes/perf/ac020-concurrent-<short-sha>.folded
```

## Required output to orchestrator

`dev/plan/runs/A1-perf-capture-output.json`:

```json
{
  "phase": "A1",
  "decision": "KEEP|REVERT|INCONCLUSIVE",
  "sequential_ms": {
    "raw_runs": [<n>, <n>, <n>, <n>, <n>],
    "min": <n>, "median": <n>, "max": <n>,
    "stddev": <n>, "n": 5
  },
  "concurrent_ms": {
    "raw_runs": [<n>, <n>, <n>, <n>, <n>],
    "min": <n>, "median": <n>, "max": <n>,
    "stddev": <n>, "n": 5
  },
  "speedup_observed": <f>,
  "speedup_required": 5.33,
  "flamegraph_paths": {
    "sequential": "dev/notes/perf/ac020-sequential-<sha>.svg",
    "concurrent": "dev/notes/perf/ac020-concurrent-<sha>.svg",
    "diff": "dev/notes/perf/ac020-diff-<sha>.svg"
  },
  "folded_paths": {"sequential": "...", "concurrent": "..."},
  "perf_record_meta": {
    "frequency_hz": 999,
    "call_graph": "dwarf",
    "events_captured_sequential": <n>,
    "events_captured_concurrent": <n>,
    "kernel_warnings": "<verbatim or empty>"
  },
  "cpu_governor_action": "set_performance|already_performance|could_not_set:<reason>",
  "cpu_freq_observed_mhz": {"min": <n>, "max": <n>},
  "sanity_symbols_present": ["pthread_mutex_lock", "mem1Malloc", "pcache1Fetch", "vec0_*", "read_search_in_tx"],
  "top_10_symbols_concurrent": [
    {"symbol": "<name>", "self_pct": <f>, "total_pct": <f>}, ...
  ],
  "top_10_symbols_sequential": [...],
  "unexpected_observations": "<free text — anything surprising: kernel symbols dominating, missing symbols, abnormal stddev, jemalloc unexpectedly present, etc.>",
  "alternative_hypothesis": "<if symbols don't match the §6 hypothesis ladder, what else could explain the shape>",
  "data_for_pivot": "<if A.2 cannot pick a clear bottleneck from this, what extra capture would help — e.g. perf c2c, perf lock-contention, USDT probes>",
  "log_path": "dev/plan/runs/A1-perf-capture-<ts>.log",
  "commit_sha": "<sha used>",
  "next_phase_recommendation": "A2-symbol-focus.md"
}
```

## Required output to downstream agents

- A.2 needs the diff folded file (`dev/plan/runs/A1-folded-diff.txt`)
  and both flamegraph SVGs to classify the bottleneck.
- A.3 needs the same harness entry points + the median-run binary so
  it can run `strace -c -f` against the same workload.

## Update log

- 2026-05-03 — A.0 KEEP, baseline for A.1 = `fec71a0` (FF-merged
  onto `0.6.0-rewrite`). Replace `<A0_COMMIT_SHA>` in spawn block
  with `fec71a0` (or literal ref `0.6.0-rewrite` — currently
  resolves to `fec71a0`).
- A.0 smoke (N=1, not a gate reading): seq=184ms / conc=117ms via
  split tests; combined gate at same tree reported
  seq=182ms / conc=118ms / bound=34ms / speedup=0.19. Fixture parity
  confirmed (no drift between split + combined harnesses).
- Combined-gate bound assertion was already failing pre-A.0
  (concurrent=118ms > bound=34ms); not introduced by harness split.
  AC-020 packet bound = 1.25/8 ≈ 0.156; current speedup=0.19 is
  combined-gate's own 1.5/8≈0.188 form. Either way, gap to close.
- Pre-existing compile errors in `tests/{compatibility,cursors,lifecycle_observability}.rs`
  noted by A.0 — `agent-verify.sh` still green; ignore unless
  flamegraph capture path needs them.
- Sub-phase entry points (use these exactly):
  - `AC020_PHASE=sequential cargo test -p fathomdb-engine --release --test perf_gates -- --ignored ac_020_sequential_only --nocapture`
  - `AC020_PHASE=concurrent cargo test -p fathomdb-engine --release --test perf_gates -- --ignored ac_020_concurrent_only --nocapture`
- Markers to grep on stderr: `AC020_PHASE_SEQUENTIAL_MS=<n>` /
  `AC020_PHASE_CONCURRENT_MS=<n>`.
