//! TC-76 / sqlite-vec issue **#99** — vec0 `DELETE` of a row carrying a TEXT
//! metadata value longer than 12 bytes.
//!
//! Commissioned as a CHARACTERIZATION PROBE ("does #99 bite us?"). It did: the
//! probe went RED against the shipped erasure verbs, so this file is now BOTH a
//! characterization pin of the upstream mechanism AND the regression test for the
//! engine-side remediation.
//!
//! # The upstream mechanism (sqlite-vec `=0.1.7`, `sqlite-vec.c`)
//!
//! A vec0 plain-TEXT metadata column stores a 16-byte inline view per row
//! (`VEC0_METADATA_TEXT_VIEW_BUFFER_LENGTH`): a 4-byte length plus the first 12
//! bytes of the value (`VEC0_METADATA_TEXT_VIEW_DATA_LENGTH`). A value LONGER
//! than those 12 bytes additionally gets a row in the `<tbl>_metadatatext<NN>`
//! shadow table.
//!
//! `vec0Update_Delete_ClearMetadata` (`sqlite-vec.c:8888`) deletes that shadow row
//! (`sqlite-vec.c:8934-8952`) and leaves `rc` holding `sqlite3_step`'s
//! `SQLITE_DONE` (101) — it never resets `rc` to `SQLITE_OK`. Its epilogue then
//! returns `rc` verbatim, so a SUCCESSFUL shadow delete is reported to
//! `vec0Update_Delete` (`sqlite-vec.c:9027-9030`) as `101 != SQLITE_OK` and the
//! whole `DELETE` is aborted. The INSERT/UPDATE twin of that code
//! (`sqlite-vec.c:8258-8320`) is saved by an unconditional
//! `rc = sqlite3_blob_close(...)` after the switch — which is why only DELETE
//! carries the defect.
//!
//! # Why FathomDB is exposed
//!
//! `vector_default` (`vector_partition_create_sql`) carries three plain-TEXT
//! metadata columns: `kind`, `status`, and one `attr_<hex>` per declared
//! `filterable` projection (0.8.20 Slice 15e). Of those only `attr_*` VALUES are
//! caller-supplied and unbounded. They are stored marker-encoded as `\x01 || V`,
//! so a raw attribute value of **12 or more UTF-8 bytes** crosses the 12-byte
//! threshold — and every by-rowid `DELETE FROM vector_default` (erasure, purge,
//! edge supersession, open-path orphan sweep, mean-vec re-quantize) fails.
//!
//! `kind` and `status` are NOT exposed: `kind` is gated at every vec0 enrolment
//! door by `kind_is_vector_committable` -> `resolve_source_type`, whose whole
//! output domain is `{email, article, paper, meeting, note, todo, edge_fact}`
//! (max 9 bytes), and `status` ships the `''` sentinel. `source_type` is a vec0
//! PARTITION KEY, a different storage mechanism entirely, and shares that <= 9
//! byte domain.
//!
//! Note the attribute-column NAME axis is a red herring: `attr_vec0_column`
//! renders `attr_ + hex(name)`, so ANY name of >= 4 characters already produces an
//! identifier longer than 12 characters and the shipped Slice-15e corpus exercises
//! that shape today. #99 is about the stored VALUE's byte length, not the column
//! identifier's.
//!
//! # Tripwire
//!
//! [`bare_vec0_delete_pins_the_upstream_length_boundary`] asserts the upstream
//! defect is STILL PRESENT. When `sqlite-vec` is bumped past a release that fixes
//! #99 that test goes red on purpose — which is the signal to delete the
//! engine-side neutralize step in `delete_vector_partition_row`.

use std::collections::BTreeSet;
use std::sync::Arc;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{
    Engine, InitialState, LifecycleState, PreparedWrite, ProjectionRole, ProjectionSpec, SourceId,
};
use fathomdb_schema::SQLITE_SUFFIX;
use rusqlite::Connection;
use tempfile::TempDir;

const DIM: usize = 8;

/// The upstream inline-view payload width. A stored value of MORE than this many
/// bytes spills into the `_metadatatext<NN>` shadow table — the branch #99 breaks.
const VEC0_TEXT_INLINE_BYTES: usize = 12;

#[derive(Clone, Debug)]
struct HashEmbedder;

impl Embedder for HashEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new("hash8", "rev-a", DIM as u32)
    }
    fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
        let mut v = vec![0.0_f32; DIM];
        for (i, b) in text.bytes().enumerate() {
            v[i % DIM] += f32::from(b) / 255.0;
        }
        if v.iter().all(|x| *x == 0.0) {
            v[0] = 1.0;
        }
        Ok(v)
    }
}

fn fixture(name: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("{name}{SQLITE_SUFFIX}"));
    (dir, path)
}

fn open(path: &std::path::Path) -> fathomdb_engine::OpenedEngine {
    let opened = Engine::open_with_embedder_for_test(path, Arc::new(HashEmbedder)).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind doc");
    opened
}

fn filterable_spec(name: &str) -> ProjectionSpec {
    let mut roles = BTreeSet::new();
    roles.insert(ProjectionRole::Filterable);
    ProjectionSpec { name: name.to_string(), roles, fts: None, vector: None, source: None }
}

fn node(kind: &str, logical: &str, body_json: &str, source: &str) -> PreparedWrite {
    PreparedWrite::Node {
        kind: kind.to_string(),
        body: body_json.to_string(),
        source_id: SourceId::new(source).expect("source id"),
        logical_id: Some(logical.to_string()),
        state: InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

/// Independent oracle for the documented `attr_<hex>` column encoding.
fn attr_col(name: &str) -> String {
    let mut s = String::from("attr_");
    for b in name.as_bytes() {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn vector_row_count(engine: &Engine) -> i64 {
    engine
        .query_i64_col_for_test("SELECT COUNT(*) FROM vector_default")
        .expect("count vector_default")
        .first()
        .copied()
        .expect("one count row")
}

// ===========================================================================
// (1) The upstream boundary, on a BARE vec0 table — no engine involved.
//     TRIPWIRE: goes red when sqlite-vec fixes #99.
// ===========================================================================

#[test]
fn bare_vec0_delete_pins_the_upstream_length_boundary() {
    let (dir, path) = fixture("tc76_bare_warm");
    // Opening an Engine installs sqlite-vec via the process-global
    // `sqlite3_auto_extension` (`register_sqlite_vec_extension`), so the bare
    // connection below gets `vec0` without this test crate taking a direct
    // `sqlite-vec` dependency.
    open(&path).engine.close().unwrap();

    let conn = Connection::open(dir.path().join("bare.sqlite")).expect("bare open");
    conn.execute_batch("CREATE VIRTUAL TABLE t USING vec0(embedding float[4], m TEXT)")
        .expect("create bare vec0");

    for (rowid, len) in [(1_i64, 1_usize), (2, 11), (3, 12), (4, 13), (5, 64), (6, 1024)] {
        let value = "x".repeat(len);
        conn.execute(
            "INSERT INTO t(rowid, embedding, m) VALUES (?1, vec_f32('[1,2,3,4]'), ?2)",
            rusqlite::params![rowid, value],
        )
        .unwrap_or_else(|e| panic!("INSERT of a {len}-byte metadata value must succeed: {e}"));

        let deleted = conn.execute("DELETE FROM t WHERE rowid = ?1", [rowid]);
        let residue: i64 = conn
            .query_row("SELECT COUNT(*) FROM t WHERE rowid = ?1", [rowid], |r| r.get(0))
            .expect("residue count");

        if len <= VEC0_TEXT_INLINE_BYTES {
            assert!(
                deleted.is_ok(),
                "a {len}-byte value fits the {VEC0_TEXT_INLINE_BYTES}-byte inline view, \
                 so DELETE must succeed: {deleted:?}"
            );
            assert_eq!(residue, 0, "a {len}-byte-valued row must be gone after DELETE");
        } else {
            // UPSTREAM DEFECT PIN, not desired behaviour. sqlite-vec 0.1.7
            // returns SQLITE_DONE (101) out of `vec0Update_Delete_ClearMetadata`
            // and aborts the DELETE, leaving the row (and its shadow value) at
            // rest. See the module doc; delete the engine-side workaround when
            // this assertion starts failing.
            let err = deleted.expect_err(
                "sqlite-vec 0.1.7 #99: DELETE of a >12-byte TEXT metadata value must still fail \
                 — if this now SUCCEEDS the dependency has been fixed, so remove the \
                 `delete_vector_partition_row` neutralize step",
            );
            assert!(
                format!("{err:?}").contains("101"),
                "the spurious failure is SQLITE_DONE (101) leaking as an error: {err:?}"
            );
            assert_eq!(residue, 1, "the spurious failure leaves the row behind: len={len}");
            // Clean up through the same workaround the engine uses, so the loop
            // can continue on a table with no residue for this rowid.
            conn.execute("UPDATE t SET m = '' WHERE rowid = ?1", [rowid]).expect("neutralize");
            conn.execute("DELETE FROM t WHERE rowid = ?1", [rowid])
                .expect("neutralized DELETE must succeed");
        }
    }
}

// ===========================================================================
// (2) `erase_source` over rows whose filterable attribute values are long.
// ===========================================================================

#[test]
fn erase_source_erases_rows_with_long_filterable_attribute_values() {
    let (_dir, path) = fixture("tc76_erase_source");
    let opened = open(&path);
    let engine = &opened.engine;
    engine.configure_projections(&[filterable_spec("priority")], &[]).expect("configure");

    // 13, 64 and 1024 raw bytes — each stores as raw+1 (the `\x01` present
    // marker), so all three exceed the 12-byte inline view.
    let source = "test:tc76-erase";
    let writes: Vec<PreparedWrite> = [13_usize, 64, 1024]
        .iter()
        .enumerate()
        .map(|(i, len)| {
            let value = "y".repeat(*len);
            node(
                "doc",
                &format!("L{i}"),
                &format!(r#"{{"title":"doc number {i} about vectors","priority":"{value}"}}"#),
                source,
            )
        })
        .collect();
    engine.write(&writes).expect("write");
    engine.drain(10_000).expect("drain");
    assert_eq!(vector_row_count(engine), 3, "three vec0 rows are at rest before the erasure");

    let col = attr_col("priority");
    let stored = engine
        .query_text_col_for_test(&format!("SELECT {col} FROM vector_default ORDER BY rowid"))
        .expect("stored attr values");
    for s in &stored {
        assert!(
            s.len() > VEC0_TEXT_INLINE_BYTES,
            "the probe is only meaningful when the STORED value spills the inline view: {}",
            s.len()
        );
    }

    let report = engine.erase_source(source).expect(
        "erase_source must succeed for a row carrying a >12-byte filterable attribute value",
    );
    assert_eq!(report.nodes_excised, 3, "all three nodes erased");
    assert_eq!(vector_row_count(engine), 0, "zero vec0 residue after erase_source");

    engine.close().unwrap();
}

// ===========================================================================
// (3) `purge` over a row whose filterable attribute value is long.
// ===========================================================================

#[test]
fn purge_erases_rows_with_long_filterable_attribute_values() {
    let (_dir, path) = fixture("tc76_purge");
    let opened = open(&path);
    let engine = &opened.engine;
    engine.configure_projections(&[filterable_spec("priority")], &[]).expect("configure");

    let value = "z".repeat(1024);
    engine
        .write(&[node(
            "doc",
            "P1",
            &format!(r#"{{"title":"purge me, a document","priority":"{value}"}}"#),
            "test:tc76-purge",
        )])
        .expect("write");
    engine.drain(10_000).expect("drain");
    assert_eq!(vector_row_count(engine), 1, "one vec0 row at rest before the purge");

    engine.transition("P1", LifecycleState::Deleted, None).expect("soft-delete");
    engine
        .purge("P1")
        .expect("purge must succeed for a row carrying a >12-byte filterable attribute value");
    assert_eq!(vector_row_count(engine), 0, "zero vec0 residue after purge");

    // The value must be gone from the vec0 TEXT shadow too, not merely from the
    // virtual table's view of it — an erasure that left it there would be exactly
    // the `search_index_v2` leak class.
    engine.close().unwrap();
    let conn = Connection::open(&path).expect("raw reopen");
    let shadows: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' \
                 AND name LIKE 'vector_default_metadatatext%'",
            )
            .expect("prepare shadow scan");
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).expect("shadow names");
        rows.collect::<rusqlite::Result<Vec<_>>>().expect("collect shadow names")
    };
    assert!(!shadows.is_empty(), "the vec0 TEXT shadow table(s) must exist to be checked");
    for shadow in &shadows {
        let left: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM \"{shadow}\""), [], |r| r.get(0))
            .expect("read the vec0 TEXT shadow table");
        assert_eq!(left, 0, "the long attribute value is gone from {shadow}");
    }
}

// ===========================================================================
// (4) The attr COLUMN-NAME axis is not implicated (already shipped shape).
// ===========================================================================

#[test]
fn long_attr_column_name_with_short_value_deletes_cleanly() {
    let (_dir, path) = fixture("tc76_col_name");
    let opened = open(&path);
    let engine = &opened.engine;
    engine.configure_projections(&[filterable_spec("priority")], &[]).expect("configure");

    let col = attr_col("priority");
    assert!(
        col.len() > VEC0_TEXT_INLINE_BYTES,
        "an attribute name of >= 4 chars already yields a >12-char column identifier: {col}"
    );

    let source = "test:tc76-colname";
    engine
        .write(&[node("doc", "C1", r#"{"title":"short valued doc","priority":"high"}"#, source)])
        .expect("write");
    engine.drain(10_000).expect("drain");
    let stored = engine
        .query_text_col_for_test(&format!("SELECT {col} FROM vector_default"))
        .expect("stored attr value");
    assert_eq!(stored.len(), 1);
    assert!(
        stored[0].len() <= VEC0_TEXT_INLINE_BYTES,
        "the VALUE fits the inline view even though the COLUMN NAME is long: {:?}",
        stored[0]
    );

    engine.erase_source(source).expect("erase_source with a long column name + short value");
    assert_eq!(vector_row_count(engine), 0, "zero vec0 residue");

    engine.close().unwrap();
}

// ===========================================================================
// (5) A `kind` longer than 12 bytes cannot reach `vector_default` at all.
// ===========================================================================

#[test]
fn kind_longer_than_twelve_bytes_cannot_reach_vector_default() {
    let (_dir, path) = fixture("tc76_kind");
    let opened = open(&path);
    let engine = &opened.engine;

    // `PreparedWrite::Node` accepts ANY non-empty kind; only the vec0 enrolment
    // doors are restricted (`kind_is_vector_committable` -> `resolve_source_type`).
    let long_kind = "quarterly_report_supplement"; // 27 bytes
    assert!(long_kind.len() > VEC0_TEXT_INLINE_BYTES);
    engine
        .write(&[
            node("doc", "K1", r#"{"title":"an ordinary document"}"#, "test:tc76-kind"),
            node(long_kind, "K2", r#"{"title":"a long-kinded record"}"#, "test:tc76-kind"),
        ])
        .expect("write both kinds");
    engine.drain(10_000).expect("drain");

    let kinds = engine
        .query_text_col_for_test("SELECT kind FROM vector_default ORDER BY rowid")
        .expect("vec0 kinds");
    assert_eq!(kinds, vec!["doc".to_string()], "only the committable kind reaches vec0: {kinds:?}");
    let over: i64 = engine
        .query_i64_col_for_test(&format!(
            "SELECT COUNT(*) FROM vector_default WHERE length(kind) > {VEC0_TEXT_INLINE_BYTES}"
        ))
        .expect("over-length kind count")[0];
    assert_eq!(over, 0, "no vec0 row can carry a kind longer than the inline view");

    // The same holds for the partition key: its domain is `resolve_source_type`'s.
    let types = engine
        .query_text_col_for_test("SELECT DISTINCT source_type FROM vector_default")
        .expect("vec0 source_types");
    for t in &types {
        assert!(
            t.len() <= 9,
            "source_type comes from the locked Pack-1 vocabulary (max 9 bytes): {t}"
        );
    }

    engine.close().unwrap();
}
