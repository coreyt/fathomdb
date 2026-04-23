#![allow(clippy::expect_used, deprecated)]

use fathomdb::{
    ChunkPolicy, Engine, EngineOptions, OperationalWrite, ProjectionTarget, WriteRequestBuilder,
};
use tempfile::NamedTempFile;

#[test]
fn write_request_builder_builds_full_bundle_without_manual_cross_reference_threading() {
    let mut builder = WriteRequestBuilder::new("memex-bundle");
    let meeting = builder.add_node(
        "row-meeting",
        "meeting-1",
        "Meeting",
        r#"{"title":"Budget review"}"#,
        Some("source:meeting".to_owned()),
        true,
        ChunkPolicy::Replace,
        None,
    );
    let task = builder.add_node(
        "row-task",
        "task-1",
        "Task",
        r#"{"title":"Draft memo"}"#,
        Some("source:task".to_owned()),
        true,
        ChunkPolicy::Preserve,
        None,
    );
    builder.add_edge(
        "row-edge",
        "edge-1",
        &meeting,
        &task,
        "HAS_TASK",
        "{}",
        Some("source:edge".to_owned()),
        true,
    );
    let chunk = builder.add_chunk("chunk-1", &meeting, "budget discussion", None, None, None);
    let run = builder.add_run(
        "run-1",
        "session",
        "completed",
        "{}",
        Some("source:run".to_owned()),
        false,
        None,
    );
    let step = builder.add_step(
        "step-1",
        &run,
        "llm",
        "completed",
        "{}",
        Some("source:step".to_owned()),
        false,
        None,
    );
    builder.add_action(
        "action-1",
        &step,
        "emit",
        "completed",
        "{}",
        Some("source:action".to_owned()),
        false,
        None,
    );
    builder.add_vec_insert(&chunk, vec![0.1, 0.2, 0.3, 0.4]);
    builder.add_optional_backfill(ProjectionTarget::Fts, r#"{"reason":"phase2"}"#);
    builder.add_operational_put(
        "connector_health",
        "gmail",
        r#"{"status":"ok"}"#,
        Some("source:ops".to_owned()),
    );

    let request = builder.build().expect("build write request");

    assert_eq!(request.label, "memex-bundle");
    assert_eq!(request.nodes.len(), 2);
    assert_eq!(request.edges.len(), 1);
    assert_eq!(request.chunks.len(), 1);
    assert_eq!(request.runs.len(), 1);
    assert_eq!(request.steps.len(), 1);
    assert_eq!(request.actions.len(), 1);
    assert_eq!(request.optional_backfills.len(), 1);
    assert_eq!(request.vec_inserts.len(), 1);
    assert_eq!(request.operational_writes.len(), 1);
    assert_eq!(request.edges[0].source_logical_id, meeting.logical_id);
    assert_eq!(request.edges[0].target_logical_id, task.logical_id);
    assert_eq!(request.chunks[0].node_logical_id, meeting.logical_id);
    assert_eq!(request.steps[0].run_id, run.id);
    assert_eq!(request.actions[0].step_id, step.id);
    assert_eq!(request.vec_inserts[0].chunk_id, chunk.id);
    assert_eq!(request.nodes[0].row_id, "row-meeting");
    assert_eq!(request.nodes[0].logical_id, "meeting-1");
    assert!(matches!(
        &request.operational_writes[0],
        OperationalWrite::Put {
            collection,
            record_key,
            payload_json,
            ..
        } if collection == "connector_health"
            && record_key == "gmail"
            && payload_json == "{\"status\":\"ok\"}"
    ));
}

#[test]
fn write_request_builder_rejects_handles_from_other_builders_before_submit() {
    let mut first = WriteRequestBuilder::new("first");
    let foreign_node = first.add_node(
        "row-a",
        "node-a",
        "Document",
        "{}",
        Some("source:a".to_owned()),
        false,
        ChunkPolicy::Preserve,
        None,
    );

    let mut second = WriteRequestBuilder::new("second");
    second.add_chunk("chunk-b", &foreign_node, "foreign handle", None, None, None);

    let error = second
        .build()
        .expect_err("foreign handle must fail before submit");
    assert!(error.to_string().contains("different WriteRequestBuilder"));
}

#[test]
fn write_request_builder_outputs_ordinary_write_request_that_can_be_submitted() {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");

    let mut builder = WriteRequestBuilder::new("submit-builder");
    let meeting = builder.add_node(
        "row-meeting",
        "meeting-1",
        "Meeting",
        r#"{"status":"active"}"#,
        Some("source:meeting".to_owned()),
        true,
        ChunkPolicy::Replace,
        None,
    );
    builder.add_chunk("chunk-1", &meeting, "budget discussion", None, None, None);

    let request = builder.build().expect("build request");
    engine
        .writer()
        .submit(request)
        .expect("submit built request");

    let compiled = engine
        .query("Meeting")
        .text_search("budget", 5)
        .limit(5)
        .compile()
        .expect("compile query");
    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("execute read");
    assert_eq!(rows.nodes.len(), 1);
    assert_eq!(rows.nodes[0].logical_id, "meeting-1");
}
