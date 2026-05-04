# Phase E.1 — `prepare_cached` on read-path search statements

## Model + effort

Sonnet 4.6, intent: high. Spawn from main thread:

```bash
PHASE=E1-prepare-cached-readers
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plan/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-pack5-${PHASE}-${TS}
git -C /home/coreyt/projects/fathomdb worktree add "$WT" -b "pack5-${PHASE}-${TS}" 0.6.0-rewrite
( cd "$WT" && \
  ( echo "$PREAMBLE"; cat /home/coreyt/projects/fathomdb/dev/plan/prompts/E1-prepare-cached-readers.md ) \
  | claude -p --model claude-sonnet-4-6 --effort high \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --disallowedTools Task Agent \
      --permission-mode bypassPermissions \
      --output-format stream-json --include-partial-messages --verbose \
  > "$LOG" 2>&1 )
```

Anti-chaining: PREAMBLE prepended via stdin per resume §4 (anti-chaining
defenses). Reviewer (codex `gpt-5.4`) **MANDATORY** if E.1 KEEPs (touches
read-path hot loop in `Engine::search`).

## Log destination

- stdout/stderr: `dev/plan/runs/E1-prepare-cached-readers-<ts>.log`
- structured: `dev/plan/runs/E1-prepare-cached-readers-output.json`
- reviewer verdict (if KEEP): `dev/plan/runs/E1-review-<ts>.md`

## Required reading + discipline

- `AGENTS.md` (§1, §3, §4, §5).
- `MEMORY.md` + `feedback_*.md` — especially `feedback_tdd.md`,
  `feedback_reliability_principles.md`.
- `dev/plan/0.6.0-Phase-9-Pack-5-performance-diagnostics.md` §4 / §12
  for context.
- `dev/notes/performance-whitepaper-notes.md` §4 (existing
  `prepare_cached` KEEPs on writer-side), §5 (read-path tweak
  revert — DO read this; the prior revert didn't specify which
  statements were tried), §7.8 (E activation; E.1 mandate).
- `dev/plan/runs/A3-secondary-diagnostics-output.json` —
  `prepares_per_search = 4` (counters), `sqlite3RunParser` 4.6%
  seq / 3.4% conc, search_us_per_query = 542 µs.
- `dev/plan/runs/B1-multithread-wiring-output.json` +
  `dev/plan/runs/C1-threadsafe2-rebuild-output.json` — both
  REVERT, mutex track closed clean. E.1 attacks the per-query
  parse-cost residual.

## Context

A.3 counters: each `Engine::search` call runs 4 prepares. Combined
with `sqlite3RunParser` at 4-5% of cycles in *both* profiles
(independent of concurrency = real per-call floor), this is a
parse-cost residual that B.1/C.1 cannot reach. Whitepaper §4
already KEEPS `prepare_cached` on the writer-side
projection-runtime connections. §5 reverts a read-path tweak but
does NOT name which statements were tried. E.1 redoes this
carefully on the named four statements.

## Mandate

1. **Identify the 4 read-path statements** from `Engine::search`
   in `src/rust/crates/fathomdb-engine/src/lib.rs`. Per A.3.3 they
   are: `vec0_match`, `canonical_lookup`, `soft_fallback_probe`,
   `fts_match`. Locate the `Connection::prepare` (or
   `transaction.prepare` / `tx.prepare`) call sites for each.

2. **Switch each call site to `prepare_cached`** on the
   long-lived reader connection (the `ReaderPool` connection
   borrowed at the top of `Engine::search`). Statement cache
   lives on the Connection, so the cache is per-reader-conn —
   8 readers × 4 statements = 32 cached statements maximum.

3. **Cache-invalidation safety check**: readers are read-only and
   never see DDL; the cache is safe across the test's lifetime.
   Document this explicitly with a comment on the first
   `prepare_cached` call site (one line, names the invariant).

4. **Test (red-green-refactor)**:
   - **Red**: write a `#[test]` in
     `src/rust/crates/fathomdb-engine/tests/prepare_cached_readers.rs`
     (new file) that opens an `Engine`, runs `Engine::search` ≥ 100
     times against a small fixture, and asserts via a counter that
     each of the 4 statements prepared **once** total (or ≤ 8 ×
     N_threads = 8 for the 8 readers, whichever is correct under
     pool semantics — the failing baseline state would be `100 × 4
     = 400` prepares). The counter goes through a small `pub fn`
     accessor on the engine that exposes a per-statement prepare
     count (gated by `#[cfg(any(test, feature = "metrics"))]` if
     the metrics gating pattern is already in use; otherwise just
     `pub fn` named for `_for_test`). Run on main first to confirm
     RED (current state has 400 prepares).
   - **Green**: switch the call sites to `prepare_cached`. Test
     passes (count drops to ≤ 8 per statement = 32 total).
   - **Refactor**: ensure no regression in `cargo test -p
     fathomdb-engine`; AC-017 + AC-018 stay green.

5. **Decision rule (numeric)**:
   - **KEEP** iff `concurrent_median_ms ≤ 80` AND `speedup ≥ 5.0×`
     AND AC-018 green (= ≥ 30% drop from A.1 baseline 115 ms).
   - **INCONCLUSIVE** band 80-100 ms → STAGE STACK E.3 (reader
     `cache_size` + `mmap_size` re-try) on top of E.1 in a follow-up
     spawn. KEEP E.1 alone if `sequential_median` drops by ≥ 15%
     (parse-cost relief is a real win even without closing AC-020).
   - **REVERT** iff `concurrent_median_ms > 115` OR AC-018 red OR
     prepared-statement cache invalidation surfaces (e.g. a
     migration test fails with a stale plan).

6. **Kill criterion**: if E.1 lands `prepares_per_search drops to
   1 (per cached statement) AND sequential drops as expected AND
   concurrent doesn't move`, the per-call parse cost is real but
   not the AC-020 bottleneck. Promote E.3 next; if E.3 also flat,
   D.1.

## Acceptance criteria

- `cargo test -p fathomdb-engine --release` is green.
- New `prepare_cached_readers.rs` test asserts prepare-count drops
  from 400 → ≤ 32 (8 readers × 4 statements) after 100 search calls.
- AC-018 stays green; AC-017 stays green.
- AC-020 5x AGENT_LONG re-runs recorded in output JSON.
- Reviewer verdict not BLOCK (mandatory for KEEP path).

## Files allowed to touch

- `src/rust/crates/fathomdb-engine/src/lib.rs` (call-site swap,
  optional small accessor for the prepare counter).
- `src/rust/crates/fathomdb-engine/tests/prepare_cached_readers.rs`
  (new test file).
- `dev/plan/runs/E1-prepare-cached-readers-output.json` and `.log`.

## Files NOT to touch

- Schema files / migrations (E.2 territory; deferred out-of-packet).
- `Cargo.toml` (no new deps; `prepare_cached` is rusqlite stdlib).
- `.cargo/config.toml` (C.1 is REVERT; don't reintroduce
  `LIBSQLITE3_FLAGS`).
- Other crates in `src/rust/crates/`.
- Reader-side `PRAGMA` calls (E.3 territory; only if E.1 KEEPs but
  AC-020 doesn't close).
- Test files outside the new one.

## Verification commands

```bash
cargo test -p fathomdb-engine --release \
    --test prepare_cached_readers
cargo test -p fathomdb-engine --release
AGENT_LONG=1 cargo test -p fathomdb-engine --release --test perf_gates \
    ac_020_reads_do_not_serialize_on_a_single_reader_connection \
    -- --nocapture
# Repeat AGENT_LONG run 5 times back to back; record min/median/max.
./scripts/agent-verify.sh
```

## Required output to orchestrator

```json
{
  "phase": "E1",
  "decision": "KEEP|REVERT|INCONCLUSIVE",
  "before": {
    "sequential_ms": 182, "concurrent_ms": 115, "bound_ms": 34, "speedup": 1.58, "n": 5,
    "prepares_per_search": 4,
    "sqlite3RunParser_pct_concurrent": 3.4,
    "sqlite3RunParser_pct_sequential": 4.6,
    "search_us_per_query": 542,
    "source": "A.1 baseline (fec71a0); A.3 counters (edb0c84)"
  },
  "after": {
    "raw_runs": [{"sequential_ms": <n>, "concurrent_ms": <n>, "bound_ms": <n>, "speedup": <f>}, ...],
    "sequential_ms": {"min": <n>, "median": <n>, "max": <n>, "stddev": <n>},
    "concurrent_ms": {"min": <n>, "median": <n>, "max": <n>, "stddev": <n>},
    "speedup":       {"min": <f>, "median": <f>, "max": <f>, "stddev": <f>},
    "n": 5
  },
  "delta_concurrent_pct": <f>,
  "delta_sequential_pct": <f>,
  "delta_speedup_pct": <f>,
  "prepares_per_search_after": <n>,
  "prepare_count_test_passed": true|false,
  "ac017_status": "green|red:<numbers>",
  "ac018_status": "green|red:<numbers>",
  "ac020_passes_5_33x": true|false,
  "ac020_passes_packet_1_25_margin": true|false,
  "decision_rule": "<rule>",
  "decision_rule_met": true|false,
  "reviewer_verdict": "PASS|CONCERN|BLOCK|skipped:REVERT",
  "reviewer_concerns": ["<text>", ...],
  "loc_added": <n>, "loc_removed": <n>,
  "files_changed": ["src/rust/crates/fathomdb-engine/src/lib.rs", ...],
  "commit_sha": "<sha if KEEP>",
  "git_status_clean_after_revert": true|null,
  "data_for_pivot": "<if KEEP but bound still red: stack E.3 (reader cache_size + mmap_size re-try). If sequential drops by >=15% but concurrent doesn't move, parse cost is real but isn't the AC-020 bottleneck — promote E.3 anyway. If E.3 also flat: D.1 (architectural).>",
  "unexpected_observations": "<free text>",
  "next_phase_recommendation": "verification-gate|E3|D1|FINAL"
}
```

## Update log

- 2026-05-03 — Activated per §7.8 after C.1 REVERT (`15c6473`)
  closed the mutex track. C.1 numbers carried for context:
  conc 121.5 ms (+5.65% vs A.1), speedup 1.509× (-4.48% vs A.1).
  Mutex track exhausted at compile-time; remaining gap is in
  the per-query / per-call overheads A.3 surfaced.
- A.1 baseline (carry into output JSON `before` block):
  - sequential N=5 `[189,199,182,179,176]` ms; median 182, stddev 9.2
  - concurrent N=5 `[120,110,117,115,112]` ms; median 115, stddev 4.0
  - speedup 1.58×; required 5.33×; gap 3.4×
- A.3 evidence: `prepares_per_search=4`,
  `search_us_per_query=542`, embedder=0 (RoutedEmbedder fixture),
  `sqlite3RunParser` 4.6% seq / 3.4% conc.
- Expected outcome: cuts ~4 × ~25 µs ≈ 100 µs of re-parse per
  query → search_us_per_query drops toward ~440 µs (~18% drop) →
  concurrent_median drops toward ~95 ms. **Likely INCONCLUSIVE
  on its own**; E.3 stack expected to push to KEEP.
- Spawn baseline: `0.6.0-rewrite` tip after this prompt + the E
  activation bookkeeping commit lands.
- Reviewer (codex `gpt-5.4`) MANDATORY for KEEP path — read-path
  hot loop change with statement-cache invalidation invariant.
