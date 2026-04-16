# Projection Profile Management

Projection profiles let operators record and change the FTS tokenizer or vector embedding model used by a database. FathomDB stores the active profile in `projection_profiles` and exposes it through `AdminClient` methods and an optional CLI.

---

## FTS Tokenizer Profiles

### Built-in presets

| Preset name | FTS5 config | Best for |
|---|---|---|
| `recall-optimized-english` | `porter unicode61 remove_diacritics 2` | General English text |
| `precision-optimized` | `unicode61 remove_diacritics 2` | Technical docs, jargon-heavy content |
| `global-cjk` | `icu` | Chinese, Japanese, Korean |
| `substring-trigram` | `trigram` | Partial-word / substring search |
| `source-code` | `unicode61 tokenchars '._-$@'` | Code, paths, identifiers |

Pass a preset name **or** a raw FTS5 tokenizer string to any of the profile methods below.

### Python API

```python
from fathomdb import FathomDB

db = FathomDB.open("store.db")

# Preview impact before changing (how many nodes must be reindexed)
impact = db.admin.preview_projection_impact("Book", "fts")
print(f"{impact.rows_to_rebuild} rows, ~{impact.estimated_seconds}s")

# Set tokenizer for a kind (raises RebuildImpactError if rows > 0)
profile = db.admin.configure_fts("Book", "source-code", agree_to_rebuild_impact=True)

# Re-register the schema to trigger a rebuild with the new tokenizer
db.admin.register_fts_property_schema("Book", ["$.title", "$.body"])

# Read back the stored profile
profile = db.admin.get_fts_profile("Book")  # FtsProfile | None
print(profile.tokenizer)
```

> **Note**: The `mode` parameter (SYNC / ASYNC) is accepted for API compatibility but not yet forwarded to a rebuild. Call `register_fts_property_schema` or `rebuild("fts")` explicitly after `configure_fts` to apply the new tokenizer.

> **Note**: `fts_strategies` (query-side tokenizer adaptations such as the trigram short-query guard) are loaded at engine open time. After calling `configure_fts`, reopen the engine for adaptations to take effect.

### CLI

```bash
# Preview
fathomdb admin preview-impact --db store.db --kind Book --target fts

# Configure (prompts for confirmation when rows > 0)
fathomdb admin configure-fts --db store.db --kind Book --tokenizer source-code

# Skip interactive prompt in CI
fathomdb admin configure-fts --db store.db --kind Book --tokenizer source-code \
  --agree-to-rebuild-impact

# Read back
fathomdb admin get-fts-profile --db store.db --kind Book
```

---

## Vector Embedding Profiles

### Python API

**With a Python-side embedder (OpenAI, Jina, Stella, Subprocess):**

```python
from fathomdb import FathomDB
from fathomdb.embedders import OpenAIEmbedder

db = FathomDB.open("store.db")
embedder = OpenAIEmbedder("text-embedding-3-small", api_key="sk-…", dimensions=1536)

# Preview impact
impact = db.admin.preview_projection_impact("*", "vec")
print(f"{impact.rows_to_rebuild} chunks to rebuild")

# Record the active model
profile = db.admin.configure_vec(embedder, agree_to_rebuild_impact=True)

# Rebuild vector index with the new embedder
db.admin.regenerate_vector_embeddings(embedder)

# Read back
profile = db.admin.get_vec_profile()  # VecProfile | None
print(profile.model_identity, profile.dimensions)
```

**With the built-in Candle/BGE-small embedder:**

When the engine is opened with `embedder="builtin"`, use `BuiltinEmbedder` to
record the correct `VecProfile`. Do **not** use another embedder class as a proxy
— the stored profile must match what the Rust engine actually used.

```python
from fathomdb import FathomDB, BuiltinEmbedder
from fathomdb import VectorRegenerationConfig

db = FathomDB.open("store.db", embedder="builtin")

# Record the correct identity for the built-in embedder
profile = db.admin.configure_vec(BuiltinEmbedder(), agree_to_rebuild_impact=True)

# Rebuild is performed by the Rust Candle runtime — no Python embedder needed
db.admin.regenerate_vector_embeddings(VectorRegenerationConfig())
```

> **Note**: `configure_vec` records the profile row but does not trigger a rebuild automatically. Call `regenerate_vector_embeddings` explicitly.

### CLI

```bash
fathomdb admin preview-impact --db store.db --kind "*" --target vec
fathomdb admin configure-vec --db store.db --embedder bge-small-en-v1.5
fathomdb admin get-vec-profile --db store.db
```

---

## Vec Identity Guard

When a FathomDB engine is opened with a `QueryEmbedder` attached, `check_vec_identity_at_open` compares the embedder's `model_identity` and `dimensions` against the stored `VecProfile`. If they differ, a `warn!` log is emitted. **Startup is never blocked** — the guard is informational only.

To silence the warning after switching models, call `configure_vec` with the new embedder before reopening the engine, then rebuild.
