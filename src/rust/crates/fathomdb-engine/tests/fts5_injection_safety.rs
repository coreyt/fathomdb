//! AC-038 FTS5-injection-safe text query.
//!
//! Asserts the two properties locked by `dev/acceptance.md` AC-038:
//!
//!   1. **Zero SQLITE_ERROR.** A query containing FTS5 control syntax
//!      submitted via [`Engine::search`] never raises a `SQLITE_ERROR`
//!      (malformed MATCH expression). The compile_text_query parser
//!      (see `src/rust/crates/fathomdb-query/src/lib.rs`) wraps every
//!      whitespace-split token in a literal FTS5 phrase, so the
//!      injection-shaped tokens reach SQLite as literal phrase
//!      content, not as operators.
//!
//!   2. **Result-set parity with the safe-grammar reference.** For
//!      each fixture query `q` and each corpus document body `b`,
//!      `b` appears in the engine result set iff the safe-grammar
//!      reference would also surface it. The reference (defined
//!      in [`safe_reference_matches`]) tokenizes `q` and `b` with
//!      FTS5 unicode61-equivalent semantics (lowercase, drop
//!      non-alphanumerics) and asserts every literal q-token appears
//!      in the body's token set.
//!
//! Fixture: 100 queries covering FTS5 control syntax — quotes,
//! wildcards, NEAR(), AND/OR/NOT, column filters, nested parens,
//! caret weights. Generated deterministically in
//! [`fixture_queries`].

use std::collections::HashSet;

use fathomdb_engine::{Engine, PreparedWrite};
use tempfile::TempDir;

const CORPUS: &[&str] = &[
    "alpha bravo charlie",
    "delta echo foxtrot",
    "golf hotel india juliet",
    "kilo lima mike november",
    "oscar papa quebec romeo",
    "sierra tango uniform victor",
    "whiskey xray yankee zulu",
    "alpha delta golf kilo",
    "bravo echo hotel lima",
    "charlie foxtrot india mike",
];

fn fts5_token_set(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect()
}

fn safe_reference_matches(query: &str, body: &str) -> bool {
    // Safe-grammar reference: strip FTS5 syntax to leave alphanumeric
    // tokens, then require every q-token to be present (as a tokenized
    // word, lowercased) in the body. Empty query never matches — same
    // as Engine::search WriteValidation path.
    let q_tokens: Vec<String> = query
        .split_whitespace()
        .map(|t| t.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    if q_tokens.is_empty() {
        return false;
    }
    let body_tokens = fts5_token_set(body);
    q_tokens.iter().all(|t| body_tokens.contains(t))
}

fn fixture_queries() -> Vec<String> {
    // 100 queries — 10 templates × 10 token-substitutions. The
    // alphabet is drawn from the corpus so a fraction of queries
    // intentionally hit non-empty result sets (proves parity is not
    // vacuously satisfied by all-empty matches).
    let tokens = [
        "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india", "juliet",
    ];
    let templates: &[fn(&str, &str) -> String] = &[
        |a, b| format!("\"{a} {b}\""),
        |a, _b| format!("{a}*"),
        |a, _b| format!("*{a}*"),
        |a, b| format!("{a} AND {b}"),
        |a, b| format!("{a} OR {b}"),
        |a, b| format!("{a} NOT {b}"),
        |a, b| format!("NEAR({a} {b}, 5)"),
        |a, b| format!("body:{a} (col:{b})"),
        |a, b| format!("\"{a}\" ^ 2 AND ({b} OR \"NEAR({a})\")"),
        |a, b| format!("{a}:\"{b}\" OR {b}^3"),
    ];
    let mut out = Vec::with_capacity(100);
    for (i, tpl) in templates.iter().enumerate() {
        for j in 0..10 {
            let a = tokens[(i + j) % tokens.len()];
            let b = tokens[(i * 3 + j * 2 + 1) % tokens.len()];
            out.push(tpl(a, b));
        }
    }
    assert_eq!(out.len(), 100, "AC-038 fixture must be exactly 100 queries");
    out
}

#[test]
fn fts5_injection_fixture_is_one_hundred_queries() {
    assert_eq!(fixture_queries().len(), 100);
}

#[test]
fn fts5_control_syntax_is_safe_and_matches_reference() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("fts5_injection.sqlite");
    let opened = Engine::open(&path).expect("open");
    let engine = opened.engine;

    let writes: Vec<PreparedWrite> = CORPUS
        .iter()
        .map(|body| PreparedWrite::Node {
            kind: "doc".to_string(),
            body: (*body).to_string(),
            source_id: None,
        })
        .collect();
    engine.write(&writes).expect("seed write");

    let queries = fixture_queries();
    let mut sqlite_errors: Vec<(String, String)> = Vec::new();
    let mut parity_failures: Vec<(String, Vec<String>, Vec<String>)> = Vec::new();

    for q in &queries {
        let outcome = engine.search(q);
        match outcome {
            Ok(result) => {
                // results[0] is the compiled SQL/MATCH expression; body
                // rows from canonical_nodes follow. Filter to corpus
                // bodies for the parity check.
                let body_set: HashSet<&str> = CORPUS.iter().copied().collect();
                let mut engine_bodies: Vec<String> = result
                    .results
                    .iter()
                    .filter(|s| body_set.contains(s.as_str()))
                    .cloned()
                    .collect();
                engine_bodies.sort();
                engine_bodies.dedup();

                let mut reference_bodies: Vec<String> = CORPUS
                    .iter()
                    .filter(|body| safe_reference_matches(q, body))
                    .map(|s| s.to_string())
                    .collect();
                reference_bodies.sort();

                if engine_bodies != reference_bodies {
                    parity_failures.push((q.clone(), engine_bodies, reference_bodies));
                }
            }
            Err(err) => {
                // Any error path is a candidate; the AC scope is
                // SQLITE_ERROR (malformed MATCH). EngineError::Storage
                // is the typed surface a SQLite parse failure becomes,
                // so treat it as the AC violation signal.
                sqlite_errors.push((q.clone(), format!("{err:?}")));
            }
        }
    }

    engine.close().expect("close");

    assert!(
        sqlite_errors.is_empty(),
        "AC-038: {} fixture queries raised an EngineError (likely SQLITE_ERROR from \
         malformed MATCH); first 3: {:?}",
        sqlite_errors.len(),
        sqlite_errors.iter().take(3).collect::<Vec<_>>(),
    );
    assert!(
        parity_failures.is_empty(),
        "AC-038: {} fixture queries diverged from safe-grammar reference; first 3: {:#?}",
        parity_failures.len(),
        parity_failures.iter().take(3).collect::<Vec<_>>(),
    );
}
