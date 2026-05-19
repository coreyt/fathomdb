# Phase 12-WF-FIX-3 — Fix three post-publish gate defects

Three real defects in post-publish gate code surfaced by the v0.6.0-rc.3
publish workflow (`gh run 26065215220`). All three are workflow / scripts
only — NO change to engine, SDK, or any published artifact code. rc.3
artifacts are correct on crates.io / PyPI / npm; only post-publish
verification is broken.

After your commit lands and the reviewer passes, the orchestrator will
cut rc.4 to re-trigger the workflow and confirm all four post-publish
jobs (co-tagging-assert + 3 smoke + github-release) go green
end-to-end.

## Required reading

- `dev/plans/prompts/12-HANDOFF.md` — full session orientation (post-rc.3
  state, registry table, what's broken).
- `dev/plans/prompts/12-RC1-WF-FIX-1.md` and `12-RC1-WF-FIX-1-resume3.md`
  — prior WF-FIX shapes; same pattern (workflow-only fix, structural
  tests, agent-verify).
- `dev/design/orchestration.md` § 2 (implementer rules), § 3 (PREAMBLE).
- `scripts/release/assert-co-tagging.sh` — Fix #1 lives here at line 75.
- `scripts/release/smoke/smoke-pypi-wheel.sh` — Fix #2.
- `scripts/release/smoke/smoke-npm-package.sh` — Fix #2 (same defect) +
  Fix #3 prerequisite.
- `scripts/tests/test_smoke_scripts.sh`, `scripts/tests/test_assert_co_tagging.sh`
  — existing structural tests; may need touch.
- `src/ts/tsconfig.json`, `src/ts/package.json` — Fix #3.
- `MEMORY.md` entries: `feedback_workflow_validation.md` (actionlint),
  `feedback_reliability_principles.md` (no scope creep),
  `feedback_release_verification.md` (smoke is the gate),
  `feedback_file_deletion.md` (never `find -delete`),
  `feedback_tdd.md` (mechanical fixes use existing suite as gate).

## Fix #1 — assert-co-tagging.sh missing User-Agent header

**Defect.** `curl https://crates.io/api/v1/crates/<crate>` returns
HTTP 403 from crates.io. Per <https://crates.io/data-access#api>, the
crates.io API requires a `User-Agent` header that identifies the tool
and includes a contact URL.

**File.** `scripts/release/assert-co-tagging.sh` — `assert_crate_has_version`
function, the `curl` invocation around line 75.

**Required change.** Add `-H` with a UA string that identifies the
release tool and links the project. Suggested literal:

```bash
-H "User-Agent: fathomdb-release-co-tagging-check (https://github.com/coreyt/fathomdb)"
```

(Keep the contact URL pattern; crates.io's policy specifically calls
out that the UA should identify the tool + provide a contact pointer.)

**Test.** `test_assert_co_tagging.sh` already exercises the script
against a local `python3 -m http.server` fixture. The fixture doesn't
care about the UA, so the existing test stays green automatically.
Optionally add an assertion that `assert-co-tagging.sh` contains a
`User-Agent:` literal to prevent regression — your call, lean toward
adding one short assertion (one line in the test).

## Fix #2 — smoke scripts use `e.write([])` which raises WriteValidationError

**Defect.** The engine rejects empty batches at write time (per the
5-verb invariant; see `src/python/tests/test_surface.py` and
`src/python/tests/test_scaffold.py` for the canonical minimal record
shape). Both PyPI and npm smoke scripts call `.write([])`, which
raises `WriteValidationError` and exits non-zero.

**Canonical minimal record** (from `test_scaffold.py:*` and
`test_surface.py:*`): `{"kind": "doc", "body": "{}"}` (Python) /
`{ kind: "doc", body: "{}" }` (TS).

**Files + required changes.**

1. `scripts/release/smoke/smoke-pypi-wheel.sh` — change
   `e.write([])` to `e.write([{"kind": "doc", "body": "{}"}])`.

2. `scripts/release/smoke/smoke-npm-package.sh` — change
   `await e.write([])` to
   `await e.write([{ kind: "doc", body: "{}" }])`.

Don't refactor — single-line change in each script.

**Test.** `test_smoke_scripts.sh` does NOT currently assert the batch
shape (it's structural only). No test update needed for the fix
itself. If you add a new structural assertion ("smoke writes a
non-empty batch"), keep it to one `assert_contains` per script.

## Fix #3 — npm tarball missing `dist/index.js`

**Defect.** Published npm tarball ships `dist/src/index.js` instead
of `dist/index.js`. `package.json` `"main": "dist/index.js"` and
`"types": "dist/index.d.ts"` both point at non-existent paths, so
`import { Engine } from "fathomdb"` fails with `Cannot find package
'.../node_modules/fathomdb/dist/index.js'`.

**Root cause (verified).** `src/ts/tsconfig.json` has
`"rootDir": "."` and `"include": ["src/**/*.ts", "tests/**/*.ts"]`.
tsc therefore emits `dist/src/...` + `dist/tests/...` preserving the
include layout. The local `src/ts/dist/src/index.js` artifact
confirms this — `find src/ts/dist -name index.js` returns the
`dist/src/` path.

**Recommended fix.** Create a separate build tsconfig that emits
`dist/index.js` directly + excludes tests from the published tarball:

1. Add `src/ts/tsconfig.build.json` with `rootDir: "src"`, `include:
   ["src/**/*.ts"]`, `outDir: "dist"`, and `exclude: ["tests"]`.
   Extend `./tsconfig.json` and override only the differences.

2. Edit `src/ts/package.json` `"scripts"`:
   - `"build": "npm run build:native && tsc -p tsconfig.build.json"`
   - `"build:debug": "npm run build:native:debug && tsc -p tsconfig.build.json"`
   - Leave `"typecheck"` and `"test"` on the original `tsconfig.json`
     so tests still type-check + emit to `dist/tests/`.

3. Edit `.github/workflows/release.yml:407` — the `publish-npm` job's
   `npx tsc -p tsconfig.json` line. Change to
   `npx tsc -p tsconfig.build.json` so the published tarball matches
   the local `npm run build` output.

Alternative considered: change `package.json` `"main"` and `"types"`
to `dist/src/index.js` / `dist/src/index.d.ts`. Rejected because the
tarball would still ship the test sources under `dist/tests/` (the
`files: ["dist", ...]` glob), inflating size and exposing test code
to npm consumers. The separate-tsconfig fix is the right substrate
shape.

**Local verification (required).**

```bash
cd src/ts
rm -rf dist
npx tsc -p tsconfig.build.json
ls dist/index.js dist/index.d.ts          # MUST exist
[ ! -d dist/src ] && echo "no dist/src — good"
[ ! -d dist/tests ] && echo "no dist/tests — good"
# Tarball check (without publishing):
npm pack --dry-run 2>&1 | grep -E 'index\.(js|d\.ts)' | head -5
```

The `npm pack --dry-run` output should list `dist/index.js` and
`dist/index.d.ts` as files going into the tarball.

**actionlint.** Run `actionlint .github/workflows/release.yml`
after the workflow edit; must stay clean.

## Scope guardrails

- NO change to engine code, SDK code, embedder code, Rust crates,
  or anything that affects published artifact behavior. rc.3
  artifacts are correct.
- NO new dependencies (cargo, pip, or npm).
- NO refactor of unrelated code.
- NO additional fixes outside the three above. If you spot something
  else, surface it in `blockers_encountered` — don't silently widen
  scope (`feedback_reliability_principles`).
- DO NOT push to origin. DO NOT push tags. DO NOT spawn agents.
  DO NOT dispatch workflow.

## Required commands

```bash
cd <worktree>

# Local verification of Fix #3.
cd src/ts
rm -rf dist
npx tsc -p tsconfig.build.json
ls dist/index.js dist/index.d.ts
npm pack --dry-run 2>&1 | grep -E 'dist/(index|src|tests)' | head -10
cd -

# Workflow lint.
actionlint .github/workflows/release.yml

# Existing structural tests — must all pass.
bash scripts/tests/test_actionlint_fixture.sh
bash scripts/tests/test_assert_co_tagging.sh
bash scripts/tests/test_verify_release_gates.sh
bash scripts/tests/test_smoke_scripts.sh

# Canonical local gate.
bash scripts/agent-verify.sh
```

Known flaky tests (rerun once before declaring red — full list in
`12-HANDOFF.md` § "Known flaky tests"). Don't bypass pre-commit
hooks.

## Commit policy

Single commit:

```text
fix(release): post-publish gate defects exposed by rc.3

- assert-co-tagging: add crates.io User-Agent (fixes HTTP 403).
- smoke-pypi-wheel + smoke-npm-package: write a minimal valid record
  (was empty batch, rejected by 5-verb invariant).
- tsconfig.build.json: rootDir=src so dist/index.js matches
  package.json "main"; release.yml publish-npm uses the new config.
```

## Output

After all commands pass and the commit is in place, write
`dev/plans/runs/12-WF-FIX-3-output.json`:

```json
{
  "phase": "12-WF-FIX-3",
  "baseline_sha": "06dc42e",
  "branch": "phase-12-WF-FIX-3-<ts>",
  "head_sha": "<HEAD after commit>",
  "commits": ["<sha>: fix(release): post-publish gate defects exposed by rc.3"],
  "fixes_landed": [
    "Fix #1: assert-co-tagging.sh User-Agent header for crates.io API",
    "Fix #2: smoke-pypi-wheel.sh + smoke-npm-package.sh write minimal valid record",
    "Fix #3: tsconfig.build.json rootDir=src + release.yml uses it for publish-npm"
  ],
  "files_changed": ["..."],
  "npm_pack_dryrun_dist_layout": "<grep output showing dist/index.js + dist/index.d.ts present, no dist/src or dist/tests>",
  "commands_run": {
    "actionlint": "pass | fail",
    "test_actionlint_fixture": "pass | fail",
    "test_assert_co_tagging": "pass | fail",
    "test_verify_release_gates": "pass | fail",
    "test_smoke_scripts": "pass | fail",
    "agent_verify": "pass | fail (+ tail)"
  },
  "blockers_encountered": [],
  "next_step_for_orchestrator": "code-reviewer pass on diff; cherry-pick onto 0.6.0-rewrite; user pushes rc.4 tag after rc.4 bump prompt runs"
}
```

Stop after output JSON written. Do NOT push, do NOT tag, do NOT
publish, do NOT spawn agents.
