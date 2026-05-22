# Hand-off — 0.6.0 release in flight (Phase 12, post-rc.3)

Picking up an in-progress 0.6.0 release. Three RCs published to
crates.io / PyPI / npm. Post-publish gate jobs have three real
defects blocking the GA path. Your job: fix the gates (WF-FIX-3),
cut rc.4 to validate the fixes end-to-end, then proceed to GA per
the existing Phase 12 plan.

## Orientation

You are the **orchestrator**. Main thread. Always. Per
`feedback_orchestrator_thread.md` + `dev/design/orchestration.md`
§ 1.

- **Do NOT spawn a separate orchestrator subagent.** Plan + verify
  here.
- **Delegate code changes to implementers** per
  `feedback_orchestrate_releases.md`. Spawn via the bash invocation
  in `dev/design/orchestration.md` § 2 (NOT the Agent tool — it
  lacks the `--disallowedTools Task Agent` guard that prevents
  runaway spawn loops).
- **Delegate diff review to `code-reviewer` subagent** before
  promotion. Independent read. Spawn via the Agent tool with
  `subagent_type=code-reviewer`.
- Cherry-pick implementer commits onto `0.6.0-rewrite` only after
  reviewer PASS (or explicit orchestrator override). Order matters:
  cherry-pick BEFORE reviewer where mainline state is needed for
  the review.

## Required reading (load before any action)

- **`MEMORY.md`** — load every entry. Especially:
  - `feedback_orchestrate_releases.md` —
    implementer-in-worktree + reviewer pattern is mandatory for
    release work.
  - `feedback_orchestrator_thread.md` — main thread = orchestrator.
  - `feedback_release_verification.md` — green CI + published
    artifact is NOT done; install-from-registry smoke is the gate.
  - `feedback_workflow_validation.md` — actionlint, not
    `yaml.safe_load`.
  - `feedback_reliability_principles.md` — no scope creep, no
    soak, delete-before-add.
  - `feedback_file_deletion.md` — never `find -delete`.
  - `feedback_tdd.md` — red-green-refactor for behavior changes;
    mechanical version bumps + workflow fixes use existing suite
    as the gate.
- `dev/design/orchestration.md` § 2 (implementer spawn), § 3
  (PREAMBLE), § 4 (cherry-pick), § 7 (fix-N loop).
- `dev/design/release.md` — two-axis versioning + 8-tier publish
  order + § RC1 bootstrap publish (explains why we're at rc.3+).
- `dev/plans/0.6.0-implementation.md` § Phase 12 — release-gates
  section. Read for GA preconditions.
- `dev/progress/0.6.0.md` — session log; most-recent entry on top.
- `CHANGELOG.md` `## 0.6.0-rc.1` through `## 0.6.0-rc.3` — full
  RC narrative.
- Three prior phase prompts: `dev/plans/prompts/12-RC1-WF-FIX-1*`,
  `12-RC2-bump.md`, `12-RC3-bump.md`. Read for pattern + scope
  precedent.

## Current state (snapshot 2026-05-18)

### Git

- Branch: `0.6.0-rewrite`
- HEAD: `06dc42e` (chore(release): bump to 0.6.0-rc.3)
- Tags pushed: `v0.6.0-rc.2`, `v0.6.0-rc.3`. **`v0.6.0-rc.1` was
  NOT git-tagged** — the rc.1 publish was an operator-run
  bootstrap script (`scripts/release/publish-rc1-bootstrap.sh`),
  not a tag-triggered workflow.
- Last 10 commits on `0.6.0-rewrite` (most recent first):

  ```text
  06dc42e chore(release): bump to 0.6.0-rc.3
  70e6487 fix(release): accept SemVer pre-release in post-publish gates
  2ea3122 chore: gitignore dev/memex/ scratch notes
  7507964 chore(release): bump to 0.6.0-rc.2
  d65f861 fix(release): pass npm --tag next for prerelease publishes
  ca0771c fix(release): split local var decl in publish-rc1-bootstrap.sh
  fee109c fix(release): code-reviewer fixups for bootstrap-publish slice
  798c7a2 fix(release): bootstrap-publish design for sibling-dep cascade
  12ee6b6 fix(release): napi win32-x64-msvc label + cargo dry-run cascade
  e66173a docs(tools): gitignored mkdocs venv + serve/build scripts for local preview
  ```

### Registry state (all live, irreversible)

| Registry                               | Versions live                                                                                           |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| crates.io (7 axis-W + 1 axis-E crates) | `0.6.0-rc.1`, `0.6.0-rc.2`, `0.6.0-rc.3`                                                                |
| PyPI (`fathomdb`)                      | `0.6.0rc2`, `0.6.0rc3` (rc.1 NOT on PyPI — bootstrap was cargo-only)                                    |
| npm (`fathomdb`, tagged `next`)        | `0.6.0-rc.2`, `0.6.0-rc.3` (rc.1 NOT on npm — same)                                                     |
| GitHub Releases                        | **NONE.** rc.2 + rc.3 workflows both failed at post-publish-smoke / co-tagging-assert / github-release. |

### What worked end-to-end

- Bootstrap publish of rc.1 (cargo only): all 7 axis-W + 1 axis-E
  crates landed via `scripts/release/publish-rc1-bootstrap.sh`.
- Workflow dispatch dry-run at rc.1: green end-to-end (run
  `26045391681`).
- rc.2 + rc.3 publish jobs: ✓ T1-T7 cargo, ✓ pypi, ✓ npm.
- rc.3 smoke (crates-cli): ✓ PASS —
  `cargo install fathomdb-cli` + `fathomdb doctor check-integrity`
  works against published crate.

### What's broken (WF-FIX-3 scope)

Three real defects in post-publish gate code surfaced by rc.3 run
`26065215220`:

1. **`scripts/release/assert-co-tagging.sh`** — `curl` to
   `https://crates.io/api/v1/crates/<name>` returns HTTP 403.
   crates.io API requires a `User-Agent` header (per
   <https://crates.io/data-access#api>). Fix: add
   `-H "User-Agent: fathomdb-release-co-tagging-check (https://github.com/coreyt/fathomdb)"`
   to the curl invocation around line 72. Per crates.io policy,
   the UA should identify the tool + include a contact URL.
2. **`scripts/release/smoke/smoke-pypi-wheel.sh`** — calls
   `e.write([])` which raises `WriteValidationError` (engine
   rejects empty batches per the 5-verb invariant). Fix: write
   one minimal valid record. Match the shape used by
   `smoke-npm-package.sh` (same defect lives there too — confirm
   by reading the JS snippet). Pick the simplest record that
   satisfies `WriteValidationError`; check
   `src/python/tests/test_surface.py` or `dev/interfaces/python.md`
   for the canonical minimal example.
3. **`scripts/release/smoke/smoke-npm-package.sh`** — `node
smoke.mjs` errors `Cannot find package '...node_modules/fathomdb/dist/index.js'`.
   The published npm tarball is missing `dist/index.js`. Two
   sub-hypotheses to investigate:
   - **(3a)** `publish-npm` job in `.github/workflows/release.yml`
     runs `npx tsc -p tsconfig.json` from `src/ts/`. Does the
     output land in `src/ts/dist/`? Check
     `src/ts/tsconfig.json` `compilerOptions.outDir`. The
     `npx tsc` step might be emitting to a different path than
     `package.json`'s `"main": "dist/index.js"` expects.
   - **(3b)** `package.json` `"files": ["dist", "fathomdb.*.node"]`
     ships `dist/`. But if tsc emits to `dist/src/index.js`
     instead of `dist/index.js`, the `main` field points at a
     non-existent path. Verify by inspecting the published
     tarball: `npm pack fathomdb@0.6.0-rc.3` + extract + ls.
   - Once fixed, the smoke test itself (the `e.write([])` call)
     ALSO needs the same WriteValidationError fix as (2).

All three fixes are workflow / scripts only — **NO change to
engine, SDK, or any published artifact code**. The artifacts of
rc.3 are correct; only the post-publish verification is broken.

### Workflow runs of record

- `26006440525` — first dispatch (rc.1 plumbing), failed at T4:
  drove WF-FIX-1.
- `26044113887` — dispatch after WF-FIX-1 cargo-package swap;
  publish-npm failed: drove npm `--tag next` fix.
- `26045391681` — clean green dry-run at rc.1 HEAD after npm fix.
- `26050992989` — REAL publish at v0.6.0-rc.2 tag.
  Cargo + pypi + npm green; co-tagging + 3 smoke failed (regex).
  Drove WF-FIX-1-resume3-and-beyond regex fix.
- `26065215220` — REAL publish at v0.6.0-rc.3 tag.
  Cargo + pypi + npm green; crates-cli smoke green; co-tagging +
  pypi-smoke + npm-smoke failed (real bugs above).

### Bootstrap script

`scripts/release/publish-rc1-bootstrap.sh` exists, is operator-run,
idempotent (sparse-index curl check on `https://index.crates.io/fa/th/<crate>`).
DO NOT run it again — rc.1 already bootstrapped. Future RCs go
through tag-push.

### Known flaky tests (rerun once before declaring red)

- `ac_017_vector_projection_freshness_p99_le_five_seconds`
- `ac_029_canonical_writes_complete_under_projection_stall`
- `t_safe_export_engine_error_exits_export_failure_66`
- `t_058_recover_truncate_wal_with_accept_data_loss_succeeds`
- `t_040a_dump_row_counts_cli_emits_counts_array`
- `t_028a_excise_source_cli_returns_excise_report`
- `t_040a_verify_embedder_cli_emits_match_status_on_matching_input`

If pre-push hook fails on one of these, retry once. If it persists,
investigate before bypassing. The user has authorized
`git push --no-verify` for unrelated flakes on tag pushes when
manually invoked; do NOT bypass for non-tag pushes without
explicit per-push HITL OK.

## Permission boundary (operator vs. orchestrator)

The harness blocks tag pushes and direct pushes to `0.6.0-rewrite`
without explicit per-action HITL. **Operator (human) runs** in
this session via `! <command>` prefix:

- `! git push origin 0.6.0-rewrite` — direct branch push.
- `! git tag v0.6.0-rc.<N> && git push origin v0.6.0-rc.<N>` — tag
  push (triggers real publish workflow).
- `! cargo login` (once per session) — populates
  `~/.cargo/credentials.toml` for the bootstrap script.

**Orchestrator runs** without HITL:

- Local edits, commits (NEVER `--no-verify` unless explicit user
  OK), cherry-picks, worktree mgmt, implementer spawns,
  code-reviewer spawns, `gh run watch` / `gh api`, prettier
  fixups, agent-verify.

`CARGO_REGISTRY_TOKEN` is in `~/.cargo/credentials.toml`. Bootstrap
script extracts via
`awk -F'"' '/^token *=/ {print $2}' ~/.cargo/credentials.toml`.

## Next slice — WF-FIX-3

Spawn one implementer in a worktree. Single commit. Prompt
template at `dev/design/orchestration.md` § 2 — PHASE=`12-WF-FIX-3`.
Pass implementer:

1. Fix #1 (co-tagging User-Agent) — 1-line change in
   `scripts/release/assert-co-tagging.sh:72`. Add UA per
   crates.io policy.
2. Fix #2 (pypi smoke write validation) — replace `e.write([])`
   in `scripts/release/smoke/smoke-pypi-wheel.sh`. Use a minimal
   valid record (check Python interface for shape). Apply same
   fix to `scripts/release/smoke/smoke-npm-package.sh` if it has
   the same defect.
3. Fix #3 (npm dist layout) — investigate
   `src/ts/tsconfig.json` outDir + actual emit path. Either fix
   tsconfig to emit `dist/index.js` directly, or fix
   `package.json` `"main"` to match the actual emit path. Then
   verify by running `npm pack` locally on `src/ts/` and
   extracting to confirm `dist/index.js` is in the tarball.
4. Tests:
   - `bash scripts/tests/test_smoke_scripts.sh` (already exists;
     may need updated assertions for the new write shape).
   - `bash scripts/tests/test_assert_co_tagging.sh` (already
     exists).
   - `bash scripts/tests/test_actionlint_fixture.sh`.
   - `bash scripts/tests/test_verify_release_gates.sh`.
   - Full `bash scripts/agent-verify.sh`.
5. Implementer must NOT push tags, NOT push origin, NOT publish,
   NOT spawn agents. Commit only.
6. Code-reviewer pass on the diff before promote.
7. Orchestrator cherry-picks onto `0.6.0-rewrite`, prettier any
   md residuals, pushes. User runs tag push.

Net LoC expectation: ~20-60 lines across 3 scripts + 1 tsconfig
edit + maybe 1 test update.

## Then — Phase 12-RC4

Cut rc.4 with the WF-FIX-3 fixes. Same shape as 12-RC2-bump.md
and 12-RC3-bump.md (both in `dev/plans/prompts/`). Single
implementer commit; bumps axis-W via
`set-version.sh --workspace` + axis-E manually + CHANGELOG entry +
co-tagging fixture row. Tag v0.6.0-rc.4. All 4 post-publish jobs
should pass end-to-end this time, including github-release.

Decision point: if rc.4 also fails for a new reason, do NOT cut
rc.5 reflexively. Stop and HITL the failure mode. We are 4 RCs in
already; each RC consumes a slot.

## Then — Phase 12-GA

After a clean rc.N run (all jobs green including github-release),
cut `0.6.0` GA per `dev/plans/0.6.0-implementation.md` § Phase 12.
Axis-E (`fathomdb-embedder-api`) regains independence at GA per
`dev/design/release.md` § RC1 bootstrap publish — bump axis-E to
its own first-stable version (likely `0.6.0`) decoupled from
axis-W's `0.6.0`. Confirm with HITL before GA tag push.

## Hard constraints (entire session)

- Do NOT run `scripts/release/publish-rc1-bootstrap.sh` — bootstrap
  is done.
- Do NOT bump versions without going through `scripts/set-version.sh
--workspace` + manual axis-E edit (axis-E opts out of workspace
  inheritance in its Cargo.toml).
- Do NOT modify a tag that's been pushed (no force-push of
  v0.6.0-rc.N). Cut a new RC instead.
- Do NOT commit `dev/memex-note-on-0.6.0.md` or anything under
  `dev/memex/` (gitignored + markdownlint-ignored).
- Do NOT bypass pre-push hooks except for known flakes on tag
  pushes, and only via operator `!` prefix.
- Net diff on workflow / release fixes: keep small. Net-negative
  LoC where possible (`feedback_reliability_principles`).
- Spawn implementers from a freshly-created git worktree per
  `dev/design/orchestration.md` § 2. Clean up worktree + phase
  branch after promote.
- Code-reviewer subagent pass before any cherry-pick to
  `0.6.0-rewrite`.
- Smart Events: log decisions / constraints / rejections via
  `wake log decision`, `wake log constraint`, `wake log rejection`
  if you make a judgment call worth preserving for the next session.

## File layout reminders

- Implementer prompts: `dev/plans/prompts/<PHASE>.md`
- Implementer run logs: `dev/plans/runs/<PHASE>-<TS>.log` (NOT
  committed — gitignored / markdownlint-ignored).
- Implementer output JSON: `dev/plans/runs/<PHASE>-output.json`
  (orchestrator copies out of worktree before worktree removal).
- Reviewer verdicts: append to commit log or
  `dev/plans/runs/<PHASE>-review.md`.
- CHANGELOG entry per RC: above the prior `## 0.6.0-rc.N` section.

## When in doubt

- Don't make scope decisions silently. Surface to user with a
  short option matrix (A/B/C with cost).
- Don't move tags or burn release slots without HITL.
- Don't claim "done" until the post-publish smoke + co-tagging +
  github-release jobs are all green and a GitHub release entry
  exists for the tag.

End of hand-off. Read `MEMORY.md` and the file references above
before starting WF-FIX-3.
