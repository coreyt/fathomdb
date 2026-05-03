# Phase A.0 — Harness split (sequential-only / concurrent-only modes)

## Model + effort

Sonnet 4.6, intent: medium. Spawn from main thread:

```bash
PHASE=A0-harness-split
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plan/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-pack5-${PHASE}-${TS}
git -C /home/coreyt/projects/fathomdb worktree add "$WT" -b "pack5-${PHASE}-${TS}" 0.6.0-rewrite
( cd "$WT" && \
  cat /home/coreyt/projects/fathomdb/dev/plan/prompts/A0-harness-split.md \
  | claude -p --model claude-sonnet-4-6 --effort medium \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --permission-mode bypassPermissions \
      --output-format json \
  > "$LOG" 2>&1 )
```

Notes (from preflight summary §"Plan amendments"):
- Prompt body via stdin, not positional.
- No `--bare`, no `--cwd`.
- `--effort medium` is intent-only — JSON envelope does not surface it.
- Cross-worktree paths must be absolute.

## Log destination

- stdout/stderr: `dev/plan/runs/A0-harness-split-<utc-ts>.log`
- structured outputs (the orchestrator-facing JSON below): `dev/plan/runs/A0-harness-split-output.json`
- Reviewer optional for this phase (test-only edit). If invoked: `dev/plan/runs/A0-harness-split-review-<utc-ts>.md`.

## Required reading + discipline

- **Read `AGENTS.md` first** — canonical agent operating manual.
  Especially §1 (Memory first / ADRs / TDD mandatory / Stale > missing),
  §3 (build/test/lint verbs — `agent-verify.sh` after every
  meaningful edit), §4 (verification ordering: lint → typecheck →
  test), §5 (test discipline: failing test first, no agent-generated
  oracles).
- **Read `MEMORY.md`** and the `feedback_*.md` files it indexes
  before changing anything.
- **TDD does not strictly apply** to this phase (test-file edit
  only, no production code), but the new tests must still be
  meaningful and not auto-generated oracles.
- **Run `./scripts/agent-verify.sh`** before declaring success.

## Context

- Plan: `dev/plan/0.6.0-Phase-9-Pack-5-performance-diagnostics.md` §4 A.0.
- Whitepaper notes: `dev/notes/performance-whitepaper-notes.md` §1 (test-of-record), §3 (test anchors).
- Test-of-record:
  `src/rust/crates/fathomdb-engine/tests/perf_gates.rs:211`
  (`ac_020_reads_do_not_serialize_on_a_single_reader_connection`).
- Anchors in that file:
  - `AC020_THREADS = 8` — line 11.
  - `AC020_ROUNDS_PER_THREAD = 50` — line 12.
  - `seed_ac020_fixture` — line 102.
  - `run_ac020_mix` — line 121.
  - sequential phase loop — line 222.
  - concurrent phase barrier + threads — line 228 onward.
  - bound assertion — line 245.
- Memory: `feedback_tdd.md` (TDD red-green-refactor),
  `feedback_reliability_principles.md` (net-negative LoC bias).

## Mandate

Add a way to run the AC-020 harness in **sequential-only** and
**concurrent-only** modes from separate processes, so that each can be
profiled independently with `perf record`. Keep the existing combined
test as the long-run gate — do not remove it, do not weaken it.

Recommended shape (pick whichever is cleaner; both are acceptable):

1. **Two new `#[ignore]` tests** that share `seed_ac020_fixture` /
   `run_ac020_mix` / `AC020_THREADS` / `AC020_ROUNDS_PER_THREAD`:
   - `ac_020_sequential_only` — runs only the sequential loop and
     reports its elapsed time via `eprintln!`. Gated by
     `AC020_PHASE=sequential` env var so a `cargo test --
     ac_020_sequential_only` invocation still requires opt-in.
   - `ac_020_concurrent_only` — runs only the concurrent phase
     (barrier + threads) and reports its elapsed time. Gated by
     `AC020_PHASE=concurrent`.
2. **Or** a single new test that branches on `AC020_PHASE`. Same gating.

Either way:
- Both new tests must use the **same fixture seed and same query mix**
  as the combined gate. No fixture drift.
- The combined gate at line 211 stays unchanged.
- `eprintln!` the sub-phase elapsed time prefixed with
  `AC020_PHASE_SEQUENTIAL_MS=<ms>` / `AC020_PHASE_CONCURRENT_MS=<ms>`
  so the parent shell can grep one line per sub-phase from the test
  output.
- Tests must be `#[ignore]` so they only run when explicitly invoked.

This is a test-only edit. **No production-code changes.**

## Acceptance criteria

- `cargo test -p fathomdb-engine --release --test perf_gates -- --ignored AC020_PHASE=sequential ac_020_sequential_only` runs only the sequential phase, prints `AC020_PHASE_SEQUENTIAL_MS=<n>`, exits 0.
- `cargo test -p fathomdb-engine --release --test perf_gates -- --ignored AC020_PHASE=concurrent ac_020_concurrent_only` runs only the concurrent phase, prints `AC020_PHASE_CONCURRENT_MS=<n>`, exits 0.
- `AGENT_LONG=1 cargo test -p fathomdb-engine --release --test perf_gates ac_020_reads_do_not_serialize_on_a_single_reader_connection` still runs the original combined gate and produces a numeric result (pass or fail; no compile-time regression, no harness change visible to the gate itself).
- `cargo test -p fathomdb-engine --release` (full engine suite) is green.
- Net diff is +tests, no production code touched.

## Files allowed to touch

- `src/rust/crates/fathomdb-engine/tests/perf_gates.rs` (only).

## Files NOT to touch

- Everything under `src/rust/crates/fathomdb-engine/src/`.
- All other test files.
- `Cargo.toml` files.
- Schema, migrations, docs, plan, prompts.

## Verification commands

```bash
cargo build -p fathomdb-engine --release --tests
cargo test -p fathomdb-engine --release --test perf_gates -- --ignored \
    ac_020_sequential_only AC020_PHASE=sequential
cargo test -p fathomdb-engine --release --test perf_gates -- --ignored \
    ac_020_concurrent_only AC020_PHASE=concurrent
AGENT_LONG=1 cargo test -p fathomdb-engine --release --test perf_gates \
    ac_020_reads_do_not_serialize_on_a_single_reader_connection
cargo test -p fathomdb-engine --release
./scripts/agent-verify.sh
```

All six must succeed (the combined gate may report numbers; pass/fail
on the bound is irrelevant to A.0 — only that it still compiles and
runs).

## Required output to orchestrator

Write `dev/plan/runs/A0-harness-split-output.json`:

```json
{
  "phase": "A0",
  "decision": "KEEP|REVERT|INCONCLUSIVE",
  "diff_summary": "<one line>",
  "files_changed": ["src/rust/crates/fathomdb-engine/tests/perf_gates.rs"],
  "loc_added": <n>, "loc_removed": <n>,
  "sequential_only_test_name": "ac_020_sequential_only",
  "concurrent_only_test_name": "ac_020_concurrent_only",
  "env_var_gate": "AC020_PHASE",
  "marker_lines_emitted": ["AC020_PHASE_SEQUENTIAL_MS=<n>", "AC020_PHASE_CONCURRENT_MS=<n>"],
  "smoke_run_sequential_ms": <n or null>,
  "smoke_run_concurrent_ms": <n or null>,
  "smoke_run_combined_gate_ms": {"sequential_ms": <n>, "concurrent_ms": <n>, "bound_ms": <n>, "speedup": <f>},
  "combined_gate_compiles": true,
  "combined_gate_unchanged_diff": true,
  "fixture_seed_drift_check": "no_drift|drift:<details>",
  "unexpected_observations": "<free text>",
  "data_for_pivot": "<if A.1 perf capture later finds the harness split changes the profile shape, what would help — e.g. inline both phases into one test process>",
  "log_path": "dev/plan/runs/A0-harness-split-<ts>.log",
  "commit_sha": "<sha if committed>",
  "next_phase_recommendation": "A1-perf-capture.md"
}
```

## Required output to downstream agents

- The two `cargo test` invocations exact strings A.1 should run.
- Confirmation that the harness emits `AC020_PHASE_SEQUENTIAL_MS=<n>` and
  `AC020_PHASE_CONCURRENT_MS=<n>` lines (A.1 will grep these).
- Worktree path + commit SHA so A.1 can `cherry-pick` or rebase if it
  runs in a fresh worktree.

## Update log

_(append dated notes here just before spawn — see plan §0.1 step 2)_
