# FathomDB Public Docs

FathomDB is a local-first retrieval and graph-oriented data system
designed for application and agent workloads. This site is the public
documentation source for users, operators, and SDK consumers.

> **Status: 0.8.21, pre-1.0 beta.** This site documents the **0.8.21**
> surface. FathomDB is pre-1.0: the surface is beta and may change
> between micro releases, and 0.8.21 carries breaking changes relative
> to 0.8.9 (typed `SearchHit.id`, mandatory `source_id` on canonical
> writes, RRF ranking). **v0.8.21 is published** to crates.io / PyPI /
> npm. Native Python and npm artifacts cover Linux x86_64/glibc and Linux
> AArch64/glibc; npm installs use its `next` dist-tag. See
> [Install](install/python.md) and the
> [CHANGELOG](https://github.com/coreyt/fathomdb/blob/main/CHANGELOG.md).

## Start here

- [Getting Started](getting-started/index.md) — pick your SDK + land
  on the [quickstart](getting-started/quickstart.md).
- [Install — Python](install/python.md) /
  [TypeScript](install/typescript.md) /
  [Rust](install/rust.md).
- [Quickstart](getting-started/quickstart.md) — install, open, write,
  search, counters, close, exit.

## Reference

- [Reference — overview](reference/index.md)
- [Python API](reference/python-api.md)
- [TypeScript API](reference/typescript-api.md)
- [CLI](reference/cli.md)
- [Errors](reference/errors.md)
- [Config](reference/config.md)

## Background

- [Concepts](concepts/index.md) — engine lifecycle, five-verb surface,
  canonical rows + projections, embedder model, recovery surface.
- [Compatibility](compatibility/index.md) — supported platforms,
  toolchains, two-axis versioning, performance posture.
- [Positions](positions/index.md) — consumer-relevant technical
  positions (SDK parity, recovery surface, tokenizer policy,
  embedder identity).
- [Release notes](release-notes/0.8.0.md) — historical per-release
  pages (0.6.0, 0.6.1, 0.8.0). Changes from 0.8.9 onward are recorded
  in the repo
  [CHANGELOG](https://github.com/coreyt/fathomdb/blob/main/CHANGELOG.md).

## Guides and operations

- [Guides](guides/index.md) — hybrid search + filtering, structured
  search hits, retrieve-by-id.
- [Operations](operations/index.md) — including
  [erasure](operations/erasure.md): what `erase_source` / `purge`
  guarantee and what they do not.
