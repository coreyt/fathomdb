//! 0.8.8 Slice 15 (OPP-9) — opt-in telemetry capture.
//!
//! Pins the §B contract: off-by-default (no sink, no rows, no feedback API), an
//! opt-in local JSONL sink that records query→result events keyed on the stable
//! `logical_id`, a correlated agent-feedback record, deterministic `query_id`,
//! and the privacy guarantees (no query text, no `source_id`, local file only —
//! no egress).

use std::sync::Arc;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

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

fn opened(name: &str) -> (TempDir, fathomdb_engine::OpenedEngine) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("{name}{SQLITE_SUFFIX}"));
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(FixedEmbedder)).expect("open");
    (dir, opened)
}

fn seed(engine: &Engine) {
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    for body in ["hybrid retrieval alpha", "hybrid retrieval beta"] {
        engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: body.to_string(),
                source_id: None,
                logical_id: None,
            }])
            .expect("write");
    }
    engine.drain(10_000).expect("drain");
}

#[test]
fn telemetry_is_off_by_default() {
    let (_dir, opened) = opened("tel_off");
    seed(&opened.engine);
    // Off by default: no captured query id; the feedback API errors.
    let _ = opened.engine.search("hybrid").expect("search");
    assert_eq!(opened.engine.last_telemetry_query_id(), None);
    assert!(
        opened.engine.record_feedback("q0-0", &[1], &[], "agent:test").is_err(),
        "record_feedback must error when telemetry is off"
    );
    opened.engine.close().unwrap();
}

#[test]
fn telemetry_captures_event_and_feedback_deterministically() {
    let (dir, opened) = opened("tel_on");
    seed(&opened.engine);
    let sink = dir.path().join("telemetry.jsonl");
    let sink_str = sink.to_str().unwrap();
    opened.engine.enable_telemetry(sink_str).expect("enable");

    // First captured query → deterministic id "q0-0".
    let r0 = opened.engine.search("hybrid").expect("search");
    assert!(!r0.results.is_empty(), "expected hits to capture");
    assert_eq!(opened.engine.last_telemetry_query_id().as_deref(), Some("q0-0"));
    // Second query → "q0-1" (deterministic sequential id).
    let _ = opened.engine.search("retrieval").expect("search");
    assert_eq!(opened.engine.last_telemetry_query_id().as_deref(), Some("q0-1"));

    // Attach agent feedback for the first query.
    opened
        .engine
        .record_feedback("q0-0", &[r0.results[0].id], &[], "agent:test")
        .expect("feedback");

    opened.engine.close().unwrap();

    let body = std::fs::read_to_string(&sink).expect("sink readable");
    let lines: Vec<&str> = body.lines().collect();
    // 2 event rows + 1 feedback row.
    assert_eq!(lines.len(), 3, "expected 2 events + 1 feedback, got {}", lines.len());

    let ev0: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(ev0["type"], "event");
    assert_eq!(ev0["query_id"], "q0-0");
    assert_eq!(ev0["schema_version"], 1);
    assert_eq!(ev0["query_chars"], "hybrid".chars().count() as u64);
    assert!(ev0["result_ids"].as_array().is_some_and(|a| !a.is_empty()));
    assert!(ev0["arm_of"].is_object());

    let fb: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
    assert_eq!(fb["type"], "feedback");
    assert_eq!(fb["query_id"], "q0-0");
    assert_eq!(fb["label_source"], "agent:test");

    // Privacy (ADR §C): the query TEXT never appears in the sink; only ids/length.
    assert!(!body.contains("hybrid"), "query text must NOT be captured");
    assert!(!body.contains("retrieval"), "query text must NOT be captured");
    // `source_id` is never a key in the sink (leak vector).
    assert!(!body.contains("source_id"), "source_id must NOT be captured");
}

#[test]
fn record_feedback_rejects_unissued_query_id_and_writes_nothing() {
    // codex §9 [P1] (privacy): `record_feedback` must only persist a `query_id`
    // the capture path has ALREADY emitted. A caller-smuggled string (e.g. query
    // text, a `source_id`, or an out-of-range deterministic id) is rejected and
    // NOTHING is appended to the sink — closing the leak vector where private text
    // could be persisted under the `query_id` key.
    let (dir, opened) = opened("tel_bogus");
    seed(&opened.engine);
    let sink = dir.path().join("telemetry.jsonl");
    opened.engine.enable_telemetry(sink.to_str().unwrap()).expect("enable");

    // One real query → emits "q0-0" (sink.seq advances to 1). One event line.
    let r0 = opened.engine.search("hybrid").expect("search");
    assert!(!r0.results.is_empty());
    assert_eq!(opened.engine.last_telemetry_query_id().as_deref(), Some("q0-0"));

    // Bogus query_ids must all error: query text, an out-of-range seq, a
    // wrong-nonce id, and a non-parsing string.
    for bogus in ["hybrid", "q0-99", "q1-0", "not-an-id", ""] {
        assert!(
            opened.engine.record_feedback(bogus, &[r0.results[0].id], &[], "agent:test").is_err(),
            "record_feedback must reject unissued query_id {bogus:?}"
        );
    }

    // The previously-issued id still works (regression guard).
    opened
        .engine
        .record_feedback("q0-0", &[r0.results[0].id], &[], "agent:test")
        .expect("issued id accepted");

    opened.engine.close().unwrap();

    let body = std::fs::read_to_string(&sink).expect("sink readable");
    let lines: Vec<&str> = body.lines().collect();
    // Exactly 1 event + 1 feedback (the issued-id call); NONE of the bogus calls wrote.
    assert_eq!(lines.len(), 2, "bogus feedback must not append: {body}");
    let ev: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(ev["type"], "event");
    let fb: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(fb["type"], "feedback");
    assert_eq!(fb["query_id"], "q0-0");
    // No bogus string leaked into the sink.
    assert!(!body.contains("hybrid"), "rejected query text must NOT appear");
    assert!(!body.contains("not-an-id"), "rejected string must NOT appear");
}
