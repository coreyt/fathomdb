//! Smoke test: NomicEmbedder loads nomic-embed-text-v1.5 and embeds.
#![cfg(feature = "default-embedder")]
use fathomdb_embedder::NomicEmbedder;
use fathomdb_embedder_api::Embedder;

#[test]
fn nomic_loads_and_embeds() {
    let dir = std::path::PathBuf::from("/root/.cache/fathomdb/embedders/nomic-v1.5");
    if !dir.join("model.safetensors").exists() {
        eprintln!("[skip] nomic weights absent");
        return;
    }
    let emb = NomicEmbedder::from_dir(&dir).expect("load nomic");
    let v = emb.embed("search_query: what was decided about the budget").expect("embed");
    eprintln!("NOMIC_SMOKE dim={} norm={:.4}", v.len(), v.iter().map(|x| x * x).sum::<f32>().sqrt());
    assert_eq!(v.len(), 768, "nomic dim");
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 1e-3, "unit norm, got {norm}");
    // a related doc should score higher than an unrelated one
    let d_rel = emb.embed("search_document: The committee approved the annual budget after discussion.").expect("e");
    let d_unrel = emb.embed("search_document: The cat sat quietly on the warm windowsill.").expect("e");
    let cos = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
    eprintln!("NOMIC_SMOKE cos_rel={:.3} cos_unrel={:.3}", cos(&v, &d_rel), cos(&v, &d_unrel));
    assert!(cos(&v, &d_rel) > cos(&v, &d_unrel), "related doc should score higher");
}
