# Phase 12-RC1 — Tag `v0.6.0-rc.1` (first irreversible publish)

**Type:** procedural slice with one implementer sub-scope. Most of
this is orchestrator + HITL coordination, not implementer code.

**Scope:** Cut the first 0.6.0 release candidate. Version-bump
manifests to `0.6.0-rc.1`, run local + CI dry-run rehearsal, get
HITL approval, push the real tag, monitor `release.yml`, record
per-tier results.

**Owner:** orchestrator drives procedure; HITL approves real tag
push.

**Exit criterion:** `v0.6.0-rc.1` tag pushed; all 9 publishes
green on registries; post-publish smoke green; co-tagging assert
green; github-release published; closure recorded in
`dev/progress/0.6.0.md` + `STATUS-phase12.md`.

**Inflection point:** this is the **first irreversible action** in
Phase 12. Once `0.6.0-rc.1` lands on crates.io / PyPI / npm, the
version slot is permanently consumed. A partial publish forces a
re-cut as `rc.2` (not a re-push of `rc.1`).

## Sub-1: Implementer-spawned prep work

Single implementer sub-scope. Worktree-isolated commit chain. ALL
local; no tag push, no workflow_dispatch from inside the implementer.

```bash
PHASE=12-RC1-prep
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plans/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-${PHASE}-${TS}
git -C /home/coreyt/projects/fathomdb worktree add "$WT" -b "phase-${PHASE}-${TS}" 0.6.0-rewrite
PREAMBLE=$(cat <<'EOF'
YOU ARE THE IMPLEMENTER. Not the orchestrator. Do the work in this
worktree. Do NOT re-spawn yourself. Do NOT spawn other agents. Do
NOT push any git tag. Do NOT trigger workflow_dispatch. Do NOT
push to any remote. Use --disallowedTools Task Agent as a hard
guard. Write code, run tests, commit. Done.
EOF
)
( cd "$WT" && \
  ( echo "$PREAMBLE"; cat /home/coreyt/projects/fathomdb/dev/plans/prompts/12-RC1-tag-rc1.md ) \
  | claude -p --model claude-opus-4-7 --effort medium \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --disallowedTools Task Agent \
      --permission-mode bypassPermissions \
      --output-format stream-json --include-partial-messages --verbose \
  > "$LOG" 2>&1 )
```

### Required reading

- `AGENTS.md` § 1, § 3, § 7.
- `MEMORY.md`, especially:
  - `feedback_release_verification` — registry-installed wheel is
    the release gate.
  - `feedback_reliability_principles` — no soak; delete-before-add.
- `dev/design/orchestration.md` § 2, § 3, § 8.
- `dev/design/release.md` — two-axis versioning, 8-tier publish
  order.
- `dev/plans/0.6.0-implementation.md` § Path to Client-Ready
  (0.6.0 GA), 12-RC1 row.
- `.github/workflows/release.yml` — the workflow that fires on
  tag.
- `scripts/set-version.sh` — version-bump tool. `--workspace`
  bumps Axis W (Cargo workspace + python + ts); accepts arbitrary
  version strings (no semver validation, so `0.6.0-rc.1` works).
- `scripts/verify-release-gates.sh` — release-gate script that
  runs at tag push.
- Current versions: all at `0.6.0` (Axis W) + `0.6.0` (Axis E).

### Implementer sub-scope tasks

1. **Bump Axis W to `0.6.0-rc.1`** via
   `bash scripts/set-version.sh --workspace 0.6.0-rc.1`. This
   updates `Cargo.toml [workspace.package].version`,
   `[workspace.dependencies]` sibling pins, `src/python/pyproject.toml`,
   `src/ts/package.json`. **Do NOT bump Axis E** — the
   `fathomdb-embedder-api` trait surface hasn't changed since
   `0.6.0` lock; keep its independent version at `0.6.0`.
2. **Add `## [0.6.0-rc.1]` section to `CHANGELOG.md`** above the
   `[Unreleased]` section. Carry forward the deferred-items
   disclosures from `docs/release-notes/0.6.0.md` (perf gates,
   logical-id verbs, open-report, TS-not-parity, no-0.5.x). Format:

   ```markdown
   ## [0.6.0-rc.1] - 2026-MM-DD

   First release candidate of 0.6.0. Engine + bindings + release-
   engineering substrate landed across Phases 5-12.

   ### Added
   - [enumerate landed surfaces]

   ### Deferred
   - [enumerate deferrals with closure targets per release notes]

   ### Removed
   (none — 0.6.0 is a rewrite; no 0.5.x→0.6.0 deprecation shims)
   ```

3. **Run local pre-flight gates:**
   - `bash scripts/set-version.sh --check-files` — confirm Axis W
     lockstep at `0.6.0-rc.1` and Axis E at `0.6.0`.
   - `bash scripts/verify-release-gates.sh` — confirm
     all gates pass (note: HEAD-on-main check will FAIL locally
     because we're on `0.6.0-rewrite`, not `main` — surface that
     as an expected pre-flight failure that resolves at GA when
     `0.6.0-rewrite` merges to `main`; for RC1 the tag is pushed
     from `0.6.0-rewrite`, which means verify-release-gates'
     main-reach check needs an env override OR the script handles
     dispatch-vs-push correctly).
   - `actionlint .github/workflows/release.yml` — confirm
     workflow YAML still parses clean.
   - `bash scripts/agent-verify.sh` — full local gate (lint +
     typecheck + STRICT=1 agent-security + test).
4. **Surface blockers** if any gate fails. Common ones:
   - **HEAD-on-main check fails**: `0.6.0-rewrite` not merged to
     main. Surface as blocker requiring orchestrator decision:
     (a) loosen verify-release-gates to allow tags from
     `0.6.0-rewrite` for RC cycle (until GA merge), OR (b)
     merge `0.6.0-rewrite` → `main` BEFORE tagging (which means
     GA-tier merge happens at RC1, not at 12-GA).
   - **agent-security strace blocker**: pre-existing. Note in
     output but don't block — bootstrap.sh installs strace on CI;
     local-host blocker is documented.
   - **CHANGELOG section parse failure**: tighten the section
     heading per `verify-release-gates.sh` expectation (it
     greps for `## 0.6.0-rc.1` or similar — verify exact regex
     and match).
5. Commit version-bump + CHANGELOG as ONE commit:
   `release(0.6.0-rc.1): bump Axis W to 0.6.0-rc.1; CHANGELOG`.
6. **Do NOT push the tag** from the implementer. Tag-push is a
   HITL-approved orchestrator action.

### Implementer output

After commit + gates green (or blockers surfaced), write
`dev/plans/runs/12-RC1-prep-output.json`:

```json
{
  "phase": "12-RC1-prep",
  "baseline_sha": "<HEAD of 0.6.0-rewrite at spawn>",
  "branch": "phase-12-RC1-prep-<ts>",
  "head_sha": "<HEAD after version-bump commit>",
  "commits": ["<sha>: release(0.6.0-rc.1): bump Axis W; CHANGELOG"],
  "version_bumps": {
    "axis_w_from": "0.6.0",
    "axis_w_to": "0.6.0-rc.1",
    "axis_e_unchanged": "0.6.0"
  },
  "set_version_check_files_result": "pass | fail (+ diagnostic)",
  "verify_release_gates_result": "pass | fail (+ which gate)",
  "actionlint_result": "pass | fail",
  "agent_verify_result": "pass | fail (+ tail — note strace blocker if present)",
  "blockers_encountered": [{...}],
  "next_step_for_orchestrator": "promote to 0.6.0-rewrite; orchestrator runs dispatch dry-run + HITL tag-push approval per 12-RC1 procedure"
}
```

Then stop.

## Sub-2: Orchestrator dry-run rehearsal (no implementer)

After implementer prep returns + cherry-pick lands on
`0.6.0-rewrite`:

1. Orchestrator triggers `release.yml` via `workflow_dispatch`
   with `dry_run: true`:

   ```bash
   gh workflow run release.yml -f dry_run=true --ref 0.6.0-rewrite
   ```

   (Requires `gh` CLI + repo write permission.)
2. Watch the run:

   ```bash
   gh run watch
   # or
   gh run list --workflow=release.yml --limit 1
   ```

3. Expected behavior on `dry_run: true`:
   - `verify-release` runs gates (NOT tag-check on dispatch; emits
     dry_run=false WARN if accidentally set false).
   - `build-python` + `build-napi` + `build-rust` matrix all
     build cleanly (real CI build signal).
   - `all-builds-passed` gates pass.
   - Each `publish-rust-tN-*` runs `cargo publish --dry-run`.
   - `publish-pypi` job-skipped entirely (no test.pypi burn).
   - `publish-npm` runs `npm publish --dry-run` (genuinely local).
   - `post-publish-smoke` job-skipped entirely.
   - `co-tagging-assert` job-skipped entirely (no publishes to
     query).
   - `github-release` job-skipped entirely.
4. Record dry-run results:
   - All build matrix legs green?
   - Each `cargo publish --dry-run` per crate green?
   - `npm publish --dry-run` green?
   - actionlint clean?
5. **If dry-run RED**: surface blockers; do NOT proceed to real
   tag. Iterate on prep (Sub-1 re-spawn or orchestrator-direct
   fix) until dry-run green.

## Sub-3: HITL approval gate

Orchestrator presents dry-run results to user. User signs off OR
requests adjustments. **No real tag push without explicit user
"approve RC1" signal.**

User signoff checklist:

- [ ] Dry-run release.yml all jobs green
- [ ] All `cargo publish --dry-run` per crate clean
- [ ] `npm publish --dry-run` clean
- [ ] actionlint workflow clean
- [ ] CHANGELOG `## [0.6.0-rc.1]` section accurate
- [ ] Deferred-items disclosures intact
- [ ] No pending changes that should land before RC1
- [ ] **Approve push of `v0.6.0-rc.1` tag**

Recorded as HITL decision in `dev/progress/0.6.0.md`.

## Sub-4: Real tag push + monitor

After user approval, orchestrator pushes the real tag:

```bash
# On 0.6.0-rewrite at the version-bump commit:
git tag -a v0.6.0-rc.1 -m "fathomdb 0.6.0 release candidate 1"
git push origin v0.6.0-rc.1
```

**This fires `release.yml` for real.** No way back without re-cut.

Monitor:

```bash
gh run watch --workflow=release.yml
```

Per-tier expected sequence:

1. `verify-release` — green within ~1 min.
2. Build matrix (`build-python` x5 + `build-napi` x4 + `build-rust`)
   — green within ~10-15 min.
3. `all-builds-passed` — green immediately.
4. **`publish-rust-t1-embedder-api`** — first real publish. If
   this is green, Axis E is permanently consumed at `0.6.0`.
5. 60s index-propagation sleep.
6. T2 schema → T3 query → T4 engine → T5 embedder → T6 facade →
   T7 cli; 60s sleep between each.
7. T8 parallel: `publish-pypi` + `publish-npm` after T4 publishes.
8. `post-publish-smoke` (crates-cli + pypi-wheel + npm-package).
9. `co-tagging-assert` — verifies sibling triple
   `(fathomdb, fathomdb-embedder, fathomdb-embedder-api)` at
   `0.6.0-rc.1` (Axis W) + `0.6.0` (Axis E).
10. `github-release` — final job; only fires if all above green.

Record per-tier results in `dev/plans/runs/12-RC1-actual-tag-push-output.json`.

## Sub-5: Failure recovery (if anything red)

**Critical:** registry publishes are irreversible. Once
`crates.io/fathomdb@0.6.0-rc.1` exists, that version slot cannot
be re-used.

Failure scenarios:

| Failure stage | Recovery |
|---------------|----------|
| `verify-release` fail | No real publish yet; fix locally + re-push tag is NOT allowed (tag is published). Use `rc.2`. |
| Build matrix fail | Same — no publish yet; bump to `rc.2`. |
| T1 publish fail (network glitch) | Manually retry `cargo publish -p fathomdb-embedder-api --token $TOKEN` from a local checkout; resume from T2 by re-running workflow with `--rerun-failed`. If unrecoverable, bump to `rc.2`. |
| T2-T7 fail mid-tier | Crates already published at T1..T(N-1) cannot be re-published. Bump to `rc.2`; the partial-published crates stay at `0.6.0-rc.1` forever. New RC must bump ALL crates to `0.6.0-rc.2` for consistency, even though some were green at rc.1. |
| T8 PyPI/npm fail after T7 green | PyPI version slot consumed if `publish-pypi` partial. npm easier (can usually re-trigger). Bump to `rc.2`. |
| `post-publish-smoke` fail | Crates already published. Investigate smoke failure; if smoke caught a real client-breaking bug, hotfix → `rc.2`. If smoke flake, document + retry the smoke step only. |
| `co-tagging-assert` fail | Sibling triple version mismatch. Indicates set-version drift. Should have been caught at `verify-release`; if it slipped, hotfix + `rc.2`. |
| `github-release` fail | All publishes already done; just need to manually create the github release via `gh release create v0.6.0-rc.1` with notes. Not a re-cut trigger. |

Per `feedback_reliability_principles` no-punt rule: surface failures
explicitly, don't paper over.

## Sub-6: Closure

After all `release.yml` jobs green:

1. Record per-tier results in
   `dev/plans/runs/12-RC1-actual-tag-push-output.json`.
2. Append HITL decision + execution log to
   `dev/progress/0.6.0.md`.
3. Update `dev/plans/runs/STATUS-phase12.md` 12-RC1 row → ✅ CLOSED.
4. Update `dev/plans/0.6.0-implementation.md` "Immediate Next
   Slice" → advance to 12-V (independent verification on fresh
   non-CI host).
5. Tag `0.6.0-rewrite` HEAD as `post-RC1` for the next rollback
   fence.
6. Commit closure single docs commit.

## Non-blocker reminders

- Tag `pre-RC1` (landed 2026-05-17) is the rollback fence —
  available if everything goes catastrophically wrong + we need
  to abandon the rc.1 cycle and start over from clean state.
- Worktrees from prior Phase 12 slices already cleaned per
  orchestration § 11.
- Memory `feedback_release_verification`: registry-installed
  wheel is the gate. 12-RC1 publishes; 12-V validates by
  installing-from-registry on a fresh host that did NOT
  participate in the build.

## Critical do-NOT list

- Do NOT push `v0.6.0` (GA tag) at this slice. That's 12-GA.
- Do NOT push `v0.6.0-rc.1` from the implementer subagent. HITL
  gate at orchestrator only.
- Do NOT skip the dry-run rehearsal. A dry-run failure caught
  pre-publish saves $/embarrassment vs a real-publish failure
  caught post-publish.
- Do NOT amend `0.6.0-rewrite` history after the tag is pushed
  (no `git push --force`, no rebase past the tagged commit).
- Do NOT delete or overwrite the `pre-RC1` tag.
