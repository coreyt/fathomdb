# Compatibility

Supported platforms, toolchains, and version-alignment policy for
the 0.6.0 release.

## Supported Python versions

`3.10`, `3.11`, `3.12`. Wheels published for each via the
`release.yml` `build-python` matrix.

## Supported Node versions

Node **18** or later. CI runs on Node **22**.

## Supported Rust toolchain

Rust **stable**. Specific minimum-supported-rust-version is pinned at
release time; the `release.yml` toolchain step uses `dtolnay/rust-toolchain@stable`.

## Supported platforms

Same matrix across Python wheels, napi `.node` binaries, and Rust
crates (per `release.yml`):

| OS      | Architecture           |
| ------- | ---------------------- |
| Linux   | `x86_64-unknown-linux-gnu`  |
| Linux   | `aarch64-unknown-linux-gnu` |
| macOS   | `x86_64-apple-darwin`       |
| macOS   | `aarch64-apple-darwin`      |
| Windows | `x86_64-pc-windows-msvc`    |

Linux wheels are manylinux 2_28. Other platforms outside this matrix
are unsupported in 0.6.0.

## SQLite + sqlite-vec

- Engine uses SQLite **FTS5** + the
  [`sqlite-vec`](https://github.com/asg017/sqlite-vec) extension.
- Wheels and platform `.node` binaries statically link a compatible
  build of `sqlite-vec`.
- Rust + source-build users need a working SQLite + `sqlite-vec`
  available to the loader.

## Versioning — two axes

`0.6.0` follows two-axis versioning:

- **Axis W (workspace lockstep)** — the six runtime / binding / CLI
  crates plus the Python and TypeScript packages all carry the same
  workspace version. `scripts/set-version.sh --check-files` enforces
  this pre-publish; the pre-push hook runs the check.
- **Axis E (`fathomdb-embedder-api`)** — the embedder-trait crate is
  versioned independently per
  `ADR-0.6.0-embedder-protocol`. Bumping a binding does not force an
  embedder-api bump.

This decouples embedder-protocol stability from binding cadence.

## 0.5.x compatibility

**No 0.5.x compatibility shims or migrations.** 0.6.0 is a rewrite.
Clients on 0.5.x follow a separate migration guide (not provided in
0.6.0). Do not point 0.6.0 binaries at 0.5.x databases.

## Performance posture — deferred gates

Four performance ACs are deferred for 0.6.0 (HITL re-confirmed
2026-05-17 per Phase 12-P decision). Clients evaluating fathomdb for
perf-sensitive workloads should plan accordingly:

| AC      | Surface                          | Closure target |
| ------- | -------------------------------- | -------------- |
| AC-012  | text query latency on FTS5 (p50 ≤ 20 ms / p99 ≤ 150 ms) | 0.6.1 (canonical-runner re-measurement) |
| AC-013  | vector retrieval latency (p50 ≤ 50 ms / p99 ≤ 200 ms)   | Pack 7 (batched-insert vec0)            |
| AC-019  | mixed-retrieval stress tail                             | Pack 7 (inherits AC-013)                |
| AC-020  | N=8 concurrent reader scaling (architectural gap)       | Pack 7 vendor-SQLite work                |

Engine surfaces these as documented gates, not weakened ones. See
`dev/test-plan.md` § Current Perf Attribution and
[release notes — 0.6.0 § Performance gates](../release-notes/0.6.0.md).

## API surface deferrals

- `Engine.open` structured open report dropped by both bindings;
  surfacing defers to 0.6.1 (slice `12-TX-OPENREPORT`).
- Logical-id verbs (`purge_logical_id`, `restore_logical_id`)
  deferred to 0.7.x.

See [release notes — 0.6.0](../release-notes/0.6.0.md) for the full
deferred-items disclosure.
