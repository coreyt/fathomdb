# Phase 11d — release.yml restoration with 8-tier topological publish + post-publish smoke

Phase 11 fourth (and final) slice. Restores `.github/workflows/release.yml`
adapted to the 0.6.0 crate topology (7 workspace crates + 2 binding
packages), enforces the two-axis version policy at release gate time,
implements the 8-tier topological publish order with index-propagation
sleeps, and wires registry-installed post-publish smoke per
`feedback_release_verification`. Lands an actionlint gate so future
workflow edits don't bit-rot.

Out of scope:

- PyO3 (11a closed), napi-rs (11b closed), `set-version.sh` (11c closed).
- Phase 12 `benchmark-and-robustness.yml`.
- Actual production publishing — this slice ships the workflow + smoke
  scripts plus a dry-run mode. Real publish gates light up at REQ-048
  tag time.

## Model + effort

Opus 4.7, intent: high. Spawn from main thread:

```bash
PHASE=11d-release-workflow
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plans/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-${PHASE}-${TS}
git -C /home/coreyt/projects/fathomdb worktree add "$WT" -b "phase-${PHASE}-${TS}" 0.6.0-rewrite
PREAMBLE=$(cat <<'EOF'
YOU ARE THE IMPLEMENTER. Not the orchestrator. Do the work in this
worktree. Do NOT re-spawn yourself. Do NOT spawn other agents. The
"## Model + effort" section in this prompt describes how YOU were
just launched (claude -p with the listed model/effort). Do NOT re-run
that block. Use --disallowedTools Task Agent as a hard guard if you
forget. Write code, run tests, commit. Done.
EOF
)
( cd "$WT" && \
  ( echo "$PREAMBLE"; cat /home/coreyt/projects/fathomdb/dev/plans/prompts/11d-release-workflow.md ) \
  | claude -p --model claude-opus-4-7 --effort high \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --disallowedTools Task Agent \
      --permission-mode bypassPermissions \
      --output-format stream-json --include-partial-messages --verbose \
  > "$LOG" 2>&1 )
```

Anti-chaining: PREAMBLE prepended via stdin. Reviewer
(`codex --model gpt-5.4 --sandbox read-only`) MANDATORY: this slice
ships the release gate that REQ-048 tag-time fires.

## Log destination

- stdout/stderr: `dev/plans/runs/11d-release-workflow-<ts>.log`
- structured output: `dev/plans/runs/11d-release-workflow-output.json`
- reviewer verdict: `dev/plans/runs/11d-review-<ts>.md`

## Required reading

- `AGENTS.md` (§1, §3, §4, §5, §7).
- `MEMORY.md`, especially:
  - `feedback_release_verification.md` — green CI + published wheel is
    NOT done. Smoke against installed-from-registry artifact.
  - `feedback_workflow_validation.md` — actionlint is the workflow
    validator. `yaml.safe_load` is NOT a validator.
  - `feedback_reliability_principles.md` — net-negative LoC on
    reliability releases; no soak; delete-before-add.
  - `feedback_no_data_migration.md` — schema-only migrations only;
    irrelevant here but flagged so you don't add a data step.
- `dev/design/release.md` (entire file, ~99 lines). Owns:
  - Two version axes (Axis W workspace lockstep, Axis E
    `fathomdb-embedder-api` independent).
  - 8-tier topological publish order (T1..T8).
  - Sibling-package co-tagging (REQ-048).
  - Post-publish smoke.
- `dev/plans/ci-deferred.md` § `release.yml — Phase 11` — the ticket
  being burned down. Lists the four adaptations required for 0.6.0.
- Pre-0.6.0 source for layout reference only — DO NOT copy blindly,
  the new layout uses `src/{rust,python,ts}/` and the crate set
  differs (now 7 workspace + 2 binding, was 4 + 2):
  `git show 39ee271^:.github/workflows/release.yml`.
- `dev/acceptance.md`:
  - AC-052 (line 836) — co-tagged sibling releases at every published
    version.
  - AC-053 (line 844) — single source of truth for version (the
    `set-version.sh --check-files` shape that 11c landed).
  - AC-054 (line 852) — atomic multi-registry publish. The release
    workflow MUST refuse to call the release done if any registry
    publish is in failed state.
  - AC-056 (line 868) — registry-installed wheel is the release gate.
    Workflow must run `pip install fathomdb==<version>` from PyPI in
    a fresh venv + an end-to-end open + write + search + close +
    process-exit script.
- Current state to patch:
  - `.github/workflows/ci.yml` — for reference on action-pinning and
    bootstrap pattern. Use the SAME pinned-SHA actions where they
    overlap (checkout, setup-python, setup-node, rust-toolchain,
    rust-cache).
  - `scripts/set-version.sh` (11c-landed). Workflow consumes
    `--check-files` mode as a release gate.
  - `scripts/agent-lint.sh`, `scripts/bootstrap.sh` — for the
    actionlint wiring point.
  - `src/rust/crates/fathomdb-py/` (PyO3 binding crate from 11a).
  - `src/rust/crates/fathomdb-napi/` (napi-rs binding from 11b).
  - `src/python/pyproject.toml` — maturin backend lands here with
    `[tool.maturin] manifest-path = "../rust/crates/fathomdb-py/Cargo.toml"`.
    `working-directory: src/python` is correct.
  - `src/ts/package.json` — flat layout (NO `packages/` subdir). The
    napi config `{"name": "fathomdb", "triples": {"defaults": true}}`
    controls native output naming. Native binary lives at
    `src/ts/fathomdb.<platform-label>.node`. Package name is
    `fathomdb` and currently `"private": true` — see Blocker 7.
  - Build script `npm run build:native` (in `src/ts/package.json`)
    invokes `napi build --platform --release --cargo-cwd
    ../rust/crates/fathomdb-napi --js false`. Prefer this over a raw
    `cargo build` + manual rename — the napi-rs CLI handles the
    platform-label rename per the `napi.triples` config.

## Scope — six work items

One commit per item is fine. TDD for the scripts (smoke + co-tagging
assertion + actionlint wiring); workflow YAML lands with actionlint
green as the test.

### Item 1 — `.github/workflows/release.yml` restoration

Restore the release workflow adapted to 0.6.0 topology. Trigger:
`push` on tags matching `v*` PLUS `workflow_dispatch` (for dry-run
rehearsal — see Item 4).

Required permissions block (top of file):

```yaml
permissions:
  actions: read
  contents: write   # for github-release
  id-token: write   # for OIDC trusted publishing (PyPI + npm)
```

Job graph (each job uses `needs:` to gate, mirroring pre-0.6.0):

1. **`verify-release`** — single ubuntu job. Runs:
   - `bash scripts/set-version.sh --check-files` (AC-053 gate; rejects
     Axis W lockstep drift + Axis E inheritance regression).
   - `bash scripts/verify-release-gates.sh` (NEW — see Item 2).
   - actionlint on workflow files (`actionlint .github/workflows/*.yml`).
2. **`build-python`** — matrix of (linux-x86_64-gnu, linux-aarch64-gnu,
   darwin-x86_64, darwin-aarch64, windows-x86_64). Uses
   `PyO3/maturin-action`. `working-directory: src/python`. Python
   versions: 3.10, 3.11, 3.12. Manylinux: `2_28` for the two linux
   targets. Artifacts: `python-dist-<target>` containing `dist/*`.
   Pin maturin-action to the SAME SHA the pre-0.6.0 source pinned
   unless that SHA is older than 12 months — if so, surface a blocker
   asking whether to bump.
3. **`build-napi`** — matrix of (linux-x86_64-gnu, darwin-x86_64,
   darwin-aarch64, win32-x86_64). Builds the napi binding via the
   `src/ts/` package script: `cd src/ts && npm ci && npm run
   build:native` with the target injected via `CARGO_BUILD_TARGET`
   (or by setting `--target <target>` on a `napi build` override
   step). The napi-rs CLI handles the rename to
   `fathomdb.<platform-label>.node` per the `napi.triples` config in
   `package.json`. Native binary lands at `src/ts/fathomdb.<label>.node`
   (flat — NO `prebuilds/` subdir; that was pre-0.6.0). Artifacts:
   `napi-<label>` containing `fathomdb.<label>.node`.
4. **`build-rust`** — ubuntu. Runs:
   - `cargo build --release --workspace`.
   - `cargo package --no-verify -p fathomdb-embedder-api`,
     `-p fathomdb-schema`, `-p fathomdb-query` (the three crates with
     no in-workspace deps — `fathomdb-embedder-api` is Axis E and
     truly leaf; `fathomdb-schema` is per ADR-0.6.0-crate-topology a
     leaf in the workspace graph; `fathomdb-query` depends on
     `fathomdb-schema` per release.md T3 — verify before adding it
     here, drop if it would fail manifest resolve).
   - Dependent crates (`fathomdb-engine`, `fathomdb-embedder`,
     `fathomdb`, `fathomdb-cli`) NOT packaged here. Manifest
     correctness for those is enforced at actual `cargo publish` time.
     Comment the rationale in the workflow (the pre-0.6.0 comment is
     correct and should be carried forward).
5. **`all-builds-passed`** — ubuntu cross-ecosystem gate. `needs:
   [verify-release, build-python, build-napi, build-rust]`. Single
   `echo` step. Rationale comment per pre-0.6.0 source.
6. **`publish-rust-t1-embedder-api`** — Tier 1. `needs:
   all-builds-passed`. `cargo publish -p fathomdb-embedder-api
   --token ${{ secrets.CARGO_REGISTRY_TOKEN }}`. Then `sleep 60` for
   index propagation.
7. **`publish-rust-t2-schema`** — Tier 2. `needs:
   publish-rust-t1-embedder-api`. Publish + sleep 60.
8. **`publish-rust-t3-query`** — Tier 3. `needs: publish-rust-t2-schema`.
   Publish + sleep 60.
9. **`publish-rust-t4-engine`** — Tier 4. `needs: publish-rust-t3-query`.
   Publish + sleep 60.
10. **`publish-rust-t5-embedder`** — Tier 5. `needs:
    publish-rust-t4-engine` (per release.md "independent of engine"
    in graph terms, but T5 must wait for the publish gate ordering;
    chain on T4 for the index sleep cadence). Publish + sleep 60.
11. **`publish-rust-t6-facade`** — Tier 6. `needs:
    publish-rust-t5-embedder`. Publish (fathomdb) + sleep 60.
12. **`publish-rust-t7-cli`** — Tier 7. `needs: publish-rust-t6-facade`.
    Publish `fathomdb-cli` + sleep 60.
13. **`publish-pypi`** — Tier 8a. `needs: publish-rust-t4-engine`
    (per release.md: PyPI wraps engine directly; can publish in
    parallel with T5..T7). Uses
    `pypa/gh-action-pypi-publish` with OIDC environment `pypi`.
14. **`publish-npm`** — Tier 8b. `needs: publish-rust-t4-engine`.
    Same parallel gate as PyPI. Uses npm OIDC trusted publishing via
    `npx npm@latest publish --provenance --access public`.
    `working-directory: src/ts`. Pull all `napi-*` artifacts into
    `src/ts/` as siblings of `dist/` so the napi loader can find them.
    NOTE the `"private": true` flag in `src/ts/package.json` and the
    bare `"name": "fathomdb"` (not `@fathomdb/fathomdb`) — see
    Blocker 7. Workflow must not paper over a misconfigured package.
15. **`post-publish-smoke`** — NEW per AC-056 + AC-053 +
    `feedback_release_verification`. `needs: [publish-rust-t7-cli,
    publish-pypi, publish-npm]`. Runs three smoke matrices in parallel
    (one job each, or a 3-leg matrix):
    - `crates-io-smoke`: fresh ubuntu. `cargo install fathomdb-cli
      --version <tag-version>`. Create a tempdir fixture DB. Run
      `fathomdb doctor check-integrity --json` (per release.md
      § Post-publish smoke). Assert exit 0 + the JSON parses.
    - `pypi-smoke`: fresh ubuntu. `python -m venv /tmp/smoke && source
      /tmp/smoke/bin/activate && pip install fathomdb==<tag-version>`.
      Run an end-to-end script (NEW — see Item 3) that opens a DB,
      writes a row, runs a query, closes, and exits with status 0.
    - `npm-smoke`: fresh ubuntu. `mkdir /tmp/smoke && cd /tmp/smoke
      && npm init -y && npm install <published-npm-name>@<tag-version>`.
      Run the TS equivalent end-to-end smoke script. The published
      npm name MUST match what publish-npm actually publishes — keep
      a single source of truth (e.g. read from
      `src/ts/package.json` `name` at workflow time).
    Extract `<tag-version>` from `${{ github.ref_name }}` stripping the
    leading `v`.
16. **`co-tagging-assert`** — NEW per AC-052. `needs:
    [publish-rust-t7-cli, publish-pypi, publish-npm]`. Runs
    `scripts/release/assert-co-tagging.sh <tag-version>` (NEW — see
    Item 5). Queries crates.io / PyPI / npm via curl + jq for the
    sibling triple `fathomdb` + `fathomdb-embedder` +
    `fathomdb-embedder-api` and asserts all three exist at the
    expected version (Axis W for the first two; Axis E for the third,
    read from `fathomdb-embedder-api/Cargo.toml` at the tag commit).
17. **`github-release`** — `needs: [post-publish-smoke,
    co-tagging-assert]`. Only this job marks the release done by
    creating the GitHub Release. Implements AC-054 atomicity: if any
    smoke or co-tagging job is red, this job does not run, so the
    release is not marked complete.

All third-party actions MUST be pinned to commit SHAs (per `ci.yml`
H-5 convention). Reuse the same pins where they overlap (checkout,
setup-python, setup-node, rust-toolchain, rust-cache).

### Item 2 — `scripts/verify-release-gates.sh`

Bash, not Python. The pre-0.6.0 source used
`scripts/verify-release-gates.py`; 0.6.0 standardizes on bash for the
release substrate (set-version.sh is bash; smoke scripts are bash).
Mixing python+bash here would add an unnecessary dependency.

Required checks (each emits one structured line on failure; exits
non-zero on first failure with a clear message):

1. Tag matches `v<axis-W-version>` and that version matches Axis W in
   `Cargo.toml` `[workspace.package].version`.
2. `bash scripts/set-version.sh --check-files` passes (delegate; do
   not duplicate the drift checks).
3. The HEAD commit is reachable from `main` (no tag-from-feature-branch).
4. `CHANGELOG.md` has a section heading matching the tag version
   (skip if `CHANGELOG.md` does not exist in this tree — surface a
   blocker, do not silently pass).
5. All Axis E + Axis W crates have a populated `description`,
   `license`, and `repository` field in their `Cargo.toml`
   (`cargo publish` requires these; failing early beats failing at
   T4 after T1+T2+T3 succeed).

Tests under `scripts/tests/test_verify_release_gates.sh` per
`test_set_version.sh` pattern. Each check has at least one positive
and one negative case. Wire into `scripts/agent-test.sh` per the
`test_set_version.sh` precedent.

### Item 3 — Post-publish smoke scripts

Three scripts under `scripts/release/smoke/`:

1. `smoke-crates-cli.sh` — installs `fathomdb-cli` via
   `cargo install` from crates.io (version passed as `$1`), creates a
   tempdir fixture DB, runs `fathomdb doctor check-integrity --json`,
   asserts exit 0 + valid JSON output. Cleans tempdir.
2. `smoke-pypi-wheel.sh` — creates a venv, `pip install
   fathomdb==$1`, invokes a Python one-liner that opens a DB at
   a tempdir, writes a row through the SDK, runs a query, closes,
   exits 0. Per AC-056 the script also process-exits cleanly (the
   open + close + exit pattern from `feedback_release_verification`).
3. `smoke-npm-package.sh` — `npm init -y`, `npm install
   @fathomdb/fathomdb@$1`, runs a Node one-liner that exercises the
   napi binding end-to-end + process-exits cleanly.

Each smoke script: hardened bash (`set -euo pipefail`), accepts the
version as `$1`, validates `$1` matches `^[0-9]+\.[0-9]+\.[0-9]+$`,
uses `mktemp -d` for fixture dirs, cleans up on EXIT trap.

Tests: NOT shelled-out integration tests against the real registry
(too slow + flaky for CI). Instead, `scripts/tests/test_smoke_scripts.sh`
asserts script structure: shebang, `set -euo pipefail`, version
regex check, mktemp + EXIT trap presence, version-pinned install
command shape. This is meta-testing of the script structure, not
behavior. Document this WHY clearly in the test file header.

Wire test file into `scripts/agent-test.sh`.

### Item 4 — `workflow_dispatch` dry-run rehearsal

Add `workflow_dispatch` trigger with a `dry_run: boolean` input.
When `dry_run: true`:

- All `cargo publish` steps run with `--dry-run`.
- `pypa/gh-action-pypi-publish` uses a `repository-url:
  https://test.pypi.org/legacy/` override (test.pypi).
- `npm publish` runs with `--dry-run`.
- Post-publish smoke jobs are SKIPPED (cannot install from test.pypi
  in a way that mirrors prod; the smoke is structured around real
  publishes).
- `co-tagging-assert` is SKIPPED.
- `github-release` is SKIPPED.

Implementation: a top-level `env: DRY_RUN: ${{
inputs.dry_run || 'false' }}` plus conditional flags on the publish
steps. Keep the conditional logic minimal — each publish job branches
on `$DRY_RUN`, no `if:` job-level gating that would change the job
graph shape.

### Item 5 — `scripts/release/assert-co-tagging.sh`

Bash. Accepts version as `$1` (Axis W). Queries:

- `https://crates.io/api/v1/crates/fathomdb` → assert `$1` in
  `versions[].num`.
- `https://crates.io/api/v1/crates/fathomdb-embedder` → assert `$1`.
- `https://crates.io/api/v1/crates/fathomdb-embedder-api` → assert
  the Axis E version (read from `src/rust/crates/fathomdb-embedder-api/
Cargo.toml` `version` field at HEAD) is in `versions[].num`.

curl + jq. `set -euo pipefail`. On failure, emit:
`co-tagging-violation: <package> <expected-version> not in registry`.

Test: `scripts/tests/test_assert_co_tagging.sh` mocks crates.io
responses via a tiny `python3 -m http.server` fixture pointing at
static JSON files under `dev/release/fixtures/co-tagging/`. Positive
case (all three present) + negative case (one missing). Wire into
`scripts/agent-test.sh`.

### Item 6 — actionlint wiring

Per `feedback_workflow_validation.md` — actionlint is the validator.

1. Add `actionlint` install step to `scripts/bootstrap.sh`. The
   actionlint binary release pattern: `go install
   github.com/rhysd/actionlint/cmd/actionlint@v1.7.7` (pin the
   version; verify v1.7.7 is current — if newer is out, bump and
   note in commit). Cache the binary path consistent with how
   bootstrap handles other go-installed binaries.
2. Add `actionlint .github/workflows/*.yml` invocation to
   `scripts/agent-lint.sh`. Place it after the shellcheck step (the
   adjacent shell-lint conceptually) and before the markdown lint.
3. Verify `agent-lint.sh` exits non-zero if any workflow file fails
   actionlint. Add a deliberately-broken workflow file under
   `scripts/tests/fixtures/actionlint-bad.yml` and a test asserting
   `actionlint` rejects it; remove the broken fixture before commit
   OR keep it under a path actionlint does not scan
   (`.github/workflows/*.yml` only). Cleanest: put the bad fixture
   under `scripts/tests/fixtures/` and run actionlint against it
   explicitly in the test, not via the agent-lint glob.

## Required commands

```bash
cd /tmp/fdb-11d-release-workflow-<ts>
# Workflow YAML is valid per actionlint (the canonical workflow validator).
actionlint .github/workflows/release.yml
actionlint .github/workflows/ci.yml
# Verify-release-gates script tests pass.
bash scripts/tests/test_verify_release_gates.sh
# Co-tagging assert script tests pass (offline; mocked registry).
bash scripts/tests/test_assert_co_tagging.sh
# Smoke-script structure tests pass.
bash scripts/tests/test_smoke_scripts.sh
# Existing set-version tests still pass (11c regression guard).
bash scripts/tests/test_set_version.sh
# Existing pip-skew + cargo-skew fixtures still pass (11c regression guard).
bash dev/release/tests/pip_skew.sh
bash dev/release/tests/cargo_skew.sh
# Canonical local gate.
bash scripts/agent-verify.sh
```

All must pass. Flake reruns (rerun once before declaring red):

- `ac_029_canonical_writes_complete_under_projection_stall`
- `ac_017_vector_projection_freshness_p99_le_five_seconds`
- `t_safe_export_engine_error_exits_export_failure_66`

Pre-existing host-load timing flakes unrelated to 11d.

## Discipline

- TDD on the bash scripts (Items 2, 3, 5, 6): failing test commits
  before the implementation makes it green. The YAML restoration
  (Item 1) lands with actionlint green as the test — that's
  acceptable because actionlint is the structural contract for that
  artifact.
- No `yaml.safe_load` anywhere in this slice for workflow validation.
  Memory hard rule.
- No copy-paste from pre-0.6.0 release.yml. The crate set differs
  (7 + 2 vs 4 + 2); the tier count differs (8 vs 3); the layout
  differs (`src/{rust,python,ts}/` vs `python/` + `typescript/`).
  Use the pre-0.6.0 source for action pins + comment phrasing, then
  rebuild the job graph from `release.md § Tiered publish order`.
- Comment policy: no WHAT comments, only non-obvious WHY. The
  pre-0.6.0 source has good WHY comments (e.g. "cargo package
  rewrites path deps to versioned deps and then does a registry
  resolve") — carry those forward, do not invent new WHAT noise.
- No scope creep: AC-054 release-finalize atomicity is partially
  satisfied by `github-release` job ordering (it only runs after
  all publishes + smoke + co-tagging succeed). A standalone
  `release-finalize.sh` script per AC-054's letter is out of scope
  for 11d — that's a 12.x item if it lands at all.
- Net-negative LoC posture: the workflow will be longer than current
  zero, but each line must earn its keep. Prefer fewer matrix legs
  (the pre-0.6.0 source had aarch64-linux for build-napi —
  it's omitted here; verify napi-rs supports the architectures the
  matrix covers before adding more).

## Blockers — surface before writing code

If any of these blocks the work, STOP and write a blocker report at
`dev/plans/runs/11d-release-workflow-output.json` per the 10b-B
blocker shape:

1. `actionlint` not installable in the bootstrap environment (no go
   toolchain, vendored binary not available). Workaround would
   require a pre-built binary download from the actionlint releases
   page; document the SHA pin if that's the chosen path.
2. The napi binding crate `fathomdb-napi` does not produce a
   library named consistently with the loader convention from 11b.
   Investigation needed before the rename + stage step in
   `build-napi` can be written. Surface the actual binary names
   produced per target.
3. The PyO3 build under `src/python/` requires a maturin config (e.g.
   `pyproject.toml` `[tool.maturin]` section) that wasn't landed in
   11a. Surface the missing config.
4. The `scripts/release/` directory does not exist yet. Create it as
   part of this slice (not a blocker, just flagged so it's
   intentional).
5. `dev/release/fixtures/co-tagging/` does not exist yet. Create it
   as part of this slice.
6. If actual `cargo publish --dry-run` of any of the seven crates
   fails locally (manifest issues, missing fields, etc.), that's a
   real blocker because the workflow will fail at tag time. Run
   `cargo publish --dry-run -p <crate>` for each of the seven and
   surface any failures.
7. `src/ts/package.json` currently has `"private": true` and
   `"name": "fathomdb"`. The pre-0.6.0 workflow published as
   `@fathomdb/fathomdb` (scoped). For npm publish to succeed at tag
   time, either `"private"` must be flipped (and the name kept
   bare) OR the name must be scoped (and `"private"` removed since
   scoped packages publish public via `--access public`). This is a
   package-config decision, not a workflow decision. RECOMMENDED
   path: flip `"private": false` and keep the bare `"fathomdb"`
   name to match the crate + python wheel name (single brand). But
   confirm with available context (any pre-0.6.0 npm publish history,
   any 11b decisions captured in
   `dev/plans/runs/11b-*-output.json`) before editing. If
   `dev/plans/runs/` has no answer, surface as blocker — do not
   silently choose.

Blocker report shape: same as 10b-B
(`dev/plans/runs/10b-B-purge-restore-output.json`).

## Output

After all commands pass, write
`dev/plans/runs/11d-release-workflow-output.json`:

```json
{
  "phase": "11d-release-workflow",
  "baseline_sha": "<HEAD on 0.6.0-rewrite before cherry-pick>",
  "branch": "phase-11d-release-workflow-<ts>",
  "head_sha": "<HEAD after final commit on the worktree branch>",
  "commits": ["<sha>: <subject>", "..."],
  "workflow_file": ".github/workflows/release.yml",
  "workflow_line_count": <int>,
  "tier_count": 8,
  "publish_jobs": ["publish-rust-t1-embedder-api", "publish-rust-t2-schema", "publish-rust-t3-query", "publish-rust-t4-engine", "publish-rust-t5-embedder", "publish-rust-t6-facade", "publish-rust-t7-cli", "publish-pypi", "publish-npm"],
  "smoke_jobs": ["crates-io-smoke", "pypi-smoke", "npm-smoke"],
  "scripts_added": ["scripts/verify-release-gates.sh", "scripts/release/assert-co-tagging.sh", "scripts/release/smoke/smoke-crates-cli.sh", "scripts/release/smoke/smoke-pypi-wheel.sh", "scripts/release/smoke/smoke-npm-package.sh"],
  "tests_added": ["scripts/tests/test_verify_release_gates.sh", "scripts/tests/test_assert_co_tagging.sh", "scripts/tests/test_smoke_scripts.sh"],
  "actionlint_wired": true,
  "actionlint_version_pin": "v1.7.7",
  "dry_run_supported": true,
  "acceptance_criteria_addressed": ["AC-052", "AC-053", "AC-056"],
  "out_of_scope_deferred": ["AC-054 standalone release-finalize.sh — partial via github-release job ordering"],
  "blockers_encountered": [],
  "agent_verify_result": "pass | fail (+ tail)",
  "next_step_for_orchestrator": "promote to 0.6.0-rewrite; respawn codex reviewer for PASS verdict"
}
```

Then stop. Do not advance to Phase 12. Do not run the reviewer
yourself. Do not attempt to push the tag to trigger the workflow —
this slice ships the workflow; REQ-048 fires it.
