As an elite database engineer tasked with bringing `fathomdb` to life, I look at SQLite not merely as a storage format, but as a high-performance, bytecode-executing VM for relational algebra. To transform SQLite into a multimodal (Graph, Document, Vector, FTS) engine for AI agents, we must build a highly opinionated **Query Engine and Compiler Shim**.

The challenge isn’t storing the data—SQLite handles that flawlessly. The challenge is **query planning and multi-domain execution**. If we blindly stitch together JSON filtering, graph traversal, and vector search, we will hit massive N+1 latency spikes or application-memory blowups.

Here is the deep architectural design for the `fathomdb` Query Engine, moving from the agent's programmatic intent down to the physical SQLite execution layer.

---

### 1. The Core Architecture of the Query Engine

The engine is divided into four distinct strata:
1. **The Fluent AST Builder (SDK):** Deterministic, composable API for the agent.
2. **The Query Compiler:** Translates the AST into a single, cohesive SQLite query, pushing bounds down to the deepest possible level.
3. **The Execution Coordinator:** Manages SQLite connections, WAL mode, prepared statements, and concurrent reader/single-writer disciplines.
4. **The Write/Projection Pipeline:** A transactional coordinator that handles append-only canonical writes, embedding lookups, and projection synchronization.

---

### 2. The Fluent AST Builder (The Agent Interface)

Agents shouldn't write SQL. LLMs hallucinate syntax, misunderstand schema evolution, and fail at complex `JOIN` or CTE logic. Instead, the agent interacts with a fluent, chainable API that builds an **Abstract Syntax Tree (AST)**.

```rust
// Conceptual AST representation
enum QueryStep {
    VectorSearch { query: Vec<f32>, limit: usize, threshold: f32 },
    TextSearch { query: String },
    Traverse { direction: Direction, edge_kind: String, depth: Range<u32> },
    FilterNode { predicate: JsonPredicate },
    FilterTime { time_range: TimeRange },
    JoinSemantic { table: String, on: String },
}
```

**Example Agent Invocation (Python/TS SDK):**
```python
# The agent wants to find high-confidence active tasks related to a specific stressful meeting
results = (
    db.nodes("Meeting")
      .vector_search("stressful work discussion", limit=5)
      .traverse(direction="out", label="GENERATED_TASK", depth=1)
      .filter(lambda node: node.properties["status"] == "active")
      .filter(lambda node: node.confidence > 0.8)
      .select("id", "properties.title", "created_at")
      .execute()
)
```

---

### 3. The Query Compiler (Translating AST to Optimized SQLite)

The Compiler is the brain of `fathomdb`. Its primary job is **Candidate Set Reduction (Top-K Pushdown)**. 

SQLite's JSON processing (`JSON1`/`JSONB`) is fast, but full table scans on JSON blobs are fatal. Therefore, the compiler must re-order AST steps to ensure the database engine uses the most restrictive index—usually FTS or Vector—as the **Driving Table**.

#### Compilation Strategy: The "Inside-Out" Query
For the query above, the compiler generates a SQL statement that executes exactly in the order of restrictiveness.

1. **The Driving Table (Vector Limit):** Evaluate `vec_nodes` first.
2. **The Graph Traversal:** Join the `edges` table based on the vector results.
3. **The Target Nodes:** Join the canonical `nodes` table.
4. **The Filters:** Apply JSON and relational filters *only* to the resolved graph targets.

**Compiled SQL Output:**
```sql
SELECT 
    target_node.id, 
    target_node.properties ->> '$.title' AS title, 
    target_node.created_at
FROM (
    -- 1. Narrowest Filter: Vector Pushdown
    SELECT source_id, distance
    FROM vec_nodes
    WHERE embedding MATCH ?  -- SQLite-vec match
    ORDER BY distance
    LIMIT 5
) AS vec_match
-- 2. Known-Depth Traversal
INNER JOIN edges e 
    ON vec_match.source_id = e.source_id 
    AND e.kind = 'GENERATED_TASK'
    AND e.superseded_at IS NULL
-- 3. Target Node Retrieval
INNER JOIN nodes target_node 
    ON e.target_id = target_node.id
    AND target_node.superseded_at IS NULL
-- 4. Late-stage Filtering (Evaluated only on the 5 * edges subset)
WHERE 
    target_node.confidence > 0.8 
    AND target_node.properties ->> '$.status' = 'active';
```

#### Handling Variable-Depth Traversals
If the agent requested `depth=1..3` (e.g., finding transitive task dependencies), the compiler swaps the standard `INNER JOIN` for a **Recursive CTE**:

```sql
WITH RECURSIVE traverse AS (
    -- Base case: Starts from Vector matches
    SELECT source_id AS current_id, 0 AS depth
    FROM vec_nodes WHERE embedding MATCH ? LIMIT 5

    UNION ALL

    -- Recursive step
    SELECT e.target_id, t.depth + 1
    FROM edges e
    INNER JOIN traverse t ON e.source_id = t.current_id
    WHERE e.kind = 'DEPENDS_ON' AND t.depth < 3 AND e.superseded_at IS NULL
)
...
```

---

### 4. Execution & Optimization Specifics

To make this engine scream on local hardware, we must leverage specific SQLite features:

1. **JSONB (SQLite 3.45+):** We explicitly compile the `properties` blob using SQLite's new internal `JSONB` format. This prevents SQLite from repeatedly parsing text-based JSON during `.filter()` evaluations, yielding a 2x-3x speedup on JSON extraction.
2. **Generated Columns for Hot Paths:** The engine dynamically profiles slow queries. If it notices the agent frequently filtering by `properties ->> '$.status'`, `fathomdb` can transparently execute:
   ```sql
   ALTER TABLE nodes ADD COLUMN _idx_status TEXT AS (properties ->> '$.status') VIRTUAL;
   CREATE INDEX idx_nodes_status ON nodes(_idx_status);
   ```
3. **Prepared Statement Caching:** The compilation phase produces parameterized SQL. The engine maintains an LRU cache of these compiled `sqlite3_stmt` pointers.
4. **WAL Mode & Connection Pooling:** We enforce `PRAGMA journal_mode = WAL;` and `PRAGMA synchronous = NORMAL;`. We provide the application with a connection pool: *N* read-only connections and *exactly 1* strictly serialized write connection.

---

### 5. The Governed Write Pipeline (Transactional Synchronization)

Because we eschew SQLite Triggers (they make debugging impossible and swallow errors), `fathomdb`'s write path uses a **Coordinated Write Pipeline**. 

Because LLM API calls (for embeddings) are slow, they **cannot** happen inside an active SQLite transaction, or we will trigger `SQLITE_BUSY` for all other operations.

**The Ingestion Flow:**
1. **Pre-Flight (Async):** The shim receives the document, parses it, extracts JSON, and calls the local/remote embedding model to generate the vector array.
2. **Transaction Lock (`BEGIN IMMEDIATE`):** Only acquired *after* all heavy computation is complete.
3. **Canonical Append:** Write to `nodes`, `edges`, and typed side-tables (e.g., `actions`, `observations`). 
   * *Superseding logic:* If updating an existing node, execute `UPDATE nodes SET superseded_at = ? WHERE id = ?` and `INSERT` the new version.
4. **Projection Sync:** 
   * Write text to `fts_nodes` (FTS5).
   * Write embedding to `vec_nodes` (`sqlite-vec`).
5. **Commit:** The database is unlocked.

By bundling this into a single atomic SQLite transaction, we guarantee that the Vector Index, FTS Index, and Graph Backbone never drift. If the app crashes on step 4, the entire graph update rolls back cleanly.

---

### 6. Temporal, Provenance, and Replay Capabilities

AI Agents need to "think out loud" and sometimes rewind their mistakes. To support this without complex temporal logic bleeding into standard queries, the engine uses **View Abstractions**.

By default, all SDK read queries transparently target an internal concept of `ACTIVE_STATE` (where `superseded_at IS NULL`).

However, the SDK allows for **Temporal Scoping**:
```python
# Query the world model exactly as the agent saw it yesterday
db.at_time(yesterday_timestamp).nodes("Meeting").execute()
```
The compiler trivially handles this by swapping the default temporal filter:
```sql
-- Instead of: WHERE superseded_at IS NULL
WHERE created_at <= ? AND (superseded_at IS NULL OR superseded_at > ?)
```

**Provenance Joins:**
Because the architecture dictates typed semantic side-tables, the compiler supports `explain()` routes. If an agent questions *why* a node exists, the query engine automatically joins the `control_artifacts` or `actions` table using the `source_ref` foreign key, retrieving the exact LLM prompt, context window, and confidence score that generated that node.

### Summary of Engineering Outcomes

By structuring `fathomdb` this way, we achieve:
* **Zero N+1 queries:** Multimodal hops happen entirely inside the SQLite VM C-code via JIT-compiled nested loops.
* **Rock-solid state:** Projections (FTS/Vector) are synchronized deterministically in application code, inside atomic transactions, rather than relying on brittle database triggers.
* **Bounded memory footprint:** Because data fetching is heavily pushed down (Top-K limits applied at the FTS/Vector layer *before* graph joins), the application memory only ever processes small, highly relevant sub-graphs. 

This engine transforms SQLite from a passive storage bin into an active, high-speed semantic reasoning substrate for local AI.
