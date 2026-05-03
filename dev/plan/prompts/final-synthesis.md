# Final synthesis — packet close

## Model + effort

Opus 4.7, intent: medium. Main thread executes directly (low token
volume; reasoning load).

## Log destination

- Updates land in `dev/notes/performance-whitepaper-notes.md`,
  `dev/test-plan.md`, `dev/progress/0.6.0.md`, plan §12.
- Output JSON: `dev/plan/runs/final-synthesis-output.json`.

## Required reading + discipline

- **Read `AGENTS.md` first** — canonical agent operating manual.
  Especially §1 (Stale > missing — keep docs current or delete),
  §3 (`agent-verify.sh`).
- **Read `MEMORY.md` + `feedback_*.md`** — especially
  `feedback_orchestrator_thread.md` (main thread is the
  orchestrator, this phase is yours),
  `feedback_release_verification.md` (CI green is not done; final
  verification matters),
  `feedback_file_deletion.md` (worktree cleanup discipline).
- **No production code, no tests**. Doc updates + cleanup only.
- **Run `./scripts/agent-verify.sh`** at the end to confirm the
  packet-close state is clean.

## Context

- All phase output JSONs in `dev/plan/runs/`.
- All reviewer verdicts in `dev/plan/runs/`.
- `00-handoff-execute.md` §11 success definition.

## Mandate

Close the Phase 9 Pack 5 packet.

1. **Whitepaper** (`dev/notes/performance-whitepaper-notes.md`):
   - §4 (kept) — append every KEPT change with hypothesis,
     before/after numbers (N=5: min/median/max), reviewer verdict
     link, commit SHA.
   - §5 (reverted) — append every REVERTED change with hypothesis,
     why it didn't work, do-not-retry rationale.
   - §11 — narrative summary (3-6 paragraphs):
     - what we found (named bottleneck symbol from A.2/A.3 evidence);
     - what worked, what didn't, why;
     - whether AC-020 closed; if not, what should come next.
   - §8 — close out the open questions that this packet answered;
     leave the rest unchanged.

2. **Test plan** (`dev/test-plan.md`):
   - Update AC-020 status. If green: mark green with the 5-run
     numbers. If still red: keep red with updated numbers and a
     pointer to the relevant whitepaper paragraph.
   - AC-017 / AC-018 status confirmed green or flagged.

3. **Progress board** (`dev/progress/0.6.0.md`):
   - Mark Phase 9 Pack 5 closed.
   - Reference the final synthesis paragraph.

4. **Plan §12**:
   - One trailing line per experiment (audit trail; should already be
     populated incrementally — verify completeness here).

5. **Cleanup**:
   - `git status` should show only intentional commits + the docs
     updates above.
   - Per `feedback_file_deletion.md`: tracked debris → `git rm`;
     untracked → `rm` after `git ls-files` double-check. Never
     `find -delete`.
   - All worktrees from `/tmp/fdb-pack5-*` removed
     (`git worktree remove`).

6. **Final STATUS.md update**: mark active phase = none; phase
   results table fully populated; "Next action" = "packet closed,
   see whitepaper §11" (or "ESCALATE: AC-020 still red, see <link>"
   if the gate did not close). STATUS.md is the durable hand-off
   document for the next packet.

## Acceptance criteria (success definition, plan §1 + handoff §11)

- AC-020 passes `concurrent <= sequential * 1.25 / 8` over 5
  consecutive `AGENT_LONG=1` runs (20% margin) — record N=5 numbers
  in `dev/plan/runs/final-synthesis-output.json`.
- AC-017 + AC-018 green on the same runs.
- Every landed change carries: hypothesis + numbers + reviewer
  verdict + §12 entry + whitepaper update.
- Pre-flight + every phase + verification gate logged under
  `dev/plan/runs/`.
- Whitepaper §11 narrative reads as a self-contained explanation a
  future paper could lift.

If AC-020 did not close, escalate to human with a written summary —
do not silently lower the bar.

## Files allowed to touch

- `dev/notes/performance-whitepaper-notes.md`.
- `dev/test-plan.md`.
- `dev/progress/0.6.0.md`.
- `dev/plan/0.6.0-Phase-9-Pack-5-performance-diagnostics.md` (§12 only).
- `dev/plan/runs/final-synthesis-output.json`.

## Files NOT to touch

- All `src/`, all `tests/` (production-code work is done before
  synthesis; if synthesis discovers a bug, escalate, do not patch
  inline).
- Other prompt files (their work is complete).
- Pre-flight artifacts.

## Verification commands

```bash
AGENT_LONG=1 cargo test -p fathomdb-engine --release --test perf_gates \
    ac_017_vector_projection_freshness_p99_le_five_seconds \
    ac_018_drain_of_100_vectors_le_two_seconds \
    ac_020_reads_do_not_serialize_on_a_single_reader_connection \
    -- --nocapture  # 5 consecutive runs, all 3 tests each
cargo test -p fathomdb-engine --release  # full engine suite
./scripts/agent-verify.sh
git worktree list
git status --short
```

## Required output to orchestrator

```json
{
  "phase": "final-synthesis",
  "decision": "PACKET_CLOSED|ESCALATE",
  "ac017_runs": [
    {"run": 1, "p99_ms": <n>, "passed": true},
    {"run": 2, "p99_ms": <n>, "passed": true},
    {"run": 3, "p99_ms": <n>, "passed": true},
    {"run": 4, "p99_ms": <n>, "passed": true},
    {"run": 5, "p99_ms": <n>, "passed": true}
  ],
  "ac018_runs": [
    {"run": 1, "drain_ms": <n>, "passed": true},
    ...
  ],
  "ac020_runs": [
    {"run": 1, "sequential_ms": <n>, "concurrent_ms": <n>, "bound_ms": <n>, "speedup": <f>, "passed_5_33x": true|false, "passed_packet_1_25_margin": true|false},
    ...
  ],
  "ac020_5x_consecutive_pass_5_33x": true|false,
  "ac020_5x_consecutive_pass_packet_margin": true|false,
  "ac020_summary_after": {
    "sequential_ms": {"min": <n>, "median": <n>, "max": <n>, "stddev": <n>},
    "concurrent_ms": {"min": <n>, "median": <n>, "max": <n>, "stddev": <n>},
    "speedup":       {"min": <f>, "median": <f>, "max": <f>}
  },
  "ac020_summary_before_packet": {
    "sequential_ms": 456, "concurrent_ms": 127, "bound_ms": 85, "speedup": 3.59
  },
  "experiment_chain": [
    {"phase": "A.0", "decision": "KEEP", "commit": "<sha>"},
    {"phase": "A.1", "decision": "KEEP", "commit": "<sha>"},
    {"phase": "A.2", "decision": "PROCEED_<X>", "commit": "<n/a>"},
    {"phase": "A.3", "decision": "DONE", "commit": "<n/a>"},
    {"phase": "A.4", "decision": "<chosen>", "commit": "<n/a>"},
    {"phase": "B.1", "decision": "KEEP|REVERT|SKIPPED", "commit": "<sha>"},
    ...
  ],
  "kept_experiments": ["B1", "..."],
  "reverted_experiments": ["..."],
  "skipped_experiments": ["..."],
  "total_loc_delta": <n>,
  "loc_net_negative": true|false,
  "whitepaper_section11_path": "dev/notes/performance-whitepaper-notes.md#section-11",
  "whitepaper_section4_added_entries": <n>,
  "whitepaper_section5_added_entries": <n>,
  "outstanding_open_questions": [
    {"question": "<text>", "status": "answered|still-open|new", "section_ref": "§8 q<n>"},
    ...
  ],
  "data_for_pivot": "<if PACKET_CLOSED but follow-up improvements obvious: name them; if ESCALATE: which evidence file the human should read first, what they should decide next, what the safest revert point is>",
  "next_packet_pointer": "<if ac020 still red, the next packet's hypothesis and first experiment>",
  "unexpected_observations": "<free text>",
  "worktrees_cleaned": true,
  "test_plan_updated": true,
  "progress_board_updated": true,
  "status_md_final": "dev/plan/runs/STATUS.md"
}
```

## Required output to downstream agents

- None — this is the terminal phase.

## Update log

_(append the experiment summary table from the §12 plan log, plus
the kept/reverted aggregate, before drafting the §11 narrative)_
