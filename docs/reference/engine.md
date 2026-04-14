# Engine

The main entry point for interacting with a fathomdb database. See the
[Getting Started](../getting-started.md) guide for first-use examples.

::: fathomdb.Engine
    options:
      members_order: source
      heading_level: 2

## Vector regeneration

The `regenerate_vector_embeddings` entry point is exposed on the
admin handle rather than directly on `Engine`, mirroring the other
admin surfaces. Access it through `engine.admin.regenerate_vector_embeddings(config)`
and see
[`AdminClient.regenerate_vector_embeddings`](./admin.md) for the full
signature. The Rust binding (`Engine::regenerate_vector_embeddings`)
is a thin convenience wrapper that uses the embedder attached at
`Engine::open` time via `EmbedderChoice` — calling it on an engine
opened with `embedder=None` raises
[`FathomError`](./types.md#errors) (`EngineError::EmbedderNotConfigured`
in Rust). See
[Vector Regeneration](../operations/vector-regeneration.md) for the
full walkthrough, migration notes from 0.3.x, and custom-embedder
guidance.
