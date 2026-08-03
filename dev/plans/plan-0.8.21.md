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

## Requirements and acceptance criteria

- TC-91 reproduces before the change and the targeted worker/recovery tests,
  binding-surface tests, full workspace check, and clippy pass with real exits.
- The serial verifier is repeatable from a clean supported environment and
  records every command and immediate result without required-suite skips.
- Every loader triple is represented in the platform manifest; a published
  entry has matching package metadata and public documentation.
- Linux aarch64 is not called supported until its build, native open/close/exit
  smoke, and registry-installed smoke are recorded.

## Slice ladder

| Slice | Work | Depends on |
| ---: | --- | --- |
| 0 | State, documentation, toolchain, and platform-manifest design | — |
| 5 | TC-91 hardened projection-worker commit | 0 |
| 10 | Reproducible serial local verifier | 5 |
| 15 | Linux aarch64 package/build/smoke proof | 10 |
| 20 | Current-documentation and platform-drift checks | 15 |

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
**IMMEDIATE NEXT: Slice 0** (`FOUNDATION`) — state, documentation, toolchain, and platform-manifest design

**Remaining ladder:** 0 → 5 → 10 → 15 → 20.<!-- END GENERATED release-state:0.8.21:plan-immediate-next -->
