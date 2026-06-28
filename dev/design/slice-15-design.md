# Slice 15 Design Memo — G11 edge enrichment + BYO-LLM ingest + edge projectability

> Status: IMPLEMENTATION DESIGN — informs the Slice 15 keystone implementation.
> ADR authority: ADR-0.8.1-graph-substrate-g11-migration.md + ADR-0.8.1-byo-llm-extraction-protocol.md.
> Schema: SCHEMA_VERSION 13 → 14.

---

## 1. G11 step-14 migration — exact SQL

`fathomdb-schema/src/lib.rs` MIGRATIONS array, after step-13:

```rust
Migration {
    step_id: 14,
    sql: "-- MIGRATION-ACCRETION-EXEMPTION: G11 edge enrichment (5 additive nullable columns)
          ALTER TABLE canonical_edges ADD COLUMN body TEXT;
          ALTER TABLE canonical_edges ADD COLUMN t_valid TEXT;
          ALTER TABLE canonical_edges ADD COLUMN t_invalid TEXT;
          ALTER TABLE canonical_edges ADD COLUMN confidence REAL;
          ALTER TABLE canonical_edges ADD COLUMN extractor_model_id TEXT;
          CREATE VIRTUAL TABLE IF NOT EXISTS search_index_edges USING fts5(
              body,
              kind UNINDEXED,
              write_cursor UNINDEXED,
              tokenize = 'porter unicode61 remove_diacritics 2'
          );",
}
```

**Migration runner compatibility:** `apply_one` uses `conn.execute_batch(migration.sql)` which handles
multiple semicolon-separated statements. Verified from step-8 (multiple ALTER TABLE + CREATE INDEX),
step-12 (multiple ALTER TABLE + CREATE INDEX), all in one SQL string.

**Accretion guard:** The step-14 SQL has ADD COLUMN (triggers the guard) but no DROP TABLE,
so the `-- MIGRATION-ACCRETION-EXEMPTION:` marker is REQUIRED. The `search_index_edges` CREATE
TABLE IF NOT EXISTS is purely additive.

**SCHEMA_VERSION:** Bumped from 13 → 14 in `SCHEMA_VERSION: u32 = 14`.

---

## 2. BYO-LLM API shape

### 2.1 Rust method signature

```rust
pub fn ingest_with_extractor(
    &self,
    provider_cmd: &[&str],
    documents: &[ExtractDocument],
) -> Result<IngestWithExtractorReceipt, EngineError>
```

In `src/rust/crates/fathomdb-engine/src/lib.rs`, as a `pub fn` on `Engine`.

### 2.2 Supporting types

```rust
pub struct ExtractDocument {
    pub source_doc_id: String,
    pub body: String,
}

pub struct IngestWithExtractorReceipt {
    pub nodes_written: u64,
    pub edges_written: u64,
    pub docs_processed: u64,
}
```

### 2.3 Spawn/handshake loop

1. `Command::new(provider_cmd[0]).args(&provider_cmd[1..])` with `stdin(Stdio::piped())`, `stdout(Stdio::piped())`, `stderr(Stdio::inherit())`.
2. Write `{"protocol":"fathomdb.extract.v1","type":"hello"}\n` to stdin.
3. Read one line from stdout; parse as JSON.
4. Validate: `protocol == "fathomdb.extract.v1"` AND `schema_version == 1`; else return `EngineError::Extractor` with code "ProtocolMismatch".
5. Record `ready.model` as `extractor_model_id_for_ingest`.
6. Read `max_docs_per_request` from ready (default 8 if absent).

### 2.4 Error propagation

- `ProtocolMismatch` → `EngineError::Extractor`
- `llm_unavailable` → `EngineError::Extractor`
- `extraction_failed` → `EngineError::Extractor`
- `timeout` → `EngineError::Extractor`
- `invalid_request` → `EngineError::Extractor`

On error: write `[WARN] fathomdb: extractor returned error code=…` to stderr (no panic).

---

## 3. `logical_id` derivation for entities

Algorithm: `sha256(type.to_lowercase() + ":" + name.to_lowercase())` — hex-encoded lowercase string.

Rationale:

- Deterministic: same (name, type) → same logical_id across runs
- Collision-resistant: sha256 (2^256 space)
- Type-scoped: different entity types with same name get different logical_ids
- Case-insensitive: normalizes to lowercase first

Example: entity `{name: "Alice", type: "PERSON"}` → `sha256("person:alice")` → hex string

Implementation: uses `sha2` crate (already in workspace? check) or `ring` or hand-rolled hex of
sha256 from `std`. If no sha2/ring dep, use a simple FNV-1a + base36 fallback, but sha256 is preferred.

If sha256 crate not available: derive from `format!("{}:{}", type.to_lowercase(), name.to_lowercase())`
hashed with std's DefaultHasher (non-cryptographic but deterministic within process — NOT suitable
for cross-process stability). Better: use the `sha2` crate.

Check: `Cargo.toml` workspace deps for `sha2`.

---

## 4. Edge projectability mechanism

### 4.1 FTS: Option B — separate `search_index_edges` FTS5 table

**Decision: Option B (separate table).**

Rationale:

- Option A requires ALTER TABLE on FTS5 virtual table (not supported; would need drop+recreate,
  losing all indexed data and requiring re-projection — same as step-11, but worse here since
  we'd need to re-tokenize all existing node bodies)
- Option B is purely additive: CREATE TABLE IF NOT EXISTS in step-14 SQL
- Minimal change to existing search path: existing `search_index` and node search are untouched
- Query path: `Engine::search_filtered` adds a second FTS query on `search_index_edges`

Edge FTS hits are distinguishable: they come from `search_index_edges`, so `branch = "text_edge"` (vs `"text"` for node hits).

### 4.2 Vector: extend `next_pending_projection_jobs` with UNION

**Decision: extend the projection scheduler to also process edge bodies.**

Mechanism:

- When an edge with non-null body is written, do NOT call `record_projection_terminal`; instead
  call nothing (let the scheduler pick it up via the UNION query)
- Add `"edge_fact"` to `resolve_source_type()`: returns `"edge_fact"`
- Auto-register `"edge_fact"` in `_fathomdb_vector_kinds` when engine opens (similar to how node
  kinds are registered via `_configure_vector_kind`)
- Extend `next_pending_projection_jobs` with UNION:

  ```sql
  SELECT canonical_edges.write_cursor, 'edge_fact' AS kind, canonical_edges.body
  FROM canonical_edges
  LEFT JOIN _fathomdb_projection_terminal
    ON _fathomdb_projection_terminal.write_cursor = canonical_edges.write_cursor
  WHERE canonical_edges.write_cursor > ?1
    AND canonical_edges.body IS NOT NULL
    AND _fathomdb_projection_terminal.write_cursor IS NULL
  ```

**Partition correctness:** `source_type = "edge_fact"` in `vector_default` distinguishes edge-body
vectors from node-body vectors (`source_type` is a vec0 partition key → different rows).

**Kind registration:** `"edge_fact"` is registered in `_fathomdb_vector_kinds` automatically when
first edge with body is written (in `commit_batch`), or at engine open time if any `canonical_edges`
rows with body exist.

Actually simpler: register it in `commit_batch` when we write an edge with non-null body:

```sql
INSERT OR IGNORE INTO _fathomdb_vector_kinds(kind, profile, created_at)
VALUES('edge_fact', 'default', ?)
```

This is idempotent (`INSERT OR IGNORE`).

---

## 5. Invalidate-not-accumulate — exact SQL

For edges written via the BYO-LLM ingest path (when `body IS NOT NULL` — fact-edges):

```sql
-- Before inserting new enriched row:
UPDATE canonical_edges
SET superseded_at = ?cursor
WHERE from_id = ?from_id
  AND to_id = ?to_id
  AND kind = ?kind
  AND superseded_at IS NULL;
```

Then insert the new row.

**Distinguishing BYO-LLM edges from regular edges:**

- Regular edges (written via `PreparedWrite::Edge` with `body: None`) retain the existing G0
  `logical_id`-based supersession semantics
- Fact-edges (written with `body: Some(...)`) trigger the `(from_id, to_id, kind)` invalidation
- The condition `body IS NOT NULL` in the `PreparedWrite::Edge` match arm determines which
  invalidation path to take

---

## 6. Python/TS API shape

### 6.1 Python

```python
class ExtractDocument:
    source_doc_id: str
    body: str

class IngestReceipt:
    nodes_written: int
    edges_written: int
    docs_processed: int

class Engine:
    def ingest_with_extractor(
        self,
        provider_cmd: list[str],
        documents: list[ExtractDocument],
    ) -> IngestReceipt: ...
```

The PyO3 binding calls `Engine::ingest_with_extractor` on the Rust engine.

### 6.2 TypeScript

```typescript
export interface ExtractDocument {
  sourceDocId: string
  body: string
}
export interface IngestReceipt {
  nodesWritten: number
  edgesWritten: number
  docsProcessed: number
}
// On Engine class:
ingestWithExtractor(
  providerCmd: Array<string>,
  documents: Array<ExtractDocument>
): Promise<IngestReceipt>
```

---

## 7. Conformance fixture harness

Location: `src/rust/crates/fathomdb-engine/tests/fixtures/slice15_byo_llm/stub_harness.py`

The stub harness:

1. Reads NDJSON lines from stdin
2. On `hello` → writes `ready` with `model="stub-v1"`, `max_docs_per_request=8`
3. On `extract` → returns deterministic fixture from `fixture_result.json` (keyed by request_id)
4. On unknown type → writes `error` response

The fixture covers:

- Simple fact: `{from: "Alice", to: "Project X", relation: "owns", body: "Alice owns the project"}`
- Temporal fact: same triple with `t_valid`/`t_invalid`
- Multi-entity sentence: multiple entities + edges
- No-facts doc: empty entities+edges → `no_facts` warning

Footprint test: no network socket opened. The engine spawns only the stub harness (a local Python
script) which never makes network calls. Test verifies via `assert no network call made during
test run` — implemented by checking `/proc/self/net/tcp` before and after, or simply asserting
that the stub harness script path is local (starts with `/`).

---

## 8. Additional notes

### 8.1 sha256 dependency check

Will check `Cargo.toml` workspace for sha2/ring. If not present, add `sha2 = "0.10"` to
`fathomdb-engine/Cargo.toml`. Alternative: use `std::collections::hash_map::DefaultHasher`
with hex encoding as a deterministic-within-session fallback, but sha256 is required for
cross-process/cross-session stability.

### 8.2 `search_filtered` vs `search` extension

The current `Engine::search_filtered` handles the text branch. Adding edge FTS results:

- Run `SELECT body, kind, write_cursor, bm25(search_index_edges) FROM search_index_edges WHERE search_index_edges MATCH ?1`
- Combine with node FTS results, return with `branch = "text_edge"`
- The combined results are returned in the `SearchResult.results` list

OR: keep edge FTS separate — `Engine::search` does NOT return edge bodies by default;
a future `Engine::search_graph` (Slice 30) can query both. For Slice 15, the test `edge_fts_searchable`
can query `search_index_edges` directly via a test-only path, or we expose it via a new
`search_edges` method.

**Decision: Option B-simpler — add edge FTS results to `search_filtered` response, tagged with
`branch = "text_edge"`.** This satisfies AC5 "distinguishable from node bodies in query results"
without adding a new method.

### 8.3 Handling `_fathomdb_vector_kinds` for "edge_fact"

Test hook `_configure_vector_kind_for_test(kind: "edge_fact")` will work. For production,
register "edge_fact" in `_fathomdb_vector_kinds` when first edge body is written in `commit_batch`.
