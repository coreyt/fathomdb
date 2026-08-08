//! 0.8.22 Slice 22 — governed projection-runtime status read.
//!
//! These are real-database tests for the status facade.  The durable registry
//! describes declarations; the answer also includes facts of this open engine
//! session, so it must not be reconstructed by re-applying a configuration.

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{
    Engine, InitialState, PreparedWrite, ProjectionRole, ProjectionRuntimeStatus,
    ProjectionRuntimeUnavailabilityReason, ProjectionSpec, ProjectionStatusDenseReadiness,
    ProjectionVector, SourceId,
};
use fathomdb_schema::SQLITE_SUFFIX;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

#[derive(Clone, Debug)]
struct DelayEmbedder {
    identity: EmbedderIdentity,
    delay: Duration,
    divergent: bool,
}

impl DelayEmbedder {
    fn faithful(identity: EmbedderIdentity, delay: Duration) -> Self {
        Self { identity, delay, divergent: false }
    }

    fn divergent(identity: EmbedderIdentity) -> Self {
        Self { identity, delay: Duration::ZERO, divergent: true }
    }
}

impl Embedder for DelayEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        if !self.delay.is_zero() {
            thread::sleep(self.delay);
        }
        let mut vector = vec![0.0_f32; self.identity.dimension as usize];
        vector[usize::from(self.divergent)] = 1.0;
        Ok(vector)
    }
}

fn db_path(dir: &TempDir, name: &str) -> PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

fn roles(values: &[ProjectionRole]) -> BTreeSet<ProjectionRole> {
    values.iter().copied().collect()
}

fn vector_spec(name: &str) -> ProjectionSpec {
    ProjectionSpec {
        name: name.to_string(),
        roles: roles(&[ProjectionRole::Searchable]),
        fts: None,
        vector: Some(ProjectionVector { embedder: None, dense_readiness: None }),
        source: None,
    }
}

fn filterable_spec(name: &str) -> ProjectionSpec {
    ProjectionSpec {
        name: name.to_string(),
        roles: roles(&[ProjectionRole::Filterable]),
        fts: None,
        vector: None,
        source: None,
    }
}

fn node(kind: &str, logical_id: &str) -> PreparedWrite {
    PreparedWrite::Node {
        kind: kind.to_string(),
        body: r#"{"alpha":"dense meaning","zeta":"plain value"}"#.to_string(),
        source_id: SourceId::new("test:slice22-status").expect("source id"),
        logical_id: Some(logical_id.to_string()),
        state: InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

fn ro(path: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("open read-only")
}

fn stored_default_identity(path: &Path) -> EmbedderIdentity {
    ro(path)
        .query_row(
            "SELECT name, revision, dimension FROM _fathomdb_embedder_profiles WHERE profile = 'default'",
            [],
            |row| {
                Ok(EmbedderIdentity::new(
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u32>(2)?,
                ))
            },
        )
        .expect("stored default identity")
}

#[derive(Debug, Eq, PartialEq)]
struct DurableSnapshot {
    registry: i64,
    vector_kinds: i64,
    terminals: i64,
    vector_rows: i64,
    projection_state: Vec<(String, i64, i64)>,
}

fn durable_snapshot(path: &Path) -> DurableSnapshot {
    let connection = ro(path);
    let count = |table: &str| -> i64 {
        connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))
            .expect("table count")
    };
    let projection_state = connection
        .prepare(
            "SELECT kind, last_enqueued_cursor, updated_at
             FROM _fathomdb_projection_state ORDER BY kind",
        )
        .expect("prepare projection-state snapshot")
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .expect("query projection-state snapshot")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("collect projection-state snapshot");
    DurableSnapshot {
        registry: count("_fathomdb_projection_registry"),
        vector_kinds: count("_fathomdb_vector_kinds"),
        terminals: count("_fathomdb_projection_terminal"),
        vector_rows: count("_fathomdb_vector_rows"),
        projection_state,
    }
}

fn entry(status: &ProjectionRuntimeStatus, name: &str) -> ProjectionStatusDenseReadiness {
    status
        .projections
        .iter()
        .find(|entry| entry.name == name)
        .map(|entry| entry.dense_readiness)
        .unwrap_or_else(|| panic!("missing projection {name:?}"))
}

fn seed_legacy_non_searchable_vector(path: &Path, name: &str) {
    let connection = rusqlite::Connection::open(path).expect("open legacy fixture mutation");
    connection
        .execute(
            "INSERT INTO _fathomdb_projection_registry
                 (name, roles, fts_tokenizer, vector_embedder, vector_declared)
             VALUES(?1, 'filterable', NULL, NULL, 1)",
            [name],
        )
        .expect("seed pre-Slice-23 legacy vector row");
}

fn force_probe_verdict_rerun(path: &Path) {
    let connection = rusqlite::Connection::open(path).expect("open probe-cache mutation");
    connection
        .execute(
            "DELETE FROM _fathomdb_open_state WHERE key = 'vector_equivalence_verified_fingerprint'",
            [],
        )
        .expect("clear passing cache so the next open verifies its backend");
}

#[test]
fn status_is_current_sorted_and_pure_over_declarations_and_unsupported_kinds() {
    let dir = TempDir::new().expect("tempdir");
    let path = db_path(&dir, "current_pure");
    let opened = Engine::open(&path).expect("open without runtime");
    let engine = &opened.engine;

    engine
        .configure_projections(&[filterable_spec("zeta"), vector_spec("alpha")], &[])
        .expect("declare projections");
    engine.write(&[node("invoice", "I1")]).expect("write invoice I1");
    engine.write(&[node("doc", "D1")]).expect("write doc D1");
    engine.write(&[node("entity", "E1")]).expect("write entity E1");
    engine.write(&[node("invoice", "I2")]).expect("write invoice I2");

    // With no runtime, node work is deliberately excluded from the dispatcher
    // scan. Drain establishes that this fixture starts idle, then freezing pins
    // `pending_scan` so an accidental status-read wake cannot be consumed before
    // the assertion below observes it.
    engine.drain(30_000).expect("drain no-runtime writes");
    engine.set_projection_scheduler_frozen_for_test(true);
    let scheduler_before = engine.projection_scheduler_pending_scan_for_test();
    assert!(!scheduler_before, "fixture must start with no pending scheduler scan");

    let before = durable_snapshot(&path);
    let first = engine.read_projection_status().expect("first status");
    let second = engine.read_projection_status().expect("second status");

    assert_eq!(first, second, "repeated reads return current facts without a configuration echo");
    assert!(!first.runtime_embedder_available);
    assert_eq!(
        first.runtime_unavailability_reason,
        ProjectionRuntimeUnavailabilityReason::NoRuntime
    );
    assert_eq!(
        first.projections.iter().map(|projection| projection.name.as_str()).collect::<Vec<_>>(),
        ["alpha", "zeta"],
        "entries are sorted by name rather than declaration order"
    );
    assert_eq!(entry(&first, "alpha"), ProjectionStatusDenseReadiness::Unavailable);
    assert_eq!(entry(&first, "zeta"), ProjectionStatusDenseReadiness::NotDeclared);
    assert_eq!(
        first.vector_unsupported_kinds,
        vec!["entity".to_string(), "invoice".to_string()],
        "the current report is sorted and deduplicated across real corpus rows"
    );
    assert_eq!(
        durable_snapshot(&path),
        before,
        "a status read neither changes durable registry/work state nor wakes work that writes it"
    );
    assert_eq!(
        engine.projection_scheduler_pending_scan_for_test(),
        scheduler_before,
        "repeated status reads neither schedule nor wake projection work"
    );

    opened.engine.close().expect("close");
}

#[test]
fn status_covers_embedding_ready_and_the_equivalence_refusal_reason() {
    let dir = TempDir::new().expect("tempdir");
    let path = db_path(&dir, "readiness_and_refusal");
    Engine::open(&path).expect("create profile").engine.close().expect("close profile setup");
    let identity = stored_default_identity(&path);

    let opened = Engine::open_with_embedder_for_test(
        &path,
        Arc::new(DelayEmbedder::faithful(identity.clone(), Duration::from_millis(200))),
    )
    .expect("open usable runtime");
    let engine = &opened.engine;
    engine.configure_projections(&[vector_spec("summary")], &[]).expect("declare vector arm");
    assert_eq!(
        entry(&engine.read_projection_status().expect("empty status"), "summary"),
        ProjectionStatusDenseReadiness::Ready,
        "a usable runtime with no work is ready"
    );

    engine.set_projection_scheduler_frozen_for_test(true);
    engine.write(&[node("doc", "D1")]).expect("write queued node");
    let embedding = engine.read_projection_status().expect("embedding status");
    assert!(embedding.runtime_embedder_available);
    assert_eq!(
        embedding.runtime_unavailability_reason,
        ProjectionRuntimeUnavailabilityReason::None
    );
    assert_eq!(entry(&embedding, "summary"), ProjectionStatusDenseReadiness::Embedding);
    engine.set_projection_scheduler_frozen_for_test(false);
    engine.drain(30_000).expect("drain queued work");
    assert_eq!(
        entry(&engine.read_projection_status().expect("ready status"), "summary"),
        ProjectionStatusDenseReadiness::Ready
    );
    opened.engine.close().expect("close usable runtime");

    let accepted = Engine::open_with_embedder_for_test(
        &path,
        Arc::new(DelayEmbedder::faithful(identity.clone(), Duration::ZERO)),
    )
    .expect("reopen to persist an accepted equivalence baseline");
    assert!(!accepted.report.dense_disabled, "fixture: faithful runtime is accepted");
    accepted.engine.close().expect("close accepted baseline");
    force_probe_verdict_rerun(&path);

    let rejected =
        Engine::open_with_embedder_for_test(&path, Arc::new(DelayEmbedder::divergent(identity)))
            .expect("degraded open stays serviceable");
    let status = rejected.engine.read_projection_status().expect("refusal status");
    assert!(!status.runtime_embedder_available);
    assert_eq!(
        status.runtime_unavailability_reason,
        ProjectionRuntimeUnavailabilityReason::VectorEquivalenceDisabled
    );
    assert_eq!(entry(&status, "summary"), ProjectionStatusDenseReadiness::Unavailable);
    rejected.engine.close().expect("close degraded runtime");
}

#[test]
fn legacy_non_searchable_vector_is_not_declared_and_never_reports_unsupported_kinds() {
    let dir = TempDir::new().expect("tempdir");
    let path = db_path(&dir, "legacy_non_searchable");
    {
        let opened = Engine::open(&path).expect("open fixture");
        opened
            .engine
            .configure_projections(&[filterable_spec("plain")], &[])
            .expect("declare plain");
        opened.engine.write(&[node("invoice", "I1")]).expect("write unsupported kind");
        opened.engine.close().expect("close before raw legacy fixture");
    }
    seed_legacy_non_searchable_vector(&path, "legacy_vector");

    let opened = Engine::open(&path).expect("reopen legacy fixture");
    let status = opened.engine.read_projection_status().expect("legacy status");
    assert_eq!(entry(&status, "plain"), ProjectionStatusDenseReadiness::NotDeclared);
    assert_eq!(entry(&status, "legacy_vector"), ProjectionStatusDenseReadiness::NotDeclared);
    assert_eq!(status.vector_unsupported_kinds, Vec::<String>::new());
    opened.engine.close().expect("close");
}
