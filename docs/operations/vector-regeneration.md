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
  --config /path/to/vector-regeneration.toml \
  --generator-timeout-ms 300000 \
  --generator-max-stdout-bytes 67108864 \
  --generator-max-stderr-bytes 1048576 \
  --generator-max-input-bytes 67108864 \
  --generator-max-chunks 1000000
```

Required flags:

- `--db`: database to regenerate
- `--bridge`: admin bridge binary
- `--config`: TOML or JSON vector regeneration contract

Optional operator policy flags:

- `--generator-timeout-ms`: wall-clock timeout for the external generator
- `--generator-max-stdout-bytes`: stdout cap for the external generator
- `--generator-max-stderr-bytes`: stderr cap for the external generator
- `--generator-max-input-bytes`: stdin JSON payload cap
- `--generator-max-chunks`: chunk-count cap for a single run
- `--generator-allowed-root`: allowlisted root for the generator executable
  path; repeatable
- `--generator-preserve-env`: environment variable to preserve for the child
  process; repeatable

These limits are operator policy, not application contract. They are not
persisted in `vector_embedding_contracts`.

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

- string fields must be non-empty after trimming and stay within engine bounds
- `table_name` must be `vec_nodes_active`
- every active chunk must have exactly one returned embedding
- embedding length must match `dimension`
- embeddings must contain only finite numeric values
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
- contract format version
- applied timestamp
- snapshot hash for the chunk set that was actually applied

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
- regeneration snapshots the active chunk set, runs the external generator
  outside the write transaction, then rechecks the same snapshot before apply
- if the chunk set changed during generation, the command fails and asks the
  operator to retry instead of mixing new metadata with stale embeddings

In other words:

- `recover` restores vector capability
- `regenerate-vectors` restores vector contents
- a failed regeneration leaves both the previously applied contract row and the
  current vec contents unchanged

## Operational Notes

- The generator command is application-controlled. `fathomdb` does not ship an
  embedding model.
- The executable trust boundary is operator-controlled. By default the
  executable path must be absolute, must not be world-writable, and inherits no
  environment variables unless explicitly allowlisted with
  `--generator-preserve-env`.
- Core regeneration validation, executable-trust enforcement, and `sqlite-vec`
  vector workflows are supported on Linux, macOS, and Windows.
- Regeneration replaces the contents of `vec_nodes_active` for the targeted
  profile table only after the generated output has been fully validated and the
  chunk snapshot is revalidated inside the apply transaction.
- If the generator is unavailable or returns invalid output, regeneration fails
  instead of silently degrading.
- The external generator is bounded by timeout, stdout/stderr caps, input-size
  caps, and max-chunk limits.
- Regeneration writes bounded provenance events for request, failure, and apply
  so operators can review the attempted profile, model metadata, snapshot hash,
  and coarse failure class after an incident. Once the request event exists,
  unsupported `sqlite-vec` capability failures are included in that failed
  audit lifecycle.
- The surrounding Rust, Go, and end-to-end coverage exists for this path, but
  it should still be treated as a recovery-sensitive surface.

## Related Docs

- [Home](../index.md)
- `dev/repair-support-contract.md` -- repair support contract (internal design doc)
- `dev/ARCHITECTURE.md` -- system design and module layout (internal design doc)
- `dev/arch-decision-vector-embedding-recovery.md` -- vector recovery design decision (internal design doc)
