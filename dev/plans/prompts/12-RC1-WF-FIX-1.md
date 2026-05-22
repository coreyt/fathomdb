# Phase 12-RC1-WF-FIX-1 — Fix release.yml issues exposed by dry-run

Two workflow bugs surfaced by the 12-RC1 dry-run dispatch
(`gh run 26006440525`):

1. **(a) napi win32-x64 artifact path mismatch.** Matrix label
   `win32-x64` produces no `.node` artifact because napi-rs CLI
   emits `fathomdb.win32-x64-msvc.node` (full triple including
   `-msvc` ABI suffix). Loader in `src/ts/src/binding.ts:30` also
   expects `win32-x64-msvc`. Real publish would ship a broken npm
   package for Windows clients.
2. **(b) cargo dry-run cascade fails at T4.** `cargo publish
--dry-run -p fathomdb-engine` resolves `fathomdb-embedder-api`
   against the real crates.io index. T1's dry-run doesn't actually
   upload, so T4-T7 dry-runs all fail with "no matching package".
   T8 PyPI + npm + smoke + co-tagging + github-release never
   exercised on dry-run.

Out of scope:

- Pushing any real release.yml run.
- Pushing any crates / pypi / npm packages.
- 12-RC1 Sub-3 HITL approval gate (still gated; this slice unblocks
  Sub-2 dry-run to actually exercise the full workflow).

## Model + effort

Opus 4.7, intent: medium. Per `dev/design/orchestration.md` § 2:

```bash
PHASE=12-RC1-WF-FIX-1
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plans/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-${PHASE}-${TS}
git -C /home/coreyt/projects/fathomdb worktree add "$WT" -b "phase-${PHASE}-${TS}" 0.6.0-rewrite
PREAMBLE=$(cat <<'EOF'
YOU ARE THE IMPLEMENTER. Not the orchestrator. Do the work in this
worktree. Do NOT re-spawn yourself. Do NOT spawn other agents. Do
NOT push to any remote. Do NOT trigger workflow_dispatch. Do NOT
push any tags. Use --disallowedTools Task Agent as a hard guard.
Write code, run tests, commit. Done.
EOF
)
( cd "$WT" && \
  ( echo "$PREAMBLE"; cat /home/coreyt/projects/fathomdb/dev/plans/prompts/12-RC1-WF-FIX-1.md ) \
  | claude -p --model claude-opus-4-7 --effort medium \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --disallowedTools Task Agent \
      --permission-mode bypassPermissions \
      --output-format stream-json --include-partial-messages --verbose \
  > "$LOG" 2>&1 )
```

## Required reading

- `dev/plans/prompts/12-RC1-tag-rc1.md` — the parent procedural
  plan; this slice unblocks Sub-2 dry-run.
- `.github/workflows/release.yml` lines 97-143 (build-napi matrix)
  and lines 200-355 (publish-rust tiers).
- `src/ts/src/binding.ts:21-32` — TRIPLES list the loader probes.
- `src/ts/package.json` — `napi.triples` block + `build:native`
  script.
- `dev/design/release.md` § Tiered publish order — the 8-tier
  publish design that the cargo dry-run cascade fails to exercise.
- Dry-run output: `gh run view 26006440525` (already failed at
  T4); two specific failures:
  - `cargo publish --dry-run -p fathomdb-engine`:
    `error: no matching package named fathomdb-embedder-api found`
  - `! No files were found with the provided path: src/ts/fathomdb.win32-x64.node`

## Sub-1 — (a) napi win32-x64 label fix

**Required fix:**

1. Edit `.github/workflows/release.yml` build-napi matrix entry
   for Windows (around line 118-120): change `label: win32-x64`
   to `label: win32-x64-msvc` so it matches the napi-rs CLI's
   actual output filename (and `binding.ts:30` TRIPLES list).
2. Re-check the artifact upload path on line 143 — it interpolates
   from `${{ matrix.label }}` so the path becomes
   `src/ts/fathomdb.win32-x64-msvc.node` automatically. No
   second edit needed.
3. Verify other matrix entries still match the loader's TRIPLES
   list:
   - linux-x64-gnu ↔ TRIPLES[0]
   - darwin-x64 ↔ TRIPLES[5]
   - darwin-arm64 ↔ TRIPLES[6]
   - win32-x64-msvc ↔ TRIPLES[8]
4. Consider whether to also add aarch64-linux + linux-arm64-gnu
   per the existing dev binary `fathomdb.linux-arm64-gnu.node`.
   The current matrix omits aarch64-linux entirely. Surface as a
   decision: (a) leave aarch64-linux out for 0.6.0-rc.1 (acceptable
   since Tegra aarch64 is dev-only); (b) add it now to match the
   committed `.node` file in src/ts/. **Recommend (a) — Out of
   scope for this fix.** Document in output JSON if implementer
   confirms aarch64 omission is intentional for now.

Run `actionlint .github/workflows/release.yml` after edit; must
stay clean.

## Sub-2 — (b) cargo dry-run cascade fix

**Required fix:**

T1-T7 all use the same shape today:

```yaml
- name: cargo publish <crate>
  run: |
    if [ "${DRY_RUN}" = "true" ]; then
      cargo publish --dry-run -p <crate>
    else
      cargo publish -p <crate> --token "${CARGO_REGISTRY_TOKEN}"
    fi
```

The dry-run branch fails for dependent crates because
`cargo publish --dry-run` resolves sibling versions against
crates.io.

**Right fix:** use `cargo package --allow-dirty --no-verify
-p <crate>` for dry-run mode. `cargo package` produces the
publishable tarball without checking the registry; `--no-verify`
skips the post-package compile check (which itself would resolve
sibling versions against crates.io). This is the canonical
"build the tarball, skip the registry resolve" path for dry-run
verification.

Apply to T1-T7 by changing the dry-run branch:

```yaml
- name: cargo publish <crate>
  run: |
    if [ "${DRY_RUN}" = "true" ]; then
      cargo package --allow-dirty --no-verify -p <crate>
    else
      cargo publish -p <crate> --token "${CARGO_REGISTRY_TOKEN}"
    fi
```

The `--allow-dirty` is needed because the workflow checkout produces
a working tree that cargo considers dirty (e.g. target/ exists from
prior matrix builds in the same job). `--no-verify` skips the
re-compile step at package time (the build matrix already validated
compilation in `build-rust`).

Alternative considered: parallel-not-cascade dry-runs. Rejected —
same resolve failure; running in parallel doesn't make the sibling
crate appear on crates.io.

Alternative considered: skip dry-run for T2-T7. Rejected — loses
the value of catching manifest/metadata errors per crate before
real publish.

**Verify the fix works locally:** the implementer should run
`cargo package --allow-dirty --no-verify -p fathomdb-embedder-api`,
`-p fathomdb-schema`, `-p fathomdb-query`, `-p fathomdb-engine`,
`-p fathomdb-embedder`, `-p fathomdb` (facade), `-p fathomdb-cli`
locally and assert each succeeds. If any fail, surface as blocker
(real packaging issue not previously caught).

The 60s index-propagation sleeps between tiers stay (per
`release.yml:351`). On dry-run they're no-ops because nothing was
published; the workflow still serializes which is fine.

Update the dry-run input description on `release.yml:10` to reflect
the new behavior (it currently says "cargo --dry-run"; should say
"cargo package --allow-dirty --no-verify").

Run `actionlint .github/workflows/release.yml` after edit; clean.

## Sub-3 — Verify in test suite

If `scripts/tests/test_verify_release_gates.sh` or
`scripts/tests/test_actionlint_fixture.sh` cover release.yml
shape assertions, ensure they still pass. Most likely these tests
don't check the matrix labels or per-step dry-run shape.

Add a small assertion to one of:

- `scripts/tests/test_actionlint_fixture.sh`
- A new small test file

that greps `.github/workflows/release.yml` for the canonical labels

- asserts `win32-x64-msvc` is present (not bare `win32-x64`) + asserts
  each `publish-rust-t*` step has `cargo package` (not `cargo publish`)
  in the dry-run branch.

This protects against regression if the matrix is rewritten later.

## Required commands

```bash
cd /tmp/fdb-12-RC1-WF-FIX-1-<ts>
# actionlint clean after edits.
actionlint .github/workflows/release.yml
# Local cargo package --allow-dirty --no-verify per crate (T1-T7
# sequence).
for c in fathomdb-embedder-api fathomdb-schema fathomdb-query \
         fathomdb-engine fathomdb-embedder fathomdb fathomdb-cli; do
  echo "=== $c ==="
  cargo package --allow-dirty --no-verify -p "$c" || exit 1
done
# Regression: existing test suites still green.
bash scripts/tests/test_verify_release_gates.sh
bash scripts/tests/test_actionlint_fixture.sh
# Canonical local gate.
bash scripts/agent-verify.sh
```

All must pass. Known flakes (rerun once before declaring red):
`ac_029`, `ac_017`, `t_safe_export_engine_error_exits_export_failure_66`,
`t_058_recover_truncate_wal_with_accept_data_loss_succeeds` +
`t_040a_dump_row_counts_cli_emits_counts_array` (NEW parallel-race
flakes observed during 12-RC1 push — both pass with `--test-threads=1`
but race under cargo's default parallel cargo-test load).

## Discipline

- Net LoC: ~10-30 lines (workflow edits + maybe a small test).
- No new dependencies.
- Single commit OK ("fix(release): napi win32-x64-msvc label + cargo
  dry-run cascade").
- Per `feedback_workflow_validation`: actionlint validates;
  yaml.safe_load does NOT.
- Per `feedback_reliability_principles`: don't add scope creep.

## Blockers — surface before writing code

1. **`cargo package --allow-dirty --no-verify` produces a different
   verification signal than `cargo publish --dry-run`.** If
   `cargo package` doesn't catch manifest issues that
   `cargo publish --dry-run` does for leaf crates (e.g. missing
   description, license fields), the dry-run gate weakens. Verify
   by running both forms locally against `fathomdb-embedder-api`
   (T1 has no internal deps so `cargo publish --dry-run` works
   there). If outputs diverge meaningfully, surface.
2. **napi-rs CLI Windows triple naming.** Verify by inspecting
   napi-rs source or the existing `fathomdb.linux-arm64-gnu.node`
   that the naming convention is `<platform>-<arch>-<abi>`. If
   the Windows naming is actually `win32-x64-msvc` vs the matrix's
   current `win32-x64`, the fix is just adding `-msvc`. If napi-rs
   doesn't append the ABI on Windows, the fix is different.

## Output

After all commands pass, write
`dev/plans/runs/12-RC1-WF-FIX-1-output.json`:

```json
{
  "phase": "12-RC1-WF-FIX-1",
  "baseline_sha": "<HEAD of 0.6.0-rewrite at spawn>",
  "branch": "phase-12-RC1-WF-FIX-1-<ts>",
  "head_sha": "<HEAD after final commit>",
  "commits": ["<sha>: <subject>", "..."],
  "fixes_landed": [
    "(a) napi win32-x64 → win32-x64-msvc label fix",
    "(b) T1-T7 cargo dry-run switched to `cargo package --allow-dirty --no-verify`"
  ],
  "actionlint_result": "pass",
  "cargo_package_per_crate_result": "all 7 crates pass | fail (+ which crate)",
  "test_actionlint_fixture_result": "pass | fail",
  "test_verify_release_gates_result": "pass | fail",
  "agent_verify_result": "pass | fail (+ tail)",
  "blockers_encountered": [{...}],
  "next_step_for_orchestrator": "promote to 0.6.0-rewrite; push to origin; re-trigger workflow_dispatch dry-run; monitor for green run end-to-end; ONLY then proceed to HITL approval for real tag push"
}
```

Then stop. Do not push anything. Do not trigger workflow_dispatch.
Do not approve any tag.
