# Phase A.2 — Symbol-focus diff (main thread, Opus xhigh)

## Model + effort

Opus 4.7, intent: xhigh. **Main thread executes this phase directly**
— per `feedback_orchestrator_thread.md`, the orchestrator is the main
thread and does not delegate this kind of judgment work to a separate
subagent. This file is the work brief the orchestrator follows.

If the orchestrator is operating from a fast/limited model on the day
of execution, escalate to the human before proceeding.

## Log destination

- Notes + classification table: write directly into the §12
  experiment log of `dev/plan/0.6.0-Phase-9-Pack-5-performance-diagnostics.md`
  AND a fresh entry in `dev/notes/performance-whitepaper-notes.md` §11
  (or §6 hypothesis-update if appropriate).
- Structured outputs: `dev/plan/runs/A2-symbol-focus-output.json`.

## Required reading + discipline

- **Read `AGENTS.md` first** — canonical agent operating manual.
  This phase touches docs only; AGENTS.md §1 (ADRs authoritative,
  Stale > missing) governs the whitepaper update.
- **Read `MEMORY.md` + `feedback_*.md`** — especially
  `feedback_orchestrator_thread.md` (main thread is the orchestrator),
  `feedback_reliability_principles.md` (no soak; no punt).
- **No production-code changes**, no test changes. Doc / decision
  artifact only.

## Context

- Plan §4 A.2.
- A.1 outputs:
  - `dev/notes/perf/ac020-sequential-<sha>.svg`
  - `dev/notes/perf/ac020-concurrent-<sha>.svg`
  - `dev/notes/perf/ac020-diff-<sha>.svg`
  - `dev/plan/runs/A1-folded-diff.txt`
- Whitepaper §6 (hypothesis hierarchy) — primary suspect is SQLite
  global allocator mutex (THREADSAFE=1). Secondary: pcache mutex /
  unconfigured lookaside. Tertiary: per-search prepare cost.

## Mandate

Read the two flamegraphs and the diff. Classify time spent in:

| Category               | Symbols                                                       |
| ---------------------- | ------------------------------------------------------------- |
| pthread / sqlite mutex | `pthread_mutex_lock`, `pthreadMutexEnter`, `sqlite3_mutex_*` |
| Allocator              | `mem1Malloc`, `mem1Free`, `malloc`, `free`, `je_*`           |
| Page cache             | `pcache1Fetch`, `pcache1Truncate`                            |
| vec0 / FTS             | `sqlite3VtabCall*`, `vec0_*`, `fts5_*`                       |
| Our code               | `read_search_in_tx`, `ReaderPool::*`, `dispatch*`            |
| Embedder               | `RoutedEmbedder::embed`, allocator under it                  |

For each category record:
- Time-share in sequential profile (% of total samples).
- Time-share in concurrent profile (% of total samples).
- Ratio (concurrent / sequential).

The bottleneck is the category whose **time-share fraction** grows
super-linearly between sequential and concurrent. A flat category
(e.g. embedder) is not the bottleneck. A category that doubles or
more is.

Then pick the next experiment:

| Suspect category that grew | First Phase B/C/D candidate |
| -------------------------- | --------------------------- |
| pthread / sqlite mutex     | B.1 (runtime MULTITHREAD) — ordering-correct |
| Allocator (mem1*)          | B.1, then B.3 (per-conn lookaside)            |
| Page cache (pcache1*)       | B.3 (lookaside) + B.1 combined                |
| vec0 / FTS                 | D.1 (single-stmt UNION refactor)              |
| Our code (`read_search_in_tx` per-prepare) | D.1 first             |

If multiple categories grew super-linearly, pick the one whose
absolute time delta is largest.

If **none** grew super-linearly: do **not** proceed to Phase B/C/D.
Plan §4.4 explicitly says extend instrumentation and recapture
(loop back to A.3 with finer-grained probes).

## Acceptance criteria

- Classification table filled in for all six rows.
- One named symbol family identified as the bottleneck, OR
  "inconclusive — recapture" decision recorded.
- Decision recorded in §12 of the plan and in the output JSON.
- §11 of the whitepaper notes appended with the classification table
  and the picked experiment.

## Files allowed to touch

- `dev/plan/0.6.0-Phase-9-Pack-5-performance-diagnostics.md` (§12 entry).
- `dev/notes/performance-whitepaper-notes.md` (append to §11/§6).
- `dev/plan/runs/A2-symbol-focus-output.json`.
- The corresponding chosen Phase B/C/D prompt's `## Update log`
  section (fold the A.2 finding in before that phase's spawn).

## Files NOT to touch

- All `src/` directories.
- All `tests/` directories.
- A.0 / A.1 prompt files.

## Verification commands

```bash
test -f dev/plan/runs/A2-symbol-focus-output.json
grep -A30 "## 12. Experiment log" \
    dev/plan/0.6.0-Phase-9-Pack-5-performance-diagnostics.md | head -50
```

Sanity: classification numbers must add to ~100% per profile (allow
small drift for unattributed frames; if >25% unattributed, recapture).

## Required output to orchestrator

`dev/plan/runs/A2-symbol-focus-output.json`:

```json
{
  "phase": "A2",
  "decision": "PROCEED_B1|PROCEED_B2|PROCEED_B3|PROCEED_C1|PROCEED_D1|RECAPTURE|ESCALATE",
  "classification": [
    {"category": "pthread_mutex", "seq_pct": <n>, "conc_pct": <n>, "ratio": <n>, "evidence_lines": ["<folded-file line>", ...]},
    {"category": "allocator",     "seq_pct": <n>, "conc_pct": <n>, "ratio": <n>, "evidence_lines": [...]},
    {"category": "page_cache",    "seq_pct": <n>, "conc_pct": <n>, "ratio": <n>, "evidence_lines": [...]},
    {"category": "vec0_fts",      "seq_pct": <n>, "conc_pct": <n>, "ratio": <n>, "evidence_lines": [...]},
    {"category": "our_code",      "seq_pct": <n>, "conc_pct": <n>, "ratio": <n>, "evidence_lines": [...]},
    {"category": "embedder",      "seq_pct": <n>, "conc_pct": <n>, "ratio": <n>, "evidence_lines": [...]},
    {"category": "kernel_other",  "seq_pct": <n>, "conc_pct": <n>, "ratio": <n>, "evidence_lines": [...]},
    {"category": "unattributed",  "seq_pct": <n>, "conc_pct": <n>, "ratio": <n>, "evidence_lines": []}
  ],
  "totals_check": {"seq_sum_pct": <n>, "conc_sum_pct": <n>},
  "bottleneck_category": "<one of the above>",
  "named_symbol": "<e.g. pthread_mutex_lock under mem1Malloc>",
  "named_symbol_seq_pct": <n>,
  "named_symbol_conc_pct": <n>,
  "secondary_growth_categories": ["<categories that also grew but weren't chosen>"],
  "rationale": "<2-3 sentences citing the strongest evidence line>",
  "primary_hypothesis": "<the hypothesis A.4 will test>",
  "alternative_hypotheses": [
    {"hypothesis": "<text>", "would_predict": "<observable>", "next_experiment_if_true": "<phase id or RECAPTURE plan>"},
    ...
  ],
  "kill_criteria_for_chosen_experiment": "<numeric: if intervention does NOT move concurrent ms by ≥ X%, abandon this track>",
  "expected_outcome_range": {"concurrent_ms_min": <n>, "concurrent_ms_max": <n>, "speedup_min": <f>, "speedup_max": <f>},
  "chosen_experiment": "B1|B2|B3|C1|D1|RECAPTURE",
  "fallback_experiment_if_chosen_fails": "B1|B2|B3|C1|D1|ESCALATE",
  "flamegraph_evidence_paths": [
    "dev/notes/perf/ac020-sequential-<sha>.svg",
    "dev/notes/perf/ac020-concurrent-<sha>.svg",
    "dev/notes/perf/ac020-diff-<sha>.svg"
  ],
  "data_for_pivot": "<if every category has flat ratio, what does that imply — e.g. CPU-bound, embedder-bound, lock-free contention, kernel-side; what additional capture or instrumentation would unblock>",
  "unexpected_observations": "<free text>"
}
```

## Required output to downstream agents

- The chosen prompt file's Update log gets the A.2 finding appended,
  including the classification table and the explicit hypothesis
  ("we expect intervention X to drop concurrent ms by Y% because the
  bottleneck is Z").
- A.3 (if still planned) gets a list of finer-grained counters to add
  if A.2 was inconclusive.

## Update log

_(append dated notes here just before A.2 work begins — fold A.1
numbers + flamegraph paths in)_
