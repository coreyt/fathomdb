# FathomDB Performance Critique — 0.6.0-rewrite (WIP)

Short version: FathomDB is not obviously "bad for SQLite," but it is also not yet in the top tier of SQLite-backed search/vector performance. Its current Phase 9 numbers look like a reasonable outcome for a correctness-heavy SQLite engine with FTS5 + vector + scheduler + recovery semantics, but they are still behind the faster specialized SQLite/vector stacks.

## What the web research says

- SQLite WAL is the standard way to get single-writer / many-reader behavior, but SQLite still allows only one writer, and read performance can degrade as the WAL grows or checkpoints stall. Source: <https://www.sqlite.org/wal.html>
- SQLite serialized mode is the default; multithread / NOMUTEX only removes connection-level serialization if each connection is single-thread-owned. It does not remove all contention sources. Source: <https://www.sqlite.org/threadsafe.html>
- Shared-cache is generally discouraged. SQLite's own docs say WAL is the better answer for most concurrent access cases. Source: <https://www.sqlite.org/sharedcache.html>
- FTS5 performance is sensitive to merge policy and index shape. automerge, crisismerge, and optimize directly trade write cost against query speed. Source: <https://www.sqlite.org/fts5.html>
- sqlite-vec is explicitly positioned as a "fast enough" brute-force vector extension, optimized for portability and local workloads, not billion-scale ANN-first search. Its own docs/blog emphasize brute-force exact search, update friendliness, and portability. Sources: <https://alexgarcia.xyz/blog/2024/sqlite-vec-stable-release/>, <https://alexgarcia.xyz/sqlite-vec/features/vec0.html>, <https://raw.githubusercontent.com/asg017/sqlite-vec/main/README.md>
- sqlite-vss took the opposite approach: Faiss-backed vector search, including IVF options for much faster large-dataset queries, but with much heavier build/training/update costs and weaker write ergonomics. Source: <https://github.com/asg017/sqlite-vss>
- libSQL/Turso has gone further and added native vector indexing with DiskANN-style ANN search, which is a different performance class from brute-force sqlite-vec-style search. Source: <https://docs.turso.tech/features/ai-and-embeddings>

## How FathomDB compares

Using your whitepaper:

- Pack 5 closed with AC-020 still far red: median about 184.7 ms sequential / 124.0 ms concurrent / 1.487x speedup.
- After F.0 thread-affine reader workers, concurrency improved a lot: median speedup rose to about 3.49x, with concurrent median around 155 ms.
- G.1 lookaside improved slightly again to about 3.53x speedup, but AC-020 still did not close.
- The whitepaper's conclusion is that the major self-inflicted Rust-side bottleneck was mostly removed, and the remaining ceiling is now closer to SQLite/WAL/rusqlite/internal contention, not just obvious pool design mistakes.

So performance-wise:

surprising. sqlite-vec's published positioning is "exact brute-force, local-first, minimal overhead," while FathomDB layers:

- canonical-row joins,
- snapshot/cursor guarantees,
- projection scheduler semantics,
- durable failure recording,
- recovery/operator surface,
- mixed text + vector retrieval behavior.

In other words, FathomDB is paying for more system behavior than a bare vec0 demo.

1. Versus sqlite-vss / libSQL vector index. FathomDB is behind these on likely large-scale vector-search performance ceilings. sqlite-vss can use Faiss IVF; libSQL/Turso exposes ANN indexing. Those systems are built to beat brute-force search once the dataset is large enough. FathomDB today is closer in spirit to sqlite-vec than to ANN-native systems.

2. Versus other SQLite-backed "databases." FathomDB now looks architecturally respectable after F.0. It no longer looks like a naive misuse of SQLite. But it also does not yet look like a highly tuned SQLite search engine. The remaining performance is better described as:

- acceptable for a correctness-first SQLite engine,
- improved meaningfully by topology fixes,
- still below the stronger SQLite/vector specialized stacks.

## Bottom line

If I place FathomDB on the SQLite-backed spectrum:

- Below: bare SQLite+FTS5 for minimal-overhead text lookup, and below specialized vector engines built on SQLite/libSQL for raw vector throughput.
- Closest to: sqlite-vec-style exact-search systems, but with much more runtime and recovery machinery layered on top.
- Better than its earlier state: yes, clearly. F.0 materially improved it.
- Competitive as a tuned SQLite/vector engine yet: not really. The whitepaper says it has moved from "avoidable architecture bottlenecks" to "SQLite/WAL-level ceiling," which is progress, but AC-020 staying red means it is still not where the strongest SQLite-backed search stacks are.

## Sources

- SQLite WAL: <https://www.sqlite.org/wal.html>
- SQLite threading: <https://www.sqlite.org/threadsafe.html>
- SQLite shared-cache: <https://www.sqlite.org/sharedcache.html>
- SQLite FTS5: <https://www.sqlite.org/fts5.html>
- sqlite-vec docs/blog: <https://alexgarcia.xyz/blog/2024/sqlite-vec-stable-release/>, <https://alexgarcia.xyz/sqlite-vec/features/vec0.html>, <https://raw.githubusercontent.com/asg017/sqlite-vec/main/README.md>
- sqlite-vss: <https://github.com/asg017/sqlite-vss>
- libSQL/Turso vectors: <https://docs.turso.tech/features/ai-and-embeddings>

If useful, I can turn this into a decision memo for whether AC-020 should be deferred, or escalated into ANN / vendor-SQLite work.
