# fathomdb

`fathomdb` is a local datastore for persistent AI agents.

It is designed for agent systems that need more than a pile of documents: they
need a durable world model, multimodal recall, provenance, replay, correction,
and high-trust local operation. Technically, `fathomdb` is a **graph/vector/FTS
shim in front of SQLite**. SQLite remains the canonical store; `fathomdb` adds
an agent-friendly query compiler, derived search projections, and a governed
write path.

## What Problem It Solves

Local AI agents need to:

- remember people, projects, meetings, tasks, and evolving context over time
- retrieve across structure, relationships, text, semantics, and time
- preserve why a fact, task, or action exists
- recover cleanly from bad agent reasoning or projection drift
- run locally without a multi-service database stack

`fathomdb` is aimed at that problem space.

## Core Idea

The current design has three key commitments:

1. **SQLite is the canonical store**
   - no custom storage engine
   - no split authority across multiple databases
   - one durable local file

2. **Graph, FTS, and vector access are derived capabilities**
   - canonical state lives in relational tables
   - FTS and vector search are projection layers
   - projection rebuild is a normal admin operation, not an emergency hack

3. **The engine is built for agent workloads**
   - deterministic SDK-driven query building
   - inside-out query planning and top-k pushdown
   - append-oriented history with provenance and recovery tooling

## Architecture Snapshot

The current architecture centers on:

- a **graph-friendly canonical backbone** in SQLite
- explicit **`logical_id` vs `row_id`** versioning for append-oriented state
- a **`chunks`** layer for text/vector projection
- derived **FTS5** and **`sqlite-vec`** search surfaces
- a **single-writer execution model** with WAL-backed readers
- a **Rust core engine**

At the query layer, the compiler works from the inside out:

- start from the narrowest indexed candidate set
- resolve vector/FTS hits through chunks
- join into canonical graph state
- apply late JSON and relational filtering

At the write layer, the engine uses:

- pre-flight enrichment before write lock acquisition
- `BEGIN IMMEDIATE` only when payloads are ready
- atomic canonical writes plus required projection updates
- optional semantic backfills for heavier background workloads

## Language Split

The implementation split is:

- **Rust for the engine**
  - AST compiler
  - SQLite execution layer
  - single-writer core
  - projection/write-path logic close to SQLite internals

- **Go for separate surrounding services**
  - sync and backup daemon
  - remote ingestion worker
  - gRPC/HTTP gateway
  - operational tooling and observability utilities
  - external-system connector services

- **Python and TypeScript for SDKs**
  - language-facing interfaces over the Rust core
  - Python bindings are implemented today
  - TypeScript remains future work

## Integrity And Recovery

The design treats recovery as a first-class feature. It explicitly plans for:

- **physical corruption**
  - recover canonical tables, then rebuild projections
- **logical corruption**
  - deterministically rebuild FTS projections and restore vector capability
  - regenerate vector embeddings through the admin-owned regeneration workflow
- **semantic corruption**
  - rollback or excise bad agent outputs by time window or `source_ref`

This is possible because the design separates canonical state from derived
projections and keeps provenance directly attached to canonical rows.

## Current Status

The repository is beyond the initial scaffold stage:

- a Rust workspace with `fathomdb`, `fathomdb-schema`, `fathomdb-query`, and
  `fathomdb-engine`
- a sibling Go module at `go/fathom-integrity`
- Python bindings under `python/fathomdb`
- vector-capable Python builds with `sqlite-vec`
- a Python example harness that exercises write/read/admin flows in baseline and
  vector modes
- response-cycle feedback across Rust, Python, and Go/CLI
- GitHub Actions CI for Rust, Go, and Python
- automated repair commands for:
  - duplicate active logical IDs
  - broken runtime FK chains
  - orphaned chunks
- an admin-owned vector regeneration workflow driven by application-supplied
  TOML or JSON contract files

See [dev/production-readiness-checklist.md](./dev/production-readiness-checklist.md)
for the production gate and
[dev/repair-support-contract.md](./dev/repair-support-contract.md) for the
exact repair and recovery boundary. The current repo position is:

- production-ready within the documented support contract
- explicit about what is canonical, what is projection material, and what
  recovery guarantees currently apply

The main design docs are:

- [USER_NEEDS.md](./dev/USER_NEEDS.md)
- [ARCHITECTURE.md](./dev/ARCHITECTURE.md)
- [preliminary-solution-design.md](./dev/preliminary-solution-design.md)
- [ARCHITECTURE-deferred-expansion.md](./dev/ARCHITECTURE-deferred-expansion.md)
- [dbim-playbook.md](./dev/dbim-playbook.md)
- [db-integrity-management.md](./dev/db-integrity-management.md)
- [experts-view.md](./dev/experts-view.md)
- [0.1_IMPLEMENTATION_PLAN.md](./dev/0.1_IMPLEMENTATION_PLAN.md)

## Developer Setup

Bootstrap a local development environment with:

```bash
./scripts/developer-setup.sh
```

That script installs the baseline Rust and Go toolchains used by this
repository, installs a project-local `sqlite3` binary for this repo, adds the
required shell `PATH` entries, and installs `cargo-nextest`.

SQLite policy for local development:

- minimum supported SQLite version: `3.41.0`
- repo-local development target: `3.46.0`

The setup script installs the repo-local SQLite under `.local/` and prepends it
to `PATH` so local CLI-driven workflows do not depend on an older system
`sqlite3`.

## What Is In Scope For v1

The current MVP direction is to prove the core engine shape first:

- canonical SQLite graph backbone
- `chunks`, `fts_nodes`, and `vec_nodes`
- AST compiler with inside-out planning
- single serialized writer
- atomic canonical writes plus required projections
- minimal repair/admin operations:
  - rebuild projections
  - safe export
  - trace by `source_ref`
  - excise bad lineage

Deferred expansions are preserved in
[ARCHITECTURE-deferred-expansion.md](./dev/ARCHITECTURE-deferred-expansion.md)
rather than being lost.

## Non-Goals

`fathomdb` is not trying to:

- replace SQLite with a custom engine
- become a generic distributed database
- depend on a cloud-first control plane
- hide recovery behind backup-only workflows

## Architecture Decisions

Current open-ended architecture choices that go beyond the shipped v0.1 support
contract are documented separately, for example:

- [arch-decision-vector-embedding-recovery.md](./dev/arch-decision-vector-embedding-recovery.md)
