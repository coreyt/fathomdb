# Vector Regeneration

## Overview

Vector regeneration recomputes embedding rows for one node kind from the
current canonical chunk set. Since 0.5.0, vector rows live in per-kind
sqlite-vec tables derived from `kind` (for example, `Document` maps to a
sanitized `vec_<kind>` table), not in the old global `vec_nodes_active`
table. Use regeneration to backfill embeddings after physical recovery,
to rebuild vectors after changing models, or to refresh vectors after
chunking or preprocessing policy changes.

Regeneration replaces the target kind's vector rows atomically: a run
either fully applies or leaves both the previous contract row and the
existing vector contents unchanged.

## Architectural Invariant: The Embedder Owns Identity

As of 0.4.0, **vector identity is the embedder's responsibility, not
the regeneration config's**. The `VectorRegenerationConfig` carries
*where* vectors live and *how* to chunk and preprocess them. The
embedder that was attached to the engine at `Engine::open` time carries
*what* model produces them: dimension, model identity, model version,
and normalization policy all come from the embedder's `identity()`.

This means the read-path (`search()` with a natural-language query) and
the regen-path share a single embedder instance, so the resulting
profile's identity is stamped directly from `QueryEmbedder::identity`
and cannot drift from what `search()` will match against at read time.
The full rationale lives in
`dev/notes/project-vector-identity-invariant.md`.

## Quick Start (Rust)

Open the engine with a concrete embedder choice, build a
`VectorRegenerationConfig`, and call
`Engine::regenerate_vector_embeddings`.

```rust
use std::sync::Arc;
use fathomdb::{EmbedderChoice, Engine, EngineOptions};
use fathomdb_engine::VectorRegenerationConfig;

let engine = Engine::open(
    EngineOptions::new("/path/to/fathom.db")
        .with_embedder(EmbedderChoice::Builtin),
)?;

let config = VectorRegenerationConfig {
    kind: "Document".to_string(),
    profile: "default".to_string(),
    chunking_policy: "per_chunk".to_string(),
    preprocessing_policy: "trim+lowercase".to_string(),
};

let report = engine.regenerate_vector_embeddings(&config)?;
println!("regenerated {} rows", report.regenerated_rows);
```

For a caller-supplied embedder (cloud API, fine-tuned model, etc.),
use `EmbedderChoice::InProcess(Arc::new(my_embedder))` instead of
`EmbedderChoice::Builtin`.

## Quick Start (Python)

The Python wrapper mirrors the Rust surface. The engine is opened with
an `embedder` argument, and regeneration runs through the admin client.

```python
from fathomdb import Engine, VectorRegenerationConfig

db = Engine.open("/path/to/fathom.db", embedder="builtin")

config = VectorRegenerationConfig(
    kind="Document",
    profile="default",
    chunking_policy="per_chunk",
    preprocessing_policy="trim+lowercase",
)

report = db.admin.regenerate_vector_embeddings(config)
print(f"regenerated {report.regenerated_rows} rows")
```

## `VectorRegenerationConfig` Fields

The config carries only destination and preprocessing metadata. Every field is
required.

| Field | Description |
|---|---|
| `kind` | Node kind to regenerate. The engine derives the target sqlite-vec table name from this kind. |
| `profile` | Logical profile name recorded with the vector contract. |
| `chunking_policy` | Describes how text was split into chunks (e.g. `"per_chunk"`). Persisted into the contract for audit. |
| `preprocessing_policy` | Describes the text normalization applied before embedding (e.g. `"trim+lowercase"`). Persisted into the contract for audit. |

The legacy fields `table_name`, `model_identity`, `model_version`,
`dimension`, `normalization_policy`, and `generator_command` have been removed.
Configs serialized from older releases that still carry any of these fields
will fail to deserialize with a `deny_unknown_fields` error; update the config
and rebuild it against the engine's open-time embedder instead.

## Error Handling

`Engine::regenerate_vector_embeddings` returns
`EngineError::EmbedderNotConfigured` when the engine was opened with
`EmbedderChoice::None` (the default). The fix is to reopen the engine
with `EmbedderChoice::Builtin` or
`EmbedderChoice::InProcess(Arc::new(...))`.

In Python the equivalent surfaces as `FathomError` with the message
`"embedder not configured: open the Engine with a non-None
EmbedderChoice to regenerate vector embeddings"`.

Other failures — unsupported `sqlite-vec` capability, invalid profile
names, an embedder that errors mid-run, or a chunk snapshot that
changes during regeneration — propagate as the corresponding
`EngineError` variant and leave both the previously applied contract
row and the current vector contents unchanged.

## Custom Embedders

To regenerate with a non-`Builtin` embedder, implement the
`fathomdb_engine::QueryEmbedder` trait (see the trait's rustdoc for the
required methods and identity contract) and pass the resulting object
via `EmbedderChoice::InProcess(Arc::new(my_embedder))` at
`Engine::open` time. The same embedder is then used by both the
read-path vector branch and the regen-path, so identity stays
consistent.

## Migration Notes

The 0.4.0 and 0.5.0 releases both changed the regeneration surface. Concrete
before/after for the most common call sites:

**Rust config shape.** The 0.3.x config carried model identity inline:

```rust
// Removed older shape
VectorRegenerationConfig {
    profile: "default".into(),
    table_name: "vec_nodes_active".into(),
    model_identity: "text-embedding-local".into(),
    model_version: "2026-03-01".into(),
    dimension: 1536,
    normalization_policy: "l2".into(),
    chunking_policy: "per_chunk".into(),
    preprocessing_policy: "trim+lowercase".into(),
    generator_command: vec!["/usr/local/bin/generate".into(), "--json".into()],
}
```

In the current shape, identity is supplied by the open-time embedder and the
table name is derived from `kind`; use the four fields shown in the Quick Start
above.

**Python method name.** The 0.3.x surface exposed a policy variant
(`admin_client.regenerate_vector_embeddings_with_policy(config,
policy)`) alongside the basic call. The policy variant has been
removed in 0.4.0. Use `admin_client.regenerate_vector_embeddings(config)`.

**Subprocess generators.** The 0.3.x subprocess generator protocol
(stdin/stdout JSON, operator-controlled executable trust, timeout and
byte caps) has been removed. Users who previously shelled out to a
Python or Node subprocess should implement a small
`SubprocessEmbedder` adapter against the `QueryEmbedder` trait and
pass it in via `EmbedderChoice::InProcess`. The trust policy then
lives in your embedder's spawn code rather than in the engine surface.

**Go `fathom-integrity` CLI.** The `fathom-integrity regenerate-vectors`
subcommand and its bridge counterpart (`RegenerateVectorEmbeddings`)
have been removed. There is no direct replacement — the admin bridge
protocol cannot carry an embedder reference across process boundaries,
which is exactly the drift hazard the new design eliminates. Call
`Engine::regenerate_vector_embeddings` from the native Rust API or from
the Python wrapper instead.

## Why the Redesign?

The old subprocess-generator contract conflated identity strings
(`model_identity`, `model_version`, `dimension`) with a separately
configured executable, which made model drift structurally possible:
the config could claim any identity the operator typed, regardless of
what the generator actually computed, and the read path had no way to
verify that the vectors it was comparing against had been produced by
the same model it was embedding queries with. The new shape makes
drift impossible: the embedder owns identity end-to-end, and the same
instance services both the read-path and the regen-path. See
`dev/notes/project-vector-identity-invariant.md` for the full design
discussion.

## Related Docs

- [Admin Operations](./admin-operations.md) — other admin surfaces
- [Home](../index.md)
- `dev/notes/project-vector-identity-invariant.md` — invariant rationale (internal design doc)
- `dev/ARCHITECTURE.md` — system design and module layout (internal design doc)
