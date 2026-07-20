//! Slice 10 (F5 — fielded FTS / BM25F) — R-F5-1 acceptance.
//!
//! plan-0.8.14 §2 R-F5-1: "RED->GREEN: a field-weighted query outranks an
//! unweighted baseline on a known fixture." ADR authority:
//! `dev/adr/ADR-0.8.1-deferred-f5-fielded-fts-bm25f.md` (BM25fQueryPlan, §3.2) +
//! `dev/adr/ADR-0.8.14-...` §D4/§D6.
//!
//! The lever: `search_index_v2` is a multi-column FTS5 index (kind/body/status);
//! `Engine::bm25f_search` recalls candidates through the FTS5 index and scores
//! them with an in-engine BM25F using per-field weights + tunable `b`. The test
//! is genuinely falsifiable: the gold row's query term lives ONLY in its `kind`
//! field, the distractor's ONLY in its `body`. Under a body-only (unweighted)
//! baseline the distractor outranks the gold; boosting the `kind` weight flips
//! it. If the scorer IGNORED the weights the two rankings would be identical and
//! the flip assertion fails.

use std::sync::Arc;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Bm25fFieldWeights, Bm25fQueryPlan, Engine, PreparedWrite};
use tempfile::TempDir;

/// Deterministic hash-placement embedder (mirrors `exp_s_substrate.rs`). No
/// vector kind is configured in these tests, so it is never actually invoked —
/// the engine just needs an embedder to open.
#[derive(Debug)]
struct DeterministicEmbedder {
    identity: EmbedderIdentity,
    dim: u32,
}

impl DeterministicEmbedder {
    fn new(dim: u32) -> Self {
        Self { identity: EmbedderIdentity::new("deterministic", "f5-bm25f", dim), dim }
    }
}

impl Embedder for DeterministicEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }
    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        Ok(vec![0.0_f32; self.dim as usize])
    }
}

fn fresh_engine(name: &str) -> (TempDir, Engine) {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join(format!("{name}.sqlite"));
    let opened =
        Engine::open_with_embedder_for_test(&path, Arc::new(DeterministicEmbedder::new(768)))
            .expect("open");
    (dir, opened.engine)
}

fn node(kind: &str, body: &str) -> PreparedWrite {
    PreparedWrite::Node {
        kind: kind.to_string(),
        body: body.to_string(),
        source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
        logical_id: None,
        state: fathomdb_engine::InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

fn rank_of(results: &[(u64, f64)], wc: u64) -> usize {
    results
        .iter()
        .position(|(id, _)| *id == wc)
        .unwrap_or_else(|| panic!("write_cursor {wc} not present in results {results:?}"))
}

/// R-F5-1 — a field-weighted BM25F query outranks the unweighted (body-only)
/// baseline on a known fixture. Falsifiable: fails if field weights are ignored.
#[test]
fn r_f5_1_field_weight_flips_ranking_vs_body_only_baseline() {
    let (_dir, engine) = fresh_engine("bm25f_flip");

    // Fixture. The query term "todo" appears:
    //   * in the GOLD row's KIND field only (body has no "todo").
    //   * in the DISTRACTOR row's BODY field only (kind is "note").
    // Plus filler rows so IDF / average field length are non-degenerate.
    let batch = vec![
        node("todo", "alpha bravo charlie delta echo"), // cursor 1 = gold (kind match)
        node("note", "todo alpha bravo charlie delta"), // cursor 2 = distractor (body match)
        node("note", "foxtrot golf hotel india"),       // cursor 3 = filler
        node("doc", "juliet kilo lima mike november"),  // cursor 4 = filler
    ];
    let receipt = engine.write(&batch).expect("write fixture");
    engine.drain(15_000).expect("drain");
    assert_eq!(receipt.row_cursors, vec![1, 2, 3, 4], "deterministic per-row cursors");
    let gold = 1_u64;
    let distractor = 2_u64;

    // Baseline: body-only (kind & status weight 0) — mimics the current
    // single-column body-only lexical arm. The gold row (kind-only match) has no
    // body hit, so the distractor MUST outrank it.
    let baseline = Bm25fQueryPlan {
        weights: Bm25fFieldWeights { kind: 0.0, body: 1.0, status: 0.0 },
        ..Bm25fQueryPlan::default()
    };
    let baseline_results = engine.bm25f_search("todo", &baseline).expect("baseline search");
    assert!(
        rank_of(&baseline_results, distractor) < rank_of(&baseline_results, gold),
        "body-only baseline: distractor must outrank the kind-only gold, got {baseline_results:?}"
    );

    // Weighted: boost the KIND field. Now the gold row's kind match dominates and
    // it MUST outrank the distractor — the ranking flips.
    let weighted = Bm25fQueryPlan {
        weights: Bm25fFieldWeights { kind: 8.0, body: 1.0, status: 0.0 },
        ..Bm25fQueryPlan::default()
    };
    let weighted_results = engine.bm25f_search("todo", &weighted).expect("weighted search");
    assert!(
        rank_of(&weighted_results, gold) < rank_of(&weighted_results, distractor),
        "kind-weighted: gold must outrank the distractor, got {weighted_results:?}"
    );

    // Both rows are candidates in BOTH rankings (so the assertions above compare
    // genuine positions, not presence) — and the relative order genuinely FLIPS.
    assert!(
        rank_of(&baseline_results, gold) != rank_of(&weighted_results, gold),
        "the gold row's rank must change between baseline and weighted plans"
    );
}

/// fix-1 (codex §9 finding 1) — the in-engine scorer must measure tf / df /
/// field-lengths under the SAME tokenization the `search_index_v2` FTS5 index
/// uses for recall (`porter unicode61 remove_diacritics 2`). A query term that
/// matches a document ONLY via porter stemming (`run` vs indexed `running`) or
/// diacritic folding (`cafe` vs indexed `café`) is recalled by `MATCH`; it must
/// therefore also be COUNTED by the scorer, not treated as absent.
///
/// Falsifiable RED under the previous lowercase-alnum splitter: it tokenized
/// `running` as `["running"]` (never `run`) and `café` as `["café"]` (never
/// `cafe`), so tf and df for the query term were both 0 and the recalled doc
/// scored exactly 0.0 — present in the result set but effectively unranked.
/// GREEN after fix-1: both the field text and the query go through FTS5, both
/// sides fold to the same stem, tf/df are positive, and the score is > 0.
#[test]
fn r_f5_1_fix1_scorer_is_tokenization_faithful_to_fts5() {
    let (_dir, engine) = fresh_engine("bm25f_tokfaithful");

    // Each gold row matches its query ONLY under FTS5 tokenization:
    //   * cursor 1 — body "running" stems to "run"  (query "run").
    //   * cursor 2 — body "café"    folds  to "cafe" (query "cafe").
    // Filler rows keep IDF / average field length non-degenerate.
    let batch = vec![
        node("note", "running late again"), // cursor 1 = porter-stem gold
        node("note", "meet at the café tonight"), // cursor 2 = diacritic gold
        node("note", "foxtrot golf hotel india"), // cursor 3 = filler
        node("doc", "juliet kilo lima mike"), // cursor 4 = filler
    ];
    let receipt = engine.write(&batch).expect("write fixture");
    assert_eq!(receipt.row_cursors, vec![1, 2, 3, 4], "deterministic per-row cursors");

    let plan = Bm25fQueryPlan::default();

    // Porter-stem case: "run" must score the "running" doc as PRESENT (> 0).
    let stem = engine.bm25f_search("run", &plan).expect("stem search");
    let stem_score = stem
        .iter()
        .find(|(id, _)| *id == 1)
        .map(|(_, s)| *s)
        .unwrap_or_else(|| panic!("porter-stem doc must be recalled by MATCH, got {stem:?}"));
    assert!(
        stem_score > 0.0,
        "query 'run' must score the porter-stemmed 'running' doc > 0 (RED under the \
         lowercase-alnum splitter, which counted the term as absent → 0.0), got {stem_score} \
         in {stem:?}"
    );

    // Diacritic case: "cafe" must score the "café" doc as PRESENT (> 0).
    let dia = engine.bm25f_search("cafe", &plan).expect("diacritic search");
    let dia_score = dia
        .iter()
        .find(|(id, _)| *id == 2)
        .map(|(_, s)| *s)
        .unwrap_or_else(|| panic!("diacritic doc must be recalled by MATCH, got {dia:?}"));
    assert!(
        dia_score > 0.0,
        "query 'cafe' must score the diacritic-folded 'café' doc > 0 (RED under the \
         lowercase-alnum splitter), got {dia_score} in {dia:?}"
    );
}

/// Write-path integration — the multi-index write populates `search_index_v2`
/// synchronously (same transaction) for every FTS-searchable row, so the BM25F
/// arm sees rows immediately after `write` (no drain needed for the sync FTS).
#[test]
fn write_path_populates_search_index_v2_synchronously() {
    let (_dir, engine) = fresh_engine("bm25f_writepath");
    let batch = vec![node("todo", "buy milk today"), node("note", "meeting notes about milk")];
    engine.write(&batch).expect("write");

    // A body-term query recalls both rows via the v2 FTS index.
    let plan = Bm25fQueryPlan::default();
    let results = engine.bm25f_search("milk", &plan).expect("search");
    assert_eq!(results.len(), 2, "both rows must be search_index_v2-indexed, got {results:?}");
    let mut ids: Vec<u64> = results.iter().map(|(id, _)| *id).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec![1, 2]);
}
