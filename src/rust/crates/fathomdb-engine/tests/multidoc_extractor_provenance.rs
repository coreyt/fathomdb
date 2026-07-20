//! 0.8.20 Slice 5 fix-4 — R-20-E2, the **multi-document** arm.
//!
//! Slice 5 made ingest provenance caller-grounded: `resolve_provenance`
//! (`fathomdb-engine/src/lib.rs`) admits the model's `source_doc_id` echo only
//! as a SELECTOR among ids the caller supplied in the same batch, never as a
//! value. Coverage for that change was **single-document only**
//! (`provenance_mandatory::extractor_omitting_source_doc_id_still_excisable`),
//! and a single-document batch is precisely the case where the echo is ignored
//! outright — so nothing exercised the branch that actually demands an echo.
//!
//! That gap shipped a regression: every MULTI-document extractor batch failed
//! with `EngineError::Extractor`, because the extractor protocol never required
//! per-entity attribution and the shipped ELPS harness backfilled
//! `source_doc_id` onto edges only. This file closes the gap from both sides:
//!
//! * the engine's demand is correct and must keep failing loudly
//!   (`multidoc_batch_*_fails_loudly`);
//! * the shipped harness must satisfy that demand
//!   (`elps_live_harness_multidoc_batch_ingests`).
//!
//! **Test-design contract (design §3, Rule 1).** Every provenance witness here
//! asserts on RAW TABLE CONTENTS via direct SQL. Proving provenance through
//! `search()` is invalid — both `search_index_v2` read paths gate on a live
//! `canonical_nodes` row, so such a test passes on broken code.

use std::path::PathBuf;

use fathomdb_engine::{Engine, EngineError, ExtractDocument};
use fathomdb_schema::SQLITE_SUFFIX;
use rusqlite::{Connection, OptionalExtension};
use tempfile::TempDir;

fn db_path(dir: &TempDir, name: &str) -> PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |row| row.get::<_, i64>(0)).expect("count query")
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture_dir() -> PathBuf {
    manifest_dir().join("tests/fixtures/slice15_byo_llm")
}

/// Absolute path to the SHIPPED ELPS harness. `CARGO_MANIFEST_DIR` is
/// `<repo>/src/rust/crates/fathomdb-engine`, so the repo root is four levels up.
fn shipped_elps_harness() -> PathBuf {
    manifest_dir()
        .ancestors()
        .nth(4)
        .expect("repo root is four levels above the engine crate")
        .join("src/python/eval/elps_live_harness.py")
}

/// A two-document batch. Both ids are caller-supplied; neither is derived from
/// anything the extractor says.
fn two_docs() -> Vec<ExtractDocument> {
    vec![
        ExtractDocument {
            source_doc_id: "caller-doc-1".to_string(),
            body: "Alice owns the project".to_string(),
        },
        ExtractDocument {
            source_doc_id: "caller-doc-2".to_string(),
            body: "Bob reviews the project".to_string(),
        },
    ]
}

fn cmd_of(parts: &[String]) -> Vec<&str> {
    parts.iter().map(String::as_str).collect()
}

// ---------------------------------------------------------------------------
// The engine's demand — a multi-document batch REQUIRES per-entity attribution
// ---------------------------------------------------------------------------

/// **Guard (do not "fix" by relaxing).** On a multi-document batch the engine
/// cannot know which document an entity came from unless the extractor says so.
/// Guessing — e.g. falling back to the batch's first id — would silently mis-file
/// the row under a document whose erasure then would NOT remove it, which is the
/// R-20-E2 defect this slice exists to close. So an absent echo must FAIL, and
/// fail as `EngineError::Extractor`.
///
/// Contrast with the single-document case, which is unambiguous and therefore
/// ignores the echo outright — asserted below so the two arms cannot drift.
#[test]
fn multidoc_batch_without_entity_echo_fails_loudly() {
    let script = fixture_dir().join("provenance_omitting_harness.py");
    assert!(script.exists(), "fixture harness must exist at {}", script.display());
    let parts = vec!["python3".to_string(), script.to_string_lossy().into_owned()];

    let dir = TempDir::new().expect("tempdir");
    let path = db_path(&dir, "multidoc_no_echo");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");

    let err = opened
        .engine
        .ingest_with_extractor(&cmd_of(&parts), &two_docs())
        .expect_err("a multi-doc batch whose entities carry no source_doc_id echo must FAIL");
    assert!(
        matches!(err, EngineError::Extractor),
        "an unattributable entity is a protocol violation, not a silent default; got {err:?}"
    );

    // And the failure is total: no half-attributed row reached the tables.
    drop(opened);
    let conn = Connection::open(&path).expect("open raw");
    assert_eq!(
        count(&conn, "SELECT count(*) FROM canonical_nodes"),
        0,
        "a rejected batch must leave no node rows behind"
    );
}

/// The **same** extractor, the **same** missing echo — but a single-document
/// batch. Attribution is unambiguous, so the caller's id is used and the ingest
/// succeeds. This is what made the regression invisible: all pre-existing
/// coverage lived on this side of the branch.
#[test]
fn singledoc_batch_without_entity_echo_still_succeeds() {
    let script = fixture_dir().join("provenance_omitting_harness.py");
    let parts = vec!["python3".to_string(), script.to_string_lossy().into_owned()];

    let dir = TempDir::new().expect("tempdir");
    let path = db_path(&dir, "singledoc_no_echo");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");

    let docs = vec![ExtractDocument {
        source_doc_id: "caller-doc-1".to_string(),
        body: "Alice owns the project".to_string(),
    }];
    let receipt = opened
        .engine
        .ingest_with_extractor(&cmd_of(&parts), &docs)
        .expect("a single-doc batch needs no echo: attribution is unambiguous");
    assert!(receipt.nodes_written > 0, "fixture must produce entity rows");

    drop(opened);
    let conn = Connection::open(&path).expect("open raw");
    assert_eq!(
        count(&conn, "SELECT count(*) FROM canonical_nodes WHERE source_id IS NOT 'caller-doc-1'"),
        0,
        "every row takes the CALLER's id"
    );
}

/// **The echo is a selector, never a value.** A model that names a document
/// outside the batch must be rejected outright. If the engine ever stored this
/// string, the row would carry provenance the caller never supplied and cannot
/// erase — a model could make a row permanently un-erasable by inventing an id.
#[test]
fn multidoc_batch_with_foreign_echo_fails_loudly() {
    let script = fixture_dir().join("multidoc_echo_harness.py");
    assert!(script.exists(), "fixture harness must exist at {}", script.display());
    let parts =
        vec!["python3".to_string(), script.to_string_lossy().into_owned(), "foreign".to_string()];

    let dir = TempDir::new().expect("tempdir");
    let path = db_path(&dir, "multidoc_foreign_echo");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");

    let err = opened
        .engine
        .ingest_with_extractor(&cmd_of(&parts), &two_docs())
        .expect_err("an echo naming a document outside the batch must FAIL");
    assert!(matches!(err, EngineError::Extractor), "expected EngineError::Extractor, got {err:?}");

    drop(opened);
    let conn = Connection::open(&path).expect("open raw");
    assert_eq!(
        count(
            &conn,
            "SELECT count(*) FROM canonical_nodes WHERE source_id = 'doc-not-in-this-batch'"
        ),
        0,
        "the model's invented id must never reach a stored row"
    );
    assert_eq!(
        count(&conn, "SELECT count(*) FROM canonical_nodes"),
        0,
        "a rejected batch must leave no node rows behind"
    );
}

/// A valid echo SELECTS among the caller's ids; the stored string is then the
/// caller's own copy. Asserted per-row against raw tables: each entity lands
/// under the document it was attributed to, and every stored `source_id` is one
/// of the two ids the caller supplied.
#[test]
fn multidoc_batch_with_valid_echo_stores_caller_copy() {
    let script = fixture_dir().join("multidoc_echo_harness.py");
    let parts =
        vec!["python3".to_string(), script.to_string_lossy().into_owned(), "valid".to_string()];

    let dir = TempDir::new().expect("tempdir");
    let path = db_path(&dir, "multidoc_valid_echo");
    // Full `Engine::open` (as `erasure_completeness` does): `erase_source`
    // drains the projection runtime, which never reaches idle on an
    // embedder-less test engine and would fail with `EngineError::Scheduler`.
    let opened = Engine::open(&path).expect("open");

    let receipt = opened
        .engine
        .ingest_with_extractor(&cmd_of(&parts), &two_docs())
        .expect("a multi-doc batch with per-entity attribution must succeed");
    assert!(receipt.nodes_written >= 2, "one entity per document: {receipt:?}");

    {
        let conn = Connection::open(&path).expect("open raw");
        // Every stored provenance is one the CALLER supplied — nothing else.
        assert_eq!(
            count(
                &conn,
                "SELECT count(*) FROM canonical_nodes \
                 WHERE source_id NOT IN ('caller-doc-1', 'caller-doc-2')",
            ),
            0,
            "no stored node may carry provenance outside the caller's batch"
        );
        // …and each row landed under the document it was attributed to.
        for (name, expected) in
            [("Entity-caller-doc-1", "caller-doc-1"), ("Entity-caller-doc-2", "caller-doc-2")]
        {
            let got: Option<String> = conn
                .query_row("SELECT source_id FROM canonical_nodes WHERE body = ?1", [name], |row| {
                    row.get(0)
                })
                .optional()
                .expect("query")
                .unwrap_or_else(|| panic!("expected a stored row for {name}"));
            assert_eq!(
                got.as_deref(),
                Some(expected),
                "{name} must be filed under the document it was attributed to"
            );
        }
    }

    // The consequence that matters: erasing ONE document removes only its rows.
    // Let the embed/projection work for the ingested rows settle first, as
    // `erasure_completeness` does: `erase_source` drains internally on a short
    // budget and reports `EngineError::Scheduler` if in-flight work outlasts it.
    opened.engine.drain(120_000).expect("drain");

    // `erase_source` (the governed SDK spelling) — not the operator-gated
    // `excise_source`: a caller document id is exactly what the governed verb
    // is for, so this also witnesses that the SDK surface can reach these rows.
    let report = opened.engine.erase_source("caller-doc-1").expect("erase");
    assert!(report.nodes_excised > 0, "doc-1's rows must be excisable by the caller's id");

    drop(opened);
    let conn = Connection::open(&path).expect("open raw");
    assert_eq!(
        count(&conn, "SELECT count(*) FROM canonical_nodes WHERE source_id = 'caller-doc-1'"),
        0,
        "every caller-doc-1 row is gone"
    );
    assert!(
        count(&conn, "SELECT count(*) FROM canonical_nodes WHERE source_id = 'caller-doc-2'") > 0,
        "caller-doc-2's rows must survive an unrelated document's erasure"
    );
}

// ---------------------------------------------------------------------------
// The contract's other side — the SHIPPED harness must satisfy that demand
// ---------------------------------------------------------------------------

/// **Regression witness (RED before the harness fix).** Drives the real
/// `src/python/eval/elps_live_harness.py` in stub mode over a two-document
/// batch — the exact shape an independent A/B found broken on this branch:
/// `_fathomdb.ExtractorError: extractor error`, where `origin/main` ingested
/// fine.
///
/// The harness backfilled `source_doc_id` onto edges only, never onto entities,
/// so the entity loop hit `resolve_provenance(None)` on a multi-doc batch. The
/// fix is on the CONTRACT side — the harness now backfills entities exactly as
/// it already did edges — not on the engine side; the engine's demand above
/// stays as strict as it was.
///
/// Uses a shim to force `ELPS_STUB_MODE=1` before import, so this needs no
/// network, no LLM and no third-party Python packages.
#[test]
fn elps_live_harness_multidoc_batch_ingests() {
    let shim = fixture_dir().join("elps_stub_shim.py");
    let harness = shipped_elps_harness();
    assert!(shim.exists(), "shim must exist at {}", shim.display());
    assert!(harness.exists(), "shipped ELPS harness must exist at {}", harness.display());

    let parts = vec![
        "python3".to_string(),
        shim.to_string_lossy().into_owned(),
        harness.to_string_lossy().into_owned(),
    ];

    let dir = TempDir::new().expect("tempdir");
    let path = db_path(&dir, "elps_multidoc");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");

    let receipt = opened.engine.ingest_with_extractor(&cmd_of(&parts), &two_docs()).expect(
        "the shipped ELPS harness must attribute every entity it emits, so that a \
         multi-document batch ingests instead of failing with EngineError::Extractor",
    );
    assert!(receipt.nodes_written > 0, "the stub harness must produce entity rows: {receipt:?}");

    drop(opened);
    let conn = Connection::open(&path).expect("open raw");
    assert_eq!(
        count(
            &conn,
            "SELECT count(*) FROM canonical_nodes \
             WHERE source_id NOT IN ('caller-doc-1', 'caller-doc-2')",
        ),
        0,
        "every row the harness produced must be filed under a caller-supplied document id"
    );
}
