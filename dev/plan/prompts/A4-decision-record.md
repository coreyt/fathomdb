# Phase A.4 — Decision record (main thread, Opus high)

## Model + effort

Opus 4.7, intent: high. **Main thread executes directly**
(orchestrator is the main thread; see `feedback_orchestrator_thread.md`).

## Log destination

- §12 entry in `dev/plan/0.6.0-Phase-9-Pack-5-performance-diagnostics.md`.
- §11 narrative entry in `dev/notes/performance-whitepaper-notes.md`.
- Structured outputs: `dev/plan/runs/A4-decision-record-output.json`.
- Updated `## Update log` section in the chosen Phase B/C/D prompt
  file (the next thing that will be spawned).

## Required reading + discipline

- **Read `AGENTS.md` first** — canonical agent operating manual.
  Especially §1 (ADRs authoritative, Memory first, Stale > missing).
- **Read `MEMORY.md`** and the `feedback_*.md` files; especially
  `feedback_orchestrator_thread.md` (main thread is the orchestrator
  — this phase is yours, not delegated),
  `feedback_reliability_principles.md` (no soak, no punt).
- **No production code, no tests**. Decision artifact only.

## Context

- A.1 output: `dev/plan/runs/A1-perf-capture-output.json` + flamegraphs.
- A.2 output: `dev/plan/runs/A2-symbol-focus-output.json`
  (classification + chosen experiment).
- A.3 output: `dev/plan/runs/A3-secondary-diagnostics-output.json`
  (strace, counters, threadsafe integer).
- Plan §3 Design-of-experiments principles.
- Whitepaper §5 (do-not-retry list), §6 (hypothesis hierarchy), §7
  (untried options ranked).

## Mandate

This is the single most important deliverable in Phase A. The choice
made here locks the next experiment.

1. **Reconcile A.2 + A.3 evidence**:
   - Does the strace top-syscall agree with the A.2 symbol family?
     (If A.2 says "allocator mutex" but strace shows top syscall is
     `pread64`, the picture is inconsistent — flag it.)
   - Do the counters agree? (If A.2 says `read_search_in_tx` is the
     bottleneck but A.3 counters show 90% of time in
     `RoutedEmbedder::embed`, A.2 mis-classified.)
   - If A.3 was PARTIAL/BLOCKED, weight A.2 alone but record the gap.

2. **Cross-check against whitepaper §5 (reverted)**:
   - The chosen experiment must NOT match anything on the §5 list.
     If it does (e.g. picking "single-statement vec0 materialize"
     which was reverted), you are forbidden to proceed without a
     written rationale that explains what's different this time
     (and that rationale lands in §12 of the plan).

3. **Pick exactly one**:
   - B.1 (runtime MULTITHREAD wiring, ordering-correct).
   - B.2 (MEMSTATUS=0).
   - B.3 (per-conn lookaside / pagecache).
   - C.1 (rebuild THREADSAFE=2).
   - D.1 (single-stmt UNION refactor; structural).
   - RECAPTURE (A.1/A.2 was inconclusive; loop back).

4. **Write the decision rule** for the chosen experiment as a single
   numeric line (concrete %, ms, or speedup threshold) — this becomes
   the `Decision rule` block in the chosen prompt's Update log so the
   subagent knows the keep/revert criterion.

5. **Update the chosen prompt file's `## Update log`** with:
   - Date.
   - A.1 baseline numbers (sequential / concurrent / bound / speedup).
   - A.2 classification table summary (one line per category).
   - A.3 strace top syscall + threadsafe integer + counters.
   - The decision rule from step 4.
   - Whitepaper §5 cross-check verdict.

6. **Append a §12 line** to the plan file, format:
   ```
   - 2026-MM-DD A.4 → chose <PHASE>; baseline seq=<n>ms conc=<n>ms bound=<n>ms; bottleneck=<symbol>; rule=<rule>.
   ```

7. **Append a §11 narrative paragraph** to the whitepaper notes
   (NOT a duplicate of the plan — the whitepaper is reasoning prose,
   the plan log is one-line audit trail).

## Acceptance criteria

- Exactly one Phase B/C/D candidate selected (or RECAPTURE).
- Decision rule is numeric and verifiable.
- §12 line and §11 paragraph both written.
- Chosen prompt's Update log filled in.
- Whitepaper §5 cross-check explicit (PASS / OVERRIDE-with-rationale).
- A.4 output JSON written.

## Files allowed to touch

- `dev/plan/0.6.0-Phase-9-Pack-5-performance-diagnostics.md` (§12).
- `dev/notes/performance-whitepaper-notes.md` (§11 / §6 update).
- The chosen Phase B/C/D prompt file (Update log section only).
- `dev/plan/runs/A4-decision-record-output.json`.

## Files NOT to touch

- All `src/`, all `tests/` directories.
- Other phase prompt files.
- Pre-flight artifacts.

## Verification commands

```bash
test -f dev/plan/runs/A4-decision-record-output.json
grep -A5 "## 12. Experiment log" \
    dev/plan/0.6.0-Phase-9-Pack-5-performance-diagnostics.md
grep -B0 -A5 "Update log" dev/plan/prompts/<chosen-phase>.md | tail -40
```

## Required output to orchestrator

```json
{
  "phase": "A4",
  "decision": "B1|B2|B3|C1|D1|RECAPTURE",
  "rationale": "<2-4 sentences: which evidence, why this experiment, why not §5 entries>",
  "decision_rule": "<numeric rule, e.g. 'concurrent_ms drops by ≥ 30% AND speedup ≥ 5.0x → KEEP'>",
  "kill_criteria": "<when to abandon this track entirely, e.g. 'if concurrent_ms doesn't drop by ≥ 10% even with B.1+B.2+B.3 stacked, mutex track is wrong; promote D.1'>",
  "expected_outcome_range": {"concurrent_ms_min": <n>, "concurrent_ms_max": <n>, "speedup_min": <f>, "speedup_max": <f>},
  "baseline": {"sequential_ms": <n>, "concurrent_ms": <n>, "bound_ms": <n>, "speedup": <f>, "n": <n>},
  "evidence_consistency": {
    "a2_a3_agree": true|false,
    "agreement_summary": "<one line>",
    "discrepancies": ["<text>", ...]
  },
  "alternative_chosen_if_primary_fails": "B1|B2|B3|C1|D1|ESCALATE",
  "alternative_rationale": "<2 sentences>",
  "evidence_paths": {
    "a1": "dev/plan/runs/A1-perf-capture-output.json",
    "a2": "dev/plan/runs/A2-symbol-focus-output.json",
    "a3": "dev/plan/runs/A3-secondary-diagnostics-output.json"
  },
  "section5_crosscheck": "PASS|OVERRIDE",
  "section5_override_rationale": "<empty unless OVERRIDE>",
  "ordering_safety_check_b1": {
    "performed": true|false,
    "extra_connection_open_callsites_found": ["<file:line>", ...],
    "verdict": "safe|unsafe:<details>"
  },
  "next_prompt_file": "dev/plan/prompts/<chosen>.md",
  "data_for_pivot": "<if chosen experiment fails AND alternative_chosen_if_primary_fails also fails, what is the next direction — extend instrumentation, larger fixture, change harness, abandon AC-020 closure for this packet?>",
  "unexpected_observations": "<free text>"
}
```

## Required output to downstream agents

- The chosen Phase B/C/D prompt is now fully briefed. Its Update log
  contains everything the implementer subagent needs to evaluate its
  own keep/revert decision.

## Update log

_(append dated notes here when this work begins; capture A.1/A.2/A.3
numbers verbatim before reasoning so the chain is auditable)_
