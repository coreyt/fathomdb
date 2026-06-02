//! 0.8.0 Slice 5 / G1 — global FTS5 tokenizer-default upgrade recall floor.
//!
//! AC-FTS-tokenizer-floor. The recall floor (>= 0.90) MUST hold on a DB
//! **migrated from the prior `SCHEMA_VERSION` (10)** — not merely on a fresh
//! DB. This pins the no-op-on-existing-DB failure mode RED: if the tokenizer
//! upgrade only affects fresh DBs (i.e. re-tokenization is not wired after the
//! drop+recreate migration), the migrated DB's `search_index` is empty and
//! recall collapses to 0.
//!
//! Deterministic, in-suite (runs every `cargo test` — no network, no
//! `AGENT_LONG`). No mocking of the database: a real engine is opened against a
//! real on-disk SQLite file at each schema version.

use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::{Migration, MIGRATIONS, SQLITE_SUFFIX};
use tempfile::TempDir;

/// The schema as it stood at `SCHEMA_VERSION = 10` (before this slice's step
/// 11). Slicing the canonical registry keeps this in lockstep with the real
/// steps 1..=10 rather than re-transcribing them.
const V10_MIGRATIONS: &[Migration] = {
    // 10 steps with ids 1..=10 occupy the first 10 entries; step 11 (this
    // slice) is the last. Take everything but the final step.
    let (head, _tail) = MIGRATIONS.split_at(MIGRATIONS.len() - 1);
    head
};

/// A small, deterministic corpus with known query -> relevant-body pairs.
/// Each query term appears verbatim in exactly its relevant doc's body; the
/// porter/diacritics tokenizer must still match the bare term.
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

/// (query, expected substring of the single relevant body).
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

fn ingest(engine: &Engine) {
    for doc in CORPUS {
        engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: doc.body.to_string(),
                source_id: None,
            }])
            .expect("write corpus doc");
    }
    engine.drain(30_000).expect("drain corpus");
}

/// Fraction of queries whose relevant body is present in the result set.
fn measure_recall(engine: &Engine) -> f64 {
    let mut hits = 0usize;
    for (query, relevant) in QUERIES {
        let result = engine.search(query).expect("recall search");
        let found = result.results.iter().any(|h| h.body == *relevant);
        if found {
            hits += 1;
        }
    }
    hits as f64 / QUERIES.len() as f64
}

const FLOOR: f64 = 0.90;

#[test]
fn ac_fts_tokenizer_floor_holds_across_migration() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("tok_recall{SQLITE_SUFFIX}"));

    // --- Phase A: build a v10 (unicode61) DB and measure the BEFORE floor. ---
    let before_recall = {
        let opened =
            Engine::open_with_migrations_for_test(&path, V10_MIGRATIONS, |_| {}).expect("open v10");
        // Confirm we really are at the prior schema version.
        assert_eq!(
            opened.report.schema_version_after, 10,
            "phase A must open at SCHEMA_VERSION 10"
        );
        ingest(&opened.engine);
        let r = measure_recall(&opened.engine);
        opened.engine.close().unwrap();
        r
    };
    eprintln!("[pr_g1_tokenizer_recall] BEFORE (v10 unicode61) recall = {before_recall:.3}");
    assert!(
        before_recall >= FLOOR,
        "BEFORE-migration recall {before_recall:.3} is below the {FLOOR} floor"
    );

    // --- Phase B: re-open with the FULL migration set so step 11 runs, then
    // measure the AFTER floor on the SAME on-disk corpus. If re-tokenization
    // is not wired, search_index was dropped+recreated empty and recall is 0. ---
    let after_recall = {
        let opened =
            Engine::open_with_migrations_for_test(&path, MIGRATIONS, |_| {}).expect("open v11");
        assert_eq!(
            opened.report.schema_version_after, 11,
            "phase B must migrate to SCHEMA_VERSION 11"
        );
        assert!(
            opened.report.schema_version_before == 10,
            "phase B must observe a 10 -> 11 migration, saw before={}",
            opened.report.schema_version_before
        );
        let r = measure_recall(&opened.engine);
        opened.engine.close().unwrap();
        r
    };

    eprintln!(
        "[pr_g1_tokenizer_recall] AFTER (v11 porter unicode61 remove_diacritics) recall = \
         {after_recall:.3} (delta {:+.3})",
        after_recall - before_recall
    );
    assert!(
        after_recall >= FLOOR,
        "AFTER-migration recall {after_recall:.3} is below the {FLOOR} floor \
         (before={before_recall:.3}); the tokenizer drop+recreate left the FTS \
         index unpopulated on the migrated DB"
    );
}
