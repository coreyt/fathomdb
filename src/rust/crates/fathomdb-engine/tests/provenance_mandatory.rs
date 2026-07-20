//! 0.8.20 Slice 5c — provenance becomes structurally mandatory.
//!
//! Covers work items 5, 6, 7 and 10 of the Slice-0 erasure design v5 §4
//! (`dev/design/0.8.20-slice0-erasure-design.md`), i.e. requirements
//! **R-20-E2** (ingest provenance from the CALLER, never the model's echo),
//! **R-20-E3** (provenance is structurally mandatory on public writes; engine-
//! derived rows take a reserved `_engine:*` provenance) and **R-20-E8** (legacy
//! NULL-provenance rows become erasable — but ONLY the ungoverned ones).
//!
//! **Test-design contract (design §3).**
//!
//! * **Rule 1** — every erasure/provenance witness asserts on RAW TABLE
//!   CONTENTS via direct SQL. A test that proves anything about erasure by
//!   issuing a `search()` is invalid: both `search_index_v2` read paths gate on
//!   a live `canonical_nodes` row, so such a test passes on broken code.
//! * **TC-11 pin (CLOSED — must not be re-opened).** The legacy backfill is
//!   gated `WHERE logical_id IS NULL` ONLY. Governed rows (non-NULL
//!   `logical_id`) keep NULL `source_id` and stay `purge`-addressable BY
//!   `logical_id`. No migration, backfill or verb may ever populate
//!   `logical_id` on an existing canonical row, and a stored row's id-space is
//!   never re-derived.

use std::path::PathBuf;

use fathomdb_engine::{Engine, ExtractDocument, PreparedWrite, RowKind};
use fathomdb_schema::SQLITE_SUFFIX;
use rusqlite::{params, Connection};
use tempfile::TempDir;

/// The reserved provenance the step-21 migration stamps onto legacy
/// **ungoverned** rows (R-20-E8).
const LEGACY_SOURCE_ID: &str = "_legacy:pre-0.8.20";

fn db_path(dir: &TempDir, name: &str) -> PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |row| row.get::<_, i64>(0)).expect("count query")
}

fn fixture_dir() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/slice15_byo_llm")
}

// ---------------------------------------------------------------------------
// Item 6 (R-20-E3) — no stored canonical row carries NULL provenance
// ---------------------------------------------------------------------------

/// **Item 6.** Engine-derived rows bypass `PreparedWrite` entirely — the EXP-S
/// substrate writer inserts `canonical_nodes` directly — so a fix confined to
/// the public write path leaves NULL-provenance rows on disk. Such a row is
/// unreachable by `excise_source` and therefore un-erasable.
///
/// Asserted over **stored rows** (raw SQL), not the write path, per the design's
/// standing note on R-20-SUR: a write-path-only assertion cannot catch a row
/// that some other writer put on disk.
#[test]
fn no_canonical_row_has_null_source_id() {
    let dir = TempDir::new().expect("tempdir");
    let path = db_path(&dir, "null_provenance");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    let engine = &opened.engine;

    // A normal, fully provenanced public write.
    engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "public node body".to_string(),
            source_id: fathomdb_engine::SourceId::new("doc-public").expect("test source id"),
            logical_id: None,
            state: fathomdb_engine::InitialState::Active,
            reason: None,
            valid_from: None,
            valid_until: None,
        }])
        .expect("public node write");
    engine
        .write(&[PreparedWrite::Edge {
            kind: "mentions".to_string(),
            from: "a".to_string(),
            to: "b".to_string(),
            source_id: fathomdb_engine::SourceId::new("doc-public").expect("test source id"),
            logical_id: Some("edge-public".to_string()),
            body: Some("public edge body".to_string()),
            t_valid: None,
            t_invalid: None,
            confidence: None,
            extractor_model_id: None,
            temporal_fallback: None,
        }])
        .expect("public edge write");

    // The engine-derived path: EXP-S coverage/graph substrate rows. These do NOT
    // go through `PreparedWrite` and are the reason item 6 exists.
    engine
        .write_canonical_row_with_kind_for_test("coverage", "coverage row body", RowKind::Coverage)
        .expect("engine-derived coverage row");
    engine
        .write_canonical_row_with_kind_for_test("graph", "graph row body", RowKind::Graph)
        .expect("engine-derived graph row");

    let conn = Connection::open(&path).expect("open raw");

    let null_nodes = count(&conn, "SELECT count(*) FROM canonical_nodes WHERE source_id IS NULL");
    let null_edges = count(&conn, "SELECT count(*) FROM canonical_edges WHERE source_id IS NULL");

    assert_eq!(
        null_nodes, 0,
        "every stored canonical_nodes row must carry provenance; \
         a NULL source_id row is unreachable by excise_source and therefore un-erasable"
    );
    assert_eq!(
        null_edges, 0,
        "every stored canonical_edges row must carry provenance; \
         a NULL source_id row is unreachable by excise_source and therefore un-erasable"
    );

    // Item 6 is specific about WHICH provenance an engine-derived row takes: the
    // reserved `_engine:*` namespace, so operators can tell engine substrate from
    // caller data at a glance.
    let engine_rows = count(
        &conn,
        "SELECT count(*) FROM canonical_nodes WHERE kind IN ('coverage', 'graph') \
         AND source_id LIKE '\\_engine:%' ESCAPE '\\'",
    );
    assert_eq!(engine_rows, 2, "engine-derived rows must take a reserved `_engine:*` provenance");
}

// ---------------------------------------------------------------------------
// Item 7 (R-20-E8) — the legacy backfill is gated, and erasing `_legacy:` is safe
// ---------------------------------------------------------------------------

/// Build a database at the PRE-0.8.20 head (schema step 20) holding two
/// NULL-provenance canonical nodes: one ungoverned (`logical_id IS NULL`) and
/// one governed (`logical_id` set). Returns the db path inside `dir`.
fn seed_pre_0_8_20_database(dir: &TempDir, name: &str) -> PathBuf {
    let path = db_path(dir, name);
    let conn = Connection::open(&path).expect("open raw");

    // Migrate only as far as the pre-slice head so the step-21 backfill has real
    // legacy rows to act on when the engine later opens this file.
    let steps: Vec<fathomdb_schema::Migration> =
        fathomdb_schema::MIGRATIONS.iter().filter(|m| m.step_id <= 20).cloned().collect();
    fathomdb_schema::migrate_with_steps(&conn, &steps).expect("migrate to step 20");

    conn.execute(
        "INSERT INTO canonical_nodes(write_cursor, kind, body, source_id, logical_id)
         VALUES(1, 'doc', 'ungoverned legacy body', NULL, NULL)",
        params![],
    )
    .expect("insert ungoverned legacy node");
    conn.execute(
        "INSERT INTO canonical_nodes(write_cursor, kind, body, source_id, logical_id)
         VALUES(2, 'doc', 'governed legacy body', NULL, 'governed-lid-1')",
        params![],
    )
    .expect("insert governed legacy node");

    drop(conn);
    path
}

/// **Item 7 / R-20-E8 + TC-11 pin.** `excise_source('_legacy:pre-0.8.20')` must
/// remove every ungoverned legacy row and **NO** governed row. The governed row
/// keeps its NULL `source_id` and stays `purge`-addressable by `logical_id`.
///
/// Note this test is NOT vacuous in the "governed row survives" direction alone
/// — at the pre-slice baseline no row carries `_legacy:` provenance at all, so
/// the survival half passes trivially. The load-bearing half is that the
/// ungoverned row IS erased, which requires the migration to have run.
#[test]
fn excise_legacy_source_deletes_no_governed_row() {
    let dir = TempDir::new().expect("tempdir");
    let path = seed_pre_0_8_20_database(&dir, "legacy_excise");

    let opened = Engine::open_without_embedder_for_test(&path).expect("open migrates to head");
    let engine = &opened.engine;

    let report = engine.excise_source(LEGACY_SOURCE_ID).expect("excise legacy source");
    assert!(
        report.nodes_excised >= 1,
        "the backfilled ungoverned legacy row must be excisable by `{LEGACY_SOURCE_ID}`; \
         removed {} node(s)",
        report.nodes_excised
    );

    drop(opened);
    let conn = Connection::open(&path).expect("open raw");

    let ungoverned =
        count(&conn, "SELECT count(*) FROM canonical_nodes WHERE body = 'ungoverned legacy body'");
    assert_eq!(ungoverned, 0, "the ungoverned legacy row must be gone after excise_source");

    let governed =
        count(&conn, "SELECT count(*) FROM canonical_nodes WHERE body = 'governed legacy body'");
    assert_eq!(
        governed, 1,
        "TC-11 pin: excise_source('{LEGACY_SOURCE_ID}') must delete NO governed row"
    );

    let governed_source: Option<String> = conn
        .query_row(
            "SELECT source_id FROM canonical_nodes WHERE body = 'governed legacy body'",
            [],
            |row| row.get(0),
        )
        .expect("governed row still present");
    assert_eq!(
        governed_source, None,
        "TC-11 pin: a governed legacy row keeps NULL source_id and stays purge-addressable \
         by logical_id"
    );
}

// ---------------------------------------------------------------------------
// Item 10 (R-20-E2) — ingest provenance comes from the CALLER
// ---------------------------------------------------------------------------

/// **Item 10.** `ingest_with_extractor` took each row's `source_id` from the
/// model's JSON echo of `source_doc_id`. An LLM that omits (or alters) that
/// field therefore produced rows with NULL (or attacker-chosen) provenance —
/// rows `excise_source` cannot reach. Provenance must come from the caller's
/// `ExtractDocument.source_doc_id`, which the caller already supplied and which
/// the model has no say over.
///
/// The fixture harness returns a well-formed extraction result with the
/// `source_doc_id` field **omitted everywhere**.
#[test]
fn extractor_omitting_source_doc_id_still_excisable() {
    let script = fixture_dir().join("provenance_omitting_harness.py");
    assert!(script.exists(), "fixture harness must exist at {}", script.display());
    let cmd_strings = ["python3".to_string(), script.to_string_lossy().to_string()];
    let cmd_refs: Vec<&str> = cmd_strings.iter().map(|s| s.as_str()).collect();

    let dir = TempDir::new().expect("tempdir");
    let path = db_path(&dir, "ingest_provenance");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    let engine = &opened.engine;

    let docs = vec![ExtractDocument {
        source_doc_id: "caller-doc-1".to_string(),
        body: "Alice owns the project".to_string(),
    }];

    let receipt = engine.ingest_with_extractor(&cmd_refs, &docs).expect("ingest must succeed");
    assert!(receipt.nodes_written > 0, "fixture must produce entity rows");
    assert!(receipt.edges_written > 0, "fixture must produce edge rows");

    {
        let conn = Connection::open(&path).expect("open raw");
        let bad_nodes = count(
            &conn,
            "SELECT count(*) FROM canonical_nodes WHERE source_id IS NOT 'caller-doc-1'",
        );
        assert_eq!(
            bad_nodes, 0,
            "every ingested node must carry the CALLER's source_doc_id, not the model's echo"
        );
        let bad_edges = count(
            &conn,
            "SELECT count(*) FROM canonical_edges WHERE source_id IS NOT 'caller-doc-1'",
        );
        assert_eq!(
            bad_edges, 0,
            "every ingested edge must carry the CALLER's source_doc_id, not the model's echo"
        );
    }

    // The point of the provenance: the rows are erasable by the caller's id.
    let report = engine.excise_source("caller-doc-1").expect("excise");
    assert!(
        report.nodes_excised > 0 && report.edges_excised > 0,
        "rows ingested from a source_doc_id-omitting extractor must remain excisable \
         by the caller's document id (removed {} nodes / {} edges)",
        report.nodes_excised,
        report.edges_excised
    );

    drop(opened);
    let conn = Connection::open(&path).expect("open raw");
    assert_eq!(count(&conn, "SELECT count(*) FROM canonical_nodes"), 0);
    assert_eq!(count(&conn, "SELECT count(*) FROM canonical_edges"), 0);
}
