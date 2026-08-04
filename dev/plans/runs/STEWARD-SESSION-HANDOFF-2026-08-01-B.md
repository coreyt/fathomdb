# Steward session hand-off — 2026-08-01-B

> **⚠ HISTORICAL RECORD — SUPERSEDED. NOT CURRENT DIRECTION.**
>
> Superseded by `STEWARD-SESSION-HANDOFF-2026-08-03-A.md`. The 0.8.20
> crates.io/PyPI recovery this document commissions **completed**: v0.8.20 is
> published (steward ledger seq-239), and 0.8.21's implementation ladder has
> since landed in full. **Do not act on the ★ IMMEDIATE NEXT STEP below.** It is
> retained only so the hand-off chain has no gap between 2026-08-01-A and
> 2026-08-03-A.
>
> This file was never on `main`. It was recovered on 2026-08-04 from branch
> `docs/steward-handoff-2026-08-01` (`180554f6`) before that branch was retired.
> A **second, divergent** 115-vs-61-line version of this same filename existed
> on branch `steward-s40-readiness-handoff` (`59a19cc0`), recording a BASE CI
> lint blocker. The longer 180554f6 version is preserved here as the hand-off of
> record; see `dev/plans/runs/0.8.21-cleanup-and-drift-resolution-plan.md` §2
> Group C for the divergence.

**Supersedes `STEWARD-SESSION-HANDOFF-2026-08-01-A.md`.**

## ★ IMMEDIATE NEXT STEP

**Commission one fresh release-recovery agent to complete the authorized 0.8.20
crates.io/PyPI recovery.** This is not a Slice 40 implementation run: Slice 40
is complete and the source/release workflow repair is already merged.

The agent's first job is to make one sparse GitHub API check. If the secondary
rate limit has cleared, dispatch `.github/workflows/release.yml` once with the
already approved recovery inputs:

```text
dry_run=false
release_version=0.8.20
confirm_release_version=0.8.20
recovery_skip_npm=true
```

The workflow must run from `origin/main` at `08d386e0`, but it deliberately
checks out immutable `refs/tags/v0.8.20` for the artifacts. Do not retag,
force-push a tag, publish npm, or change the source/CI configuration without a
new concrete failure log. `recovery_skip_npm=true` is intentional: npm is not
a gate for the crates.io/PyPI recovery.

## 1. Verified state

- **Slice 40 is complete and landed:** `833a2035`; `SCHEMA` remains 24. The
  implementation ladder is complete (`next_slice: null`, no remaining ladder).
  The separate explicit HITL **PUBLISH** decision remains the sole unruled
  release decision in `dev/plans/release-state-0.8.20.json`.
- **Recovery repair is merged:** PR #172, merge commit `08d386e0`
  (`fix(release): recover v0.8.20 crates.io and PyPI without npm`).
- **The repair's GitHub CI is green:** Actions run `30714462031` completed
  successfully; its `verify` job passed in 37m02s. The repair includes the
  exact local/CI toolchain alignment and the Go-installed `actionlint` PATH
  handoff regression coverage.
- **Local evidence before that PR:** the CI-like actionlint regression and an
  actual `agent-lint` run passed; lint, typecheck, and security passed. A prior
  full authoritative local verify completed 46/46 green in 785 seconds. Do not
  claim a fresh full local verify unless its real exit code is captured.
- **No recovery publish started.** Several release-dispatch attempts were
  rejected by GitHub with secondary-rate-limit HTTP 403 before a workflow was
  created. This is the current external blocker, not a workflow failure.

## 2. Recovery-agent contract

1. Work in a fresh worktree from `origin/main`; run
   `scripts/preflight.sh --worktree <path>` before work. Preserve the dirty
   shared checkout and never run `maturin develop` or `pip install -e` from a
   worktree.
2. Use sparse GitHub polling: do not repeatedly query while rate-limited. After
   one failed API request, record the time/error and stop rather than consuming
   the rate budget.
3. If dispatch succeeds, monitor the one run sparsely. Treat any source,
   workflow, or version failure as a new diagnosis: reproduce the smallest
   relevant local check before proposing a patch. Do not make speculative CI
   edits.
4. After success, verify registries rather than narrating success:
   - crates.io Axis-W chain: `fathomdb-schema`, `fathomdb-query`,
     `fathomdb-embedder`, `fathomdb-engine`, `fathomdb`, and
     `fathomdb-cli` at `0.8.20`;
   - `fathomdb-embedder-api` remains Axis-E at `0.6.1` unless the release
     evidence proves otherwise;
   - PyPI `fathomdb==0.8.20`, followed by an isolated-wheel install and a
     real open/close/exit smoke.
5. Stop and report if any publish tier fails after an immutable upload. The
   tiered publish scripts are intentionally idempotent; recovery must be based
   on the actual registry state, never on a retag.

## 3. Version and platform boundaries already decided

- The `v0.8.20` tag is immutable at `5e3c95eb`; do not move it.
- `scripts/set-version.sh` aligns Axis-W Rust, Python, and TypeScript versions;
  Axis-E is intentionally independent. The release repair added tests covering
  runtime and release alignment.
- 0.8.20 publishes Linux x86_64 native artifacts only. npm's incomplete
  platform topology is explicitly out of this recovery: the ruled unscoped
  sibling-package approach belongs to a coherent future release, never as an
  orphan upload against this immutable tag. Its release slot must be checked
  against the then-current plan and HITL direction.
- macOS and Windows native artifacts remain 0.8.22 work; aarch64 Linux remains
  deferred to 0.8.24+.

## 4. CI noise and Dependabot boundary

- Do not conflate the green recovery run `30714462031` with cancelled or
  failing Dependabot-branch runs. The observed Dependabot failures were stale
  exact-SHA test-fixture expectations after an `actions/checkout` bump, not a
  failure on the recovery source.
- PR #173 (`chore/pause-dependabot-updates`) is open and **must not be merged**.
  The HITL later said "leave the dependabot" after directing specific bot PRs
  closed; whether #173 should now be closed is still ambiguous. Ask before
  touching Dependabot configuration or reopening those bot PRs.

## 5. Workspace and cleanup facts

- The shared checkout is user-dirty and must not be used for edits. At this
  handoff, local `main` is an ancestor of `origin/main`; trust explicit refs,
  not the session-start branch report.
- The disposable actionlint verification clones and their Go caches were
  removed. `/tmp` has about 46 GiB free. Preserve existing named worktrees;
  do not perform broad deletion without a verified owner/scope.

## 6. Evidence links and stop conditions

- Recovery PR: <https://github.com/coreyt/fathomdb/pull/172>
- Green recovery CI: <https://github.com/coreyt/fathomdb/actions/runs/30714462031>
- Release repair commits culminate in `c17b14ae`, merged as `08d386e0`.

**Stop conditions:** GitHub secondary rate limiting, any post-upload registry
failure, or an unapproved decision to alter the tag/source/npm scope. Surface
the exact command output and state; do not keep retrying blindly.
