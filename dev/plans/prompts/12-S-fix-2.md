# Phase 12-S-fix-2 — Rust nested-block-comment fix

Single targeted fix for the one codex `gpt-5.4` finding on Phase
12-S-fix-1 (verdict `BLOCK`, single medium finding). See
`dev/plans/runs/12-S-fix-1-review-20260517T201827Z.md`.

Operates in the existing 12-S worktree
`/tmp/fdb-12-S-security-fixtures-20260517T195735Z` on branch
`phase-12-S-security-fixtures-20260517T195735Z`. Builds new commits
on top of `27a818e`.

## Model + effort

Opus 4.7, intent: medium. Tiny scope.

```bash
PHASE=12-S-fix-2
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plans/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-12-S-security-fixtures-20260517T195735Z
PREAMBLE=$(cat <<'EOF'
YOU ARE THE IMPLEMENTER. Not the orchestrator. Do the work in this
worktree. Do NOT re-spawn yourself. Do NOT spawn other agents. Use
--disallowedTools Task Agent as a hard guard. Write code, run
tests, commit. Done.
EOF
)
( cd "$WT" && \
  ( echo "$PREAMBLE"; cat /home/coreyt/projects/fathomdb/dev/plans/prompts/12-S-fix-2.md ) \
  | claude -p --model claude-opus-4-7 --effort medium \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --disallowedTools Task Agent \
      --permission-mode bypassPermissions \
      --output-format stream-json --include-partial-messages --verbose \
  > "$LOG" 2>&1 )
```

## Required reading

- `dev/plans/runs/12-S-fix-1-review-20260517T201827Z.md` — verdict.
- `scripts/security/ast_scan.py` lines 133 + 148 — `scan_rust()`
  block-comment state machine.
- `scripts/tests/test_ast_scan.sh` line 70 — block-comment test
  invocation site.

## Scope — single finding, single commit

### Finding (medium) — nested block comments

Rust allows nested `/* /* */ */` block comments (edition 2018+).
Current state machine uses a boolean flag, so on the input
`/* outer /* inner */ still outer */ pub fn legacy_foo() {}`:

- The first `/*` sets `in_block_comment = true`.
- The first `*/` (closing inner) sets `in_block_comment = false`.
- `still outer */ pub fn legacy_foo() {}` is now treated as live
  code → false-positive flag on `legacy_foo`.

**Required fix:**

1. In `scan_rust()` (around line 133/148), replace
   `in_block_comment: bool` with `block_depth: int` (initial 0).
2. Each `/*` increments `block_depth` by 1.
3. Each `*/` decrements by 1.
4. A line is in-comment iff `block_depth > 0` AFTER processing
   any opens, but BEFORE processing any closes on the same line.
   Actually: the safe interpretation is **scan the line
   character-by-character**, alternating between in-comment and
   live-code regions, with depth incremented on `/*` and
   decremented on `*/`. Lines that contain only comment text
   (depth > 0 throughout) yield no scanned content. Lines with
   mixed comment + code (e.g. `*/ pub fn legacy_foo() {}` where
   depth goes 1→0 mid-line) yield only the post-comment portion
   for scanning.

   Simpler equivalent: pre-strip block comments from the line by
   walking left-to-right, tracking depth, dropping characters
   while depth > 0. Pass the resulting "code-only" line to the
   existing line-anchored regex.

5. Add a Rust fixture for nested block comments:
   `scripts/tests/fixtures/ast-shim/rust/clean/nested_block_comment_safe.rs`:

   ```rust
   // Nested-block-comment safety fixture for AC-050a.
   /* outer /* legacy_inner */ still outer — must not flag */
   pub fn safe_function() {}
   ```

   Expected: scanner does NOT flag (legacy_inner is inside a
   block comment, and "outer" + "still outer" are also inside
   the OUTER block comment).
6. Add a dirty Rust fixture for nested-block-then-real-code:
   `scripts/tests/fixtures/ast-shim/rust/block-comment-real/nested_real.rs`:

   ```rust
   /* /* inner */ outer */ pub fn legacy_admin() {}
   ```

   Expected: scanner DOES flag legacy_admin (outer block fully
   closes after the second `*/`; subsequent `pub fn legacy_admin`
   is live code).
7. Extend `scripts/tests/test_ast_scan.sh` to invoke both
   fixtures + assert expected behavior.

## Required commands

```bash
cd /tmp/fdb-12-S-security-fixtures-20260517T195735Z
# AST scan tests (including new nested-block fixtures).
bash scripts/tests/test_ast_scan.sh
# Direct scanner runs on clean tree (regression).
python3 scripts/security/ast_scan.py --language rust
# Strict agent-security still surfaces strace BLOCKER (expected on aarch64).
STRICT=1 bash scripts/agent-security.sh; echo "rc=$?"
# Canonical local gate (expected to fail on this host per fix-1 design;
# CI is the green path).
bash scripts/agent-verify.sh; echo "rc=$?"
```

Document agent-verify expected-fail status on local aarch64 (no
strace) as in fix-1 output.

## Discipline

- TDD: nested-fixture clean case + nested-fixture dirty case
  land RED (scanner false-positive / false-negative) before the
  depth-counter fix makes them green.
- Net LoC: depth counter is ~5-10 LoC; fixtures are 3-4 lines
  each; test invocations are 4-6 lines. Total ~25-40 LoC.
- Comment policy: WHY only. Brief WHY in scan_rust():
  "Rust permits nested block comments (edition 2018+); track
  depth not boolean."

## Output

After all commands pass, write
`dev/plans/runs/12-S-fix-2-output.json`:

```json
{
  "phase": "12-S-fix-2",
  "baseline_sha": "27a818e",
  "branch": "phase-12-S-security-fixtures-20260517T195735Z",
  "head_sha": "<HEAD after final commit>",
  "commits": ["<sha>: <subject>"],
  "findings_addressed": [
    "1 [medium]: scan_rust() block-comment uses depth counter; nested /* /* */ */ correctly tracked; clean + dirty nested fixtures added"
  ],
  "agent_verify_local_aarch64_status": "fail (strace absent; expected)",
  "test_ast_scan_result": "pass (including 2 new nested-block cases)",
  "next_step_for_orchestrator": "promote to 0.6.0-rewrite; respawn codex reviewer for PASS"
}
```

Then stop.
