//! IR-C reuse tier — validate the RESOLVED real gold set against the IR-B
//! harness loader/validator.
//!
//! `tests/corpus/scripts/build_ir_gold.py` resolves the dataset-authored eval
//! QA (`data/corpus-data/eval/<source>_qa.jsonl`) into the corpus-pinned
//! `GoldSet` schema this harness consumes (`support/ir_eval.rs`). This test
//! proves the produced gold sets actually load, validate clean, and are pinned
//! to the frozen 0.8.x-B snapshot — i.e. they are ready to drive a real
//! Evidence Recall@K run.
//!
//! The gold JSON is a gitignored build artifact (derived from cache-only /
//! licensed sources, like the raw corpus), so this test SKIPs gracefully when
//! it has not been built — `cargo test` stays green without the corpus.
//! Rebuild with: `tests/corpus/scripts/build_ir_gold.py`.

#[path = "support/corpus_subset.rs"]
mod corpus_subset;
#[path = "support/ir_eval.rs"]
mod ir_eval;

use std::collections::BTreeSet;
use std::path::PathBuf;

use ir_eval::{load_gold_set, required_doc_ids, validate_gold_set, QueryClass};

/// The frozen 0.8.x-B corpus hash (mirrors `tests/corpus/snapshot.json`); the
/// resolved gold must be pinned to exactly this snapshot.
const FROZEN_CORPUS_HASH: &str =
    "fe973fcd49fbbda083158f69fe720f17858ab8528e171fa2188eec84131c7d4e";

fn gold_path(name: &str) -> Option<PathBuf> {
    let root = corpus_subset::repo_root()?;
    let p = root.join("data/corpus-data/eval/ir_gold").join(name);
    p.exists().then_some(p)
}

#[test]
fn resolved_combined_gold_loads_validates_and_is_pinned() {
    let Some(path) = gold_path("all.gold.json") else {
        eprintln!(
            "SKIP: data/corpus-data/eval/ir_gold/all.gold.json absent \
             (gitignored — run tests/corpus/scripts/build_ir_gold.py)"
        );
        return;
    };

    let gold = load_gold_set(&path).expect("load resolved gold set");

    // Pinned to the frozen snapshot — never the unpinned placeholder.
    assert_eq!(
        gold.corpus_hash, FROZEN_CORPUS_HASH,
        "resolved gold must be pinned to the frozen 0.8.x-B snapshot"
    );
    assert_eq!(gold.qrels_version, "ir-c-reused-v1");

    // The validator (methodology + schema invariants) must be fully clean —
    // this is the real source of truth, not the Python build-time mirror.
    let issues = validate_gold_set(&gold);
    assert!(issues.is_empty(), "resolved gold has validator issues: {issues:?}");

    // The reuse tier carries three classes: exact_fact (enronqa/qaconv factoid),
    // exploratory (qmsum summaries), negative (qaconv abstain).
    let classes: BTreeSet<_> = gold.queries.iter().map(|q| q.query_class).collect();
    for c in [QueryClass::ExactFact, QueryClass::Exploratory, QueryClass::Negative] {
        assert!(classes.contains(&c), "expected class {} in resolved gold", c.label());
    }

    // Every positive query has a non-empty required denominator that resolves to
    // a doc_id; every negative query abstains (empty denominator). (The
    // validator already enforces this, but assert the shape explicitly.)
    let mut positives = 0usize;
    let mut negatives = 0usize;
    for q in &gold.queries {
        let req = required_doc_ids(q);
        if q.query_class == QueryClass::Negative {
            negatives += 1;
            assert!(req.is_empty(), "negative query {:?} has a denominator", q.query_id);
        } else {
            positives += 1;
            assert!(!req.is_empty(), "positive query {:?} has empty denominator", q.query_id);
        }
    }
    assert!(positives > 0 && negatives > 0, "expected both positive and negative queries");
    eprintln!(
        "IR-C resolved gold OK: {} queries ({} positive, {} negative), pinned to {}",
        gold.queries.len(),
        positives,
        negatives,
        &FROZEN_CORPUS_HASH[..12]
    );
}

#[test]
fn resolved_per_source_gold_sets_validate_when_present() {
    for source in ["enronqa", "qaconv", "qmsum"] {
        let Some(path) = gold_path(&format!("{source}.gold.json")) else {
            eprintln!("SKIP: {source}.gold.json absent (run build_ir_gold.py)");
            continue;
        };
        let gold = load_gold_set(&path).unwrap_or_else(|e| panic!("load {source} gold: {e}"));
        assert_eq!(gold.corpus_hash, FROZEN_CORPUS_HASH, "{source}: not pinned to frozen snapshot");
        let issues = validate_gold_set(&gold);
        assert!(issues.is_empty(), "{source} gold validator issues: {issues:?}");
        assert!(!gold.queries.is_empty(), "{source} gold is empty");
    }
}
