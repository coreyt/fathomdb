# Hand-off — Execute 0.6.0 Phase 9 Pack 5

Self-contained brief for the agent (or future-you) who will execute
`dev/plan/0.6.0-Phase-9-Pack-5-performance-diagnostics.md` end-to-end.
Read this file in full before touching anything else.

---

## 1. Your role

You are the **orchestrator / main thread** for Phase 9 Pack 5 of the
fathomdb 0.6.0 release. You do not write production code yourself.
Your job is:

1. Run the §0.2 pre-flight checks.
2. Pre-write every prompt under `dev/plan/prompts/`.
3. Spawn implementer subagents (`claude -p`) phase by phase per §10.
4. Spawn reviewer passes (`codex exec`) after each landed experiment.
5. At each orchestrator decision point (plan §0.1 workflow step 5),
   read the run log, read the reviewer verdict, decide
   KEEP / REVERT / INCONCLUSIVE, record in §12, update next prompt.
6. Verify the §1 acceptance criteria and close the packet.

You **may** edit prompt files, plan §12 log, the whitepaper notes,
and `dev/test-plan.md` / `dev/progress/0.6.0.md` after a keep.
You **must not** edit production code in `src/rust/...` directly —
that work is delegated to subagents per the §0 model+effort table.

---

## 2. Required reading (in this order)

1. `dev/plan/0.6.0-Phase-9-Pack-5-performance-diagnostics.md` — the plan.
2. `dev/notes/performance-whitepaper-notes.md` — context, what was
   already tried, hypothesis ladder, breadcrumbs. **§4 (kept) and §5
   (reverted) are critical** — do not re-attempt anything in §5
   without explicit human override and a written justification in
   §12.
3. `dev/test-plan.md` — AC definitions for AC-017 / AC-018 / AC-020.
4. `dev/progress/0.6.0.md` — current packet status board.
5. `src/rust/crates/fathomdb-engine/tests/perf_gates.rs` — the gate
   itself. Anchors: lines 11, 43, 102, 121, 211, 245.
6. `src/rust/crates/fathomdb-engine/src/lib.rs` — engine internals.
   Anchors: lines 48, 158, 869, 942, 1283, 1824, 2401, 2457.
7. `src/rust/crates/fathomdb-engine/src/lifecycle.rs` — lifecycle +
   subscriber registry. Anchors: 115, 235.
8. `/home/coreyt/.claude/projects/-home-coreyt-projects-fathomdb/memory/MEMORY.md`
   — durable user feedback. **Especially:**
   - `feedback_orchestrate_releases.md` — orchestrator pattern.
   - `feedback_orchestrator_thread.md` — main thread IS the
     orchestrator; do not spawn a separate orchestrator subagent.
   - `feedback_tdd.md` — red-green-refactor for behavior changes.
   - `feedback_reliability_principles.md` — net-negative LoC bias,
     no punt, no soak.
   - `feedback_workflow_validation.md` — actionlint, not yaml.safe_load.
   - `feedback_cross_platform_rust.md` — c_char rules for any FFI.
   - `feedback_release_verification.md` — CI green is not done.

---

## 3. Critical state at hand-off (2026-05-03)

- Branch: `0.6.0-rewrite`. Baseline commit: `b4a3261`.
- AC-017: green. AC-018: green. AC-020: long-run **red**.
- AC-020 best retained: `sequential=456ms / concurrent=127ms /
  bound=85ms`. Speedup 3.59x; bound requires 5.33x.
- Hardware: ARMv8 12-core, Linux 5.15 Tegra.
- Bundled SQLite via `rusqlite 0.31` features = ["bundled"]
  (`Cargo.toml:13`). Default `SQLITE_THREADSAFE=1` (serialized).
- Current pre-existing modified files (do **not** touch unless your
  phase explicitly says to): see `git status` at session start;
  `Cargo.lock`, `dev/interfaces/rust.md`, `dev/progress/0.6.0.md`,
  `dev/test-plan.md`, several engine tests, schema migrations.

---

## 4. Hard rules (do not violate)

1. **Do not weaken the AC-020 bound formula.** No editing
   `tests/perf_gates.rs:245` to relax `1.5 / AC020_THREADS`.
2. **Snapshot / cursor contract is sacred** — REQ-013 / AC-059b /
   REQ-055. The `BEGIN DEFERRED` reader-tx wrapping
   `read_search_in_tx` (lib.rs:1283) and the `projection_cursor`
   derivation must remain consistent after every change.
3. **AC-018 must stay green.** Re-run after any change that touches
   the writer or projection runtime.
4. **No retry of plan §5 reverted experiments** without explicit
   human override and §12 written rationale.
5. **No destructive git** — no `--force`, no `reset --hard` on
   shared branches, no `--no-verify`, no `--no-gpg-sign`. Always
   create new commits. Worktree-isolate any spawned implementer.
6. **No data migration in this packet.** Phase 9 Pack 5 is a
   diagnostics + perf packet only. No schema changes.
7. **Cross-provider FFI rules** — any new FFI work uses
   `std::os::raw::c_char` not hardcoded `i8`/`u8`
   (memory: feedback_cross_platform_rust.md).
8. **Do not chain subagents to each other.** Orchestrator (you) is
   the only routing point. Subagents return; you decide; you spawn
   the next.

---

## 5. Tooling

Both tools are installed and PATH-resolved:

- `claude` 2.1.126 — implementer / main-line subagents (§0.1
  Route 1).
- `codex` 0.128.0 — reviewer passes (§0.1 Reviewer).

**Run §0.2 pre-flight first.** Until pre-flight artifacts exist at
`dev/plan/runs/preflight-summary.md` with all checks PASS, do not
spawn any phase work. If a check FAILS, fix the harness and rerun
that check; do not proceed past pre-flight on partial PASS.

---

## 6. Execution sequence (matches plan §10)

0. **Pre-flight** — `dev/plan/runs/preflight-summary.md` with all
   checks PASS.
1. **Pre-write all prompt files** under `dev/plan/prompts/` per
   §0.1 file table. Each must include all eight named sections from
   the prompt template. Do not leave placeholder TODOs.
2. **Phase A.0** — harness split (Sonnet medium). Test-only edit.
   Reviewer pass optional.
3. **Phase A.1** — perf flamegraphs (Sonnet medium). Hardware time;
   this is real `perf record` work, not a simulation.
4. **Phase A.2 / A.3** — symbol-focus + secondary diagnostics
   (Opus xhigh / Sonnet medium).
5. **Phase A.4 — decision record** (Opus high). The single most
   important deliverable in Phase A. The chosen Phase B candidate
   is locked here; do not deviate without rerunning A.4.
6. **Phase B.1** — runtime MULTITHREAD wiring (Opus high).
   **Reviewer pass mandatory** (FFI ordering risk).
7. **Phase B.2 / B.3** — only if B.1 didn't close the gap.
8. **Phase C.1** — only if Phase B didn't close the gap.
9. **Phase D.1** — single-stmt UNION refactor (Opus xhigh).
   **Reviewer pass mandatory** (snapshot/cursor risk surface).
10. **Final synthesis** (Opus medium) — update whitepaper notes,
    `dev/test-plan.md`, `dev/progress/0.6.0.md`. Close the packet.

Pause points (per plan §10):
- Before pre-flight (now).
- After A.0, A.1, A.2/A.3, A.4 — orchestrator confirms before next
  experiment lands.

---

## 7. Excellence bar (what good looks like)

- Every kept change has: hypothesis stated up-front, before/after
  numbers (sequential ms, concurrent ms, bound ms, speedup, all
  N=5 minimum, median + min + max), reviewer verdict, §12 entry,
  whitepaper notes update.
- Every reverted change has: hypothesis, why it didn't work,
  cost-of-keeping reason if applicable, whitepaper §5 update so
  future-you knows not to retry.
- `git status` at packet close is clean except for genuinely
  intentional commits (no leftover experiment debris). Pre-existing
  modifications from session-start should stay or merge cleanly.
- AC-017 / AC-018 / AC-020 all green over 5 consecutive
  `AGENT_LONG=1` runs. Variance numbers reported, not just medians.
- No silent hooks-skipped, no `--no-verify`, no rationale-free
  reverts.
- Whitepaper notes (`dev/notes/performance-whitepaper-notes.md`)
  ends with a clean §11/§12-equivalent narrative someone could
  lift directly into a future paper.

---

## 8. Common failure modes from prior session

Avoid these — they are how this packet has burned cycles already:

1. **Tuning without diagnostic.** Don't try a Phase B/C/D variant
   before Phase A signal supports it. Plan exists because of this
   exact failure mode.
2. **Goodhart on the ratio.** A change that slows sequential to
   make the ratio "look better" is not a win. Always report
   absolute concurrent ms; only keep changes that improve absolute
   concurrent latency.
3. **`sqlite3_config` ordering pitfall.** `sqlite3_initialize` runs
   on first connection open, including from `sqlite3_auto_extension`.
   Any `sqlite3_config` call must precede that. Capture the return
   code; `SQLITE_MISUSE` means silent no-op.
4. **Single-run conclusions.** ~10% variance observed. N=1 reads
   nothing. N=5 is the floor; N=10 for borderline outcomes.
5. **Re-trying §5 reverted experiments.** Reader cache_size,
   prepare_cached on the read path, READ_ONLY|NO_MUTEX flags,
   pool size > 8, vec0 single-stmt materialize, subscriber
   empty fast-path. All measured negative. Do not retry.
6. **Forgetting to revert experiment debris.** Per
   `feedback_file_deletion.md`: tracked → `git rm`; untracked →
   `rm` after `git ls-files` double-check. Never `find -delete`.

---

## 9. Decision authority and escalation

You decide:
- Whether to proceed past each pause point.
- KEEP / REVERT / INCONCLUSIVE per experiment.
- Whether reviewer BLOCK is overridden (must record rationale in §12).
- When N exceeds the §3 floor for borderline measurements.

Escalate to human:
- Reviewer flags a Phase 7/8 invariant break that you cannot
  resolve via revert.
- Phase A diagnostics produce no clear single-mutex bottleneck
  signal (plan §4.4 — extend instrumentation; do not guess).
- Phase C.1 rebuild is required (deployment-mode question; see
  whitepaper §8 open question 3).
- AC-018 regresses and cannot be restored by reverting the most
  recent change.
- Any data-loss risk discovered in passing.

---

## 10. First action

Verify both CLIs and write the pre-flight artifact:

```bash
which claude codex
claude --version
codex --version
mkdir -p dev/plan/runs dev/plan/prompts
```

Then begin §0.2 pre-flight check 1 (model pin). Do not proceed to
check 2 until check 1 PASS is recorded in `preflight-summary.md`.

---

## 11. Success definition (§1, restated)

- AC-020 passes `concurrent <= sequential * 1.25 / 8` over 5
  consecutive `AGENT_LONG=1` runs (20% margin on the original
  bound).
- AC-017 + AC-018 green on the same runs.
- Every landed change carries hypothesis + numbers + reviewer
  verdict + §12 entry + whitepaper update.
- Pre-flight + every phase + verification gate logged under
  `dev/plan/runs/`.
- Final synthesis lands in `dev/notes/performance-whitepaper-notes.md`
  and the packet is closed in `dev/progress/0.6.0.md`.

When all five hold: packet done. Commit, hand back to human.
