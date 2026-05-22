# Phase 12-RC2 — Bump to 0.6.0-rc.2

Bootstrap publish + workflow fixes consumed `0.6.0-rc.1` slot.
Cut `0.6.0-rc.2` for the real RC release. No functional code
change — just version bump + CHANGELOG entry.

## State at spawn

- Branch: `0.6.0-rewrite`
- HEAD: `d65f861` (npm --tag fix)
- Current workspace version: `0.6.0-rc.1`
- Registry state: all 7 axis-W crates + axis-E embedder-api
  published at `0.6.0-rc.1`.
- Dry-run dispatch `26045391681` green end-to-end (T1-T7 +
  publish-npm; pypi/smoke/co-tagging/release skipped by design).

## Required reading

- `dev/plans/prompts/12-RC1-tag-rc1.md` — original tag procedure
  (your work is the rc.2 analogue).
- `dev/design/release.md` § RC1 bootstrap publish — explains why
  rc.2 is the "real" RC.
- `CHANGELOG.md` — see existing `## 0.6.0-rc.1` section for shape.
- `scripts/set-version.sh` — bumps Axis W only. Axis E
  (`fathomdb-embedder-api`) needs separate manual edit.

## Tasks

1. **Bump Axis W via script.** Run
   `bash scripts/set-version.sh --workspace 0.6.0-rc.2`. This
   touches root `Cargo.toml [workspace.package].version`, the
   five `[workspace.dependencies]` axis-W pins,
   `src/python/pyproject.toml`, `src/ts/package.json`. Verify
   with `git diff` — expect only those files.

2. **Bump Axis E manually.** Edit
   `src/rust/crates/fathomdb-embedder-api/Cargo.toml`: change
   `version = "0.6.0-rc.1"` → `version = "0.6.0-rc.2"`. Also
   edit root `Cargo.toml` `[workspace.dependencies]` pin for
   `fathomdb-embedder-api`: change `version = "0.6.0-rc.1"` →
   `version = "0.6.0-rc.2"`. (Axis-E stays on Axis-W lockstep
   through 0.6.0 GA per `dev/design/release.md` § RC1 bootstrap
   publish.)

3. **CHANGELOG entry.** Add a new `## 0.6.0-rc.2` section above
   the existing `## 0.6.0-rc.1` section. Body should explain:
   - Cut as the real RC after the rc.1 slot was consumed by the
     bootstrap publish (see CHANGELOG rc.1 + release.md § RC1
     bootstrap publish).
   - Workflow fixes since rc.1: napi `win32-x64-msvc` label,
     publish-rust dry-run cascade restored via bootstrap, npm
     `--tag next` for prerelease publish. No functional
     engine/SDK code change.
   - Note that this is the first RC that will exercise the
     workflow's tag-trigger end-to-end (smoke + co-tagging +
     github-release jobs).
     Keep 4-6 bullets max. Match existing style.

4. **Cargo.lock.** Run `cargo update --workspace` only if needed
   to regenerate `Cargo.lock` workspace-member versions. If
   `cargo check --workspace` produces no diff in `Cargo.lock`,
   skip. (Some workflows leave Cargo.lock entries alone; check
   what happened during the rc.1 bump in git log.)

5. **Verify.**

   ```bash
   cargo check --workspace
   actionlint .github/workflows/release.yml
   bash scripts/tests/test_actionlint_fixture.sh
   bash scripts/tests/test_assert_co_tagging.sh
   bash scripts/tests/test_verify_release_gates.sh
   bash scripts/tests/test_set_version.sh
   ```

   All must pass. (test_set_version covers the sed pattern that
   handles the rc.1 literal — it may need an update for rc.2.)

6. **Fixture update.** Check if
   `dev/release/fixtures/co-tagging/fathomdb-embedder-api-ok.json`
   needs `0.6.0-rc.2` added to its versions[] list so
   `assert-co-tagging.sh` finds the live manifest version in the
   offline fixture. (Resume2 added rc.1; add rc.2 the same way.)

7. **Commit.** Single commit:

   ```text
   chore(release): bump to 0.6.0-rc.2

   - Axis W: 0.6.0-rc.1 -> 0.6.0-rc.2 (workspace + python + ts)
   - Axis E: 0.6.0-rc.1 -> 0.6.0-rc.2 (lockstep through GA)
   - CHANGELOG: 0.6.0-rc.2 section (real RC; rc.1 was bootstrap)
   - co-tagging fixture: add 0.6.0-rc.2 row
   ```

## Hard constraints

- Do NOT push the tag (`v0.6.0-rc.2`) — orchestrator handles
  tag + push.
- Do NOT push to origin.
- Do NOT dispatch workflow.
- Do NOT publish to crates.io / pypi / npm.
- Do NOT spawn agents.
- Stay on branch `0.6.0-rewrite` (no worktree this time —
  bump is small and the rc.1 procedure also worked on
  `0.6.0-rewrite` directly via the implementer-prep phase).

## Output

Write `dev/plans/runs/12-RC2-bump-output.json`:

```json
{
  "phase": "12-RC2-bump",
  "parent_commit": "d65f861",
  "head_sha": "<new HEAD>",
  "commits": ["<sha>: chore(release): bump to 0.6.0-rc.2"],
  "axis_w_version": "0.6.0-rc.1 -> 0.6.0-rc.2",
  "axis_e_version": "0.6.0-rc.1 -> 0.6.0-rc.2",
  "files_changed": [...],
  "commands_run": {
    "cargo_check_workspace": "pass | fail",
    "actionlint": "pass | fail",
    "test_actionlint_fixture": "...",
    "test_assert_co_tagging": "...",
    "test_verify_release_gates": "...",
    "test_set_version": "..."
  },
  "blockers_encountered": []
}
```

Stop after output.json written.
