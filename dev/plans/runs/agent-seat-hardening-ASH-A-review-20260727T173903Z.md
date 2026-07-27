# ASH-A Review — codex gpt-5.4 (§ 9 gate)

- **Effort:** agent-seat hardening (cross-cutting; NO pico label, NO `R-20-xx` requirement id).
- **Slice:** ASH-A — Steps 1 / 2 / 5 / 6 (docs + governance only).
- **Branch:** `agent-seat-hardening` · **HEAD reviewed:** `725e7f08` · **baseline:** `9861ad7d` (== `main` tip).
- **Reviewer:** `codex exec --model gpt-5.4 -c model_reasoning_effort=high --sandbox read-only`,
  invoked through `dev/agent-tools/codex-nostdin.sh` (bare `codex exec` deadlocks on stdin).
- **Round:** 1 of the § 6 cap (bound for this effort: 3 same-finding / 6 total — no
  `fathomdb-engine/src` touched, so the standard bound applies).
- **Terminal transcript (TC-RUBRIC-7):**
  `dev/plans/runs/codex/agent-seat-hardening/ASH-A-20260727T173903Z.log`

## Verdict: PASS

No review findings.

## What passed on inspection

- The new § 1.1 "main thread" definition forecloses the steward-ledger `seq-121` misreading:
  it makes the term **role-indexed, not globally unique**, and states the `tools:` frontmatter
  limitation as **spawned-subagent-only**, not a main-thread guard.
- The chronology (`72af7045` 2026-05-17 vs `31a73401` 2026-07-02, 46 days) is recorded at
  **both** required sites (§ 1 anti-pattern bullet and § 10 rule 1), and the anti-patterns were
  **scoped, not deleted** — the nesting prohibition survives intact.
- § 1.2 reads as the single authoritative write-path boundary; the two seat files **defer** to it
  rather than re-copying the list. Reviewer explicitly **accepted** the literal `engine/**` wording
  as written: conceptually redundant (the engine crate lives at `src/rust/crates/fathomdb-engine`,
  already covered by `src/**`) but not a contradictory rule.
- The § 6 `SendMessage` scoping **preserves** the original reasoning: fresh-spawn remains the rule
  for every closed-boundary hand-off; `SendMessage` is confined to correcting or halting a
  still-running subagent; and the OPEN/UNRULED round-cap caveat is framed as **non-capability**,
  not permission.
- **Extra in-scope change ACCEPTED, not reverted:** `AGENTS.md:88`. Reviewer's reasoning — without
  it `AGENTS.md` would keep stating the old absolute form while `orchestration.md` now scopes the
  same rule, leaving the governing docs internally inconsistent on the exact point this slice
  exists to settle.
- All seven hard constraints verified individually: no `tools:` frontmatter line changed;
  `.claude/agents/implementer.md` untouched; `.claude/settings.json` untouched;
  `dev/todos-and-considerations-ledger.jsonl` (+ `.seq`) untouched; 0.8.20 plan/board/release-state
  and `src/conformance/governed-surface-allowlist.json` untouched; no source or test file modified;
  anti-patterns scoped rather than deleted.
- No new internal contradiction introduced in the governing docs.

## Reviewer process notes

Direct `git show` / `git diff` invocations were intermittently rejected by the codex sandbox
wrapper; the reviewer substituted stable filesystem reads plus the worktree reflog for chronology
and baseline comparison, and recorded that this did not change the verdict.

One factual clarification the reviewer raised and the orchestrator confirms: the closure artifact's
`head_sha` is `904f2a2e` (the content commit) while the branch HEAD given for review was `725e7f08`
(the following docs commit that adds the closure artifact itself). This is the § 8 self-reference
structure — a closure JSON cannot contain the SHA of the commit that adds it — and is the exact
case § 7 names as *structural*, not a defect.

## Orchestrator triage

PASS with zero findings; nothing to override, nothing to remediate. Fix-N round count for ASH-A: **0**.
