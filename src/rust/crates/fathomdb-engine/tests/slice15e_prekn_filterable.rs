//! 0.8.20 Slice 15e — pre-KNN vector-path routing for a registry `filterable`
//! attribute, via a NON-DESTRUCTIVE vec0 reshape.
//!
//! Slice 15d already projects `filterable` attributes into the row-owned EAV
//! store (`canonical_attributes`). 15e adds ONLY the missing piece: a
//! `filterable` attribute compiles into the INDEXED PRE-KNN vec0 metadata column
//! (`ADR-0.8.11` D3 — no demotion to a post-KNN `json_extract`). Declaring the
//! projection reshapes `vector_default` NON-DESTRUCTIVELY (following the shipped
//! `migrate_vector_partition_pack1_to_pack2` precedent): the existing rows are
//! re-inserted preserving their `rowid` and their `embedding_bin` bytes VERBATIM.
//!
//! The four load-bearing conditions (any one broken ⇒ silently wrong results):
//!   1. the re-insert lists `rowid` explicitly (vec0 row ↔ node is `rowid ==
//!      write_cursor`);
//!   2. the attribute column is PLAIN metadata, never a vec0 `aux`/`+` column (aux
//!      hard-errors a filtered KNN);
//!   3. pre-existing rows back-fill the `''` sentinel (fail-to-match, not error);
//!   4. `embedding_bin` is copied VERBATIM (`vec_bit`), never re-quantized (old +
//!      new Hamming distances must stay comparable).

use std::sync::Arc;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{
    vector_phase1_sql_for_test, Engine, InitialState, PreparedWrite, ProjectionRole,
    ProjectionSpec, SearchFilter, SourceId,
};
use fathomdb_schema::SQLITE_SUFFIX;
use rusqlite::Connection;
use std::collections::BTreeSet;
use tempfile::TempDir;

const DIM: usize = 8;

/// A deterministic embedder that VARIES by text (so old and new rows get
/// distinct bit vectors, letting us prove Hamming distances stay comparable
/// across the reshape). Not mean-centering-required (identity name is not
/// `bge-small`), so the pin/re-quantize path stays inert.
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
        // Guarantee a non-zero vector so quantization is well-defined.
        if v.iter().all(|x| *x == 0.0) {
            v[0] = 1.0;
        }
        Ok(v)
    }
}

/// Independent oracle for the documented byte-safe column encoding:
/// `attr_` + lowercase hex of the UTF-8 bytes of the attribute name.
/// (Deliberately a second implementation from the engine's, so the test pins the
/// SCHEME, not just agreement with one function.)
fn attr_col(name: &str) -> String {
    let mut s = String::from("attr_");
    for b in name.as_bytes() {
        s.push_str(&format!("{b:02x}"));
    }
    s
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

fn node(logical: &str, body_json: &str) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: body_json.to_string(),
        source_id: SourceId::new("test:15e").expect("source id"),
        logical_id: Some(logical.to_string()),
        state: InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

fn filterable_spec(name: &str) -> ProjectionSpec {
    let mut roles = BTreeSet::new();
    roles.insert(ProjectionRole::Filterable);
    ProjectionSpec { name: name.to_string(), roles, fts: None, vector: None }
}

fn table_sql(path: &std::path::Path) -> String {
    let conn = Connection::open(path).expect("raw reopen");
    conn.query_row(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name='vector_default'",
        [],
        |row| row.get::<_, String>(0),
    )
    .expect("read vector_default sql")
}

// ===========================================================================
// (1) A filterable-attribute predicate compiles into the phase-1 pre-KNN clause.
// ===========================================================================

#[test]
fn filterable_predicate_compiles_into_prekn_match_clause() {
    // Attribute-only filter: the attr predicate lands at ?3 (after ?1 sign-quant
    // query, ?2 f32 rerank query), INSIDE the phase-1 candidates `MATCH ... WHERE`.
    let filter = SearchFilter {
        attributes: vec![("priority".to_string(), "high".to_string())],
        ..Default::default()
    };
    let sql = vector_phase1_sql_for_test(Some(&filter));
    let col = attr_col("priority"); // attr_7072696f72697479
    assert_eq!(col, "attr_7072696f72697479", "byte-safe hex encoding of the name");
    assert!(
        sql.contains(&format!("AND {col}=?3")),
        "filterable attribute lowers to the indexed pre-KNN column at ?3:\n{sql}"
    );
    // D3 no-demotion: it is NEVER a post-KNN json_extract.
    assert!(!sql.contains("json_extract"), "must not demote to a post-KNN json_extract:\n{sql}");
    // The predicate sits in the phase-1 candidates MATCH block (before ORDER BY
    // distance), not in the phase-2 rerank tail.
    let candidates_block = sql.split("ORDER BY distance").next().unwrap();
    assert!(
        candidates_block.contains(&format!("AND {col}=?3")),
        "attr predicate must be pre-KNN (inside the MATCH candidates block):\n{sql}"
    );

    // Combined with a shipped metadata field: metadata keeps canonical order, the
    // attribute is appended AFTER it (kind ?3, attr ?4).
    let combined = SearchFilter {
        kind: Some("doc".to_string()),
        attributes: vec![("priority".to_string(), "high".to_string())],
        ..Default::default()
    };
    let csql = vector_phase1_sql_for_test(Some(&combined));
    assert!(csql.contains("AND kind=?3"), "metadata field keeps ?3:\n{csql}");
    assert!(csql.contains(&format!("AND {col}=?4")), "attribute appended at ?4:\n{csql}");

    // A name with a space (Slice-15d validated names allow spaces/unicode) must
    // still yield a bare, unquoted, byte-safe identifier vec0 accepts.
    let spaced = SearchFilter {
        attributes: vec![("due date".to_string(), "2020".to_string())],
        ..Default::default()
    };
    let ssql = vector_phase1_sql_for_test(Some(&spaced));
    let scol = attr_col("due date"); // attr_6475652064617465
    assert_eq!(scol, "attr_6475652064617465");
    assert!(
        ssql.contains(&format!("AND {scol}=?3")),
        "space-containing name is byte-safe:\n{ssql}"
    );
    assert!(!ssql.contains('"'), "no quoted identifier (vec0 rejects them):\n{ssql}");
}

// ===========================================================================
// (2) ALIGNMENT + COHERENCE: reshape preserves rowids + bits; sentinel skips.
// ===========================================================================

#[test]
fn reshape_is_nondestructive_preserves_bits_rowids_and_sentinel() {
    let (_dir, path) = fixture("s15e_reshape");
    let opened = open(&path);
    let engine = &opened.engine;

    // OLD rows: written BEFORE the projection is declared, bodies WITHOUT the
    // attribute — so after the reshape they carry the `''` sentinel.
    engine
        .write(&[
            node("OLD1", r#"{"title":"alpha document about vectors"}"#),
            node("OLD2", r#"{"title":"beta note concerning storage"}"#),
        ])
        .expect("write old");
    engine.drain(10_000).expect("drain old");

    let old_rowids = engine
        .query_i64_col_for_test("SELECT rowid FROM vector_default ORDER BY rowid")
        .expect("old rowids");
    assert_eq!(old_rowids.len(), 2, "two old vector rows exist pre-reshape");
    let old_bits: Vec<(i64, Vec<u8>)> =
        old_rowids.iter().map(|&r| (r, engine.read_vector_bin_for_test(r).expect("bin"))).collect();
    let old_hamming: Vec<i64> = engine
        .query_i64_col_for_test(
            "SELECT CAST(vec_distance_hamming(embedding_bin, \
                 vec_quantize_binary(vec_f32('[1,0,0,0,0,0,0,0]'))) AS INTEGER) \
             FROM vector_default ORDER BY rowid",
        )
        .expect("old hamming");

    // Declare the filterable projection — triggers the NON-DESTRUCTIVE reshape.
    let delta =
        engine.configure_projections(&[filterable_spec("priority")], &[]).expect("configure");
    assert!(delta.built.contains(&"priority".to_string()), "projection built");

    // The reshaped table has a PLAIN metadata attr column, never an aux (`+`) one.
    let col = attr_col("priority");
    let sql = table_sql(&path);
    assert!(sql.contains(&format!("{col} TEXT")), "plain TEXT attr column present:\n{sql}");
    assert!(!sql.contains(&format!("+{col}")), "attr column must NOT be aux (`+`):\n{sql}");

    // Condition #1 (rowid alignment): the same rowids survive the reshape.
    let after_rowids = engine
        .query_i64_col_for_test("SELECT rowid FROM vector_default ORDER BY rowid")
        .expect("after rowids");
    assert_eq!(after_rowids, old_rowids, "rowids preserved verbatim across reshape");

    // Condition #4 (verbatim bits): embedding_bin bytes are byte-identical.
    for (r, before) in &old_bits {
        let after = engine.read_vector_bin_for_test(*r).expect("bin after");
        assert_eq!(
            &after, before,
            "embedding_bin copied verbatim (not re-quantized) for rowid {r}"
        );
    }

    // Condition #3 (sentinel): pre-existing rows carry `''` in the new column.
    let sentinels = engine
        .query_text_col_for_test(&format!("SELECT {col} FROM vector_default ORDER BY rowid"))
        .expect("sentinel read");
    assert!(
        sentinels.iter().all(|s| s.is_empty()),
        "old rows back-fill the '' sentinel: {sentinels:?}"
    );

    // NEW row: written AFTER the projection, body carries the attribute → the
    // column populates from the body at write time.
    engine
        .write(&[node("NEW1", r#"{"title":"gamma record on ranking","priority":"high"}"#)])
        .expect("write new");
    engine.drain(10_000).expect("drain new");

    let new_rowids: Vec<i64> = engine
        .query_i64_col_for_test("SELECT rowid FROM vector_default ORDER BY rowid")
        .expect("new rowids")
        .into_iter()
        .filter(|r| !old_rowids.contains(r))
        .collect();
    assert_eq!(new_rowids.len(), 1, "one new vector row");
    let new_id = new_rowids[0];
    let new_val = engine
        .query_text_col_for_test(&format!("SELECT {col} FROM vector_default WHERE rowid={new_id}"))
        .expect("new attr value");
    assert_eq!(new_val, vec!["high".to_string()], "new row's attr column populated from body");

    // Coherence: old + new Hamming distances are on the SAME scale [0, dim] — the
    // old bits were NOT re-quantized under a different centering.
    let all_hamming = engine
        .query_i64_col_for_test(
            "SELECT CAST(vec_distance_hamming(embedding_bin, \
                 vec_quantize_binary(vec_f32('[1,0,0,0,0,0,0,0]'))) AS INTEGER) \
             FROM vector_default ORDER BY rowid",
        )
        .expect("all hamming");
    assert!(
        all_hamming.iter().all(|d| (0..=DIM as i64).contains(d)),
        "all Hamming distances comparable in [0,{DIM}]: {all_hamming:?}"
    );
    // Old rows' Hamming distances are UNCHANGED (byte-identity ⇒ same distance).
    for (i, &r) in old_rowids.iter().enumerate() {
        let idx = after_rowids.iter().position(|x| *x == r).unwrap();
        assert_eq!(all_hamming[idx], old_hamming[i], "old row {r} Hamming distance unchanged");
    }

    // Condition #2 + pre-KNN pruning: a filtered KNN over the plain column does
    // NOT error, and prunes to ONLY the populated (new) row — the sentinel old
    // rows fail-to-match.
    let matched = engine
        .query_i64_col_for_test(&format!(
            "SELECT rowid FROM vector_default \
             WHERE embedding_bin MATCH vec_quantize_binary(vec_f32('[1,0,0,0,0,0,0,0]')) \
               AND {col}='high' ORDER BY distance LIMIT 10"
        ))
        .expect("filtered KNN must not error (plain column, not aux)");
    assert_eq!(matched, vec![new_id], "filter matches only the populated new row");
    for r in &old_rowids {
        assert!(!matched.contains(r), "sentinel old row {r} is skipped by the filter");
    }

    engine.close().unwrap();
}

// ===========================================================================
// (3) Idempotent re-registration of the same filterable set is a vec0 no-op.
// ===========================================================================

#[test]
fn idempotent_refilterable_registration_is_vec0_noop() {
    let (_dir, path) = fixture("s15e_idempotent");
    let opened = open(&path);
    let engine = &opened.engine;
    engine
        .write(&[node("A", r#"{"title":"one"}"#), node("B", r#"{"title":"two"}"#)])
        .expect("write");
    engine.drain(10_000).expect("drain");

    let first =
        engine.configure_projections(&[filterable_spec("priority")], &[]).expect("first configure");
    assert!(first.built.contains(&"priority".to_string()), "first apply builds + reshapes");

    let sql_after_first = table_sql(&path);
    let rowids = engine
        .query_i64_col_for_test("SELECT rowid FROM vector_default ORDER BY rowid")
        .expect("rowids");
    let bits: Vec<Vec<u8>> =
        rowids.iter().map(|&r| engine.read_vector_bin_for_test(r).expect("bin")).collect();

    // Re-register the IDENTICAL set: diff is a no-op, so vec0 is untouched.
    let second = engine
        .configure_projections(&[filterable_spec("priority")], &[])
        .expect("second configure");
    assert!(second.unchanged, "identical re-registration diffs to a no-op");

    assert_eq!(table_sql(&path), sql_after_first, "vec0 shape byte-unchanged on no-op re-register");
    let rowids2 = engine
        .query_i64_col_for_test("SELECT rowid FROM vector_default ORDER BY rowid")
        .expect("rowids2");
    assert_eq!(rowids2, rowids, "rowids byte-unchanged (no re-insert)");
    for (i, &r) in rowids.iter().enumerate() {
        assert_eq!(
            engine.read_vector_bin_for_test(r).expect("bin2"),
            bits[i],
            "embeddings unchanged"
        );
    }

    engine.close().unwrap();
}

// ===========================================================================
// (4) A filtered KNN over the attr column does NOT hard-error (aux-trap avoided).
// ===========================================================================

#[test]
fn filtered_knn_over_attr_column_does_not_hard_error() {
    let (_dir, path) = fixture("s15e_aux_trap");
    let opened = open(&path);
    let engine = &opened.engine;
    engine.write(&[node("A", r#"{"priority":"low"}"#)]).expect("write");
    engine.drain(10_000).expect("drain");
    engine.configure_projections(&[filterable_spec("priority")], &[]).expect("configure");

    let col = attr_col("priority");
    // If the column had been declared aux (`+attr_...`) this KNN would hard-error.
    let res = engine.query_i64_col_for_test(&format!(
        "SELECT rowid FROM vector_default \
         WHERE embedding_bin MATCH vec_quantize_binary(vec_f32('[1,0,0,0,0,0,0,0]')) \
           AND {col}='low' ORDER BY distance LIMIT 5"
    ));
    assert!(res.is_ok(), "filtered KNN over a plain metadata attr column must not error: {res:?}");

    engine.close().unwrap();
}
