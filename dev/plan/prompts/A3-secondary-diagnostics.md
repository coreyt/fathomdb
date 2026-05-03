# Phase A.3 — Secondary diagnostics (cheap; runs alongside A.1/A.2)

## Model + effort

Sonnet 4.6, intent: medium.

```bash
PHASE=A3-secondary-diagnostics
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plan/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-pack5-${PHASE}-${TS}
git -C /home/coreyt/projects/fathomdb worktree add "$WT" -b "pack5-${PHASE}-${TS}" <A0_COMMIT_SHA>
( cd "$WT" && \
  cat /home/coreyt/projects/fathomdb/dev/plan/prompts/A3-secondary-diagnostics.md \
  | claude -p --model claude-sonnet-4-6 --effort medium \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --permission-mode bypassPermissions \
      --output-format json \
  > "$LOG" 2>&1 )
```

`<A0_COMMIT_SHA>`: same A.0 commit as A.1.

## Log destination

- stdout/stderr: `dev/plan/runs/A3-secondary-diagnostics-<ts>.log`
- structured outputs: `dev/plan/runs/A3-secondary-diagnostics-output.json`
- raw evidence files (strace tally, EXPLAIN dumps, compile_options
  text): `dev/plan/runs/A3-evidence/`

## Required reading + discipline

- **Read `AGENTS.md` first** — canonical agent operating manual.
  Especially §1 (TDD mandatory), §3 (run `agent-verify.sh`), §5
  (test discipline). Test-only counter additions still must
  encode human intent — no auto-generated oracles.
- **Read `MEMORY.md` + `feedback_*.md`** — especially
  `feedback_tdd.md` (red-green-refactor; counters added under
  `#[cfg(test)]` should still come with a test that exercises them
  enough to confirm the wiring works).
- **No production-code changes in this phase.** If exposing
  `read_search_in_tx` time would require a public hook, document
  the gap in the output JSON and stop. Do not add production
  surface area for a diagnostic.
- **Run `./scripts/agent-verify.sh`** before declaring success.

## Context

- Plan §4 A.3.
- Whitepaper §6 (hypothesis hierarchy), §8 (open questions).
- A.0 entry points (`AC020_PHASE=concurrent`, `ac_020_concurrent_only`).
- Code anchors:
  - `read_search_in_tx` — `src/rust/crates/fathomdb-engine/src/lib.rs:1283`.
  - SQL inside it (4 statements):
    - vec0 rowid retrieval: lib.rs:1293.
    - canonical_nodes body lookup: lib.rs:1307.
    - search_index soft-fallback probe: lib.rs:1319.
    - search_index UNION-style FTS read: lib.rs:1343.
  - `ReaderPool::borrow` — lib.rs:168.
  - `Engine::search` / `search_inner` — lib.rs:869 / 942.

## Mandate

Run four cheap diagnostics in parallel with A.1/A.2 capture work.
**No production code changes**; instrumentation goes in the test
binary only, behind `#[cfg(test)]` or a `perf_diag` feature.

### A.3.1 strace tally on the concurrent run

```bash
AC020_PHASE=concurrent strace -c -f -o dev/plan/runs/A3-evidence/strace-concurrent.txt \
    cargo test -p fathomdb-engine --release --test perf_gates \
    -- --ignored ac_020_concurrent_only --nocapture
```

Differentiates SQLite-internal mutex contention (high `futex` time
share) from VFS contention (high `pread64`/`pwrite64`/`fdatasync`).
Record top 10 syscalls by total time.

### A.3.2 In-process counters around the read path

Add `#[cfg(test)]`-gated counters in
`src/rust/crates/fathomdb-engine/tests/perf_gates.rs` (or a tiny
helper module under `tests/`) wrapping calls in `run_ac020_mix`:

- Reader checkout wait: time blocked inside `Engine::search` between
  call entry and the embed step (proxy for `ReaderPool::borrow`
  blocking).
- Embedder time: total time inside `RoutedEmbedder::embed`.
- `read_search_in_tx` total time: must come from a public-test helper
  or by wrapping `Engine::search`.
- Per-search count of statements prepared (constant by code, but
  capture for the record).

Aggregate per-thread, sum, dump on test exit. Counter values go into
`dev/plan/runs/A3-evidence/counters.json`.

If the engine does not expose enough hooks to measure these without
production changes, document what is missing and stop at the
strace + EXPLAIN + compile_options checks. **Do not add production
hooks for this phase** — that is structural work and would change the
profile.

### A.3.3 EXPLAIN QUERY PLAN

Open a fresh `:memory:` SQLite via the same migrations, seed the same
fixture, then capture `EXPLAIN QUERY PLAN` for each of the four
read-path statements (literal SQL strings from lib.rs:1293, 1307,
1319, 1343). Save to
`dev/plan/runs/A3-evidence/explain-query-plan.txt`. Verify no obvious
planner regression (e.g. `SCAN` where `SEARCH` is expected, missing
index hits).

### A.3.4 sqlite3_threadsafe + compile_options

One-off probe:

```rust
let conn = rusqlite::Connection::open_in_memory()?;
let ts: i32 = conn.query_row("SELECT sqlite_threadsafe()", [], |r| r.get(0))?;
let opts: Vec<String> = conn.prepare("PRAGMA compile_options")?
    .query_map([], |r| r.get::<_, String>(0))?
    .flatten().collect();
```

(There is no `sqlite_threadsafe()` SQL function; if the rusqlite
binding does not expose it, use the C API
`unsafe { rusqlite::ffi::sqlite3_threadsafe() }`.) Save:

- `compile_options.txt` — full output.
- `threadsafe.txt` — single integer.

Confirms whitepaper §6 baseline assumption (THREADSAFE=1).

## Acceptance criteria

- `dev/plan/runs/A3-evidence/` exists and contains:
  - `strace-concurrent.txt`
  - `counters.json` (or a clearly-marked `counters-skipped.md` with
    rationale if production hooks would have been required).
  - `explain-query-plan.txt`
  - `compile_options.txt` and `threadsafe.txt`.
- Output JSON summarizes top syscall by time, top 3 counters, and the
  `THREADSAFE` integer.

## Files allowed to touch

- `src/rust/crates/fathomdb-engine/tests/perf_gates.rs` (test-only
  counter wiring; allowed only if it does not require production-code
  hooks).
- `dev/plan/runs/A3-evidence/`.
- `dev/plan/runs/A3-secondary-diagnostics-output.json` and `.log`.

## Files NOT to touch

- All `src/.../src/` directories (no production code).
- Schema, migrations.
- Other test files.

## Verification commands

```bash
ls dev/plan/runs/A3-evidence/
head -20 dev/plan/runs/A3-evidence/strace-concurrent.txt
cat dev/plan/runs/A3-evidence/threadsafe.txt
grep -E "^(THREADSAFE|MUTEX|MEMSYS)" dev/plan/runs/A3-evidence/compile_options.txt
./scripts/agent-verify.sh
```

## Required output to orchestrator

```json
{
  "phase": "A3",
  "decision": "DONE|PARTIAL|BLOCKED",
  "strace_top_10_syscalls": [
    {"name": "futex", "calls": <n>, "time_pct": <f>, "time_total_s": <f>, "errors": <n>},
    ...
  ],
  "strace_summary": {"total_syscalls": <n>, "total_wall_s": <f>, "futex_share_pct": <f>, "io_share_pct": <f>},
  "counters": {
    "reader_borrow_ms_total": <n>,
    "reader_borrow_ms_per_query": <f>,
    "embedder_ms_total": <n>,
    "embedder_ms_per_query": <f>,
    "read_search_ms_total": <n>,
    "read_search_ms_per_query": <f>,
    "prepares_per_search": <n>,
    "queries_total": <n>
  },
  "counters_collection_status": "complete|partial:<reason>|skipped:<reason>",
  "threadsafe": <integer>,
  "compile_options": {
    "threadsafe": "<line>",
    "memsys": "<line or empty>",
    "mutex": "<line or empty>",
    "default_pagesize": "<line or empty>",
    "default_cache_size": "<line or empty>",
    "full_path": "dev/plan/runs/A3-evidence/compile_options.txt"
  },
  "pragma_observed_per_reader": {"journal_mode": "wal", "query_only": "1", "cache_size": <n>, "mmap_size": <n>, "page_size": <n>, "synchronous": "<n>"},
  "explain_query_plan": [
    {"statement": "vec0_match", "plan_summary": "<one line>", "regression": false},
    {"statement": "canonical_lookup", "plan_summary": "<one line>", "regression": false},
    {"statement": "soft_fallback_probe", "plan_summary": "<one line>", "regression": false},
    {"statement": "fts_match", "plan_summary": "<one line>", "regression": false}
  ],
  "explain_regression_observed": true|false,
  "consistency_check_vs_a1_a2": "<does strace top syscall match A.2 named symbol family? agree|disagree:<details>>",
  "data_for_pivot": "<e.g. if futex dominates AND threadsafe == 1, MULTITHREAD path is the obvious play. If pread64 dominates, consider OS page cache / disk binding instead. If neither, suspect rust-side mutex (ReaderPool, projection runtime). If counters show embedder_ms_total > read_search_ms_total, embedder is the bottleneck and Phase B/C/D are misdirected.>",
  "unexpected_observations": "<free text>",
  "evidence_dir": "dev/plan/runs/A3-evidence/",
  "log_path": "dev/plan/runs/A3-secondary-diagnostics-<ts>.log",
  "next_phase_recommendation": "feeds A4 decision"
}
```

## Required output to downstream agents

- A.4 (decision record) folds the strace top-syscall + threadsafe
  integer + counter ratios into its decision rule.
- B.1 (if chosen) uses the threadsafe integer as the "before" baseline
  it must change (expecting `2` after intervention).

## Update log

_(append dated notes here just before spawn)_
