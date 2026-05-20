# Phase 12-GA — Cut 0.6.0 GA

rc.4 publish completed cleanly end-to-end on workflow run
`26105261444`: 11 builds, 7 cargo tiers, pypi + npm, three smoke
jobs, co-tagging-assert, and github-release — all green. WF-FIX-3
verified against real-registry installs. Preconditions for GA cut
per `dev/plans/0.6.0-implementation.md` § Phase 12 are met.

Cutting `0.6.0` GA. Single implementer commit. Same mechanical
shape as 12-RC4-bump.md, with three additions:

1. CHANGELOG collapsed into a single `## 0.6.0` block (intent
   decided in this session: rc.1..rc.4 fold into one stable
   entry).
2. Axis-E version held at `0.6.0` to match axis-W at the fork
   point (decision logged this session as `d-001`: mechanics
   support independent semver, but pin them together at the GA
   anchor so 0.6.0 is the historical reference; future axis-W
   bumps no longer auto-bump axis-E — axis-E moves only on trait
   surface change).
3. `github-release` workflow step gets an explicit `prerelease`
   field (rc.4 was marked `isPrerelease: false` because
   softprops/action-gh-release v2.6.1 did not auto-detect the
   prerelease suffix; this would silently mark a future rc as
   stable).

## State at spawn

- Branch: `0.6.0-rewrite`
- HEAD: `f985538` (chore(release): bump to 0.6.0-rc.4)
- Tag `v0.6.0-rc.4` pushed to origin; run `26105261444` GA-gating
  workflow green end-to-end including `github-release`.
- Registry state at spawn:
  - crates.io: 7 axis-W crates + axis-E `fathomdb-embedder-api`
    at `0.6.0-rc.1` (bootstrap), `0.6.0-rc.2`, `0.6.0-rc.3`,
    `0.6.0-rc.4`.
  - PyPI: `fathomdb 0.6.0rc2`, `0.6.0rc3`, `0.6.0rc4`.
  - npm: `fathomdb@0.6.0-rc.{2,3,4}` tagged `next`.
  - GitHub releases: `v0.6.0-rc.4` exists with python wheel assets.
    (rc.2 + rc.3 never created GH release entries — both failed
    the gates that rc.4 now passes.)

## Required reading

- `dev/plans/prompts/12-RC4-bump.md` — most-recent mechanical
  bump; same shape minus the additions above.
- `CHANGELOG.md` — current state has `## 0.6.0-rc.4` through
  `## 0.6.0-rc.1`. The 4 rc sections collapse into one
  `## 0.6.0` block.
- `dev/design/release.md` § Version axes — axis-W vs axis-E
  semver story.
- `dev/design/release.md` § RC1 bootstrap publish — axis-E
  independence resumes at/after GA cut; this release pins
  axis-E to 0.6.0 to match axis-W (mechanics permit decoupling
  later).
- `.github/workflows/release.yml:500-514` — `github-release` job
  uses softprops/action-gh-release v2.6.1.

## Tasks

1. **Bump Axis W via script.**
   `bash scripts/set-version.sh --workspace 0.6.0`. Touches
   root `Cargo.toml`, `src/python/pyproject.toml`,
   `src/ts/package.json`, plus 5 `[workspace.dependencies]`
   pins. Verify `git diff` shows only those files (Cargo.lock
   will also update — that's expected).

2. **Bump Axis E manually.** Edit
   `src/rust/crates/fathomdb-embedder-api/Cargo.toml`:
   `version = "0.6.0-rc.4"` → `version = "0.6.0"`. Also edit
   root `Cargo.toml` `[workspace.dependencies]` pin for
   `fathomdb-embedder-api` to `0.6.0`.

3. **CHANGELOG consolidation.** Replace the four
   `## 0.6.0-rc.{1,2,3,4}` sections with a single `## 0.6.0 -
2026-05-19` block. Preserve all substantive content; drop
   the rc-narrative meta-text (e.g. "Cut after rc.3's
   post-publish gates failed…"). Structure:

   ```markdown
   ## 0.6.0 - 2026-05-19

   First stable release of FathomDB 0.6.0 — local-first
   retrieval engine on SQLite (FTS5 + sqlite-vec) with Rust,
   Python, and TypeScript SDKs.

   ### Added
   <merge content from rc.1 Added>

   ### Changed
   <single combined bulleted list summarizing the release-
   workflow refinements that landed across rc.2–rc.4:
   - napi `win32-x64-msvc` target label
   - cargo publish dry-run cascade fixed via rc.1 bootstrap
   - npm `--tag next` for prerelease publishes
   - post-publish gates accept SemVer pre-release; PEP 440
     normalization for PyPI
   - `assert-co-tagging.sh` sends `User-Agent` header
   - PyPI + npm smoke scripts write a minimal valid record
   - `src/ts/tsconfig.build.json` emits `dist/index.js`
     correctly>

   ### Deferred
   <copy from rc.1 verbatim>

   ### Removed
   <copy from rc.1 verbatim — "(none — 0.6.0 is a rewrite…)">
   ```

   Aim for ~40-70 lines total in the new `## 0.6.0` block.
   Keep tone and bullet density consistent with rc.1's Added
   list. Drop the per-rc framing (no "rc.N supersedes…" lines;
   the GA section is the canonical 0.6.0 narrative).

4. **Fixture state.** All three co-tagging fixtures already
   contain a `0.6.0` row (visible in
   `dev/release/fixtures/co-tagging/{fathomdb,fathomdb-embedder,fathomdb-embedder-api}-ok.json`).
   No fixture edits needed for GA.

5. **`github-release` workflow: explicit `prerelease`.** Edit
   `.github/workflows/release.yml` around lines 510-514. Under
   the `softprops/action-gh-release@…` step, add:

   ```yaml
       prerelease: ${{ contains(github.ref_name, '-') }}
   ```

   Evaluates `true` for tags like `v0.6.0-rc.5` (or future
   `-beta`, `-alpha`); `false` for stable tags like `v0.6.0`.
   Run `actionlint .github/workflows/release.yml` after; must
   stay clean.

6. **Verify.**

   ```bash
   cargo check --workspace
   actionlint .github/workflows/release.yml
   bash scripts/tests/test_actionlint_fixture.sh
   bash scripts/tests/test_assert_co_tagging.sh
   bash scripts/tests/test_verify_release_gates.sh
   bash scripts/tests/test_set_version.sh
   bash scripts/tests/test_smoke_scripts.sh
   bash scripts/agent-verify.sh
   ```

   All must pass. Known parallel-race flakes (rerun with
   `--test-threads=1` if hit on first run, then declare green):
   - `t_028a_excise_source_cli_returns_excise_report`
   - `t_042_trace_cli_enumerates_canonical_rows_for_source`
   - `t_058_recover_truncate_wal_with_accept_data_loss_succeeds`
   - `t_040a_dump_row_counts_cli_emits_counts_array`
   - `t_040a_verify_embedder_cli_emits_match_status_on_matching_input`
   - `t_safe_export_engine_error_exits_export_failure_66`
   - `ac_017_vector_projection_freshness_p99_le_five_seconds`
   - `ac_029_canonical_writes_complete_under_projection_stall`

7. **Commit.** Single commit:

   ```text
   chore(release): bump to 0.6.0 GA

   - Axis W: 0.6.0-rc.4 -> 0.6.0 (workspace + python + ts)
   - Axis E: 0.6.0-rc.4 -> 0.6.0 (decision d-001: aligned at GA
     fork point; future axis-W bumps no longer auto-bump axis-E)
   - CHANGELOG: collapse 4 rc sections into single ## 0.6.0 block
   - release.yml: explicit `prerelease:` on github-release step
     so future rcs are marked prerelease correctly (rc.4 was
     mislabeled as stable by softprops auto-detect default)
   ```

## Hard constraints

- Do NOT push the tag (`v0.6.0`) — orchestrator hands tag push to
  operator via `!`.
- Do NOT push to origin.
- Do NOT dispatch workflow.
- Do NOT publish to crates.io / pypi / npm.
- Do NOT spawn agents.
- Do NOT modify engine, SDK, or any published artifact code.
  GA is a version bump + workflow tweak + CHANGELOG
  consolidation; nothing else.
- Do NOT bump axis-E beyond `0.6.0` (matches axis-W per d-001).

## Output

Write `dev/plans/runs/12-GA-bump-output.json`:

```json
{
  "phase": "12-GA-bump",
  "parent_commit": "f985538",
  "head_sha": "<new HEAD>",
  "commits": ["<sha>: chore(release): bump to 0.6.0 GA"],
  "axis_w_version": "0.6.0-rc.4 -> 0.6.0",
  "axis_e_version": "0.6.0-rc.4 -> 0.6.0 (per d-001)",
  "files_changed": [...],
  "changelog_summary": "collapsed rc.1..rc.4 into single ## 0.6.0 - 2026-05-19 block (~<N> lines)",
  "workflow_changes": "github-release step gets `prerelease: ${{ contains(github.ref_name, '-') }}`",
  "commands_run": {
    "cargo_check_workspace": "...",
    "actionlint": "...",
    "test_actionlint_fixture": "...",
    "test_assert_co_tagging": "...",
    "test_verify_release_gates": "...",
    "test_set_version": "...",
    "test_smoke_scripts": "...",
    "agent_verify": "..."
  },
  "blockers_encountered": [],
  "next_step_for_orchestrator": "code-reviewer pass; cherry-pick onto 0.6.0-rewrite; operator pushes branch + v0.6.0 tag; watch workflow; verify isPrerelease=false on GA + all 4 post-publish jobs green"
}
```

Stop after output.json written.
