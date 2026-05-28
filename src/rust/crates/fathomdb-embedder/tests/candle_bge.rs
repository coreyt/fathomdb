//! Integration tests for `CandleBgeEmbedder` (EU-4).
//!
//! Per `dev/plans/prompts/0.7.1-EMBEDDER-UNDEFER-HANDOFF.md` §EU-4, this slice
//! ships seven required tests that drive the embedder contract:
//!
//! 1. `identity_returns_chosen_model_at_chosen_revision`
//! 2. `embed_returns_unit_norm_vector`
//! 3. `embed_returns_dimension_correct_vector`
//! 4. `embed_is_deterministic_for_same_input`
//! 5. `embed_two_similar_inputs_high_cosine`
//! 6. `embed_does_not_panic_on_empty_string`
//! 7. `embed_does_not_call_back_into_engine`  (compile-time invariant —
//!    see `tests/no_engine_dep.rs`)
//!
//! ## Network / weight-download note
//!
//! These tests construct a real `CandleBgeEmbedder` via the loader; on first
//! run that triggers `load_pinned_default_embedder()` which fetches three
//! files (~135 MB total) from `huggingface.co`. Subsequent runs hit the
//! local cache and are fast. There is no mock seam: the candle BertModel
//! requires real weights to produce semantically meaningful vectors, and
//! the §EU-4 sentinel-cosine assertion is only meaningful against the real
//! model.
//!
//! ## FP determinism
//!
//! Per `dev/design/embedder.md` §0.4, BERT forward on CPU with identical
//! inputs and identical loaded weights is bit-identical. We assert byte
//! equality on the f32 representation; if a future candle release breaks
//! this on a given platform, switch test 4 to an `f32::abs_diff <= 0` (same
//! semantics today, robustness room for tomorrow).

#![cfg(all(feature = "default-embedder", feature = "loader-test-hooks"))]

use fathomdb_embedder::loader::HF_REVISION as PINNED_REVISION;
use fathomdb_embedder::CandleBgeEmbedder;
use fathomdb_embedder_api::Embedder;

fn make_embedder() -> CandleBgeEmbedder {
    CandleBgeEmbedder::new().expect("CandleBgeEmbedder::new must succeed")
}

#[test]
fn identity_returns_chosen_model_at_chosen_revision() {
    let e = make_embedder();
    let id = e.identity();
    assert_eq!(id.name, "fathomdb-bge-small-en-v1.5");
    assert!(
        id.revision.contains(PINNED_REVISION),
        "revision {:?} must contain pinned snapshot sha {}",
        id.revision,
        PINNED_REVISION
    );
    assert_eq!(id.dimension, 384);
}

#[test]
fn embed_returns_unit_norm_vector() {
    let e = make_embedder();
    let inputs = [
        "the quick brown fox",
        "",
        "a longer paragraph with multiple sentences. Some commas, periods, etc.",
    ];
    for input in inputs {
        let v = e.embed(input).expect("embed must succeed");
        let n = v.iter().map(|x| (x as &f32) * x).sum::<f32>().sqrt();
        assert!((n - 1.0).abs() < 1e-5, "vector for {input:?} not unit-norm: ‖v‖ = {n}");
    }
}

#[test]
fn embed_returns_dimension_correct_vector() {
    let e = make_embedder();
    let dim = e.identity().dimension as usize;
    let v = e.embed("hello world").expect("embed must succeed");
    assert_eq!(v.len(), dim);
    assert_eq!(dim, 384);
}

#[test]
fn embed_is_deterministic_for_same_input() {
    // BERT forward on CPU with identical weights is bit-identical for the
    // same input. We assert byte equality on the f32 representation.
    let e = make_embedder();
    let a = e.embed("deterministic forward pass").unwrap();
    let b = e.embed("deterministic forward pass").unwrap();
    assert_eq!(a.len(), b.len());
    let a_bytes: Vec<[u8; 4]> = a.iter().map(|x| x.to_le_bytes()).collect();
    let b_bytes: Vec<[u8; 4]> = b.iter().map(|x| x.to_le_bytes()).collect();
    assert_eq!(a_bytes, b_bytes, "two embeddings of the same input must be bit-identical");
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    // Both inputs are unit-norm post-embed, so cosine reduces to dot product.
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[test]
fn embed_two_similar_inputs_high_cosine() {
    let e = make_embedder();
    let v_cat_a = e.embed("the cat sat on the mat").unwrap();
    let v_cat_b = e.embed("a cat is on a mat").unwrap();
    let v_unrelated =
        e.embed("quantum mechanics dictates the behaviour of subatomic particles").unwrap();

    let near = cosine(&v_cat_a, &v_cat_b);
    let far_a = cosine(&v_cat_a, &v_unrelated);
    let far_b = cosine(&v_cat_b, &v_unrelated);

    // Threshold: at least a 0.15 absolute cosine gap. BGE-small typically
    // produces > 0.85 cosine for the cat pair and < 0.5 for the cat-vs-
    // physics pair, so the 0.15 floor is comfortably loose.
    assert!(
        near - far_a > 0.15 && near - far_b > 0.15,
        "expected near-pair cosine to dominate by >0.15: near={near}, far_a={far_a}, far_b={far_b}"
    );
}

#[test]
fn embed_does_not_panic_on_empty_string() {
    let e = make_embedder();
    let r = e.embed("");
    assert!(r.is_ok(), "embed(\"\") must produce a vector, got {r:?}");
    let v = r.unwrap();
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!((n - 1.0).abs() < 1e-5, "empty-string vector not unit-norm: ‖v‖ = {n}");
}
