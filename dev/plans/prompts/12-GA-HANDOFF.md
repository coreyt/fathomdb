# Hand-off — 0.6.0 GA in flight (Phase 12, post-tag)

Picking up an in-progress 0.6.0 GA release. rc.4 published cleanly
end-to-end yesterday (run `26105261444` all green incl.
github-release). Today: cut 0.6.0 GA, hit a gate bug, fix landed,
tag re-cut. Workflow run `26240769192` in flight at hand-off time.

Your job: confirm the GA workflow lands green end-to-end, log
decisions for next-session continuity, then proceed to post-GA
follow-ups (Axis-E first-class independence enablement, deferred-
slice ledger update, dependabot security advisories review).

## Orientation

You are the **orchestrator**. Main thread. Always. Per
`feedback_orchestrator_thread.md` + `dev/design/orchestration.md`
§ 1.

- Do NOT spawn a separate orchestrator subagent. Plan + verify
  here.
- Delegate code changes to implementers per
  `feedback_orchestrate_releases.md`. Spawn via the bash
  invocation in `dev/design/orchestration.md` § 2 (NOT the Agent
  tool — it lacks the `--disallowedTools Task Agent` guard).
- Delegate diff review to `code-reviewer` subagent before
  promotion. Independent read. Spawn via the Agent tool with
  `subagent_type=code-reviewer`.
- Cherry-pick implementer commits onto `0.6.0-rewrite` only after
  reviewer PASS (or explicit orchestrator override). Order
  matters: cherry-pick BEFORE reviewer where mainline state is
  needed for the review.

## Required reading (load before any action)

- `MEMORY.md` — load every entry. Especially:
  - `feedback_orchestrate_releases.md`
  - `feedback_orchestrator_thread.md`
  - `feedback_release_verification.md`
  - `feedback_workflow_validation.md`
  - `feedback_reliability_principles.md`
  - `feedback_file_deletion.md`
  - `feedback_tdd.md`

- `dev/design/orchestration.md` — § 2 (implementer spawn),
  § 3 (PREAMBLE), § 4 (cherry-pick), § 7 (fix-N loop).
- `dev/design/release.md` — § Version axes (axis-W vs axis-E),
  § Tiered publish order, § RC1 bootstrap publish.
- `dev/plans/0.6.0-implementation.md` § Phase 12 + § Merge-to-main
  strategy (line ~1085-1115) — confirms 12-GA is the merge slice
  and main now carries the GA commits.
- `dev/progress/0.6.0.md` — session log; most-recent entry on top.
- `CHANGELOG.md` `## 0.6.0` — single GA block (rc.1..rc.4 narrative
  collapsed).
- `dev/plans/prompts/12-HANDOFF.md` — prior session's pre-GA
  hand-off; still useful context for the WF-FIX-3 / rc.4 arc.
- `dev/plans/prompts/12-GA-bump.md` — the GA bump prompt that
  drove this session's implementer; check the "## Hard
  constraints" and "Tasks" sections for the substrate that
  shipped.
- `dev/plans/runs/12-GA-bump-output.json` and
  `dev/plans/runs/12-GA-bump-review-*.md` — implementer closure
  artifact + codex-equivalent code-reviewer verdict
  (PASS_WITH_NOTES; both nits fixed inline).

## Current state (snapshot)

### Git

- Branch state at hand-off: `0.6.0-rewrite` = `main` = `v0.6.0`
  = `edf00f7` on origin (all three pointers at the same SHA).
- Local working tree at the same commit; no uncommitted changes
  expected. `git status --short` to confirm.
- Last commits on the GA line, most recent first:

  ```text
  edf00f7 docs: prettier-fixup on 12-WF-FIX-3 prompt
  082321e fix(release): point verify-release gate at refs/remotes/origin/main
  80ffa6b docs(release): prettier-fixup on 12-GA-bump prompt
  e02d50a docs(release): GA fixups + code-reviewer verdict (PASS_WITH_NOTES)
  bf5c3b3 chore(release): bump to 0.6.0 GA
  f985538 chore(release): bump to 0.6.0-rc.4
  ```

### Tags

| Tag           | SHA                                   | Status                                                                   |
| ------------- | ------------------------------------- | ------------------------------------------------------------------------ |
| `v0.6.0`      | `edf00f7`                             | Pushed; workflow `26240769192` in flight at hand-off.                    |
| `v0.6.0-rc.4` | `f985538`                             | Pushed; run `26105261444` green end-to-end (rc.4 final smoke baseline).  |
| `v0.6.0-rc.3` | `06dc42e`                             | Pushed; run `26065215220` failed at post-publish gates (drove WF-FIX-3). |
| `v0.6.0-rc.2` | `2ea3122`                             | Pushed; failed at regex defect (drove `70e6487`).                        |
| `v0.6.0-rc.1` | (none — bootstrap script, no git tag) | Operator-run bootstrap; cargo-only.                                      |

### Workflow run `26240769192`

- Triggered by `v0.6.0` tag push at edf00f7.
- Status at hand-off: in_progress (~18s into verify-release step).
- Watch: `gh run watch 26240769192 --exit-status`.
- ETA: ~17min based on rc.4 timing (`26105261444` = 17m49s).
- Expected jobs (25): verify-release; 11 builds (rust + napi×4 +
  python×5); all-builds-passed; 7 cargo publish tiers (T1..T7);
  publish-pypi; publish-npm; 3 post-publish-smoke jobs (pypi-wheel,
  npm-package, crates-cli); co-tagging-assert; github-release.

### Registry state (just before tag push)

| Registry                                                     | Versions live                                                                      | GA expected                              |
| ------------------------------------------------------------ | ---------------------------------------------------------------------------------- | ---------------------------------------- |
| crates.io (7 axis-W crates + axis-E `fathomdb-embedder-api`) | `0.6.0-rc.{1,2,3,4}`                                                               | adds `0.6.0`                             |
| PyPI (`fathomdb`)                                            | `0.6.0rc{2,3,4}` (rc.1 NOT on PyPI — bootstrap was cargo-only)                     | adds `0.6.0`                             |
| npm (`fathomdb`, tagged `next`)                              | `0.6.0-rc.{2,3,4}`                                                                 | adds `0.6.0` tagged `latest`             |
| GitHub Releases                                              | `v0.6.0-rc.4` (the first one that landed; rc.2/rc.3 never created release entries) | adds `v0.6.0` with `isPrerelease: false` |

### Session decisions logged via wake

- `d-001` — Axis-E (`fathomdb-embedder-api`) at 0.6.0 GA = `0.6.0`
  (matches axis-W). Mechanics support independent semver
  (`workspace.dependencies` + per-crate `version` field in
  `src/rust/crates/fathomdb-embedder-api/Cargo.toml`); pin them
  together at the GA anchor so `0.6.0` is the historical
  reference point. Future axis-W bumps no longer auto-bump
  axis-E — axis-E moves only on trait surface change.

### What landed this session (post-rc.4)

- `bf5c3b3` — GA version bump (axis-W + axis-E both rc.4 → 0.6.0;
  CHANGELOG collapse; `prerelease:` field on github-release step).
- `e02d50a` — doc fixups from reviewer's nits + verdict promotion.
- `80ffa6b` — prettier-fixup on the GA bump prompt (pre-push
  lint-md-format).
- `082321e` — **release-gate bug fix**: `verify-release-gates.sh`
  checked `refs/heads/main` but `actions/checkout` doesn't create
  a local main branch when checking out a tag, only
  `refs/remotes/origin/*`. RC path skipped this check (hyphen
  version short-circuits); GA was first exposure. Fix: set
  `RELEASE_GATES_HEAD_REF: refs/remotes/origin/main` env on the
  verify-release job. Single-line workflow change; script's
  existing env override hook (used by the test harness) does the
  work. No script edit needed.
- `edf00f7` — prettier-fixup on the WF-FIX-3 prompt (pre-push
  lint-md-format; same prettier drift class as 80ffa6b).

### What caught me (lessons for next-session orchestrator)

- The `12-GA-bump.md` prompt I (prior session) wrote did NOT
  spell out the merge-to-main step before tagging. The plan doc
  (`dev/plans/0.6.0-implementation.md:1115`) explicitly calls for
  "Merge `0.6.0-rewrite` → `main`; tag `v0.6.0` on `main`." I
  treated GA like just another RC bump. Result: tag push tripped
  the HEAD-on-main gate, then exposed the gate's
  `refs/heads/main` bug behind it. Two cycles burned.

- For GA-class events the pre-tag checklist must include
  "main FFed to HEAD?" in addition to the rc bump checklist.

- The `agent-verify.sh` gate uses `run_capped` and is silent on
  pass. I tail-checked the security step output and assumed
  test step ran. It did (exit=0 means all four steps), but the
  silence is easy to misread. Look at `exit=N` not the final
  log line.

- Pre-push lint-md-format keeps tripping on the per-phase prompt
  files. Prettier auto-reformats nested-list indentation in ways
  that differ from my hand-written shape. The standard remedy
  has been `npx prettier --write` then commit the result. Doing
  prettier-write as part of writing the prompt (before any
  commit) would avoid the rebound. Worth adding to the prompt-
  authoring checklist.

## If the GA workflow goes green

(Expected; rc.4 had identical post-publish job set and went
green end-to-end yesterday.)

1. Verify all 25 jobs `success`:

   ```bash
   gh api repos/coreyt/fathomdb/actions/runs/26240769192/jobs \
     --jq '.jobs[] | "\(.conclusion)\t\(.name)"'
   ```

2. Verify GitHub release attributes:

   ```bash
   gh release view v0.6.0 --json name,tagName,isPrerelease,assets \
     --jq '{name, tagName, isPrerelease, asset_count: (.assets | length)}'
   ```

   Expect `isPrerelease: false` (this RC's `prerelease:` field
   fix lands at GA).

3. Update `dev/progress/0.6.0.md` — newest-on-top entry
   summarizing the GA cut, the gate bug fix arc, and final
   workflow run number.

4. Mark Phase 12-GA CLOSED in
   `dev/plans/0.6.0-implementation.md` § "Immediate Next Slice".
   GA is the merge slice (line ~929) — the merge happened today.

5. Open follow-up tasks (none release-critical; all 0.6.1+
   shape):
   - **0.6.1 axis-E independence demo.** Per `d-001`, axis-E is
     now structurally independent. First post-GA bump should
     exercise that — even if axis-W moves to 0.6.1, axis-E
     stays at 0.6.0 unless the trait surface changes. This is
     more a discipline check than a feature.
   - **TS SDK Python-parity.** Surfaced in
     `CHANGELOG.md ### Deferred`; the GA fixup commit
     (e02d50a) rephrased the entry to past-tense ("did NOT
     land at 0.6.0 GA"). Ticket / next-slice planning belongs
     to 0.6.1.
   - **Dependabot advisories.** Push output noted "GitHub
     found 4 vulnerabilities on coreyt/fathomdb's default
     branch (2 moderate, 2 low)." main is now the default
     branch and carries the GA commits. Triage the 4 advisories
     before any 0.6.1 work; some may close via the dependabot
     PRs already filed (visible in `git fetch` output).

6. Optional cleanup: the worktree-cleanup task list at the
   bottom of this hand-off (none outstanding from this session).

## If the GA workflow fails

Stop. Read the failure mode from `gh run view <run-id>
--log-failed`. Do NOT cut rc.5 or a v0.6.0.1 patch reflexively.
Common failure modes to expect:

- **Post-publish smoke flake.** rc.3 had three real defects;
  rc.4 had none. If a single smoke job fails on an environment
  blip (network, runner load), rerun the failed jobs only:
  `gh run rerun --failed 26240769192`.
- **co-tagging-assert.** If any of the 3 sibling crates fails
  to land on crates.io in time (index propagation), rerun.
- **github-release.** softprops/action-gh-release publishes
  the GitHub Release entry; `prerelease:` field is new on this
  RC. Verify it evaluates correctly for `v0.6.0` (→ false).
- **A new bug not caught by rc.4.** This is the real worry.
  HITL: stop, capture failure mode in
  `dev/progress/0.6.0.md`, plan the fix as a WF-FIX-5 slice.
  Do not move forward without a clean run.

If the workflow needs a re-trigger AFTER a commit fix:

- The same destructive-on-tag pattern applies. `v0.6.0` already
  has registry side effects (if any tier ran before the failure)
  — moving the tag does NOT unpublish crates / wheels / npm
  packages. The tag is a pointer, not a release container. If
  T1 succeeded but T4 failed, you can NOT yank the rc.4
  precedent — escalate to HITL before any tag move.

## Permission boundary (operator vs orchestrator)

The harness blocks tag pushes and pushes to `0.6.0-rewrite` /
`main` without explicit per-action HITL. **Operator runs** via
`! <command>` prefix:

- `! git push origin <branch>` — branch pushes.
- `! git push origin <branch>:<other-branch>` — branch-to-branch.
- `! git tag v0.6.X && git push origin v0.6.X` — tag pushes.
- `! gh run rerun <run-id>` and `! gh run rerun --failed
<run-id>` — workflow reruns.

**Orchestrator runs** without HITL:

- Local edits, commits (NEVER `--no-verify` unless the operator
  explicitly OKs it), cherry-picks, worktree mgmt, implementer
  spawns, code-reviewer spawns, `gh run watch` / `gh api`,
  prettier fixups, `agent-verify`.

`CARGO_REGISTRY_TOKEN` is in `~/.cargo/credentials.toml`.

## Known flakes (rerun once before declaring red)

- `t_028a_excise_source_cli_returns_excise_report`
- `t_042_trace_cli_enumerates_canonical_rows_for_source`
- `t_058_recover_truncate_wal_with_accept_data_loss_succeeds`
- `t_040a_dump_row_counts_cli_emits_counts_array`
- `t_040a_verify_embedder_cli_emits_match_status_on_matching_input`
- `t_safe_export_engine_error_exits_export_failure_66`
- `ac_017_vector_projection_freshness_p99_le_five_seconds`
- `ac_029_canonical_writes_complete_under_projection_stall`

If pre-push hook fails on one of these, retry once. If
persistent, HITL for `--no-verify` (precedent: rc.4 push, GA
push attempt 2).

## Hard constraints (entire session)

- Do NOT cut a new tag (`v0.6.0.1`, `v0.6.0-rc.5`, etc.) without
  HITL.
- Do NOT modify a tag that's been pushed AND triggered a
  successful publish tier. If you must move a tag (gate failure
  before any tier ran), HITL the move.
- Do NOT bump axis-E beyond `0.6.0` without explicit HITL — per
  `d-001` axis-E stays at 0.6.0 through the next axis-W bump
  unless the trait surface changes.
- Do NOT modify `main` or `0.6.0-rewrite` without operator-run
  HITL push.
- Do NOT bypass pre-push hooks except for known flakes on tag
  pushes, and only via operator `!` prefix.
- Spawn implementers from a freshly-created git worktree per
  `dev/design/orchestration.md` § 2. Clean up worktree + phase
  branch after promote.
- Code-reviewer subagent pass before any cherry-pick to mainline.
- Smart Events: log decisions / constraints / rejections via
  `wake log decision`, `wake log constraint`, `wake log
rejection` if you make a judgment call worth preserving.

## File layout reminders

- Implementer prompts: `dev/plans/prompts/<PHASE>.md`
- Implementer run logs: `dev/plans/runs/<PHASE>-<TS>.log`
  (gitignored / markdownlint-ignored).
- Implementer output JSON: `dev/plans/runs/<PHASE>-output.json`
  (orchestrator copies out of worktree before worktree removal).
- Reviewer verdicts:
  `dev/plans/runs/<PHASE>-review-<TS>.md`.
- CHANGELOG entry per release: above the prior `## <version>`
  section. (The `## 0.6.0` block lives at the top of
  CHANGELOG.md right now; future 0.6.1 entry goes above it.)

## When in doubt

- Don't make scope decisions silently. Surface to user with a
  short option matrix (A/B/C with cost).
- Don't move tags or burn release slots without HITL.
- Don't claim "GA done" until ALL 25 jobs are green AND the
  GitHub release entry exists with `isPrerelease: false`.

End of hand-off. Read `MEMORY.md` and the file references above
before any action. The first thing to do is check the workflow
run status:

```bash
gh run view 26240769192 --json status,conclusion,url \
  --jq '{status, conclusion, url}'
```
