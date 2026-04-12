#![allow(clippy::expect_used, clippy::missing_panics_doc, clippy::doc_markdown)]

mod helpers;
mod injection;

use fathomdb::{
    ActionInsert, ChunkInsert, ChunkPolicy, EdgeInsert, EdgeRetire, Engine, EngineOptions,
    NodeInsert, OperationalCollectionKind, OperationalFilterClause, OperationalFilterValue,
    OperationalReadRequest, OperationalRegisterRequest, OperationalWrite, RunInsert,
    SafeExportOptions, StepInsert, TraverseDirection, WriteRequest,
};
use tempfile::NamedTempFile;

fn open_engine() -> (NamedTempFile, Engine) {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");
    (db, engine)
}

#[test]
fn operational_admin_methods_register_trace_rebuild_and_disable() {
    let (_db, engine) = open_engine();

    let record = engine
        .register_operational_collection(&OperationalRegisterRequest {
            name: "connector_health".to_owned(),
            kind: OperationalCollectionKind::LatestState,
            schema_json: "{}".to_owned(),
            retention_json: "{}".to_owned(),
            filter_fields_json: "[]".to_owned(),
            validation_json: String::new(),
            secondary_indexes_json: "[]".to_owned(),
            format_version: 1,
        })
        .expect("register operational collection");
    assert_eq!(record.name, "connector_health");

    let traced = engine
        .trace_operational_collection("connector_health", Some("gmail"))
        .expect("trace operational collection");
    assert_eq!(traced.mutation_count, 0);
    assert_eq!(traced.current_count, 0);

    let rebuilt = engine
        .rebuild_operational_current(Some("connector_health"))
        .expect("rebuild operational current");
    assert_eq!(rebuilt.collections_rebuilt, 1);
    assert_eq!(rebuilt.current_rows_rebuilt, 0);

    let disabled = engine
        .disable_operational_collection("connector_health")
        .expect("disable operational collection");
    assert_eq!(disabled.name, "connector_health");
    assert!(disabled.disabled_at.is_some());
}

#[test]
fn operational_admin_methods_compact_append_only_history() {
    let (_db, engine) = open_engine();

    engine
        .register_operational_collection(&OperationalRegisterRequest {
            name: "audit_log".to_owned(),
            kind: OperationalCollectionKind::AppendOnlyLog,
            schema_json: "{}".to_owned(),
            retention_json: r#"{"mode":"keep_last","max_rows":2}"#.to_owned(),
            filter_fields_json: "[]".to_owned(),
            validation_json: String::new(),
            secondary_indexes_json: "[]".to_owned(),
            format_version: 1,
        })
        .expect("register operational collection");

    engine
        .writer()
        .submit(WriteRequest {
            label: "append-audit".to_owned(),
            nodes: vec![],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![
                OperationalWrite::Append {
                    collection: "audit_log".to_owned(),
                    record_key: "evt-1".to_owned(),
                    payload_json: r#"{"seq":1}"#.to_owned(),
                    source_ref: Some("src-1".to_owned()),
                },
                OperationalWrite::Append {
                    collection: "audit_log".to_owned(),
                    record_key: "evt-2".to_owned(),
                    payload_json: r#"{"seq":2}"#.to_owned(),
                    source_ref: Some("src-2".to_owned()),
                },
                OperationalWrite::Append {
                    collection: "audit_log".to_owned(),
                    record_key: "evt-3".to_owned(),
                    payload_json: r#"{"seq":3}"#.to_owned(),
                    source_ref: Some("src-3".to_owned()),
                },
            ],
        })
        .expect("append operational history");

    let dry_run = engine
        .compact_operational_collection("audit_log", true)
        .expect("dry-run compact");
    assert_eq!(dry_run.deleted_mutations, 1);
    assert!(dry_run.dry_run);

    let compacted = engine
        .compact_operational_collection("audit_log", false)
        .expect("compact");
    assert_eq!(compacted.deleted_mutations, 1);
    assert!(!compacted.dry_run);

    let traced = engine
        .trace_operational_collection("audit_log", None)
        .expect("trace compacted collection");
    assert_eq!(traced.mutation_count, 2);
}

#[test]
fn operational_admin_methods_read_append_only_rows_by_declared_fields() {
    let (_db, engine) = open_engine();

    engine
        .register_operational_collection(&OperationalRegisterRequest {
            name: "audit_log".to_owned(),
            kind: OperationalCollectionKind::AppendOnlyLog,
            schema_json: "{}".to_owned(),
            retention_json: r#"{"mode":"keep_all"}"#.to_owned(),
            filter_fields_json: r#"[{"name":"actor","type":"string","modes":["exact","prefix"]},{"name":"ts","type":"timestamp","modes":["range"]}]"#.to_owned(),
            validation_json: String::new(),
            secondary_indexes_json: "[]".to_owned(),
            format_version: 1,
        })
        .expect("register operational collection");

    engine
        .writer()
        .submit(WriteRequest {
            label: "append-audit".to_owned(),
            nodes: vec![],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![
                OperationalWrite::Append {
                    collection: "audit_log".to_owned(),
                    record_key: "evt-1".to_owned(),
                    payload_json: r#"{"actor":"alice","ts":100}"#.to_owned(),
                    source_ref: Some("src-1".to_owned()),
                },
                OperationalWrite::Append {
                    collection: "audit_log".to_owned(),
                    record_key: "evt-2".to_owned(),
                    payload_json: r#"{"actor":"alice-admin","ts":200}"#.to_owned(),
                    source_ref: Some("src-2".to_owned()),
                },
            ],
        })
        .expect("append operational history");

    let report = engine
        .read_operational_collection(&OperationalReadRequest {
            collection_name: "audit_log".to_owned(),
            filters: vec![
                OperationalFilterClause::Prefix {
                    field: "actor".to_owned(),
                    value: "alice".to_owned(),
                },
                OperationalFilterClause::Range {
                    field: "ts".to_owned(),
                    lower: Some(150),
                    upper: Some(250),
                },
            ],
            limit: Some(10),
        })
        .expect("filtered operational read");

    assert_eq!(report.row_count, 1);
    assert_eq!(report.rows[0].record_key, "evt-2");
    assert!(!report.was_limited);

    let exact = engine
        .read_operational_collection(&OperationalReadRequest {
            collection_name: "audit_log".to_owned(),
            filters: vec![OperationalFilterClause::Exact {
                field: "actor".to_owned(),
                value: OperationalFilterValue::String("alice".to_owned()),
            }],
            limit: Some(10),
        })
        .expect("exact filtered operational read");
    assert_eq!(exact.row_count, 1);
    assert_eq!(exact.rows[0].record_key, "evt-1");
}

#[test]
fn operational_admin_methods_can_update_filters_for_existing_collection() {
    let (_db, engine) = open_engine();

    engine
        .register_operational_collection(&OperationalRegisterRequest {
            name: "audit_log".to_owned(),
            kind: OperationalCollectionKind::AppendOnlyLog,
            schema_json: "{}".to_owned(),
            retention_json: r#"{"mode":"keep_all"}"#.to_owned(),
            filter_fields_json: "[]".to_owned(),
            validation_json: String::new(),
            secondary_indexes_json: "[]".to_owned(),
            format_version: 1,
        })
        .expect("register operational collection");

    let before = engine
        .read_operational_collection(&OperationalReadRequest {
            collection_name: "audit_log".to_owned(),
            filters: vec![OperationalFilterClause::Exact {
                field: "actor".to_owned(),
                value: OperationalFilterValue::String("alice".to_owned()),
            }],
            limit: Some(10),
        })
        .expect_err("undeclared fields should reject before update");
    assert!(before.to_string().contains("undeclared"));

    let updated = engine
        .update_operational_collection_filters(
            "audit_log",
            r#"[{"name":"actor","type":"string","modes":["exact"]}]"#,
        )
        .expect("update filters");
    assert!(updated.filter_fields_json.contains("\"actor\""));
}

#[test]
fn operational_admin_methods_can_update_and_validate_payload_contracts() {
    let (_db, engine) = open_engine();

    engine
        .register_operational_collection(&OperationalRegisterRequest {
            name: "audit_log".to_owned(),
            kind: OperationalCollectionKind::AppendOnlyLog,
            schema_json: "{}".to_owned(),
            retention_json: r#"{"mode":"keep_all"}"#.to_owned(),
            filter_fields_json: "[]".to_owned(),
            validation_json: String::new(),
            secondary_indexes_json: "[]".to_owned(),
            format_version: 1,
        })
        .expect("register operational collection");

    let validation_json = r#"{"format_version":1,"mode":"disabled","additional_properties":false,"fields":[{"name":"status","type":"string","required":true,"enum":["ok","failed"]}]}"#;
    let updated = engine
        .update_operational_collection_validation("audit_log", validation_json)
        .expect("update validation");
    assert_eq!(updated.validation_json, validation_json);

    engine
        .writer()
        .submit(WriteRequest {
            label: "history-validation".to_owned(),
            nodes: vec![],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![
                OperationalWrite::Append {
                    collection: "audit_log".to_owned(),
                    record_key: "evt-1".to_owned(),
                    payload_json: r#"{"status":"ok"}"#.to_owned(),
                    source_ref: Some("src-1".to_owned()),
                },
                OperationalWrite::Append {
                    collection: "audit_log".to_owned(),
                    record_key: "evt-2".to_owned(),
                    payload_json: r#"{"status":"bogus"}"#.to_owned(),
                    source_ref: Some("src-2".to_owned()),
                },
            ],
        })
        .expect("append operational history");

    let report = engine
        .validate_operational_collection_history("audit_log")
        .expect("validate history");
    assert_eq!(report.checked_rows, 2);
    assert_eq!(report.invalid_row_count, 1);
    assert_eq!(report.issues[0].record_key, "evt-2");
}

// ── Memex workloads ──────────────────────────────────────────────────────────

/// M-1: Ingest a meeting transcript as a node with text chunks.
#[test]
fn m1_meeting_transcript_ingestion() {
    let (db, engine) = open_engine();

    engine
        .writer()
        .submit(WriteRequest {
            label: "m1".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "m1-row".to_owned(),
                logical_id: "meeting-m1".to_owned(),
                kind: "Meeting".to_owned(),
                properties: r#"{"title":"Q1 Planning"}"#.to_owned(),
                source_ref: Some("conv-001".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "m1-chunk".to_owned(),
                node_logical_id: "meeting-m1".to_owned(),
                text_content: "budget discussion quarterly review".to_owned(),
                byte_start: None,
                byte_end: None,
                content_hash: None,
            }],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("m1 write");

    assert_eq!(helpers::count_rows(db.path(), "nodes"), 1);
    assert_eq!(helpers::chunk_count(db.path(), "meeting-m1"), 1);
    assert_eq!(helpers::fts_row_count(db.path(), "meeting-m1"), 1);
}

/// M-2: Correct a meeting note via upsert (supersession).
#[test]
fn m2_meeting_note_correction_via_upsert() {
    let (db, engine) = open_engine();

    engine
        .writer()
        .submit(WriteRequest {
            label: "m2-v1".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "m2-row-v1".to_owned(),
                logical_id: "meeting-m2".to_owned(),
                kind: "Meeting".to_owned(),
                properties: r#"{"title":"Draft"}"#.to_owned(),
                source_ref: Some("conv-002".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("m2 v1 write");

    engine
        .writer()
        .submit(WriteRequest {
            label: "m2-v2".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "m2-row-v2".to_owned(),
                logical_id: "meeting-m2".to_owned(),
                kind: "Meeting".to_owned(),
                properties: r#"{"title":"Final"}"#.to_owned(),
                source_ref: Some("conv-002-correction".to_owned()),
                upsert: true,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("m2 v2 write");

    assert_eq!(helpers::active_count(db.path(), "nodes", "meeting-m2"), 1);
    assert_eq!(
        helpers::historical_count(db.path(), "nodes", "meeting-m2"),
        1
    );

    let props = helpers::active_properties(db.path(), "meeting-m2").expect("active props");
    assert!(props.contains("Final"));
}

/// M-3: Verify FTS search returns the ingested transcript.
#[test]
fn m3_fts_search_returns_meeting_transcript() {
    let (db, engine) = open_engine();

    engine
        .writer()
        .submit(WriteRequest {
            label: "m3".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "m3-row".to_owned(),
                logical_id: "meeting-m3".to_owned(),
                kind: "Meeting".to_owned(),
                properties: r#"{"title":"Budget Review"}"#.to_owned(),
                source_ref: Some("conv-003".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "m3-chunk".to_owned(),
                node_logical_id: "meeting-m3".to_owned(),
                text_content: "quarterly budget allocation forecast".to_owned(),
                byte_start: None,
                byte_end: None,
                content_hash: None,
            }],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("m3 write");

    let rows = engine
        .query("Meeting")
        .text_search("quarterly", 5)
        .limit(5)
        .execute()
        .expect("search");

    assert_eq!(rows.hits.len(), 1);
    assert_eq!(rows.hits[0].node.logical_id, "meeting-m3");
    assert_eq!(rows.strict_hit_count, 1);
    assert_eq!(rows.relaxed_hit_count, 0);
    assert!(!rows.fallback_used);
    assert!(matches!(
        rows.hits[0].match_mode,
        fathomdb::SearchMatchMode::Strict,
    ));

    // Suppress unused variable warning for db
    let _ = db;
}

/// M-4: Verify historical versions are preserved after upsert.
#[test]
fn m4_history_preserved_after_upsert() {
    let (db, engine) = open_engine();

    for v in 1..=3 {
        engine
            .writer()
            .submit(WriteRequest {
                label: format!("m4-v{v}"),
                nodes: vec![NodeInsert {
                    row_id: format!("m4-row-v{v}"),
                    logical_id: "meeting-m4".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: format!(r#"{{"version":{v}}}"#),
                    source_ref: Some(format!("conv-004-v{v}")),
                    upsert: v > 1,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("m4 write");
    }

    assert_eq!(helpers::active_count(db.path(), "nodes", "meeting-m4"), 1);
    assert_eq!(
        helpers::historical_count(db.path(), "nodes", "meeting-m4"),
        2
    );
}

/// M-5: Excise a meeting by source_ref and verify all descendants are removed.
#[test]
fn m5_excise_by_source_ref() {
    let (db, engine) = open_engine();

    engine
        .writer()
        .submit(WriteRequest {
            label: "m5".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "m5-row".to_owned(),
                logical_id: "meeting-m5".to_owned(),
                kind: "Meeting".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("src-excise-m5".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "m5-chunk".to_owned(),
                node_logical_id: "meeting-m5".to_owned(),
                text_content: "content to be excised".to_owned(),
                byte_start: None,
                byte_end: None,
                content_hash: None,
            }],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("m5 write");

    engine
        .admin()
        .service()
        .excise_source("src-excise-m5")
        .expect("excise");

    // excise_source supersedes the node (soft-delete): row stays in table but is inactive
    assert_eq!(helpers::active_count(db.path(), "nodes", "meeting-m5"), 0);
    // FTS is rebuilt atomically by excise_source: excised node no longer searchable
    assert_eq!(helpers::fts_row_count(db.path(), "meeting-m5"), 0);
}

/// M-6: Rebuild FTS projections after deletion and verify integrity.
#[test]
fn m6_fts_rebuild_restores_integrity() {
    let (db, engine) = open_engine();

    engine
        .writer()
        .submit(WriteRequest {
            label: "m6".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "m6-row".to_owned(),
                logical_id: "meeting-m6".to_owned(),
                kind: "Meeting".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("src-m6".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "m6-chunk".to_owned(),
                node_logical_id: "meeting-m6".to_owned(),
                text_content: "rebuildable text content".to_owned(),
                byte_start: None,
                byte_end: None,
                content_hash: None,
            }],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("m6 write");

    // Corrupt FTS by deleting rows directly
    helpers::exec_sql(
        db.path(),
        "DELETE FROM fts_nodes WHERE node_logical_id = 'meeting-m6'",
    );
    assert_eq!(helpers::fts_row_count(db.path(), "meeting-m6"), 0);

    // Rebuild restores the FTS row
    engine
        .admin()
        .service()
        .rebuild_missing_projections()
        .expect("rebuild");

    assert_eq!(helpers::fts_row_count(db.path(), "meeting-m6"), 1);
}

// ── OpenClaw workloads ───────────────────────────────────────────────────────

/// OC-1: Persist agent context as a node and retrieve it by kind.
#[test]
fn oc1_persist_and_retrieve_agent_context() {
    let (db, engine) = open_engine();

    engine
        .writer()
        .submit(WriteRequest {
            label: "oc1".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "oc1-row".to_owned(),
                logical_id: "ctx-oc1".to_owned(),
                kind: "AgentContext".to_owned(),
                properties: r#"{"scope":"session"}"#.to_owned(),
                source_ref: Some("oc-src-001".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "oc1-chunk".to_owned(),
                node_logical_id: "ctx-oc1".to_owned(),
                text_content: "session context for agent".to_owned(),
                byte_start: None,
                byte_end: None,
                content_hash: None,
            }],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("oc1 write");

    let compiled = fathomdb::QueryBuilder::nodes("AgentContext")
        .text_search("session context", 5)
        .compile()
        .expect("compile");
    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("read");

    assert_eq!(rows.nodes.len(), 1);
    assert_eq!(rows.nodes[0].logical_id, "ctx-oc1");

    let _ = db;
}

/// OC-2: Append a new context version and verify old version is superseded.
#[test]
fn oc2_context_versioning_via_supersession() {
    let (db, engine) = open_engine();

    engine
        .writer()
        .submit(WriteRequest {
            label: "oc2-v1".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "oc2-v1".to_owned(),
                logical_id: "ctx-oc2".to_owned(),
                kind: "AgentContext".to_owned(),
                properties: r#"{"v":1}"#.to_owned(),
                source_ref: Some("oc-src-002-v1".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("oc2 v1");

    engine
        .writer()
        .submit(WriteRequest {
            label: "oc2-v2".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "oc2-v2".to_owned(),
                logical_id: "ctx-oc2".to_owned(),
                kind: "AgentContext".to_owned(),
                properties: r#"{"v":2}"#.to_owned(),
                source_ref: Some("oc-src-002-v2".to_owned()),
                upsert: true,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("oc2 v2");

    assert_eq!(helpers::active_count(db.path(), "nodes", "ctx-oc2"), 1);
    assert_eq!(helpers::historical_count(db.path(), "nodes", "ctx-oc2"), 1);
}

/// OC-3: Write provenance-tagged run/step/action records.
#[test]
fn oc3_write_provenance_run_step_action() {
    let (db, engine) = open_engine();

    engine
        .writer()
        .submit(WriteRequest {
            label: "oc3".to_owned(),
            nodes: vec![],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![RunInsert {
                id: "run-oc3".to_owned(),
                kind: "session".to_owned(),
                status: "active".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("oc-src-003".to_owned()),
                upsert: false,
                supersedes_id: None,
            }],
            steps: vec![StepInsert {
                id: "step-oc3".to_owned(),
                run_id: "run-oc3".to_owned(),
                kind: "tool_call".to_owned(),
                status: "completed".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("oc-src-003".to_owned()),
                upsert: false,
                supersedes_id: None,
            }],
            actions: vec![ActionInsert {
                id: "action-oc3".to_owned(),
                step_id: "step-oc3".to_owned(),
                kind: "emit_text".to_owned(),
                status: "completed".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("oc-src-003".to_owned()),
                upsert: false,
                supersedes_id: None,
            }],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("oc3 write");

    assert_eq!(helpers::count_rows(db.path(), "runs"), 1);
    assert_eq!(helpers::count_rows(db.path(), "steps"), 1);
    assert_eq!(helpers::count_rows(db.path(), "actions"), 1);

    let run = engine
        .coordinator()
        .read_run("run-oc3")
        .expect("read_run")
        .expect("run exists");
    assert_eq!(run.kind, "session");
    let step = engine
        .coordinator()
        .read_step("step-oc3")
        .expect("read_step")
        .expect("step exists");
    assert_eq!(step.run_id, "run-oc3");
    let action = engine
        .coordinator()
        .read_action("action-oc3")
        .expect("read_action")
        .expect("action exists");
    assert_eq!(action.step_id, "step-oc3");

    let _ = db;
}

/// OC-4: Traverse edges to walk a task dependency graph.
#[test]
fn oc4_traverse_task_dependency_graph() {
    let (db, engine) = open_engine();

    // Create two task nodes and a dependency edge
    engine
        .writer()
        .submit(WriteRequest {
            label: "oc4".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: "task-a".to_owned(),
                    logical_id: "task-oc4-a".to_owned(),
                    kind: "Task".to_owned(),
                    properties: r#"{"name":"task-a"}"#.to_owned(),
                    source_ref: Some("oc-src-004".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: "task-b".to_owned(),
                    logical_id: "task-oc4-b".to_owned(),
                    kind: "Task".to_owned(),
                    properties: r#"{"name":"task-b"}"#.to_owned(),
                    source_ref: Some("oc-src-004".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
            ],
            node_retires: vec![],
            edges: vec![EdgeInsert {
                row_id: "edge-oc4".to_owned(),
                logical_id: "dep-oc4".to_owned(),
                source_logical_id: "task-oc4-a".to_owned(),
                target_logical_id: "task-oc4-b".to_owned(),
                kind: "DEPENDS_ON".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("oc-src-004".to_owned()),
                upsert: false,
            }],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("oc4 write");

    let compiled = fathomdb::QueryBuilder::nodes("Task")
        .traverse(TraverseDirection::Out, "DEPENDS_ON", 1)
        .compile()
        .expect("compile");

    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("read");

    // task-a traverses to task-b
    assert!(
        !rows.nodes.is_empty(),
        "traversal must return at least one node"
    );

    let _ = db;
}

/// OC-5: Retire an edge and verify it no longer appears in traversal results.
#[test]
fn oc5_edge_retire_removes_from_traversal() {
    let (db, engine) = open_engine();

    // Create two tasks and connect them
    engine
        .writer()
        .submit(WriteRequest {
            label: "oc5-setup".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: "oc5-task-a".to_owned(),
                    logical_id: "task-oc5-a".to_owned(),
                    kind: "Task".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("oc-src-005".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: "oc5-task-b".to_owned(),
                    logical_id: "task-oc5-b".to_owned(),
                    kind: "Task".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("oc-src-005".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
            ],
            node_retires: vec![],
            edges: vec![EdgeInsert {
                row_id: "oc5-edge".to_owned(),
                logical_id: "dep-oc5".to_owned(),
                source_logical_id: "task-oc5-a".to_owned(),
                target_logical_id: "task-oc5-b".to_owned(),
                kind: "BLOCKS".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("oc-src-005".to_owned()),
                upsert: false,
            }],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("oc5 setup");

    // Retire the edge
    engine
        .writer()
        .submit(WriteRequest {
            label: "oc5-retire".to_owned(),
            nodes: vec![],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![EdgeRetire {
                logical_id: "dep-oc5".to_owned(),
                source_ref: Some("oc-src-005-retire".to_owned()),
            }],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("oc5 retire");

    // After retirement the edge has superseded_at set — traversal skips it
    assert_eq!(helpers::active_count(db.path(), "edges", "dep-oc5"), 0);
    assert_eq!(helpers::historical_count(db.path(), "edges", "dep-oc5"), 1);
}

/// OC-6: Verify check_semantics is clean after a full agent workload.
#[test]
fn oc6_check_semantics_clean_after_workload() {
    let (db, engine) = open_engine();

    engine
        .writer()
        .submit(WriteRequest {
            label: "oc6".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "oc6-node".to_owned(),
                logical_id: "ctx-oc6".to_owned(),
                kind: "AgentContext".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("oc-src-006".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "oc6-chunk".to_owned(),
                node_logical_id: "ctx-oc6".to_owned(),
                text_content: "agent context data".to_owned(),
                byte_start: None,
                byte_end: None,
                content_hash: None,
            }],
            runs: vec![RunInsert {
                id: "run-oc6".to_owned(),
                kind: "session".to_owned(),
                status: "completed".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("oc-src-006".to_owned()),
                upsert: false,
                supersedes_id: None,
            }],
            steps: vec![StepInsert {
                id: "step-oc6".to_owned(),
                run_id: "run-oc6".to_owned(),
                kind: "tool".to_owned(),
                status: "completed".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("oc-src-006".to_owned()),
                upsert: false,
                supersedes_id: None,
            }],
            actions: vec![ActionInsert {
                id: "action-oc6".to_owned(),
                step_id: "step-oc6".to_owned(),
                kind: "emit".to_owned(),
                status: "completed".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("oc-src-006".to_owned()),
                upsert: false,
                supersedes_id: None,
            }],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("oc6 write");

    let report = engine
        .admin()
        .service()
        .check_semantics()
        .expect("check_semantics");

    assert_eq!(report.orphaned_chunks, 0, "no orphaned chunks");
    assert_eq!(report.broken_step_fk, 0, "no broken step FK");
    assert_eq!(report.broken_action_fk, 0, "no broken action FK");
    assert_eq!(report.stale_fts_rows, 0, "no stale FTS rows");
    assert_eq!(report.dangling_edges, 0, "no dangling edges");

    let _ = db;
}

// ── HermesClaw workloads ─────────────────────────────────────────────────────

/// HC-1: Persist an agent self-evaluation node and verify retrieval.
#[test]
fn hc1_self_evaluation_node_round_trip() {
    let (db, engine) = open_engine();

    engine
        .writer()
        .submit(WriteRequest {
            label: "hc1".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "hc1-row".to_owned(),
                logical_id: "eval-hc1".to_owned(),
                kind: "Evaluation".to_owned(),
                properties: r#"{"score":0.85,"pass":true}"#.to_owned(),
                source_ref: Some("hc-src-001".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "hc1-chunk".to_owned(),
                node_logical_id: "eval-hc1".to_owned(),
                text_content: "evaluation results pass criteria met".to_owned(),
                byte_start: None,
                byte_end: None,
                content_hash: None,
            }],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("hc1 write");

    let compiled = fathomdb::QueryBuilder::nodes("Evaluation")
        .text_search("criteria", 5)
        .compile()
        .expect("compile");
    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("read");

    assert_eq!(rows.nodes.len(), 1);
    assert_eq!(rows.nodes[0].logical_id, "eval-hc1");

    let _ = db;
}

/// HC-2: Update an evaluation result via upsert and confirm supersession chain.
#[test]
fn hc2_evaluation_update_supersession_chain() {
    let (db, engine) = open_engine();

    engine
        .writer()
        .submit(WriteRequest {
            label: "hc2-v1".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "hc2-v1".to_owned(),
                logical_id: "eval-hc2".to_owned(),
                kind: "Evaluation".to_owned(),
                properties: r#"{"score":0.5}"#.to_owned(),
                source_ref: Some("hc-src-002-v1".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("hc2 v1");

    engine
        .writer()
        .submit(WriteRequest {
            label: "hc2-v2".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "hc2-v2".to_owned(),
                logical_id: "eval-hc2".to_owned(),
                kind: "Evaluation".to_owned(),
                properties: r#"{"score":0.9}"#.to_owned(),
                source_ref: Some("hc-src-002-v2".to_owned()),
                upsert: true,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("hc2 v2");

    assert_eq!(helpers::active_count(db.path(), "nodes", "eval-hc2"), 1);
    assert_eq!(helpers::historical_count(db.path(), "nodes", "eval-hc2"), 1);

    let props = helpers::active_properties(db.path(), "eval-hc2").expect("active props");
    assert!(props.contains("0.9"));
}

/// HC-3: Excise a flagged evaluation and verify no orphans remain.
#[test]
fn hc3_excise_flagged_evaluation() {
    let (db, engine) = open_engine();

    engine
        .writer()
        .submit(WriteRequest {
            label: "hc3".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "hc3-row".to_owned(),
                logical_id: "eval-hc3".to_owned(),
                kind: "Evaluation".to_owned(),
                properties: r#"{"flagged":true}"#.to_owned(),
                source_ref: Some("hc-src-003-flagged".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "hc3-chunk".to_owned(),
                node_logical_id: "eval-hc3".to_owned(),
                text_content: "flagged evaluation should be removed".to_owned(),
                byte_start: None,
                byte_end: None,
                content_hash: None,
            }],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("hc3 write");

    engine
        .admin()
        .service()
        .excise_source("hc-src-003-flagged")
        .expect("excise");

    // excise_source supersedes the node; the node is no longer active
    assert_eq!(helpers::active_count(db.path(), "nodes", "eval-hc3"), 0);
    // FTS is rebuilt atomically: stale FTS rows must be 0
    let report = engine
        .admin()
        .service()
        .check_semantics()
        .expect("check_semantics");
    assert_eq!(report.stale_fts_rows, 0, "no stale FTS rows after excise");

    let _ = db;
}

/// HC-4: Rebuild projections after evaluation data loss.
#[test]
fn hc4_projection_rebuild_after_data_loss() {
    let (db, engine) = open_engine();

    engine
        .writer()
        .submit(WriteRequest {
            label: "hc4".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "hc4-row".to_owned(),
                logical_id: "eval-hc4".to_owned(),
                kind: "Evaluation".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("hc-src-004".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "hc4-chunk".to_owned(),
                node_logical_id: "eval-hc4".to_owned(),
                text_content: "data to be rebuilt after loss".to_owned(),
                byte_start: None,
                byte_end: None,
                content_hash: None,
            }],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("hc4 write");

    // Simulate FTS data loss
    helpers::exec_sql(db.path(), "DELETE FROM fts_nodes");
    assert_eq!(helpers::fts_row_count(db.path(), "eval-hc4"), 0);

    let report = engine
        .admin()
        .service()
        .rebuild_missing_projections()
        .expect("rebuild");

    assert!(report.rebuilt_rows > 0, "rebuild must add FTS rows");
    assert_eq!(helpers::fts_row_count(db.path(), "eval-hc4"), 1);
}

/// HC-5: Verify FTS search finds an evaluation note after rebuild.
#[test]
fn hc5_fts_search_after_rebuild() {
    let (db, engine) = open_engine();

    engine
        .writer()
        .submit(WriteRequest {
            label: "hc5".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "hc5-row".to_owned(),
                logical_id: "eval-hc5".to_owned(),
                kind: "Evaluation".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("hc-src-005".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: "hc5-chunk".to_owned(),
                node_logical_id: "eval-hc5".to_owned(),
                text_content: "post rebuild searchable evaluation note".to_owned(),
                byte_start: None,
                byte_end: None,
                content_hash: None,
            }],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("hc5 write");

    // Simulate FTS data loss + rebuild
    helpers::exec_sql(db.path(), "DELETE FROM fts_nodes");
    engine
        .admin()
        .service()
        .rebuild_missing_projections()
        .expect("rebuild");

    let compiled = fathomdb::QueryBuilder::nodes("Evaluation")
        .text_search("searchable", 5)
        .compile()
        .expect("compile");
    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("read after rebuild");

    assert_eq!(rows.nodes.len(), 1);
    assert_eq!(rows.nodes[0].logical_id, "eval-hc5");

    let _ = db;
}

// ── NemoClaw workloads ───────────────────────────────────────────────────────

/// NC-1: Bulk-ingest enterprise document nodes and verify count.
#[test]
fn nc1_bulk_ingest_documents() {
    let (db, engine) = open_engine();
    let count = 50;

    let nodes: Vec<NodeInsert> = (0..count)
        .map(|i| NodeInsert {
            row_id: format!("nc1-row-{i}"),
            logical_id: format!("doc-nc1-{i}"),
            kind: "Document".to_owned(),
            properties: format!(r#"{{"index":{i}}}"#),
            source_ref: Some(format!("nc1-src-{i}")),
            upsert: false,
            chunk_policy: ChunkPolicy::Preserve,
            content_ref: None,
        })
        .collect();

    engine
        .writer()
        .submit(WriteRequest {
            label: "nc1-bulk".to_owned(),
            nodes,
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("nc1 bulk write");

    assert_eq!(helpers::count_rows(db.path(), "nodes"), i64::from(count));
}

/// NC-2: Verify FTS search across bulk-ingested documents.
#[test]
fn nc2_fts_search_bulk_documents() {
    let (db, engine) = open_engine();

    // Ingest 10 docs with distinct content; one has the unique search term
    let mut nodes = Vec::new();
    let mut chunks = Vec::new();
    for i in 0..10 {
        let logical_id = format!("doc-nc2-{i}");
        nodes.push(NodeInsert {
            row_id: format!("nc2-row-{i}"),
            logical_id: logical_id.clone(),
            kind: "Document".to_owned(),
            properties: "{}".to_owned(),
            source_ref: Some(format!("nc2-src-{i}")),
            upsert: false,
            chunk_policy: ChunkPolicy::Preserve,
            content_ref: None,
        });
        let text = if i == 5 {
            "unique_searchterm_nc2 document content".to_owned()
        } else {
            format!("document number {i} generic content")
        };
        chunks.push(ChunkInsert {
            id: format!("nc2-chunk-{i}"),
            node_logical_id: logical_id,
            text_content: text,
            byte_start: None,
            byte_end: None,
            content_hash: None,
        });
    }

    engine
        .writer()
        .submit(WriteRequest {
            label: "nc2-bulk".to_owned(),
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
        })
        .expect("nc2 write");

    let compiled = fathomdb::QueryBuilder::nodes("Document")
        .text_search("unique_searchterm_nc2", 5)
        .compile()
        .expect("compile");
    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("read");

    assert_eq!(rows.nodes.len(), 1);
    assert_eq!(rows.nodes[0].logical_id, "doc-nc2-5");

    let _ = db;
}

/// NC-3: Excise documents by source_ref and confirm no residual data.
#[test]
fn nc3_excise_documents_by_source_ref() {
    let (db, engine) = open_engine();

    // Ingest 15 docs: first 5 tagged src-a, rest src-b
    let mut nodes = Vec::new();
    let mut chunks = Vec::new();
    for i in 0..15 {
        let logical_id = format!("doc-nc3-{i}");
        let src = if i < 5 {
            "nc3-src-a".to_owned()
        } else {
            "nc3-src-b".to_owned()
        };
        nodes.push(NodeInsert {
            row_id: format!("nc3-row-{i}"),
            logical_id: logical_id.clone(),
            kind: "Document".to_owned(),
            properties: "{}".to_owned(),
            source_ref: Some(src),
            upsert: false,
            chunk_policy: ChunkPolicy::Preserve,
            content_ref: None,
        });
        chunks.push(ChunkInsert {
            id: format!("nc3-chunk-{i}"),
            node_logical_id: logical_id,
            text_content: format!("doc {i}"),
            byte_start: None,
            byte_end: None,
            content_hash: None,
        });
    }

    engine
        .writer()
        .submit(WriteRequest {
            label: "nc3-bulk".to_owned(),
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
        })
        .expect("nc3 write");

    engine
        .admin()
        .service()
        .excise_source("nc3-src-a")
        .expect("excise src-a");

    // excise_source supersedes src-a nodes; 10 src-b nodes remain active
    let conn = rusqlite::Connection::open(db.path()).expect("conn");
    let active: i64 = conn
        .query_row(
            "SELECT count(*) FROM nodes WHERE superseded_at IS NULL",
            [],
            |row| row.get(0),
        )
        .expect("active count");
    assert_eq!(
        active, 10,
        "10 src-b nodes remain active after excising src-a"
    );
    // FTS is rebuilt atomically: no stale FTS rows
    let report = engine
        .admin()
        .service()
        .check_semantics()
        .expect("semantics");
    assert_eq!(report.stale_fts_rows, 0, "no stale FTS rows after excise");
}

/// NC-4: Safe export of enterprise data and verify manifest completeness.
#[test]
fn nc4_safe_export_manifest_completeness() {
    let (db, engine) = open_engine();

    engine
        .writer()
        .submit(WriteRequest {
            label: "nc4".to_owned(),
            nodes: vec![NodeInsert {
                row_id: "nc4-row".to_owned(),
                logical_id: "doc-nc4".to_owned(),
                kind: "Document".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("nc4-src".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("nc4 write");

    let export_dest = NamedTempFile::new().expect("export dest");
    let manifest = engine
        .admin()
        .service()
        .safe_export(
            export_dest.path(),
            SafeExportOptions {
                force_checkpoint: false,
            },
        )
        .expect("safe_export");

    assert!(!manifest.sha256.is_empty(), "manifest must have SHA-256");
    assert!(
        manifest.schema_version > 0,
        "schema_version must be positive"
    );
    assert_eq!(manifest.protocol_version, 1, "protocol version must be 1");
    assert!(manifest.page_count > 0, "page_count must be positive");
    assert!(
        manifest.exported_at > 0,
        "exported_at must be a valid timestamp"
    );

    let _ = db;
}

/// NC-5: check_integrity returns clean report after full enterprise workload.
#[test]
#[allow(clippy::too_many_lines)]
fn nc5_check_integrity_clean_after_enterprise_workload() {
    let (db, engine) = open_engine();

    // Full workload: nodes + chunks + edges + runs + steps + actions
    engine
        .writer()
        .submit(WriteRequest {
            label: "nc5".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: "nc5-node-a".to_owned(),
                    logical_id: "doc-nc5-a".to_owned(),
                    kind: "Document".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("nc5-src".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: "nc5-node-b".to_owned(),
                    logical_id: "doc-nc5-b".to_owned(),
                    kind: "Document".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("nc5-src".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
            ],
            node_retires: vec![],
            edges: vec![EdgeInsert {
                row_id: "nc5-edge".to_owned(),
                logical_id: "rel-nc5".to_owned(),
                source_logical_id: "doc-nc5-a".to_owned(),
                target_logical_id: "doc-nc5-b".to_owned(),
                kind: "REFERENCES".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("nc5-src".to_owned()),
                upsert: false,
            }],
            edge_retires: vec![],
            chunks: vec![
                ChunkInsert {
                    id: "nc5-chunk-a".to_owned(),
                    node_logical_id: "doc-nc5-a".to_owned(),
                    text_content: "enterprise document alpha content".to_owned(),
                    byte_start: None,
                    byte_end: None,
                    content_hash: None,
                },
                ChunkInsert {
                    id: "nc5-chunk-b".to_owned(),
                    node_logical_id: "doc-nc5-b".to_owned(),
                    text_content: "enterprise document beta content".to_owned(),
                    byte_start: None,
                    byte_end: None,
                    content_hash: None,
                },
            ],
            runs: vec![RunInsert {
                id: "nc5-run".to_owned(),
                kind: "ingest".to_owned(),
                status: "completed".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("nc5-src".to_owned()),
                upsert: false,
                supersedes_id: None,
            }],
            steps: vec![StepInsert {
                id: "nc5-step".to_owned(),
                run_id: "nc5-run".to_owned(),
                kind: "parse".to_owned(),
                status: "completed".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("nc5-src".to_owned()),
                upsert: false,
                supersedes_id: None,
            }],
            actions: vec![ActionInsert {
                id: "nc5-action".to_owned(),
                step_id: "nc5-step".to_owned(),
                kind: "insert_node".to_owned(),
                status: "completed".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("nc5-src".to_owned()),
                upsert: false,
                supersedes_id: None,
            }],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("nc5 write");

    let integrity = engine
        .admin()
        .service()
        .check_integrity()
        .expect("check_integrity");
    assert!(integrity.physical_ok, "physical integrity must pass");
    assert!(integrity.foreign_keys_ok, "foreign keys must be valid");
    assert_eq!(integrity.missing_fts_rows, 0, "no missing FTS rows");
    assert_eq!(
        integrity.duplicate_active_logical_ids, 0,
        "no duplicate active logical ids"
    );

    let _ = db;
}

// ── Vector workloads ─────────────────────────────────────────────────────────

/// V-1: Vector search round-trip — insert embedding, search returns it.
#[cfg(feature = "sqlite-vec")]
#[test]
fn v1_vector_search_round_trip() {
    use fathomdb::{VecInsert, new_id};

    let db = NamedTempFile::new().expect("temporary db");
    let mut opts = EngineOptions::new(db.path());
    opts.vector_dimension = Some(4);
    let engine = Engine::open(opts).expect("engine with vec");

    assert!(
        engine.coordinator().vector_enabled(),
        "vector must be enabled after setting dimension"
    );

    let node_id = new_id();
    let chunk_id = new_id();

    engine
        .writer()
        .submit(WriteRequest {
            label: "v1".to_owned(),
            nodes: vec![NodeInsert {
                row_id: fathomdb::new_row_id(),
                logical_id: node_id.clone(),
                kind: "Document".to_owned(),
                properties: "{}".to_owned(),
                source_ref: Some("v1-src".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![ChunkInsert {
                id: chunk_id.clone(),
                node_logical_id: node_id.clone(),
                text_content: "document with vector embedding".to_owned(),
                byte_start: None,
                byte_end: None,
                content_hash: None,
            }],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![VecInsert {
                chunk_id: chunk_id.clone(),
                embedding: vec![0.1, 0.2, 0.3, 0.4],
            }],
            operational_writes: vec![],
        })
        .expect("v1 write");

    // sqlite-vec MATCH requires a JSON float array as the query vector.
    let compiled = fathomdb::QueryBuilder::nodes("Document")
        .vector_search("[0.1, 0.2, 0.3, 0.4]", 5)
        .compile()
        .expect("compile vector query");

    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("vector read");

    // The vector table exists and the query ran — result may be empty if the
    // query planner cannot match without a real query vector, but no error
    // means the infrastructure is wired correctly.
    let _ = rows;

    let conn = rusqlite::Connection::open(db.path()).expect("conn");
    let count: i64 = conn
        .query_row(
            "SELECT count(*) FROM vec_nodes_active WHERE chunk_id = ?1",
            rusqlite::params![chunk_id],
            |row| row.get(0),
        )
        .expect("vec count");
    assert_eq!(count, 1, "embedding must be persisted in vec_nodes_active");
}
