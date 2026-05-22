# Phase 12-RC1-WF-FIX-1 — RESUME (prior run crashed mid-stream)

You are resuming a previously-crashed implementer run for
`dev/plans/prompts/12-RC1-WF-FIX-1.md`. Read that file IN FULL for
context (Sub-1, Sub-2, Sub-3, Blockers, Output spec, Required commands).

## State at resume

You are in worktree `/tmp/fdb-12-RC1-WF-FIX-1-20260518T031153Z` on
branch `phase-12-RC1-WF-FIX-1-20260518T031153Z`. Baseline SHA `e66173a`.

**Already applied (uncommitted, in working tree):**

- Sub-1: `.github/workflows/release.yml` line 120 — matrix label
  `win32-x64` → `win32-x64-msvc`. Verify with `git diff`. Do not
  re-apply; just keep it.

**Not yet done — your job:**

1. Sub-1 step 3 — verify other matrix entries match `binding.ts`
   TRIPLES list (linux-x64-gnu, darwin-x64, darwin-arm64,
   win32-x64-msvc). Confirm in `src/ts/src/binding.ts:21-32` and
   `src/ts/package.json` `napi.triples`. If anything misaligned,
   surface as blocker; otherwise proceed.
2. Sub-1 step 4 — confirm aarch64-linux omission is intentional
   for 0.6.0-rc.1 (recommendation (a) from the parent prompt).
   Document in output JSON.
3. Sub-2 — T1-T7 publish-rust steps: switch dry-run branch from
   `cargo publish --dry-run -p <crate>` to
   `cargo package --allow-dirty --no-verify -p <crate>`. Also
   update the `dry_run` input description on `release.yml:10`.
4. Sub-3 — add a small regression assertion (preferred:
   `scripts/tests/test_actionlint_fixture.sh`) that greps
   release.yml for canonical labels and asserts each
   `publish-rust-t*` dry-run branch uses `cargo package`, not
   `cargo publish`.
5. Run the Required commands block from the parent prompt:
   - `actionlint .github/workflows/release.yml` (clean).
   - `cargo package --allow-dirty --no-verify -p <crate>` for each
     of: `fathomdb-embedder-api`, `fathomdb-schema`,
     `fathomdb-query`, `fathomdb-engine`, `fathomdb-embedder`,
     `fathomdb`, `fathomdb-cli`. Each must succeed. Note: this
     run produces tarballs in `target/package/` — do NOT commit
     them (gitignored under `target/`).
   - `bash scripts/tests/test_verify_release_gates.sh`.
   - `bash scripts/tests/test_actionlint_fixture.sh`.
   - `bash scripts/agent-verify.sh`.
     Flake list (rerun once before declaring red): `ac_029`, `ac_017`,
     `t_safe_export_engine_error_exits_export_failure_66`,
     `t_058_recover_truncate_wal_with_accept_data_loss_succeeds`,
     `t_040a_dump_row_counts_cli_emits_counts_array`.
6. Blocker check from parent prompt §Blockers: run
   `cargo publish --dry-run -p fathomdb-embedder-api` (no internal
   deps so it works on dry-run) and compare its output / failure
   modes to `cargo package --allow-dirty --no-verify -p
fathomdb-embedder-api`. If `cargo package` skips a real
   manifest gate that `publish --dry-run` catches (missing
   description, license, etc.), surface in output JSON
   `blockers_encountered`. Otherwise note "equivalent for our
   manifests" and proceed.
7. Single commit: `fix(release): napi win32-x64-msvc label + cargo
dry-run cascade`. Include both the Sub-1 + Sub-2 + Sub-3
   changes in one commit. Use HEREDOC for message body.
8. Write `dev/plans/runs/12-RC1-WF-FIX-1-output.json` per the
   schema in the parent prompt.

## Hard constraints (unchanged from parent)

- Do NOT push to any remote.
- Do NOT trigger workflow_dispatch.
- Do NOT push tags.
- Do NOT spawn agents (`Task`/`Agent` disallowed by harness).
- Net LoC ~10-50.
- Stay in this worktree. Do not switch branches.

Stop after output.json is written.
