# Design: Bounded Shape SQL Cache

## Purpose

Address the verified finding that `shape_sql_map` grows without bound
(M-2).

---

## Current State

`crates/fathomdb-engine/src/coordinator.rs:104,193-196`

`HashMap<ShapeHash, String>` maps compiled query shape hashes to their SQL
strings. Every unique query shape adds an entry that persists for the
engine's lifetime. A `shape_sql_count()` method exists for monitoring
(line 464) but no eviction or size limit is enforced.

---

## Assessment

In practice, the set of query shapes is small and bounded by the
application's query vocabulary. A Memex-style agent application might
generate 50-200 distinct query shapes over its lifetime. Each entry is a
hash (8-16 bytes) plus a SQL string (typically 200-2000 bytes). Even 1000
shapes at 2 KB each is only 2 MB.

The adversarial case — dynamically generating many unique query shapes —
requires the caller to construct distinct ASTs, which is unusual in
normal agent operation. This is a defense-in-depth concern, not a
production-critical risk.

---

## Design

### Option A: Cap with eviction (recommended)

Add a maximum size constant and evict the oldest entries when exceeded:

```rust
const MAX_SHAPE_CACHE_SIZE: usize = 4096;
```

When inserting and the cache exceeds `MAX_SHAPE_CACHE_SIZE`, clear half
the entries. A simple clear-half strategy is acceptable because:
- Shape SQL is cheap to regenerate (one `compile_to_sql` call per miss).
- LRU tracking adds complexity and per-access overhead that is not
  justified for a cache that rarely reaches its limit.
- After a clear-half, hot shapes are re-added on the next query and cold
  shapes are naturally evicted.

```rust
fn get_or_compile_sql(&self, shape_hash: ShapeHash, compile: impl FnOnce() -> String) -> String {
    let mut map = self.shape_sql_map.lock().unwrap();
    if let Some(sql) = map.get(&shape_hash) {
        return sql.clone();
    }
    let sql = compile();
    if map.len() >= MAX_SHAPE_CACHE_SIZE {
        map.clear(); // or retain half
    }
    map.insert(shape_hash, sql.clone());
    sql
}
```

### Option B: Document the assumption

Add a comment at the cache declaration documenting that the shape set is
assumed small and bounded by the application's query vocabulary. Monitor
via `shape_sql_count()` in diagnostics and add a warning log if the count
exceeds a threshold.

### Recommendation

Option A is trivial to implement and eliminates the concern entirely.
The compile-on-miss cost is negligible compared to SQLite query execution.

---

## Test Plan

- Insert `MAX_SHAPE_CACHE_SIZE + 1` shapes. Verify the cache size stays
  bounded.
- Verify a cache miss regenerates the correct SQL.
