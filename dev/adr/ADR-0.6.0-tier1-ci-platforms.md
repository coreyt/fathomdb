---
title: ADR-0.6.0-tier1-ci-platforms
date: 2026-04-27
target_release: 0.6.0
desc: Tier-1 CI platforms — linux x86_64 + aarch64, macOS x86_64 + aarch64, windows x86_64; NAPI gap closed
blast_radius: .github/workflows/ci.yml; .github/workflows/release.yml; .github/workflows/python.yml; .github/workflows/typescript.yml; deps/* (cross-platform constraints); test-plan.md (matrix coverage)
status: accepted
---

# ADR-0.6.0 — Tier-1 CI platforms

**Status:** accepted (HITL 2026-04-27, decision-recording — lite batch).

Phase 2 #10 acceptance ADR. Closes the platforms-list candidate by recording
existing matrix and patching the NAPI gap.

## Context

0.5.x release matrix already covers five Python targets (linux x86_64,
linux aarch64, macOS x86_64, macOS aarch64, windows x86_64) but only three
NAPI targets (linux x86_64, macOS x86_64, macOS aarch64). The gap is
unintentional: NAPI lacks linux aarch64 (Graviton, Pi, Jetson — including
this very dev box) and windows x86_64.

`feedback_cross_platform_rust` (c_char rule) already exists because we have
been bitten by the linux-aarch64-vs-x86_64 split. Tier-1 list must commit
to that platform across **every** binding, not just Python.

## Decision

**Tier-1 platforms (every binding must build, test, and publish on each):**

| Platform | Target triple |
|----------|---------------|
| Linux x86_64 (glibc) | `x86_64-unknown-linux-gnu` (manylinux_2_28 for Python) |
| Linux aarch64 (glibc) | `aarch64-unknown-linux-gnu` (manylinux_2_28 for Python) |
| macOS x86_64 | `x86_64-apple-darwin` |
| macOS aarch64 | `aarch64-apple-darwin` |
| Windows x86_64 (MSVC) | `x86_64-pc-windows-msvc` |

**Bindings × platforms.** Python wheels + NAPI prebuilt + CLI binary
ship per-target artifacts on all five targets at every release; the
Rust crate is source-distributed and the Tier-1 promise for it is
"CI builds + tests on all five," not "we publish five artifacts."
NAPI gap (missing linux-aarch64 + windows) closed in 0.6.0.

**Cross-cite ADR-0.6.0-zerocopy-blob Z-2:** every Tier-1 target above is
little-endian. The Tier-1 list IS the LE-target enumeration that Z-2's
"CI matrix asserts" clause refers to. Adding a big-endian target to
Tier-1 reopens both this ADR and zerocopy-blob.

**Tier-1 gates (per release):**

- Build green on all five.
- Unit + integration tests green on all five.
- Post-publish smoke (per `feedback_release_verification`) on all five.
  Per-binding smoke spec:
  - **Rust crate** — Tier-1 = build + test on all five (no per-target
    artifact). Smoke = `cargo test` against the published crate version
    in a scratch crate on each target.
  - **Python wheel** — `pip install fathomdb==<ver>` from PyPI; run
    `Engine.open(tmp) → write → search → close → process exit` end-to-end.
  - **NAPI prebuilt** — `npm install fathomdb@<ver>` from npm; run the
    same open+write+search+close+exit script in Node.
  - **CLI binary** — download release artifact; `fathomdb --version` +
    `fathomdb open tmp; fathomdb close` exits 0.
  - Detailed checklist lives in followup `release-checklist.md`.

## Out of scope (non-Tier-1)

- musl linux (`*-unknown-linux-musl`) — no current users; revisit if
  Alpine demand surfaces.
- 32-bit anything.
- FreeBSD / OpenBSD / illumos.
- Windows aarch64 — defer until aarch64 Windows runners are GA on GitHub
  Actions.
- Android / iOS — out of scope for this product.

These do not get build coverage and are not promised to work. PRs adding
them to CI must amend this ADR.

## Options considered

**A — Five-target tier-1 across every binding (chosen).** Pros: matches
Python matrix already in place; closes NAPI gap (Graviton + windows users
unblocked); single matrix definition reusable per workflow. Cons: NAPI
build time grows ~2×; aarch64 windows still excluded.

**B — Linux x86_64 only; everything else best-effort.** Pros: fastest CI.
Cons: cross-platform regressions ship (we already had c_char incidents).
Rejected.

**C — Add musl + windows-aarch64 to Tier-1.** Pros: broader reach. Cons:
no current user demand; windows-aarch64 runners not stable; speculative
coverage we will not maintain. Rejected.

## Consequences

- `release.yml` NAPI matrix expanded to add `aarch64-unknown-linux-gnu`
  and `x86_64-pc-windows-msvc`. Tracked as implementation followup
  (Phase 5).
- `test-plan.md` lists per-AC platform coverage; soak/perf gates may
  carve out platform subsets explicitly (e.g. perf gate runs only on
  one reference target — call it out per gate).
- `deps/*.md` records cross-platform support per dep; any dep that
  fails on a Tier-1 target is a blocker, not a workaround.
- Followup `release-checklist.md` adds: "all 5 Tier-1 wheels published
  - smoke-installed before release marked done."
- Memory `feedback_cross_platform_rust` continues to apply at the
  C-interop boundary — this ADR does not relax it.

## Citations

- Existing `release.yml` matrix (Python five-target).
- `feedback_cross_platform_rust` (c_char incident).
- `feedback_release_verification` (post-publish smoke).
