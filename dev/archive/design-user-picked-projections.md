# Design: User-Picked Tokenizers and Embeddings

## Purpose

Provide a transparent, operator-driven mechanism for users to configure and
evolve the Full-Text Search (FTS) and Vector (Embedding) projections in FathomDB.

FathomDB currently uses hardcoded defaults for FTS (Porter stemming) and
embeddings (BGE-Small). This design allows users to "pick" their own
configurations while ensuring that such changes are **intentional**,
**impact-aware**, and **reversible**.

## Non-Goals

- User-defined SQLite virtual table extensions (users cannot upload C code).
- Real-time switching of tokenizers within a single query.
- Automatic "auto-scaling" of embedding models.

## Constraints

1. **Durability**: All projection configurations must be persisted in the canonical database.
2. **Impact Awareness**: Changing a tokenizer or embedder requires a rewrite of the derived tables. The operator must be warned of the performance impact.
3. **Admin Ownership**: Configuration changes are managed through the Python Admin CLI (`fathomdb admin`) and `AdminClient` methods, not through the standard write path.
4. **Safety**: "No accident" policy. Changes require explicit confirmation or a specialized flag.

## High-Level Architecture

The design introduces a **Projection Profile** model. Instead of a single
global configuration, FathomDB manages profiles scoped by kind and facet.

### 1. Schema Extensions

A new canonical table in the SQLite file manages these profiles:

```sql
CREATE TABLE projection_profiles (
    kind TEXT NOT NULL,         -- node kind (e.g. 'WMKnowledgeObject'), or '*' for global profiles
    facet TEXT NOT NULL,        -- 'fts' | 'vec'
    config_json TEXT NOT NULL,  -- Tokenizer args or Model metadata
    active_at INTEGER,          -- Timestamp when this profile became active
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (kind, facet)
);
```

### 2. Supporting Types

The administration surface utilizes the following types:

- **`ProjectionTarget`**: Enum with variants `Fts` and `Vec`.
- **`ImpactReport`**: Data structure returned by preview methods:
    - `rows_to_rebuild: int`: Count of canonical rows to be re-projected.
    - `estimated_time_seconds: int`: Projected duration based on current engine telemetry.
    - `temp_db_size_bytes: int`: Estimated temporary storage required for the atomic swap.

### 3. Identity and Ownership (Directive vs. Observation)

The relationship between the profile and the underlying engine differs by facet:

- **FTS Facet (Directive)**: The `config_json` (tokenizer settings) is a 
  **directive**. The engine uses these settings to dynamically generate the 
  `CREATE VIRTUAL TABLE` SQL for the per-kind FTS5 table (e.g., 
  `fts_node_properties_<kind>`). The profile *drives* the schema.
- **Vector Facet (Observation)**: Per the **Vector Identity Invariant**, 
  the embedder owns its identity. For vector facets, `config_json` acts as 
  an **observation** (a system-of-record). It persists the model identity, 
  version, and dimensions provided by the embedder that last populated the 
  table (typically `vec_nodes_active`). Since vector storage in FathomDB 0.4.x 
  is global, the profile is keyed by a global sentinel: `('*', 'vec')`.
  
#### The FTS Configuration Flow
When `configure_fts(kind, tokenizer, mode)` is invoked:
1. The profile row is written to `projection_profiles`.
2. The existing per-kind FTS5 table is dropped.
3. A new FTS5 table is created using the new tokenizer string.
4. An asynchronous rebuild is enqueued via the existing `RebuildActor`.

#### The Vector Migration Workflow
The Python Admin CLI orchestrates migrations by:
1. Identifying a target embedder (via its live identity).
2. Verifying if the target's identity matches the *current* observation in 
   the active profile.
3. If they differ, the CLI manages the **regeneration-and-swap** process, 
   updating the `projection_profiles` observation only after the new 
   derived state is successfully established.

### 3. Python Admin Surface

The `fathomdb.admin.AdminClient` provides the following methods for profile management:

- `configure_fts(kind: str, tokenizer: str, mode: RebuildMode)`: Updates the tokenizer settings for a node kind.
- `configure_vec(embedder: QueryEmbedder, mode: RebuildMode)`: Updates the model/dimension settings for the global vector profile using the provided embedder's identity.
- `preview_projection_impact(kind: str, target: ProjectionTarget) -> ImpactReport`: Returns a report on the number of rows requiring rebuild and estimated resource usage.

### 4. Admin CLI: `fathomdb admin`

The CLI is the primary interface for "thoughtful" configuration.

#### Commands

- `fathomdb admin configure-fts --kind Document --tokenizer "porter unicode61"`
- `fathomdb admin configure-vec` (uses the engine's configured embedder)

#### The "Safety Gate" Flow

To prevent accidental data rewrites:

1. **Impact Analysis (Automatic)**:
   When a user submits a configuration change, the CLI first calls `preview_projection_impact`.
   
2. **Interactive Warning**:
   ```text
   WARNING: Changing the tokenizer for kind 'Document' will require a 
   full rebuild of the FTS index.
   
   Impact:
   - Rows to rebuild: 1,450,200
   - Estimated time: 8-12 minutes
   - DB Size Increase (temp): ~450MB
   
   During this time, search results may be inconsistent.
   
   Do you want to proceed? [y/N]: 
   ```

3. **Explicit Confirmation**:
   In non-interactive environments (CI), the command will fail unless the user
   provides `--agree-to-rebuild-impact`.

## Ensuring Architectural Integrity and Synchronicity

To prevent "hidden" secondary configurations or hardcoded optimizations from 
breaking user-picked changes, the following architectural updates are 
required:

1. **Dynamic Profile Discovery (SDKs & Bindings)**: SDKs (Python, TS) must
   default to discovering projection metadata (dimensions, tokenizers) 
   directly from the `projection_profiles` table upon `Engine.open()`, 
   rather than requiring hardcoded parameters. This ensures that client code 
   stays in sync with the database even if the model or tokenizer is 
   migrated via the CLI.
2. **Profile-Aware Lifecycle Guards**: Open-time rebuild and integrity 
   guards (e.g., in `ExecutionCoordinator`) must be parameterized by the 
   active profile. They must not trigger an automatic rebuild using a 
   hardcoded default tokenizer if a custom profile is already active in the 
   database.
   - **Vector Reconciliation**: If the open-time `EmbedderChoice` produces an
     identity that differs from the stored `('*', 'vec')` observation, the 
     engine must issue a warning (and potentially error in strict modes) 
     to prevent writing mismatched embeddings into the index.
3. **Bootstrap Parametrization**: The `fathomdb-schema` bootstrap process 
   must be refactored to dynamically generate FTS and Vector virtual table 
   definitions based on the persisted `projection_profiles` table.
   - **Bootstrap Fallback**: If `projection_profiles` is empty (e.g., on a 
     fresh database):
     - FTS defaults to `'porter unicode61 remove_diacritics 2'`.
     - Vector defaults to the identity of the open-time embedder.
   - **Table Name Sanitization**: Per-kind FTS tables must be named using 
     the rule: `fts_node_properties_<sanitized_kind>`, where 
     `sanitized_kind` is the lowercase kind name with all 
     non-alphanumeric characters replaced by `_`.

## Tokenizer-Specific Code Optimizations

To ensure that each supported tokenizer configuration performs optimally, 
the following code-level enhancements are required:

1. **Recall-Optimized English**:
   - **Query-Side Stemming**: The `fathomdb-query` crate must ensure that 
     search terms are passed to SQLite in a way that allows the FTS5 query 
     parser to apply the same Porter stemming.
   - **Stopword Filter Optimization**: Provide a localized stopword list to 
     the `bootstrap` process to keep the index size small and searches fast.
2. **Precision-Optimized Technical**:
   - **Exact Phrase Bias**: The query renderer should be optimized to wrap 
     multi-word technical terms in double quotes to avoid splitting on 
     punctuation (e.g., `vector_search` → `"vector_search"`).
   - **Diacritic Normalization Sync**: Ensure that query-time normalization 
     matches the `remove_diacritics 2` index setting.
3. **Global/Universal (CJK)**:
   - **ICU Extension Pre-loading**: Warm up the `icu` extension in the 
     `ReadPool` during connection initialization to avoid search-time 
     latency spikes.
   - **Bigram Query Parsing**: Detect CJK character ranges and adjust query 
     rendering to favor overlapping tokens if results are too coarse.
4. **Substring (Trigram)**:
   - **Minimum Query Length Gate**: Implement a safety valve that rejects 
     searches under 3 characters to prevent performance-killing scans.
   - **Wildcard Suppression**: Automatically strip redundant `*` wildcards 
     from trigram queries to speed up execution.
5. **Source Code & Identifier-Aware**:
   - **Symbol Escaping**: Update the query renderer to escape critical 
     symbols (`.`, `-`, `_`, `$`, `@`) so they are treated as tokens rather 
     than FTS5 operators.
   - **Token-Aware Snippets**: Optimize snippet generation to ensure that 
     matched identifiers are displayed in their entirety, rather than 
     being truncated at symbol boundaries.

## Embedding Implementation Responsibilities & Optimizations

To maintain the **Vector Identity Invariant**, the engine treats the embedder 
as the source of truth for model identity and vector content. Advanced 
capabilities and performance optimizations are implementation details of 
their respective `QueryEmbedder` implementations (in-process or SDK-side), 
not engine-level configuration flags.

1. **In-Process Embedders (e.g., BGE-Small via Candle)**:
   - **Performance**: Should be compiled with SIMD support (AVX2/NEON) and 
     utilize lazy-loading with internal caching of model weights to avoid 
     redundant I/O across the `ReadPool`.
2. **Cloud-Based Embedders (e.g., OpenAI SDK-side adapter)**:
   - **Networking**: Should utilize HTTP/2 multiplexing and client-side 
     embedding caches to minimize latency and API costs.
3. **Matryoshka-Capable Embedders (e.g., Nomic, Stella)**:
   - **Truncation**: The embedder implementation is responsible for 
     performing any dimensionality reduction (truncation) and ensuring that 
     L2-normalization is applied *after* truncation to maintain distance 
     metric integrity.
4. **Long-Context Embedders (e.g., Jina, Nomic)**:
   - **Context Window**: The embedder should expose its maximum token limit 
     (e.g., 8192) to the engine's chunking logic to prevent unnecessary 
     fragmentation of long-form documents.
   - **Positional Integrity**: Implementation must correctly handle 
     architecture-specific biases (like ALiBi) across the full window.
5. **Custom Subprocess Pattern (SDK layer, not engine layer)**:
   - If an application requires a subprocess-driven model (e.g., via 
     `sentence-transformers`), it must implement a `QueryEmbedder` 
     adapter in the SDK or user code. The subprocess generator pattern was 
     removed from FathomDB proper in 0.4.0.
   - **Efficiency**: The adapter should maintain a persistent worker 
     process and use binary pipes for vector exchange to avoid the 
     process-spawning and serialization overhead of legacy designs.

## Implementation Strategy

### Phase 1: Parameterized Rebuild (Rust)
Refactor `register_fts_property_schema_with_entries` (in `admin.rs`) and 
`RebuildActor` / `run_rebuild` (in `rebuild_actor.rs`) to accept 
configuration objects instead of relying on hardcoded constants. 
*Note: `RebuildMode::Eager/Async` already exists; wiring the tokenizer string 
into per-kind table creation remains for 0.4.2/0.4.5.*

### Phase 2: Configuration Persistence
Add the `projection_profiles` table to `bootstrap.rs` and implement the
registry logic to load these settings at engine startup.

### Phase 3: Python Admin Surface Extension
Update the Python `AdminClient` and `fathomdb admin` CLI to support profile 
configuration and impact analysis. This replaces the legacy Go-based 
Admin Bridge.

### Phase 4: Atomic Swap (Complete)
Phase 4 is complete — shipped in 0.4.1 as `RebuildActor` + 
`fts_property_rebuild_staging` + `IMMEDIATE` transaction swap. The migration 
path will utilize this existing infrastructure (see 
`crates/fathomdb-engine/src/rebuild_actor.rs`). It also leverages 
`fts_property_rebuild_state` for progress tracking.

## Verification Plan

1. **Python Unit Tests**: Verify that the CLI rejects changes without confirmation.
2. **Integration Tests (Rust/Python)**: Verify that changing a tokenizer and 
   running `rebuild` results in the expected tokenization in the per-kind 
   `fts_node_properties_<kind>` table.
3. **Safety Tests**: Verify that interrupted rebuilds leave the database in a
   consistent (if stale) state and can be resumed/repaired.
