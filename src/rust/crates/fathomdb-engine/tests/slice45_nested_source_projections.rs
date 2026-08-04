//! 0.8.21 Slice 45 — literal nested-source projection declarations and their
//! public attribute-query contract. These tests use normal engine writes against
//! a real database; SQLite is read only as an at-rest oracle.

use fathomdb_engine::{
    Engine, EngineError, InitialState, ProjectionFts, ProjectionRole, ProjectionSpec, ReadView,
    SearchFilter, SoftFallbackBranch, SourceId,
};
use fathomdb_schema::SQLITE_SUFFIX;
use proptest::prelude::*;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn db_path(dir: &TempDir, name: &str) -> PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

fn roles(items: &[ProjectionRole]) -> BTreeSet<ProjectionRole> {
    items.iter().copied().collect()
}

fn nested_spec(
    name: &str,
    source: &[&str],
    roles_: &[ProjectionRole],
    fts: bool,
) -> ProjectionSpec {
    ProjectionSpec {
        name: name.to_string(),
        roles: roles(roles_),
        fts: fts.then_some(ProjectionFts { tokenizer: None }),
        vector: None,
        source: Some(source.iter().map(|segment| (*segment).to_string()).collect()),
    }
}

fn node(logical_id: &str, source_id: &str, body: &str) -> fathomdb_engine::PreparedWrite {
    fathomdb_engine::PreparedWrite::Node {
        kind: "doc".to_string(),
        body: body.to_string(),
        source_id: SourceId::new(source_id).expect("source id"),
        logical_id: Some(logical_id.to_string()),
        state: InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

fn eav_values(path: &Path, name: &str) -> Vec<String> {
    let conn = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("open read only");
    let values = conn
        .prepare(
            "SELECT attr_value FROM canonical_attributes WHERE attr_name = ?1 ORDER BY attr_value",
        )
        .expect("prepare")
        .query_map([name], |row| row.get::<_, String>(0))
        .expect("query")
        .map(|row| row.expect("row"))
        .collect();
    values
}

#[test]
fn nested_literal_path_round_trips_and_projects_scalar_values() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "nested");
    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;
    let spec = nested_spec(
        "core:deadline",
        &["attributes", "core:deadline", "value.with[punctuation]"],
        &[ProjectionRole::Filterable, ProjectionRole::Searchable],
        true,
    );

    engine.configure_projections(std::slice::from_ref(&spec), &[]).unwrap();
    engine
        .write(&[node(
            "N1",
            "slice45:nested",
            r#"{"attributes":{"core:deadline":{"value.with[punctuation]":"2026-10-01"}}}"#,
        )])
        .unwrap();

    assert_eq!(engine.read_projections().unwrap(), vec![spec]);
    opened.engine.close().unwrap();
    assert_eq!(eav_values(&path, "core:deadline"), vec!["2026-10-01"]);
}

#[test]
fn nested_scalars_use_canonical_text_and_missing_or_null_do_not_project() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "scalars");
    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;
    engine
        .configure_projections(
            &[nested_spec(
                "value",
                &["attributes", "literal:key", "value"],
                &[ProjectionRole::Filterable, ProjectionRole::Searchable],
                true,
            )],
            &[],
        )
        .unwrap();
    engine
        .write(&[
            node("text", "slice45:text", r#"{"attributes":{"literal:key":{"value":"1"}}}"#),
            node("int", "slice45:int", r#"{"attributes":{"literal:key":{"value":1}}}"#),
            node("real", "slice45:real", r#"{"attributes":{"literal:key":{"value":2.5}}}"#),
            node("bool", "slice45:bool", r#"{"attributes":{"literal:key":{"value":true}}}"#),
            node("null", "slice45:null", r#"{"attributes":{"literal:key":{"value":null}}}"#),
            node("missing", "slice45:missing", r#"{"attributes":{"literal:key":{}}}"#),
        ])
        .unwrap();

    let mut filter = SearchFilter::default();
    filter.attributes = vec![("value".to_string(), "1".to_string())];
    let result =
        engine.search_projected_text("1", "value", Some(filter), &ReadView::default()).unwrap();
    assert_eq!(
        result.results.iter().map(|hit| hit.body.as_str()).collect::<Vec<_>>(),
        vec![
            r#"{"attributes":{"literal:key":{"value":"1"}}}"#,
            r#"{"attributes":{"literal:key":{"value":1}}}"#,
        ],
        "canonical text equality deliberately collapses string \"1\" and number 1"
    );
    let mut hybrid_filter = SearchFilter::default();
    hybrid_filter.attributes = vec![("value".to_string(), "1".to_string())];
    let hybrid = engine.search_filtered("1", Some(hybrid_filter)).unwrap();
    assert!(
        hybrid
            .results
            .iter()
            .all(|hit| hit.body.contains(r#"\"value\":\"1\""#)
                || hit.body.contains(r#"\"value\":1"#)),
        "normal hybrid search must retain public projected-attribute filters"
    );
    opened.engine.close().unwrap();
    assert_eq!(eav_values(&path, "value"), vec!["1", "1", "2.5", "true"]);
}

#[test]
fn nested_composite_terminal_rejects_write_and_backfill_atomically() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "composite");
    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;
    let spec = nested_spec(
        "value",
        &["attributes", "core:deadline", "value"],
        &[ProjectionRole::Filterable],
        false,
    );
    engine.configure_projections(std::slice::from_ref(&spec), &[]).unwrap();
    assert_eq!(
        engine.write(&[node(
            "object",
            "slice45:object",
            r#"{"attributes":{"core:deadline":{"value":{"not":"scalar"}}}}"#,
        )]),
        Err(EngineError::WriteValidation)
    );
    assert!(eav_values(&path, "value").is_empty());

    let backfill_path = db_path(&dir, "backfill_composite");
    let backfill = Engine::open(backfill_path.clone()).unwrap();
    backfill
        .engine
        .write(&[node(
            "object",
            "slice45:backfill",
            r#"{"attributes":{"core:deadline":{"value":[]}}}"#,
        )])
        .unwrap();
    assert_eq!(
        backfill.engine.configure_projections(&[spec], &[]),
        Err(EngineError::WriteValidation)
    );
    assert!(backfill.engine.read_projections().unwrap().is_empty());

    let searchable_backfill_path = db_path(&dir, "searchable_backfill_composite");
    let searchable_backfill = Engine::open(searchable_backfill_path).unwrap();
    searchable_backfill
        .engine
        .write(&[node(
            "object",
            "slice45:searchable-backfill",
            r#"{"attributes":{"core:deadline":{"value":{"not":"scalar"}}}}"#,
        )])
        .unwrap();
    let searchable_only = nested_spec(
        "value",
        &["attributes", "core:deadline", "value"],
        &[ProjectionRole::Searchable],
        true,
    );
    assert_eq!(
        searchable_backfill
            .engine
            .configure_projections(&[searchable_only], &[]),
        Err(EngineError::WriteValidation),
        "a nested composite terminal is invalid regardless of whether the projection also wants EAV"
    );
    assert!(searchable_backfill.engine.read_projections().unwrap().is_empty());
}

#[test]
fn projected_text_search_is_field_scoped_filtered_and_text_only() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "search")).unwrap();
    let engine = &opened.engine;
    engine
        .configure_projections(
            &[
                nested_spec(
                    "title",
                    &["attributes", "core:title", "value"],
                    &[ProjectionRole::Filterable, ProjectionRole::Searchable],
                    true,
                ),
                nested_spec(
                    "status",
                    &["attributes", "core:status", "value"],
                    &[ProjectionRole::Filterable],
                    false,
                ),
            ],
            &[],
        )
        .unwrap();
    engine
        .write(&[
            node(
                "A",
                "slice45:a",
                r#"{"body_only":"needle","attributes":{"core:title":{"value":"needle alpha"},"core:status":{"value":"open"}}}"#,
            ),
            node(
                "B",
                "slice45:b",
                r#"{"attributes":{"core:title":{"value":"needle beta"},"core:status":{"value":"closed"}}}"#,
            ),
            node(
                "C",
                "slice45:c",
                r#"{"body_only":"needle","attributes":{"core:title":{"value":"not a match"},"core:status":{"value":"open"}}}"#,
            ),
        ])
        .unwrap();

    let mut filter = SearchFilter::default();
    filter.attributes = vec![("status".to_string(), "open".to_string())];
    let result = engine
        .search_projected_text("needle", "title", Some(filter), &ReadView::default())
        .unwrap();
    assert_eq!(result.soft_fallback, None);
    assert_eq!(result.results.len(), 1);
    assert_eq!(
        result.results[0].body,
        r#"{"body_only":"needle","attributes":{"core:title":{"value":"needle alpha"},"core:status":{"value":"open"}}}"#
    );
    assert_eq!(result.results[0].branch, SoftFallbackBranch::Text);
}

proptest! {
    #[test]
    fn literal_path_segments_round_trip_through_the_registry(
        first in "[a-z0-9:.\\[\\]]{1,12}",
        second in "[a-z0-9:.\\[\\]]{1,12}",
        value in "[a-zA-Z0-9]{1,12}",
    ) {
        let dir = TempDir::new().unwrap();
        let path = db_path(&dir, "property");
        let opened = Engine::open(path.clone()).unwrap();
        let spec = nested_spec("field", &[&first, &second], &[ProjectionRole::Filterable], false);
        opened.engine.configure_projections(std::slice::from_ref(&spec), &[]).unwrap();
        let body = format!(r#"{{\"{first}\":{{\"{second}\":\"{value}\"}}}}"#);
        opened.engine.write(&[node("property", "slice45:property", &body)]).unwrap();
        prop_assert_eq!(opened.engine.read_projections().unwrap(), vec![spec]);
        opened.engine.close().unwrap();
        prop_assert_eq!(eav_values(&path, "field"), vec![value]);
    }
}
