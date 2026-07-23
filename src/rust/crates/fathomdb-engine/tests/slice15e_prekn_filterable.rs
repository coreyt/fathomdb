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

fn edge(logical: &str, from: &str, to: &str, body_json: &str) -> PreparedWrite {
    PreparedWrite::Edge {
        kind: "link".to_string(),
        from: from.to_string(),
        to: to.to_string(),
        source_id: SourceId::new("test:15e").expect("source id"),
        logical_id: Some(logical.to_string()),
        body: Some(body_json.to_string()),
        t_valid: None,
        t_invalid: None,
        confidence: None,
        extractor_model_id: None,
        temporal_fallback: None,
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
    // `SearchFilter` is `#[non_exhaustive]` (0.8.20 Slice 15e fix-2); build from
    // `default()` (downstream crates, incl. this test crate, cannot struct-literal).
    let mut filter = SearchFilter::default();
    filter.attributes = vec![("priority".to_string(), "high".to_string())];
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
    let mut combined = SearchFilter::default();
    combined.kind = Some("doc".to_string());
    combined.attributes = vec![("priority".to_string(), "high".to_string())];
    let csql = vector_phase1_sql_for_test(Some(&combined));
    assert!(csql.contains("AND kind=?3"), "metadata field keeps ?3:\n{csql}");
    assert!(csql.contains(&format!("AND {col}=?4")), "attribute appended at ?4:\n{csql}");

    // A name with a space (Slice-15d validated names allow spaces/unicode) must
    // still yield a bare, unquoted, byte-safe identifier vec0 accepts.
    let mut spaced = SearchFilter::default();
    spaced.attributes = vec![("due date".to_string(), "2020".to_string())];
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
    // fix-3 [P2] — the vec0 filter column stores a PRESENT value encoded `\x01 || V`
    // (disjoint from the `''`-absent sentinel). RAW `canonical_attributes` stays "high".
    assert_eq!(
        new_val,
        vec!["\u{1}high".to_string()],
        "new row's attr column populated from body (fix-3 marker-encoded)"
    );

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
               AND {col}=char(1)||'high' ORDER BY distance LIMIT 10"
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

// ===========================================================================
// fix-1 finding 1 [P2] — TOTAL backend-dispatch: the attribute-equality filter
// is applied on the TEXT/FTS arm too, so a doc that MATCHES the FTS query but
// FAILS the attribute filter does NOT leak through the FTS arm of a hybrid
// search (ADR-0.8.11 D3 — every filter term has a defined outcome on EVERY
// surface). Isolated to the FTS arm: the projection is declared BEFORE the docs
// are written, so the vec0 `attr_<hex>` column is populated at write time and
// the vector arm already prunes the MISS row — the ONLY way it can appear is via
// the (previously unfiltered) FTS arm.
// ===========================================================================

#[test]
fn hybrid_fts_arm_applies_attribute_filter() {
    let (_dir, path) = fixture("s15e_hybrid_totality");
    let opened = open(&path);
    let engine = &opened.engine;

    // Declare the filterable projection FIRST, so the vec0 attr column is
    // populated from each body at write time (async worker path).
    engine.configure_projections(&[filterable_spec("priority")], &[]).expect("configure");

    // Two docs, BOTH matching the FTS term "vectors", differing ONLY in the
    // filterable attribute: MATCH has priority=high, MISS has priority=low.
    engine
        .write(&[
            node("MATCH", r#"{"title":"alpha vectors document","priority":"high"}"#),
            node("MISS", r#"{"title":"beta vectors document","priority":"low"}"#),
        ])
        .expect("write");
    engine.drain(10_000).expect("drain");

    // Sanity: the vec0 attr column IS populated at write time (so the vector arm
    // prunes MISS on its own; any MISS in results can ONLY come from the FTS arm).
    let col = attr_col("priority");
    let miss_vec_val = engine
        .query_text_col_for_test(&format!(
            "SELECT v.{col} FROM vector_default v \
             JOIN canonical_nodes n ON n.write_cursor = v.rowid \
             WHERE n.logical_id = 'MISS'"
        ))
        .expect("read MISS vec0 attr");
    // fix-3 [P2] — vec0 filter column stores the PRESENT value marker-encoded.
    assert_eq!(
        miss_vec_val,
        vec!["\u{1}low".to_string()],
        "MISS vec0 column populated from body (fix-3 marker-encoded)"
    );

    // `#[non_exhaustive]` (0.8.20 Slice 15e fix-2): build from `default()`.
    let mut filter = SearchFilter::default();
    filter.attributes = vec![("priority".to_string(), "high".to_string())];
    let res = engine.search_filtered("vectors", Some(filter)).expect("search");
    let bodies: Vec<String> = res.results.iter().map(|h| h.body.clone()).collect();

    // The matching (priority=high) doc must be present.
    assert!(
        bodies.iter().any(|b| b.contains("alpha vectors")),
        "the priority=high doc must be present: {bodies:?}"
    );
    // The MISS doc matches the FTS query but FAILS the attribute filter; it must
    // NOT leak through the FTS arm.
    assert!(
        !bodies.iter().any(|b| b.contains("beta vectors")),
        "priority=low doc must NOT leak through the FTS arm: {bodies:?}"
    );

    engine.close().unwrap();
}

// ===========================================================================
// fix-1 finding 2 [P2] — a PRE-EXISTING row whose body HAS the attribute is
// back-filled from the already-populated `canonical_attributes` (not the blanket
// `''` sentinel), so it is IMMEDIATELY filterable after the projection is
// declared. A genuinely-absent row still gets `''` and correctly fails-to-match.
// ===========================================================================

#[test]
fn existing_row_backfilled_from_canonical_attributes_not_blanket_sentinel() {
    let (_dir, path) = fixture("s15e_existing_backfill");
    let opened = open(&path);
    let engine = &opened.engine;

    // PRE-EXISTING rows written + embedded BEFORE the projection is declared.
    // PRESENT: body carries priority=high. ABSENT: body has no priority at all.
    engine
        .write(&[
            node("PRESENT", r#"{"title":"vectors alpha record","priority":"high"}"#),
            node("ABSENT", r#"{"title":"vectors beta record"}"#),
        ])
        .expect("write");
    engine.drain(10_000).expect("drain");

    // Declare the filterable projection: `configure_projections` backfills
    // `canonical_attributes` from the bodies FIRST, THEN reshapes vec0. The
    // reshape must back-fill the new attr column from `canonical_attributes`.
    engine.configure_projections(&[filterable_spec("priority")], &[]).expect("configure");

    let col = attr_col("priority");

    // RAW vec0 assertion — the load-bearing one for the reshape back-fill:
    // PRESENT reads its real value (marker-encoded "\x01high", fix-3), NOT the
    // blanket '' absent sentinel.
    let present_val = engine
        .query_text_col_for_test(&format!(
            "SELECT v.{col} FROM vector_default v \
             JOIN canonical_nodes n ON n.write_cursor = v.rowid \
             WHERE n.logical_id = 'PRESENT'"
        ))
        .expect("read PRESENT vec0 attr");
    assert_eq!(
        present_val,
        vec!["\u{1}high".to_string()],
        "pre-existing row whose body HAS the attribute must back-fill its real value \
         (fix-3 marker-encoded), not the '' absent sentinel"
    );

    // The genuinely-absent row still gets the '' sentinel (condition #3 holds).
    let absent_val = engine
        .query_text_col_for_test(&format!(
            "SELECT v.{col} FROM vector_default v \
             JOIN canonical_nodes n ON n.write_cursor = v.rowid \
             WHERE n.logical_id = 'ABSENT'"
        ))
        .expect("read ABSENT vec0 attr");
    assert_eq!(absent_val, vec![String::new()], "genuinely-absent row keeps the '' sentinel");

    // A pre-KNN filter `priority == high` must RETURN the pre-existing PRESENT row
    // and exclude ABSENT (results-level assertion).
    // `#[non_exhaustive]` (0.8.20 Slice 15e fix-2): build from `default()`.
    let mut filter = SearchFilter::default();
    filter.attributes = vec![("priority".to_string(), "high".to_string())];
    let res = engine.search_filtered("vectors", Some(filter)).expect("search");
    let bodies: Vec<String> = res.results.iter().map(|h| h.body.clone()).collect();
    assert!(
        bodies.iter().any(|b| b.contains("alpha record")),
        "pre-existing PRESENT row must be filterable immediately: {bodies:?}"
    );
    assert!(
        !bodies.iter().any(|b| b.contains("beta record")),
        "genuinely-absent row must fail-to-match: {bodies:?}"
    );

    engine.close().unwrap();
}

// ===========================================================================
// fix-2 (Finding 1 → HITL ruling (A)) — PIN: an attribute filter is NODE-SCOPED
// and therefore EXCLUDES edge hits on BOTH retrieval arms (edge-FTS AND
// edge-vector). This is a CHARACTERIZATION / pin test, not a RED→GREEN — the
// behaviour is already correct post-fix-1 and this test locks it against silent
// regression toward any of the reserved widenings (B: edges pass through; C:
// project edge attributes; D: endpoint-node filtering).
//
// Mechanism this pins:
//   * Attribute projection is `PreparedWrite::Node`-gated (`collect_projection_jobs`
//     / `project_one_attribute`), so an EDGE is NEVER projected into
//     `canonical_attributes` NOR carries a populated vec0 `attr_<hex>` column —
//     even when the edge body itself names the attribute (this edge body carries
//     `"priority":"high"` and is STILL excluded).
//   * edge-vector arm: the edge IS a `vector_default` candidate (rowid =
//     write_cursor, kind `edge_fact`), but its `attr_<hex>` column is the `''`
//     sentinel (the async worker reads the body from `canonical_nodes`, which has
//     no row for an edge cursor), so the pre-KNN `attr_<hex>='high'` predicate
//     prunes it.
//   * edge-FTS arm: the edge IS a `search_index_edges` candidate, but
//     `edge_fts_hit_passes_filter` → `hit_attributes_pass_filter` finds no
//     `canonical_attributes` row for the edge's write_cursor, reads the `''`
//     sentinel, and fails the non-empty equality.
//
// `search_filtered` fuses both arms, so a regression that let edges through on
// EITHER arm would make the edge body reappear in the filtered results and fail
// this test — that is how the single fused-search assertion guards BOTH arms. The
// raw per-arm state assertions below additionally pin each arm's exclusion
// mechanism independently.
// ===========================================================================

#[test]
fn attribute_filter_excludes_edge_hits_on_both_arms() {
    let (_dir, path) = fixture("s15e_edge_excluded_both_arms");
    let opened = open(&path);
    let engine = &opened.engine;

    // Declare the filterable projection FIRST, so every subsequently-embedded row
    // is written against a `vector_default` that already carries the `attr_<hex>`
    // column (nodes populate it from their body; edges get the `''` sentinel).
    engine.configure_projections(&[filterable_spec("priority")], &[]).expect("configure");

    // A NODE carrying priority=high (the non-vacuous control — it must SURVIVE the
    // filter), the two edge endpoints, and an EDGE whose body ALSO names
    // priority=high yet is node-scope-excluded. All three content rows share the
    // FTS token "sharedtoken" and, in a tiny corpus, are all vector candidates.
    engine
        .write(&[
            node("ENTFROM", r#"{"title":"origin entity"}"#),
            node("ENTTO", r#"{"title":"target entity"}"#),
            node("NODEMATCH", r#"{"title":"sharedtoken alpha node","priority":"high"}"#),
            edge(
                "EDGEFACT",
                "ENTFROM",
                "ENTTO",
                r#"{"fact":"sharedtoken edgefact beta","priority":"high"}"#,
            ),
        ])
        .expect("write");
    engine.drain(10_000).expect("drain");

    // The edge's write_cursor keys every one of its projection shadow rows.
    let edge_cursor = engine
        .query_i64_col_for_test(
            "SELECT write_cursor FROM canonical_edges WHERE logical_id='EDGEFACT'",
        )
        .expect("edge cursor");
    assert_eq!(edge_cursor.len(), 1, "exactly one edge row");
    let ec = edge_cursor[0];
    let col = attr_col("priority");

    // --- edge-VECTOR arm mechanism: candidate present, attr col is the sentinel.
    let edge_vec_rowid = engine
        .query_i64_col_for_test(&format!("SELECT rowid FROM vector_default WHERE rowid={ec}"))
        .expect("edge vector candidate");
    assert_eq!(edge_vec_rowid, vec![ec], "edge IS a vector_default candidate (rowid=write_cursor)");
    let edge_attr = engine
        .query_text_col_for_test(&format!("SELECT {col} FROM vector_default WHERE rowid={ec}"))
        .expect("edge attr col");
    assert_eq!(
        edge_attr,
        vec![String::new()],
        "edge's vec0 attr column is the '' sentinel (edges are not attribute-projected), \
         so the pre-KNN attr_<hex>='high' predicate prunes it"
    );

    // --- edge-FTS arm mechanism: candidate present, but NO canonical_attributes row.
    let edge_fts_rows = engine
        .query_i64_col_for_test(&format!(
            "SELECT COUNT(*) FROM search_index_edges WHERE write_cursor={ec}"
        ))
        .expect("edge fts candidate");
    assert_eq!(edge_fts_rows, vec![1], "edge IS a search_index_edges (FTS) candidate");
    let edge_eav_rows = engine
        .query_i64_col_for_test(&format!(
            "SELECT COUNT(*) FROM canonical_attributes WHERE write_cursor={ec}"
        ))
        .expect("edge eav count");
    assert_eq!(
        edge_eav_rows,
        vec![0],
        "edge has NO canonical_attributes row (attribute projection is Node-gated), \
         so hit_attributes_pass_filter reads '' and the FTS arm excludes it"
    );

    // --- CONTROL (unfiltered): the edge IS retrievable through search, so the
    // filtered-absence below is a real exclusion, not a never-indexed edge.
    let unfiltered = engine.search_filtered("sharedtoken", None).expect("unfiltered search");
    let ub: Vec<String> = unfiltered.results.iter().map(|h| h.body.clone()).collect();
    assert!(ub.iter().any(|b| b.contains("edgefact")), "edge is retrievable unfiltered: {ub:?}");
    assert!(ub.iter().any(|b| b.contains("alpha node")), "node is retrievable unfiltered: {ub:?}");

    // --- FILTERED: node with priority=high SURVIVES; edge (node-scope-excluded on
    // BOTH arms) is DROPPED, even though its own body names priority=high.
    // `#[non_exhaustive]` (0.8.20 Slice 15e fix-2): build from `default()`.
    let mut filter = SearchFilter::default();
    filter.attributes = vec![("priority".to_string(), "high".to_string())];
    let filtered = engine.search_filtered("sharedtoken", Some(filter)).expect("filtered search");
    let fb: Vec<String> = filtered.results.iter().map(|h| h.body.clone()).collect();
    assert!(
        fb.iter().any(|b| b.contains("alpha node")),
        "the matching NODE with the attribute must still appear (non-vacuous control): {fb:?}"
    );
    assert!(
        !fb.iter().any(|b| b.contains("edgefact")),
        "(A) edges are EXCLUDED: an attribute filter is node-scoped, so the edge hit must NOT \
         appear via the edge-FTS arm NOR the edge-vector arm: {fb:?}"
    );

    engine.close().unwrap();
}

// ===========================================================================
// fix-3 [P2] — a REAL empty-string attribute value (`{"status":""}`) must be
// distinguished from an ABSENT attribute. The `''` empty-string vec0 sentinel
// used to mean BOTH "absent" AND "present-and-equal-to-''", so an equality filter
// `status == ""` false-matched every absent row on BOTH arms. fix-3 encodes a
// PRESENT value `V` in the vec0 `attr_<hex>` column as `\x01 || V` (marker byte),
// reserving `''` for ABSENT; the FTS arm distinguishes by canonical_attributes
// row EXISTENCE (present-empty has a row with attr_value='' ; absent has no row).
// The two arms produce IDENTICAL pass/fail. property_search_index / the RAW
// canonical_attributes.attr_value are UNCHANGED (only the vec0 filter column and
// the filter-value lowering are encoded).
// ===========================================================================

#[test]
fn empty_string_attribute_value_vs_absent_vector_arm() {
    let (_dir, path) = fixture("s15e_empty_vs_absent_vec");
    let opened = open(&path);
    let engine = &opened.engine;

    // Declare filterable FIRST so the vec0 attr column is populated at write time.
    engine.configure_projections(&[filterable_spec("status")], &[]).expect("configure");

    // PRESENTEMPTY: status is a real empty string. ABSENT: no status key at all.
    // PRESENTOPEN: status = "open". All share the FTS token "sharedtoken".
    engine
        .write(&[
            node("PRESENTEMPTY", r#"{"title":"sharedtoken alpha","status":""}"#),
            node("ABSENT", r#"{"title":"sharedtoken beta"}"#),
            node("PRESENTOPEN", r#"{"title":"sharedtoken gamma","status":"open"}"#),
        ])
        .expect("write");
    engine.drain(10_000).expect("drain");

    let col = attr_col("status");
    let marker = "\u{1}"; // vec0 present-marker (fix-3): enc(V) = "\x01" || V

    // ---- RAW at-rest, vec0 arm: PRESENT values carry the marker; ABSENT stays ''.
    let read_col = |logical: &str| {
        engine
            .query_text_col_for_test(&format!(
                "SELECT v.{col} FROM vector_default v \
                 JOIN canonical_nodes n ON n.write_cursor = v.rowid \
                 WHERE n.logical_id = '{logical}'"
            ))
            .expect("read vec0 attr")
    };
    assert_eq!(
        read_col("PRESENTEMPTY"),
        vec![marker.to_string()],
        "present-empty encodes to the marker (enc(\"\")), NOT the '' absent sentinel"
    );
    assert_eq!(
        read_col("ABSENT"),
        vec![String::new()],
        "absent stays the '' sentinel (disjoint from every present value)"
    );
    assert_eq!(
        read_col("PRESENTOPEN"),
        vec![format!("{marker}open")],
        "present 'open' encodes to marker||'open'"
    );

    // ---- canonical_attributes (the FTS-arm data basis): present-empty HAS a row
    // (attr_value='' RAW, unchanged); absent has NO row.
    let eav_value = |logical: &str| {
        engine
            .query_text_col_for_test(&format!(
                "SELECT ca.attr_value FROM canonical_attributes ca \
                 JOIN canonical_nodes n ON n.write_cursor = ca.write_cursor \
                 WHERE n.logical_id = '{logical}' AND ca.attr_name='status'"
            ))
            .expect("eav value read")
    };
    let eav_count = |logical: &str| {
        engine
            .query_i64_col_for_test(&format!(
                "SELECT COUNT(*) FROM canonical_attributes ca \
                 JOIN canonical_nodes n ON n.write_cursor = ca.write_cursor \
                 WHERE n.logical_id = '{logical}' AND ca.attr_name='status'"
            ))
            .expect("eav count read")
    };
    assert_eq!(eav_count("PRESENTEMPTY"), vec![1], "present-empty HAS a canonical_attributes row");
    assert_eq!(
        eav_value("PRESENTEMPTY"),
        vec![String::new()],
        "present-empty canonical_attributes.attr_value is RAW '' (NOT encoded)"
    );
    assert_eq!(eav_count("ABSENT"), vec![0], "absent has NO canonical_attributes row");

    // ---- vector arm INDEPENDENTLY (raw pre-KNN SQL, production `char(1)||V`
    // encoding): filter status="" matches ONLY present-empty; absent is pruned.
    let cursor = |logical: &str| {
        engine
            .query_i64_col_for_test(&format!(
                "SELECT write_cursor FROM canonical_nodes WHERE logical_id='{logical}'"
            ))
            .expect("cursor")[0]
    };
    let (pe, ab, po) = (cursor("PRESENTEMPTY"), cursor("ABSENT"), cursor("PRESENTOPEN"));

    let match_empty = engine
        .query_i64_col_for_test(&format!(
            "SELECT rowid FROM vector_default \
             WHERE embedding_bin MATCH vec_quantize_binary(vec_f32('[1,0,0,0,0,0,0,0]')) \
               AND {col}=char(1)||'' ORDER BY distance LIMIT 10"
        ))
        .expect("vector arm empty filter");
    assert!(match_empty.contains(&pe), "status='' matches present-empty on the vector arm");
    assert!(!match_empty.contains(&ab), "status='' must NOT match ABSENT on the vector arm");
    assert!(
        !match_empty.contains(&po),
        "status='' must NOT match present-'open' on the vector arm"
    );

    let match_open = engine
        .query_i64_col_for_test(&format!(
            "SELECT rowid FROM vector_default \
             WHERE embedding_bin MATCH vec_quantize_binary(vec_f32('[1,0,0,0,0,0,0,0]')) \
               AND {col}=char(1)||'open' ORDER BY distance LIMIT 10"
        ))
        .expect("vector arm open filter");
    assert!(match_open.contains(&po), "status='open' matches present-'open'");
    assert!(!match_open.contains(&pe), "status='open' must NOT match present-empty");
    assert!(!match_open.contains(&ab), "status='open' must NOT match ABSENT");

    engine.close().unwrap();
}

#[test]
fn empty_string_attribute_value_vs_absent_both_arms_fused() {
    let (_dir, path) = fixture("s15e_empty_vs_absent_fused");
    let opened = open(&path);
    let engine = &opened.engine;

    engine.configure_projections(&[filterable_spec("status")], &[]).expect("configure");
    engine
        .write(&[
            node("PRESENTEMPTY", r#"{"title":"sharedtoken alpha","status":""}"#),
            node("ABSENT", r#"{"title":"sharedtoken beta"}"#),
            node("PRESENTOPEN", r#"{"title":"sharedtoken gamma","status":"open"}"#),
        ])
        .expect("write");
    engine.drain(10_000).expect("drain");

    // search_filtered fuses the vector AND the FTS arms (a union). "ABSENT does NOT
    // appear" therefore requires BOTH arms to exclude it — a joint both-arms proof
    // (if EITHER arm false-matched absent, the fused result would surface it).
    let search = |value: &str| -> Vec<String> {
        let mut filter = SearchFilter::default();
        filter.attributes = vec![("status".to_string(), value.to_string())];
        engine
            .search_filtered("sharedtoken", Some(filter))
            .expect("search")
            .results
            .iter()
            .map(|h| h.body.clone())
            .collect()
    };

    // filter status="" : present-empty matches; absent must NOT false-match;
    // present-'open' must NOT match.
    let empty = search("");
    assert!(
        empty.iter().any(|b| b.contains("alpha")),
        "present-empty must match status='' (non-vacuous control): {empty:?}"
    );
    assert!(
        !empty.iter().any(|b| b.contains("beta")),
        "ABSENT must NOT false-match status='' on EITHER arm: {empty:?}"
    );
    assert!(
        !empty.iter().any(|b| b.contains("gamma")),
        "present-'open' must NOT match status='': {empty:?}"
    );

    // filter status="open" : only present-'open' (non-vacuous; the empty filter
    // above must not be the only discriminator).
    let open = search("open");
    assert!(
        open.iter().any(|b| b.contains("gamma")),
        "present-'open' must match status='open': {open:?}"
    );
    assert!(
        !open.iter().any(|b| b.contains("alpha")),
        "present-empty must NOT match status='open': {open:?}"
    );
    assert!(
        !open.iter().any(|b| b.contains("beta")),
        "ABSENT must NOT match status='open': {open:?}"
    );

    engine.close().unwrap();
}

// ===========================================================================
// fix-2 finding 1 [P2] — an UNDECLARED filter attribute is a TYPED rejection on
// BOTH arms, never a raw `no such column` backend crash (vector arm) nor a silent
// no-match (text arm). ADR-0.8.11 D3: every filter term has a DEFINED outcome —
// "compiles" or "typed rejection" — IDENTICAL across arms.
//
// Pre-fix: a filter naming an attribute with NO declared `filterable` projection
// emitted `AND attr_<hex>=?` into the vec0 phase-1 KNN for a column that does not
// exist, so SQLite failed with `no such column` and the whole search returned an
// opaque `Storage` error — while the FTS arm's `hit_attributes_pass_filter`
// probed `canonical_attributes`, found no row, and silently NO-MATCHED. A caller
// typo (or a filter issued before `configure_projections`) therefore became a
// storage/search FAILURE that diverged between arms. The fix validates every
// filter attribute name against the declared `filterable` registry set BEFORE any
// arm runs, returning a typed `InvalidFilter` naming the attribute — so the fused
// two-arm `search_filtered` (which exercises BOTH arms) yields ONE consistent
// typed rejection.
// ===========================================================================

#[test]
fn undeclared_filter_attribute_is_typed_rejection_on_both_arms() {
    use fathomdb_engine::EngineError;
    let (_dir, path) = fixture("s15e_undeclared_attr");
    let opened = open(&path);
    let engine = &opened.engine;

    // A declared `filterable` projection (the control name) + a corpus that is
    // BOTH vector-indexed (embedder present, drained) AND FTS-indexed (they share
    // the token "sharedtoken"), so a filtered `search_filtered` exercises BOTH
    // retrieval arms.
    engine.configure_projections(&[filterable_spec("priority")], &[]).expect("configure");
    engine
        .write(&[
            node("A", r#"{"title":"sharedtoken alpha","priority":"high"}"#),
            node("B", r#"{"title":"sharedtoken beta","priority":"low"}"#),
        ])
        .expect("write");
    engine.drain(10_000).expect("drain");

    // (1) UNDECLARED attribute name → typed InvalidFilter naming the attribute.
    //     The fused `search_filtered` runs BOTH arms, so this single rejection is
    //     the joint both-arms proof: neither the vector arm's `no such column`
    //     crash nor the FTS arm's silent no-match survives — both are pre-empted by
    //     the pre-dispatch validation.
    let mut bad = SearchFilter::default();
    bad.attributes = vec![("nonexistent".to_string(), "x".to_string())];
    match engine.search_filtered("sharedtoken", Some(bad)) {
        Err(EngineError::InvalidFilter { reason }) => assert!(
            reason.contains("nonexistent"),
            "the typed rejection must NAME the undeclared attribute, got: {reason}"
        ),
        Err(other) => panic!(
            "an undeclared filter attribute must be a typed InvalidFilter rejection, not \
             an opaque error: got {other:?}"
        ),
        Ok(r) => panic!(
            "an undeclared filter attribute must be a typed InvalidFilter rejection, not a \
             silent Ok with {} result(s)",
            r.results.len()
        ),
    }

    // (2) CONTROL — a DECLARED attribute whose value is simply ABSENT on the rows
    //     still NO-MATCHES (fix-3 behaviour UNCHANGED): it is an `Ok` result that
    //     excludes the non-matching rows, NOT a rejection.
    let mut declared_absent = SearchFilter::default();
    declared_absent.attributes = vec![("priority".to_string(), "nonesuch".to_string())];
    let res = engine
        .search_filtered("sharedtoken", Some(declared_absent))
        .expect("a DECLARED attribute with an absent value must NO-MATCH, never reject");
    let absent_bodies: Vec<String> = res.results.iter().map(|h| h.body.clone()).collect();
    assert!(
        !absent_bodies.iter().any(|b| b.contains("alpha") || b.contains("beta")),
        "declared-but-absent value excludes the non-matching rows (fix-3 no-match path): \
         {absent_bodies:?}"
    );

    // (3) CONTROL — the declared value that IS present still MATCHES (non-vacuous:
    //     proves the fix did not turn every attribute filter into a rejection).
    let mut declared_present = SearchFilter::default();
    declared_present.attributes = vec![("priority".to_string(), "high".to_string())];
    let ok = engine
        .search_filtered("sharedtoken", Some(declared_present))
        .expect("a declared present value must search normally");
    let present_bodies: Vec<String> = ok.results.iter().map(|h| h.body.clone()).collect();
    assert!(
        present_bodies.iter().any(|b| b.contains("alpha")),
        "the declared present value still matches its row: {present_bodies:?}"
    );

    engine.close().unwrap();
}

// ===========================================================================
// keystone closeout fix-3 (codex §9 [P2], TOCTOU) — the fix-2 undeclared-attr
// validation ran on the WRITER connection BEFORE dispatch, then the reader
// prepared the vec0 query on a DIFFERENT connection/snapshot. A
// `configure_projections` DROP of the `filterable` projection landing in the
// window between the check and the reader snapshot let the `attr_<hex>` vec0
// column vanish AFTER validation passed → the reader crashed with an opaque
// `no such column` `Storage` error: the exact untyped failure fix-2 meant to
// prevent, re-introduced by concurrency.
//
// This test makes that race DETERMINISTIC with the `reader_search_hook` seam,
// which parks the reader worker at the top of `read_search_in_tx` — after the
// caller-side pre-dispatch validation has already passed, and BEFORE the reader
// pins its deferred snapshot. While parked, the main thread commits the DROP on
// the writer connection (the exact race window), then releases the reader, which
// then pins a snapshot that INCLUDES the drop.
//
// Pre-fix-3: the reader has no validation on its own snapshot, so the vec0
//   phase-1 SQL emits `AND attr_<hex>=?` for the dropped column → `no such
//   column` → `EngineError::Storage`.  <-- RED asserts this must NOT happen.
// Post-fix-3: `validate_filter_attributes_on_snapshot` runs INSIDE the reader's
//   deferred transaction, so the registry it reads and the vec0 columns the query
//   compiles against are ONE snapshot → a consistent, typed
//   `EngineError::InvalidFilter` naming the attribute.
// ===========================================================================

#[test]
fn undeclared_after_concurrent_drop_is_typed_invalidfilter_not_storage_race() {
    use fathomdb_engine::{
        arm_reader_search_hook_for_test, clear_reader_search_hook_for_test, EngineError,
    };
    use std::sync::mpsc;

    let (_dir, path) = fixture("s15e_fix3_toctou");
    let opened = open(&path);
    let engine = &opened.engine;

    // A declared `filterable` projection + a corpus that is BOTH vector-indexed
    // (HashEmbedder, drained) and FTS-indexed (shared token), so the filtered
    // `search_filtered` runs the vec0 arm that emits the `attr_<hex>=?` predicate.
    engine.configure_projections(&[filterable_spec("priority")], &[]).expect("configure");
    engine
        .write(&[
            node("A", r#"{"title":"sharedtoken alpha","priority":"high"}"#),
            node("B", r#"{"title":"sharedtoken beta","priority":"low"}"#),
        ])
        .expect("write");
    engine.drain(10_000).expect("drain");

    // Rendezvous channels: `reached` fires when the reader parks at the hook (before
    // its snapshot is pinned); `go` releases it after the concurrent drop commits.
    let (reached_tx, reached_rx) = mpsc::channel::<()>();
    let (go_tx, go_rx) = mpsc::channel::<()>();
    // `Sender::send` / `Receiver::recv` take `&self`, so an `Fn` closure can drive
    // both by shared reference — no need to move them out of the closure.
    arm_reader_search_hook_for_test(Box::new(move || {
        reached_tx.send(()).ok();
        go_rx.recv().ok();
    }));

    let result = std::thread::scope(|s| {
        let search = s.spawn(|| {
            let mut f = SearchFilter::default();
            f.attributes = vec![("priority".to_string(), "high".to_string())];
            engine.search_filtered("sharedtoken", Some(f))
        });

        // Wait for the reader to park (pre-dispatch validation has already passed on
        // the writer connection, and the reader has NOT yet pinned its snapshot).
        reached_rx.recv().expect("reader must reach the pre-snapshot hook");
        // The race: DROP `priority` on the writer connection NOW — this removes the
        // `attr_<hex>` vec0 column that the in-flight search's filter still names.
        engine
            .configure_projections(&[], &["priority".to_string()])
            .expect("concurrent DROP of the filterable projection");
        // Release the parked reader; it now pins a snapshot that includes the drop.
        go_tx.send(()).expect("release the parked reader");

        search.join().expect("search thread joined")
    });

    clear_reader_search_hook_for_test();

    match result {
        Err(EngineError::InvalidFilter { reason }) => assert!(
            reason.contains("priority"),
            "the reader-snapshot rejection must NAME the now-undeclared attribute, got: {reason}"
        ),
        Err(EngineError::Storage) => panic!(
            "TOCTOU: a configure_projections DROP racing the search made the vec0 attr_<hex> \
             column vanish AFTER the pre-dispatch check passed on the writer connection, so the \
             reader crashed with an opaque `no such column` Storage error. fix-3 must validate on \
             the reader's OWN transaction snapshot so this is a typed InvalidFilter."
        ),
        other => panic!(
            "expected a typed InvalidFilter after the concurrent drop (never a raw error / silent \
             Ok), got {other:?}"
        ),
    }

    engine.close().unwrap();
}
