# Phase 12-B — benchmark-and-robustness.yml restoration

Phase 12 Wave 1 slice (parallel with 12-S). Restores the weekly-cron
benchmark + robustness workflow adapted to 0.6.0 topology. Per
`dev/plans/ci-deferred.md` § benchmark-and-robustness.yml — Phase 12.

Out of scope:

- 12-D durability harnesses (closed at `f2f21b5`).
- 12-S security fixtures (Wave 1 sibling slice).
- Any actual perf-evidence work (Pack 7 territory; this slice
  ships the workflow, not the experiments).

## Model + effort

Opus 4.7, intent: medium. Smaller scope than 12-S (workflow + 2-3
small scripts).

```bash
PHASE=12-B-benchmark-robustness-workflow
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plans/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-${PHASE}-${TS}
git -C /home/coreyt/projects/fathomdb worktree add "$WT" -b "phase-${PHASE}-${TS}" 0.6.0-rewrite
PREAMBLE=$(cat <<'EOF'
YOU ARE THE IMPLEMENTER. Not the orchestrator. Do the work in this
worktree. Do NOT re-spawn yourself. Do NOT spawn other agents. The
"## Model + effort" section in this prompt describes how YOU were
just launched (claude -p with the listed model/effort). Do NOT
re-run that block. Use --disallowedTools Task Agent as a hard
guard. Write code, run tests, commit. Done.
EOF
)
( cd "$WT" && \
  ( echo "$PREAMBLE"; cat /home/coreyt/projects/fathomdb/dev/plans/prompts/12-B-benchmark-robustness-workflow.md ) \
  | claude -p --model claude-opus-4-7 --effort medium \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --disallowedTools Task Agent \
      --permission-mode bypassPermissions \
      --output-format stream-json --include-partial-messages --verbose \
  > "$LOG" 2>&1 )
```

## Log destination

- stdout/stderr: `dev/plans/runs/12-B-benchmark-robustness-workflow-<ts>.log`
- structured: `dev/plans/runs/12-B-benchmark-robustness-workflow-output.json`
- reviewer verdict: `dev/plans/runs/12-B-review-<ts>.md`

## Required reading

- `AGENTS.md` § 1, § 3, § 7.
- `MEMORY.md`, especially:
  - `feedback_workflow_validation.md` — actionlint = canonical
    workflow validator. `yaml.safe_load` is NOT.
  - `feedback_reliability_principles.md` — net-negative LoC; no
    soak; delete-before-add.
- `dev/design/orchestration.md` § 2, § 3, § 8.
- `dev/plans/ci-deferred.md` § benchmark-and-robustness.yml — Phase 12
  (the ticket being burned down). Lists adaptations:
  - Drop `go-fuzz-smoke` entirely (no `go/` surface in 0.6.0).
  - Repath `python/` → `src/python/`, `typescript/` → `src/ts/`.
  - Drop `cargo build -p fathomdb --features node` step until
    napi-rs lands.  **NOTE: napi-rs HAS LANDED (Phase 11b).**
    Adapt this step to the new napi build (`npm run build:native`
    in `src/ts`) per `dev/design/orchestration.md` § 4.2 or per
    `.github/workflows/release.yml` build-napi job for reference.
  - Re-target `python-stress-tests` to whatever `--features python`
    / binding shape Phase 11 chose. **NOTE: maturin via
    `pip install -e src/python/` is the canonical Python build
    (per `release.yml` build-python job + `feedback_python_native_build`).**
- Pre-0.6.0 source (layout reference, do NOT copy blindly):
  `git show 39ee271^:.github/workflows/benchmark-and-robustness.yml`.
  The pre-0.6.0 shape: weekly cron `0 7 * * 1`; jobs
  `rust-benchmarks`, `go-fuzz-smoke`, `rust-scale-tests`,
  `rust-tracing-stress`, `python-stress-tests`,
  `typescript-observability-harness`.
- Current substrate to compare against:
  - `.github/workflows/ci.yml` for action SHA pins (reuse same
    pins for checkout / setup-python / setup-node / rust-toolchain
    / rust-cache).
  - `.github/workflows/release.yml` for the napi + python build
    patterns (Phase 11d landed these; mirror them).
- `scripts/bootstrap.sh` and `scripts/agent-verify.sh` for the
  canonical bootstrap pattern.

## Scope — one workflow + supporting scripts

### Item 1: `.github/workflows/benchmark-and-robustness.yml`

Restore the weekly-cron workflow adapted to 0.6.0 topology.

Trigger:

```yaml
on:
  workflow_dispatch:
  schedule:
    - cron: "0 7 * * 1"   # weekly Monday 07:00 UTC
```

Permissions:

```yaml
permissions:
  contents: read
```

Jobs (drop pre-0.6.0 `go-fuzz-smoke` entirely):

1. **`rust-benchmarks`** — `bash scripts/run-benchmarks.sh`.
   Check if that script exists; if not, surface as blocker
   (don't write a stub). It existed pre-0.6.0; verify it survived
   the rewrite.
2. **`rust-scale-tests`** — `cargo nextest run -p fathomdb-engine
   --test scale --run-ignored=only`. Verify `fathomdb-engine`
   has a `scale` test target; if not, surface as blocker (or scope
   in placeholder per orchestrator decision).
   `FATHOM_RUST_STRESS_DURATION_SECONDS: "60"` env.
3. **`rust-tracing-stress`** — `cargo test -p fathomdb-engine
   --features tracing --test tracing_events
   tracing_events_continue_under_concurrent_load`. Verify the
   `tracing` feature exists on `fathomdb-engine` + the test
   exists. If not, surface.
   `FATHOM_RUST_TRACING_STRESS_DURATION_SECONDS: "60"`.
4. **`python-stress-tests`** — adapted: `pip install -e src/python/`
   (per `feedback_python_native_build` memory; NOT `pip install -e
   python` like pre-0.6.0). Then `pytest src/python/tests/test_stress.py
   -v --timeout=120` if the test file exists; if not, surface as
   blocker (don't stub).
   `FATHOM_PY_STRESS_DURATION_SECONDS: "60"`.
5. **`typescript-observability-harness`** — adapted:
   `working-directory: src/ts`. Use `npm ci` + `npm run
   build:native` (the Phase 11b napi build script) + `npm run
   build` + the harness test. Check if a `sdk-harness` workspace
   or equivalent exists in `src/ts`; if not, surface.
   `FATHOM_TS_STRESS_DURATION_SECONDS: "30"`.

Common patterns (carry from `.github/workflows/release.yml` Phase
11d landings):

- Per-session TMPDIR root pattern (already in pre-0.6.0; carry
  forward).
- Pin every third-party action to commit SHA per H-5 convention
  (reuse same pins as `release.yml` / `ci.yml`).
- `timeout-minutes` per job as in pre-0.6.0 source.

Job graph: each job runs independently (parallel; no `needs:`
chain). Per pre-0.6.0 source — same shape.

### Item 2: `scripts/run-benchmarks.sh` if missing

If pre-0.6.0 had `scripts/run-benchmarks.sh` and it's not in the
current tree, the workflow can't run. Two options:

- (a) If the script existed pre-0.6.0 and was lost in the rewrite,
  restore it from `git show 39ee271^:scripts/run-benchmarks.sh`
  adapted to current crate layout. Per "delete-before-add"
  reliability principle, do not invent a new harness — use the
  proven pre-0.6.0 one.
- (b) If no such script existed pre-0.6.0 either, surface blocker
  and recommend the workflow ship without rust-benchmarks until
  the script lands (commented-out job with TODO referencing
  follow-up slice).

### Item 3: actionlint validation

Per `feedback_workflow_validation`: run `actionlint
.github/workflows/benchmark-and-robustness.yml` as part of the
local gate before commit. Phase 11d already wired actionlint into
bootstrap + agent-lint (Phase 11d landed `actionlint v1.7.7`
install). Verify the workflow passes actionlint with zero
findings.

### Item 4 (optional): workflow_dispatch dry_run

If feasible, add a `workflow_dispatch.inputs.dry_run: boolean`
input mirroring Phase 11d release.yml pattern. When `dry_run:
true`, jobs run their setup + actionlint validation but skip the
actual long-run stress commands (or run them with a 10s cap
instead of 60s). This lets us validate the workflow shape
without waiting for the weekly cron to confirm it works.

If this adds too much yaml complexity for marginal value, skip
this item; not blocking. Surface decision in output JSON.

## Required commands

```bash
cd /tmp/fdb-12-B-benchmark-robustness-workflow-<ts>
# Workflow YAML is valid per actionlint.
actionlint .github/workflows/benchmark-and-robustness.yml
# All existing CI / release workflows still pass actionlint
# (regression guard).
actionlint .github/workflows/ci.yml
actionlint .github/workflows/release.yml
# Canonical local gate.
bash scripts/agent-verify.sh
```

If Item 4 dry-run lands, mention how to manually trigger:
`gh workflow run benchmark-and-robustness.yml -f dry_run=true`.

Known flakes (rerun once before declaring red):
`ac_029_canonical_writes_complete_under_projection_stall`,
`ac_017_vector_projection_freshness_p99_le_five_seconds`,
`t_safe_export_engine_error_exits_export_failure_66`.

## Discipline

- Net-negative LoC bias: drop pre-0.6.0 jobs that no longer have a
  target (go-fuzz-smoke definitively; others only if blocker
  confirms their substrate is gone).
- Don't stub workflows that can't actually run. If a target is
  missing, surface as blocker and either restore the substrate
  (per Item 2 option (a)) or comment-out with TODO.
- No `yaml.safe_load` workflow validation. Actionlint only.
- Reuse the action SHAs already pinned in `ci.yml` and
  `release.yml`. Do not introduce new pins.
- No data migration; no production code changes. Workflow + maybe
  one restored bash script.

## Blockers — surface before writing code

If any blocks, STOP and write blocker report at
`dev/plans/runs/12-B-benchmark-robustness-workflow-output.json`:

1. **`scripts/run-benchmarks.sh` missing.** See Item 2 (a)/(b)
   options.
2. **`fathomdb-engine` `scale` test target missing.** Surface
   - recommend comment-out + follow-up slice.
3. **`fathomdb-engine` `tracing` feature missing.** Same.
4. **`src/python/tests/test_stress.py` missing.** Same.
5. **`src/ts/` `sdk-harness` workspace missing.** May need
   adaptation if Phase 11b used a different structure. Read
   `src/ts/package.json` first to see workspace shape.
6. **Pre-0.6.0 cron `0 7 * * 1` UTC inappropriate for current
   release cadence.** Not really a blocker; mention in output if
   you have an opinion.

## Output

After all commands pass, write
`dev/plans/runs/12-B-benchmark-robustness-workflow-output.json`:

```json
{
  "phase": "12-B-benchmark-robustness-workflow",
  "baseline_sha": "<HEAD of 0.6.0-rewrite at spawn>",
  "branch": "phase-12-B-benchmark-robustness-workflow-<ts>",
  "head_sha": "<HEAD after final commit>",
  "commits": ["<sha>: <subject>", "..."],
  "workflow_file": ".github/workflows/benchmark-and-robustness.yml",
  "workflow_line_count": <int>,
  "jobs_restored": ["rust-benchmarks", "rust-scale-tests", "rust-tracing-stress", "python-stress-tests", "typescript-observability-harness"],
  "jobs_dropped": ["go-fuzz-smoke (no go/ surface in 0.6.0)"],
  "scripts_restored_or_added": ["..."],
  "dry_run_supported": true | false,
  "actionlint_result": "green",
  "blockers_encountered": [{...}],
  "agent_verify_result": "pass | fail (+ tail)",
  "next_step_for_orchestrator": "promote to 0.6.0-rewrite; respawn codex reviewer for verdict"
}
```

Then stop. Do not advance to 12-P. Do not run the reviewer yourself.
