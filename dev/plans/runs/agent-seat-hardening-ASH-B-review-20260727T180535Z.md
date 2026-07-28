# ASH-B Review — codex gpt-5.4 (§ 9 gate), round 1

- **Effort:** agent-seat hardening (cross-cutting; NO pico label, NO `R-20-xx` requirement id).
- **Slice:** ASH-B — Step 3, the path-scoped PreToolUse guard, shipped **UNWIRED**.
- **Branch:** `agent-seat-hardening` · **HEAD reviewed:** `81735bf0` · **slice baseline:** `822c6444`.
- **Reviewer:** `codex exec --model gpt-5.4 -c model_reasoning_effort=high --sandbox read-only`
  via `dev/agent-tools/codex-nostdin.sh`.
- **Round:** 1 of the § 6 cap (3 same-finding / 6 total — no `fathomdb-engine/src` touched).
- **Terminal transcript (TC-RUBRIC-7):**
  `dev/plans/runs/codex/agent-seat-hardening/ASH-B-20260727T180535Z.log`

## Verdict: CONCERN

### 1. [medium] `sed|perl|ruby -i` can wrongly DENY an allowed write, because the edit program is parsed as a path

Refs: `.claude/hooks/seat-path-guard.sh:27`, `.claude/hooks/seat-path-guard.sh:284`,
`scripts/tests/test_seat_path_guard.sh:247`

The header promises a deny-list that stays silent outside the MUST-NEVER column, but the
`sed|gsed|perl|ruby` branch sends **every** non-flag token through `consider()` — including the
edit *program* itself. So `sed -i 's#src/rust/lib.rs#x#' dev/plans/note.md` denies, even though
the only write target is an allowed `dev/plans/**` file. The suite proves the deny case for
`sed -i src/...` but does not cover this false-positive path.

### 2. [low] Glued clobber redirection `>|FILE` slips the redirection matcher

Refs: `.claude/hooks/seat-path-guard.sh:254`, `.claude/hooks/seat-path-guard.sh:259`

The comment claims the scanner handles `>|`, but the code only catches the spaced form (`>| FILE`)
or glued `>FILE` / `>>FILE`. `printf x >|src/rust/lib.rs` falls through both regexes and is not
denied. A real write form on the load-bearing Bash surface.

## What passed on inspection

- **Hard constraint 1 — the hook is genuinely UNWIRED.** Verified three ways: the worktree has no
  tracked `.claude/settings*.json`; the primary checkout's `.claude/settings.json` PreToolUse hooks
  do not reference `seat-path-guard`; `settings.local.json` does not wire it either.
- **(a) The `.gitignore` negation is correct and safe.** `!.claude/hooks/` re-includes the parent
  directory, `.claude/hooks/*` re-ignores all children, `!.claude/hooks/seat-path-guard.sh`
  re-includes exactly one file. Confirmed it does NOT newly track `.claude/settings.json`,
  `.claude/settings.local.json`, or the three local hooks. The ordering semantics were checked
  against git's rules, not merely believed.
- **(b) Deny-list narrowing ACCEPTED** as a Phase-1 choice: narrower than the full § 1.2 MAY/MUST
  table, but the rationale is coherent — avoid over-blocking ordinary coordinator edits so the guard
  stays enabled.
- **(c) Fail-open posture sound.** `trap 'exit 0' EXIT`, the deliberate omission of
  `set -euo pipefail`, and "silence on allow" are internally consistent; the deny path stays
  reachable because it travels through stdout JSON, not exit status.
- **(d) The suite is non-vacuous, and arm 16 is not vacuous either.** The deny arms genuinely
  discriminate a real guard from a no-op; arm 16 proves its detector on a positive fixture first,
  then resolves candidate settings paths via `git rev-parse --git-common-dir` before checking real
  files.
- **(e)** Aside from findings 1 and 2, the Bash scanner does the right high-value work, and the
  "test source beats `scripts/**`" precedence is structurally preserved (basename check before the
  segment walk).
- **(f) Scope honesty adequate.** MultiEdit / NotebookEdit named as out of scope; the eight
  documented evasions framed correctly as "best-effort, not a sandbox" rather than silently widened
  into stronger guarantees.
- The `scripts/agent-test.sh` registration hunk is correct and keeps the suite live in the harness.

## Reviewer process notes

The codex sandbox could not run `git` porcelain (`bwrap: loopback: Failed RTM_NEWADDR: Operation
not permitted`), so the reviewer anchored on direct file reads, the worktree reflog, and direct
inspection of the primary checkout's local `.claude/settings*.json` for the unwired-state check.

## Orchestrator triage

**NOT overridden — routed to fix-1.** Both findings are substantive parser defects in the code
under review, not the *structural* or *prompt-induced* category § 7 permits an orchestrator to
accept. They are also cheap and precisely localized, and each is exactly the kind of defect the
suite exists to catch, so each fix must arrive with the test arm that would have caught it.

Finding 2 is additionally a **self-inconsistency**: the header claims `>|` coverage the code does
not implement. Whichever way it is resolved, code and comment must agree at the end.

Fix-N round count for ASH-B after this verdict: **1** (bound: 3 same-finding / 6 total).
