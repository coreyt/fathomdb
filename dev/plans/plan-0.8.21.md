---
title: FathomDB 0.8.21 — reliability and platform foundation
status: ACTIVE
target_release: 0.8.21
---

# FathomDB 0.8.21 — reliability and platform foundation

0.8.21 is a label-only foundation release. It starts from the published
v0.8.20 release and establishes reliable local verification before expanding
native-platform delivery. It does not publish artifacts.

The former free-threaded-Python and benchmark ladder is **SUPERSEDED as this
release plan**. Its research remains in
`dev/design/free-threaded-python-value-lift-and-experiments.md`; any future
experiment needs its own proposal and evidence gate.

## Goal and scope

1. Close TC-91 using the hardened worker-commit design and its red-first
   regression tests. Public APIs remain stable.
2. Make the serial, whole-workspace verifier reproducible locally with the
   exact CI toolchain and locked dependency installs. Required suites must
   fail rather than skip; each command must report its immediate exit status.
3. Establish `dev/platform-capabilities.json` as the checked capability source
   for Rust targets, N-API packages, Python wheels, loader behavior, release
   paths, compatibility, and install documentation.
4. Prepare Linux aarch64 packaging only after the manifest and local proof
   gate exist. It is not advertised or published until artifact, package,
   loader, registry smoke, and documentation evidence all agree.
5. Repair current documentation and release-state truth, including the
   published 0.8.20 status, npm `next` channel, and the nine-crate workspace.
6. Make shell-level verification failures **statically detectable and early**,
   so the class that fails at 33 minutes fails at 60 seconds instead. Slices
   25–35, added 2026-08-04.
7. Stop a dependency pin from silently becoming the vulnerability it was added
   to fix. Slice 40, added 2026-08-04.
8. Make CI **observable**: a failure legible without log-diving, and suite
   execution visible on a green run. Slice 45, added 2026-08-04.
9. Stop paying for avoidable runs and floating linters. Slice 50, added
   2026-08-04.

## Requirements and acceptance criteria

- TC-91 reproduces before the change and the targeted worker/recovery tests,
  binding-surface tests, full workspace check, and clippy pass with real exits.
- The serial verifier is repeatable from a clean supported environment and
  records every command and immediate result without required-suite skips.
- Every loader triple is represented in the platform manifest; a published
  entry has matching package metadata and public documentation.
- Linux aarch64 is not called supported until its build, native open/close/exit
  smoke, and registry-installed smoke are recorded.
- `shellcheck` runs over every tracked shell file and its findings are
  enforced, not advisory. A newly introduced `cmd | head` under `pipefail`, or a
  newly masked return, must fail the gate.
- The shell gate is **always-on** — never `docs_only`-gated, never `needs:`-gated
  behind the expensive jobs — and reports in about a minute.
- No suite is silently skipped, retried, or quarantined to achieve any of this.
  A remediation that converts a real failure into a pass is out of scope by
  construction.
- A failed CI run names its failing suites **on the run's front page**, and the
  evidence needed to diagnose it survives the runner. "The log was truncated and
  the spill file is gone" is not an acceptable outcome.
- A **green** run states which suites ran. A summary that cannot distinguish
  "passed" from "never executed" is a vacuous pass at the harness level, which
  is the same defect as TC-37 one layer up.
- Linters are **pinned and version-checked**, so a finding set changes only when
  the repository decides it does.

## Slice ladder

| Slice | Work | Depends on |
| ---: | --- | --- |
| 0 | State, documentation, toolchain, and platform-manifest design | — |
| 5 | TC-91 hardened projection-worker commit | 0 |
| 10 | Reproducible serial local verifier | 5 |
| 15 | Linux aarch64 package/build/smoke proof | 10 |
| 20 | Current-documentation and platform-drift checks | 15 |
| 25 | Remediate the audited SIGPIPE / fail-open shell sites | 20 |
| 30 | `shellcheck` in `scripts/agent-lint.sh` (+ `.shellcheckrc`, masked-return checks) | 25 |
| 35 | Always-on `shell-lint` CI job ahead of the `verify` gate | 30 |
| 40 | Guard that a dependency pin is still a fix, not the vulnerability | 20 |
| 45 | Make a CI failure legible without log-diving; make suite execution visible on success | 35 |
| 50 | Pin `pyright` with a drift test; stop superseded runs | 20 |

### Slices 45 and 50 — observability and run hygiene (added 2026-08-04)

Added because the release goal is a CI that is robust, **observable**, and fails
early. Slices 25–35 deliver robustness and early failure; nothing so far
delivers observability. Design of record:
`dev/design/ci-verify-robustness-review.md` (R3.7, R2.3, R2.7 for Slice 45;
R2.9, R2.4 for Slice 50).

**Slice 45 — the failure you cannot read.** `verify` uploads **nothing** on
failure; only the `rust-workspace-race-report` job in
`.github/workflows/ci.yml` does. `run_capped`
truncates to 200 lines and writes the remainder to a `/tmp` file that **dies
with the runner**, so two `test-ts` failures in the review's sample are **not
root-causable at all**. Failure artifacts, a `$GITHUB_STEP_SUMMARY` naming the
failing suites, and `::error` annotations fix that.

There is a second, quieter half. `run_capped` is **silent on success**, so a
green run cannot tell you which suites ran — the Steward ran the full suite on
2026-08-04, saw `55/55 suites passed`, and could not confirm from the log that
any specific suite had executed. Per-suite timing on success is also the only
way any future performance claim about this gate can be *measured* rather than
asserted.

A related live case for the same requirement: `scripts/tests/test_check_design_refs.sh`
exists but is **registered in nothing**, so it runs never and its red gates
nothing. An orphan suite is dead verification that reads as coverage. Whatever
Slice 45 builds should make that visible.

**Slice 50 — run hygiene.** `pyright` is unpinned at `>=1.1.380`
(`src/python/pyproject.toml:36,46`) while `ruff==0.15.17` on the adjacent line is
pinned exactly and enforced with a loud version check. That asymmetry is the
mechanism behind the review's largest failure bucket — 46 of 77 failures at
~4 minutes were `pyright`, and a floating typechecker red-lined `main` for about
two days. The pinning discipline exists; it was simply not applied here.

`ci.yml` also has **zero** `concurrency` groups, so a superseded push burns
another full ~35-minute run rather than cancelling. Runs on `main` must **not**
be cancelled — a cancelled post-merge run leaves `main`'s status ambiguous.

### Slices 25–35 — CI reliability (added 2026-08-04)

Added by HITL decision after the foundation ladder closed, rather than deferred
to 0.8.22: this is verification-reliability work, which is the release's stated
theme. Design of record is
`dev/design/ci-verify-robustness-review.md` (PROPOSED).

**What prompted them.** PR #178's `verify` job failed at ~33 minutes on a SIGPIPE
race in a shell test — `grep | head` under `set -o pipefail`, where `head` closes
the pipe, `grep` dies on SIGPIPE (rc=2), and `pipefail` aborts the suite before
its last assertion. It passed locally, because the race depends on output volume.
It was the **third** occurrence of that class.

**The measured case for fixing it here.** Across 100 CI runs (2026-07-30→08-04),
8 failures reached `step=test` at 25–37 minutes, and **5 of those failed in a
suite costing under 6 seconds**. On the #178 run the verdict was in the log at
`00:45:39` and the job exited at `01:15:26` — **29m47s after the answer was
known**. Lint, typecheck and security together cost 45s.

`shellcheck` is currently invoked **nowhere** in the repository: 145 shell files,
123 under `set -euo pipefail`, zero linting. All 43 `shellcheck` occurrences are
`# shellcheck` comment directives.

**Slice 25 is remediation, 30 is prevention, 35 is early detection.** 35 is what
converts this failure class from a 33-minute discovery into a ~60-second one; it
depends on 25 so the new gate does not land red.

### Slice 40 — pinned-override rot (added 2026-08-04)

Parallel to 25–35; depends only on 20. Design of record:
`dev/design/pinned-override-rot-guard.md`.

`package.json` pinned `js-yaml` to `4.2.0` in 0.8.9 **to fix** GHSA-h67p-54hq-rp68.
On 2026-08-04 GHSA-52cp-r559-cp3m landed with vulnerable range `>= 4.0.0, < 4.3.0`
— **the pin was inside it.** The line written to close a js-yaml advisory had
become the js-yaml exposure, while its own comment still advertised it as the
remedy.

The guard asserts a pin is still a fix (not itself vulnerable), is still
*needed* (not outliving its reason), and still states why it exists. It must
gate rather than advise, must not silently pass when the advisory source is
unreachable, and must not re-litigate the exceptions `.github/dependabot.yml`
already documents as accepted.

**Explicitly NOT in scope.** The review argues against test-level retries,
generic flake quarantine, and per-language path skipping, on the grounds that
each can mask a real failure. Splitting `verify` into fast and heavy tiers is
deferred: it needs a mechanical totality guard first, or it reintroduces the
vacuous-green hazard that the 0.8.20 collect-all harness was built to remove.

## Landed foundation

<!-- BEGIN GENERATED release-state:0.8.21:plan-landed-roll-up -->
**LANDED on `origin/main`, in full:** Slices 0 (`2ea2c884`) · 5 (`a6cf2bbe`) · 10 (`f94275e1`) · 15 (`19d8f072`) · 20 (`354ee9b4`) · 25 (`11766d8b`) · 40 (`895d7cec`). SCHEMA is 24; remaining ladder = 30 → 35 → 45 → 50 → 55.<!-- END GENERATED release-state:0.8.21:plan-landed-roll-up -->

## Reserved-gap policy

No reserved-gap work is authorized by this plan. The former free-threading and
benchmark material is a future experiment proposal, not an implicit slice.

## Cross-cutting DoD

Every slice preserves public binding surfaces unless its plan updates the
interface contracts, runs the relevant binding tests, and records exact local
verification exits. Platform support requires agreement among the manifest,
artifact, package metadata, loader, registry smoke, and public documentation.

## 0.9 readiness follow-through

0.8.22 reduces navigation and records document debt using the existing
two-phase `repo-prune` classifier. 0.8.23 completes lifecycle classification,
current architecture/contract baselines, link validation, and bounded module
extractions demonstrated by TC-91 and platform work. Historical records are
retained in place; current indexes must identify current authority.

## Immediate next slice

<!-- BEGIN GENERATED release-state:0.8.21:plan-immediate-next -->
**IMMEDIATE NEXT: Slice 30** (`SHELLCHECK`) — shellcheck in agent-lint.sh with .shellcheckrc and masked-return checks

**Remaining ladder:** 30 → 35 → 45 → 50 → 55.<!-- END GENERATED release-state:0.8.21:plan-immediate-next -->

Release closure and the opening of 0.8.22 remain explicit state transitions and
are not implied by this plan.
