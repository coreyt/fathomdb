# Design: External Content Objects

## Problem

FathomDB stores structured knowledge as nodes, edges, and chunks. But much of
the information an AI agent needs lives in objects too large or too varied to
store in a SQLite database: PDFs, DOCX files, MP3/MP4 media, multi-gigabyte
datasets, DOI-referenced papers, web pages. FathomDB is not a RAG database, but
it needs to **know about** these objects -- their metadata, their relevance
signals, enough extracted text to make them discoverable when an agent or user
asks a question.

Additionally, these objects live in wildly different places: a 3 MB PDF on the
local filesystem, a 1 TB dataset behind a web interface, an IMDB page that
should be cached, a DOI that resolves through a registry. FathomDB needs an
adapter layer that abstracts over storage and retrieval without trying to become
a general-purpose object store.

## Design Constraints

- **Local-first**: the metadata and search indexes must work offline.
- **No schema rigidity**: heterogeneous content types must not require per-type
  schema migrations (learned from NEPOMUK's failure on the KDE Semantic Desktop).
- **Lazy over eager**: full content extraction is expensive; do it on demand or
  in the background, not as a blocking prerequisite.
- **Leverage existing primitives**: nodes, edges, chunks, the operational store,
  provenance, and the write builder already exist. Prefer composition over new
  engine-level abstractions.
- **Agent-oriented**: the primary consumer is an AI agent deciding "what do I
  know about that might help answer this question?" -- not a human browsing a
  file manager.

## Research Summary

### What works (proven patterns)

| Pattern | Evidence |
|---|---|
| **Thin metadata node + content pointer** | DEVONthink's "index" mode (metadata + path, no duplication) is preferred by power users over "import" mode. Palantir Foundry's ontology maps external datasources into typed objects without inlining content. |
| **Content-addressable hashing** | Git and IPFS use content hashes as primary identifiers. Detects staleness, enables deduplication, works across storage backends. |
| **Lazy extraction with cached results** | Apache Tika / Solr extract on first access, cache results, re-extract on content-hash change. Microsoft Recall and Rewind.ai index lazily on-device at scale. |
| **Dual retrieval (FTS + vector)** | "Vague recollection" retrieval -- "I saw something about X months ago" -- requires both keyword and semantic search. All modern PKM systems (Recall, Rewind, Mem.ai) use both. |
| **URI-scheme resolver dispatch** | DOI resolution, IPFS gateways, and data catalog systems (Apache Iceberg, AWS Glue) all use scheme-based dispatch to resolve heterogeneous references. |

### What fails

| Anti-pattern | Evidence |
|---|---|
| **Heavy ontology enforcement** | NEPOMUK required RDF/SPARQL for every desktop operation; performance was unusable, project abandoned in favor of Baloo's simpler approach. |
| **Eager full-content import** | Duplicates storage, creates sync problems, and blocks on extraction. DEVONthink community and Zotero users consistently prefer linked references. |
| **Rigid per-type schemas** | Every new content type requiring a migration creates friction. Flexible metadata (JSON properties) with optional type-specific conventions scales better. |

### Memex relevance

Bush's original Memex concept emphasized **associative trails** -- user-created
links between items that capture subjective relevance. The 2026 MemexRL paper
(arXiv:2603.04257) applies this to AI agents: compact summaries in an active
index, full content in an external key-value store, dereferenced on demand. This
is the architecture FathomDB should target: the graph is the index of
associations, external stores hold the bulk content.

---

## Option A: Content Object Nodes (Convention-Based)

**Approach**: External content objects are regular nodes with a reserved `kind`
prefix (`content.*`) and a structured `properties` contract. Chunks hold
extracted text. Edges link content to the rest of the knowledge graph. No engine
changes required.

### Data Model

```
Node (kind: "content.pdf")
  properties: {
    "uri": "file:///home/user/papers/attention.pdf",
    "content_hash": "sha256:e3b0c44298fc...",
    "mime_type": "application/pdf",
    "byte_size": 3145728,
    "title": "Attention Is All You Need",
    "extracted_at": "2026-04-10T12:00:00Z",
    "adapter": "local_file",
    "adapter_meta": { "watched": true }
  }

Chunks (node_logical_id → above node)
  chunk 1: "Abstract: The dominant sequence transduction models..."
  chunk 2: "We propose a new simple network architecture..."
  ...each chunk gets FTS + vector indexing automatically

Edges
  (content node) --[content.references]--> (other node)
  (meeting node) --[content.attachment]--> (content node)
```

### Content Adapter

Adapters are application-level code (not engine code) that implement a simple
interface:

```python
class ContentAdapter(Protocol):
    """Resolves a URI to content metadata and extracted text."""

    def resolve(self, uri: str) -> ContentMetadata:
        """Fetch metadata without downloading full content."""
        ...

    def extract(self, uri: str) -> ExtractedContent:
        """Download (if needed) and extract searchable text + metadata."""
        ...

    def is_stale(self, uri: str, content_hash: str) -> bool:
        """Check if cached extraction is outdated."""
        ...
```

Adapter dispatch by URI scheme:

| Scheme | Adapter | Behavior |
|---|---|---|
| `file://` | `LocalFileAdapter` | Hash local file, extract via Tika/unstructured, watch for changes |
| `https://` | `WebAdapter` | Fetch with caching (ETag/Last-Modified), extract text, respect robots.txt |
| `doi:` | `DoiAdapter` | Resolve via doi.org, delegate to WebAdapter for resolved URL |
| `s3://` | `S3Adapter` | Stream object metadata from S3, extract on demand |
| `imdb:` | `ImdbAdapter` | Structured scrape/API, cache with TTL |

Adapter state (sync timestamps, error counts, fetch history) stored in the
operational store:

```
collection: "content_adapter_sync"
record_key: "sha256:e3b0c44298fc..."
payload: {
  "uri": "file:///home/user/papers/attention.pdf",
  "last_sync": "2026-04-10T12:00:00Z",
  "status": "ok",
  "extract_duration_ms": 1200
}
```

### Strengths

- **Zero engine changes**. Ships as a Python/TypeScript library on top of the
  existing SDK. Can iterate without touching Rust.
- **Fully leverages existing search**. Chunks go through the same FTS + vector
  pipeline as any other content.
- **Flexible**. New content types are just new `kind` values and adapter
  implementations.
- **Provenance works naturally**. `source_ref` on the content node traces back
  to the adapter run that created it. `excise_source()` cleans up a bad import.

### Weaknesses

- **Convention, not contract**. Nothing prevents a malformed content node. An
  agent could write a `content.pdf` node missing `content_hash` or `uri`.
  Relies on application discipline or validation in the adapter library.
- **No native content resolution**. The engine doesn't know how to fetch
  content -- the application must call the adapter. Queries return metadata; the
  caller must dereference URIs themselves.
- **Chunk management is manual**. Re-extraction on content change requires the
  adapter to delete old chunks and write new ones (using `ChunkPolicy.REPLACE`
  on upsert).

---

## Option B: Content Object as a First-Class Engine Concept

**Approach**: Add a `content_objects` table to the engine schema alongside nodes,
edges, and chunks. Content objects have dedicated columns for URI, content hash,
MIME type, and extraction state. The engine manages the lifecycle (staleness
detection, extraction state machine) while adapters plug in via a trait.

### Schema Addition

```sql
CREATE TABLE content_objects (
    row_id          TEXT PRIMARY KEY,
    logical_id      TEXT NOT NULL,
    uri             TEXT NOT NULL,
    content_hash    TEXT,            -- NULL until first extraction
    mime_type       TEXT,
    byte_size       INTEGER,
    title           TEXT,
    properties      BLOB NOT NULL,   -- Additional metadata (JSON)
    extraction_state TEXT NOT NULL DEFAULT 'pending',
                    -- pending | extracting | extracted | failed | stale
    extracted_at    INTEGER,
    created_at      INTEGER NOT NULL,
    superseded_at   INTEGER,
    source_ref      TEXT
);

CREATE INDEX idx_content_objects_uri ON content_objects(uri)
    WHERE superseded_at IS NULL;
CREATE INDEX idx_content_objects_state ON content_objects(extraction_state)
    WHERE superseded_at IS NULL;
```

Content objects participate in the same supersession model as nodes. Chunks
link to content objects via `node_logical_id` (content objects are queryable
alongside nodes).

### Content Adapter (Engine-Level Trait)

```rust
pub trait ContentResolver: Send + Sync + 'static {
    /// Resolve metadata without fetching full content.
    fn resolve(&self, uri: &str) -> Result<ContentMetadata, ResolverError>;

    /// Extract searchable text from content.
    fn extract(&self, uri: &str) -> Result<ExtractionResult, ResolverError>;

    /// Check if a previously extracted content hash is still current.
    fn is_current(&self, uri: &str, content_hash: &str) -> Result<bool, ResolverError>;
}

pub struct ContentMetadata {
    pub mime_type: Option<String>,
    pub byte_size: Option<u64>,
    pub title: Option<String>,
    pub content_hash: Option<String>,
    pub properties: serde_json::Value,
}

pub struct ExtractionResult {
    pub content_hash: String,
    pub chunks: Vec<ExtractedChunk>,
    pub metadata: ContentMetadata,
}
```

The engine accepts a `Vec<Box<dyn ContentResolver>>` at open time, dispatching
by URI scheme. A background thread (or the admin service) drives the extraction
state machine:

```
pending → extracting → extracted
                    ↘ failed (with backoff)
extracted → stale (on content_hash mismatch) → extracting
```

### Write Builder Extension

```python
builder.add_content_object(
    logical_id="paper-attention",
    uri="doi:10.48550/arXiv.1706.03762",
    title="Attention Is All You Need",
    properties={"authors": ["Vaswani et al."], "year": 2017},
    source_ref="import/arxiv-crawl-42",
    upsert=True
)
```

### Strengths

- **Enforced contract**. URI and extraction state are schema-level columns, not
  JSON conventions. Impossible to create a content object without a URI.
- **Engine-managed lifecycle**. Staleness detection, re-extraction, and state
  tracking happen inside the engine. Agents don't need to manage chunk cleanup.
- **Queryable as first-class entities**. Content objects can appear in query
  results alongside nodes, with dedicated filters (`extraction_state = 'extracted'`,
  `mime_type LIKE 'application/pdf'`).

### Weaknesses

- **Schema migration required**. Adds a new table and indexes. Increases engine
  surface area and maintenance burden.
- **Resolver coupling**. The engine now depends on external I/O (network, filesystem)
  via the resolver trait. This complicates testing, error handling, and the
  single-writer model (extraction must not block the writer thread).
- **Cross-language binding cost**. The `ContentResolver` trait must be exposed
  through PyO3 and napi-rs. Callback-based traits across FFI are complex
  (Python callables holding the GIL, JS async functions).
- **Scope creep risk**. The extraction state machine, background scheduler, and
  retry logic are substantial new engine complexity for a feature that could live
  in application space.

---

## Option C: Hybrid -- Thin Engine Primitive + Application Adapter Library

**Approach**: Add minimal engine support -- a `content_ref` field on nodes and a
`content_hash` field on chunks -- then build the adapter layer, extraction
pipeline, and lifecycle management as an application-level library. The engine
knows just enough to enable content-aware queries without owning the full
lifecycle.

### Engine Changes (Minimal)

**Migration v-next: add content_ref to nodes**

```sql
ALTER TABLE nodes ADD COLUMN content_ref TEXT;
-- A URI or content-addressable identifier for the external object
-- this node represents. NULL for nodes that are not content proxies.

CREATE INDEX idx_nodes_content_ref ON nodes(content_ref)
    WHERE content_ref IS NOT NULL AND superseded_at IS NULL;
```

**Migration v-next: add content_hash to chunks**

```sql
ALTER TABLE chunks ADD COLUMN content_hash TEXT;
-- SHA-256 of the source content this chunk was extracted from.
-- Enables staleness detection: if the source hash changes,
-- these chunks need re-extraction.
```

### Application-Level Library: `fathomdb.content`

```python
from fathomdb.content import ContentManager, LocalFileAdapter, WebAdapter, DoiAdapter

# Register adapters by URI scheme
manager = ContentManager(engine)
manager.register("file", LocalFileAdapter(watch=True))
manager.register("https", WebAdapter(cache_dir="~/.fathomdb/cache"))
manager.register("doi", DoiAdapter())

# Ingest: creates node + chunks + vector embeddings
receipt = manager.ingest(
    uri="file:///home/user/papers/attention.pdf",
    kind="content.paper",
    properties={"tags": ["ml", "transformers"]},
    source_ref="import/paper-scan"
)

# Refresh: re-extract if content_hash changed
stale = manager.find_stale()  # queries nodes where content_ref hash != chunk content_hash
manager.refresh(stale)

# Query (standard fathomdb query -- chunks are already indexed)
rows = engine.query("content.paper").text_search("attention mechanism").execute()
```

### How It Works

1. **Ingest**: `ContentManager.ingest(uri)` calls the appropriate adapter to
   resolve metadata and extract text. It then builds a write request:
   - One node with `kind="content.<type>"`, `content_ref=uri`,
     `properties={metadata}`.
   - N chunks with `content_hash=sha256(source)`, `text_content=extracted_text`,
     byte offsets for source navigation.
   - Vector embeddings on each chunk (if an embedding function is configured).
   - Edges linking to related nodes (if the caller provides them).

2. **Staleness detection**: `find_stale()` queries for content nodes, calls
   `adapter.is_current(uri, content_hash)`, returns nodes needing refresh.
   The `content_hash` on chunks makes this a simple comparison without
   re-downloading content.

3. **Refresh**: `refresh(nodes)` re-extracts, upserts the node (with
   `ChunkPolicy.REPLACE` to swap chunks), and updates `content_hash`.

4. **Adapter state**: Sync history, error counts, and scheduling stored in the
   operational store (collection: `content_sync`). This enables retry with
   backoff, audit trails, and admin visibility.

### Adapter Interface

```python
class ContentAdapter(Protocol):
    def resolve(self, uri: str) -> ContentMetadata:
        """Fetch metadata (title, size, mime_type, content_hash)
        without downloading full content when possible."""
        ...

    def extract(self, uri: str) -> ExtractionResult:
        """Download if needed, extract text chunks and metadata."""
        ...

    def is_current(self, uri: str, content_hash: str) -> bool:
        """Return True if the remote content still matches the hash."""
        ...

@dataclass
class ContentMetadata:
    title: str | None
    mime_type: str | None
    byte_size: int | None
    content_hash: str | None  # SHA-256 of raw content
    properties: dict           # Adapter-specific metadata

@dataclass
class ExtractionResult:
    metadata: ContentMetadata
    chunks: list[ExtractedChunk]  # text + byte offsets

@dataclass
class ExtractedChunk:
    text: str
    byte_start: int | None
    byte_end: int | None
```

### Staleness Flow

```
             ┌─────────────┐
             │ content node │
             │ content_ref  │──── URI ────► adapter.is_current(uri, hash)
             └──────┬───────┘                      │
                    │                          ┌────┴────┐
              ┌─────┴─────┐                    │ current │ stale │
              │  chunks[]  │                    └────┬────┘───┬───┘
              │content_hash│                         │        │
              └────────────┘                     (no-op)  re-extract
                                                          upsert node
                                                          REPLACE chunks
```

### Strengths

- **Minimal engine surface**. Two nullable columns and one index. No new tables,
  no state machines, no resolver traits in Rust. The engine stays focused on
  storage and search.
- **Content-aware queries without engine coupling**. The `content_ref` column
  lets queries filter "nodes that represent external content" efficiently.
  `content_hash` on chunks enables staleness detection in a single SQL query.
- **Full application flexibility**. The adapter library can evolve independently
  of engine releases. New adapters, new extraction pipelines, new scheduling
  strategies -- all without schema migrations.
- **Cross-language parity is cheap**. The two new columns flow through existing
  node/chunk write paths. The adapter library needs implementation in each
  language, but the engine bindings require no changes beyond exposing the
  new fields.
- **Leverages existing primitives**. Supersession, provenance, chunks,
  FTS + vector, the operational store -- all used as designed.
- **Memex-aligned**. Follows the MemexRL pattern: compact summaries (chunks) in
  the active index, full content in external storage, dereferenced on demand.
  Associative trails are edges between content nodes and knowledge nodes.

### Weaknesses

- **Convention still matters**. The `content_ref` column is typed guidance, but
  the adapter library's behavior (chunk structure, metadata shape, extraction
  quality) is still convention-based.
- **Two-place schema change**. Even minimal, adding columns to `nodes` and
  `chunks` is a migration that must be tested across all platforms and language
  bindings.
- **No engine-level lifecycle guarantees**. If the application crashes
  mid-extraction, the node might exist without chunks. The adapter library must
  handle partial state. (Mitigated: writes are atomic per request, so either
  the node + chunks commit together or neither does.)

---

## Comparison

| Dimension | A: Convention | B: First-Class | C: Hybrid |
|---|---|---|---|
| Engine changes | None | New table + trait + state machine | 2 columns + 1 index |
| Contract enforcement | Application convention | Schema-level | Column + convention |
| Extraction lifecycle | Application-managed | Engine-managed | Application-managed |
| Query integration | Via `kind` filter | Dedicated query surface | Via `content_ref IS NOT NULL` |
| Cross-language cost | Zero | High (FFI callbacks) | Low (new fields only) |
| Time to ship | Days | Months | 1-2 weeks |
| Scope creep risk | Low | High | Low |
| Memex alignment | Good | Good | Best |

## Recommendation

**Option C (Hybrid)** is the recommended approach. It provides the engine with
just enough awareness of external content to enable efficient queries and
staleness detection, while keeping the complex adapter/extraction/lifecycle
logic in application space where it can iterate quickly. This follows the proven
"thin metadata + pointer" pattern from DEVONthink and Foundry, the lazy
extraction pattern from Tika/Recall, and the MemexRL architecture of compact
index entries over external content stores.

Option A is a reasonable starting point if we want zero engine changes and are
willing to accept weaker query integration. Option B is appropriate if external
content becomes a core engine concern with strict lifecycle guarantees, but the
engineering cost and scope creep risk are significant.
