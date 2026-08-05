# FathomDB

FathomDB is a local-first retrieval and graph-oriented data system for
application and agent workloads. It embeds SQLite (FTS5 + `sqlite-vec`) and
ships one engine behind three SDKs — Python, TypeScript and Rust — plus an
operator CLI.

- Hybrid retrieval: a vector branch and an FTS5 branch fused by Reciprocal Rank
  Fusion, with an optional CPU cross-encoder rerank and an optional graph-BFS
  third arm.
- Governed, allowlisted command surface with a single typed error taxonomy
  shared 1:1 across the bindings.
- Bitemporal-ish record model: transaction-time supersession, a world-time
  validity window on nodes, and edge `t_valid` / `t_invalid`.
- Lifecycle and erasure verbs in the SDK — `transition`, `purge`,
  `erase_source` — so a consumer with no CLI on `PATH` can still discharge a
  deletion obligation.
- Optional in-process default embedder (`bge-small-en-v1.5`, pure Rust).

**Status: 0.8.21, pre-1.0 beta.** The surface may change between micro
releases. **v0.8.21 is published** to crates.io, PyPI, and npm; native Python
and npm artifacts cover Linux x86_64/glibc and Linux AArch64/glibc. The main npm
package is on the `next` dist-tag.
Licensed **MIT** (see `LICENSE`).

Public documentation: `docs/` (built with `mkdocs build --strict`).
Changes since 0.8.9: `CHANGELOG.md`.

Repository layout:

- `docs/` contains public MkDocs source and client-facing technical positions.
- `dev/` contains internal engineering material: requirements, architecture,
  ADRs, subsystem design, interface contracts, and planning notes.
- `src/` contains implementation roots and unit-test-adjacent code.
- `test/` contains cross-language, smoke, fixture, and performance assets that
  are not package-local unit tests.

Implementation roots:

- Nine Rust workspace members live under `src/rust/crates/`
- Python package root lives under `src/python/`
- TypeScript package root lives under `src/ts/`

Start here:

- Agent instructions: `AGENTS.md` — the canonical operating manual for AI
  coding agents; read it first if you are one
- Public docs: `docs/index.md`
- Internal docs index: `dev/README.md` — and from there the release
  schedule-of-record and the live board under `dev/plans/`
- Workspace checks: `scripts/check.sh`

Common commands:

```bash
cargo check --workspace
pip install -e src/python/
cd src/ts && npm install
mkdocs build --strict
```
