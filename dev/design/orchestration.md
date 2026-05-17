---
title: Multi-Agent Orchestration Pattern
date: 2026-05-17
target_release: 0.6.0
desc: Canonical Claude-implementer + Codex-reviewer spawn discipline for orchestrated work
blast_radius: dev/plans/prompts/*; .github/workflows/release.yml; release engineering; all multi-agent slices
status: locked
supersedes:
  - dev/plans/0.6.0-Phase-9-Pack-5-performance-diagnostics.md § 0.1 (Pack-5-era invocation pattern)
  - dev/plans/prompts/01-orchestrator-resume.md § 4 (lifted here; resume doc now points back)
  - dev/plans/prompts/00-handoff-execute.md § 3-5 (Pack-5-era handoff)
---

# Multi-Agent Orchestration Pattern

Canonical pattern for orchestrated work: **Claude implements (writes
code), Codex reviews (read-only verdict), main thread routes.**

This is the source of truth. Per-phase prompts cite this doc rather
than re-declaring invocation patterns. AGENTS.md § 7 owns the
principles; this file owns the mechanics.

## 1. Three-role separation

| Role | Who | Tools | Output |
| --- | --- | --- | --- |
| **Orchestrator** | Main thread (you). Always. | Bash, Edit, Read, Write. May spawn subagents. | Plan files, cherry-picks, verdict promotions, commit decisions. |
| **Implementer** | Fresh `claude -p` process in a git worktree. | Read, Edit, Write, Bash, Grep, Glob. **NEVER Task/Agent.** | Commits on a `phase-<id>-<ts>` branch + structured `<phase>-output.json`. |
| **Reviewer** | `codex exec` against the implementer's worktree. | Read-only sandbox. | Verdict body (PASS / CONCERN / BLOCK) in log; main thread promotes to .md. |

Anti-patterns (do not violate):

- **Do not spawn an "orchestrator" subagent.** The main thread IS
  the orchestrator. (`feedback_orchestrator_thread.md`)
- **Do not use the Agent tool to spawn implementers.** It lacks
  per-spawn `--model` / `--effort` knobs.
- **Do not chain subagents to each other.** Implementer never spawns
  another agent (PREAMBLE + `--disallowedTools Task Agent` enforce
  this; B.1 incident 2026-05-03 lost work when wrapper agent
  misread "Spawn from main thread" as instruction to spawn-again).
- **Do not edit in a subagent's worktree from the main thread.**
  Worktree is the unit of isolation.

## 2. Implementer (Claude writes code)

```bash
PHASE=<id>                       # e.g. 11d-release-workflow, 11d-fix-1
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plans/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-${PHASE}-${TS}

git -C /home/coreyt/projects/fathomdb worktree add "$WT" \
    -b "phase-${PHASE}-${TS}" <BASELINE_COMMIT_SHA>

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

Invocation rules:

- Prompt body via stdin (`echo PREAMBLE; cat <prompt>` piped into
  `claude -p`). NOT positional.
- No `--bare` — breaks keychain-only OAuth path.
- No `--cwd` on claude — use `--add-dir` plus shell-side `cd`.
- `--effort` is intent-only; JSON envelope does not surface it.
- `--disallowedTools Task Agent` is the physical anti-chain guard.
- `--output-format stream-json --include-partial-messages --verbose`
  so the log grows continuously; monitor mid-flight via `tail -f` /
  `wc -l`. Final result is the last `result` event; parse with `jq`.
- Cross-worktree paths must be absolute.
- Spawn as `run_in_background: true` Bash; runtime notifies on
  completion. Do not poll.

## 3. Reviewer (Codex reads diff, returns verdict)

```bash
PHASE=<id>                                # match implementer PHASE
RTS=$(date -u +%Y%m%dT%H%M%SZ)
REV_LOG=/home/coreyt/projects/fathomdb/dev/plans/runs/${PHASE}-review-${RTS}.log
WT=/tmp/fdb-${PHASE}-<implementer-ts>     # implementer WT, post-commit

PROMPT=$(cat <<'EOF'
You are reviewing Phase <id>.

Branch: phase-<id>-<ts>, HEAD <sha>.
Baseline: <prior-CLOSED-sha>.

Required reading:
- dev/plans/prompts/<id>.md (the spec)
- dev/plans/runs/<id>-output.json (closure artifact)
- Commits <baseline-sha>..<head-sha> in chronological order
- <slice-specific design docs and ACs>

Verdict format:
- `## Verdict: PASS|CONCERN|BLOCK` markdown header
- Findings as `### N. [severity] short title` then `Refs:` (file:line
  citations) then 2-4 line explanation. Severity: high/medium/low.
- "What passed on inspection" wrap.
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

- **Model**: `gpt-5.4`. `gpt-5` is rejected on a ChatGPT account.
- **Effort**: `-c model_reasoning_effort=high` — codex defaults to
  lower effort; always set explicitly.
- **Sandbox**: `--sandbox read-only` — reviewer MUST NOT modify the
  worktree (drift would corrupt the diff under review).
- **Working directory**: `--cd "$WT"` — codex's wd flag. Distinct
  from claude's banned `--cwd`. Required so codex can read the
  implementer's WT files.
- **Stdin**: `printf '%s\n' "$PROMPT" | codex exec ... -`. The `-`
  positional reads stdin. **Do NOT use `echo "$PROMPT"`** — it loses
  escaping on multiline bodies. The `printf '%s\n'` form preserves
  literal newlines.
- **Reviewer prompt is inline per-slice**, not template-cat. Pack-5
  templates (`review-experiment.md`, `review-phase78-robustness.md`)
  remain for Pack-5 historical retries only; current practice writes
  targeted inline prompts.

## 4. Verdict promotion (codex sandbox cannot write)

Codex read-only sandbox cannot write the verdict file to canonical
paths. Main thread promotes the verdict body from the log:

1. Read `$REV_LOG`, locate the verdict block (typically last
   ~100 lines).
2. Write to canonical path: `dev/plans/runs/<phase>-review-<rts>.md`.
3. Canonical verdict format:

```markdown
# Phase <id> Review — codex gpt-5.4 verdict

Reviewer: `codex --model gpt-5.4 --sandbox read-only`.
Target: branch `phase-<id>-<ts>` HEAD `<sha>`.
Baseline: `<prior-CLOSED-sha>`.
Cherry-pick: `<sha-on-mainline>` on `<mainline-branch>`.
Review log: `dev/plans/runs/<phase>-review-<rts>.log`.

Sandbox note: reviewer could not write this file directly (read-only
sandbox). Main thread promoted the verbatim verdict from log line
<N>+.

## Verdict: PASS|CONCERN|BLOCK

### 1. [severity] short title
Refs: file:line citations
<2-4 line explanation>

## Addressed
<for fix-N passes: list what was fixed from prior verdict>

## What passed on inspection
<reviewer's positive findings>

## Reviewer process notes
<sandbox limits, what was/wasn't verified>

## Orchestrator triage
<main thread's KEEP / FIX-1 / OVERRIDE call with rationale>
```

## 5. Cherry-pick to mainline

Implementer commits sit on `phase-<id>-<ts>` branch in the worktree.
After reviewer PASS (or orchestrator override), cherry-pick the
slice onto the mainline branch:

```bash
# In main repo, on the release branch (e.g. 0.6.0-rewrite):
git cherry-pick <implementer-sha-1> <implementer-sha-2> ...
```

Cherry-pick (not merge) lets the orchestrator select exactly which
commits land — skips WT-internal experiments and keeps mainline
history linear.

Order matters: cherry-pick BEFORE spawning the reviewer when the
reviewer needs to see the mainline state, or AFTER review verdict
when the reviewer should see only the worktree branch. Phase 11+
practice: cherry-pick before review so mainline reflects intended
land state during reviewer execution.

## 6. Fix-1 remediation pass (on BLOCK / CONCERN)

If reviewer returns BLOCK or an actionable CONCERN, write a
targeted remediation prompt `dev/plans/prompts/<id>-fix-1.md` and
re-spawn the implementer in the **existing** worktree on the
**existing** branch (don't add a new worktree). Build new commits
on top of the prior head.

Spawn pattern: same as § 2 but with `WT=<existing-wt-path>` (no
`git worktree add`). Prompt operates additively — no rewrites of
landed commits.

After fix-1: cherry-pick the new commit(s), respawn the reviewer
for re-verdict. Iterate until PASS or orchestrator override.

Numbering convention: `<id>-fix-1.md`, `<id>-fix-2.md`, etc. Past
practice: ~1 fix-N pass per BLOCK; CONCERN often accepted via
override.

## 7. Orchestrator override (CONCERN accept)

When reviewer returns CONCERN and the finding is structural (e.g.
closure output.json self-reference: docs commit cannot contain its
own SHA) or prompt-induced (implementer followed the prompt
literally, but the prompt produced an awkward artifact), the
orchestrator may accept the CONCERN without further remediation.

Override discipline:

- Add explicit `**Orchestrator override <YYYY-MM-DD>: CONCERN
  accepted.**` line to the verdict .md.
- Document the rationale in `## Orchestrator triage` section.
- **Never override BLOCK** — that's a code or correctness issue;
  always remediate.

## 8. Closure output.json schema (per slice)

Implementer writes this as part of the slice; orchestrator reads it
to drive the cherry-pick + plan-update step.

```json
{
  "phase": "<id>",
  "baseline_sha": "<sha branch was cut from>",
  "branch": "phase-<id>-<ts>",
  "head_sha": "<HEAD after final commit>",
  "commits": ["<sha>: <subject>", "..."],
  "findings_addressed": ["..."],
  "blockers_encountered": [{"id": "...", "description": "...", "resolution": "..."}],
  "additional_changes_made_in_scope": ["..."],
  "agent_verify_result": "pass | fail (+ tail)",
  "next_step_for_orchestrator": "..."
}
```

Implementer-side discipline: write the JSON last, after all commits.
Implementer may commit the JSON itself as a final docs commit OR
leave it untracked (orchestrator promotes via Write tool before
worktree removal — see § 11).

## 9. Decision loop (per slice)

After implementer returns:

1. Read `dev/plans/runs/<phase>-output.json` (per § 8 schema).
2. Cherry-pick implementer commits from WT branch onto mainline
   release branch (per § 5).
3. Spawn codex reviewer on the WT branch HEAD (per § 3).
4. Promote codex verdict body from log to canonical
   `dev/plans/runs/<phase>-review-<rts>.md` (per § 4).
5. Decide:
   - **PASS** → close phase, advance.
   - **CONCERN (structural / prompt-induced)** → orchestrator
     override (per § 7), close phase.
   - **CONCERN (substantive) / BLOCK** → fix-1 remediation (per
     § 6), goto step 1.
6. Edit master plan ("Immediate Next Slice" section or equivalent):
   add `Phase <id> CLOSED` block, advance mainline pointer.
7. Commit plan + verdict + prompt files in single docs commit:
   `docs(<phase>): promote codex <verdict>; close Phase <id>;
   advance to <next>`.
8. After all sub-phases of a phase family close: worktree cleanup
   (per § 11).

## 10. Hard rules summary

1. Main thread orchestrates. No orchestrator subagent.
2. Bash to spawn `claude -p`. Never Agent tool.
3. PREAMBLE prepended via stdin. Always.
4. `--disallowedTools Task Agent` on every implementer spawn.
5. Codex reviewer is read-only. Always `--sandbox read-only`.
6. Verdict promoted by main thread (codex can't write).
7. Cherry-pick to mainline, never merge from WT branch.
8. Never override BLOCK. Override CONCERN only when structural or
   prompt-induced.
9. Worktree cleanup after phase family closes (§ 11).

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

## 12. Context management

Long multi-phase work (Phase 11 = 4 slices + 4 fix-1 passes ~$30 +
~80min; Phase 12 = 10 slices spanning weeks) requires discipline
about what lives where so context survives compaction + new
sessions.

### 12.1 Three context tiers

| Tier | Lifetime | What lives there |
|------|----------|------------------|
| **Subagent (implementer/reviewer)** | Single slice spawn (~10-30min) | Per-slice prompt + worktree files only. Fresh `claude -p` / `codex exec` every spawn — never grows. |
| **Main-thread conversation** | Single session (hours-days, until `/compact` or new session) | Plan-update decisions, codex verdict promotion, cherry-picks, HITL escalation. Limited by Claude Code's context window. |
| **On-disk** | Survives forever (compaction-safe) | Plan doc, prompts/, runs/, design docs, MEMORY, progress log, STATUS-<phase>.md. |

**Rule:** if it must survive a `/compact` or new session, it goes on
disk. Main-thread chat is throwaway working memory.

### 12.2 Cold-start resume protocol

Codified read order on every new session (or post-compact resume):

1. `AGENTS.md` — ~300-line invariants (front-loaded, cached).
2. `MEMORY.md` — index of feedback/project memories (auto-loaded).
3. `dev/plans/<release>-implementation.md` § "Immediate Next Slice"
   — next mainline pointer with prior-slice CLOSED blocks.
4. `dev/plans/<release>-implementation.md` § "Path to Client-Ready"
   (or equivalent slice-sequence section) — find current slice's
   row in the table.
5. `dev/progress/<release>.md` top entry — per-session log,
   newest on top.
6. `dev/plans/runs/STATUS-<phase>.md` — live state board for the
   current phase (slice scoreboard, open HITL questions, outstanding
   worktrees, next action).
7. Slice prompt file `dev/plans/prompts/<slice-id>.md` — the work
   itself.

Steps 1-6 are ~600 lines of focused reading; cold-resume to
oriented in <5min.

### 12.3 Per-slice context discipline

Each slice gets the proven Phase 11 substrate:

- `dev/plans/prompts/<slice-id>.md` — self-contained prompt
  (canonical pattern per §§ 2, 3 of this doc).
- `dev/plans/runs/<slice-id>-<ts>.log` — implementer stdout.
- `dev/plans/runs/<slice-id>-output.json` — closure artifact
  (per § 8).
- `dev/plans/runs/<slice-id>-review-<ts>.md` — promoted codex
  verdict.
- Worktree at `/tmp/fdb-<slice-id>-<ts>` — cleaned after PASS/override
  per § 11.

**Implementer subagent never sees prior-slice conversation context.**
Prompt carries everything it needs by file reference (baseline SHA,
AC ids, design-doc links, prior-slice `output.json` if relevant).

### 12.4 Plan doc as canonical state machine

The release plan (`dev/plans/<release>-implementation.md`) is the
authoritative source of "where are we?". Discipline:

- Each closed slice gets a `CLOSED` block in "Immediate Next Slice"
  section with cherry-pick SHA + codex verdict arc + key
  deliverables.
- "Immediate Next Slice" pointer advances in the **same commit**
  that closes the prior slice. No drift.
- Per-AC scoreboard rows in the slice-sequence table mark
  ✅/⚠️/❌/⏳ as work lands. State visible at a glance.
- **No state in chat that's also in the plan doc.** Chat is
  throwaway.

### 12.5 Phase STATUS board

For phases > 4 slices, create `dev/plans/runs/STATUS-<phase>.md`
modeled on Pack 5/6 STATUS.md. Single source of truth for the
phase's live state. Update at every slice close. Includes:

- Current slice in flight
- Per-slice status table (CLOSED / IN-FLIGHT / BLOCKED / not-started)
- Per-AC scoreboard for ACs the phase targets
- Parallelization plan (which slices can run concurrently)
- Open HITL questions (with decision-options + recommendation)
- Outstanding worktrees
- Recent decisions (newest on top)
- Next action
- Compaction-resume checklist

Worked example: `dev/plans/runs/STATUS-phase12.md`.

### 12.6 Compaction discipline

When main-thread context approaches limit:

1. **Land everything in flight** — commit any docs/plan updates,
   cherry-pick any pending slices, promote any pending verdicts.
2. **Update `dev/progress/<release>.md`** — newest-on-top entry
   summarizing the session.
3. **Update STATUS-<phase>.md** — "Current state" + scoreboard.
4. `/compact` (or start new session).
5. **First post-compact prompt:** cold-start resume — read the 6
   files in § 12.2.

**Do not compact mid-flight.** If a worktree is active, a codex
review is pending, or a HITL question is open, let it land first.
Capture state on disk. Then compact.

### 12.7 HITL gates as natural conversation boundaries

HITL-decision slices (e.g. 12-P perf re-confirm, 12-V-VERBS deferred
verbs, 12-GA release notes signoff) are natural session boundaries.
Main thread escalates, captures HITL decision in
`dev/progress/<release>.md`, then `/compact` or starts new session.
Reduces context bloat between distinct work phases.

### 12.8 Perf-experiment work (Pack 7+ shape)

Perf-experiment packs (Pack 5/6/6.G/7) follow a **different decision
loop** than mainline release slices:

- Mainline (Phase 11+): per § 9 — cherry-pick → review → PASS /
  CONCERN+override / BLOCK→fix-N.
- Perf-experiment (Pack 5+): KEEP / REVERT / INCONCLUSIVE with
  numeric decision rules; falsified levers go on a do-not-retry list
  (`dev/notes/performance-whitepaper-notes.md` § 5) to avoid burning
  cycles re-trying them.

Each perf pack carries:

- Hypothesis ladder (whitepaper § 6).
- Kept-experiments ledger (whitepaper § 4) + raw N=5 numbers.
- Reverted-experiments ledger (whitepaper § 5) — do-not-retry.
- Open questions (whitepaper § 8).
- Per-experiment prompt files (Pack 7 = `dev/plans/prompts/P7-*.md`).
- Separate STATUS board (`STATUS-pack7.md`) — perf state moves
  faster than release slices.

If Pack 7 spins up, the dual-track structure (mainline + perf) means
each track has its own STATUS board + decision loop but they share
the plan doc, MEMORY, and AGENTS.md invariants.

## 13. References

- `AGENTS.md` § 7 — principles (this doc owns mechanics).
- `MEMORY.md` entries: `feedback_orchestrator_thread.md`,
  `feedback_orchestrate_releases.md`, `project_orchestration_doc.md`.
- `dev/plans/prompts/01-orchestrator-resume.md` — Pack-5 resume
  wrapper; § 4 SUPERSEDED by this file, other sections retained as
  Pack-5 historical state.
- Phase 11 prompts (`dev/plans/prompts/11{a,b,c,d}-*.md`) as worked
  examples of this pattern.
- `dev/plans/runs/STATUS-phase12.md` — worked example of
  per-phase STATUS board.
- `dev/notes/performance-whitepaper-notes.md` — worked example of
  perf-experiment context discipline (Pack 5/6/6.G).
