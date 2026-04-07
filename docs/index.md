# fathomdb Documentation

fathomdb is canonical local storage for persistent AI agents. Graph, vector, and full-text search on SQLite.

## Getting Started

- [Getting Started](./getting-started.md) -- prerequisites, building, first database

## Concepts

- [Data Model](./concepts/data-model.md)
- [Temporal Model](./concepts/temporal-model.md)
- [Operational Store](./concepts/operational-store.md)

## Guides

- [Querying](./guides/querying.md)
- [Writing Data](./guides/writing-data.md)

## API Reference

- [Engine](./reference/engine.md)
- [Query](./reference/query.md)
- [Write Builder](./reference/write-builder.md)
- [Admin](./reference/admin.md)
- [Types](./reference/types.md)

## Operations

- [Admin Operations](./operations/admin-operations.md) -- integrity checks, provenance, export, retention, projections
- [Vector Regeneration](./operations/vector-regeneration.md) -- regenerating embeddings after recovery or model updates
- [Telemetry](./operations/telemetry.md) -- resource usage collection and profiling levels

## Internal Design

Detailed design documents, architecture notes, and implementation plans are in `dev/`.
