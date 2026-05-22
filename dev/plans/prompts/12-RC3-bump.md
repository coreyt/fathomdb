# Phase 12-RC3 — Bump to 0.6.0-rc.3

rc.2 publish succeeded for cargo/pypi/npm artifacts but the
post-publish gates (co-tagging-assert + 3 smoke jobs +
github-release) failed because `assert-co-tagging.sh` and the
three `smoke-*.sh` scripts had a hardcoded `MAJOR.MINOR.PATCH`
regex that rejected the `-rc.N` suffix. Scripts fixed at
`0.6.0-rewrite` HEAD (commit `70e6487`).

Cutting rc.3 to re-trigger the full release workflow with the
fixed gates. No functional engine/SDK change — workflow-only
fixes since rc.2.

## State at spawn

- Branch: `0.6.0-rewrite`
- HEAD: `70e6487` (test_smoke_scripts assertion update for
  loosened regex + PEP 440 normalize)
- Recent commits since rc.2 tag:
  - `70e6487` fix(release): accept SemVer pre-release in
    post-publish gates (+ test assertions)
  - `2ea3122` chore: gitignore dev/memex/ scratch notes (+
    markdownlint ignore mirror)
  - `7507964` chore(release): bump to 0.6.0-rc.2 (the rc.2 tag
    points here)
- Registry state: all 7 axis-W crates + axis-E embedder-api
  published at BOTH `0.6.0-rc.1` (bootstrap) and `0.6.0-rc.2`
  (real). pypi has `fathomdb 0.6.0rc2`. npm has `fathomdb@0.6.0-rc.2`
  tagged `next`. **GitHub release for v0.6.0-rc.2 was NOT
  created** (gated by smoke+co-tagging; rc.3 supersedes it).

## Required reading

- `dev/plans/prompts/12-RC2-bump.md` — the rc.2 analogue you're
  repeating.
- `CHANGELOG.md` — see `## 0.6.0-rc.2` for shape.
- `dev/design/release.md` § RC1 bootstrap publish (background
  on why we're already 3 RCs deep).
- `scripts/set-version.sh` — bumps Axis W only.

## Tasks

1. **Bump Axis W via script.**
   `bash scripts/set-version.sh --workspace 0.6.0-rc.3`. Touches
   root `Cargo.toml`, python pyproject, ts package.json. Verify
   `git diff` shows only those files (plus 5
   workspace.dependencies pins).

2. **Bump Axis E manually.** Edit
   `src/rust/crates/fathomdb-embedder-api/Cargo.toml`:
   `version = "0.6.0-rc.2"` → `version = "0.6.0-rc.3"`. Also
   edit root `Cargo.toml` `[workspace.dependencies]` pin for
   `fathomdb-embedder-api` to `0.6.0-rc.3`. (Axis-E stays on
   Axis-W lockstep through GA per release.md § RC1 bootstrap
   publish.)

3. **CHANGELOG entry.** Add `## 0.6.0-rc.3` above the existing
   `## 0.6.0-rc.2` section. Body should explain:
   - Cut after rc.2's post-publish gates (co-tagging-assert + 3
     smoke jobs + github-release) failed on a regex that
     rejected pre-release semver.
   - rc.2 artifacts ARE live on crates.io / PyPI / npm but
     without a GitHub release entry.
   - rc.3 re-triggers the workflow with `70e6487`'s fix:
     `assert-co-tagging.sh` + the 3 smoke scripts accept
     `MAJOR.MINOR.PATCH(-PRERELEASE)?`; `smoke-pypi-wheel.sh`
     normalizes SemVer to PEP 440 (`0.6.0-rc.3` → `0.6.0rc3`)
     before `pip install`.
   - No functional engine/SDK change since rc.2.
     4-6 bullets max. Match existing style.

4. **Fixture update.** Add `0.6.0-rc.3` row to
   `dev/release/fixtures/co-tagging/fathomdb-embedder-api-ok.json`
   so `test_assert_co_tagging.sh` finds the live manifest
   version. (rc.1 + rc.2 rows already there; rc.3 joins.)

5. **Verify.**

   ```bash
   cargo check --workspace
   actionlint .github/workflows/release.yml
   bash scripts/tests/test_actionlint_fixture.sh
   bash scripts/tests/test_assert_co_tagging.sh
   bash scripts/tests/test_verify_release_gates.sh
   bash scripts/tests/test_set_version.sh
   bash scripts/tests/test_smoke_scripts.sh
   ```

   All must pass.

6. **Commit.** Single commit:

   ```text
   chore(release): bump to 0.6.0-rc.3

   - Axis W: 0.6.0-rc.2 -> 0.6.0-rc.3 (workspace + python + ts)
   - Axis E: 0.6.0-rc.2 -> 0.6.0-rc.3 (lockstep through GA)
   - CHANGELOG: 0.6.0-rc.3 section (re-triggers workflow with
     post-publish-gate semver fix from 70e6487)
   - co-tagging fixture: add 0.6.0-rc.3 row
   ```

## Hard constraints

- Do NOT push the tag (`v0.6.0-rc.3`) — orchestrator handles
  tag + push.
- Do NOT push to origin.
- Do NOT dispatch workflow.
- Do NOT publish to crates.io / pypi / npm.
- Do NOT spawn agents.

## Output

Write `dev/plans/runs/12-RC3-bump-output.json`:

```json
{
  "phase": "12-RC3-bump",
  "parent_commit": "70e6487",
  "head_sha": "<new HEAD>",
  "commits": ["<sha>: chore(release): bump to 0.6.0-rc.3"],
  "axis_w_version": "0.6.0-rc.2 -> 0.6.0-rc.3",
  "axis_e_version": "0.6.0-rc.2 -> 0.6.0-rc.3",
  "files_changed": [...],
  "commands_run": {
    "cargo_check_workspace": "...",
    "actionlint": "...",
    "test_actionlint_fixture": "...",
    "test_assert_co_tagging": "...",
    "test_verify_release_gates": "...",
    "test_set_version": "...",
    "test_smoke_scripts": "..."
  },
  "blockers_encountered": []
}
```

Stop after output.json written.
