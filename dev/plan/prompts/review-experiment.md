# Reviewer template — per-experiment diff review (codex)

## Model + effort

`codex exec --model gpt-5.4 -c model_reasoning_effort=high`. Stdin
closed (`< /dev/null`) unless deliberately piping content.

Codex is **read-only** for reviewer use. It must not edit files.

## Log destination

`dev/plan/runs/<phase>-review-<utc-ts>.md`. The orchestrator commits
this file alongside the keep/revert decision so the audit trail
includes the review.

## Required reading + discipline (reviewer)

- **Read `AGENTS.md`** — canonical agent operating manual. Use it
  to judge whether the diff respects §1 invariants (TDD, ADRs,
  Public surface = contract, Stale > missing) and §5 test
  discipline. A diff that adds production code without a failing
  test first is a finding even if numbers look good.
- **Read `MEMORY.md`** and the `feedback_*.md` files; flag any
  diff that violates a rule encoded there.
- **Reviewer is read-only**. No file edits except the verdict file.
- **Do not run code**. Reviewer reads diffs and logs only.

## Context

- Plan: `dev/plan/0.6.0-Phase-9-Pack-5-performance-diagnostics.md` §0.1
  (reviewer mandate).
- Plan: §1 (acceptance for the packet).
- Per-phase prompt under `dev/plan/prompts/<phase>.md` —
  acceptance + decision rule live there.
- Implementer's run log:
  `dev/plan/runs/<phase>-<ts>.log`.
- Implementer's structured output:
  `dev/plan/runs/<phase>-output.json`.
- Diff under review: latest commit on the worktree branch
  `pack5-<phase>-<ts>`.

## Mandate

Read the diff and the implementer log. Issue a structured verdict.

### What to check

1. **Diff matches mandate**: every change traces to a mandate item in
   the per-phase prompt. Flag scope creep
   (extra files touched, refactors not requested, comments not
   asked for).
2. **Acceptance criteria from the per-phase prompt are met**: cite
   numbers from the implementer's output JSON. If the decision rule
   is numeric (e.g. concurrent ms drops by ≥ 30%), do the math
   yourself; do not trust the implementer's `decision_rule_met` flag
   without verification.
3. **AGENTS.md compliance**:
   - §1 invariants (TDD mandatory, ADRs authoritative, Public
     surface = contract).
   - §3 (was `agent-verify.sh` run? evidence in implementer log?).
   - §5 (failing test first; no test edits during fix-to-spec; no
     agent-generated oracles).
4. **TDD evidence**: a red-green-refactor cycle should be visible
   either in commit history or in the implementer log. Mechanical
   passes (pure rename, version bump) are exempt; behavior changes
   are not.
5. **`feedback_*` rules** (read MEMORY.md):
   - `feedback_cross_platform_rust.md` — c_char / c_int rule for any FFI.
   - `feedback_reliability_principles.md` — net-negative LoC bias.
   - `feedback_release_verification.md` — CI green is not done.
   - `feedback_tdd.md` — failing test first.
6. **Plan §5 cross-check**: no retry of any reverted experiment from
   `dev/notes/performance-whitepaper-notes.md` §5 without explicit
   override rationale in §12.

### Output shape (Markdown)

```markdown
# Review verdict — <phase> — <ts>

Verdict: PASS | CONCERN | BLOCK

## Summary
<2-4 sentence summary>

## Findings
1. <file:line> — <what> — <severity: nit | concern | block>
2. ...

## Acceptance criteria check
- <criterion 1>: PASS | FAIL — <evidence file:line or numbers>
- ...

## Decision-rule arithmetic
<show the math: numbers used, threshold, computed delta, met?>

## §5 retry cross-check
PASS | OVERRIDE-with-rationale: <text>

## Phase 7/8 invariants
PASS | CONCERN | BLOCK — <details>

## Recommended next step
<KEEP | REVERT | RUN_MORE_VERIFICATION | ESCALATE>
```

`PASS` = clean keep. `CONCERN` = orchestrator may keep but must
record the concern in §12. `BLOCK` = orchestrator must revert unless
they explicitly override with written rationale (per plan §0.1).

## Acceptance criteria (for the reviewer pass itself)

- Verdict is one of PASS / CONCERN / BLOCK; nothing else.
- Every finding cites `file:line`.
- Decision-rule arithmetic is shown.
- §5 retry cross-check is explicit.
- Phase 7/8 invariant cross-check (next file) is folded in OR
  delegated to a separate `review-phase78-robustness.md` invocation.

## Files allowed to touch

- `dev/plan/runs/<phase>-review-<utc-ts>.md` (the verdict file).

## Files NOT to touch

- Everything else. Reviewer is read-only.

## Verification commands

```bash
test -f dev/plan/runs/<phase>-review-<ts>.md
grep -E "^Verdict: (PASS|CONCERN|BLOCK)$" dev/plan/runs/<phase>-review-<ts>.md
```

## Required output to orchestrator

The Markdown file itself is the output. Orchestrator reads:

- `Verdict:` line.
- `Recommended next step:` line.
- `Findings` for any block-severity items.

## Required output to downstream agents

- None — reviewer feeds the orchestrator's keep/revert decision only.

## Update log

_(append the implementer's commit SHA, log path, and decision rule
before invoking codex)_
