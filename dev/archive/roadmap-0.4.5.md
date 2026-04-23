# FathomDB 0.4.5 roadmap

Captures the user-picked projections feature (design item 10d) with
full implementation scope. Companion to `dev/notes/scope-0.4.2.md`
(which must ship first) and `dev/design-user-picked-projections.md`
(the design reference).

## Scope

0.4.5 is the **operator configurability** release. One headline item:

- **User-picked tokenizers and embedding adapters** (item 10d).

Items 10a (`matched_paths`), 10b (per-kind FTS5 tables + BM25 weights),
and 10c (snippet stability docs) all ship in **0.4.2**. The 0.4.2
release delivers the storage foundation (`projection_profiles` table,
per-kind FTS5 table creation, tokenizer parameter machinery) that 0.4.5
plugs into.

> Item 9 (async rebuild) shipped in 0.4.1. All of 0.4.5's rebuild
> operations ride on that machinery via `RebuildActor`.

---

## Dependency on 0.4.2

0.4.5 Phase 1 completion requires two exports from `fathomdb-schema`
that 0.4.2 Pack A delivers:

- `fts_kind_table_name(kind: &str) -> String` — canonical kind → table
  name mapping.
- `resolve_fts_tokenizer(conn, kind: &str) -> String` — profile lookup
  with `porter unicode61 remove_diacritics 2` fallback.

The `projection_profiles` table also ships in 0.4.2 (empty). Phase 2
and Phase 3 below can begin immediately. Phase 1 completion waits only
on `fts_kind_table_name` and `resolve_fts_tokenizer` being merged.

---

## Item 10d: User-Picked Tokenizers and Embedding Adapters

### Problem

FathomDB currently hardcodes `porter unicode61 remove_diacritics 2`
for all property-FTS schemas and BGE-Small for all embeddings.
Different data domains — English prose, source code, CJK documents,
long-form meeting notes — benefit from different configurations.
Changing either requires a manual schema migration with no guardrails
and no observability.

### Design reference

Full design in `dev/design-user-picked-projections.md`. This roadmap
captures the implementation phases, sequencing, and ship criteria.

---

## Implementation phases

### Phase 1: Rust-side tokenizer wiring (depends on 0.4.2 Pack A)

Wire `configure_fts` result into per-kind FTS5 table creation:

1. `configure_fts` (Phase 3 admin surface) writes a row to
   `projection_profiles(kind='<kind>', facet='fts')` with
   `config_json = {"tokenizer": "<tokenizer_string>"}`.
2. When `register_fts_property_schema_with_entries` is called for that
   kind, `resolve_fts_tokenizer(conn, kind)` reads the profile and
   passes the tokenizer string to `create_fts_kind_table`.
3. Re-registration with an existing schema triggers async rebuild
   (already the 0.4.1 behavior); tokenizer change does the same.

One additional Rust change: the `bootstrap.rs` recovery path (`.recover`)
must call `resolve_fts_tokenizer` when recreating per-kind tables rather
than hardcoding the default. This is the "Bootstrap Parametrization"
requirement from the design doc.

### Phase 2: `projection_profiles` configuration persistence

The `projection_profiles` table ships empty in 0.4.2. Phase 2 adds:

- `AdminService::set_fts_profile(kind, tokenizer_str)` — writes or
  upserts a `(kind, 'fts')` row. Validates the tokenizer string against
  the supported preset list (see tokenizer presets below) before writing.
- `AdminService::get_fts_profile(kind) -> Option<FtsProfile>` — reads
  the stored profile for a kind.
- `AdminService::set_vec_profile(embedder_identity)` — writes or upserts
  the `('*', 'vec')` observation row. Called by the engine after a
  successful vector regeneration to record the embedder's identity.
- `AdminService::get_vec_profile() -> Option<VecProfile>` — reads the
  current global vec profile.

Profile types:

```rust
pub struct FtsProfile {
    pub kind: String,
    pub tokenizer: String,
    pub active_at: Option<i64>,
    pub created_at: i64,
}

pub struct VecProfile {
    pub model_identity: String,
    pub model_version: Option<String>,
    pub dimensions: u32,
    pub active_at: Option<i64>,
    pub created_at: i64,
}
```

`VecProfile` is an **observation**, not a directive. Writing it does not
trigger any rebuild; it records what the embedder last reported. The
engine writes this automatically after vector regeneration completes.

### Phase 3: Python admin surface

#### `AdminClient` new methods

```python
def configure_fts(
    self,
    kind: str,
    tokenizer: str,
    mode: RebuildMode = RebuildMode.ASYNC,
    *,
    agree_to_rebuild_impact: bool = False,
) -> FtsProfile: ...

def configure_vec(
    self,
    embedder: QueryEmbedder,
    mode: RebuildMode = RebuildMode.ASYNC,
    *,
    agree_to_rebuild_impact: bool = False,
) -> VecProfile: ...

def preview_projection_impact(
    self,
    kind: str,
    target: Literal["fts", "vec"],
) -> ImpactReport: ...

def get_fts_profile(self, kind: str) -> FtsProfile | None: ...
def get_vec_profile(self) -> VecProfile | None: ...
```

`ImpactReport` dataclass:
```python
@dataclasses.dataclass
class ImpactReport:
    rows_to_rebuild: int
    estimated_seconds: int       # rough: rows / 5000
    temp_db_size_bytes: int      # rough: rows * avg_text_len * 2
    current_tokenizer: str | None
    target_tokenizer: str | None
```

`configure_fts` flow:
1. Call `preview_projection_impact(kind, "fts")`.
2. If `rows_to_rebuild > 0` and not `agree_to_rebuild_impact`, raise
   `RebuildImpactError` with the report (interactive CLI handles
   prompt; programmatic callers must pass `agree_to_rebuild_impact=True`).
3. Call `AdminService::set_fts_profile(kind, tokenizer)`.
4. Call `register_fts_property_schema_with_entries` for the kind with
   `mode` (triggers rebuild using the new tokenizer via Phase 1 wiring).

`configure_vec` flow:
1. Call `preview_projection_impact('*', "vec")`.
2. Safety gate as above.
3. Call `engine.regenerate_vector_embeddings(embedder, config)`.
4. After regeneration, engine writes `set_vec_profile` observation.

#### `fathomdb admin` CLI

```
fathomdb admin configure-fts \
    --db ./fathomdb.sqlite \
    --kind WMKnowledgeObject \
    --tokenizer recall-optimized-english

fathomdb admin configure-fts \
    --db ./fathomdb.sqlite \
    --kind SourceFile \
    --tokenizer source-code \
    --agree-to-rebuild-impact

fathomdb admin configure-vec \
    --db ./fathomdb.sqlite \
    --embedder nomic-v1.5

fathomdb admin preview-impact \
    --db ./fathomdb.sqlite \
    --kind WMKnowledgeObject \
    --target fts
```

Tokenizer names accepted by the CLI map to the canonical preset strings
(see tokenizer presets below). Raw tokenizer strings (e.g.
`"porter unicode61"`) are also accepted for advanced use.

---

## Tokenizer presets

Five named presets, each maps to a validated FTS5 tokenize= string:

| CLI name | `tokenize=` string | Use case |
|---|---|---|
| `recall-optimized-english` | `porter unicode61 remove_diacritics 2` | General English; default |
| `precision-optimized` | `unicode61 remove_diacritics 2` | Technical text; no stemming |
| `global-cjk` | `icu` | Chinese / Japanese / Korean |
| `substring-trigram` | `trigram` | Partial word matching |
| `source-code` | `unicode61 tokenchars '._-$@'` | Variable names, identifiers |

### Query-side optimizations per tokenizer

Each preset requires a corresponding query-path adaptation in
`fathomdb-query`:

1. **`recall-optimized-english`**: Ensure query terms flow through
   Porter stemming at the FTS5 query-parser level. Apply stopword
   filter to reduce index noise.
2. **`precision-optimized`**: Wrap multi-word technical terms in
   double quotes to prevent unwanted token splitting.
3. **`global-cjk`**: Warm `icu` extension in `ReadPool` connection
   initialization. Detect CJK ranges and use bigram query form.
4. **`substring-trigram`**: Reject queries under 3 characters (return
   empty results with a diagnostic, not an error). Strip redundant `*`
   wildcards.
5. **`source-code`**: Escape `.`, `-`, `_`, `$`, `@` in query terms so
   they are treated as token characters, not FTS5 operators. Adjust
   snippet generation to not truncate at symbol boundaries.

The query path must read the active tokenizer for a kind from
`projection_profiles` at query time (or cache it on
`ExecutionCoordinator` open) to select the correct query rendering
strategy.

---

## Embedding adapter suite

All adapters are `QueryEmbedder` implementations. They live in the
Python SDK (`fathomdb.embedders`) and TypeScript SDK
(`@fathomdb/fathomdb/embedders`). None are engine-level features.

### In-process (Rust/Candle)

`BgeSmallEmbedder` (existing):
- Compile with SIMD support (AVX2/NEON) via `candle` feature flags.
- Lazy-load model weights on first use; share across `ReadPool` via
  `Arc<Model>`.

`NomicEmbedder` (new):
- `nomic-embed-text-v1.5`, 768 dimensions, 8192-token context.
- Matryoshka truncation: caller specifies `serving_dimensions`
  (128–768). Embedder truncates and L2-normalizes after truncation.
- Expose `max_tokens() -> usize` returning 8192 for engine chunking.

### SDK-side adapters (Python)

`OpenAIEmbedder`:
- Uses HTTP/2 via `httpx` (already available in Python env).
- Client-side TTL cache keyed on query text.
- Exposes embedder identity: `openai/<model>/<dimensions>`.

`JinaEmbedder`:
- `jina-embeddings-v2-base-en`, 768 dimensions, 8192-token context.
- ALiBi positional bias: pass full 8k window without chunking for
  documents under 8192 tokens.
- SDK-side `QueryEmbedder` implementation.

`StellaEmbedder`:
- `stella_en_400M_v5`, 1024 dimensions, Matryoshka-capable.
- Same truncation contract as `NomicEmbedder`.

`SubprocessEmbedder` (replaces the removed engine-level generator):
- Takes a `command: list[str]` and spawns a persistent worker via
  `subprocess.Popen` on first use.
- Communicates via binary stdin/stdout (raw f32 little-endian vectors).
- Implements `QueryEmbedder` — identity string is a required
  constructor parameter that the caller must provide.
- Not a built-in; lives in `fathomdb.embedders.subprocess`.

### TypeScript adapters

Mirror the Python surface: `OpenAIEmbedder`, `JinaEmbedder`,
`StellaEmbedder`, `SubprocessEmbedder` all implement the TypeScript
`QueryEmbedder` interface.

---

## Profile-aware lifecycle guards

`ExecutionCoordinator::open` must:
1. Load all `projection_profiles WHERE facet = 'fts'` rows into an
   in-memory map `kind → tokenizer_string`.
2. For each kind in `fts_property_schemas`, verify the per-kind FTS5
   table exists. If missing (e.g. after a partial migration), recreate
   it using `resolve_fts_tokenizer` and enqueue async rebuild.
3. Load the `('*', 'vec')` profile row if present. If the open-time
   embedder's identity does not match the stored observation, emit a
   warning log (not an error) — the embedder may have been upgraded
   intentionally.

This replaces any hardcoded tokenizer string in lifecycle guard paths.

---

## Non-goals for 0.4.5

- Per-leaf weights within a recursive path (ships in 10b / 0.4.2 at
  the spec level; recursive paths use one column with one weight).
- Runtime weight tuning without re-registering the schema.
- Cross-kind weight normalization.
- Write-priority / foreground-read isolation (item 11 / post-0.5.0).
- Per-kind vec tables. Vector storage remains global in 0.4.5;
  `('*', 'vec')` sentinel covers the entire vec index.

---

## Memex impact

On ship:
- `m004_register_fts_property_schemas_v2.py` can optionally call
  `configure_fts` to set per-kind tokenizers for the four hot kinds.
  No required change; existing behavior (default tokenizer) is
  unchanged.
- `SubprocessEmbedder` available as a migration path for any Memex
  tooling that previously used the engine-level subprocess generator.
- `NomicEmbedder` / `JinaEmbedder` available for upgrading long-form
  content kinds from 512-token BGE-Small to 8192-token context.

---

## Open implementation questions

- **`icu` extension availability**: ICU tokenizer requires the SQLite
  `icu` extension to be compiled in. Check availability on all CI
  targets (manylinux, macOS, Windows) before committing `global-cjk`
  to the supported preset list. If ICU is unavailable on a platform,
  document the constraint and fail at `configure_fts` call time with
  a clear error, not at query time.
- **Chunking integration for long-context embedders**: The engine
  currently chunks content at write time. `QueryEmbedder::max_tokens()`
  is not yet in the trait. Adding it is a Rust-side trait change that
  must be coordinated with the existing in-process embedder (BGE-Small
  reports 512, Nomic/Jina report 8192). Confirm trait extension plan
  before Phase 1 starts.
- **`fathomdb admin` database access pattern**: The CLI must open the
  engine (not just SQLite directly) to call `AdminService` methods.
  This means the engine must not be running when the CLI runs (single-
  writer constraint). Document this constraint prominently. For the
  `configure-vec` command specifically, the CLI opens the engine with
  the specified embedder — confirm the CLI open path doesn't conflict
  with a running Memex engine process.

---

## Critical path

1. Ship **0.4.2** (Pack A must merge first to unblock Phase 1).
2. 0.4.5 Phases 2 and 3 can start immediately (no 0.4.2 dependency).
3. Phase 1 completes once 0.4.2 Pack A is merged.
4. Phase 4 (lifecycle guards + `QueryEmbedder::max_tokens`) after
   Phase 1.
5. Embedding adapters (Phase 3 Python/TS) can be implemented in
   parallel with Phases 1–2.
6. 0.4.5 release.
