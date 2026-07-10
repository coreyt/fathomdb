//! 0.8.0 Slice 5 / G1 — structured `SearchHit` shape.
//!
//! AC-G1-hit-shape, AC-G1-no-eq, AC-G1-dedup-order. Asserts that
//! `SearchResult.results` is `Vec<SearchHit>` (not `Vec<String>`), each hit
//! carries `id == write_cursor`, populated `kind`, populated `body`, a finite
//! `score`, and the correct `branch`; that `SearchResult` no longer derives
//! `Eq` (a `SearchHit` carries `score: f64`); and that dedup-on-body +
//! vector-first ordering is preserved.
//!
//! Uses a deterministic in-process embedder so both retrieval branches
//! exercise without network. No mocking of the database.

use std::sync::Arc;
use std::time::{Duration, Instant};

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, IdSpace, IdSpaceKind, PreparedWrite, SearchHit, SoftFallbackBranch};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

/// Deterministic embedder: every text maps to the same unit vector, so the
/// vector branch always surfaces the (single, when one doc) candidate and the
/// f32 rerank distance is finite.
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

fn search_after_projection(
    engine: &Engine,
    query: &str,
    min_cursor: u64,
) -> fathomdb_engine::SearchResult {
    let started = Instant::now();
    loop {
        let result = engine.search(query).expect("search");
        if result.projection_cursor >= min_cursor && !result.results.is_empty() {
            return result;
        }
        if started.elapsed() > Duration::from_secs(10) {
            return result;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn ac_g1_hit_shape_text_branch() {
    let (_dir, path) = fixture("g1_hit_shape");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");

    let receipt = opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "note".to_string(),
            body: "structured retrieval hit shape document".to_string(),
            source_id: None,
            logical_id: None,
            state: fathomdb_engine::InitialState::Active,
            reason: None,
        }])
        .expect("write");
    opened.engine.drain(10_000).expect("drain");

    let result = search_after_projection(&opened.engine, "structured", receipt.cursor);
    assert_eq!(result.results.len(), 1, "expected exactly one hit");
    let hit: &SearchHit = &result.results[0];

    // C-2 (0.8.19): the caller-facing `id` is now the typed IdSpace; the
    // engine-internal positional cursor lives on `write_cursor`.
    assert_eq!(hit.write_cursor, receipt.cursor, "hit write_cursor must be the projection cursor");
    // populated kind + body.
    assert_eq!(hit.kind, "note");
    assert_eq!(hit.body, "structured retrieval hit shape document");
    // finite score.
    assert!(hit.score.is_finite(), "score must be finite, got {}", hit.score);
    // text branch tag (no vector kind configured -> text-only).
    assert_eq!(hit.branch, SoftFallbackBranch::Text);

    opened.engine.close().unwrap();
}

#[test]
fn ac_g1_hit_shape_vector_branch() {
    let (_dir, path) = fixture("g1_hit_shape_vec");
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(FixedEmbedder)).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");

    let receipt = opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            // FTS query term ("vectorize") is NOT in the body, so the only
            // way this surfaces is the vector branch.
            body: "semantic only payload".to_string(),
            source_id: None,
            logical_id: None,
            state: fathomdb_engine::InitialState::Active,
            reason: None,
        }])
        .expect("write");
    opened.engine.drain(10_000).expect("drain");

    let result = search_after_projection(&opened.engine, "vectorize", receipt.cursor);
    assert_eq!(result.results.len(), 1, "expected exactly one vector hit");
    let hit = &result.results[0];
    assert_eq!(hit.write_cursor, receipt.cursor);
    assert_eq!(hit.kind, "doc");
    assert_eq!(hit.body, "semantic only payload");
    assert!(hit.score.is_finite());
    assert_eq!(hit.branch, SoftFallbackBranch::Vector);

    opened.engine.close().unwrap();
}

#[test]
fn ac_g1_dedup_on_body_and_vector_first_order() {
    let (_dir, path) = fixture("g1_dedup_order");
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(FixedEmbedder)).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");

    // Two docs: both share the FTS term "hybrid"; both are vector candidates.
    let r1 = opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "hybrid retrieval document".to_string(),
            source_id: None,
            logical_id: None,
            state: fathomdb_engine::InitialState::Active,
            reason: None,
        }])
        .expect("write 1");
    let r2 = opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "another hybrid document".to_string(),
            source_id: None,
            logical_id: None,
            state: fathomdb_engine::InitialState::Active,
            reason: None,
        }])
        .expect("write 2");
    opened.engine.drain(10_000).expect("drain");

    let result = search_after_projection(&opened.engine, "hybrid", r2.cursor.max(r1.cursor));

    // Dedup-on-body: a body surfaced by BOTH branches must appear exactly once.
    let mut bodies: Vec<&str> = result.results.iter().map(|h| h.body.as_str()).collect();
    let mut deduped = bodies.clone();
    deduped.sort_unstable();
    deduped.dedup();
    bodies.sort_unstable();
    assert_eq!(bodies, deduped, "results must be deduped on body");

    // Vector-first ordering: the vector branch's hits precede any text-only
    // hits. With this fixed embedder both docs are vector candidates, so every
    // surviving hit is tagged Vector and none is a trailing text-only dup.
    let first_branch = result.results[0].branch;
    assert_eq!(
        first_branch,
        SoftFallbackBranch::Vector,
        "vector-first ordering: leading hit must be from the vector branch"
    );

    // Every hit carries the structured shape.
    for hit in &result.results {
        assert!(hit.write_cursor > 0, "write_cursor must be populated");
        assert_eq!(hit.kind, "doc");
        assert!(!hit.body.is_empty());
        assert!(hit.score.is_finite());
    }

    opened.engine.close().unwrap();
}

/// Compile-level proof that `SearchResult` (and `SearchHit`) no longer derive
/// `Eq` is enforced by the `score: f64` field. This runtime check additionally
/// asserts `PartialEq` is retained (results compare by value) — the derive set
/// is `Clone, Debug, PartialEq`, NOT `Eq`.
#[test]
fn ac_g1_no_eq_but_partial_eq() {
    let (_dir, path) = fixture("g1_no_eq");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    let receipt = opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "note".to_string(),
            body: "equality probe document".to_string(),
            source_id: None,
            logical_id: None,
            state: fathomdb_engine::InitialState::Active,
            reason: None,
        }])
        .expect("write");
    opened.engine.drain(10_000).expect("drain");
    let a = search_after_projection(&opened.engine, "equality", receipt.cursor);
    let b = a.clone();
    assert_eq!(a, b, "SearchResult must retain PartialEq");
    opened.engine.close().unwrap();
}

// ---------------------------------------------------------------------------
// Cause-A (0.8.11.2) / C-2 (0.8.19, TC-8) — the cross-session-stable hit id is
// now the typed `SearchHit.id: IdSpace` (the additive `stable_id` field is
// subsumed INTO `id`). A governed node's `id` is `Logical` (`"l:"` + logical_id);
// a doc-seeded node's `id` is `Content` (`"h:"` + content hash). The id VALUE is
// byte-identical to the pre-swap `stable_id` (eu7 no-op basis) and never
// participates in ranking — proven additive by the unchanged ordering/score
// assertions above.
// ---------------------------------------------------------------------------

/// A doc node (NULL `logical_id`, the dominant corpus case) surfaces a
/// `Content` id whose prefixed form is `"h:" + 64 hex chars`, deterministic
/// across repeated searches.
#[test]
fn cause_a_doc_node_stable_id_is_content_hash() {
    let (_dir, path) = fixture("cause_a_doc_hash");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    let receipt = opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "note".to_string(),
            body: "cause-a content hash probe".to_string(),
            source_id: None,
            logical_id: None,
            state: fathomdb_engine::InitialState::Active,
            reason: None,
        }])
        .expect("write");
    opened.engine.drain(10_000).expect("drain");

    let result = search_after_projection(&opened.engine, "probe", receipt.cursor);
    let hit = &result.results[0];
    assert_eq!(hit.id.space, IdSpaceKind::Content, "doc node id must be the Content space");
    let sid = hit.id.to_prefixed();
    assert!(sid.starts_with("h:"), "doc node id must be content-hash tagged, got {sid}");
    assert_eq!(sid.len(), 2 + 64, "h: + sha256 hex (64 chars)");
    assert!(
        sid["h:".len()..].chars().all(|c| c.is_ascii_hexdigit()),
        "content-hash must be lowercase hex, got {sid}"
    );

    // Deterministic across a second search (no re-ingest).
    let again = search_after_projection(&opened.engine, "probe", receipt.cursor);
    assert_eq!(again.results[0].id, hit.id.clone(), "id must be deterministic");
    opened.engine.close().unwrap();
}

/// Cross-session stability — the WHOLE point of Cause-A. The interim `id`
/// (`write_cursor`) is reassigned on re-ingest, but the content-hash stable id
/// of the same body is byte-identical across two independent databases.
#[test]
fn cause_a_doc_node_stable_id_survives_reingest() {
    let body = "cause-a reingest survival document";

    let capture = |name: &str| -> String {
        let (_dir, path) = fixture(name);
        let opened = Engine::open_without_embedder_for_test(&path).expect("open");
        // Write a throwaway node first so the second DB's write_cursor differs.
        if name.ends_with("_b") {
            opened
                .engine
                .write(&[PreparedWrite::Node {
                    kind: "note".to_string(),
                    body: "padding so cursors diverge".to_string(),
                    source_id: None,
                    logical_id: None,
                    state: fathomdb_engine::InitialState::Active,
                    reason: None,
                }])
                .expect("pad");
        }
        let receipt = opened
            .engine
            .write(&[PreparedWrite::Node {
                kind: "note".to_string(),
                body: body.to_string(),
                source_id: None,
                logical_id: None,
                state: fathomdb_engine::InitialState::Active,
                reason: None,
            }])
            .expect("write");
        opened.engine.drain(10_000).expect("drain");
        let result = search_after_projection(&opened.engine, "survival", receipt.cursor);
        let hit = &result.results[0];
        let sid = hit.id.to_prefixed();
        // Record the interim positional cursor too, to prove it is NOT what gives stability.
        let interim = hit.write_cursor;
        opened.engine.close().unwrap();
        format!("{sid}|{interim}")
    };

    let a = capture("cause_a_reingest_a");
    let b = capture("cause_a_reingest_b");
    let (sid_a, id_a) = a.split_once('|').unwrap();
    let (sid_b, id_b) = b.split_once('|').unwrap();
    assert_eq!(sid_a, sid_b, "content-hash id must survive re-ingest");
    assert_ne!(
        id_a, id_b,
        "interim write_cursor diverges across sessions (the reason the stable id exists)"
    );
}

/// A node WITH a `logical_id` surfaces an `"l:"`-tagged stable id carrying that
/// exact logical id (the post-G0 supersession-stable identity).
#[test]
fn cause_a_logical_id_node_is_l_tagged() {
    let (_dir, path) = fixture("cause_a_logical");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    let receipt = opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "person".to_string(),
            body: "cause-a logical identity entity".to_string(),
            source_id: None,
            logical_id: Some("entity-cause-a-42".to_string()),
            state: fathomdb_engine::InitialState::Active,
            reason: None,
        }])
        .expect("write");
    opened.engine.drain(10_000).expect("drain");

    let result = search_after_projection(&opened.engine, "entity", receipt.cursor);
    let hit = &result.results[0];
    assert_eq!(
        hit.id,
        IdSpace::logical("entity-cause-a-42"),
        "a node with a logical_id must carry the Logical (l:) id"
    );
    assert_eq!(hit.id.to_prefixed(), "l:entity-cause-a-42", "prefixed form is byte-stable");
    opened.engine.close().unwrap();
}

/// Supersession correctness (0.8.11.2 pico): a node rewritten via the same
/// `logical_id` must NOT surface its superseded version in default `search`.
///
/// Node supersession is tombstone-then-insert: the prior `canonical_nodes` row
/// is tombstoned (`superseded_at` set, row + its OLD `search_index` row kept)
/// and a NEW `search_index` row is inserted for the new body. Before the fix,
/// the default node-text branch `LEFT JOIN`ed `canonical_nodes` only to decorate
/// the hit with `logical_id` and had no `superseded_at IS NULL` filter, so a
/// query for a term unique to the OLD body still returned the stale version.
///
/// Asserts: (1) a term unique to the OLD body returns ZERO hits (the superseded
/// version is excluded and the new body lacks the term); (2) a term unique to
/// the NEW body returns exactly the active version, `l:`-tagged with the shared
/// logical id. Against the unpatched query, assertion (1) fails (one stale hit).
#[test]
fn supersession_search_excludes_superseded_node_version() {
    let (_dir, path) = fixture("supersede_search");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    let logical_id = "entity-supersede-7";

    // V1: original body carrying a term unique to the OLD version.
    opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "person".to_string(),
            body: "alpha obsoleteoldterm original biography".to_string(),
            source_id: None,
            logical_id: Some(logical_id.to_string()),
            state: fathomdb_engine::InitialState::Active,
            reason: None,
        }])
        .expect("write v1");
    opened.engine.drain(10_000).expect("drain v1");

    // V2: rewrite via the SAME logical_id → V1 is superseded (tombstone-then-
    // insert). The new body carries a DIFFERENT unique term.
    let receipt = opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "person".to_string(),
            body: "beta freshactiveterm replacement biography".to_string(),
            source_id: None,
            logical_id: Some(logical_id.to_string()),
            state: fathomdb_engine::InitialState::Active,
            reason: None,
        }])
        .expect("write v2");
    opened.engine.drain(10_000).expect("drain v2");

    // Positive: the active version surfaces for a NEW-body term (also blocks on
    // projection so the FTS index is fully populated).
    let active = search_after_projection(&opened.engine, "freshactiveterm", receipt.cursor);
    assert_eq!(active.results.len(), 1, "active version must surface for the new-body term");
    assert_eq!(
        active.results[0].id,
        IdSpace::logical("entity-supersede-7"),
        "the surfaced hit must be the active (Logical / l:) version"
    );

    // Regression: the OLD-body term must surface NOTHING — the superseded
    // search_index row is still live in FTS, so the SQL `superseded_at IS NULL`
    // filter is the only thing excluding it. Direct `search` (no projection
    // wait) since we assert emptiness.
    let stale = opened.engine.search("obsoleteoldterm").expect("search stale term");
    assert!(
        stale.results.is_empty(),
        "a term unique to the superseded version must return no hits, got {:?}",
        stale.results.iter().map(|h| (&h.body, h.id.to_prefixed())).collect::<Vec<_>>()
    );
    opened.engine.close().unwrap();
}

/// Distinct bodies → distinct content-hash stable ids (no collisions on the
/// dominant doc-node path).
#[test]
fn cause_a_distinct_bodies_distinct_stable_ids() {
    let (_dir, path) = fixture("cause_a_distinct");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    let r1 = opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "note".to_string(),
            body: "cause-a distinct alpha unique".to_string(),
            source_id: None,
            logical_id: None,
            state: fathomdb_engine::InitialState::Active,
            reason: None,
        }])
        .expect("w1");
    let r2 = opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "note".to_string(),
            body: "cause-a distinct beta unique".to_string(),
            source_id: None,
            logical_id: None,
            state: fathomdb_engine::InitialState::Active,
            reason: None,
        }])
        .expect("w2");
    opened.engine.drain(10_000).expect("drain");

    let result = search_after_projection(&opened.engine, "unique", r1.cursor.max(r2.cursor));
    let ids: std::collections::HashSet<_> =
        result.results.iter().map(|h| h.id.to_prefixed()).collect();
    assert!(ids.len() >= 2, "two distinct bodies must yield two distinct ids, got {ids:?}");
    assert!(ids.iter().all(|s| s.starts_with("h:")), "all doc-node ids are content-hash tagged");
    opened.engine.close().unwrap();
}
