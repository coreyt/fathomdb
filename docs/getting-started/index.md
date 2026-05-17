# Getting Started

> **Pre-release notice.** `0.6.0` has not yet been published to
> crates.io / PyPI / npm. Install commands shown on this site preview
> the post-GA paths; current installs are from source on the
> `0.6.0-rewrite` branch. See
> [release notes — 0.6.0](../release-notes/0.6.0.md) for what ships
> and what is deferred.

## Where to go

- [Quickstart](quickstart.md) — install, open, write, search,
  counters, close, exit. ~5 minutes.
- [Install — Python](../install/python.md)
- [Install — TypeScript / Node.js](../install/typescript.md)
- [Install — Rust](../install/rust.md)

## What ships in 0.6.0

- Five-verb runtime SDK across Python, TypeScript, and Rust:
  `Engine.open`, `write`, `search`, `close`, `admin.configure`.
- Engine-attached instrumentation: `drain`, `counters`,
  `set_profiling`, `set_slow_threshold_ms`, host-logger attach.
- Operator CLI: `fathomdb doctor` (integrity, safe-export, recovery
  info) and `fathomdb recover` (accept-data-loss path). Logical-id
  verbs (`purge_logical_id`, `restore_logical_id`) deferred to
  **0.7.x**.
- Local-first storage on SQLite (FTS5 + `sqlite-vec`).
- Two-axis versioning: workspace lockstep across the
  runtime/binding/CLI crates and the independently versioned
  `fathomdb-embedder-api` trait crate.

## Use the right SDK

Prefer **Python** for production pilots in 0.6.0. The TypeScript SDK
shipped its first working slice on 2026-04-07 and is functionally
covered for the locked surface, but it is the less-mature option.
Rust users consume the `fathomdb` facade crate or the
`fathomdb-cli` operator binary.

## Deferred for 0.6.0

These appear in [release notes — 0.6.0](../release-notes/0.6.0.md):

- Performance gates AC-012, AC-013, AC-019, AC-020 (closing in 0.6.1
  via canonical-runner re-measurement + Pack 7 substrate work).
- `Engine.open` structured open report (Python + TS both drop it;
  surfacing defers to 0.6.1, slice `12-TX-OPENREPORT`).
- Logical-id verbs (deferred to 0.7.x).
- No 0.5.x compatibility shims.
