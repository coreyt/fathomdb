# FathomDB needs vs Turso readiness — gap analysis

**Snapshot date:** 2026-06-21
**Turso version evaluated:** v0.6.1 (May 2026), BETA
**Verdict:** 🔴 **Not a drop-in. Re-platforming, not a swap.** Revisit when Turso ships ANN vector indexing + exits beta + supports `WITH RECURSIVE`.

> This is a point-in-time snapshot. Turso is fast-moving (203 releases as of this writing).
> Re-run `prompt-update-turso-watch.md` to refresh and append a new dated snapshot.

---

## Disambiguation (read this first)

There are **two** distinct "Turso" products. Do not conflate them.

- **`tursodatabase/turso`** (formerly **Limbo**) — a **from-scratch Rust rewrite** of SQLite. **This is the subject of this analysis.** Status: BETA, v0.6.1. Compatibility tracked in its `COMPAT.md`.
- **libSQL / Turso Cloud** — the older C **fork** of SQLite (this is what `docs.turso.tech/features/sqlite-extensions` describes: SQLean suite, libSQL vector datatype, sqlite-vec 0.1.3 bundled on Fly only). Different codebase, different capabilities.

When refreshing this watch, always confirm which product a source describes.

---

## Part 1 — FathomDB's SQLite hard-dependencies

FathomDB is welded to specific SQLite features, not loosely backed by it. Engine (`crates/fathomdb-engine`) is built on **`rusqlite 0.31` (`bundled`)** + **`sqlite-vec =0.1.7`** (exact pin).

| # | Pillar | How FathomDB uses it | Where |
|---|---|---|---|
| 1 | **sqlite-vec (`vec0` vtab)** | Entire ANN vector path. `vector_default` with `embedding float[d]` + `embedding_bin bit[d]`; query `embedding_bin MATCH vec_quantize_binary(vec_f32(?))`, f32 rerank `vec_distance_l2`. Registered via `sqlite3_auto_extension`. No fallback. | `lib.rs:7689`, `:5088` |
| 2 | **FTS5 + `bm25()`** | `search_index` / `search_index_edges` vtabs; tokenizer `'porter unicode61 remove_diacritics 2'`; ranked by `bm25()`. | `schema/lib.rs:269`; `lib.rs:5388` |
| 3 | **Recursive CTEs** | Graph BFS is pure `WITH RECURSIVE` — cycle detect via `instr()`, temporal filter via `datetime()`. | `lib.rs:5995–6116` |
| 4 | **WAL + `locking_mode=EXCLUSIVE`** | Single-writer thread (`BEGIN IMMEDIATE`), 8-reader pool (`query_only=ON`, `BEGIN DEFERRED`), sidecar flock `{db}.sqlite.lock`. | `lib.rs:2262`, `:380`, `:7541` |
| 5 | **Partial UNIQUE indexes** | Canonical identity dedup: `CREATE UNIQUE INDEX … WHERE superseded_at IS NULL` (**G0 logical_id keystone**). | `schema/lib.rs:304` |
| 6 | **PRAGMAs / introspection** | `user_version` (schema versioning + migrations), `wal_checkpoint(TRUNCATE)`, `integrity_check`, `application_id`, custom `pcache2` via `SQLITE_CONFIG_PCACHE2` (experimental). | `lib.rs:4239`, `:7098`, `pcache2.rs` |

**Not used → not a migration risk:** window functions, generated columns, INSTEAD OF triggers, rollback-journal mode.

---

## Part 2 — Turso (Rust rewrite) capabilities

Status: **BETA, v0.6.1**. README warns against production data without backups.

Beyond-SQLite additions: `BEGIN CONCURRENT` / **MVCC** concurrent writes, CDC, native vector functions, **Tantivy-based** FTS, experimental encryption-at-rest, DBSP incremental compute, **`io_uring` async I/O** (Linux), WASM/browser, built-in MCP server.

---

## Part 3 — Gap table

| FathomDB hard-dep | Turso status (per COMPAT.md) | Verdict |
|---|---|---|
| **sqlite-vec `vec0` + binary quant** | No loadable C extensions (`sqlite3_load_extension` ❌). Turso has *own* vector funcs (`vector_distance_cos/l2`) but **no `vec0` vtab, no `vec_quantize_binary`, no `bit[]` MATCH**. **ANN indexing still roadmap — only exact search today.** | 🔴 **Blocker** |
| **FTS5 + `bm25()`** | Unsupported ("Use Turso FTS instead"). Native FTS = **Tantivy**, scored via `fts_score()`, different tokenizer model. | 🔴 **Blocker** |
| **`WITH RECURSIVE`** | Not yet supported (`WITH` is 🚧 partial because recursive missing). | 🔴 **Blocker** |
| **rusqlite 0.31 binding** | Turso ships its own crate; C API only partial (no UDFs, collations, backup, BLOB I/O, loadable ext). No rusqlite-compat. | 🔴 **Blocker** |
| **Partial UNIQUE index (`WHERE`)** | Not documented in COMPAT either way. | 🟡 **Unverified risk** (G0 keystone — must prove first) |
| **WAL + `EXCLUSIVE`** | ✅ WAL; `locking_mode` 🚧 EXCLUSIVE-only (matches FathomDB); `query_only` ✅; `wal_checkpoint` 🚧 (no param); `integrity_check` ✅; `user_version` ✅. | 🟢 Largely fine (checkpoint param form may need tweak) |
| Transactions / `BEGIN IMMEDIATE` / savepoints | ✅ supported. | 🟢 Fine |
| Custom pcache2 (`SQLITE_CONFIG_PCACHE2`) | C config API not in Turso surface. | 🟡 Lose experiment (non-prod) |

---

## Verdict & recommendation

Three of six pillars (sqlite-vec vec0/binary-quant ANN, FTS5+`bm25()`, `WITH RECURSIVE`) are each independently hard blockers; the rusqlite binding is a fourth; and the one most critical for correctness (partial unique index = G0 keystone) is unverified. The parts Turso covers well (WAL+EXCLUSIVE, transactions, savepoints, PRAGMAs) were never the problem.

The tempting features — native vector + native FTS as built-ins (killing extension bundling/version-pinning friction), `io_uring`, MVCC (relax single-writer), encryption-at-rest — are real but (a) mostly experimental/beta, (b) semantically different enough that they **relocate** work rather than reduce it, and (c) the ANN vector *indexing* FathomDB needs is still roadmap.

Note also: FathomDB's 0.8.x retrieval baselines (BM25 ground truth, recall floors) are defined against **SQLite FTS5 `bm25()`**. Switching to Tantivy `fts_score()` shifts the eval ground truth — any migration invalidates those comparisons and forces re-baselining.

**Recommendation: Not now, not as a swap.** Cheapest real signal = a spike on the single most-coupled, least-replaceable pillar — the **sqlite-vec ANN path** — testing whether Turso native vector reproduces binary-quantized recall at FathomDB's floor. If that fails, the rest is moot.

**Revisit triggers (any source clearing all three → re-evaluate seriously):**
1. Turso ships **ANN vector indexing** (not just exact search).
2. Turso exits **beta** → GA / production reliability claim.
3. Turso supports **`WITH RECURSIVE`** — OR FathomDB has already moved graph traversal out of SQL.

---

## Sources
- https://github.com/tursodatabase/turso
- https://raw.githubusercontent.com/tursodatabase/turso/main/COMPAT.md
- https://turso.tech/blog/beyond-fts5
- https://turso.tech/blog/turso-0.5.0
- https://docs.turso.tech/features/sqlite-extensions (libSQL — older fork, for contrast)
