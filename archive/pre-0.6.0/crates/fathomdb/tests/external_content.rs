#![allow(clippy::expect_used)]

mod helpers;

use fathomdb::{
    ChunkInsert, ChunkPolicy, Engine, EngineOptions, NodeInsert, WriteRequest, WriteRequestBuilder,
};
use tempfile::NamedTempFile;

fn open_engine() -> (NamedTempFile, Engine) {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");
    (db, engine)
}

fn write_request(label: &str, nodes: Vec<NodeInsert>, chunks: Vec<ChunkInsert>) -> WriteRequest {
    WriteRequest {
        label: label.into(),
        nodes,
        node_retires: vec![],
        edges: vec![],
        edge_retires: vec![],
        chunks,
        runs: vec![],
        steps: vec![],
        actions: vec![],
        optional_backfills: vec![],
        vec_inserts: vec![],
        operational_writes: vec![],
    }
}

#[test]
fn content_ref_persists_on_node_roundtrip() {
    let (db, engine) = open_engine();

    engine
        .writer()
        .submit(write_request(
            "content-ref-test",
            vec![NodeInsert {
                row_id: "row-1".into(),
                logical_id: "doc-1".into(),
                kind: "Document".into(),
                properties: r#"{"title":"Annual Report"}"#.into(),
                source_ref: None,
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: Some("s3://docs/annual-report.pdf".into()),
            }],
            vec![],
        ))
        .expect("write completes");

    let fields = helpers::node_fields(db.path(), "doc-1");
    assert_eq!(
        fields.content_ref.as_deref(),
        Some("s3://docs/annual-report.pdf")
    );
}

#[test]
fn content_hash_persists_on_chunk_roundtrip() {
    let (db, engine) = open_engine();

    engine
        .writer()
        .submit(write_request(
            "content-hash-test",
            vec![NodeInsert {
                row_id: "row-1".into(),
                logical_id: "doc-1".into(),
                kind: "Document".into(),
                properties: "{}".into(),
                source_ref: None,
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            vec![ChunkInsert {
                id: "chunk-1".into(),
                node_logical_id: "doc-1".into(),
                text_content: "page one content".into(),
                byte_start: Some(0),
                byte_end: Some(16),
                content_hash: Some("sha256:abc123".into()),
            }],
        ))
        .expect("write completes");

    let fields = helpers::chunk_fields(db.path(), "chunk-1");
    assert_eq!(fields.content_hash.as_deref(), Some("sha256:abc123"));
}

#[test]
fn upsert_replaces_content_ref_and_chunks() {
    let (_db, engine) = open_engine();

    engine
        .writer()
        .submit(write_request(
            "upsert-v1",
            vec![NodeInsert {
                row_id: "row-v1".into(),
                logical_id: "doc-1".into(),
                kind: "Document".into(),
                properties: r#"{"version":1}"#.into(),
                source_ref: None,
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: Some("s3://docs/v1.pdf".into()),
            }],
            vec![ChunkInsert {
                id: "chunk-v1".into(),
                node_logical_id: "doc-1".into(),
                text_content: "version one".into(),
                byte_start: None,
                byte_end: None,
                content_hash: Some("sha256:v1hash".into()),
            }],
        ))
        .expect("initial write");

    engine
        .writer()
        .submit(write_request(
            "upsert-v2",
            vec![NodeInsert {
                row_id: "row-v2".into(),
                logical_id: "doc-1".into(),
                kind: "Document".into(),
                properties: r#"{"version":2}"#.into(),
                source_ref: None,
                upsert: true,
                chunk_policy: ChunkPolicy::Replace,
                content_ref: Some("s3://docs/v2.pdf".into()),
            }],
            vec![ChunkInsert {
                id: "chunk-v2".into(),
                node_logical_id: "doc-1".into(),
                text_content: "version two".into(),
                byte_start: None,
                byte_end: None,
                content_hash: Some("sha256:v2hash".into()),
            }],
        ))
        .expect("upsert write");

    let compiled = engine
        .query("Document")
        .text_search("version", 5)
        .limit(5)
        .compile()
        .expect("compile query");
    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("execute read");
    assert_eq!(rows.nodes.len(), 1);
    assert_eq!(
        rows.nodes[0].content_ref.as_deref(),
        Some("s3://docs/v2.pdf")
    );
}

#[test]
fn content_ref_not_null_filter_returns_only_content_nodes() {
    let (_db, engine) = open_engine();

    engine
        .writer()
        .submit(write_request(
            "filter-test",
            vec![
                NodeInsert {
                    row_id: "row-with".into(),
                    logical_id: "doc-with".into(),
                    kind: "Document".into(),
                    properties: r#"{"has_content":true}"#.into(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: Some("s3://docs/report.pdf".into()),
                },
                NodeInsert {
                    row_id: "row-without".into(),
                    logical_id: "doc-without".into(),
                    kind: "Document".into(),
                    properties: r#"{"has_content":false}"#.into(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
            ],
            vec![
                ChunkInsert {
                    id: "chunk-with".into(),
                    node_logical_id: "doc-with".into(),
                    text_content: "content node text".into(),
                    byte_start: None,
                    byte_end: None,
                    content_hash: None,
                },
                ChunkInsert {
                    id: "chunk-without".into(),
                    node_logical_id: "doc-without".into(),
                    text_content: "plain node text".into(),
                    byte_start: None,
                    byte_end: None,
                    content_hash: None,
                },
            ],
        ))
        .expect("write completes");

    let compiled = engine
        .query("Document")
        .text_search("node text", 10)
        .filter_content_ref_not_null()
        .limit(10)
        .compile()
        .expect("compile content_ref_not_null query");
    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("execute read");
    assert_eq!(rows.nodes.len(), 1);
    assert_eq!(rows.nodes[0].logical_id, "doc-with");
}

#[test]
fn content_ref_eq_filter_matches_exact_uri() {
    let (_db, engine) = open_engine();

    engine
        .writer()
        .submit(write_request(
            "eq-filter-test",
            vec![
                NodeInsert {
                    row_id: "row-a".into(),
                    logical_id: "doc-a".into(),
                    kind: "Document".into(),
                    properties: "{}".into(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: Some("s3://docs/alpha.pdf".into()),
                },
                NodeInsert {
                    row_id: "row-b".into(),
                    logical_id: "doc-b".into(),
                    kind: "Document".into(),
                    properties: "{}".into(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: Some("s3://docs/beta.pdf".into()),
                },
            ],
            vec![
                ChunkInsert {
                    id: "chunk-a".into(),
                    node_logical_id: "doc-a".into(),
                    text_content: "alpha document".into(),
                    byte_start: None,
                    byte_end: None,
                    content_hash: None,
                },
                ChunkInsert {
                    id: "chunk-b".into(),
                    node_logical_id: "doc-b".into(),
                    text_content: "beta document".into(),
                    byte_start: None,
                    byte_end: None,
                    content_hash: None,
                },
            ],
        ))
        .expect("write completes");

    let compiled = engine
        .query("Document")
        .text_search("document", 10)
        .filter_content_ref_eq("s3://docs/beta.pdf")
        .limit(10)
        .compile()
        .expect("compile content_ref_eq query");
    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("execute read");
    assert_eq!(rows.nodes.len(), 1);
    assert_eq!(rows.nodes[0].logical_id, "doc-b");
}

#[test]
fn nodes_without_content_ref_return_none() {
    let (_db, engine) = open_engine();

    engine
        .writer()
        .submit(write_request(
            "none-test",
            vec![NodeInsert {
                row_id: "row-plain".into(),
                logical_id: "plain-1".into(),
                kind: "Note".into(),
                properties: "{}".into(),
                source_ref: None,
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            vec![ChunkInsert {
                id: "chunk-plain".into(),
                node_logical_id: "plain-1".into(),
                text_content: "just a note".into(),
                byte_start: None,
                byte_end: None,
                content_hash: None,
            }],
        ))
        .expect("write completes");

    let compiled = engine
        .query("Note")
        .text_search("note", 5)
        .limit(5)
        .compile()
        .expect("compile query");
    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("execute read");
    assert_eq!(rows.nodes.len(), 1);
    assert!(rows.nodes[0].content_ref.is_none());
}

#[test]
fn content_ref_not_null_filter_via_nodes_driving_table() {
    let (_db, engine) = open_engine();

    engine
        .writer()
        .submit(write_request(
            "nodes-dt-test",
            vec![
                NodeInsert {
                    row_id: "row-ext".into(),
                    logical_id: "doc-ext".into(),
                    kind: "Document".into(),
                    properties: "{}".into(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: Some("s3://docs/ext.pdf".into()),
                },
                NodeInsert {
                    row_id: "row-plain".into(),
                    logical_id: "doc-plain".into(),
                    kind: "Document".into(),
                    properties: "{}".into(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
            ],
            vec![],
        ))
        .expect("write completes");

    // Query without text_search/vector_search uses the Nodes driving table,
    // exercising the pushdown code path in compile.rs.
    let compiled = engine
        .query("Document")
        .filter_content_ref_not_null()
        .limit(10)
        .compile()
        .expect("compile nodes-DT query");
    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("execute read");
    assert_eq!(rows.nodes.len(), 1);
    assert_eq!(rows.nodes[0].logical_id, "doc-ext");
}

#[test]
fn content_ref_eq_filter_via_nodes_driving_table() {
    let (_db, engine) = open_engine();

    engine
        .writer()
        .submit(write_request(
            "nodes-eq-dt-test",
            vec![
                NodeInsert {
                    row_id: "row-a".into(),
                    logical_id: "doc-a".into(),
                    kind: "Document".into(),
                    properties: "{}".into(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: Some("s3://docs/alpha.pdf".into()),
                },
                NodeInsert {
                    row_id: "row-b".into(),
                    logical_id: "doc-b".into(),
                    kind: "Document".into(),
                    properties: "{}".into(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: Some("s3://docs/beta.pdf".into()),
                },
            ],
            vec![],
        ))
        .expect("write completes");

    let compiled = engine
        .query("Document")
        .filter_content_ref_eq("s3://docs/alpha.pdf")
        .limit(10)
        .compile()
        .expect("compile nodes-DT eq query");
    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("execute read");
    assert_eq!(rows.nodes.len(), 1);
    assert_eq!(rows.nodes[0].logical_id, "doc-a");
}

#[test]
fn write_request_builder_roundtrips_content_ref_and_content_hash() {
    let (db, engine) = open_engine();

    let mut builder = WriteRequestBuilder::new("builder-ext-test");
    let node = builder.add_node(
        "row-1",
        "doc-1",
        "Document",
        r#"{"title":"Report"}"#,
        None,
        false,
        ChunkPolicy::Preserve,
        Some("s3://docs/report.pdf".to_owned()),
    );
    builder.add_chunk(
        "chunk-1",
        &node,
        "report content",
        Some(0),
        Some(14),
        Some("sha256:deadbeef".to_owned()),
    );

    let request = builder.build().expect("build request");
    assert_eq!(
        request.nodes[0].content_ref.as_deref(),
        Some("s3://docs/report.pdf")
    );
    assert_eq!(
        request.chunks[0].content_hash.as_deref(),
        Some("sha256:deadbeef")
    );

    engine.writer().submit(request).expect("submit");

    let nf = helpers::node_fields(db.path(), "doc-1");
    assert_eq!(nf.content_ref.as_deref(), Some("s3://docs/report.pdf"));

    let cf = helpers::chunk_fields(db.path(), "chunk-1");
    assert_eq!(cf.content_hash.as_deref(), Some("sha256:deadbeef"));
}
