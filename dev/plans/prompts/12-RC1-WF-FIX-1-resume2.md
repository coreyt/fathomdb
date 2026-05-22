# Phase 12-RC1-WF-FIX-1 — RESUME 2 (sentinel-publish pivot)

You are resuming the WF-FIX-1 implementer. Prior commit `aa587d2`
landed Sub-1 (napi label) + Sub-2 (cargo package swap) + Sub-3
(test). The Sub-2 swap surfaced a real blocker — `cargo package
--allow-dirty --no-verify` fails identically to `cargo publish
--dry-run` for T4-T7 because both resolve sibling deps against
crates.io.

Orchestrator + user decided: bootstrap-publish 0.6.0-rc.1 of
T1-T7 so future dispatches (rc.2 / GA) have sibling registry
state to resolve against. Burns the rc.1 slot deliberately.

## Required reading

- `dev/plans/prompts/12-RC1-WF-FIX-1.md` — parent prompt.
- `dev/plans/prompts/12-RC1-WF-FIX-1-resume.md` — resume 1.
- `dev/design/release.md` § Version axes + § Tiered publish order.
- Current worktree HEAD `aa587d2` (Sub-1 + Sub-2 + Sub-3 already
  landed).
- `Cargo.toml` root [workspace.dependencies] block.
- `src/rust/crates/fathomdb-embedder-api/Cargo.toml` (axis-E, opted
  out of workspace.package.version).
- `scripts/tests/test_actionlint_fixture.sh` lines 30-62 (the
  forbid-`cargo publish --dry-run` assertion you must update).

## State at resume

- Worktree: `/tmp/fdb-12-RC1-WF-FIX-1-20260518T031153Z`.
- Branch: `phase-12-RC1-WF-FIX-1-20260518T031153Z`.
- HEAD: `aa587d2` (clean — no uncommitted edits).

## Sub-A — Bump fathomdb-embedder-api to axis-W lockstep for RC

Rationale: axis-E was scheduled to debut at `0.6.0`. User reserved
`0.6.0` GA slot. Bump embedder-api to `0.6.0-rc.1` (axis-W
lockstep for RC). After GA, axis-E independence resumes —
embedder-api may bump independently when its trait surface
changes.

1. Edit `src/rust/crates/fathomdb-embedder-api/Cargo.toml`:
   change `version = "0.6.0"` → `version = "0.6.0-rc.1"`. Leave
   the comment explaining axis-E opt-out — still accurate, just
   the value changes.
2. Edit root `Cargo.toml` `[workspace.dependencies]` line for
   `fathomdb-embedder-api`: change `version = "0.6.0"` →
   `version = "0.6.0-rc.1"`.
3. Update the `[workspace.package]` comment block in root
   `Cargo.toml` (currently lines 16-21) — note that axis-E is on
   axis-W lockstep through RC1, regains independence at/after GA.
4. Run `cargo check --workspace` to confirm version bump compiles.
   Run `cargo build --workspace` if check passes.

## Sub-B — Revert Sub-2's `cargo package` swap

Rationale: with rc.1 bootstrap-published, `cargo publish
--dry-run` will resolve siblings against registry rc.1 for any
future rc.2 / GA dispatch. The `cargo package` workaround is no
longer needed and conflicts with the bootstrap design.

1. Edit `.github/workflows/release.yml`: for each of
   `publish-rust-t1-embedder-api`, `publish-rust-t2-schema`,
   `publish-rust-t3-query`, `publish-rust-t4-engine`,
   `publish-rust-t5-embedder`, `publish-rust-t6-facade`,
   `publish-rust-t7-cli` — revert dry-run branch from `cargo
package --allow-dirty --no-verify -p <crate>` to `cargo publish
--dry-run -p <crate>`.
2. Revert the `dry_run` input description on `release.yml:10` to
   its original "cargo publish --dry-run" phrasing.
3. Run `actionlint .github/workflows/release.yml` — must stay
   clean.

## Sub-C — Update Sub-3 test assertion

The current Sub-3 assertion (in `scripts/tests/test_actionlint_fixture.sh`
lines ~37-62) requires `cargo package --allow-dirty --no-verify` in
T1-T7 and forbids `cargo publish --dry-run`. After Sub-B this is
backwards.

1. Edit `scripts/tests/test_actionlint_fixture.sh`: invert the
   T1-T7 assertion to require `cargo publish --dry-run -p` in
   each tier's dry-run branch and forbid `cargo package
--allow-dirty --no-verify`. Update the explanatory comment
   to reflect the bootstrap-publish design.
2. Keep the napi-label assertion block as-is — that's still
   valid.
3. Run `bash scripts/tests/test_actionlint_fixture.sh` — must
   pass.

## Sub-D — Bootstrap publish script

Create `scripts/release/publish-rc1-bootstrap.sh`. Operator-run,
takes `CARGO_REGISTRY_TOKEN` from env. Sequential publish of
T1-T7 at 0.6.0-rc.1 with index propagation sleeps. Idempotent
(skip if version already exists on crates.io).

Shape:

```bash
#!/usr/bin/env bash
# Bootstrap-publish 0.6.0-rc.1 of T1-T7 to crates.io so future
# workflow_dispatch dry-runs of rc.2 / GA resolve sibling deps
# against registry rc.1. One-time operation per the
# 12-RC1-WF-FIX-1 sentinel-publish design.
set -euo pipefail
: "${CARGO_REGISTRY_TOKEN:?CARGO_REGISTRY_TOKEN must be set}"
RC_VERSION="0.6.0-rc.1"
TIERS=(
  fathomdb-embedder-api
  fathomdb-schema
  fathomdb-query
  fathomdb-engine
  fathomdb-embedder
  fathomdb
  fathomdb-cli
)
for c in "${TIERS[@]}"; do
  # Idempotent: skip if already on crates.io at RC_VERSION.
  if cargo search "$c" --limit 1 | grep -qE "^${c} = \"${RC_VERSION}\""; then
    printf 'SKIP  %s %s already on crates.io\n' "$c" "$RC_VERSION"
    continue
  fi
  printf '==> publishing %s %s\n' "$c" "$RC_VERSION"
  cargo publish -p "$c" --token "${CARGO_REGISTRY_TOKEN}"
  printf '==> sleeping 60s for index propagation\n'
  sleep 60
done
printf 'DONE bootstrap publish %s\n' "$RC_VERSION"
```

Make it executable (`chmod +x`). Do NOT execute it. The script is
the deliverable; running it is an operator step outside this
slice.

## Sub-E — Document in release.md

Edit `dev/design/release.md`. Add a § "RC1 bootstrap publish"
(or wherever fits the existing structure) that explains:

- Sibling-crate dep resolution requires registry state.
- `0.6.0-rc.1` is the bootstrap-publish version that establishes
  registry presence for all 7 axis-W crates plus axis-E
  embedder-api.
- After bootstrap, real RC tag is `0.6.0-rc.2` (rc.1 slot
  consumed by bootstrap).
- All future axis-W dispatches (rc.2, rc.3, ..., GA) use `cargo
publish --dry-run` for the dry-run mode; siblings resolve
  against registry rc.1.
- Axis-E (fathomdb-embedder-api) is on axis-W lockstep for RC1
  bootstrap; independence resumes at/after axis-E's first
  post-GA bump.
- Bootstrap is operator-run via `scripts/release/publish-rc1-bootstrap.sh`.

Keep prose tight — 1-2 short paragraphs + a bullet list. No
exhaustive design doc, just enough so the design intent isn't
lost.

## Required commands

```bash
cd /tmp/fdb-12-RC1-WF-FIX-1-20260518T031153Z
cargo check --workspace
actionlint .github/workflows/release.yml
bash scripts/tests/test_actionlint_fixture.sh
bash scripts/tests/test_verify_release_gates.sh
bash scripts/agent-verify.sh
```

All must pass. Known flakes (rerun once): `ac_029`, `ac_017`,
`t_safe_export_engine_error_exits_export_failure_66`,
`t_058_recover_truncate_wal_with_accept_data_loss_succeeds`,
`t_040a_dump_row_counts_cli_emits_counts_array`.

## Hard constraints (unchanged)

- Do NOT push to any remote.
- Do NOT trigger workflow_dispatch.
- Do NOT push tags.
- Do NOT run `publish-rc1-bootstrap.sh` (the script is the
  deliverable; running it is operator step).
- Do NOT spawn agents (`Task`/`Agent` disallowed).
- Stay in this worktree. Do not switch branches.

## Commit

Single commit on top of `aa587d2`:

```text
fix(release): bootstrap-publish design for sibling-dep cascade

- bump fathomdb-embedder-api 0.6.0 -> 0.6.0-rc.1 (axis-W lockstep
  for RC; axis-E independence resumes post-GA)
- revert workflow dry-run swap (cargo publish --dry-run is the
  correct gate once rc.1 sibling state exists on crates.io)
- update Sub-3 test assertion to match
- add scripts/release/publish-rc1-bootstrap.sh (operator-run)
- doc: dev/design/release.md § RC1 bootstrap publish
```

## Output

After all commands pass, append to existing
`dev/plans/runs/12-RC1-WF-FIX-1-output.json` OR write a new
`dev/plans/runs/12-RC1-WF-FIX-1-resume2-output.json` with:

```json
{
  "phase": "12-RC1-WF-FIX-1-resume2",
  "parent_commit": "aa587d2",
  "head_sha": "<new HEAD>",
  "commits": ["<sha>: <subject>"],
  "subs_landed": ["A", "B", "C", "D", "E"],
  "bootstrap_script_path": "scripts/release/publish-rc1-bootstrap.sh",
  "embedder_api_version": "0.6.0 -> 0.6.0-rc.1",
  "workflow_dry_run_command": "cargo publish --dry-run (restored)",
  "release_md_section": "<name of section added/edited>",
  "commands_run": {
    "cargo_check_workspace": "pass | fail",
    "actionlint": "pass | fail",
    "test_actionlint_fixture": "pass | fail",
    "test_verify_release_gates": "pass | fail",
    "agent_verify": "pass | fail (+ tail)"
  },
  "blockers_encountered": [],
  "next_step_for_orchestrator": "cherry-pick onto 0.6.0-rewrite; operator runs publish-rc1-bootstrap.sh with CARGO_REGISTRY_TOKEN; subsequent RC tag is 0.6.0-rc.2"
}
```

Stop after output.json written. Do not push, do not publish, do
not dispatch.
