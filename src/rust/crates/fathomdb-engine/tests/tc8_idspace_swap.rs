//! 0.8.19 Slice 15 / C-2 typed `SearchHit.id` swap (TC-8, R-ID-1/2).
//!
//! Pins the C-2 contract: `SearchHit.id` is a typed [`IdSpace`] (`{ space,
//! value }`), non-null and **id-space-total** across the three hit classes —
//! `Logical` (`"l:"`, governed), `Content` (`"h:"`, doc-seeded/anonymous), and
//! `Passage` (`"p:"`, synthetic rerank). The interim positional `write_cursor`
//! AND the additive Cause-A `stable_id` field are both **subsumed into `id`**:
//! `write_cursor` stays a separate engine-internal positional cursor, and the
//! `id` VALUE is byte-identical to the pre-swap `stable_id` output so real-gold
//! keying is a no-op (eu7 no-op basis).
//!
//! Uses the deterministic no-embedder text branch (no network, no mocking).

use std::time::{Duration, Instant};

use fathomdb_engine::{
    rerank_passages, Engine, IdSpace, IdSpaceKind, InitialState, PreparedWrite, SearchResult,
};
use fathomdb_schema::SQLITE_SUFFIX;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

/// Reproduce the PRE-swap `derive_stable_id` content-hash form byte-for-byte:
/// `"h:" + lowercase-hex(sha256(body))`. Kept local (the engine fn is private)
/// so the eu7 no-op proof does not depend on engine internals — if the id ever
/// diverges from this exact string the assertion fails.
fn prior_stable_id_content(body: &str) -> String {
    let hex: String = Sha256::digest(body.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
    format!("h:{hex}")
}

fn fixture(name: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("{name}{SQLITE_SUFFIX}"));
    (dir, path)
}

fn search_after_projection(engine: &Engine, query: &str, min_cursor: u64) -> SearchResult {
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

/// R-ID-1 — a **governed** node hit (carries a `logical_id`) surfaces a typed
/// `Logical` (`"l:"`) id whose value is exactly that `logical_id`, and its
/// engine-internal `write_cursor` is populated separately (subsumption, not
/// drop). The `l:` space is the only lifecycle-addressable one (consumed by the
/// Slice-10 verbs).
#[test]
fn governed_hit_id_is_logical_space() {
    let (_dir, path) = fixture("tc8_logical");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    let receipt = opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "person".to_string(),
            body: "tc8 governed logical entity payload".to_string(),
            source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
            logical_id: Some("gov-entity-19".to_string()),
            state: InitialState::Active,
            reason: None,
        }])
        .expect("write");
    opened.engine.drain(10_000).expect("drain");

    let result = search_after_projection(&opened.engine, "governed", receipt.cursor);
    let hit = &result.results[0];

    // Typed id-space (variant, NOT a magic-prefixed string).
    assert_eq!(hit.id.space, IdSpaceKind::Logical);
    assert_eq!(hit.id.value, "gov-entity-19");
    // Byte-stable prefixed form == pre-swap `stable_id`.
    assert_eq!(hit.id.to_prefixed(), "l:gov-entity-19");
    assert_eq!(hit.id, IdSpace::logical("gov-entity-19"));
    // write_cursor subsumed (separate positional cursor), not the caller id.
    assert_eq!(hit.write_cursor, receipt.cursor, "positional cursor retained internally");
    assert_ne!(hit.id.value, hit.write_cursor.to_string(), "id is NOT the positional cursor");

    opened.engine.close().unwrap();
}

/// R-ID-1 — a **doc-seeded/anonymous** node hit (NULL `logical_id`, the dominant
/// corpus class) surfaces a typed `Content` (`"h:"`) id: `"h:" + 64 hex`. It is
/// NOT migrated to `l:` (gap-1 ruling 1a — no surrogate in Phase-1).
#[test]
fn doc_seeded_hit_id_is_content_space() {
    let (_dir, path) = fixture("tc8_content");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    let body = "tc8 anonymous docseeded payload xyzzy";
    let receipt = opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "note".to_string(),
            body: body.to_string(),
            source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
            logical_id: None,
            state: InitialState::Active,
            reason: None,
        }])
        .expect("write");
    opened.engine.drain(10_000).expect("drain");

    let result = search_after_projection(&opened.engine, "docseeded", receipt.cursor);
    let hit = &result.results[0];

    assert_eq!(hit.id.space, IdSpaceKind::Content);
    let prefixed = hit.id.to_prefixed();
    assert!(prefixed.starts_with("h:"), "doc-seeded id is content-hash tagged, got {prefixed}");
    assert_eq!(prefixed.len(), 2 + 64, "h: + sha256 hex");
    assert!(hit.id.value.chars().all(|c| c.is_ascii_hexdigit()), "content hash is lowercase hex");
    // eu7 NO-OP PROOF (byte-for-byte): the prefixed form is the EXACT pre-swap
    // `derive_stable_id` output `"h:" + sha256(body)` — real-gold keys on this
    // string, so it must not drift by a single byte from the prior release.
    assert_eq!(prefixed, prior_stable_id_content(body), "content id == prior stable_id bytes");
    assert_eq!(
        hit.id.value,
        prior_stable_id_content(body)["h:".len()..],
        "bare value is prefix-stripped"
    );
    // write_cursor is a separate positional cursor.
    assert_eq!(hit.write_cursor, receipt.cursor);

    opened.engine.close().unwrap();
}

/// R-ID-1 (totality) — every returned hit carries a non-null `id` whose space is
/// one of exactly the three variants; no hit is left unaddressed-by-`id`.
#[test]
fn every_hit_id_is_non_null_and_space_total() {
    let (_dir, path) = fixture("tc8_total");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    let r = opened
        .engine
        .write(&[
            PreparedWrite::Node {
                kind: "person".to_string(),
                body: "tc8 total governed alpha totalterm".to_string(),
                source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
                logical_id: Some("total-gov-1".to_string()),
                state: InitialState::Active,
                reason: None,
            },
            PreparedWrite::Node {
                kind: "note".to_string(),
                body: "tc8 total anonymous beta totalterm".to_string(),
                source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
                logical_id: None,
                state: InitialState::Active,
                reason: None,
            },
        ])
        .expect("write");
    opened.engine.drain(10_000).expect("drain");

    let result = search_after_projection(&opened.engine, "totalterm", r.cursor);
    assert!(result.results.len() >= 2, "both docs retrieved");
    for hit in &result.results {
        // Non-null + total: the space is one of the three typed variants and the
        // prefixed form round-trips back to the same typed id.
        assert!(matches!(
            hit.id.space,
            IdSpaceKind::Logical | IdSpaceKind::Content | IdSpaceKind::Passage
        ));
        assert!(!hit.id.value.is_empty(), "id value is non-null");
        assert_eq!(IdSpace::parse(&hit.id.to_prefixed()), Some(hit.id.clone()), "round-trip");
    }
    // Both classes present: at least one Logical and one Content.
    assert!(result.results.iter().any(|h| h.id.space == IdSpaceKind::Logical));
    assert!(result.results.iter().any(|h| h.id.space == IdSpaceKind::Content));

    opened.engine.close().unwrap();
}

/// R-ID-1 — synthetic `rerank_passages` hits mint the `Passage` (`"p:"`) id from
/// the caller-supplied ordinal (they carried NO stable id before the swap). The
/// public projection returns that same ordinal (proving the ordinal is retained
/// as the positional cursor), and the `Passage` space round-trips.
#[test]
fn synthetic_passage_id_is_passage_space_from_ordinal() {
    // `rerank_depth == 0` → identity: order + the caller ordinals are preserved.
    let passages = vec![
        (11u64, "passage eleven body".to_string(), 0.9_f64),
        (22u64, "passage twenty-two body".to_string(), 0.8_f64),
    ];
    let out = rerank_passages("q", passages, 0, 0.3, 0).expect("rerank");
    let ordinals: Vec<u64> = out.iter().map(|(id, _, _)| *id).collect();
    assert_eq!(ordinals, vec![11, 22], "caller ordinals retained as the positional cursor");

    // The minted Passage id for an ordinal round-trips as `p:<ordinal>`.
    let pid = IdSpace::passage(11u64.to_string());
    assert_eq!(pid.space, IdSpaceKind::Passage);
    assert_eq!(pid.to_prefixed(), "p:11");
    assert_eq!(IdSpace::parse("p:11"), Some(pid));
}
