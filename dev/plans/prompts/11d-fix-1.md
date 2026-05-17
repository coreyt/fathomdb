# Phase 11d-fix-1 — Reviewer remediation pass

Targeted fix for the three codex `gpt-5.4` findings on Phase 11d
(verdict `BLOCK`, see `dev/plans/runs/11d-review-20260517T152114Z.md`).

Operates in the **existing 11d worktree**
`/tmp/fdb-11d-release-workflow-20260517T145932Z` on branch
`phase-11d-release-workflow-20260517T145932Z`. Builds new commits on
top of `77cb7e2`.

## Model + effort

Opus 4.7, intent: medium. Spawn from main thread:

```bash
PHASE=11d-fix-1
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plans/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-11d-release-workflow-20260517T145932Z
PREAMBLE=$(cat <<'EOF'
YOU ARE THE IMPLEMENTER. Not the orchestrator. Do the work in this
worktree. Do NOT re-spawn yourself. Do NOT spawn other agents. Use
--disallowedTools Task Agent as a hard guard. Write code, run tests,
commit. Done.
EOF
)
( cd "$WT" && \
  ( echo "$PREAMBLE"; cat /home/coreyt/projects/fathomdb/dev/plans/prompts/11d-fix-1.md ) \
  | claude -p --model claude-opus-4-7 --effort medium \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --disallowedTools Task Agent \
      --permission-mode bypassPermissions \
      --output-format stream-json --include-partial-messages --verbose \
  > "$LOG" 2>&1 )
```

## Required reading

- `dev/plans/runs/11d-review-20260517T152114Z.md` — reviewer verdict.
- `.github/workflows/release.yml` — the artifact under remediation.
  Especially lines 1-50 (trigger + verify-release), 358-373
  (publish-pypi), 412-418 (publish-npm).
- `scripts/verify-release-gates.sh` — the gate script that finding 1
  bypasses on dispatch.
- `dev/design/release.md:80-90` — finding 3's design-text site.

## Scope — three findings, one commit per finding is fine

### Finding 1 (`high`) — `workflow_dispatch` must always run release gates

Current state (`.github/workflows/release.yml:43-49`):

```yaml
      - name: Run release gates
        if: github.event_name == 'push'
        run: bash scripts/verify-release-gates.sh
```

This gates the script on `push` events only, so any
`workflow_dispatch` (including `dry_run: false`) skips the gate
entirely. A dispatch with `dry_run: false` could then publish for
real without tag-format / HEAD-on-main / CHANGELOG / version-axis
checks.

**Required fix:** drop the `if: github.event_name == 'push'`
condition so the gate runs on both `push` and `workflow_dispatch`.
Then teach `scripts/verify-release-gates.sh` to handle the dispatch
case correctly:

- The tag-format check (`$GITHUB_REF` matches `refs/tags/v*` + version
  matches `Cargo.toml [workspace.package].version`) MUST still pass
  on real-tag push.
- On `workflow_dispatch`, `$GITHUB_REF` is `refs/heads/<branch>`,
  not a tag. The script should detect dispatch via
  `$GITHUB_EVENT_NAME == "workflow_dispatch"` and:
  - SKIP the tag-format check (no tag exists yet).
  - STILL run: set-version `--check-files`, HEAD-reachable-from-main,
    CHANGELOG section presence, crate publish-metadata completeness.
  - If `dry_run: false` on dispatch (an emergency-republish path),
    fail-loud with an explicit message: "dispatch with dry_run=false
    is an emergency-republish path; verify the dispatched ref matches
    the intended tag manually before approving."

Implementation hint: the script reads `$GITHUB_EVENT_NAME` and
`$DRY_RUN` (already exported as env in the workflow). Branch on
both, not just on `$GITHUB_REF` shape.

Tests in `scripts/tests/test_verify_release_gates.sh`: add cases:

- dispatch + dry_run=true → exit 0 with tag-check skipped.
- dispatch + dry_run=false → emit the explicit warning + still exit 0
  on otherwise-clean state (don't reject the path; just make it
  loud).
- push + tag mismatch → exit non-zero (existing case; ensure still
  green).

### Finding 2 (`medium`) — dry-run must skip irreversible PyPI publish

Current state (`.github/workflows/release.yml:358-373`):

```yaml
  publish-pypi:
    runs-on: ubuntu-latest
    needs: publish-rust-t4-engine
    environment:
      name: pypi
    steps:
      - uses: actions/download-artifact@... v8.0.1
        with:
          pattern: python-dist-*
          merge-multiple: true
          path: dist
      - name: Publish to PyPI (or test.pypi on dry-run)
        uses: pypa/gh-action-pypi-publish@... v1.14.0
        with:
          packages-dir: dist
          repository-url: ${{ env.DRY_RUN == 'true' && 'https://test.pypi.org/legacy/' || '' }}
```

TestPyPI publishes are still irreversible (the registry rejects
same-version re-uploads). Dry-run must not consume TestPyPI version
slots.

**Required fix:** add `if: ${{ inputs.dry_run != true }}` at the
JOB level on `publish-pypi`, mirroring the existing pattern on
`post-publish-smoke` (line 427). The build-python matrix still runs
on dry-run (legitimate test signal — verifies wheels build across
all platforms), but the publish step is fully skipped.

Verify `publish-npm` does NOT need the same treatment: it uses
`npm publish --dry-run` which is genuinely local (no network
publish, no version slot consumed). Confirm by reading the npm
publish step (lines 412-418); leave as-is if confirmed.

Update the workflow_dispatch input description on line 10 from:

```
"Rehearse against test.pypi + cargo --dry-run + npm --dry-run (no real publish)."
```

to:

```
"Rehearse cargo --dry-run + npm --dry-run; PyPI publish + post-publish smoke skipped entirely (no test.pypi slot burn)."
```

so the help text matches actual behavior.

### Finding 3 (`medium`) — `dev/design/release.md` npm-name mismatch

Current state (`dev/design/release.md:82-86`):

```
Per `feedback_release_verification`, "green CI + published wheel" is not
done. Release-evidence sweep installs the published wheel from PyPI and
runs an end-to-end open + close + exit smoke before the release is
declared signed. Equivalent npm smoke applies for `@fathomdb/...`
publishes. Crate publishes are smoked via `cargo install fathomdb-cli`
```

The 11d Blocker 7 resolution decided on bare `fathomdb` (single
brand across crates / wheel / npm). The design doc must follow.

**Required fix:** patch the paragraph. Replace `@fathomdb/...` with
`fathomdb`. Add a one-sentence note documenting the single-brand
decision: "The npm package is published as bare `fathomdb` (not a
`@fathomdb/` scope) per 11d Blocker 7 — single brand across crates,
wheel, and npm." Keep the rest of the paragraph identical.

No code changes required for this finding — it's pure docs.

## Required commands

```bash
cd /tmp/fdb-11d-release-workflow-20260517T145932Z
# Workflow still parses cleanly per actionlint.
actionlint .github/workflows/release.yml
# Verify-release-gates tests including the new dispatch cases.
bash scripts/tests/test_verify_release_gates.sh
# Existing 11c regression guards still green.
bash scripts/tests/test_set_version.sh
bash dev/release/tests/cargo_skew.sh
bash dev/release/tests/pip_skew.sh
# Existing 11d test suites still green.
bash scripts/tests/test_assert_co_tagging.sh
bash scripts/tests/test_smoke_scripts.sh
bash scripts/tests/test_actionlint_fixture.sh
# Canonical local gate.
bash scripts/agent-verify.sh
```

All must pass. Flake reruns (rerun once before declaring red):
`ac_029_canonical_writes_complete_under_projection_stall`,
`ac_017_vector_projection_freshness_p99_le_five_seconds`,
`t_safe_export_engine_error_exits_export_failure_66`.

## Discipline

- TDD: dispatch test cases for finding 1 land as failing tests
  before the script change makes them green.
- No scope creep. AC-054 release-finalize.sh is NOT in this slice.
- Comment policy unchanged: WHY only, not WHAT. No
  "fixed in 11d-fix-1" markers.

## Blockers — surface before writing code

If the dispatch-event-handling in `scripts/verify-release-gates.sh`
requires substrate not in the current environment (e.g. no way to
detect dispatch vs push from inside the script without a real GH
context), STOP and write a blocker report at
`dev/plans/runs/11d-fix-1-output.json` per the 10b-B blocker shape.
Most likely the script already reads `$GITHUB_EVENT_NAME` so this
won't be a blocker, but verify.

## Output

After all commands pass, write
`dev/plans/runs/11d-fix-1-output.json`:

```json
{
  "phase": "11d-fix-1",
  "baseline_sha": "77cb7e2",
  "branch": "phase-11d-release-workflow-20260517T145932Z",
  "head_sha": "<HEAD after final commit>",
  "commits": ["<sha>: <subject>", "..."],
  "findings_addressed": [
    "1 [high]: verify-release-gates.sh now runs on workflow_dispatch; tag-check skipped on dispatch; dry_run=false dispatch path emits explicit warning",
    "2 [medium]: publish-pypi job gated `if: inputs.dry_run != true`; dispatch description updated to match",
    "3 [medium]: dev/design/release.md npm name swapped from @fathomdb/... to bare fathomdb with single-brand decision note"
  ],
  "agent_verify_result": "pass | fail (+ tail)",
  "next_step_for_orchestrator": "promote to 0.6.0-rewrite; respawn codex reviewer for clean PASS"
}
```

Then stop. Do not advance to Phase 12. Do not run the reviewer
yourself.
