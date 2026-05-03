# Phase D.1 — Single-statement UNION refactor (Opus xhigh; reviewer mandatory)

## Model + effort

Opus 4.7, intent: xhigh. Spawn from main thread:

```bash
PHASE=D1-union-refactor
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plan/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-pack5-${PHASE}-${TS}
git -C /home/coreyt/projects/fathomdb worktree add "$WT" -b "pack5-${PHASE}-${TS}" <BASELINE_COMMIT_SHA>
( cd "$WT" && \
  cat /home/coreyt/projects/fathomdb/dev/plan/prompts/D1-union-refactor.md \
  | claude -p --model claude-opus-4-7 --effort xhigh \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --permission-mode bypassPermissions \
      --output-format json \
  > "$LOG" 2>&1 )
```

`<BASELINE_COMMIT_SHA>`: latest KEPT commit at the time D.1 spawns
(could be A.0 head if D.1 runs early, or B-stack/C.1 head later).
D.1 is on a parallel track per plan §7 — spawn only when A.2 signal
supports it OR after the mutex track is exhausted.

Reviewer pass: **mandatory** (snapshot/cursor risk surface).

```bash
RPHASE=D1-review
RTS=$(date -u +%Y%m%dT%H%M%SZ)
RLOG=/home/coreyt/projects/fathomdb/dev/plan/runs/${RPHASE}-${RTS}.md
( cd "$WT" && \
  cat /home/coreyt/projects/fathomdb/dev/plan/prompts/review-experiment.md \
       /home/coreyt/projects/fathomdb/dev/plan/prompts/review-phase78-robustness.md \
  | codex exec --model gpt-5.4 -c model_reasoning_effort=high \
  > "$RLOG" 2>&1 < /dev/null )
```

## Log destination

- `dev/plan/runs/D1-union-refactor-<ts>.log`
- `dev/plan/runs/D1-union-refactor-output.json`
- `dev/plan/runs/D1-review-<ts>.md`

## Required reading + discipline

- **Read `AGENTS.md` first** — canonical agent operating manual.
  Especially §1 (TDD mandatory, ADRs authoritative, Public surface
  is contract — `Engine::search` and `read_search_in_tx` shape are
  part of the contract; preserve), §3 (`agent-verify.sh`),
  §4 (verification ordering), §5 (failing test first; test files
  read-only during fix-to-spec).
- **Read `MEMORY.md` + `feedback_*.md`** — especially
  `feedback_tdd.md`, `feedback_reliability_principles.md`
  (net-negative LoC bias).
- **TDD path is explicit below and mandatory** — the cursor /
  snapshot / dedupe contracts are sacred. Write the failing test
  first that pins the expected ordering and dedupe; then refactor.
- **Run `./scripts/agent-verify.sh`** before declaring success
  AND `cargo test -p fathomdb-engine --release` (full engine suite).

## Context

- Plan §7 D.1.
- Whitepaper §5 (a single-statement vec0 + canonical join was tried
  and reverted — that variant was a join, not a UNION; this is a
  different shape, but the precedent demands extra care).
- Whitepaper §7.6 (structural collapse rationale).
- Hard rules from `00-handoff-execute.md` §4:
  - Snapshot / cursor contract is sacred (REQ-013 / AC-059b /
    REQ-055).
  - `BEGIN DEFERRED` reader-tx wrapping `read_search_in_tx` and the
    `projection_cursor` derivation must remain consistent.
- Code anchors:
  - `read_search_in_tx` — `src/rust/crates/fathomdb-engine/src/lib.rs:1283`.
  - vec0 retrieval (statement 1): lib.rs:1293.
  - canonical_nodes body lookup (statement 2): lib.rs:1307.
  - search_index soft-fallback probe (statement 3): lib.rs:1319.
  - search_index UNION-style FTS read (statement 4): lib.rs:1343.
  - Snapshot derivation (`load_projection_cursor`): called at lib.rs:1289.
  - Dedupe via `BTreeSet<String>` named `seen`: lib.rs:1335.
- Tests that must stay green:
  - `tests/projection_runtime.rs` (vector-only and hybrid dedupe).
  - `tests/cursors.rs` (cursor consistency).
  - `tests/error_taxonomy.rs`.
  - `tests/perf_gates.rs` AC-017, AC-018, AC-020.
  - `tests/compatibility.rs`.

## Mandate

Collapse the four read-path SQL statements in `read_search_in_tx`
into a **single prepared statement** that combines vec0 retrieval
and FTS5 retrieval via `UNION ALL` with explicit ordering and a
deterministic dedupe. The statement must:

1. Run inside the same `BEGIN DEFERRED` reader transaction (do not
   move the transaction boundary).
2. Read `projection_cursor` first via `load_projection_cursor` (as
   today), so the cursor returned to the caller is derived from the
   same snapshot as the result rows.
3. Produce the same Rust-level output:
   `(cursor, soft_fallback, results)`.
4. Preserve dedupe order: vector hits first, then FTS hits not
   already returned, in `write_cursor` ascending order for the FTS
   side (today's tie-break).
5. Preserve the soft-fallback semantics:
   `query_vector.is_some() && vector_rows_visible.is_empty()` →
   probe `search_index` for a match whose write_cursor has not been
   projection-terminalized; if a match exists, return
   `Some(SoftFallback { branch: SoftFallbackBranch::Vector })`.
   - The simplest preserving approach: keep the soft-fallback probe
     as a **second** prepared statement (still inside the same
     reader-tx), only when needed. The 4-stmt → 1-stmt collapse is
     for the happy path; the rare soft-fallback can stay separate.
     Document this choice in code comment.
6. Use `prepare_cached` on the new statement
   (whitepaper §4 keeps `prepare_cached` on long-lived runtime
   connections; same principle applies here, but verify it does not
   regress AC-017/018 because the statement closes with the read-tx).

### Concrete SQL shape (sketch — adjust for actual column names)

```sql
WITH vec_hits AS (
  SELECT cn.body AS body, 0 AS branch_rank, vd.distance AS rank_value
  FROM vector_default vd
  JOIN canonical_nodes cn ON cn.write_cursor = vd.rowid
  WHERE vd.embedding MATCH vec_f32(?1)
  ORDER BY vd.distance
  LIMIT 10
),
fts_hits AS (
  SELECT body AS body, 1 AS branch_rank, write_cursor AS rank_value
  FROM search_index
  WHERE search_index MATCH ?2
)
SELECT body FROM vec_hits
UNION ALL
SELECT fh.body
FROM fts_hits fh
WHERE fh.body NOT IN (SELECT body FROM vec_hits)
ORDER BY branch_rank, rank_value;
```

`?1` is the query vector (NULL-safe: bind empty/NULL when
`query_vector.is_none()` and gate the vec_hits CTE accordingly, or
keep two prepared statements — vector-only + FTS-only — and pick at
runtime; the orchestrator-facing decision is fewer-prepares, not
must-be-one-statement).

If a single prepared statement turns out to be infeasible (e.g.
sqlite-vec MATCH cannot live in a CTE on this version), fall back to
**two** prepared statements (was four). Two is still a meaningful
reduction. Document the exact reason in the code comment.

### TDD path

1. **Red**: copy the existing dedupe ordering test (or write one if
   missing) so it covers vector-then-FTS ordering and dedupe.
   Confirm it passes on baseline.
2. **Green**: refactor `read_search_in_tx` to the new shape. All
   existing tests stay green.
3. **Refactor**: tighten error handling on the new statement
   (rusqlite `Result` propagation unchanged).

## Acceptance criteria

- `cargo test -p fathomdb-engine --release` (full engine suite) green.
- All listed tests stay green.
- AC-020 long-run: sequential **≥ 15% improvement** AND concurrent
  **≥ 15% improvement** AND speedup ratio does not regress, vs the
  baseline used for D.1's worktree branch (decision rule per plan §7).
  Otherwise REVERT.
- Reviewer verdict not BLOCK.
- §12 + whitepaper updated.
- Net-LoC target (memory: `feedback_reliability_principles.md`):
  prefer net-negative LoC. If net-positive, document why in
  whitepaper §4.

## Files allowed to touch

- `src/rust/crates/fathomdb-engine/src/lib.rs` (`read_search_in_tx`
  and any small helpers it needs).
- §12 + whitepaper update.

## Files NOT to touch

- Schema files / migrations (no DDL changes).
- Other crates.
- `_fathomdb_projection_terminal` / `_fathomdb_vector_kinds` table
  shapes.
- `tests/perf_gates.rs:245` bound formula.
- AC-018 / AC-017 test bodies — they must pass unchanged.

## Verification commands

```bash
cargo test -p fathomdb-engine --release  # full suite
AGENT_LONG=1 cargo test -p fathomdb-engine --release --test perf_gates \
    ac_020_reads_do_not_serialize_on_a_single_reader_connection \
    -- --nocapture  # x5
./scripts/agent-verify.sh
```

## Required output to orchestrator

```json
{
  "phase": "D1",
  "decision": "KEEP|REVERT|INCONCLUSIVE",
  "statement_count_before": 4,
  "statement_count_after": <1|2|3>,
  "fallback_used": "single_stmt|two_stmt|three_stmt",
  "fallback_rationale": "<empty if 1-stmt achieved; otherwise the SQLite-side reason — e.g. vec0 MATCH not allowed in CTE>",
  "prepares_per_search_before": 4,
  "prepares_per_search_after": <n>,
  "before": {
    "raw_runs": [{"sequential_ms": <n>, "concurrent_ms": <n>}, ...],
    "sequential_ms": <n>, "concurrent_ms": <n>, "bound_ms": <n>, "speedup": <f>, "stddev_concurrent": <n>, "n": 5,
    "source": "<which prior commit served as baseline>"
  },
  "after": {
    "raw_runs": [{"sequential_ms": <n>, "concurrent_ms": <n>}, ...],
    "sequential_ms": <n>, "concurrent_ms": <n>, "bound_ms": <n>, "speedup": <f>, "stddev_concurrent": <n>, "n": 5
  },
  "delta_sequential_pct": <f>,
  "delta_concurrent_pct": <f>,
  "delta_speedup_pct": <f>,
  "speedup_regressed": true|false,
  "ac017_status": "green|red:<numbers>",
  "ac018_status": "green|red:<numbers>",
  "ac018_drain_ms_after": <n>,
  "ac020_passes_5_33x": true|false,
  "ac020_passes_packet_1_25_margin": true|false,
  "correctness_evidence": {
    "result_set_equality_check": "rows returned by new query equal rows returned by old query on the gate fixture (yes|no:<details>)",
    "dedupe_order_preserved": true|false,
    "soft_fallback_triggers_when_expected": true|false,
    "snapshot_contract_preserved": true,
    "projection_runtime_test_status": "green|red:<test name>",
    "cursors_test_status": "green|red:<test name>",
    "error_taxonomy_test_status": "green|red:<test name>"
  },
  "explain_query_plan_after": [
    {"statement": "<id>", "plan_summary": "<one line>"}
  ],
  "reviewer_verdict": "PASS|CONCERN|BLOCK",
  "phase78_review_verdict": "PASS|CONCERN|BLOCK",
  "reviewer_log": "dev/plan/runs/D1-review-<ts>.md",
  "phase78_review_log": "dev/plan/runs/D1-review-phase78-<ts>.md",
  "loc_added": <n>, "loc_removed": <n>,
  "loc_net_negative": true|false,
  "files_changed": ["src/rust/crates/fathomdb-engine/src/lib.rs"],
  "commit_sha": "<sha if KEEP>",
  "data_for_pivot": "<distinguish three failure modes: (a) sequential improved but concurrent did not — bottleneck is mutex, not prepare cost; (b) neither improved — query plan changed and lost an index; record EXPLAIN delta; (c) both regressed — the new SQL is wrong, revert immediately. For each case, name the next experiment.>",
  "unexpected_observations": "<free text>",
  "next_phase_recommendation": "final-synthesis|REVERT_AND_RECONSIDER"
}
```

## Required output to downstream agents

- Final synthesis: D.1 result determines whitepaper narrative on
  per-search prepare cost (§7.6 outcome).

## Update log

_(append baseline numbers + branch SHA + reminder of §5 reverted
single-statement vec0 join and how this refactor differs)_
