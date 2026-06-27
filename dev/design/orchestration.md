---
title: Multi-Agent Orchestration Pattern — the cross-release runbook
date: 2026-05-17
revised: 2026-06-26
applies_to: all releases (this is the stable METHOD; per-release deliverables live in dev/plans/<release>-plan.md)
desc: Canonical Claude-implementer + Codex-reviewer spawn discipline for orchestrated work
blast_radius: dev/plans/prompts/*; .github/workflows/release.yml; release engineering; all multi-agent slices
status: locked
supersedes:
  - dev/plans/0.6.0-Phase-9-Pack-5-performance-diagnostics.md § 0.1 (Pack-5-era invocation pattern)
  - dev/plans/prompts/01-orchestrator-resume.md § 4 (lifted here; resume doc now points back)
  - dev/plans/prompts/00-handoff-execute.md § 3-5 (Pack-5-era handoff)
  - § 2 (this doc) — the `claude -p` subprocess implementer pattern, superseded 2026-05-31 by the `implementer` subagent (clean replacement; old bash recipe removed)
---

# Multi-Agent Orchestration Pattern

Canonical pattern for orchestrated work: **Claude implements (writes
code), Codex reviews (read-only verdict), main thread routes.**

This is the source of truth. Per-phase prompts cite this doc rather
than re-declaring invocation patterns. AGENTS.md § 7 owns the
principles; this file owns the mechanics.

> **This is the runbook, not a plan.** It is **release-independent** —
> the *method* (roles, state spine, preflight, decision loop, failure
> recovery) is stable across every release. Per-release **deliverables**
> (scope, slice ladder, DoD, acceptance criteria) live in
> `dev/plans/<release>-plan.md`, authored from
> `dev/plans/prompts/PLAN-TEMPLATE.md`. **Lesson learned about *how* to
> run work → edit this file. New deliverable → edit the plan.** Worked
> examples below name specific historical phases (Phase 11, Pack 5,
> phase12) only as illustrations of the stable method; do not read the
> release numbers as the method's scope. A self-contained, portable
> distillation of this method is `dev/agent-harness-bootstrap-prompt.md`.

## 1. Three-role separation

| Role             | Who                                              | Tools                                                      | Output                                                                     |
| ---------------- | ------------------------------------------------ | ---------------------------------------------------------- | -------------------------------------------------------------------------- |
| **Orchestrator** | Main thread (you). Always.                       | Bash, Edit, Read, Write. May spawn subagents.              | Plan files, cherry-picks, verdict promotions, commit decisions.            |
| **Implementer**  | `implementer` subagent in a main-thread-owned worktree. | Read, Edit, Write, Bash, Grep, Glob. **No Task/Agent** (omitted from the agent def). | Commits on a `phase-<id>-<ts>` branch + structured `<phase>-output.json`.  |
| **Reviewer**     | `codex exec` against the implementer's worktree. | Read-only sandbox.                                         | Verdict body (PASS / CONCERN / BLOCK) in log; main thread promotes to .md. |

Anti-patterns (do not violate):

- **Do not spawn an "orchestrator" subagent.** The main thread IS
  the orchestrator. (`feedback_orchestrator_thread.md`)
- **Do not chain subagents to each other.** The implementer never
  spawns another agent — the `implementer` agent type omits Agent/Task
  (the physical guard; replaces the old `--disallowedTools Task Agent`).
  B.1 incident 2026-05-03 lost work when a wrapper agent misread
  "Spawn from main thread" as instruction to spawn-again.
- **Do not let the implementer own its worktree.** The main thread
  creates it (`git worktree add` from a chosen baseline); never use
  Agent-native isolation — it forfeits baseline control and § 11 cleanup.
- **Do not edit in a subagent's worktree from the main thread.**
  Worktree is the unit of isolation.

## 1.5 State spine

The per-slice flow below (§§ 2–11) is a state machine. Keep it
honest by deriving position from the repo, not from memory:

**State is a derived function of durable on-disk witnesses.
`STATUS-<phase>.md` is a cache of that derivation, not the source of
truth.** A slice's current state is the furthest state whose witness
exists and verifies:

| State              | Witness (what proves it on disk)                                              |
| ------------------ | ---------------------------------------------------------------------------- |
| `WORKTREE_CREATED` | `git worktree list` shows WT + branch at the chosen baseline                 |
| `IMPLEMENTING`     | *(sole transient state — Agent task active; no durable witness)*             |
| `IMPLEMENTED`      | `output.json` present at its path **and** branch head advanced past baseline |
| `CHERRY_PICKED`    | equivalent commit verified on mainline (`git log --grep` / SHA)              |
| `REVIEWED`         | `<phase>-review-<rts>.md` exists with a `## Verdict:` line                   |
| `CLOSED`           | plan has `Phase <id> CLOSED` block + promoted verdict                        |
| `CLEANED`          | `git worktree list` no longer shows the WT                                   |

Each transition's **guard** = the prior state's witness exists and
verifies; its **effect** = produce the next witness. §§ 5–11 are the
transition bodies; § 9 is the transition function.

Four invariants (the entire added formality):

1. **Derived position.** State = furthest state whose witness exists
   and verifies. On any conflict, **witnesses win over `STATUS`.**
2. **Artifact-gated transitions, never belief-gated.** Advance only
   when the prior witness is present. Never cherry-pick before
   `output.json`; never spawn fix-N before the implementer's
   completion event **and** its commits exist.
3. **No undefined transition.** Any state with no satisfiable next
   step — expected witness missing (failed implementer), a BLOCK
   fix-N can't clear, fix-N past a small bound — **halts to HITL**
   rather than improvising.
4. **Idempotent re-entry.** On resume, re-derive from witnesses and
   continue at the first incomplete transition; a transition whose
   witness already exists is a verified no-op.

## 1.6 Preflight gate (run before every worktree spawn)

Every `WORKTREE_CREATED` transition is gated by a preflight. Witnesses
beat belief (§ 1.5 invariant 1) *before* the spawn too: the orchestrator
verifies the repo and the freshly-cut worktree on disk, never assumes
they are sane. This exists because two slices were lost when a worktree
was cut from a ~206-commit-stale base (`agent-worktree-stale-base-trap`)
— a silent failure a one-command gate catches.

```bash
# After: git worktree add "$WT" -b "slice-<N>-<ts>" "$(git rev-parse main)"
scripts/preflight.sh --worktree "$WT" --expect-closed <DEP> --plan dev/plans/<release>-plan.md
# exit 0 = spawn; exit 1 = STOP, read the HARD lines, fix, re-run.
```

What it gates (all derived from disk; HARD = blocks the spawn):

- **Stale-base guard (HARD).** The worktree HEAD must be current `main`
  or a descendant of it. A worktree whose HEAD is *behind* main (main is
  not its ancestor) means it was cut from a stale base → re-create off
  `$(git rev-parse main)`. This is the load-bearing check.
- **Dependency CLOSED (HARD).** `--expect-closed <N>` confirms the
  declared dependency slice has a `CLOSED` witness in the plan before a
  dependent spawns (§ 1.5 invariant 2 applied to cross-slice order).
- **No mid-operation canonical repo (HARD).** Refuses to spawn while the
  canonical checkout is mid-merge/rebase/cherry-pick.
- **Disk headroom (HARD).** Each worktree carries its own `target/`;
  `--min-disk-gb` (default 10) refuses to spawn into a near-full disk.
- **Clean tracked source (WARN).** `dev/` doc churn is expected and
  ignored; dirty `src/`/`scripts/`/`mkdocs.yml` warns (a worktree would
  inherit the uncommitted-or-not state of the base commit).
- **Build-isolation reminder (INFO).** Never `maturin develop` /
  `pip install -e` from a worktree — it rebinds the shared `.venv` to the
  worktree tree. GPU/maturin builds happen on the MAIN tree only.

A HARD fail is an off-spine halt (§ 1.5 invariant 3): fix the cause, do
not spawn around it.

## 2. Implementer (Claude writes code)

> **Superseded 2026-05-31.** Implementers were previously launched as
> `claude -p` subprocesses piped a PREAMBLE+prompt over stdin. That
> recipe is retired and removed. Implementers are now Claude Code
> subagents (`subagent_type: implementer`). Branch-point and worktree
> ownership stay with the main thread — only the spawn mechanism
> changed.

The main thread creates the worktree (unchanged — this is what gives
branch-point control), then spawns the `implementer` subagent into it:

```bash
PHASE=<id>                       # e.g. 11d-release-workflow, 11d-fix-1
TS=$(date -u +%Y%m%dT%H%M%SZ)
WT=/tmp/fdb-${PHASE}-${TS}

git -C /home/coreyt/projects/fathomdb worktree add "$WT" \
    -b "phase-${PHASE}-${TS}" <BASELINE_COMMIT_SHA>

# Gate the spawn (§ 1.6) — STOP on exit 1, never spawn around a HARD fail:
scripts/preflight.sh --worktree "$WT" --expect-closed <DEP> --plan dev/plans/<release>-plan.md
```

Then one `Agent` call from the main thread:

- `subagent_type: "implementer"` — tool contract (Read, Edit, Write,
  Bash, Grep, Glob; **no Agent/Task**) lives in
  `.claude/agents/implementer.md`. Do not re-list tools here.
- `model: "opus"` or `"sonnet"` per slice. (Tier, not exact pin;
  `--effort` no longer applies — it was always intent-only.)
- `run_in_background: true` — runtime notifies on completion; do not
  poll. The completion notification is the sole
  `IMPLEMENTING → IMPLEMENTED` trigger (§ 1.5 invariant 2).
- `isolation` is **not** set. The worktree already exists and is
  main-thread-owned; Agent-native isolation would forfeit baseline
  control and § 11 cleanup.
- Prompt body carries the per-spawn facts the subagent cannot infer:

      worktree: <ABS_WT_PATH>          (operate only here; not your cwd)
      branch:   phase-<id>-<ts>
      baseline: <BASELINE_COMMIT_SHA>
      output:   <ABS_PATH to <phase>-output.json>   (§ 8 schema)
      <then the slice spec: ## Mandate / ## What to do / commit policy>

Invocation rules:

- Worktree is created by the main thread, never by the subagent and
  never via Agent isolation. Cross-worktree paths must be absolute.
- The `implementer` agent omits Agent/Task — the physical anti-chain
  guard (replaces the old `--disallowedTools Task Agent`). Never grant
  them.
- Per-spawn facts go in the Agent prompt (worktree/branch/baseline/
  output), not a stdin PREAMBLE. The durable role contract lives in
  the agent definition.
- Monitor mid-flight with `TaskOutput` / `TaskGet` or `/workflows`;
  the durable artifact is still `<phase>-output.json` on disk (§ 8).
  Final subagent result text returns directly to the main thread — no
  `jq` log-parsing.
- On completion, gate the transition (§ 1.5 invariant 2): if
  `output.json` is absent **or** the result text reports a blocker,
  the slice is FAILED (no witness — an off-spine halt per invariant 3,
  not a § 1.5 state row) — triage (fix-N or abandon), never
  cherry-pick (§ 1.5 invariant 3).
- `git commit` / `git add` for the slice are allowlisted in
  `.claude/settings.local.json`; the `wake commit-check` PreToolUse
  hook is non-blocking (exits 0).

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

If reviewer returns BLOCK or an actionable CONCERN, write a targeted
remediation prompt `dev/plans/prompts/<id>-fix-1.md`, then re-spawn a
**fresh** `implementer` subagent into the **existing** worktree on the
**existing** branch — same Agent spawn as § 2 but with no
`git worktree add`, `baseline` set to the prior head, and an
`address:` line pointing at the promoted verdict .md:

      worktree: <ABS_EXISTING_WT_PATH>    (operate only here)
      branch:   phase-<id>-<ts>           (existing — build on top)
      baseline: <PRIOR_HEAD_SHA>          (the head the fix extends)
      output:   <ABS_PATH to <phase>-output.json>
      address:  dev/plans/runs/<phase>-review-<rts>.md   (the findings)

Commits are additive; never rewrite landed commits.

Fresh-spawn is the intended mechanism, **not** a fallback for missing
conversational continuity: per § 12.1 the implementer's state lives on
disk (worktree diff + output.json + verdict.md), so a fresh subagent
reading the verdict has everything it needs. Do not wait on / reach
for SendMessage.

After fix-N: cherry-pick the new commit(s), re-spawn the reviewer for
re-verdict. Iterate until PASS or orchestrator override. A BLOCK that
fix-N cannot clear, or fix-N past a small bound, halts to HITL
(§ 1.5 invariant 3) rather than looping indefinitely.

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

## 9. Decision loop (per slice) — the transition function

This is the transition function for the § 1.5 state spine; each step
is gated on the prior state's witness. After implementer returns:

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
2. Main thread spawns implementers via the Agent tool,
   `subagent_type: implementer`. Worktree is main-thread-owned
   (`git worktree add`); never use Agent isolation. (The `claude -p`
   subprocess pattern is retired — § 2.)
3. Per-spawn facts (worktree path, branch, baseline SHA, output path)
   passed in the Agent prompt. Always. Durable role lives in the agent
   definition.
4. The `implementer` agent type omits Agent/Task — the physical
   anti-chain guard. Never add them to its tool list.
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

| Tier                                | Lifetime                                                     | What lives there                                                                                                        |
| ----------------------------------- | ------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------- |
| **Subagent (implementer/reviewer)** | Single slice spawn (~10-30min)                               | Per-slice prompt + worktree files only. Fresh `implementer` subagent / `codex exec` every spawn — never grows.          |
| **Main-thread conversation**        | Single session (hours-days, until `/compact` or new session) | Plan-update decisions, codex verdict promotion, cherry-picks, HITL escalation. Limited by Claude Code's context window. |
| **On-disk**                         | Survives forever (compaction-safe)                           | Plan doc, prompts/, runs/, design docs, MEMORY, progress log, STATUS-<phase>.md.                                        |

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

## 13. Failure & recovery catalog

Hard-won failure modes, consolidated here so a fresh orchestrator meets
them in the runbook rather than rediscovering them. Each is a real
incident captured in a `MEMORY` `feedback_*`/`project_*` entry; the
memory holds the detail, this table is the index + the move.

| When you observe | Likely cause | The move |
| ---------------- | ------------ | -------- |
| Implementer's commits are missing / based on old code; "main moved under me" | Worktree cut from a **stale base** (Agent-native isolation, or a hand-typed baseline that wasn't current `main`). | The § 1.6 preflight HARD-fails this. Re-create the worktree off `$(git rev-parse main)`; never use Agent `isolation` (§ 1, § 2). (`agent-worktree-stale-base-trap`) |
| A wrapper/implementer agent spawned **another** agent; work vanished | Implementer read "spawn from main thread" as "spawn again"; agent had Agent/Task tools. | The `implementer` agent type omits Agent/Task — the physical guard. Never grant them. Main thread is the only orchestrator. (B.1 incident; § 1) |
| A run reports green but a later step shows the real command failed (e.g. pytest "exit 0" was a wrapper's trailing `echo`) | A **background/wrapper exit masked the real command's exit**. | Read `PIPESTATUS`/`$?` of the *actual* command; cross-check the green claim against printed output; a collection/import error ≠ a code defect — check the harness's build flags first. (`background-exit-masks-real-exit`) |
| A conformance/parity test rewrite **passes on first run** (suspiciously) | Vacuously-green test: hard-coded surface enumeration, or same-file duplicate "parity" so drift is undetectable. | Demonstrate the catch in RED first (real `dir()` introspection minus an exclusion set; a single shared allowlist both bindings read). Independent codex § 9 is load-bearing here. (`conformance-rewrite-vacuous-green-trap`) |
| `output.json` absent, or result text reports a blocker | Implementer FAILED (no `IMPLEMENTED` witness). | Off-spine halt (§ 1.5 invariant 3): triage (fix-N or abandon), **never** cherry-pick a slice with no witness (§ 2). |
| Codex returns BLOCK that fix-N cannot clear, or fix-N exceeds a small bound | Mis-scoped slice or a genuine spec problem. | Halt to HITL (§ 1.5 invariant 3, § 6). Do **not** loop fix-N indefinitely; do **not** override BLOCK (§ 7). |
| A destructive bash call (bundled `rm`/`worktree remove`/`branch -D`) is denied | Permission model denies bundled destructive ops. | One destructive op per Bash call (§ 11). Never `find -delete`. Surface to HITL if still blocked — don't seek workarounds (AGENTS.md § 11). |
| Slice prompt asserts "test X (does/does not) assert Y" and the mechanism depends on it | Prompt anchor drifted; the load-bearing claim is **false**. | Implementer must read test X in full at baseline and STOP+escalate if false (a defect report on the prompt, not over-scope). Authors: verify such claims before writing them. (`slice-prompt-verify-test-claims`) |
| Same failure mode hit twice | Thrash. | Stop. Re-read the failing test + the relevant ADR/`feedback_*`. Do not loop a third time; externalize plan and `/clear` if needed (AGENTS.md § 5, § 8). |

The general loop under any surprise: **notice → witness it on disk →
classify (in-slice deviation vs cross-slice/spec) → smallest fix or
escalate → never hide.** A clean halt with state captured on disk is a
good outcome, not a failure.

## 14. Reviewer topology & empirical resolution

The default review is one codex pass per slice on the worktree branch
(§ 3). Two refinements for harder cases:

- **Topology — siloed vs joint.** Independent slices get **siloed**
  reviewers (one per slice, parallel — reviewers don't share context).
  When a set of slices share an invariant (a cross-binding contract, a
  schema both touch), run **one joint review** over the combined diff so
  the reviewer can see the interaction a siloed pass would miss. Choose
  joint only for the genuine cross-cutting concern; default siloed.
- **Empirical, not argued, resolution.** When a verdict turns on a
  *behavior* question — "does this actually reproduce?", a reviewer and
  implementer disagree on what the code does, a flaky test — do not
  resolve it by argument or by trusting the orchestrator's model.
  **Construct a minimal scratch test, run it, decide from the
  observation, then delete the scratch** before the closing commit. Use
  this for contradictions / unverified assumptions; skip it for API-design
  or fully-specified-contract questions where there is nothing to observe.
- **Cadence.** One pass per slice + at most one cross-slice joint pass.
  Beyond that, review hits diminishing returns — spend the next cycle on
  a falsifiable test, not a third read.

## 15. References

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
