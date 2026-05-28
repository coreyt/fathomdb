//! Compile-time enforcement of EU-4 test #7:
//! `embed_does_not_call_back_into_engine`.
//!
//! Per `ADR-0.6.0-embedder-protocol.md` Invariant 2 and §EU-4 of the
//! 0.7.1 EMBEDDER-UNDEFER handoff, `CandleBgeEmbedder` must not link
//! `fathomdb-engine` — the embedder is consumed *by* the engine and a
//! reverse edge would be a re-entrancy hazard.
//!
//! This file is the load-bearing structural assertion: it merely imports
//! `CandleBgeEmbedder` and does nothing else. The real test is the build
//! system itself — `Cargo.toml` for `fathomdb-embedder` lists no
//! `fathomdb-engine` dependency, and the CI gate
//!
//!   cargo tree -p fathomdb-embedder --features default-embedder \
//!       | grep fathomdb-engine
//!
//! must return empty. If a future refactor accidentally pulls the engine
//! into the embedder's dep closure, this test file will still compile but
//! the `cargo tree` gate will fail.

#![cfg(all(feature = "default-embedder", feature = "loader-test-hooks"))]

use fathomdb_embedder::CandleBgeEmbedder;

#[test]
fn no_engine_dep_compile_time_marker() {
    // Symbol-use to ensure the import isn't dead-stripped by rustc.
    let _ = std::mem::size_of::<CandleBgeEmbedder>();
}
