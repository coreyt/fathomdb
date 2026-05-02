//! Asserts the public shape of `SearchResult` per `dev/interfaces/rust.md`
//! § Caller-visible data shapes.

use fathomdb_engine::{SearchResult, SoftFallback, SoftFallbackBranch};

#[test]
fn search_result_carries_optional_soft_fallback() {
    let r = SearchResult {
        projection_cursor: 0,
        soft_fallback: Some(SoftFallback { branch: SoftFallbackBranch::Text }),
        results: Vec::new(),
    };
    assert!(r.soft_fallback.is_some());
}

#[test]
fn search_result_default_has_no_soft_fallback() {
    let r = SearchResult { projection_cursor: 0, soft_fallback: None, results: Vec::new() };
    assert!(r.soft_fallback.is_none());
}
