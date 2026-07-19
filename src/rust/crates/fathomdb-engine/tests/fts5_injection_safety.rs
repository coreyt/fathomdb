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
//!      non-alphanumerics) and — mirroring the IR-C **content-OR**
//!      compiler (`fathomdb-query`) — asserts that ANY content
//!      q-token (≥3 chars, stopwords stripped) appears in the body's
//!      token set. The injection property is unchanged by AND→OR:
//!      every q-token still reaches SQLite as a literal phrase, so
//!      control syntax (`*`, `NEAR()`, `^`, `:`, quotes) never acts
//!      as an operator — that is exactly what parity proves.
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

// Stopwords + content-token extraction, mirroring the `fathomdb-query`
// content-OR compiler so the reference matches the engine's actual MATCH
// semantics (the injection property holds regardless of AND vs OR).
const STOPWORDS: &[&str] = &[
    "the", "and", "for", "are", "was", "were", "what", "when", "where", "who", "whom", "which",
    "how", "why", "did", "does", "do", "is", "of", "to", "in", "on", "at", "by", "an", "a", "it",
    "its", "this", "that", "these", "those", "with", "from", "as", "be", "or", "if", "about",
    "into", "over", "than", "then", "they", "them", "their", "you", "your", "we", "our", "i",
];

fn content_tokens(query: &str) -> Vec<String> {
    let stop: HashSet<&str> = STOPWORDS.iter().copied().collect();
    query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 3 && !stop.contains(t))
        .map(|t| t.to_string())
        .collect()
}

fn safe_reference_matches(query: &str, body: &str) -> bool {
    // Safe-grammar reference, mirroring the content-OR compiler: strip FTS5
    // syntax to leave alphanumeric content tokens (≥3 chars, stopwords removed),
    // then surface the body iff ANY content token is present (OR). All-stopword /
    // symbol-only queries fall back to any raw whitespace token — matching the
    // compiler's fallback. Empty query never matches.
    let content = content_tokens(query);
    let body_tokens = fts5_token_set(body);
    if !content.is_empty() {
        return content.iter().any(|t| body_tokens.contains(t));
    }
    let raw_tokens: Vec<String> = query
        .split_whitespace()
        .map(|t| t.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    if raw_tokens.is_empty() {
        return false;
    }
    raw_tokens.iter().any(|t| body_tokens.contains(t))
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
            source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
            logical_id: None,
            state: fathomdb_engine::InitialState::Active,
            reason: None,
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
                    .map(|h| h.body.clone())
                    .filter(|b| body_set.contains(b.as_str()))
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
