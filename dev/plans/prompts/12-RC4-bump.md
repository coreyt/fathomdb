# Phase 12-RC4 — Bump to 0.6.0-rc.4

rc.3 publish succeeded for cargo/pypi/npm artifacts but the
post-publish gates (co-tagging-assert + pypi-smoke + npm-smoke)
failed because three real defects existed in the verification
scripts (rc.3 artifacts on the registries are CORRECT — only the
verification was broken). Defects fixed at `0.6.0-rewrite` HEAD
(WF-FIX-3 cherry-picked as `26bb7da`, doc-only fixup +
verdict promotion in `368d17d`).

Cutting rc.4 to re-trigger the full release workflow with the
fixed gates. No functional engine/SDK change — workflow-only
fixes since rc.3.

## State at spawn

- Branch: `0.6.0-rewrite`
- HEAD: `368d17d` (docs(release): refresh smoke header comments +
  WF-FIX-3 verdict)
- Recent commits since rc.3 tag:
  - `368d17d` docs(release): refresh smoke header comments +
    WF-FIX-3 verdict (smoke header comment refresh; codex-equivalent
    code-reviewer verdict promotion for WF-FIX-3; carry-forward of
    the WF-FIX-3 prompt)
  - `26bb7da` fix(release): post-publish gate defects exposed by rc.3
    (assert-co-tagging User-Agent; smoke scripts write minimal valid
    record instead of empty batch; tsconfig.build.json so
    `dist/index.js` matches `package.json "main"`; release.yml uses
    the new tsconfig)
  - `06dc42e` chore(release): bump to 0.6.0-rc.3 (the rc.3 tag points
    here)
- Registry state: all 7 axis-W crates + axis-E embedder-api published
  at `0.6.0-rc.1` (bootstrap), `0.6.0-rc.2`, AND `0.6.0-rc.3`. pypi
  has `fathomdb` at `0.6.0rc2` + `0.6.0rc3`. npm has `fathomdb` at
  `0.6.0-rc.2` + `0.6.0-rc.3` tagged `next`. **GitHub release for
  v0.6.0-rc.2 and v0.6.0-rc.3 were NOT created** (gated by
  smoke+co-tagging; rc.4 supersedes them all on the GitHub-Release
  axis).

## Required reading

- `dev/plans/prompts/12-RC3-bump.md` — the rc.3 analogue you're
  repeating.
- `dev/plans/prompts/12-WF-FIX-3.md` — the gate-fix slice that this
  RC re-triggers.
- `dev/plans/runs/12-WF-FIX-3-review-20260519T002806Z.md` — codex-
  equivalent reviewer verdict on the WF-FIX-3 commit.
- `CHANGELOG.md` — see `## 0.6.0-rc.3` for shape.
- `dev/design/release.md` § RC1 bootstrap publish (background on why
  we're already 4 RCs deep).
- `scripts/set-version.sh` — bumps Axis W only.

## Tasks

1. **Bump Axis W via script.**
   `bash scripts/set-version.sh --workspace 0.6.0-rc.4`. Touches
   root `Cargo.toml`, python pyproject, ts package.json. Verify
   `git diff` shows only those files (plus 5
   workspace.dependencies pins).

2. **Bump Axis E manually.** Edit
   `src/rust/crates/fathomdb-embedder-api/Cargo.toml`:
   `version = "0.6.0-rc.3"` → `version = "0.6.0-rc.4"`. Also
   edit root `Cargo.toml` `[workspace.dependencies]` pin for
   `fathomdb-embedder-api` to `0.6.0-rc.4`. (Axis-E stays on
   Axis-W lockstep through GA per release.md § RC1 bootstrap
   publish.)

3. **CHANGELOG entry.** Add `## 0.6.0-rc.4` above the existing
   `## 0.6.0-rc.3` section. Body should explain:
   - Cut after rc.3's post-publish gates failed on three real
     verification defects (NOT on the rc.3 artifacts themselves,
     which are correct on crates.io / PyPI / npm).
   - Three gate fixes from `26bb7da` re-triggered by rc.4:
     `assert-co-tagging.sh` now sends a `User-Agent` header
     (crates.io API returns HTTP 403 without one); PyPI + npm
     smoke scripts write a minimal valid record
     (`{"kind":"doc","body":"{}"}`) instead of an empty batch
     that the engine rejects per the 5-verb invariant;
     `src/ts/tsconfig.build.json` (new) emits `dist/index.js` at
     the path `package.json "main"` points to (previous layout
     emitted `dist/src/index.js` so the published npm tarball
     was broken at `import { Engine } from "fathomdb"`).
   - rc.3 artifacts ARE live on crates.io / PyPI / npm but without
     a GitHub release entry.
   - No functional engine/SDK change since rc.3.
     4-6 bullets max. Match existing style.

4. **Fixture update.** Add `0.6.0-rc.4` row to
   `dev/release/fixtures/co-tagging/fathomdb-embedder-api-ok.json`
   so `test_assert_co_tagging.sh` finds the live manifest
   version. (rc.1 + rc.2 + rc.3 rows already there; rc.4 joins.)

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
   chore(release): bump to 0.6.0-rc.4

   - Axis W: 0.6.0-rc.3 -> 0.6.0-rc.4 (workspace + python + ts)
   - Axis E: 0.6.0-rc.3 -> 0.6.0-rc.4 (lockstep through GA)
   - CHANGELOG: 0.6.0-rc.4 section (re-triggers workflow with
     post-publish-gate fixes from 26bb7da: assert-co-tagging UA,
     smoke scripts write valid record, tsconfig.build.json for
     dist/index.js layout)
   - co-tagging fixture: add 0.6.0-rc.4 row
   ```

## Hard constraints

- Do NOT push the tag (`v0.6.0-rc.4`) — orchestrator handles
  tag + push (and orchestrator hands tag push to operator via `!`).
- Do NOT push to origin.
- Do NOT dispatch workflow.
- Do NOT publish to crates.io / pypi / npm.
- Do NOT spawn agents.

## Output

Write `dev/plans/runs/12-RC4-bump-output.json`:

```json
{
  "phase": "12-RC4-bump",
  "parent_commit": "368d17d",
  "head_sha": "<new HEAD>",
  "commits": ["<sha>: chore(release): bump to 0.6.0-rc.4"],
  "axis_w_version": "0.6.0-rc.3 -> 0.6.0-rc.4",
  "axis_e_version": "0.6.0-rc.3 -> 0.6.0-rc.4",
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
