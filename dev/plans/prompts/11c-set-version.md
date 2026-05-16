# Phase 11c — Two-axis version single-source-of-truth + set-version.sh rewrite

Phase 11 third slice. Decouples the embedder-api crate from workspace
lockstep, restores `scripts/set-version.sh` with two-axis enforcement,
wires the pre-push hook, and lands the AC-051a/b skew fixtures.

Out of scope: PyO3 (11a closed), napi-rs (11b closed), `release.yml`
restoration (11d).

## Model + effort

Opus 4.7, intent: high. Spawn from main thread:

```bash
PHASE=11c-set-version
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
  ( echo "$PREAMBLE"; cat /home/coreyt/projects/fathomdb/dev/plans/prompts/11c-set-version.md ) \
  | claude -p --model claude-opus-4-7 --effort high \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --disallowedTools Task Agent \
      --permission-mode bypassPermissions \
      --output-format stream-json --include-partial-messages --verbose \
  > "$LOG" 2>&1 )
```

Anti-chaining: PREAMBLE prepended via stdin per
`dev/plans/prompts/01-orchestrator-resume.md` §4. Reviewer
(`codex --model gpt-5.4`) MANDATORY: this slice changes the version
substrate that the 11d release workflow will consume.

## Log destination

- stdout/stderr: `dev/plans/runs/11c-set-version-<ts>.log`
- structured output: `dev/plans/runs/11c-set-version-output.json`
- reviewer verdict: `dev/plans/runs/11c-review-<ts>.md`

## Required reading

- `AGENTS.md` (§1, §3, §4, §5, §7).
- `MEMORY.md`, especially `feedback_tdd.md`,
  `feedback_reliability_principles.md`,
  `feedback_release_verification.md`.
- `dev/design/release.md` (entire file, ~99 lines). The two-axis
  policy and 8-tier publish order are owned here.
- `dev/plans/ci-deferred.md` § `scripts/set-version.sh — Phase 11`
  (the rewrite ticket).
- `dev/acceptance.md` AC-051a (Cargo skew), AC-051b (pip skew),
  AC-052 (sibling co-tagging — context only; not in this slice).
- Pre-0.6.0 set-version.sh for layout reference only — DO NOT copy
  blindly, the new layout uses `src/{rust,python,ts}/`:
  `git show 39ee271^:scripts/set-version.sh`.
- Current state to patch:
  - `Cargo.toml` lines 15-16 (`[workspace.package] version = "0.6.0"`).
  - `src/rust/crates/fathomdb-embedder-api/Cargo.toml` —
    **currently uses `version.workspace = true`; this is wrong**.
  - All other `src/rust/crates/*/Cargo.toml` — keep
    `version.workspace = true`.
  - `src/python/pyproject.toml` `[project] version`.
  - `src/ts/package.json` `version`.
  - `scripts/hooks/pre-push` (only runs `agent-verify.sh` today).
  - `scripts/hooks/pre-commit` (for reference).

## Scope

Four work items, one commit per item is fine.

### 1. Decouple Axis E from workspace lockstep

`src/rust/crates/fathomdb-embedder-api/Cargo.toml`:

- Replace `version.workspace = true` with explicit
  `version = "0.6.0"`. (Initial Axis E value happens to equal Axis W
  today; the axes will diverge as the trait surface evolves.)

`Cargo.toml`:

- Add a comment beside `[workspace.package]` explaining: workspace
  version is Axis W lockstep; `fathomdb-embedder-api` is the only
  workspace crate that opts out and carries an independent Axis E
  version in its own `[package]` block. Cite
  `dev/design/release.md § Version axes`.

Verify `cargo check` still passes — the decoupling is metadata-only
and must not break workspace resolution.

### 2. `scripts/set-version.sh` rewrite

New executable bash script at `scripts/set-version.sh`. Three modes:

```text
scripts/set-version.sh --workspace <new-w-version>
scripts/set-version.sh --embedder-api <new-e-version>
scripts/set-version.sh --check-files
```

Behavior:

- `--workspace <new>`:
  - Update `Cargo.toml` `[workspace.package] version`.
  - Update `src/python/pyproject.toml` `[project] version`.
  - Update `src/ts/package.json` top-level `"version"`.
  - Do NOT touch `src/rust/crates/fathomdb-embedder-api/Cargo.toml`
    (Axis E).
  - Idempotent — re-running with the same `<new>` is a no-op.
  - After write, run `--check-files` and fail if inconsistent.

- `--embedder-api <new>`:
  - Update `src/rust/crates/fathomdb-embedder-api/Cargo.toml`
    `[package] version`.
  - Do NOT touch any Axis W manifest.
  - Idempotent.

- `--check-files`:
  - Read `[workspace.package] version` from `Cargo.toml`.
  - Assert `src/python/pyproject.toml` `[project] version` matches.
  - Assert `src/ts/package.json` `"version"` matches.
  - For every `src/rust/crates/*/Cargo.toml` except
    `fathomdb-embedder-api`: assert `version.workspace = true`
    (no per-crate explicit version pinning that would silently drift).
  - Assert `fathomdb-embedder-api/Cargo.toml` has an explicit
    `[package] version` value (proof Axis E is decoupled).
  - Exit 0 on consistent, exit 1 on any drift, with one error line
    per drift naming `file:line` + observed vs expected.

Implementation notes:

- Use POSIX-portable bash (`#!/usr/bin/env bash`, `set -euo pipefail`).
- Parse TOML / JSON with `sed`/`awk`/`grep` (no extra deps assumed in
  bootstrap). If you need a TOML lib, prefer reading a single
  declared-format line per file rather than installing a parser.
- All writes must be atomic (`mv tmpfile orig` after sed-on-tmp);
  partial-write on a manifest is unacceptable.
- Treat unknown flags / missing args / wrong format as errors with
  usage text on stderr and exit 2.

### 3. Wire pre-push hook

`scripts/hooks/pre-push`:

- Add `scripts/set-version.sh --check-files` as the FIRST step,
  before `agent-verify.sh`. If `--check-files` fails, the push is
  blocked with the actionable error.

Resolved per `dev/plans/ci-deferred.md` note: the pre-push hook
omitted this previously because the script did not exist. Now it
does. No `--no-verify` skip authorization beyond what AGENTS.md
already says.

### 4. AC-051a/b skew fixtures + tests

`dev/test-plan.md` (or wherever cargo-skew / pip-skew fixtures live
— search first; if `dev/release/tests/` does not exist yet, create
it). Two fixtures + two test scripts. Both are tiny.

#### Cargo-skew fixture (AC-051a)

`dev/release/fixtures/cargo-skew/Cargo.toml`:

```toml
[package]
name = "cargo-skew-probe"
version = "0.0.0"
edition = "2021"

[dependencies]
fathomdb = "=0.6.0"
fathomdb-embedder = "=0.6.0"
# Force a constraint that cannot resolve under the current
# fathomdb-embedder-api Axis E value.
fathomdb-embedder-api = "=99.99.99"
```

`dev/release/tests/cargo_skew.sh` (or equivalent Rust integration
test under a crate): invokes `cargo update` in the fixture against
a local-path override that pins the workspace crates to the live
versions; asserts non-zero exit; asserts stderr names the conflict.

#### Pip-skew fixture (AC-051b)

`dev/release/fixtures/pip-skew/constraints.txt`:

```text
fathomdb==0.6.0
fathomdb-embedder==0.6.0
fathomdb-embedder-api==99.99.99
```

`dev/release/tests/pip_skew.sh`: runs
`pip install -c constraints.txt fathomdb fathomdb-embedder` in a
clean venv; asserts non-zero exit; asserts stderr names the
conflict. Note: `fathomdb-embedder-api` is not currently published
to PyPI, so this test may need to use a local-built wheel index;
if achieving real pip resolution requires substrate not in this
slice (e.g. building wheels for all three packages), surface as a
blocker — do NOT silently skip.

### 5. set-version.sh tests

`scripts/tests/test_set_version.sh` (or `.bats` if bats is already
in bootstrap; otherwise plain bash). Cover:

- `--check-files` returns 0 on a clean tree.
- `--workspace 9.9.9` updates all Axis W manifests; `--check-files`
  still 0; revert via `--workspace 0.6.0`.
- `--embedder-api 9.9.9` updates only the Axis E manifest; Axis W
  manifests untouched; `--check-files` still 0; revert.
- Manual drift on one Axis W file → `--check-files` exits 1 with the
  drift named.
- Idempotent re-run: `--workspace 0.6.0` twice in a row is a no-op
  (no file mtime change on the second run, OR the second run
  detects a clean tree and exits 0).
- Unknown flag → exit 2 with usage.

Wire the test runner into `scripts/agent-test.sh` so
`agent-verify.sh` exercises it. The wiring should match the existing
pattern for python/ts test invocations in `agent-test.sh`.

## Required commands

```bash
cd "$WT"
# Sanity: workspace still resolves after Axis E decoupling.
cargo check --workspace
# set-version.sh script tests.
bash scripts/tests/test_set_version.sh
# Skew fixture tests (if landed).
bash dev/release/tests/cargo_skew.sh
bash dev/release/tests/pip_skew.sh
# Canonical local gate.
./scripts/agent-verify.sh
```

All must pass. If `agent-verify.sh` flakes on the known
`ac_029_canonical_writes_complete_under_projection_stall` or
`t_safe_export_engine_error_exits_export_failure_66`, rerun once —
both are pre-existing host-load timing flakes unrelated to 11c.

## Discipline

- TDD per `feedback_tdd.md`: write each test before its code.
- No scope creep into 11d (`release.yml`). 11d consumes
  `set-version.sh` but does not itself land here.
- No backwards-compat shim that allows Axis E to inherit from the
  workspace version "for now". The decoupling is the slice.
- Comment policy per `AGENTS.md`: no WHAT comments, only non-obvious
  WHY. Document the Axis E decoupling in `Cargo.toml` because the
  reason (two-axis release policy) is non-obvious from the code.
- Cite acceptance ids in test names / module docs: `AC-051a`,
  `AC-051b`.
- Bash discipline: `set -euo pipefail`, atomic writes,
  POSIX-portable, no `gawk`/`mawk` assumptions unless documented.
- One commit per logical step; closure summary line in last commit
  message.

## Blockers — surface before writing the code

If any of these are true, STOP and write a blocker report:

- Pip-skew fixture genuinely requires publishing all three packages
  to a real index (PyPI or local devpi) before the resolver can be
  exercised. If `pip install` cannot exercise the resolver on
  unpublished local packages without substantial substrate, scope
  AC-051b out of this slice and into a follow-up; document
  rationale; do NOT silently ship a no-op test.
- `agent-test.sh` cannot be extended to run
  `scripts/tests/test_set_version.sh` without rewriting
  `agent-verify.sh` itself (out of scope for 11c).
- The workspace fails `cargo check` after Axis E decoupling
  because some other crate `[dependencies]` block references
  `fathomdb-embedder-api` with `version.workspace = true` (the
  workspace-shared `[dependencies]` syntax), which would also have
  to decouple. Surface and ask for guidance.

Blocker report shape: same as 10b-B
(`dev/plans/runs/10b-B-purge-restore-output.json`).

## Output

After all commands pass, write
`dev/plans/runs/11c-set-version-output.json` with:

```json
{
  "phase": "11c",
  "baseline_sha": "<HEAD at spawn time>",
  "branch": "phase-11c-set-version-<ts>",
  "head_sha": "<HEAD after final commit>",
  "axis_e_decoupled": true,
  "set_version_modes": [
    "--workspace",
    "--embedder-api",
    "--check-files"
  ],
  "pre_push_hook_wired": true,
  "ac_051a_fixture": "<path or 'deferred with rationale'>",
  "ac_051b_fixture": "<path or 'deferred with rationale'>",
  "tests_added": ["<test names>"],
  "acceptance_ids_bound": ["AC-051a", "AC-051b"],
  "agent_verify_result": "pass | fail (+ tail)",
  "next_step_for_orchestrator": "spawn 11c reviewer; then 11d release.yml"
}
```

Then stop. Do not advance to 11d. Do not run the reviewer yourself.
