# Tokenization and Embedding Choices

This document captures the technical assessment, trade-offs, and supported configurations for Full-Text Search (FTS5) and Vector Projections (`sqlite-vec`) within FathomDB.

## Supported Tokenizer Configurations (FTS5)

FathomDB provides five "Golden Path" tokenizer presets to balance recall, precision, and internationalization.

| Preset | Configuration | Primary Use Case |
| :--- | :--- | :--- |
| **Recall-Optimized** | `porter unicode61 remove_diacritics 2` | General English documentation and broad search. |
| **Precision-Optimized** | `unicode61 remove_diacritics 2` | Technical text where stemming creates false positives. |
| **Global (CJK)** | `icu` | Chinese, Japanese, and Korean multi-lingual support. |
| **Substring (Trigram)** | `trigram` | Partial word matching for paths, URLs, and hex codes. |
| **Source Code** | `unicode61 tokenchars '._-$@'` | Exact matching for variable names and identifiers. |

---

## Supported Embedding Configurations (Vector)

FathomDB supports a tiered set of embedding engines to address the varying context and resolution needs of Memex applications.

### 1. Baseline Models
*   **Standard In-Process (`BGE-Small-v1.5`)**: 384-dim, 512-token context. The low-overhead default for local-first agents.
*   **Standard Cloud (`OpenAI-v3-Small`)**: 1536-dim. Industry standard for general-purpose semantic search.

### 2. Next-Gen "Memex-Forward" Models
*   **High-Efficiency Matryoshka (`Nomic-v1.5`)**: 768-dim with **8k-token context**. Supports Matryoshka dimensionality for flexible storage/speed trade-offs.
*   **Long-Context Universal (`Jina-v2`)**: 768-dim with **8k-token context**. Optimized for long documents and cross-lingual retrieval.
*   **SOTA Retrieval (`Stella-v5`)**: 1024-dim with Matryoshka support. Top-tier precision for specialized technical domains.

---

## Concerns, Downsides, and Mitigations

| Issue | Description | Mitigation |
| :--- | :--- | :--- |
| **Porter Over-Stemming** | Conflates distinct technical terms (e.g., "computer" vs "computation"). | Use the **Precision-Optimized** or **Source Code** presets. |
| **CJK Blind Spot** | `unicode61` fails on non-space-separated languages. | Use the **Global (CJK)** `icu` tokenizer. |
| **512-Token Ceiling** | Baseline models truncate long Memex nodes. | Upgrade to **Next-Gen** (Nomic/Jina) for 8k context. |
| **Model Lock-in** | Changing models requires a full index rewrite. | Use the **User-Picked Projections** admin workflow for safe migration. |

---

## Reviewed but Not Picked

The following options were analyzed during research but excluded from the "Supported" list for technical or strategic reasons.

### Tokenizers
*   **Simple Tokenizer**: Excluded because it only splits on whitespace and does not handle Unicode case-folding or punctuation correctly. It is too primitive for modern technical or international search requirements.
*   **Standard Snowball Stemmers**: While robust, they were not picked to avoid configuration "bloat." The Porter stemmer provides the desired recall for English, and users needing non-English support are better served by the `icu` tokenizer.

### Embedding Engines
*   **Word2Vec / GloVe**: Excluded because they are static (non-contextual) embeddings. They lack the semantic depth required for "Memex" tasks where the meaning of a word depends heavily on its surrounding context.
*   **BERT-Large (Vanilla)**: Excluded due to its excessive latency and memory footprint in a local-first environment. BGE-Small and Nomic provide comparable or superior retrieval quality at a fraction of the compute cost.
*   **FastEmbed (Python)**: While excellent for Python-only environments, it was not picked as a core engine to maintain the "zero-dependency Rust" goal for the base FathomDB distribution. Users needing FastEmbed can still utilize it via the **Legacy/Custom Subprocess** path.
