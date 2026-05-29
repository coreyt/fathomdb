//! EU-5a1 RED tests — plumbing-only first half of EU-5a.
//!
//! These tests assert the type-system plumbing for the EU-5 campaign:
//! the public `EmbedderChoice` enum exposed on `Engine.open`, the four
//! new `OpenReport` fields (per `dev/design/embedder.md` §0.6 + §7), and
//! the transitional `DefaultEmbedderNotWired` typed error path.
//!
//! See `dev/plans/prompts/0.7.1-EMBEDDER-UNDEFER-HANDOFF.md` §EU-5 step 2,
//! step 3, step 7.

use std::sync::Arc;

use fathomdb_embedder::NoopEmbedder;
use fathomdb_embedder_api::EmbedderError;
use fathomdb_engine::{EmbedderChoice, Engine, EngineError, EngineOpenError};
use tempfile::TempDir;

fn fixture_path(name: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("{name}.sqlite"));
    (dir, path)
}

#[test]
fn open_with_embedder_choice_default_returns_default_embedder_not_wired() {
    // EU-5a1: EmbedderChoice::Default is a deliberate, typed compile-time
    // hole until EU-5b lands CandleBgeEmbedder wiring + the identity
    // constant flip. NO noop fallback.
    let (_dir, path) = fixture_path("eu5a1_default");
    let err = Engine::open_with_choice(&path, EmbedderChoice::Default)
        .expect_err("EmbedderChoice::Default must error in EU-5a1");
    match err {
        EngineOpenError::Embedder(EmbedderError::DefaultEmbedderNotWired) => {}
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn open_with_embedder_choice_caller_succeeds_with_noop() {
    let (_dir, path) = fixture_path("eu5a1_caller_noop");
    let opened =
        Engine::open_with_choice(&path, EmbedderChoice::Caller(Arc::new(NoopEmbedder::default())))
            .expect("open with caller-supplied noop");
    assert_eq!(opened.report.default_embedder.name, "fathomdb-noop");
}

#[test]
fn open_with_embedder_choice_none_succeeds_but_vector_write_fails() {
    let (_dir, path) = fixture_path("eu5a1_none");
    let opened = Engine::open_with_choice(&path, EmbedderChoice::None)
        .expect("open with EmbedderChoice::None must succeed");
    opened.engine.configure_vector_kind_for_test("doc").expect("configure vector kind");
    let err = opened
        .engine
        .write_vector_for_test("doc", "hello")
        .expect_err("vector write must fail with EmbedderNotConfigured");
    assert_eq!(err, EngineError::EmbedderNotConfigured);
}

#[test]
fn open_report_embedder_mean_centering_required_false_for_noop() {
    // NoopEmbedder's identity ("fathomdb-noop") does NOT require
    // mean-centering. EU-5b will flip the default identity to bge-small,
    // at which point this becomes true for the Default choice — but the
    // capability for a caller-supplied NoopEmbedder remains false.
    let (_dir, path) = fixture_path("eu5a1_mc_required_noop");
    let opened =
        Engine::open_with_choice(&path, EmbedderChoice::Caller(Arc::new(NoopEmbedder::default())))
            .expect("open");
    assert!(!opened.report.embedder_mean_centering_required);
}

#[test]
fn open_report_embedder_mean_vec_pinned_false_in_eu5a1() {
    // In EU-5a1 the `_fathomdb_embedder_profiles.mean_vec` column does
    // not exist yet (EU-5a2 adds it via migration step 10). Until then
    // OpenReport.embedder_mean_vec_pinned is unconditionally false.
    // This assertion will be re-anchored in EU-5a2 to reflect actual
    // workspace state.
    let (_dir, path) = fixture_path("eu5a1_mc_pinned_noop");
    let opened =
        Engine::open_with_choice(&path, EmbedderChoice::Caller(Arc::new(NoopEmbedder::default())))
            .expect("open");
    assert!(!opened.report.embedder_mean_vec_pinned);
}

#[test]
fn open_report_embedder_download_ms_none_and_events_empty_for_caller_supplied() {
    // Caller-supplied embedders bypass the loader entirely, so the
    // download timing surface and the structured event stream are both
    // empty. EU-5b wires these for the Default path.
    let (_dir, path) = fixture_path("eu5a1_download_events");
    let opened =
        Engine::open_with_choice(&path, EmbedderChoice::Caller(Arc::new(NoopEmbedder::default())))
            .expect("open");
    assert!(opened.report.embedder_download_ms.is_none());
    assert!(opened.report.embedder_events.is_empty());
}
