//! Slice 30 (G3) — governed `read.collection` / `read.mutations` op-store read-back.
//!
//! Paginated read over `operational_mutations` for a `collection_name` on the
//! ReaderWorkerPool DEFERRED-tx path:
//!   * rows are returned ORDER BY id;
//!   * the limit is MANDATORY — no public path yields an unbounded SELECT, and
//!     the effective SQL LIMIT is clamped to the ~1M cap;
//!   * the after-id cursor paginates correctly across the page boundary;
//!   * `read.mutations` is an alias surface over the same read-back.
//!
//! Op-store rows are appended via `PreparedWrite::OpStore` against an
//! `append_only_log` collection registered with `PreparedWrite::AdminSchema`.
//!
//! Consumes `dev/plans/0.8.0-implementation.md` § "Slice 30" and the design memo
//! `dev/design/slice-30-design.md`. Binds gaps G3/F2/F4-READ + AC-074/REQ-053.

use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::{migrate, SQLITE_SUFFIX};
use rusqlite::Connection;
use tempfile::TempDir;

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

fn register_log(name: &str) -> PreparedWrite {
    PreparedWrite::AdminSchema {
        name: name.to_string(),
        kind: "append_only_log".to_string(),
        schema_json: "{\"type\":\"object\"}".to_string(),
        retention_json: "{}".to_string(),
    }
}

fn append(collection: &str, record_key: &str, body: &str) -> PreparedWrite {
    PreparedWrite::OpStore {
        collection: collection.to_string(),
        record_key: record_key.to_string(),
        schema_id: None,
        body: body.to_string(),
    }
}

fn seed(engine: &Engine, collection: &str, count: usize) {
    engine.write(&[register_log(collection)]).expect("register collection");
    for i in 0..count {
        engine
            .write(&[append(collection, &format!("k{i}"), &format!("{{\"n\":{i}}}"))])
            .expect("append op-store row");
    }
}

#[test]
fn read_collection_returns_rows_ordered_by_id() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "collection_order")).expect("open");
    seed(&opened.engine, "events", 5);

    let rows = opened.engine.read_collection("events", None, 100).expect("read_collection");
    assert_eq!(rows.len(), 5);
    // Strictly increasing id (ORDER BY id), record_keys in insertion order.
    for window in rows.windows(2) {
        assert!(window[1].id > window[0].id, "rows must be ORDER BY id ascending");
    }
    assert_eq!(rows[0].record_key, "k0");
    assert_eq!(rows[4].record_key, "k4");
    assert_eq!(rows[0].collection, "events");
    assert_eq!(rows[0].op_kind, "append");

    opened.engine.close().unwrap();
}

#[test]
fn read_collection_only_returns_the_named_collection() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "collection_scoped")).expect("open");
    seed(&opened.engine, "alpha", 3);
    seed(&opened.engine, "beta", 4);

    let alpha = opened.engine.read_collection("alpha", None, 100).expect("read alpha");
    assert_eq!(alpha.len(), 3);
    assert!(alpha.iter().all(|r| r.collection == "alpha"));

    opened.engine.close().unwrap();
}

#[test]
fn read_collection_honors_the_mandatory_limit() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "collection_limit")).expect("open");
    seed(&opened.engine, "events", 10);

    let page = opened.engine.read_collection("events", None, 4).expect("limited read");
    assert_eq!(page.len(), 4, "the limit caps the page size");

    opened.engine.close().unwrap();
}

#[test]
fn read_collection_after_id_cursor_paginates_across_the_boundary() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "collection_cursor")).expect("open");
    seed(&opened.engine, "events", 10);

    let page1 = opened.engine.read_collection("events", None, 4).expect("page1");
    assert_eq!(page1.len(), 4);
    let cursor = page1.last().unwrap().id;

    let page2 = opened.engine.read_collection("events", Some(cursor), 4).expect("page2");
    assert_eq!(page2.len(), 4);
    // No overlap across the boundary: every page2 id is strictly greater.
    assert!(page2.iter().all(|r| r.id > cursor), "after-id cursor excludes the boundary id");
    assert_eq!(page2[0].record_key, "k4", "page2 resumes exactly after page1's last row");

    // Final partial page.
    let cursor2 = page2.last().unwrap().id;
    let page3 = opened.engine.read_collection("events", Some(cursor2), 4).expect("page3");
    assert_eq!(page3.len(), 2, "10 rows over pages of 4 → 4 + 4 + 2");

    opened.engine.close().unwrap();
}

#[test]
fn read_mutations_aliases_the_same_read_back() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "mutations_alias")).expect("open");
    seed(&opened.engine, "events", 3);

    let via_collection = opened.engine.read_collection("events", None, 100).expect("collection");
    let via_mutations = opened.engine.read_mutations("events", None, 100).expect("mutations");
    assert_eq!(via_collection.len(), via_mutations.len());
    assert_eq!(
        via_collection.iter().map(|r| r.id).collect::<Vec<_>>(),
        via_mutations.iter().map(|r| r.id).collect::<Vec<_>>(),
        "read.mutations and read.collection return the same op-store rows"
    );

    opened.engine.close().unwrap();
}

#[test]
fn read_collection_clamps_limit_to_the_one_million_cap() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "collection_cap")).expect("open");
    seed(&opened.engine, "events", 3);

    // A huge limit must not error and must be clamped to the ~1M cap; with only
    // 3 rows present we simply get all 3 back (the clamp is internal, exercised
    // here for "no unbounded path / no panic on a large limit").
    let rows = opened
        .engine
        .read_collection("events", None, 5_000_000)
        .expect("a huge limit clamps, never an unbounded scan");
    assert_eq!(rows.len(), 3);

    opened.engine.close().unwrap();
}

// ---------------------------------------------------------------------------
// Slice 33 (G3 / F4-READ) — cursor + limit hardening under a genuine large
// multi-collection `operational_mutations` log. The step-13 additive index
// `operational_mutations(collection_name, id)` makes the read_collection SELECT
// index-driven (O(page), not O(rows-scanned)) and the clamp/cursor are robust at
// the edges. See `dev/design/slice-33-cursor-hardening-design.md`. No SDK
// signature change.
// ---------------------------------------------------------------------------

/// EXPLAIN gate (mirrors the `pr_g8` plan gate): the read_collection SELECT must
/// ride `operational_mutations_collection_id_idx` (the step-13 `(collection_name,
/// id)` index) — **no `SCAN`, no `USE TEMP B-TREE FOR ORDER BY`, no `USING
/// INTEGER PRIMARY KEY`**. This is the structural proxy for O(page) per-page work
/// when a small collection lives inside a large multi-collection log.
#[test]
fn read_collection_plan_is_index_driven_no_scan_no_temp_btree() {
    let dir = TempDir::new().unwrap();
    let conn = Connection::open(db_path(&dir, "plan")).expect("open sqlite");
    migrate(&conn).expect("migrate to head");

    // Seed a multi-collection log: a tiny `small` collection interleaved inside
    // a much larger `bulk` log, so the planner has a real reason to pick the
    // composite index over the id PK walk.
    for i in 0..2000i64 {
        let coll = if i % 100 == 0 { "small" } else { "bulk" };
        conn.execute(
            "INSERT INTO operational_mutations(collection_name, record_key, op_kind, payload_json, write_cursor)
             VALUES (?1, ?2, 'append', '{}', ?3)",
            rusqlite::params![coll, format!("k{i}"), i],
        )
        .unwrap();
    }

    let plan: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "EXPLAIN QUERY PLAN
                 SELECT id, collection_name, record_key, op_kind, payload_json, schema_id, write_cursor
                 FROM operational_mutations
                 WHERE collection_name = ?1 AND id > ?2
                 ORDER BY id
                 LIMIT ?3",
            )
            .expect("prepare EXPLAIN");
        stmt.query_map(rusqlite::params!["small", 0i64, 50i64], |row| row.get::<_, String>(3))
            .expect("query plan")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect plan")
    };
    let detail = plan.join(" | ");

    assert!(
        detail.contains("operational_mutations_collection_id_idx"),
        "read_collection must use the (collection_name, id) index; plan: {detail}"
    );
    assert!(
        !detail.contains("SCAN"),
        "read_collection must not full-scan operational_mutations; plan: {detail}"
    );
    assert!(
        !detail.contains("USE TEMP B-TREE FOR ORDER BY"),
        "the composite index must satisfy ORDER BY id without a temp B-tree; plan: {detail}"
    );
    assert!(
        !detail.contains("USING INTEGER PRIMARY KEY"),
        "read_collection must not ride the id PK walk (the pre-step-13 pathology); plan: {detail}"
    );
}

/// Bounded large-log pagination: a small collection paginated via `after_id`
/// inside a much larger interleaved log returns the correct ordered,
/// non-overlapping pages whose union is exactly the small collection. (The
/// EXPLAIN gate above is the per-page O(page) structural proof; this asserts
/// row-level correctness across the page boundary under the large-log shape.)
#[test]
fn read_collection_paginates_small_collection_inside_large_log() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "large_log")).expect("open");
    opened.engine.write(&[register_log("small"), register_log("bulk")]).expect("register");

    // 30 `small` rows interleaved with 9x as many `bulk` rows.
    let mut small_keys = Vec::new();
    for i in 0..300usize {
        if i % 10 == 0 {
            let key = format!("s{i}");
            opened
                .engine
                .write(&[append("small", &key, &format!("{{\"i\":{i}}}"))])
                .expect("small");
            small_keys.push(key);
        } else {
            opened.engine.write(&[append("bulk", &format!("b{i}"), "{}")]).expect("bulk");
        }
    }
    assert_eq!(small_keys.len(), 30);

    // Paginate `small` in pages of 7 via after_id.
    let mut collected: Vec<String> = Vec::new();
    let mut cursor: Option<i64> = None;
    let mut last_id = i64::MIN;
    loop {
        let page = opened.engine.read_collection("small", cursor, 7).expect("page");
        if page.is_empty() {
            break;
        }
        for row in &page {
            assert_eq!(row.collection, "small", "pagination must stay scoped to the collection");
            assert!(row.id > last_id, "ids strictly increasing across pages (no overlap)");
            last_id = row.id;
            collected.push(row.record_key.clone());
        }
        cursor = Some(page.last().unwrap().id);
    }

    assert_eq!(
        collected, small_keys,
        "the union of pages is exactly the small collection, in order"
    );

    opened.engine.close().unwrap();
}

/// Clamp + cursor edge cases: `limit == 0` empty (no SELECT); over-MAX limit
/// clamps (no error / no unbounded scan); `after_id` past the end is an empty
/// page; a negative `after_id` reads from the start of the log; an unknown
/// collection is empty; order is stable across pages.
#[test]
fn read_collection_clamp_and_cursor_edge_cases() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "edges")).expect("open");
    seed(&opened.engine, "events", 6);

    // limit == 0 -> empty page, no rows.
    let zero = opened.engine.read_collection("events", None, 0).expect("limit 0");
    assert!(zero.is_empty(), "limit == 0 returns an empty page");

    // limit > READ_COLLECTION_MAX_LIMIT -> clamped, never an unbounded scan / panic.
    let over =
        opened.engine.read_collection("events", None, usize::MAX).expect("over-MAX limit clamps");
    assert_eq!(over.len(), 6, "all rows returned; the clamp is internal");

    // after_id past the end -> empty page.
    let all = opened.engine.read_collection("events", None, 100).expect("all");
    let max_id = all.last().unwrap().id;
    let past = opened.engine.read_collection("events", Some(max_id + 1000), 100).expect("past end");
    assert!(past.is_empty(), "after_id past the last id is an empty page");

    // Negative after_id -> reads from the start of the log (normalized to 0).
    let neg = opened.engine.read_collection("events", Some(-42), 100).expect("negative cursor");
    assert_eq!(
        neg.iter().map(|r| r.id).collect::<Vec<_>>(),
        all.iter().map(|r| r.id).collect::<Vec<_>>(),
        "a negative after_id reads from the start of the log (same as None)"
    );

    // Unknown collection -> empty.
    let unknown = opened.engine.read_collection("does_not_exist", None, 100).expect("unknown");
    assert!(unknown.is_empty(), "an unknown / unregistered collection is empty");

    opened.engine.close().unwrap();
}
