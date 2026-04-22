# OpenClaw memory backends vs FathomDB

| Field | Value |
|---|---|
| Date | 2026-04-18 |
| FathomDB version | 0.5.1 |
| MemoryIndexManager version | openclaw `@openclaw/memory-core` 2026.4.18 |
| QmdMemoryManager version | openclaw `@openclaw/memory-core` 2026.4.18 (wraps external `qmd` CLI, version determined at runtime) |

## TL;DR

FathomDB is a **graph + FTS + vector hybrid datastore with provenance**, exposed as a multi-language SDK. OpenClaw's memory backends are **memory-plugin consumers** sitting behind a `MemorySearchManager` interface, tuned for prompt-context retrieval only.

All three share substrate (SQLite + FTS5 + sqlite-vec; QMD bundles its own). Overlap is technical; use cases differ.

## OpenClaw backend summaries

### MemoryIndexManager ("builtin")

- **File**: `extensions/memory-core/src/memory/manager.ts:78`
- **Storage**: `node:sqlite` with FTS5 + sqlite-vec. Tables: `chunks`, `chunks_vec`, `chunks_fts`, `embedding_cache`.
- **Index unit**: Line-bounded chunks of files from memory sources + session transcripts.
- **Search**: Hybrid BM25 + cosine fusion (weighted), optional MMR, temporal decay. Falls back to FTS-only when embedder unavailable.
- **Writes**: File watcher (chokidar, 200ms debounce) + session listener + interval + search-triggered sync.
- **Embedders**: Pluggable chain — OpenAI, Mistral, Ollama, local-embed. Batched with configurable concurrency.
- **Concurrency**: SQLite `busy_timeout=5000`, readonly-recovery reopen on corruption.
- **Quirks**: Temporal decay, relaxed-fallback FTS, session delta tracking (append-only byte range), auto schema rebuild.

### QmdMemoryManager ("qmd")

- **File**: `extensions/memory-core/src/memory/qmd-manager.ts:252`
- **Storage**: External `qmd` CLI owns SQLite at `$XDG_CACHE_HOME/qmd/index.sqlite`. OpenClaw invokes subprocess, parses JSON stdout.
- **Index unit**: Opaque QMD documents; OpenClaw tracks collection roots (path + glob) + session export state.
- **Search**: QMD 1.1+ unified `query` (BM25 + vec + HyDE expansion). Falls back to `search`/`vsearch` per-mode for older QMD.
- **Writes**: Watcher + `qmd update` subprocess + separate lock-protected `qmd embed` with exponential backoff (60s → 1h).
- **Embedders**: Delegated to QMD binary. OpenClaw only symlinks model cache.
- **Concurrency**: File-lock `qmd/embed.lock` (15 min wait budget), single pending update promise, optional mcporter daemon.
- **Quirks**: Collection repair on null-byte corruption, MCP tool version detection, stdout truncation cap (200k chars), HyDE LLM query rewrites.

## Feature matrix vs FathomDB 0.5.1

| Aspect | FathomDB 0.5.1 | MemoryIndexManager | QmdMemoryManager |
|---|---|---|---|
| **Data model** | Nodes, edges, chunks, runs, steps, actions; logical IDs + supersession | Chunks (file/line grain) | Opaque QMD docs |
| **Write API** | Explicit `writer().submit(WriteRequest)` atomic batch | Implicit — file watcher drives all writes | Implicit — subprocess drives all writes |
| **Search surface** | `search / text_search / vector_search / query / traverse` | `search()` only | `search()` only |
| **Graph** | First-class edges + traversal | None | None |
| **Provenance** | Append-only mutation log, source_ref, trace/excise/purge admin | `source` column on chunks | `docPathCache` metadata only |
| **Supersession** | Old versions retained, marked superseded | Delete-on-path-change | Collection rebuild on corruption |
| **Embedder ownership** | Engine-side pluggable; identity owned by embedder (invariant) | Plugin chain inside manager | Fully delegated to QMD |
| **Ranking** | Unified planner, deterministic block precedence, adaptive strict→relaxed FTS | Weighted fusion + MMR + temporal decay | QMD-side fusion or HyDE |
| **Concurrency** | Exclusive file lock, single-writer runtime | SQLite `busy_timeout` + batch semaphore | File-lock on embed + single update queue |
| **Bindings** | Rust, Python (PyO3), TypeScript (napi-rs), Go CLI | TypeScript in-process only | TypeScript wrapper over CLI |
| **Primary use case** | Durable agent state (runs/steps/actions) | Prompt-context retrieval cache | Prompt-context retrieval cache |

## Architectural fit

- **MemoryIndexManager ≈ subset** of FathomDB — same substrate (SQLite + FTS5 + sqlite-vec), chunk grain, hybrid search. Missing: graph, provenance log, supersession, multi-language SDK, explicit write API.
- **QmdMemoryManager** is a **different axis** — outsources entire index to external binary. FathomDB is what QMD is (self-contained indexer), with richer data model + SDKs.
- FathomDB could plausibly ship as a third OpenClaw backend implementing `MemorySearchManager`. Adapter responsibilities:
  - Map `search(query, opts)` → engine unified search builder.
  - Map `readFile` → chunk lookup by path + line range.
  - Map `sync(sessionFiles?)` → ingestion pipeline calling `writer().submit` with chunked content.
  - Map `probeEmbeddingAvailability` → engine embedder registry probe.

## Mismatches to note when integrating

- OpenClaw `MemorySearchManager` is **search-centric + file-sync-driven**; no `put/get/delete`. FathomDB is **write-centric**. Integration puts file-scanning in an ingester layer above FathomDB.
- OpenClaw memory targets **prompt-context filler**. FathomDB targets **durable agent state** across runs/steps/actions. Overlapping tech, distinct scope.
- FathomDB vector identity invariant: embedders own identity strings. OpenClaw today caches embeddings keyed by `provider/model/hash` — compatible direction, stricter enforcement.

## References

- OpenClaw `MemorySearchManager` interface: `packages/memory-host-sdk/src/host/types.ts:61`
- OpenClaw backend selection: `src/memory-host-sdk/host/backend-config.ts:79` (default `"builtin"`), `extensions/memory-core/src/memory/search-manager.ts:54-133`
- FathomDB Engine: `crates/fathomdb/src/lib.rs`
- FathomDB search/query builders: `crates/fathomdb/src/search.rs`
- FathomDB write builder: `crates/fathomdb/src/write_request_builder.rs`
- FathomDB schema types: `crates/fathomdb-schema/src/lib.rs`
