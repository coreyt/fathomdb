# Phase 10b-A-fix-1 — Pin remaining verify-embedder JSON keys in CLI test

Tiny remediation pass for reviewer finding #1 from
`dev/plans/runs/10b-A-review-20260516T000718Z.md`.

The `verify-embedder` JSON top-level contract per `dev/design/recovery.md:89`
is `{verb, stored_identity, stored_dimension, supplied_identity,
supplied_dimension, status}`. The current CLI test at
`src/rust/crates/fathomdb-cli/tests/operator_cli.rs:386`
(`t_040a_verify_embedder_cli_emits_match_status_on_matching_input`)
asserts only four of the six. Add assertions for the missing keys
(`supplied_identity`, `supplied_dimension`) so the public JSON contract
is fully pinned.

## Model + effort

Opus 4.7, intent: medium.

```bash
PHASE=10b-A-fix-1
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plans/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-${PHASE}-${TS}
git -C /home/coreyt/projects/fathomdb worktree add "$WT" -b "phase-${PHASE}-${TS}" 273a5fb
PREAMBLE=$(cat <<'EOF'
YOU ARE THE IMPLEMENTER. Not the orchestrator. Tiny remediation pass.
Do the work in this worktree. Do NOT re-spawn yourself. Do NOT spawn
other agents. Do NOT broaden scope.
EOF
)
( cd "$WT" && \
  ( echo "$PREAMBLE"; cat /home/coreyt/projects/fathomdb/dev/plans/prompts/10b-A-fix-1.md ) \
  | claude -p --model claude-opus-4-7 --effort medium \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --disallowedTools Task Agent \
      --permission-mode bypassPermissions \
      --output-format stream-json --include-partial-messages --verbose \
  > "$LOG" 2>&1 )
```

## Scope

ONLY: extend `t_040a_verify_embedder_cli_emits_match_status_on_matching_input`
in `src/rust/crates/fathomdb-cli/tests/operator_cli.rs` to assert:

- top-level key `supplied_identity` equals the value passed via `--identity`
- top-level key `supplied_dimension` equals the value passed via `--dimension`

The existing assertions for `verb`, `stored_identity`, `stored_dimension`,
`status` remain. Add a second mismatch test only if the existing test
suite does not already cover the mismatched-input case (likely the
identity-mismatch / dimension-mismatch engine unit tests cover it; check
first — do NOT duplicate).

Out of scope:

- Anything in `fathomdb-engine`.
- Anything else in the CLI parser, dispatcher, or other tests.
- Refactoring or comment additions.
- New report-type fields. Engine `VerifyEmbedderReport` already carries
  these (CLI serializer at `src/rust/crates/fathomdb-cli/src/lib.rs:624`
  range emits them).

## TDD

Red-green: first run the existing test to confirm green, then add the
new assertions, run again (red on the new lines if the JSON doesn't
actually emit them), then green. Capture the failing run in the
implementer log explicitly — `cargo test -p fathomdb-cli operator_cli::t_040a_verify_embedder_cli_emits_match_status` should fail
between adding assertions and verifying the JSON shape.

If the CLI already emits the keys (it should — the serializer was
already implemented in 10b-A), the test will go green immediately. In
that case capture a deliberate breaking variant (assert the wrong
value), run red, revert to correct assertion, run green. The point is
preserving the red-green evidence requested by reviewer finding #2 for
this specific gap.

## Commands

```bash
cd "$WT"
cargo test -p fathomdb-cli operator_cli::t_040a_verify_embedder
./scripts/agent-verify.sh
```

## Output

After agent-verify passes, write
`dev/plans/runs/10b-A-fix-1-output.json`:

```json
{
  "phase": "10b-A-fix-1",
  "baseline_sha": "273a5fb",
  "branch": "phase-10b-A-fix-1-<ts>",
  "head_sha": "<HEAD>",
  "tests_modified": ["fathomdb-cli::operator_cli::t_040a_verify_embedder_cli_emits_match_status_on_matching_input"],
  "agent_verify_result": "pass | fail (+ tail)",
  "next_step_for_orchestrator": "merge worktree branches into 0.6.0-rewrite"
}
```

Then stop.
