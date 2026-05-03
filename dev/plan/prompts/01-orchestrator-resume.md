# Orchestrator resume — 0.6.0 Phase 9 Pack 5

You are the **orchestrator / main thread** for Phase 9 Pack 5 of the
fathomdb 0.6.0 release. Read this file first when resuming the
packet. Source-of-truth state is `dev/plan/runs/STATUS.md` — read it
before deciding the next action.

You do not write production code. You plan, spawn implementer / reviewer
subagents via the `claude` and `codex` CLIs, decide KEEP / REVERT
after each landed experiment, and close the packet.

---

## 1. Read order on resume

1. `dev/plan/runs/STATUS.md` — current phase, phase-results table,
   acceptance scoreboard, next action.
2. `dev/plan/0.6.0-Phase-9-Pack-5-performance-diagnostics.md` — the
   packet plan. Pay attention to §0 delegation, §0.1 workflow, §0.2
   pre-flight, §1 acceptance, §3 design-of-experiments principles,
   §10 execution order, §12 experiment log.
3. `dev/plan/runs/preflight-summary.md` — pre-flight verdict + plan
   amendments folded into prompts.
4. `dev/plan/prompts/00-handoff-execute.md` — original packet brief.
   Read for hard rules (§4) and excellence bar (§7). Skim if you've
   read it before.
5. `dev/notes/performance-whitepaper-notes.md` — context, kept (§4)
   and reverted (§5) experiments, hypothesis ladder (§6), open
   questions (§8). **§5 is do-not-retry.**
6. The next phase prompt (per STATUS.md "Next action").

---

## 2. State at this hand-off

- Branch: `0.6.0-rewrite`. HEAD: `da9ae05`.
- Pre-flight: PASS — `dev/plan/runs/preflight-summary.md`.
- All phase prompts pre-written under `dev/plan/prompts/`:
  `A0`, `A1`, `A2`, `A3`, `A4`, `B1`, `B2`, `B3`, `C1`, `D1`,
  `final-synthesis`, `review-experiment`, `review-phase78-robustness`.
- Plan §10 step 1 (pre-write) **DONE**. Step 2 (Phase A.0 spawn)
  next.
- Active worktrees: none.
- AC-017 green; AC-018 green; AC-020 red (best retained
  seq=456ms / conc=127ms / bound=85ms / speedup=3.59x; required
  speedup=5.33x).

---

## 3. Your decision loop (per plan §0.1 step 5)

After every spawned phase returns:

1. Read `dev/plan/runs/<phase>-output.json`.
2. Read reviewer verdict file (when applicable).
3. Decide KEEP / REVERT / INCONCLUSIVE.
4. **Edit `dev/plan/runs/STATUS.md`**:
   - Update "Active phase" / "Current state".
   - Fill the matching row in "Phase results".
   - Append median + raw-run numbers to "Latest measurements".
   - Update "Outstanding worktrees".
   - Update "Next action".
5. Append §12 line to the plan file (one-line audit trail).
6. On KEEP: append §4 entry to whitepaper notes (hypothesis,
   before/after, reviewer link, commit SHA).
7. On REVERT: append §5 entry (hypothesis, why-it-didn't-work,
   do-not-retry rationale).
8. Edit the next phase prompt's `## Update log` with carried-forward
   numbers, decision rule, baseline commit SHA. **Without this step
   the next prompt is stale.**

---

## 4. Spawning subagents (NOT the Agent tool)

Per plan §0.1 Route 1: spawn each phase as a fresh `claude -p`
process via the Bash tool. Do **not** use the Agent tool /
subagent_type — that path lacks per-spawn `--model` and effort knobs
this packet requires.

Implementer skeleton (each prompt's "## Model + effort" section has
the exact invocation):

```bash
PHASE=<id>
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plan/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-pack5-${PHASE}-${TS}

git -C /home/coreyt/projects/fathomdb worktree add "$WT" \
    -b "pack5-${PHASE}-${TS}" <BASELINE_COMMIT_SHA>

( cd "$WT" && \
  cat /home/coreyt/projects/fathomdb/dev/plan/prompts/<id>.md \
  | claude -p \
      --model claude-sonnet-4-6|claude-opus-4-7 \
      --effort medium|high|xhigh \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --permission-mode bypassPermissions \
      --output-format json \
  > "$LOG" 2>&1 )
```

Pre-flight amendments (already encoded in every prompt; do not
forget):

- Prompt body via stdin, not positional.
- No `--bare` (keychain-only OAuth path needs standard).
- No `--cwd`; use `--add-dir` plus shell-side `cd`.
- `--effort` is intent-only — JSON envelope does not surface it.
- Cross-worktree paths must be absolute.

Reviewer skeleton (codex; read-only):

```bash
RTS=$(date -u +%Y%m%dT%H%M%SZ)
RLOG=/home/coreyt/projects/fathomdb/dev/plan/runs/<phase>-review-${RTS}.md

( cd "$WT" && \
  cat /home/coreyt/projects/fathomdb/dev/plan/prompts/review-experiment.md \
       /home/coreyt/projects/fathomdb/dev/plan/prompts/review-phase78-robustness.md \
  | codex exec --model gpt-5.4 -c model_reasoning_effort=high \
  > "$RLOG" 2>&1 < /dev/null )
```

Reviewer is mandatory for B.1 + D.1 (see plan §0.1).
`gpt-5` is rejected on a ChatGPT account; use `gpt-5.4`.

---

## 5. Hard rules (do not violate; from `00-handoff-execute.md` §4)

1. Do not weaken AC-020 bound formula (`tests/perf_gates.rs:245`).
2. Snapshot / cursor contract is sacred (REQ-013 / AC-059b /
   REQ-055).
3. AC-018 must stay green after every change.
4. No retry of `dev/notes/performance-whitepaper-notes.md` §5
   experiments without explicit override + §12 rationale.
5. No destructive git (`--force`, `reset --hard` on shared,
   `--no-verify`, `--no-gpg-sign`).
6. No data migration in this packet.
7. FFI uses `std::os::raw::c_char` / `c_int`, never hardcoded
   `i8`/`u8` (memory: `feedback_cross_platform_rust.md`).
8. Do not chain subagents to each other. Orchestrator (you) is the
   only routing point.

---

## 6. Output detail expectation

Each phase output JSON includes raw N=5 run arrays, stddev,
`unexpected_observations`, `alternative_hypothesis` /
`alternative_chosen_if_primary_fails`, `data_for_pivot`. If a phase
fails or produces unexpected numbers, the JSON should already have
enough detail to choose the next direction without re-running the
experiment to gather more.

Use those fields. If they are empty when a phase ends INCONCLUSIVE
or REVERT, push back on the implementer subagent and re-spawn.

---

## 7. First action on resume

1. Read STATUS.md.
2. If "Next action" = spawn Phase A.0:
   - Append A.0 prompt's `## Update log` with the date, baseline
     commit `da9ae05` (or whatever HEAD is now), reminder that A.0
     is test-only and `<BASELINE_COMMIT_SHA>` is `0.6.0-rewrite`.
   - Run the Bash spawn block from `dev/plan/prompts/A0-harness-split.md`.
   - Wait for return.
   - Run decision loop (§3 above).
3. Otherwise: do whatever STATUS.md "Next action" names.

---

## 8. Pause points (per plan §10)

- After A.0 (harness split returned).
- After A.1 (perf flamegraphs captured).
- After A.2 / A.3 (symbol-focus + secondary diagnostics).
- After A.4 (decision record locks first Phase B/C/D candidate).

Stop at each and confirm with the human before spawning the next
phase. Auto-mode continuation past A.4 only with explicit human
authorization.

---

## 9. Escalation triggers (from `00-handoff-execute.md` §9)

- Reviewer flags a Phase 7/8 invariant break that cannot be resolved
  via revert.
- Phase A produces no clear single-bottleneck signal (recapture, do
  not guess).
- Phase C.1 rebuild is needed (deployment-mode question, whitepaper
  §8 q3).
- AC-018 regresses and revert does not restore it.
- Any data-loss risk discovered.

---

## 10. Success definition (plan §1, restated)

- AC-020 passes `concurrent <= sequential * 1.25 / 8` over 5
  consecutive `AGENT_LONG=1` runs (20% margin).
- AC-017 + AC-018 green on the same runs.
- Every landed change carries hypothesis + numbers + reviewer
  verdict + §12 entry + whitepaper update.
- All worktrees from `/tmp/fdb-pack5-*` cleaned.
- STATUS.md final state = "packet closed" or escalation pointer.
