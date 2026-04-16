# Supported Tokenizer Configurations

This document defines the five "Golden Path" tokenizer configurations supported by FathomDB. These presets ensure that operators can choose the right search behavior for their specific data types while maintaining architectural consistency.

## 1. Recall-Optimized English (Default)
**Config:** `porter unicode61 remove_diacritics 2`

*   **Logic:** Combines the `unicode61` tokenizer with the `porter` stemming algorithm.
*   **Behavior:** Maps related words to a common root (e.g., "running," "runs," and "ran" all match "run").
*   **Use Case:** General-purpose English text where user intent is broad and recall is prioritized.

## 2. Precision-Optimized Technical
**Config:** `unicode61 remove_diacritics 2`

*   **Logic:** Uses `unicode61` for clean Unicode tokenization but omits the stemmer.
*   **Behavior:** Requires exact word matches (diacritic-agnostic). Prevents "false positives" caused by aggressive stemming of technical jargon.
*   **Use Case:** Technical documentation, API references, and logs where specific terms must remain distinct (e.g., "computation" vs. "computer").

## 3. Global/Universal (CJK)
**Config:** `icu`

*   **Logic:** Leverages the International Components for Unicode (ICU) library.
*   **Behavior:** Correct word-breaking for non-space-separated languages like Chinese, Japanese, and Korean.
*   **Use Case:** Multi-lingual datasets and internationalized technical content.
*   **Note:** Requires the SQLite `icu` extension to be enabled in the environment.

## 4. Identifier & Fragment (Trigram)
**Config:** `trigram`

*   **Logic:** Breaks text into 3-character overlapping sequences.
*   **Behavior:** Enables "substring" matching. A search for "admin" will match "fathomdb-admin-bridge."
*   **Use Case:** Searching source code, file paths, URLs, or hex codes where partial matches are common.
*   **Note:** Requires SQLite 3.34.0 or higher.

## 5. Source Code & Identifier-Aware
**Config:** `unicode61 tokenchars '._-$@'`

*   **Logic:** A precision-focused `unicode61` tokenizer that treats specific symbols as part of the word.
*   **Behavior:** Ensures that symbols common in programming—like `_` (snake_case), `.` (namespaces), `-` (kebab-case/CSS), `$` (shell variables), and `@` (decorators/handles)—are indexed as part of the token.
*   **Use Case:** Source code search, log files, and configuration files where `fathomdb_core` and `fathomdb.core` are distinct and must be searchable as full identifiers.
