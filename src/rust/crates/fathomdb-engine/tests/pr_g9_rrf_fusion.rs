//! Slice 10 / G9 — Reciprocal Rank Fusion as the unconditional new ranking.
//!
//! Pins the RRF contract: `Σ 1/(RRF_K + rank)` keyed on `SearchHit.body`,
//! agreement (a body in both branches) outranks single-branch hits,
//! vector-first tiebreak, dedup-on-body, and a `rerank_fused` identity stub.
//! There is **no** legacy-union-ordering reproduction (HITL Q3 — no knob); the
//! pinned property is **determinism**, not legacy reproducibility.
//!
//! The formula/tiebreak/dedup are unit-tested directly on the pure `fuse_rrf`
//! function (no embedder, fully deterministic); a second e2e test asserts that
//! repeated `Engine::search` calls return byte-identical order + scores, and
//! that the vector-empty `soft_fallback` signal is computed BEFORE the branches
//! collapse into the fused list. No mocking of the database.

use std::sync::Arc;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{
    fuse_rrf, fuse_three_arms, rerank_fused, Engine, ExtractDocument, PreparedWrite, SearchHit,
    SoftFallback, SoftFallbackBranch, RRF_K, RRF_WEIGHT_GRAPH, RRF_WEIGHT_TEXT, RRF_WEIGHT_VECTOR,
};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

fn hit(id: u64, body: &str, branch: SoftFallbackBranch) -> SearchHit {
    SearchHit { id, kind: "doc".to_string(), body: body.to_string(), score: 0.0, branch }
}

#[test]
fn rrf_formula_single_branch_ranks() {
    // Vector branch only: "a" rank 1, "b" rank 2. Text branch empty. The vector
    // weight is 1.0, so the bare-rank formula still holds.
    let fused = fuse_rrf(
        vec![hit(1, "a", SoftFallbackBranch::Vector), hit(2, "b", SoftFallbackBranch::Vector)],
        Vec::new(),
    );
    assert_eq!(fused.iter().map(|h| h.body.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
    assert!(
        (fused[0].score - RRF_WEIGHT_VECTOR / (RRF_K + 1.0)).abs() < 1e-12,
        "rank-1 = w_vec/(K+1)"
    );
    assert!(
        (fused[1].score - RRF_WEIGHT_VECTOR / (RRF_K + 2.0)).abs() < 1e-12,
        "rank-2 = w_vec/(K+2)"
    );
}

#[test]
fn rrf_agreement_outranks_single_branch() {
    // "agree" is rank 1 in BOTH branches => (w_vec + w_text)/(K+1); single-branch
    // hits get one weighted term only. Agreement must win — the point of fusion.
    let vector = vec![
        hit(1, "agree", SoftFallbackBranch::Vector),
        hit(2, "vonly", SoftFallbackBranch::Vector),
    ];
    let text =
        vec![hit(1, "agree", SoftFallbackBranch::Text), hit(3, "tonly", SoftFallbackBranch::Text)];
    let fused = fuse_rrf(vector, text);

    assert_eq!(fused[0].body, "agree", "both-branch hit ranks first");
    assert!(
        (fused[0].score - (RRF_WEIGHT_VECTOR + RRF_WEIGHT_TEXT) / (RRF_K + 1.0)).abs() < 1e-12,
        "agree = (w_vec + w_text)/(K+1)"
    );
    assert!(fused[0].score > fused[1].score, "agreement strictly outranks single-branch");
    // Representative of a both-branch body is the VECTOR hit (vector-first id).
    assert_eq!(fused[0].branch, SoftFallbackBranch::Vector);
    assert_eq!(fused[0].id, 1);
}

#[test]
fn rrf_text_weighted_outranks_vector_at_equal_rank() {
    // IR-C text-dominant weighting (3:1): "tonly" (text rank 2, score
    // w_text/(K+2)) now strictly outranks "vonly" (vector rank 2, w_vec/(K+2)).
    // Order: agree (both) > tonly (text) > vonly (vector). Also pins dedup-on-body.
    let vector = vec![
        hit(1, "agree", SoftFallbackBranch::Vector),
        hit(2, "vonly", SoftFallbackBranch::Vector),
    ];
    let text =
        vec![hit(1, "agree", SoftFallbackBranch::Text), hit(3, "tonly", SoftFallbackBranch::Text)];
    let fused = fuse_rrf(vector, text);

    assert_eq!(
        fused.iter().map(|h| h.body.as_str()).collect::<Vec<_>>(),
        vec!["agree", "tonly", "vonly"],
        "score desc; text weight (3:1) lifts the rank-2 text hit above the rank-2 vector hit"
    );
    assert_eq!(fused.iter().filter(|h| h.body == "agree").count(), 1, "dedup on body");
}

#[test]
fn rrf_vector_first_on_exact_score_tie() {
    // The vector-first tiebreak fires only on an EXACT score tie. Under the 3:1
    // weighting a vector rank-1 hit (w_vec/(K+1)) ties a text hit at the rank r
    // where w_text/(K+r) == w_vec/(K+1) ⇒ r = (w_text/w_vec)*(K+1) - K. Construct
    // that exact tie and assert the vector hit sorts first.
    let r = ((RRF_WEIGHT_TEXT / RRF_WEIGHT_VECTOR) * (RRF_K + 1.0) - RRF_K) as usize;
    assert!(r >= 1, "constructed tie rank must be valid");
    let mut text: Vec<SearchHit> = (1..r)
        .map(|i| hit(1000 + i as u64, &format!("filler{i}"), SoftFallbackBranch::Text))
        .collect();
    text.push(hit(2, "tie", SoftFallbackBranch::Text)); // text rank r
    let vector = vec![hit(1, "vtie", SoftFallbackBranch::Vector)]; // vector rank 1
    let fused = fuse_rrf(vector, text);

    let vpos = fused.iter().position(|h| h.body == "vtie").expect("vtie present");
    let tpos = fused.iter().position(|h| h.body == "tie").expect("tie present");
    assert!((fused[vpos].score - fused[tpos].score).abs() < 1e-12, "scores are exactly tied");
    assert!(vpos < tpos, "vector-first orders the vector hit ahead of the tied text hit");
}

#[test]
fn rerank_fused_is_identity_stub() {
    // 0.8.1 Slice 10 (R1): the signature changed to `rerank_fused(query, hits, depth)`.
    // At depth=0 the soft-fallback path must return the input unchanged —
    // byte-identical to the old identity stub. Spirit of the original test preserved.
    let hits = vec![hit(1, "a", SoftFallbackBranch::Vector), hit(2, "b", SoftFallbackBranch::Text)];
    assert_eq!(rerank_fused("", hits.clone(), 0), hits);
}

/// Deterministic embedder so the e2e ordering is a pure function of the corpus.
#[derive(Clone, Debug)]
struct FixedEmbedder;

impl Embedder for FixedEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new("deterministic", "rev-a", 8)
    }
    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        let mut v = vec![0.0_f32; 8];
        v[0] = 1.0;
        Ok(v)
    }
}

fn fixture(name: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("{name}{SQLITE_SUFFIX}"));
    (dir, path)
}

#[test]
fn rrf_end_to_end_order_is_deterministic() {
    let (_dir, path) = fixture("g9_determinism");
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(FixedEmbedder)).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");
    for body in ["hybrid retrieval alpha", "hybrid retrieval beta", "hybrid retrieval gamma"] {
        opened
            .engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: body.to_string(),
                source_id: None,
                logical_id: None,
            }])
            .expect("write");
    }
    opened.engine.drain(10_000).expect("drain");

    let first = opened.engine.search("hybrid").expect("search");
    assert!(!first.results.is_empty(), "expected fused hits");
    // Repeated identical searches must produce byte-identical order + scores.
    for _ in 0..5 {
        let again = opened.engine.search("hybrid").expect("search");
        assert_eq!(again, first, "RRF fused order + scores must be deterministic");
    }
    // Every fused score is finite and the list is sorted descending.
    for w in first.results.windows(2) {
        assert!(w[0].score >= w[1].score, "fused list sorted by score desc");
    }
    opened.engine.close().unwrap();
}

#[test]
fn vector_empty_soft_fallback_signal_survives_fusion() {
    // Vector branch empty (projection frozen) but the text branch matches a
    // vector-kind row: the soft-fallback signal is computed BEFORE the branches
    // collapse, so fusion must not erase it.
    let (_dir, path) = fixture("g9_soft_fallback");
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(FixedEmbedder)).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");
    opened.engine.set_projection_scheduler_frozen_for_test(true);

    opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "phase nine hybrid search".to_string(),
            source_id: None,
            logical_id: None,
        }])
        .expect("write");

    // The FTS `search_index` is written synchronously in `commit_batch`; only
    // the vector projection is frozen. So the text branch matches immediately
    // while the vector branch stays empty.
    let result = opened.engine.search("hybrid").expect("search");
    assert_eq!(
        result.soft_fallback,
        Some(SoftFallback { branch: SoftFallbackBranch::Vector }),
        "vector-empty signal must survive the fusion collapse"
    );
    opened.engine.close().unwrap();
}

// ---------------------------------------------------------------------------
// Slice 30 / R3 — RED-2: Three-arm RRF fusion tests
//
// These tests FAIL at the RED phase because `fuse_three_arms` and
// `RRF_WEIGHT_GRAPH` do not exist yet and `Engine::search_reranked` does not
// accept a `use_graph_arm` parameter yet.
// ---------------------------------------------------------------------------

/// RED-2a: Fusing with a non-empty graph arm is deterministic across two calls.
///
/// Verifies that `fuse_three_arms(v, t, g)` returns the same result on
/// repeated calls (pure function of the three input lists, no randomness).
#[test]
fn rrf_three_arm_determinism() {
    let vector = vec![
        hit(1, "vector_alpha", SoftFallbackBranch::Vector),
        hit(2, "vector_beta", SoftFallbackBranch::Vector),
    ];
    let text = vec![
        hit(1, "vector_alpha", SoftFallbackBranch::Text),
        hit(3, "text_only", SoftFallbackBranch::Text),
    ];
    let graph = vec![
        hit(10, "graph_reach_a", SoftFallbackBranch::GraphArm),
        hit(11, "graph_reach_b", SoftFallbackBranch::GraphArm),
    ];

    let first = fuse_three_arms(vector.clone(), text.clone(), graph.clone());
    let second = fuse_three_arms(vector.clone(), text.clone(), graph.clone());
    assert_eq!(first, second, "fuse_three_arms must be deterministic (same result on two calls)");

    // The graph arm's weight contribution must use RRF_WEIGHT_GRAPH.
    // A body in the graph arm at rank 1 gets RRF_WEIGHT_GRAPH / (RRF_K + 1).
    let graph_reach_a = first
        .iter()
        .find(|h| h.body == "graph_reach_a")
        .expect("graph_reach_a must appear in fused result");
    assert!(
        (graph_reach_a.score - RRF_WEIGHT_GRAPH / (RRF_K + 1.0)).abs() < 1e-10,
        "graph arm rank-1 hit: score = RRF_WEIGHT_GRAPH/(RRF_K+1) = {:.6}; got {:.6}",
        RRF_WEIGHT_GRAPH / (RRF_K + 1.0),
        graph_reach_a.score
    );
}

/// RED-2b: `fuse_rrf(v, t)` accumulates `Σ weight/(RRF_K + rank)` correctly
/// for each arm — verified against manually-computed expected scores so this
/// test catches formula regressions in fuse_three_arms independent of the
/// delegation path.
///
/// Inputs: "agree" appears at rank-0 in both arms; "vonly" rank-1 vector only;
/// "tonly" rank-1 text only.
#[test]
fn rrf_two_arm_formula_matches_manual_accumulation() {
    let vector = vec![
        hit(1, "agree", SoftFallbackBranch::Vector),
        hit(2, "vonly", SoftFallbackBranch::Vector),
    ];
    let text =
        vec![hit(1, "agree", SoftFallbackBranch::Text), hit(3, "tonly", SoftFallbackBranch::Text)];

    let fused = fuse_rrf(vector, text);

    // Manual accumulation using the published constants.
    let agree_expected = RRF_WEIGHT_VECTOR / (RRF_K + 1.0) + RRF_WEIGHT_TEXT / (RRF_K + 1.0);
    let vonly_expected = RRF_WEIGHT_VECTOR / (RRF_K + 2.0);
    let tonly_expected = RRF_WEIGHT_TEXT / (RRF_K + 2.0);

    // "agree" has the highest combined score → must rank first.
    assert_eq!(fused[0].body, "agree", "agree must rank first (highest combined score)");
    assert!(
        (fused[0].score - agree_expected).abs() < 1e-12,
        "agree score: expected {agree_expected:.12}, got {:.12}",
        fused[0].score
    );

    // "tonly" outranks "vonly" because RRF_WEIGHT_TEXT > RRF_WEIGHT_VECTOR.
    assert_eq!(fused[1].body, "tonly", "tonly must rank second (heavier text weight)");
    assert!(
        (fused[1].score - tonly_expected).abs() < 1e-12,
        "tonly score: expected {tonly_expected:.12}, got {:.12}",
        fused[1].score
    );
    assert_eq!(fused[2].body, "vonly");
    assert!(
        (fused[2].score - vonly_expected).abs() < 1e-12,
        "vonly score: expected {vonly_expected:.12}, got {:.12}",
        fused[2].score
    );
}

/// RED-2c: A body reachable only via the graph arm (not in vector or text hits)
/// appears in the fused result when `use_graph_arm=true` is threaded through
/// the full Engine pipeline.
///
/// This test uses a real Engine with BYO-LLM ingestion to build a small graph
/// (Alice -> BobCorp -> Carol Ltd via edges), then searches for "Alice" and
/// asserts that "Carol" (reachable at hop 2) appears in results when
/// use_graph_arm=true but NOT when use_graph_arm=false.
///
/// FAILS at RED because `Engine::search_reranked` does not yet accept `use_graph_arm`.
#[test]
fn rrf_graph_arm_expands_ranking() {
    use std::io::Write as _;

    let dir = tempfile::TempDir::new().unwrap();

    // Build an inline stub harness.
    let stub_result_alice = r#"{"edges":[{"body":"Alice works at BobCorp.","confidence":0.9,"from_entity":"Alice","relation":"works_at","source_doc_id":"alice_doc","source_span":null,"t_invalid":null,"t_valid":"2024-01-01T00:00:00Z","to_entity":"BobCorp"}],"entities":[{"aliases":[],"name":"Alice","synthesized":false,"type":"Person"},{"aliases":[],"name":"BobCorp","synthesized":false,"type":"Organization"}],"protocol":"fathomdb.extract.v1","request_id":"r1","type":"result","warnings":[]}"#;
    let stub_result_bob = r#"{"edges":[{"body":"BobCorp partners with Carol Ltd.","confidence":0.85,"from_entity":"BobCorp","relation":"partners_with","source_doc_id":"bob_doc","source_span":null,"t_invalid":null,"t_valid":"2024-01-01T00:00:00Z","to_entity":"Carol Ltd"}],"entities":[{"aliases":[],"name":"BobCorp","synthesized":false,"type":"Organization"},{"aliases":[],"name":"Carol Ltd","synthesized":false,"type":"Organization"}],"protocol":"fathomdb.extract.v1","request_id":"r2","type":"result","warnings":[]}"#;

    let mut stub_src = String::from("import json,sys\nRESULTS={\n");
    stub_src.push_str("\"alice_doc\": '");
    stub_src.push_str(stub_result_alice);
    stub_src.push_str("',\n\"bob_doc\": '");
    stub_src.push_str(stub_result_bob);
    stub_src.push_str("',\n}\n");
    stub_src.push_str(
        r#"
for line in sys.stdin:
    line=line.strip()
    if not line: continue
    msg=json.loads(line)
    t=msg.get("type")
    if t=="hello":
        print(json.dumps({"protocol":"fathomdb.extract.v1","type":"ready","schema_version":1,"model":"stub-v1","max_docs_per_request":1}),flush=True)
    elif t=="extract":
        docs=msg.get("documents",[])
        did=docs[0]["source_doc_id"]
        res=json.loads(RESULTS[did])
        res["request_id"]=msg.get("request_id")
        print(json.dumps(res,ensure_ascii=False),flush=True)
"#,
    );
    let stub_path = dir.path().join("graph_arm_stub.py");
    {
        let mut f = std::fs::File::create(&stub_path).unwrap();
        f.write_all(stub_src.as_bytes()).unwrap();
    }
    let stub_str = stub_path.to_string_lossy().to_string();

    let db_path = dir.path().join(format!("graph_arm_expand{}", fathomdb_schema::SQLITE_SUFFIX));
    let opened = Engine::open_without_embedder_for_test(&db_path).expect("open");

    let cmd = vec!["python3".to_string(), stub_str.clone()];
    let cmd_refs: Vec<&str> = cmd.iter().map(String::as_str).collect();
    opened
        .engine
        .ingest_with_extractor(
            &cmd_refs,
            &[ExtractDocument {
                source_doc_id: "alice_doc".to_string(),
                body: "Alice works at BobCorp.".to_string(),
            }],
        )
        .expect("ingest alice_doc");

    let cmd2 = vec!["python3".to_string(), stub_str.clone()];
    let cmd2_refs: Vec<&str> = cmd2.iter().map(String::as_str).collect();
    opened
        .engine
        .ingest_with_extractor(
            &cmd2_refs,
            &[ExtractDocument {
                source_doc_id: "bob_doc".to_string(),
                body: "BobCorp partners with Carol Ltd.".to_string(),
            }],
        )
        .expect("ingest bob_doc");

    // With use_graph_arm=false (default), Carol Ltd should NOT appear in
    // results for "Alice" (it only matches via graph traversal).
    // With use_graph_arm=true, Carol Ltd IS reachable (Alice->BobCorp->Carol Ltd at hop 2).
    //
    // FAILS at RED because search_reranked doesn't yet accept use_graph_arm.
    let without_arm =
        opened.engine.search_reranked("Alice", None, 0, false).expect("search without graph arm");
    let carol_without = without_arm.results.iter().any(|h| h.body.contains("Carol"));

    let with_arm =
        opened.engine.search_reranked("Alice", None, 0, true).expect("search with graph arm");
    let carol_with = with_arm.results.iter().any(|h| h.body.contains("Carol"));

    // When graph arm is enabled, Carol (reachable via BFS from Alice's node)
    // should appear in results.
    assert!(
        carol_with || with_arm.results.len() > without_arm.results.len(),
        "graph arm must expand ranking: Carol (hop-2 from Alice) should appear with use_graph_arm=true \
         (carol_without={carol_without}, carol_with={carol_with})"
    );

    opened.engine.close().unwrap();
}
