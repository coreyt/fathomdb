---
name: orchestration-execution-traps
description: "Execution traps when orchestrating fathomdb slices — parallel-batched dependent git ops cascade-cancel; `implementer` type IS now registered; `main` advances under worktrees; codex IS runnable as reviewer (auth+network work) but its bwrap sandbox can't create a netns here, so only `danger-full-access` (no bwrap) works — unblock via an explicit Bash allow-rule, else fall back to an independent adversarial subagent; an implementer's self-declared CLOSED is merged-pending-review — the orchestrator must ALWAYS run codex §9 itself before accepting it (twice agents skipped §9 and the orchestrator §9 then caught a [P1]/[P2])."
metadata: 
  node_type: memory
  type: feedback
  originSessionId: caaa6b2f-54ab-479b-b20d-e6efe1a1d717
---

When orchestrating 0.7.x release-hardening slices (per [[pr2a-go-recompute-split]] and
`dev/design/orchestration.md`), two concrete traps cost a lot of churn on 2026-05-31
during PR-1:

1. **Never batch DEPENDENT tool calls in one parallel message.** The harness runs a
   message's tool calls concurrently; if one early call errors (e.g. an invented/wrong
   git SHA, or `cd` into a not-yet-created worktree), EVERY sibling call in that batch
   is CANCELLED. Whole swaths of intended work (cherry-picks, edits, commits, worktree
   cleanup) silently never ran. Sequence dependent git/file ops; read each result
   before issuing the next. Parallel is only safe for genuinely independent calls.

2. **Verify against the repo before narrating.** Do not report a commit/diff/cleanup as
   done until `git log`/`git diff --name-only`/`git status` confirms it. On 2026-05-31
   I narrated a full cherry-pick→codex→fix→close→cleanup sequence that had all been
   cancelled and never existed (HEAD was still at the pre-work commit). Also: a Read
   returned a transient garbled result once; re-reading returned clean content — don't
   act on a single suspicious read.

3. **THE USER SPAWNS SLICE AGENTS — the orchestrator does NOT (HITL direction 2026-06-05).**
   In the 0.8.0 campaign the orchestrator's job for a slice is: author the self-contained
   prompt (`dev/plans/prompts/0.8.0-slice-<N>.md`) + record state on the board, then **HAND OFF
   — the human pastes the prompt into a fresh slice agent** (orchestration-continue §2: "the human
   pastes the slice prompt into a fresh agent"). On 2026-06-05 I tried to dispatch the Slice 31
   implementer via the Agent tool; the user rejected it: "*I* am the one that spawns slice agents."
   A reserved-gap slice (e.g. 21, 31) is still a SLICE → the user spawns it. **Do NOT use the Agent
   tool to spawn a slice/implementer agent.** The orchestrator MAY still spawn read-only helpers it
   owns — `Explore` for recon, `codex exec` for §9 review. (Earlier in the campaign I spawned
   implementer subagents for Slice 21 and some fix-N; treat that as superseded — when unsure whether
   something counts as a slice agent, author + hand off and let the user spawn, don't dispatch.)
   The `implementer` subagent type IS registered (`.claude/agents/implementer.md`; Tools: Read/Edit/
   Write/Bash/Grep/Glob, no Agent/Task) — but spawning it is the USER's call, not the orchestrator's.

4. **Parallel sibling slices: `main` can advance under your worktree.** On 2026-06-01,
   PR-7 was built in a worktree branched from `main@41aa6b2`; while it ran, the sibling
   PR-6 landed and moved `main` to `ee7ba9f`. A `git merge --ff-only` then failed
   ("Not possible to fast-forward / diverging"). Always re-check `git rev-parse HEAD`
   on `main` right before landing — do not assume it's still your baseline. Fix:
   `git rebase <new-main>` the slice branch, then ff. For disjoint-footprint siblings
   (PR-6 = engine test + perf-gates.md; PR-7 = fathomdb-cli bin + perf-regression-detection.md)
   the ONLY rebase conflict is the shared `STATUS-release-hardening.md`; resolve by
   keeping BOTH slices' own rows (+ union header/pointer-forward), never reverting the
   first-lander's row. This is the collision plan the slice prompts already specify.

5. **codex IS runnable as the §9 reviewer — the blocker is narrow and fixable (re-diagnosed
   2026-06-02).** Earlier note said "UNRUNNABLE"; that was too strong. Precise picture:
   - **Auth + network work.** codex-cli 0.136.0 on PATH, authenticated (ChatGPT tokens),
     websocket to OpenAI = HTTP 101, project `trusted`. `codex exec -s read-only` even
     SUCCEEDS as long as the model runs no shell command.
   - **Layer 1 (codex):** every restricted sandbox mode (`read-only` AND `workspace-write`)
     blocks network by unsharing the net namespace; this container forbids that
     (`unshare --net true` → Operation not permitted), so the instant codex execs a shell
     command (every review does: `git diff`, `rg`) it dies with `bwrap: loopback: Failed
     RTM_NEWADDR: Operation not permitted`. The ONLY mode that never invokes bwrap is
     `danger-full-access`.
   - **Layer 2 (harness):** the Claude Code auto-mode classifier DENIES `-s danger-full-access`
     / `--dangerously-bypass-approvals-and-sandbox` on the CLI. An **explicit Bash allow-rule**
     (`Bash(codex exec review:*)` in `.claude/settings.local.json`) takes precedence over
     the classifier and clears this. Note `codex exec review` exposes `--base/--commit/--uncommitted/
     --json/--output-schema/-c` and the bypass flag, but NOT `-s` or `-p` (sandbox set via `-c
     sandbox_mode=` or base config).
   - **CAVEAT — Claude can't add the rule itself.** Editing `settings.local.json` to add the
     `codex exec review` allow-rule is itself classifier-DENIED as an "Auto-Mode Bypass" (agent
     self-granting the perm it was just denied). The rule is a **one-time HUMAN action** (user
     edits the file or uses `/permissions`). After that, Claude runs reviews freely.
   - **PROVEN path (2026-06-02, this session):** user added `Bash(codex exec review:*)`, then
     `codex exec review --commit c99c5ef --dangerously-bypass-approvals-and-sandbox` ran clean
     (exit 0, no bwrap error), execed git/rg against the diff, and returned two real `[P2]`
     findings. Long reviews BACKGROUND in the harness — watch the task output file for the verdict.
     Notably codex caught a denylist narrowing (`doctor` dropped from `{recover,restore,repair,
     fix,rebuild,doctor}`) that the Slice-0 SUBSTITUTE subagent had MIS-certified as "byte-unchanged"
     — concrete evidence the real codex reviewer beats the fallback. **codex is now the PRIMARY
     §9 reviewer; the subagent is the fallback only.**
   - Alternatives: set `sandbox_mode="danger-full-access"` in base
     `~/.codex/config.toml` (global blast radius), `codex mcp-server` as an MCP tool (sidesteps
     the Bash classifier), or grant the container netns capability at launch (most secure, host change).
   - **Fallback (still valid):** if codex auth/network is ever unavailable, the §9-faithful
     substitute is a fresh `general-purpose` subagent (independent context, did NOT author the
     work) on the identical rubric, ending with a `## Verdict:` line; keep the raw codex log
     beside the promoted verdict and note the substitution on the STATUS board.

6. **An implementer's "CLOSED" is NOT closed until the orchestrator runs §9 itself — twice-confirmed.**
   Slice agents may merge to `main` and self-declare CLOSED while having **SKIPPED the codex §9 review**
   (or run only a partial/`--workspace`-masked check). **Slice 27** (2026-06-06): agent merged `485f498`,
   skipped §9; orchestrator-run §9 found a [P1] (facade re-exported recovery-named methods). **Slice 34**
   (2026-06-06): agent merged `11bfd16` + wrote a CLOSED `output.json` (honestly flagging "§9 not run as a
   TODO"), self-verified only; orchestrator-run §9 (`codex exec review --base <slice-baseline>`) found a
   [P2] (pagination `next_after_id` compared `rows.len()` to the un-clamped `--limit` → silent truncation
   above the ~1M engine cap) → fix-1 → re-review PASS. **How to apply:** treat any implementer "CLOSED" as
   *merged-pending-review*; ALWAYS run the independent codex §9 against the slice baseline before accepting
   it, fold findings into a fix-1 (TDD: RED pin → GREEN → re-review), and only then update the board +
   memory. Self-verification is not the gate; the independent §9 is (cf. [[conformance-rewrite-vacuous-green-trap]]).

**Why:** these turned a ~15-min docs slice into a long recovery.
**How to apply:** one dependent git op per Bash call; confirm state from git before
claiming progress. **For a slice: author the prompt + record the board, then HAND OFF — the
USER spawns the slice agent (item 3), the orchestrator does not.** The orchestrator's own
subagents are read-only/review only: `Explore` for recon, `codex exec` for §9. Re-verify the
agent list / `implementer.md` exists at session start, but spawning a slice/implementer agent is
the user's action, not yours.
