---
title: Linux AArch64 native release artifacts
date: 2026-08-02
target_release: next versioned release after 0.8.20
status: accepted
---

# Linux AArch64 native release artifacts

## Decision

The next versioned FathomDB release publishes and validates native Linux glibc
artifacts for both x86_64 and AArch64:

- a `manylinux_2_28_aarch64` Python wheel;
- `fathomdb.linux-arm64-gnu.node` in the unscoped
  `fathomdb-linux-arm64-gnu` npm platform package; and
- registry-install smoke tests on a native ARM64 GitHub runner.

The thin main npm package injects both Linux platform packages as exact-version
optional dependencies before publication. macOS, Windows, and Linux musl remain
out of scope and the npm dist-tag remains non-`latest` while coverage is partial.

## Rationale

FathomDB builds successfully on real Linux AArch64 once its default embedder
uses the controlled Candle packages, but source-only success does not make a
PyPI or npm install usable. Native GitHub runner `ubuntu-24.04-arm` builds the
wheel and N-API binary on the architecture users run, avoiding unsupported
cross-compilation assumptions. A branch-runnable no-publish preflight proves
that path before a merge or release is authorized.

## Consequences

`release.yml` builds and stages both Linux architectures before any publish;
the main npm publish waits for both platform packages; and GitHub release
creation waits for both x86_64 and AArch64 registry smokes. The previously
accepted Linux-x64-only release scope is superseded only for Linux AArch64.
