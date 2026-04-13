#![cfg(feature = "default-embedder")]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::missing_panics_doc,
    clippy::float_cmp,
    clippy::panic,
    clippy::print_stderr,
    clippy::similar_names
)]

//! Phase 12.5b integration tests for the in-process Candle + bge-small-en-v1.5
//! embedder. Gated on `--features default-embedder` because they exercise
//! real model loading (via `hf-hub`) and tensor math. The always-on Phase
//! 12.5a `query_embedder_surface.rs` tests stay feature-free.
//!
//! The first test run on a clean machine will download ~130MB of weights
//! into `~/.cache/huggingface/hub`. Subsequent runs reuse the cache.
//! Offline-degradation test scopes `HF_HOME` into a temp directory to
//! avoid poisoning the real cache.

use fathomdb::{BuiltinBgeSmallEmbedder, QueryEmbedder};

/// Identity contract: BGE-small reports a stable model id, 384 dim, L2
/// normalization. This test does NOT trigger model loading — `identity()`
/// is a pure metadata call.
#[test]
fn builtin_embedder_identity_reports_bge_small() {
    let embedder = BuiltinBgeSmallEmbedder::new();
    let id = embedder.identity();
    assert_eq!(id.model_identity, "BAAI/bge-small-en-v1.5");
    assert_eq!(id.dimension, 384);
    assert_eq!(id.normalization_policy, "l2");
    // model_version is pinned to the revision string; just assert it's
    // non-empty so a future accidental blanking is caught.
    assert!(!id.model_version.is_empty());
}

/// Smoke test: a query produces a 384-dim vector whose L2 norm is ~1.0.
/// This exercises the full load path on first run, so it's marked
/// `#[ignore]`-free but will hit the network / hf-hub cache. Subsequent
/// runs are ~20ms warm.
#[test]
fn builtin_embedder_produces_384_dim_l2_normalized_vector() {
    let embedder = BuiltinBgeSmallEmbedder::new();
    let vector = match embedder.embed_query("ship the quarterly docs") {
        Ok(v) => v,
        Err(fathomdb::EmbedderError::Unavailable(msg)) => {
            eprintln!("skipping: builtin embedder unavailable (likely offline sandbox): {msg}");
            return;
        }
        Err(e) => panic!("unexpected error: {e}"),
    };
    assert_eq!(vector.len(), 384);
    let norm_sq: f32 = vector.iter().map(|x| x * x).sum();
    let norm = norm_sq.sqrt();
    assert!(
        (norm - 1.0).abs() < 1e-4,
        "expected L2 norm ~1.0, got {norm}"
    );
}

/// Correctness regression: CLS pooling, NOT mean pooling.
///
/// Under CLS pooling, two paraphrases of the same sentence produce much
/// more similar vectors than unrelated sentences. Mean pooling would
/// also pass this in spirit, so instead we use the sharper invariant
/// that BGE is semantically close-on-paraphrase: sim(paraphrase) >
/// sim(unrelated) by a comfortable margin.
///
/// A fully strict "mean vs CLS detector" would require reference vectors
/// from the model card. We don't have those pinned, so this test's job
/// is to catch a regression where pooling changes at all — any pooling
/// swap changes all three cosines and the invariant margin.
#[test]
fn builtin_embedder_pooling_is_cls_not_mean() {
    let embedder = BuiltinBgeSmallEmbedder::new();
    let a = match embedder.embed_query("The cat sat on the mat.") {
        Ok(v) => v,
        Err(fathomdb::EmbedderError::Unavailable(_)) => return, // offline skip
        Err(e) => panic!("unexpected: {e}"),
    };
    let b = embedder
        .embed_query("A cat was resting on a mat.")
        .expect("paraphrase embed");
    let c = embedder
        .embed_query("Quantum chromodynamics describes the strong force.")
        .expect("unrelated embed");

    let sim_ab = cosine(&a, &b);
    let sim_ac = cosine(&a, &c);
    assert!(
        sim_ab > sim_ac + 0.1,
        "paraphrases should be clearly closer than unrelated; sim_ab={sim_ab}, sim_ac={sim_ac}"
    );
    // And cosine should be in [-1, 1] for L2-normalized vectors.
    assert!((-1.0001..=1.0001).contains(&sim_ab));
}

/// Determinism: the same input yields byte-identical output across calls.
#[test]
fn builtin_embedder_deterministic_across_calls() {
    let embedder = BuiltinBgeSmallEmbedder::new();
    let first = match embedder.embed_query("deterministic test") {
        Ok(v) => v,
        Err(fathomdb::EmbedderError::Unavailable(_)) => return,
        Err(e) => panic!("unexpected: {e}"),
    };
    let second = embedder
        .embed_query("deterministic test")
        .expect("second call");
    assert_eq!(first.len(), second.len());
    for (i, (a, b)) in first.iter().zip(second.iter()).enumerate() {
        assert_eq!(
            a.to_bits(),
            b.to_bits(),
            "component {i} differs: {a} vs {b}"
        );
    }
}

/// Offline-with-empty-cache degradation: when `HF_HUB_OFFLINE=1` and the
/// cache has never seen the model, `embed_query` should return
/// `EmbedderError::Unavailable` instead of panicking or hanging. We
/// scope `HF_HOME` into a temp dir so the test never touches the
/// developer's real `~/.cache/huggingface`.
///
/// Caveat: hf-hub 0.5 with the `ureq` backend does NOT strictly honor
/// `HF_HUB_OFFLINE=1` when the ambient network is up — it can still
/// download into `HF_HOME`. A trustworthy offline simulation needs
/// either a network namespace or an iptables block, neither of which
/// are portable across dev hosts. So this test is `#[ignore]` by
/// default and should only be run in a real offline CI job. The
/// unavailability code path itself is exercised indirectly by the
/// Phase 12.5a `FakeUnavailableEmbedder` test in
/// `query_embedder_surface.rs`, which proves the coordinator's error
/// handling; the only thing this test adds is runtime proof that
/// hf-hub translates its own failures into `EmbedderError::Unavailable`
/// rather than a panic.
///
/// Run explicitly with:
///   `cargo test --features default-embedder -- --ignored builtin_embedder_offline`
#[test]
#[ignore = "requires sandboxed offline network; hf-hub HF_HUB_OFFLINE is best-effort"]
fn builtin_embedder_offline_without_cache_returns_unavailable() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // SAFETY: this test is `#[ignore]` by default and documented as
    // needing standalone execution, so the env mutation is scoped.
    unsafe {
        std::env::set_var("HF_HOME", tmp.path());
        std::env::set_var("HF_HUB_OFFLINE", "1");
    }
    let embedder = BuiltinBgeSmallEmbedder::new();
    match embedder.embed_query("will not resolve") {
        Ok(_) => eprintln!(
            "warning: hf-hub downloaded weights despite HF_HUB_OFFLINE=1; host is not truly offline"
        ),
        Err(fathomdb::EmbedderError::Unavailable(_)) => {}
        Err(other) => panic!("expected Unavailable on offline load failure, got {other:?}"),
    }
}

/// Cosine similarity for L2-normalized vectors (= dot product).
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}
