//! 0.8.22 Slice 19 — planner pins for canonical FTS-hydration joins.

use fathomdb_engine::{
    Engine, InitialState, PreparedWrite, ProjectionFts, ProjectionRole, ProjectionSpec, SourceId,
};
use fathomdb_schema::SQLITE_SUFFIX;
use rusqlite::Connection;
use std::collections::BTreeSet;
use std::path::PathBuf;
use tempfile::TempDir;

const NODE_CURSOR_INDEX: &str = "canonical_nodes_write_cursor_idx";
const EDGE_CURSOR_INDEX: &str = "canonical_edges_write_cursor_idx";
const ACTIVE_NODE_CURSOR_INDEX: &str = "canonical_nodes_state_active_idx";

fn db_path(dir: &TempDir, name: &str) -> PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

fn node(logical_id: &str, body: &str) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: body.to_string(),
        source_id: SourceId::new("test:slice19").expect("source id"),
        logical_id: Some(logical_id.to_string()),
        state: InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

fn edge(from: &str, to: &str, body: &str) -> PreparedWrite {
    PreparedWrite::Edge {
        kind: "mentions".to_string(),
        from: from.to_string(),
        to: to.to_string(),
        source_id: SourceId::new("test:slice19").expect("source id"),
        logical_id: Some("edge-1".to_string()),
        body: Some(body.to_string()),
        t_valid: None,
        t_invalid: None,
        confidence: None,
        extractor_model_id: None,
        temporal_fallback: None,
    }
}

fn searchable_title() -> ProjectionSpec {
    ProjectionSpec {
        name: "title".to_string(),
        roles: BTreeSet::from([ProjectionRole::Searchable]),
        fts: Some(ProjectionFts { tokenizer: None }),
        vector: None,
        source: None,
    }
}

fn explain_details(connection: &Connection, sql: &str) -> Vec<String> {
    let explain = format!("EXPLAIN QUERY PLAN {sql}");
    let mut statement = connection.prepare(&explain).expect("prepare explain");
    statement
        .query_map([], |row| row.get::<_, String>(3))
        .expect("run explain")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("collect explain details")
}

fn assert_plan_uses(plan: &[String], index: &str) {
    assert!(
        plan.iter().any(|detail| detail.contains(index)),
        "expected planner to use {index}; plan:\n{}",
        plan.join("\n")
    );
}

#[test]
fn body_fts_hydration_plan_uses_unconditional_node_cursor_index() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "body");
    let opened = Engine::open(path.clone()).expect("open");
    opened.engine.write(&[node("node-1", "joinindex body needle")]).expect("write");
    opened.engine.close().expect("close");

    let connection = Connection::open(path).expect("open read connection");
    let plan = explain_details(
        &connection,
        "SELECT search_index.body, search_index.kind, search_index.write_cursor, \
         bm25(search_index), cn.logical_id, cn.source_id FROM search_index \
         LEFT JOIN canonical_nodes cn ON cn.write_cursor = search_index.write_cursor \
         WHERE search_index MATCH 'joinindex' \
           AND cn.superseded_at IS NULL \
           AND (cn.state = 'active' OR cn.state IS NULL) \
           AND (cn.valid_from IS NULL OR cn.valid_from <= 2_000_000_000) \
           AND (cn.valid_until IS NULL OR cn.valid_until > 2_000_000_000) \
         ORDER BY bm25(search_index), search_index.write_cursor",
    );

    assert_plan_uses(&plan, NODE_CURSOR_INDEX);
}

#[test]
fn edge_fts_hydration_plan_uses_unconditional_edge_cursor_index() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "edge");
    let opened = Engine::open(path.clone()).expect("open");
    opened
        .engine
        .write(&[
            node("from", "source node"),
            node("to", "target node"),
            edge("from", "to", "joinindex edge needle"),
        ])
        .expect("write");
    opened.engine.close().expect("close");

    let connection = Connection::open(path).expect("open read connection");
    let plan = explain_details(
        &connection,
        "SELECT sei.body, sei.kind, sei.write_cursor, bm25(search_index_edges), \
         ce.logical_id, ce.source_id \
         FROM search_index_edges sei \
         JOIN canonical_edges ce ON ce.write_cursor = sei.write_cursor \
         WHERE search_index_edges MATCH 'joinindex' \
           AND ce.superseded_at IS NULL \
           AND (ce.t_valid IS NULL OR ce.t_valid <= 2_000_000_000) \
           AND (ce.t_invalid IS NULL OR ce.t_invalid > 2_000_000_000) \
         ORDER BY bm25(search_index_edges), sei.write_cursor",
    );

    assert_plan_uses(&plan, EDGE_CURSOR_INDEX);
}

#[test]
fn projected_text_hydration_plan_retains_active_node_cursor_index() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "projected");
    let opened = Engine::open(path.clone()).expect("open");
    opened.engine.configure_projections(&[searchable_title()], &[]).expect("configure");
    opened
        .engine
        .write(&[node("node-1", r#"{"title":"joinindex projected needle"}"#)])
        .expect("write");
    opened.engine.close().expect("close");

    let connection = Connection::open(path).expect("open read connection");
    let plan = explain_details(
        &connection,
        "SELECT p.write_cursor, bm25(property_search_index), n.kind, n.body, n.logical_id, n.source_id \
         FROM property_search_index p \
         JOIN canonical_nodes n ON n.write_cursor = p.write_cursor \
         WHERE p.attr_name = 'title' AND property_search_index MATCH 'joinindex' \
           AND n.superseded_at IS NULL AND n.state = 'active' \
           AND (n.valid_from IS NULL OR n.valid_from <= 2_000_000_000) \
           AND (n.valid_until IS NULL OR n.valid_until > 2_000_000_000) \
         ORDER BY bm25(property_search_index) ASC, p.write_cursor ASC",
    );

    assert_plan_uses(&plan, ACTIVE_NODE_CURSOR_INDEX);
}
