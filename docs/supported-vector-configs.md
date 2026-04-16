# Supported Vector Embedding Configurations

FathomDB supports a tiered set of embedding configurations. Operators can choose between "Baseline" models for maximum compatibility and "Next-Gen" models for advanced Memex features like long-context awareness and flexible storage.

## 1. Baseline Performance (The "Standard" Path)

### Standard In-Process
**Model:** `BAAI/bge-small-en-v1.5`
*   **Dimensions:** 384
*   **Context:** 512 tokens
*   **Implementation:** Rust `candle` (In-process)
*   **Use Case:** The stable default for local-first agents. Fast and low-overhead.

### Standard Cloud
**Model:** `text-embedding-3-small`
*   **Dimensions:** 1536
*   **Implementation:** `OpenAI` (SDK-side adapter)
*   **Use Case:** Industry standard for general-purpose semantic search.

---

## 2. Next-Gen Configurations (The "Memex-Forward" Path)

These models address the limitations of earlier research by offering longer context windows and flexible dimensionality.

### High-Efficiency Matryoshka
**Model:** `nomic-embed-text-v1.5`
*   **Dimensions:** 768 (Flexible 128-768 via Matryoshka)
*   **Context:** **8,192 tokens**
*   **Implementation:** Rust `candle` or SDK-side adapter
*   **Value:** **Matryoshka support** allows the embedder to truncate vectors to 128 dimensions for extreme speed/storage savings without a massive drop in accuracy.
*   **Memex Use:** Ideal for long-form meeting notes or technical papers that exceed the 512-token limit of baseline models.

### Long-Context & Universal
**Model:** `jina-embeddings-v2-base-en`
*   **Dimensions:** 768
*   **Context:** **8,192 tokens**
*   **Implementation:** SDK-side `QueryEmbedder` implementation
*   **Value:** State-of-the-art long-context support. Jina-v2 uses an ALiBi-based architecture that maintains high retrieval quality even at the end of long documents.
*   **Memex Use:** Best for large "Personal Knowledge Management" (PKM) libraries and cross-lingual research tasks.

### State-of-the-Art (SOTA) Retrieval
**Model:** `stella_en_400M_v5`
*   **Dimensions:** 1024 (Flexible via Matryoshka)
*   **Implementation:** SDK-side `QueryEmbedder` implementation
*   **Value:** Currently one of the highest-performing models on the MTEB leaderboard. Offers significantly better semantic "resolution" than BGE-Small.
*   **Memex Use:** When retrieval precision is the absolute priority, particularly in specialized technical domains.

---

## 3. Custom / Legacy Pattern (SDK-side)
**Model:** User-defined (e.g., `sentence-transformers/all-MiniLM-L6-v2`)
*   **Dimensions:** Variable
*   **Implementation:** SDK-side adapter (Subprocess)
*   **Use Case:** Migration from early FathomDB versions or specialized local model research where the implementation is owned by application code.
*   **Note:** This pattern requires implementing the `QueryEmbedder` trait in user or SDK code; the FathomDB engine does not include a built-in subprocess runner as of 0.4.0.

---

## 4. Managing the Active Vec Profile (0.4.5+)

The `AdminClient` exposes profile CRUD so operators can record which model is active and preview the cost of switching:

```python
from fathomdb import FathomDB
from fathomdb.embedders import OpenAIEmbedder

db = FathomDB.open("store.db")
embedder = OpenAIEmbedder("text-embedding-3-small", api_key="sk-…", dimensions=1536)

# Preview impact before switching
impact = db.admin.preview_projection_impact("*", "vec")
print(f"Switching will rebuild {impact.rows_to_rebuild} chunks")

# Record the active model (triggers impact gate)
profile = db.admin.configure_vec(embedder, agree_to_rebuild_impact=True)
# Then rebuild explicitly:
db.admin.regenerate_vector_embeddings(embedder)

# Read back the stored profile
profile = db.admin.get_vec_profile()   # returns VecProfile | None
```

`VecProfile` fields: `model_identity`, `model_version`, `dimensions`, `active_at`, `created_at`.

**Vec identity guard**: at engine open time, `check_vec_identity_at_open` emits a `warn!` log if the embedder's `model_identity` or `dimensions` differ from the stored `VecProfile`. Startup is never blocked.

---

## Architectural Note: Matryoshka & Long-Context
FathomDB's "User-Picked" design maintains the **Vector Identity Invariant**:

1.  **Long-Context:** The engine's chunking logic respects the maximum context window reported by the `QueryEmbedder` implementation, ensuring that long-form documents are not unnecessarily fragmented.
2.  **Matryoshka:** Truncation and dimensionality reduction are the responsibility of the `QueryEmbedder` implementation. The engine persists the resulting vector as a system-of-record.
3.  **Normalization:** The embedder implementation must ensure that vectors are L2-normalized (after truncation, if applicable) to ensure distance metrics remain stable.
