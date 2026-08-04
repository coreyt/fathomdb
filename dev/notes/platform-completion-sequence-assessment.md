# Finding — cross-platform completion sequence assessment

> **Assessed:** 2026-08-01
> **Purpose:** Preserve the implementation dependencies for completing Linux,
> macOS, and Windows delivery without assigning the work to a particular
> release. This is a supporting assessment, not a requirements document or a
> substitute for the release board.

## Scope and present boundary

The released-artifact workflow currently builds and publishes only the Linux
x64 Python wheel and the `linux-x64-gnu` npm native package. Its npm dist-tag
is intentionally non-`latest` while that coverage is partial. The deferred
Python and napi matrix entries for macOS x64, macOS arm64, and Windows x64 are
preserved as comments in `.github/workflows/release.yml`.

This is narrower than ordinary CI coverage. `ci.yml` already runs Rust
workspace tests on Windows and macOS, and its wheel-size job builds all of the
Linux, macOS, and Windows targets. Those checks demonstrate useful build and
test coverage; they do not prove that artifacts can be installed from the
public registries on those operating systems.

Linux arm64 is intentionally outside this sequence. It needs an ARM runner or
cross-compilation support and remains a separately deferred platform.

## Findings

1. **Platform correctness precedes platform expansion.** TC-91 is a real
   engine correctness defect, not merely a Windows CI issue: a failed
   projection-worker commit can be discarded and lead to duplicate embeds. The
   Windows signal makes it particularly visible, but the failure mode also
   exists on Linux. A cross-platform publish must therefore be gated on the
   corrective engine change and on trustworthy, repeatedly green workspace CI.

2. **The loader contract is already designed for the missing platforms.**
   `src/ts/src/platform.ts` maps the required labels:
   `darwin-arm64`, `darwin-x64`, and `win32-x64-msvc`. The release tree contains
   only `src/ts/npm/linux-x64-gnu/package.json`, however. Each missing label
   needs a sibling package with correct `os`, `cpu`, `main`, and shipped binary
   entries.

3. **The main-package wiring scales without a new mechanism.**
   `scripts/release/npm-inject-optional-deps.sh` discovers every committed
   platform-package directory and injects matching optional dependencies at
   publish time. Adding platform directories is therefore sufficient for the
   main package to name all generated binaries; it must still be proven with a
   multi-platform test fixture.

4. **Publication ordering is a correctness property.** The native packages
   must publish successfully before the thin main npm package, because its
   injected optional dependencies reference their exact version. Every native
   build must also feed the existing all-builds-passed barrier before any
   registry publication begins.

5. **Build success is not the end-to-end acceptance signal.** The existing
   post-publish smoke job runs only on Ubuntu. macOS and Windows require fresh
   registry installation of the Python wheel and npm package, followed by the
   real open, close, and process-exit smoke. Apple Silicon needs that check on
   an actual arm64 execution environment, not just an artifact cross-build.

## Completion sequence

### 1. Establish a trustworthy functional baseline

- Correct TC-91 with a red test that asserts the intended worker-commit and
  duplicate-embed behavior.
- Preserve full Rust workspace execution on Linux, macOS, and Windows. Linux
  and macOS should expose all failures, rather than hiding later failures after
  the first one.
- Require repeated green CI runs that execute the relevant jobs. A skipped job
  is neither validation nor a substitute for a green platform run.

### 2. Make the package topology complete

- Upgrade the napi build toolchain before validating the expanded matrix, so
  the validated binding version is the one eventually shipped.
- Add the macOS arm64, macOS x64, and Windows x64 npm package manifests and
  assert their metadata, binary names, and loader labels in tests.
- Confirm npm organization ownership and trusted-publishing configuration for
  each new scoped platform package before a publication attempt.

### 3. Prove artifact construction on each target

- Enable the deferred Python-wheel and napi matrix rows.
- Keep the existing all-builds-passed barrier as the point at which every
  Python, napi, and Rust build must succeed before publication can start.
- Replace the Linux-only workflow-scope assertion with an exact target-matrix
  assertion and add regression coverage for artifact names and uploads.
- Recalibrate the existing per-platform wheel-size baselines from measured
  artifacts where they are still provisional.

### 4. Publish in dependency order and verify from registries

- Publish each platform npm package before the thin main package, then check
  that the published main package contains the complete, version-locked
  optional-dependency set.
- Install the published Python and npm artifacts on Linux, macOS, and Windows
  from their registries; execute open, close, and exit on each host.
- Keep registry idempotency and provenance behavior for each newly added npm
  package, not only the existing Linux package.

### 5. Promote only after complete evidence

- Verify all supported platform artifacts and the corresponding registry
  smokes before changing the npm default dist-tag.
- Retain a separate explicit authorization for any real registry publication;
  a successful build or dry-run is evidence, not publication authority.

## Source pointers

- `.github/workflows/release.yml` — deferred build rows, build barrier, npm
  publication order, and Linux-only post-publish smoke.
- `.github/workflows/ci.yml` — Windows/macOS Rust legs and cross-platform
  wheel-size matrix.
- `src/ts/src/platform.ts` — native loader triple labels.
- `scripts/release/npm-inject-optional-deps.sh` — publish-time optional
  dependency injection.
- `scripts/tests/test_release_workflow_scope.sh` — current Linux-only
  workflow-scope assertion that must change with the matrix.
- `dev/plans/release-state-0.8.20.json` — current ruled platform schedule and
  explicit publish gate; it remains authoritative over this note.
