#![allow(clippy::expect_used)]

use std::sync::{Arc, Mutex};

use fathomdb::{
    ChunkInsert, ChunkPolicy, Engine, EngineOptions, FeedbackConfig, NodeInsert, OperationObserver,
    ProjectionTarget, ResponseCycleEvent, ResponseCyclePhase, WriteRequest,
    compile_query_with_feedback, new_row_id,
};
use tempfile::NamedTempFile;

#[derive(Clone, Default)]
struct RecordingObserver {
    events: Arc<Mutex<Vec<ResponseCycleEvent>>>,
}

impl RecordingObserver {
    fn phases(&self) -> Vec<ResponseCyclePhase> {
        self.events
            .lock()
            .expect("observer mutex")
            .iter()
            .map(|event| event.phase)
            .collect()
    }
}

impl OperationObserver for RecordingObserver {
    fn on_event(&self, event: &ResponseCycleEvent) {
        self.events
            .lock()
            .expect("observer mutex")
            .push(event.clone());
    }
}

#[test]
fn open_with_feedback_emits_started_and_finished() {
    let db = NamedTempFile::new().expect("temporary db");
    let observer = RecordingObserver::default();

    let engine = Engine::open_with_feedback(
        EngineOptions::new(db.path()),
        &observer,
        FeedbackConfig::default(),
    )
    .expect("engine opens");

    let phases = observer.phases();
    assert_eq!(
        phases,
        vec![ResponseCyclePhase::Started, ResponseCyclePhase::Finished]
    );

    let _ = engine;
}

#[test]
fn compile_write_query_and_admin_feedback_are_publicly_available() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");
    let observer = RecordingObserver::default();
    let config = FeedbackConfig::default();

    let compiled = compile_query_with_feedback(
        &engine
            .query("Meeting")
            .text_search("budget", 5)
            .limit(5)
            .into_ast(),
        &observer,
        config.clone(),
    )
    .expect("query compiles");

    engine
        .submit_write_with_feedback(
            WriteRequest {
                label: "feedback-seed".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: new_row_id(),
                    logical_id: "meeting:feedback".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: r#"{"title":"Feedback"}"#.to_owned(),
                    source_ref: Some("source:feedback".to_owned()),
                    upsert: true,
                    chunk_policy: ChunkPolicy::Replace,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![ChunkInsert {
                    id: "chunk:feedback:0".to_owned(),
                    node_logical_id: "meeting:feedback".to_owned(),
                    text_content: "budget feedback coverage".to_owned(),
                    byte_start: None,
                    byte_end: None,
                }],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
            },
            &observer,
            config.clone(),
        )
        .expect("write succeeds");

    let rows = engine
        .execute_compiled_query_with_feedback(&compiled, &observer, config.clone())
        .expect("query executes");
    assert_eq!(rows.nodes.len(), 1);

    let repair = engine
        .rebuild_projections_with_feedback(ProjectionTarget::Fts, &observer, config.clone())
        .expect("rebuild succeeds");
    assert_eq!(repair.targets, vec![ProjectionTarget::Fts]);

    let integrity = engine
        .check_integrity_with_feedback(&observer, config)
        .expect("integrity succeeds");
    assert!(integrity.physical_ok);
}
