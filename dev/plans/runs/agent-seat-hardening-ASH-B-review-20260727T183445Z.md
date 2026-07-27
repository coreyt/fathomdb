# ASH-B fix-1 Review — codex gpt-5.4 (§ 9 gate), round 2

- **Slice:** ASH-B fix-1 · **Branch:** `agent-seat-hardening` · **HEAD reviewed:** `b8d037fa`
  (fix-1 diff `81735bf0..b8d037fa`).
- **Reviewer:** `codex exec --model gpt-5.4 -c model_reasoning_effort=high --sandbox read-only`
  via `dev/agent-tools/codex-nostdin.sh`.
- **Fix-N round count after this verdict:** **2** (bound: 3 same-finding / 6 total).
- **Terminal transcript (TC-RUBRIC-7):**
  `dev/plans/runs/codex/agent-seat-hardening/ASH-B-fix-1-retry-20260727T183445Z.log`

> **REVIEWER-PROCESS INCIDENT — first round-2 attempt was VOID and is retained as evidence.**
> `dev/plans/runs/codex/agent-seat-hardening/ASH-B-fix-1-20260727T182705Z.log` is **NOT a valid
> verdict.** Given the promoted round-1 verdict `.md` as required reading, codex reproduced that
> document **verbatim** — including the orchestrator's own `## Orchestrator triage` section and its
> "round count: 1" line — instead of analysing the fix. It was caught by reading the output rather
> than the verdict header, and discarded. The retry above forbade opening any
> `*-review-*.md` and inlined the prior findings into the prompt instead; the retry log records that
> the reviewer complied. **Generalisable lesson: never hand a reviewer the previous verdict as a
> file; inline the findings.** A verdict header alone is not evidence a review happened.

## Verdict: CONCERN

### 1. [medium] `sed -i -f - src/...` is a NEW false negative introduced by fix-1

Refs: `.claude/hooks/seat-path-guard.sh:344`, `:373`, `:381`, `:399`;
`scripts/tests/test_seat_path_guard.sh:614`

While waiting for the `-f` / `--file` program operand, the branch treats any `-…` token as "a flag,
not the program" and keeps searching. For the valid `sed -i -f - src/rust/lib.rs` (or
`--file - src/…`), the `-` means *read the script from stdin*, but the hook skips it, then consumes
`src/rust/lib.rs` as the "first bare operand = program" — so **no protected file operand is ever
considered**. This is exactly the dangerous direction fix-1 was scoped to avoid. **D1 is NOT fully
closed.**

### 2. [low] The redirection header still is not exact about what the suite proves

Refs: `.claude/hooks/seat-path-guard.sh:296`; `scripts/tests/test_seat_path_guard.sh:653`

The runtime `>|` fix is correct, but the header claims the listed redirection forms are "each
verified by an arm" and the list includes `&>>`. Arms 56e/56f cover `&>` **only**; there is no `&>>`
arm. A doc/test exactness mismatch rather than a behaviour hole — but eliminating exactly this class
of drift was the point of the round.

## What passed on inspection

- **The implementer's correction of D1's witness is CONFIRMED right.** With segment-based matching,
  `sed -i 's#src/rust/lib.rs#x#' …` never produces a bare `src` segment, so that specific command was
  already allowed; the round-1 diagnosis of the *code* was sound but its demonstration was not. Arms
  47-50 do target real pre-fix mechanisms.
- **D2 is CLOSED.** `scan_bash_command` now folds `>|` to `>` before splitting on `|`, so the glued,
  spaced and `2>|` forms all reach the redirection matcher.
- **`absorb_quoted_word` (the net-new logic) survived adversarial reading.** Bounded by token count;
  no infinite-loop path found; the `'\''` and double-quoted-apostrophe cases behave as intended.
- **No regression.** Only `orchestrator|steward` are guarded; `implementer` stays untouched; only
  `Edit|Write|Bash` are handled; heredoc bodies are stripped; test-source precedence still wins;
  allow paths stay silent and fail-open.
- **Hard constraints hold.** The hook is still unwired in every settings file the reviewer could
  read, and `.claude/agents/implementer.md`'s frontmatter is unchanged.

## Reviewer process notes

Git-history commands and standalone `bash -n -c …` probes were intermittently rejected by the codex
sandbox (`bwrap: loopback: Failed RTM_NEWADDR: Operation not permitted`), so the reviewer confined
findings to what is derivable from the current source and the on-disk settings.

## Orchestrator triage

**NOT overridden — routed to fix-2.** Finding 1 is a false negative in the load-bearing path, which
is the one direction this guard must not fail in; finding 2 is the precise doc/test drift the round
existed to remove. Neither is *structural* or *prompt-induced*, so § 7 does not permit acceptance.

These are **NEW, DISTINCT findings**, not repeats — the same-finding counter stays at 1 for each.
Fix rounds after this verdict: **2 of 6**.
