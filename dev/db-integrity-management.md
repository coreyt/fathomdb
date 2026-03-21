In a local AI system, "corruption" takes three distinct forms:
1. **Physical Corruption:** The disk flipped a bit or the OS crashed mid-write.
2. **Logical Corruption (State Divergence):** The FTS or Vector index gets out of sync with the canonical graph.
3. **Semantic Corruption:** The agent hallucinates, makes a terrible deduction, and poisons its own world model with garbage data.

Most database wrappers treat recovery as an afterthought (e.g., "restore from yesterday's backup"). The architecture we have mapped out makes recovery a **first-class, programmatic feature** because it explicitly designs around the assumption that all three forms of corruption *will* happen.

Here is exactly how this architecture turns catastrophic failures into routine, API-driven recovery operations.

---

### 1. Logical Corruption Recovery: The "Deterministic Rebuild"
In standard databases, if a vector index or FTS index gets corrupted (or if a migration fails), the entire database is considered degraded. 

Because we strictly separated **Canonical State** (`nodes`, `edges`, `chunks`) from **Derived Projections** (`vec_nodes`, `fts_nodes`), logical corruption is completely trivialized.

If `sqlite-vec` throws an internal error, or if you suspect the vector indexes are out of sync with the text, the recovery is a first-class SDK method:
```python
db.admin.rebuild_projections(target=["vector", "fts"])
```
**Under the hood, the engine:**
1. Drops the virtual `vec_nodes` and `fts_nodes` tables entirely.
2. Recreates the virtual tables.
3. Scans the canonical `chunks` and `nodes` tables where `superseded_at IS NULL`.
4. Deterministically re-inserts the data into the projections.

*Because the canonical SQLite JSONB tables hold the ground truth, the blast radius of projection corruption is zero.* You never lose agent memory; you only temporarily lose the fast-access pathway, which rebuilds itself.

### 2. Semantic Recovery: The "Time-Travel Reversal"
When an AI agent runs unattended, it will eventually make a mistake. It might incorrectly merge two "John Smith" nodes, overwrite a critical system prompt, or hallucinate 50 phantom tasks. 

In a CRUD database (`UPDATE` / `DELETE`), this is a fatal semantic corruption. You have to restore a snapshot and lose all good work done since the snapshot.

Because our architecture uses **Append-Only Logical Identity with `superseded_at`**, semantic recovery is just an `UPDATE` statement. 
```python
# The agent messed up in the last hour. Revert its changes.
db.admin.rollback_agent_actions(since=datetime.now() - timedelta(hours=1))
```
**Under the hood, the engine:**
1. Finds all rows in `nodes` and `edges` where `created_at >= T`.
2. Marks them as `superseded_at = NOW`.
3. Finds all previous versions of those nodes (where `superseded_at >= T`) and un-supersedes them (`UPDATE nodes SET superseded_at = NULL`).

The graph instantly snaps back to its exact state from an hour ago. Furthermore, because you didn't *delete* the agent's mistakes, you can still query the poisoned nodes for evaluation and debugging: "Why did the agent do this?"

### 3. Diagnostic Recovery: The "Blast Radius Containment"
When a node is corrupted, the first question is always: *Which LLM call or tool execution did this?*

Because we enforce a strict `source_ref` foreign key on every canonical graph append, pointing to the `steps` or `actions` runtime table, corruption is strictly traceable.

If a human user flags a specific generated Task as "hallucinated garbage", the engine can natively execute a **Lineage Teardown**:
```sql
-- Conceptual logic handled by the compiler
WITH corrupted_action AS (
    SELECT source_ref FROM nodes WHERE id = 'bad_task_123'
)
-- Automatically supersede ALL nodes/edges created by that specific hallucinated LLM turn
UPDATE nodes 
SET superseded_at = unixepoch() 
WHERE source_ref = (SELECT source_ref FROM corrupted_action);
```
This means you don't have to guess what else the agent broke during that specific thought process. You surgically excise the entire output of that specific inference run, leaving the rest of the graph perfectly intact.

### 4. Physical / Crash Recovery: The "Pre-Flight" Coordinated Write
If you open a SQLite transaction, make a 5-second network call to OpenAI for embeddings, and the user closes their laptop, the database connection drops. In many ORM-based architectures, this leaves dangling rows or corrupts the application state.

By enforcing the **Pre-Flight Write Pipeline**, physical and power-loss recovery is guaranteed by the math of SQLite's WAL (Write-Ahead Log).

1. **Async Pre-Flight:** Document chunked. Embeddings generated. (No DB locks held).
2. **`BEGIN IMMEDIATE`**
3. **Canonical Append**
4. **Projection Sync**
5. **`COMMIT`**

If the OS kills the process at Step 4, SQLite's WAL completely ignores the partial write on the next boot. The database is instantly reverted to Step 1. There is no partial state. There are no "nodes without chunks" or "chunks without vectors." 

### 5. Portability as a Recovery Mechanism
Because we explicitly rejected a custom storage engine, a persistent background worker queue, and multiple database files (e.g., storing vectors in ChromaDB and text in SQLite), **the entire world state is a single `.sqlite` file.**

If an agent gets hopelessly stuck in a corrupted state loop on a user's machine, the recovery mechanism is literally:
1. Copy `agent_memory.sqlite`.
2. Send it to the developer.
3. The developer runs the exact same `fathomdb` engine locally, inspects the `superseded_at` lineage, identifies the bug, writes a script to nullify the bad `source_ref`, and sends the single file back.

## Summary
By combining **Derived Projections** (rebuildable indexes), **Append-Only Cascades** (time-travel), **Strict Provenance** (surgical excision), and **Single-File WAL** (crash immunity), this architecture does not just "survive" corruption. It expects it, sandboxes it, and exposes deterministic SDK methods to repair it without ever relying on fragile external backup files.

---

To make database recovery a true first-class feature in `fathomdb`, we must bridge the gap between SQLite’s low-level C-engine mechanics and the semantic reality of an AI agent's world model. 

Here is the exact operational playbook and tooling design for handling physical, logical, and semantic corruption using native SQLite features and a dedicated Go-based management CLI (e.g., `fathom-cli`).

---

### 1. Leveraging Existing SQLite3 Recovery Tooling (The FathomDB Way)

SQLite has phenomenal built-in recovery tools (`PRAGMA integrity_check`, the `.recover` CLI command, and `.dump`), but **using them blindly on `fathomdb` will destroy the database.** 

Why? Because `FTS5` and `sqlite-vec` utilize hidden "shadow tables" (e.g., `fts_nodes_data`, `vec0_chunks_idx`). If SQLite tries to physically recover or `.dump` a partially corrupted shadow table, it often writes garbage data that permanently breaks the virtual table extension on the next boot.

**The Correct FathomDB Physical Recovery Protocol:**
If the disk corrupts or the SQLite header is mangled, you use SQLite's native `.recover` extension, but with a strict whitelist approach.

1. **Isolate the DB:** Ensure no agents are connected.
2. **Recover ONLY Canonical State:** Use the SQLite CLI to dump only the heavily normalized JSONB and relational tables, explicitly ignoring projections.
   ```bash
   sqlite3 corrupted.sqlite ".dump 'nodes' 'edges' 'chunks' 'runs' 'steps' 'actions'" > recovered_canonical.sql
   ```
3. **Rebuild into a fresh DB:**
   ```bash
   sqlite3 pristine.sqlite < recovered_canonical.sql
   ```
4. **Trigger FathomDB Logical Rebuild:** The new database has the entire human and agent history, but zero search indexes. You now use the Go tool to rebuild the projections from the canonical truth.

---

### 2. Go-Based Logical Corruption Recovery

Logical corruption means the database file is physically fine, but the Agent is failing to recall facts because the Vector/FTS projections drifted from the `nodes`/`chunks` tables (e.g., a crash happened during projection sync).

You build a single, statically compiled Go binary (`fathom-cli`) that acts as the sidecar/admin tool for the local agent.

**The Go Implementation Strategy for `fathom-cli repair`:**
Using `mattn/go-sqlite3` (which requires CGO to compile in the `FTS5` and `sqlite-vec` extensions), the Go tool executes a deterministic rebuild.

```go
// Conceptual Go Implementation for `fathom-cli repair --projections`
func RebuildProjections(db *sql.DB) error {
    // 1. Enter exclusive mode so the agent can't write while we repair
    db.Exec("PRAGMA locking_mode = EXCLUSIVE;")
    
    // 2. Drop the corrupted virtual tables completely (destroys bad shadow tables)
    db.Exec("DROP TABLE IF EXISTS vec_nodes;")
    db.Exec("DROP TABLE IF EXISTS fts_nodes;")
    
    // 3. Recreate them
    db.Exec("CREATE VIRTUAL TABLE fts_nodes USING fts5(chunk_id UNINDEXED, text_content);")
    // ... create vec_nodes ...

    // 4. Deterministic Stream & Insert
    // We only project the ACTIVE state (superseded_at IS NULL)
    rows, _ := db.Query(`
        SELECT c.id, c.text_content, c.embedding_blob 
        FROM chunks c
        JOIN nodes n ON c.node_id = n.id
        WHERE n.superseded_at IS NULL AND c.superseded_at IS NULL
    `)
    defer rows.Close()

    tx, _ := db.Begin()
    stmtFTS, _ := tx.Prepare("INSERT INTO fts_nodes (chunk_id, text_content) VALUES (?, ?)")
    stmtVec, _ := tx.Prepare("INSERT INTO vec_nodes (chunk_id, embedding) VALUES (?, ?)")
    
    for rows.Next() {
        var id, text string
        var emb []byte
        rows.Scan(&id, &text, &emb)
        stmtFTS.Exec(id, text)
        stmtVec.Exec(id, emb)
    }
    
    return tx.Commit()
}
```
Because the canonical state is append-only and immutable, this Go function is 100% idempotent. If it fails halfway, you just run it again. 

---

### 3. Automating the "Hopelessly Stuck Agent" Workflow

When an agent enters a hallucination loop (Semantic Corruption), copying a 5GB `.sqlite` file, emailing it to a developer, and sending a 5GB file back is a terrible user experience. 

Instead, we leverage the `fathom-cli` tool and `fathomdb`'s strict provenance (`source_ref`) to turn this into a lightweight, asynchronous debug-and-patch workflow.

#### Step A: The User Safely Exports the State (Local Go Tool)
The user notices the agent is acting crazy. They click a button in the UI or run a command. The Go tool ensures the WAL is flushed and creates a safe snapshot.
```bash
$ fathom-cli export --out agent_debug.sqlite
```
*Behind the scenes, the Go tool runs `PRAGMA wal_checkpoint(TRUNCATE)` and safely copies the file. The user uploads `agent_debug.sqlite` to the developer.*

#### Step B: The Developer Inspects the Lineage (Local Go Tool)
The developer receives the file. Because `fathomdb` enforces `source_ref` foreign keys, the developer doesn't have to guess what happened. They use the Go tool to print a causal tree of the agent's recent thoughts.

```bash
$ fathom-cli trace --db agent_debug.sqlite --last 2h

[RUN: run_01HQ...] 10:00 AM - "Summarize my emails"
  └─ [STEP: step_abc] 10:01 AM - Evaluated context.
     └─ [ACTION: act_xyz] 10:02 AM - Generated 500 duplicate 'Follow-up' tasks. 
         ⚠️ Caused by: LLM got stuck in a JSON generation loop.
```
The developer identifies that everything downstream of `act_xyz` is garbage.

#### Step C: The Developer Generates a "Surgical Patch" (Local Go Tool)
Instead of manually editing the database and sending 5GB back to the user, the developer uses the Go tool to generate a **Temporal Rollback Patch**.

```bash
$ fathom-cli excise --db agent_debug.sqlite --source-ref act_xyz --export patch.sql
```
Because of the architecture, the Go tool writes a tiny, deterministic SQL script (`patch.sql`) containing the exact logic needed to supersede the bad state:

```sql
-- patch.sql (Generated by fathom-cli)
BEGIN IMMEDIATE;

-- 1. Identify all corrupted physical nodes generated by this action
CREATE TEMP TABLE bad_nodes AS 
SELECT id, logical_id FROM nodes WHERE source_ref = 'act_xyz';

-- 2. Supersede them
UPDATE nodes SET superseded_at = unixepoch() 
WHERE id IN (SELECT id FROM bad_nodes);

-- 3. Supersede any edges connected to them
UPDATE edges SET superseded_at = unixepoch() 
WHERE source_id IN (SELECT logical_id FROM bad_nodes) 
   OR target_id IN (SELECT logical_id FROM bad_nodes);

-- 4. Re-activate the previous version of any node we just superseded
UPDATE nodes SET superseded_at = NULL 
WHERE logical_id IN (SELECT logical_id FROM bad_nodes) 
  AND superseded_at IS NOT NULL 
  AND id NOT IN (SELECT id FROM bad_nodes)
ORDER BY created_at DESC LIMIT 1;

COMMIT;
```

#### Step D: The User Applies the Patch (Local Go Tool)
The developer emails the 2KB `patch.sql` file back to the user. The user applies it:
```bash
$ fathom-cli apply --db local_agent.sqlite patch.sql
$ fathom-cli repair --projections
```

### Why this is an Elite Engineering Approach
1. **Zero Data Loss:** We didn't restore from yesterday's backup. We surgically excised the bad 5 minutes of agent thought, preserving the good work it did an hour before the loop.
2. **Bandwidth Efficient:** The developer returns a 2KB text file (the patch), not a 5GB database binary.
3. **Provable & Auditable:** The user (or their IT dept) can read `patch.sql` to see exactly what the developer is reverting before executing it.
4. **Resilient:** Because the projections are explicitly derived, patching the canonical state and running `--projections` guarantees the FTS and Vector indexes perfectly match the newly repaired reality.

