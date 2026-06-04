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
use fathomdb_schema::SQLITE_SUFFIX;
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
