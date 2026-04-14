# Project invariant: vector identity is the embedder's responsibility

Established in 0.4.0 (GH #39) by the second Memex 0.4.0 headline pack.

## The rule

Vector identity (`model_identity`, `model_version`, `dimension`,
`normalization_policy`) is owned by the `QueryEmbedder` trait. It is
NOT a field on `VectorRegenerationConfig`, NOT a caller-provided string,
NOT something that can drift between the read path and the regen path.

The fathomdb `Engine` holds exactly one embedder per instance (via
`EmbedderChoice` at `Engine::open` time). Both the read path
(`search()`'s vector branch, resolved in
`ExecutionCoordinator::fill_vector_branch`) and the regen path
(`Engine::regenerate_vector_embeddings`) use that same embedder. The
persisted vector profile is stamped directly from
`embedder.identity()`, so the strings cannot disagree with the
computation — they are the same source.

## What this disallows

Any future PR that:

- Adds `model_identity` / `model_version` / `dimension` /
  `normalization_policy` / `generator_command` back to
  `VectorRegenerationConfig`.
- Adds an overload of `regenerate_vector_embeddings` that takes a
  caller-provided identity string instead of (or in addition to) an
  `&dyn QueryEmbedder`.
- Adds a subprocess generator pattern back into `fathomdb-engine`
  directly. Clients that genuinely need a subprocess-driven embedder
  implement a `SubprocessEmbedder` adapter against the `QueryEmbedder`
  trait in their own code and pass it to `Engine::open` via
  `EmbedderChoice::InProcess`.
- Plumbs caller-authored identity strings through the Python / Node FFI
  for regen.

will be rejected on review citing this invariant.

## Why

The 0.4.0 read path already threads an `EmbedderChoice` through
`Engine::open` and uses it for `search()`'s vector branch. Prior to
this invariant, the write-time (regen) path took a separate set of
identity strings on `VectorRegenerationConfig` and shelled out to a
subprocess generator to produce the vectors. Memex call sites had to
serialize the same model into two different places — the
`EmbedderChoice` passed to `Engine::open` for reads, and the
`model_identity`/`model_version`/`generator_command` fields in the
regen config for writes. Any drift between these (typo, copy-paste
error, stale config file, mismatched binary) silently produced a
database where query embeddings and stored embeddings were in
different vector spaces, returning plausible-but-wrong nearest
neighbours.

The subprocess generator pattern is removed from fathomdb proper. It
was a surface with significant complexity (executable trust, stream
overflow limits, timeouts, env-var allowlisting) and provided no
capability that an in-process embedder doesn't — every subprocess
generator is just a thin adapter over some process-external embedding
service, and callers who need that shape can write a
`SubprocessEmbedder` in their own code against the existing
`QueryEmbedder` trait.
