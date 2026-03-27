# Design: Windows-Compatible Executable Trust Policy and Support Boundary

## Purpose

Resolve review finding 2:

- remove the unconditional Unix-only dependency introduced in
  `crates/fathomdb-engine/src/admin.rs`
- determine and document how `fathomdb` continues to support Windows

The immediate regression is compilation failure on non-Unix targets due to
`std::os::unix::fs::PermissionsExt` in shared production code. The larger issue
is that executable-trust enforcement now has platform-specific behavior without
an explicit platform design.

## Decision

Keep Windows as a supported build/test target for the Rust engine and Go
integrity tooling.

Support boundary for this phase:

- Windows must compile and run the non-`sqlite-vec` Rust and Go test suites
- vector capability may remain feature-gated or Unix-scoped where
  `sqlite-vec`/tooling packaging is not yet proven on Windows
- executable trust policy must remain enforceable on Windows, not silently
  disabled

Rejected alternatives:

- declare `fathomdb-engine` Unix-only
- keep Linux-only CI and treat Windows breakage as acceptable
- disable the world-writable executable check everywhere for portability

## Problem

The current implementation assumes Unix file-permission bits in shared code:

- import of `std::os::unix::fs::PermissionsExt`
- use of `permissions().mode() & 0o002`

That causes two problems:

1. the crate no longer compiles on Windows
2. executable-trust semantics are not defined per platform

Absolute-path enforcement, allowlisted roots, and reduced environment
inheritance are cross-platform concepts. “Reject world-writable executable” is
not, because Windows uses ACLs rather than Unix permission bits.

## Goals

1. Restore cross-platform compilation.
2. Preserve secure-by-default operator policy on both Unix and Windows.
3. Make platform differences explicit in one helper layer instead of scattering
   `cfg(...)` branches through `admin.rs`.
4. Add CI coverage that would catch this regression in the future.

## Non-Goals

This design does not:

- promise full Windows parity for `sqlite-vec` e2e workflows in the same change
- redesign Go bridge binary validation in this document, except where needed for
  consistency later
- add macOS-specific policy beyond what falls out of the Unix implementation

## Design

### 1. Move executable trust checks into a platform module

Create a small helper module, for example:

- `crates/fathomdb-engine/src/executable_trust.rs`

Expose one cross-platform API:

```rust
fn validate_generator_executable(
    executable: &Path,
    policy: &VectorGeneratorPolicy,
) -> Result<(), VectorRegenerationFailure>
```

`admin.rs` stops importing any Unix-only traits directly.

Shared responsibilities in the helper:

- absolute-path enforcement
- existence / metadata lookup
- allowlisted-root enforcement
- dispatch to platform-specific “broadly writable” check

### 2. Define platform-specific writable semantics

#### Unix

Keep the existing intent:

- reject when the executable is writable by “other”
- implementation may continue using `PermissionsExt::mode() & 0o002`

This code lives behind `#[cfg(unix)]`.

#### Windows

Implement an ACL-based “broadly writable executable” check behind
`#[cfg(windows)]`, using a Windows API crate such as `windows-sys`.

Definition for this phase:

- reject when the file grants write-equivalent access to broad principals such
  as `Everyone`, `Authenticated Users`, or `Users`
- acceptable write-equivalent rights include `FILE_WRITE_DATA`,
  `FILE_APPEND_DATA`, `FILE_GENERIC_WRITE`, `GENERIC_WRITE`, or write DAC/owner
  rights that effectively let non-admin broad principals modify the executable

Failure behavior:

- if Windows ACL inspection fails while
  `reject_world_writable_executable == true`, return a clear operator-facing
  error instead of silently allowing execution

This keeps the policy enforceable and avoids a false sense of safety.

### 3. Keep support explicit in tests and CI

Add Windows CI jobs:

- `cargo build --workspace`
- `cargo test --workspace --exclude fathomdb-engine --exclude fathomdb` only if
  some vector-dependent crates still block, otherwise `cargo test --workspace`
- Go unit tests under `go/fathom-integrity/internal/...`

Preferred boundary:

- Windows Rust CI must at minimum cover `fathomdb-schema`,
  `fathomdb-query`, and `fathomdb-engine` non-`sqlite-vec` paths
- existing Linux jobs continue to own `sqlite-vec` and e2e recovery coverage

If the full Rust workspace already passes on Windows without `sqlite-vec`, use
the full workspace test job instead of a reduced matrix.

### 4. Document the support contract

Update docs to say:

- the core engine and integrity tool support Windows builds
- vector regeneration policy is cross-platform
- Windows vector-feature/e2e coverage remains limited to the supported feature
  set in CI
- Unix and Windows both enforce executable trust, but the writability check is
  platform-specific

## Implementation Changes

Primary files:

- `crates/fathomdb-engine/src/admin.rs`
- new platform helper module under `crates/fathomdb-engine/src/`
- `crates/fathomdb-engine/Cargo.toml`
- `.github/workflows/ci.yml`
- operator docs if they mention platform assumptions

Concrete edits:

- remove `std::os::unix::fs::PermissionsExt` from shared `admin.rs`
- add target-specific helper implementation:
  - `#[cfg(unix)]` permission-bit version
  - `#[cfg(windows)]` ACL inspection version
- add target-specific dependency for Windows ACL inspection
- keep all root and environment checks platform-neutral
- add Windows CI jobs and keep Linux vector/e2e jobs in place

## Test Plan

Rust tests:

- shared tests for absolute-path and allowlisted-root validation on all targets
- Unix-only test for rejecting an `0o777` executable
- Windows-only test that creates an executable with a permissive ACL and asserts
  rejection when the policy requires it
- Windows-only test that a normally owned temp executable passes validation

CI tests:

- `windows-latest` Rust build job must fail if a Unix-only import reappears in
  shared code
- `windows-latest` Go test job must run the internal packages
- Linux continues to run vector-enabled and e2e suites

## Acceptance Criteria

- `fathomdb-engine` no longer imports Unix-only permission APIs in shared code
- the workspace has explicit Windows CI coverage
- executable trust policy remains enforced on Windows rather than silently
  degraded
- Unix vector behavior is unchanged
- the support boundary for Windows versus Unix vector e2e coverage is documented
