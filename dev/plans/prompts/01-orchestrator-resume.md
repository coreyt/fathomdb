# Orchestrator resume — 0.6.0 Phase 9 Pack 5

You are the **orchestrator / main thread** for Phase 9 Pack 5 of the
fathomdb 0.6.0 release. Read this file first when resuming the
packet. Source-of-truth state is `dev/plans/runs/STATUS.md` — read it
before deciding the next action.

You do not write production code. You plan, spawn implementer / reviewer
subagents via the `claude` and `codex` CLIs, decide KEEP / REVERT
after each landed experiment, and close the packet.

---

## 1. Read order on resume

1. `dev/plans/runs/STATUS.md` — current phase, phase-results table,
   acceptance scoreboard, next action.
2. `dev/plans/0.6.0-Phase-9-Pack-5-performance-diagnostics.md` — the
   packet plan. Pay attention to §0 delegation, §0.1 workflow, §0.2
   pre-flight, §1 acceptance, §3 design-of-experiments principles,
   §10 execution order, §12 experiment log.
3. `dev/plans/runs/preflight-summary.md` — pre-flight verdict + plan
   amendments folded into prompts.
4. `dev/plans/prompts/00-handoff-execute.md` — original packet brief.
   Read for hard rules (§4) and excellence bar (§7). Skim if you've
   read it before.
5. `dev/notes/performance-whitepaper-notes.md` — context, kept (§4)
   and reverted (§5) experiments, hypothesis ladder (§6), open
   questions (§8). **§5 is do-not-retry.**
6. The next phase prompt (per STATUS.md "Next action").

---

## 2. State at this hand-off

- Branch: `0.6.0-rewrite`. Branch tip: `2dc2134` (docs: STATUS +
  resume baseline refresh).
- A.0 spawn baseline: **`1980bf6`** (`feat(engine): Phase 9 Pack 1-4
…`). Phase 9 Pack 1-4 production work was sitting uncommitted in
  the working tree at original Pack 5 hand-off; landed clerically on
  2026-05-03 after `agent-verify.sh` green. See STATUS.md "Baseline
  drift note".
- Pushed to `origin/0.6.0-rewrite` through `2dc2134`.
- Pre-flight: PASS — `dev/plans/runs/preflight-summary.md`. Pre-flight
  HEAD-snapshot was `da9ae05`; engine src was uncommitted at that
  time. None of the seven pre-flight checks depend on engine src
  state, so no preflight amendment required.
- All phase prompts pre-written under `dev/plans/prompts/`:
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

Pack-5 era loop (perf-experiment-oriented):

1. Read `dev/plans/runs/<phase>-output.json`.
2. Read reviewer verdict file (when applicable).
3. Decide KEEP / REVERT / INCONCLUSIVE.
4. **Edit `dev/plans/runs/STATUS.md`**:
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

Phase 11+ loop (release-engineering-oriented, no perf measurement):

1. Read `dev/plans/runs/<phase>-output.json` (per §4.7 schema).
2. Cherry-pick implementer commits from WT branch onto mainline
   release branch (per §4.4).
3. Spawn codex reviewer on the WT branch HEAD (per §4.2).
4. Promote codex verdict body from log to canonical
   `dev/plans/runs/<phase>-review-<rts>.md` (per §4.3).
5. Decide:
   - **PASS** → close phase, advance.
   - **CONCERN (structural / prompt-induced)** → orchestrator
     override (per §4.6), close phase.
   - **CONCERN (substantive) / BLOCK** → fix-1 remediation (per
     §4.5), goto step 1.
6. Edit `dev/plans/0.6.0-implementation.md` "Immediate Next Slice"
   section: add Phase <id> CLOSED block, advance "Mainline next
   slice" pointer.
7. Commit plan + verdict + prompt files in single docs commit:
   `docs(<phase>): promote codex <verdict>; close Phase <id>;
   advance to <next>`.
8. After all sub-phases of a phase family close: worktree cleanup
   (remove WT, delete branch) — see §11.

---

## 4. Spawning subagents (NOT the Agent tool)

**This section was promoted to canonical doc 2026-05-17.** Read
`dev/design/orchestration.md` as the source of truth for invocation
patterns, codex reviewer flags, cherry-pick + fix-1 + override
discipline, and worktree cleanup.

The expanded subsections below (4.1 implementer through 4.7 output
schema) are preserved verbatim for Pack-5 audit-trail purposes and
are kept in lockstep with the canonical doc. **When in doubt or in
conflict, defer to `dev/design/orchestration.md`.**

### 4.1 Implementer (Claude writes code)

```bash
PHASE=<id>                  # e.g. 11d-release-workflow, 11d-fix-1
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plans/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-${PHASE}-${TS}  # generic prefix; pack5- was Phase 9 legacy

git -C /home/coreyt/projects/fathomdb worktree add "$WT" \
    -b "phase-${PHASE}-${TS}" <BASELINE_COMMIT_SHA>

# Anti-chaining preamble — prepend to prompt body via stdin.
# B.1 attempt #1 (2026-05-03) chained: wrapper claude -p read the
# prompt's "Spawn from main thread" block and spawned ANOTHER agent
# via Task tool, then exited prematurely. Implementer's working code
# was lost when the wrapper exited. Three-layer defense below.
PREAMBLE=$(cat <<'EOF'
============================================================
YOU ARE THE IMPLEMENTER. Not the orchestrator.

The "## Model + effort" section in this prompt describes how YOU
were just launched (claude -p with the listed model/effort). Do NOT
re-spawn yourself. Do NOT spawn other agents.

The "Reviewer pass after implementer" block (if present) describes
what the orchestrator (the human-facing main thread that launched
you) will run AFTER you exit. You do NOT spawn the reviewer either.

You are running inside the worktree shown by `pwd`. Do the work
described under "## Mandate" / "## What to do", write the output
JSON to the path under "## Log destination" / "## Required output",
commit any code changes per the prompt's commit policy, then exit.

If the spec is ambiguous or impossible (e.g. an assertion that
SQLite docs prove cannot pass), STOP and report in your final
result text — do not silently change the spec.
============================================================
EOF
)

( cd "$WT" && \
  ( echo "$PREAMBLE"; cat /home/coreyt/projects/fathomdb/dev/plans/prompts/<id>.md ) \
  | claude -p \
      --model claude-sonnet-4-6|claude-opus-4-7 \
      --effort medium|high|xhigh \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --disallowedTools Task Agent \
      --permission-mode bypassPermissions \
      --output-format stream-json --include-partial-messages --verbose \
  > "$LOG" 2>&1 )
```

Invocation rules (do not forget):

- Prompt body via stdin, not positional.
- No `--bare` (keychain-only OAuth path needs standard).
- No `--cwd` on claude; use `--add-dir` plus shell-side `cd`.
- `--effort` is intent-only — JSON envelope does not surface it.
- Cross-worktree paths must be absolute.
- `--disallowedTools Task Agent` physically prevents chained spawns
  (added 2026-05-03 after B.1 #1 BLOCKER incident).
- `--output-format stream-json --include-partial-messages --verbose`
  so the log file grows continuously; monitor mid-flight via
  `tail -f` / `wc -l`. Final result is the last `result` event;
  parse with `jq` if needed.
- Run as `run_in_background: true` Bash; you get notified on
  completion. Do NOT poll.

### 4.2 Reviewer (Codex reads diff, returns verdict)

```bash
PHASE=<id>                  # match implementer PHASE
RTS=$(date -u +%Y%m%dT%H%M%SZ)
REV_LOG=/home/coreyt/projects/fathomdb/dev/plans/runs/${PHASE}-review-${RTS}.log
WT=/tmp/fdb-${PHASE}-<implementer-ts>   # implementer worktree, post-commit

PROMPT=$(cat <<'EOF'
You are reviewing Phase <id>.

Branch: phase-<id>-<ts>, HEAD <sha>.
Baseline: <prior-CLOSED-sha>.

Required reading:
- dev/plans/prompts/<id>.md (the spec)
- dev/plans/runs/<id>-output.json (closure artifact)
- Commits <baseline-sha>..<head-sha> in chronological order

Verdict format:
- PASS / CONCERN / BLOCK on first line of verdict block
- Findings as `### N. [severity] short title` then `Refs:` (file:line
  citations) then 2-4 line explanation. Severity: high/medium/low.
- "Addressed" section listing fixes from prior verdict if a fix-1 pass.
- "Reviewer process notes" wrap.

Focus the review on <slice-specific assertions>. Sandbox is
read-only; do not attempt to write the verdict file — main thread
promotes it.
EOF
)

printf '%s\n' "$PROMPT" \
  | codex exec \
      --model gpt-5.4 \
      -c model_reasoning_effort=high \
      --sandbox read-only \
      --cd "$WT" \
      - \
  > "$REV_LOG" 2>&1
```

Invocation rules:

- `gpt-5` is rejected on ChatGPT account; use `gpt-5.4`.
- `-c model_reasoning_effort=high` — codex defaults to lower effort;
  always set explicitly.
- `--sandbox read-only` — reviewer must not modify worktree (drift
  would corrupt the diff under review).
- `--cd "$WT"` — codex's working-directory flag (distinct from
  claude's banned `--cwd`).
- `-` positional reads prompt from stdin (fed via `printf '%s\n'`).
  Do NOT use `echo "$PROMPT"` — it loses escaping on multiline
  bodies.
- Reviewer prompt is inline per-slice, not template-cat. Templates
  (`review-experiment.md`, `review-phase78-robustness.md`) are
  Pack-5 specific; phase-11+ uses targeted inline prompts.

### 4.3 Verdict promotion (codex sandbox cannot write)

Codex sandbox is read-only — reviewer cannot write the verdict file
to canonical paths. Main thread promotes the verdict body from the
log:

```bash
# 1. Read $REV_LOG, locate the verdict block (typically last ~100 lines).
# 2. Write to canonical path with frontmatter:
#    dev/plans/runs/<phase>-review-<rts>.md
# 3. Verdict format (canonical, Phase 11+ practice):
#    ## Verdict: PASS|CONCERN|BLOCK
#    ### 1. [severity] short title
#    Refs: file:line citations
#    <2-4 line explanation>
#    ## Addressed (for fix-N passes)
#    ## Reviewer process notes
#    ## Orchestrator triage (main thread's KEEP / FIX-1 / OVERRIDE call)
```

### 4.4 Cherry-pick to mainline

Implementer commits sit on `phase-<id>-<ts>` branch in worktree.
After reviewer PASS (or orchestrator override), cherry-pick the
slice onto the mainline branch:

```bash
# In main repo, on 0.6.0-rewrite (or current release branch):
git cherry-pick <implementer-sha-1> <implementer-sha-2> ...
```

Cherry-pick (not merge) lets the orchestrator select exactly which
commits land — skips WT-internal experiments + keeps mainline
history linear.

### 4.5 Fix-1 remediation pass (on BLOCK / CONCERN)

If reviewer returns BLOCK or actionable CONCERN, write a targeted
remediation prompt `dev/plans/prompts/<id>-fix-1.md` and re-spawn
the implementer in the **existing** worktree on the **existing**
branch (don't add a new worktree). Build new commits on top of the
prior head.

Spawn pattern: same as §4.1 but with `WT=<existing-wt-path>` (no
`git worktree add`). Prompt operates additively — no rewrites of
landed commits.

After fix-1: cherry-pick the new commit(s), respawn the reviewer
for re-verdict. Iterate until PASS or orchestrator override.

### 4.6 Orchestrator override (CONCERN accept)

When reviewer returns CONCERN and the finding is structural (e.g.
output.json self-reference: docs commit cannot contain its own SHA)
or prompt-induced (implementer followed the prompt literally, but
prompt produced an awkward artifact), the orchestrator may accept
the CONCERN without further remediation.

Override discipline:

- Add explicit "Orchestrator override <YYYY-MM-DD>: CONCERN
  accepted." line to the verdict .md.
- Document the rationale in `## Orchestrator triage` section.
- Never override BLOCK — that's a code or correctness issue;
  always remediate.

### 4.7 Closure output.json schema (per slice)

```json
{
  "phase": "<id>",
  "baseline_sha": "<sha branch was cut from>",
  "branch": "phase-<id>-<ts>",
  "head_sha": "<HEAD after final commit>",
  "commits": ["<sha>: <subject>", "..."],
  "findings_addressed": ["..."],          // for fix-N passes
  "blockers_encountered": [{...}],         // surface-and-resolve log
  "agent_verify_result": "pass | fail",
  "next_step_for_orchestrator": "..."
}
```

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
   - Confirm A.0 prompt's `## Update log` carries the baseline note
     (already filled on 2026-05-03 with baseline `1980bf6`); top up
     if HEAD has moved since.
   - In the spawn block, `<BASELINE_COMMIT_SHA>` is the literal
     branch ref `0.6.0-rewrite` — equivalent to whatever
     `git rev-parse 0.6.0-rewrite` currently resolves to (today:
     `2dc2134`; A.0's edits will branch off that tip with Pack 1-4
     production at `1980bf6` already in history).
   - Run the Bash spawn block from `dev/plans/prompts/A0-harness-split.md`.
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

---

## 11. Worktree cleanup (after phase family closes)

After all sub-phases of a phase family CLOSE (e.g. 11a + 11b + 11c +
11d all cherry-picked and PASS/override-accepted):

1. Verify each WT branch head has equivalent commits on mainline:
   `git log --oneline --grep="<phase>" <mainline-branch> | head`.
2. Save any uncommitted closure artifacts in the WT
   (`dev/plans/runs/<phase>-*-output.json` that the implementer
   wrote but never committed) — Write tool into main repo, commit.
3. Remove worktrees **one per Bash call** (bundled destructive ops
   trigger permission denial):

   ```bash
   git worktree remove --force /tmp/fdb-<phase-family>-...
   ```

4. Delete branches **one per Bash call**:

   ```bash
   git branch -D phase-<phase-family>-...
   ```

5. Verify clean:
   `git worktree list` should show only the main repo;
   `git branch | grep phase-<phase-family>` should be empty.

Per `feedback_file_deletion.md` memory: never `find -delete`. Stray
sidecar lock files (`*.sqlite.lock`) in WTs are disposable — they
disappear with `git worktree remove --force`.
