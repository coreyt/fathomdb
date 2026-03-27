# Vector Regeneration

## Purpose

`fathomdb` supports Choice C vector recovery: embeddings are treated as derived
data that can be regenerated after recovery.

The admin tool owns regeneration. The application supplies a TOML or JSON
contract that tells `fathomdb`:

- which vector profile to target
- what model/config metadata applies
- which external generator command to run

The contract is persisted in the database so recovery-time vector regeneration
has durable metadata in `vector_embedding_contracts`.

## When To Use It

Use vector regeneration when:

- a database was physically recovered and `vec_nodes_active` is empty or stale
- embeddings need to be rebuilt for the current chunk set
- the application wants `fathomdb` admin tooling to orchestrate embedding
  rebuilds while the application continues to own the actual embedding model

## Command

Run regeneration through `fathom-integrity`:

```bash
fathom-integrity regenerate-vectors \
  --db /path/to/fathom.db \
  --bridge /path/to/fathomdb-admin-bridge \
  --config /path/to/vector-regeneration.toml
```

Required flags:

- `--db`: database to regenerate
- `--bridge`: admin bridge binary
- `--config`: TOML or JSON vector regeneration contract

## Configuration Contract

The config must include:

- `profile`
- `table_name`
- `model_identity`
- `model_version`
- `dimension`
- `normalization_policy`
- `chunking_policy`
- `preprocessing_policy`
- `generator_command`

### TOML Example

```toml
profile = "default"
table_name = "vec_nodes_active"
model_identity = "text-embedding-local"
model_version = "2026-03-01"
dimension = 1536
normalization_policy = "l2"
chunking_policy = "per_chunk"
preprocessing_policy = "trim+lowercase"
generator_command = [
  "/usr/local/bin/generate-embeddings",
  "--model",
  "text-embedding-local",
  "--format",
  "json"
]
```

### JSON Example

```json
{
  "profile": "default",
  "table_name": "vec_nodes_active",
  "model_identity": "text-embedding-local",
  "model_version": "2026-03-01",
  "dimension": 1536,
  "normalization_policy": "l2",
  "chunking_policy": "per_chunk",
  "preprocessing_policy": "trim+lowercase",
  "generator_command": [
    "/usr/local/bin/generate-embeddings",
    "--model",
    "text-embedding-local",
    "--format",
    "json"
  ]
}
```

## Generator Protocol

`fathomdb` executes the configured generator command and sends JSON on stdin.

The input contains:

- profile/table metadata
- model/config metadata
- the current active chunk set

Input shape:

```json
{
  "profile": "default",
  "table_name": "vec_nodes_active",
  "model_identity": "text-embedding-local",
  "model_version": "2026-03-01",
  "dimension": 1536,
  "normalization_policy": "l2",
  "chunking_policy": "per_chunk",
  "preprocessing_policy": "trim+lowercase",
  "chunks": [
    {
      "chunk_id": "chunk-1",
      "node_logical_id": "doc-1",
      "kind": "Document",
      "text_content": "budget discussion",
      "byte_start": null,
      "byte_end": null,
      "source_ref": "source-1",
      "created_at": 1743100000
    }
  ]
}
```

The generator must return JSON on stdout in this shape:

```json
{
  "embeddings": [
    {
      "chunk_id": "chunk-1",
      "embedding": [0.1, 0.2, 0.3]
    }
  ]
}
```

Rules:

- every active chunk must have exactly one returned embedding
- embedding length must match `dimension`
- duplicate `chunk_id` outputs are invalid
- malformed JSON or generator failure causes the command to fail

## What Gets Persisted

The contract metadata is stored in `vector_embedding_contracts`.

That persisted record includes:

- profile
- table name
- model identity
- model version
- dimension
- normalization policy
- chunking policy
- preprocessing policy
- generator command JSON

This lets recovered databases retain the metadata required to perform
regeneration later, even though embedding rows themselves are still treated as
derived data.

## Recovery Semantics

Current behavior:

- physical recovery restores canonical tables first
- vector profile metadata and table capability are restored
- embeddings written through `VecInsert` are not treated as canonical recovery
  material
- embeddings are regained through `regenerate-vectors`

In other words:

- `recover` restores vector capability
- `regenerate-vectors` restores vector contents

## Operational Notes

- The generator command is application-controlled. `fathomdb` does not ship an
  embedding model.
- Regeneration replaces the contents of `vec_nodes_active` for the targeted
  profile table.
- If the generator is unavailable or returns invalid output, regeneration fails
  instead of silently degrading.
- The surrounding Rust, Go, and end-to-end coverage exists for this path, but
  it should still be treated as a recovery-sensitive surface.

## Related Docs

- [../README.md](../README.md)
- [../dev/repair-support-contract.md](../dev/repair-support-contract.md)
- [../dev/ARCHITECTURE.md](../dev/ARCHITECTURE.md)
- [../dev/arch-decision-vector-embedding-recovery.md](../dev/arch-decision-vector-embedding-recovery.md)
