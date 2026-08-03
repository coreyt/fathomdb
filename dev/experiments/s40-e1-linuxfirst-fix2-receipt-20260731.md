# Slice 40 E1 Linux-first guard correction 2 receipt

- Role: ROLE-SIMULATED implementer; final explicitly authorized correction for
  the Linux-x86 workflow-guard invariant.
- Private clone: `/tmp/fathomdb-s40-e1-linuxfirst-fix2-bioZyf/repo`.
- Branch: `s40-e1-linuxfirst-fix2-20260731`.
- Immutable base: `111313b91603d7feca3df0119673a4c8444e38df` (`origin/main`
  at each commit; descendant of `0f7642cc`).
- No tag, push, PR, dispatch, publish, credential, registry, shared-checkout,
  release-state, ledger, handoff, or B5 action occurred.

## Commit chain

| Commit | Purpose |
| --- | --- |
| `2a772ba75e2d310e1f56b0527a40ae41bb7ad23d` | Carry forward the reviewed Linux-first workflow, documentation, and baseline guard changes. |
| `50aa0ca5177aafb34becb2e5386721999f24884d` | RED regression: committed macOS/Windows `matrix.runner` fixture. |
| `b286004d7c983254da0db4cfe5e158a228d9ba14` | GREEN fix: guard literal runners, matrix runner routes/values, and native target/label values. |
| receipt commit below | This receipt only. |

## RED-first evidence

The committed fixture
`scripts/tests/fixtures/linux_first_matrix_runner_macos_windows.yml` has both
`runner: macos-latest` and `runner: windows-latest`, each routed through
`runs-on: ${{ matrix.runner }}`. Before `b286004d`, running
`bash scripts/tests/test_linux_first_platform_scope.sh` exited **1**. Its
probe reported that the predecessor guard accepted those matrix runner values;
the outer RED assertion then failed with `guard accepts macOS/Windows values
routed through matrix.runner`.

## Guard contract and source locations

`scripts/tests/test_linux_first_platform_scope.sh` is deliberately text-only;
it does not parse YAML. `actionlint` remains the workflow syntax/schema
authority.

1. Literal `runs-on` labels in `.github/workflows/ci.yml` and
   `.github/workflows/release.yml` must be Ubuntu.
2. A `runs-on: ${{ matrix.runner }}` route must have active `runner` values;
   any other matrix field is rejected, and every active runner value must be
   Ubuntu.
3. Matrix include `runner` values are extracted as values, including the
   first `- runner:` entry, so macOS/Windows cannot hide behind an expression.
4. CI artifact `target` and `label` inputs must be respectively
   `x86_64-unknown-linux-gnu` and `linux-x64`.
5. Release native artifact `target` and `label` inputs must be respectively
   `x86_64-unknown-linux-gnu` and `linux-x64-gnu`.

Matrix *input* expressions such as `${{ matrix.target }}` and
`${{ matrix.label }}` are explicitly excluded from value checks so consumers
are not mistaken for values. The guard retains the Linux `changes`, `verify`,
and `security` gates, the Linux release paths, and explicit macOS/Windows
deferral to 0.8.22.

## Verification

| Command | Exit |
| --- | --- |
| RED: `bash scripts/tests/test_linux_first_platform_scope.sh` before GREEN | 1 |
| `bash scripts/tests/test_linux_first_platform_scope.sh` after GREEN | 0 |
| `bash scripts/tests/test_release_workflow_scope.sh` | 0 |
| `actionlint .github/workflows/ci.yml .github/workflows/release.yml` | 0 |
| `bash scripts/check-release-state-views.sh --check` | 0 |
| `git diff --check 111313b91603d7feca3df0119673a4c8444e38df..HEAD` | 0 |

`scripts/agent-lint.sh` was not run because this fresh clone has no
`node_modules`; no dependency installation was performed. Expected CI runs:
none (private, unpushed branch).

## Scoped documentation exception

The carried-forward changes to the plan, master schedule, board, and commission
brief are non-generated prose outside release-state-owned regions. They are
retained because the guard directly verifies the bounded HITL ruling they
record: 0.8.20 Linux-x86 native support; B4 cancelled/deferred to 0.8.22; and
five relevant Linux CI TC-91 greens. No release-state JSON or generated region
was edited.

## Review and next action

Predecessor review transcripts:

- `/tmp/fathomdb-s40-linuxfirst-review-85X6l5/repo/dev/experiments/s40-e1-linuxfirst-independent-review-20260731.md`
- `/tmp/fathomdb-s40-linuxfirst-fix1-review-yHiJgT/repo/dev/experiments/s40-e1-linuxfirst-fix1-independent-review-20260731.md`

Exact next action: obtain an independent fresh adversarial review of this
branch. If it finds any incomplete runner/matrix guard coverage, hard-stop to
the Steward; this was the final authorized same-invariant correction and no
third correction is permitted.
