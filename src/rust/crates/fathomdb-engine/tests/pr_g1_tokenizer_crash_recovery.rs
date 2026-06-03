//! 0.8.0 Slice 5 / G1 fix-1 — crash-safety of the FTS5 tokenizer reproject.
//!
//! The step-11 migration drops + recreates `search_index` (empty) and durably
//! commits `user_version = 11` in ONE transaction; the reproject that
//! repopulates the FTS shadow runs in a SEPARATE later transaction on open.
//! A crash (or a reproject error) AFTER step 11 commits but BEFORE the
//! reproject commits leaves a durable state of `user_version = 11` with an
//! EMPTY `search_index`. A boundary-crossing guard (`before < 11 && after >=
//! 11`) is FALSE on the next open (it sees `before == 11`), so the reproject
//! is skipped and the index stays empty FOREVER — recall silently collapses.
//!
//! This test reconstructs that exact durable crash artifact by manipulating
//! the real on-disk DB with a raw `rusqlite::Connection` (NOT a mock), then
//! re-opens the engine and asserts the index is repaired and recall recovers.

use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::{MIGRATIONS, SQLITE_SUFFIX};
use rusqlite::Connection;
use tempfile::TempDir;

/// Completion marker key the GREEN fix writes inside the reproject tx. The
/// test clears it to faithfully reproduce the pre-commit crash state (a crash
/// before the reproject commit leaves no marker AND an empty index).
const REPROJECT_MARKER_KEY: &str = "search_index_tokenizer_reproject_complete";

struct Doc {
    body: &'static str,
}

const CORPUS: &[Doc] = &[
    Doc { body: "migration tokenizer recall validation corpus" },
    Doc { body: "structured search hits carry score and branch" },
    Doc { body: "forward only schema migrations are immutable" },
    Doc { body: "porter unicode diacritics tokenizer upgrade" },
    Doc { body: "canonical nodes project into the fts index" },
    Doc { body: "vector branch reranks with euclidean distance" },
    Doc { body: "bm twentyfive scores the text retrieval branch" },
    Doc { body: "deduplicate on body keep vector ordering first" },
    Doc { body: "write cursor is the interim identity carrier" },
    Doc { body: "recall floor must hold across the migration boundary" },
];

const QUERIES: &[(&str, &str)] = &[
    ("tokenizer", "migration tokenizer recall validation corpus"),
    ("structured", "structured search hits carry score and branch"),
    ("immutable", "forward only schema migrations are immutable"),
    ("diacritics", "porter unicode diacritics tokenizer upgrade"),
    ("canonical", "canonical nodes project into the fts index"),
    ("euclidean", "vector branch reranks with euclidean distance"),
    ("twentyfive", "bm twentyfive scores the text retrieval branch"),
    ("deduplicate", "deduplicate on body keep vector ordering first"),
    ("interim", "write cursor is the interim identity carrier"),
    ("boundary", "recall floor must hold across the migration boundary"),
];

const FLOOR: f64 = 0.90;

fn ingest(engine: &Engine) {
    for doc in CORPUS {
        engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: doc.body.to_string(),
                source_id: None,
                logical_id: None,
            }])
            .expect("write corpus doc");
    }
    engine.drain(30_000).expect("drain corpus");
}

fn measure_recall(engine: &Engine) -> f64 {
    let mut hits = 0usize;
    for (query, relevant) in QUERIES {
        let result = engine.search(query).expect("recall search");
        if result.results.iter().any(|h| h.body == *relevant) {
            hits += 1;
        }
    }
    hits as f64 / QUERIES.len() as f64
}

#[test]
fn ac_fts_tokenizer_reproject_recovers_after_crash() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("tok_crash{SQLITE_SUFFIX}"));

    // --- Build a fully-migrated (head) corpus the normal way. The reproject
    // ran on this open, so recall is healthy. (Head is SCHEMA_VERSION 12 after
    // the Slice-15 G0 substrate step; the tokenizer reproject is gated on its
    // completion marker, not the step boundary, so this test is unchanged in
    // intent — only the head version literal moves 11 → 12.) ---
    {
        let opened = Engine::open(&path).expect("open head");
        assert_eq!(opened.report.schema_version_after, 12, "must open at SCHEMA_VERSION 12");
        ingest(&opened.engine);
        let recall = measure_recall(&opened.engine);
        assert!(recall >= FLOOR, "baseline v11 recall {recall:.3} below floor");
        opened.engine.close().unwrap();
    }

    // --- Reconstruct the durable post-crash artifact directly on the DB file:
    // empty `search_index` while `user_version` stays at head, and (if the fix
    // uses a completion marker) the marker cleared — exactly what a crash after
    // the tokenizer-step commit but before the reproject commit leaves behind.
    // No mock: this is the real SQLite file in the real crash-window state. ---
    {
        let raw = Connection::open(&path).expect("raw open");
        let user_version: u32 =
            raw.query_row("PRAGMA user_version", [], |r| r.get(0)).expect("user_version");
        assert_eq!(user_version, 12, "precondition: durable schema is head (v12)");

        raw.execute("DELETE FROM search_index", []).expect("simulate empty fts index");
        raw.execute("DELETE FROM _fathomdb_open_state WHERE key = ?1", [REPROJECT_MARKER_KEY])
            .expect("clear reproject completion marker");

        let fts_rows: u64 = raw
            .query_row("SELECT COUNT(*) FROM search_index", [], |r| r.get(0))
            .expect("count fts rows");
        assert_eq!(fts_rows, 0, "crash artifact must leave search_index empty");
        // user_version must remain at head — the crash did NOT roll back a step.
        let after: u32 =
            raw.query_row("PRAGMA user_version", [], |r| r.get(0)).expect("user_version");
        assert_eq!(after, 12, "crash artifact must keep user_version = 12");
        drop(raw);
    }

    // --- Re-open normally. With a boundary-crossing guard this is a no-op
    // (before == 11), the empty index survives, and recall is 0 -> RED. The
    // fix must re-run the reproject (it sees an unfinished/absent marker) and
    // repopulate the index. ---
    {
        let opened = Engine::open_with_migrations_for_test(&path, MIGRATIONS, |_| {})
            .expect("reopen after crash artifact");
        assert_eq!(
            opened.report.schema_version_before, 12,
            "reopen observes a durable v12 head (no boundary crossing)"
        );
        let recall = measure_recall(&opened.engine);
        opened.engine.close().unwrap();
        assert!(
            recall >= FLOOR,
            "post-crash reopen recall {recall:.3} below the {FLOOR} floor — the \
             tokenizer reproject did not re-run on a durable-v11 DB with an empty \
             search_index (crash-window state), so the FTS shadow stayed empty"
        );
    }
}
