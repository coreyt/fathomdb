//! 0.8.22 Slice 21 — dense runtime truthfulness and safe boot graft.
//!
//! The existing projection registry is durable while the dense runtime is a
//! property of an open session. These tests use the real SQLite tables as their
//! oracle: a no-runtime session must report `unavailable`; a later approved
//! runtime may atomically enrol and repair only the eligible stranded rows; and
//! a same-identity backend that fails equivalence must not mutate that repair.

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{
    DenseReadiness, Engine, InitialState, PreparedWrite, ProjectionRole, ProjectionSpec,
    ProjectionVector, SourceId,
};
use fathomdb_schema::SQLITE_SUFFIX;
use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::TempDir;

#[derive(Clone, Debug)]
struct CountingEmbedder {
    identity: EmbedderIdentity,
    calls: Arc<AtomicUsize>,
    divergent: bool,
    fail_on_call: Option<usize>,
}

impl CountingEmbedder {
    fn faithful(identity: EmbedderIdentity) -> Self {
        Self {
            identity,
            calls: Arc::new(AtomicUsize::new(0)),
            divergent: false,
            fail_on_call: None,
        }
    }

    fn divergent(identity: EmbedderIdentity) -> Self {
        Self { identity, calls: Arc::new(AtomicUsize::new(0)), divergent: true, fail_on_call: None }
    }

    fn failing_on(identity: EmbedderIdentity, call: usize) -> Self {
        Self {
            identity,
            calls: Arc::new(AtomicUsize::new(0)),
            divergent: false,
            fail_on_call: Some(call),
        }
    }
}

impl Embedder for CountingEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_on_call == Some(call) {
            return Err(EmbedderError::Failed {
                message: "deterministic prospective preflight failure".to_string(),
            });
        }
        let mut vector = vec![0.0_f32; self.identity.dimension as usize];
        vector[if self.divergent { 1 } else { 0 }] = 1.0;
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

fn node(logical_id: &str, body_json: &str) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: body_json.to_string(),
        source_id: SourceId::new("test:slice21-runtime").expect("source id"),
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

fn readiness(engine: &Engine, name: &str) -> Option<DenseReadiness> {
    engine
        .read_projections()
        .expect("read projections")
        .into_iter()
        .find(|spec| spec.name == name)
        .and_then(|spec| spec.vector)
        .and_then(|vector| vector.dense_readiness)
}

fn cursor(path: &Path, logical_id: &str) -> i64 {
    ro(path)
        .query_row(
            "SELECT write_cursor FROM canonical_nodes WHERE logical_id = ?1 AND superseded_at IS NULL",
            [logical_id],
            |row| row.get(0),
        )
        .expect("active cursor")
}

fn vector_row_exists(path: &Path, write_cursor: i64) -> bool {
    ro(path)
        .query_row(
            "SELECT COUNT(*) FROM _fathomdb_vector_rows WHERE write_cursor = ?1",
            [write_cursor],
            |row| row.get::<_, i64>(0),
        )
        .expect("vector row count")
        > 0
}

fn terminal_state(path: &Path, write_cursor: i64) -> Option<String> {
    ro(path)
        .query_row(
            "SELECT state FROM _fathomdb_projection_terminal WHERE write_cursor = ?1",
            [write_cursor],
            |row| row.get(0),
        )
        .ok()
}

fn vector_kind_exists(path: &Path, kind: &str) -> bool {
    ro(path)
        .query_row("SELECT COUNT(*) FROM _fathomdb_vector_kinds WHERE kind = ?1", [kind], |row| {
            row.get::<_, i64>(0)
        })
        .expect("vector kind count")
        > 0
}

#[derive(Eq, PartialEq)]
struct ProspectiveArmSnapshot {
    vector_kind_registered: bool,
    terminal: Option<String>,
    vector_row_exists: bool,
    probe_rows: Vec<(i64, String, Vec<u8>, String, String, i64)>,
    cached_verdict: Option<String>,
}

impl fmt::Debug for ProspectiveArmSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProspectiveArmSnapshot")
            .field("vector_kind_registered", &self.vector_kind_registered)
            .field("terminal", &self.terminal)
            .field("vector_row_exists", &self.vector_row_exists)
            .field("probe_row_count", &self.probe_rows.len())
            .field("cached_verdict", &self.cached_verdict)
            .finish()
    }
}

fn prospective_arm_snapshot(path: &Path, write_cursor: i64) -> ProspectiveArmSnapshot {
    let connection = ro(path);
    let mut statement = connection
        .prepare(
            "SELECT probe_ordinal, probe_text, reference_vec, embedder_name, embedder_revision, dim \
             FROM _fathomdb_embed_probe ORDER BY probe_ordinal",
        )
        .expect("prepare probe snapshot");
    let probe_rows = statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
        })
        .expect("read probe snapshot")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("collect probe snapshot");
    let cached_verdict = connection
        .query_row(
            "SELECT value FROM _fathomdb_open_state \
             WHERE key = 'vector_equivalence_verified_fingerprint'",
            [],
            |row| row.get(0),
        )
        .ok();

    ProspectiveArmSnapshot {
        vector_kind_registered: vector_kind_exists(path, "doc"),
        terminal: terminal_state(path, write_cursor),
        vector_row_exists: vector_row_exists(path, write_cursor),
        probe_rows,
        cached_verdict,
    }
}

fn make_probe_verdict_stale(path: &Path) {
    let connection = rusqlite::Connection::open(path).expect("open mutation connection");
    connection
        .execute(
            "UPDATE _fathomdb_open_state \
             SET value = 'deliberately-stale-slice21-preflight' \
             WHERE key = 'vector_equivalence_verified_fingerprint'",
            [],
        )
        .expect("make accepted probe cache stale without deleting it");
}

fn make_probe_verdict_stale_and_leave_vector_arm_cold(path: &Path) {
    make_probe_verdict_stale(path);
    let connection = rusqlite::Connection::open(path).expect("open mutation connection");
    connection
        .execute("DELETE FROM _fathomdb_vector_kinds WHERE kind = 'doc'", [])
        .expect("leave the declared dense arm cold");
}

#[test]
fn no_runtime_declaration_is_unavailable_and_a_fresh_drain_is_passive() {
    let dir = TempDir::new().expect("tempdir");
    let opened = Engine::open(db_path(&dir, "fresh_no_runtime")).expect("open without embedder");
    let engine = &opened.engine;

    engine.configure_projections(&[vector_spec("summary")], &[]).expect("declare vector arm");

    assert_eq!(
        readiness(engine, "summary"),
        Some(DenseReadiness::Unavailable),
        "a declaration is not a usable dense runtime"
    );
    engine.drain(500).expect("a fresh no-runtime declaration has no work to drain");
    assert_eq!(
        readiness(engine, "summary"),
        Some(DenseReadiness::Unavailable),
        "successful passive drain must not claim a runtime exists"
    );
    opened.engine.close().expect("close");
}

#[test]
fn enrolled_no_runtime_work_times_out_but_remains_unavailable() {
    let dir = TempDir::new().expect("tempdir");
    let path = db_path(&dir, "enrolled_no_runtime");
    Engine::open(&path).expect("create profile").engine.close().expect("close create");
    let identity = stored_default_identity(&path);

    {
        let opened = Engine::open_with_embedder_for_test(
            &path,
            Arc::new(CountingEmbedder::faithful(identity.clone())),
        )
        .expect("open with runtime");
        opened
            .engine
            .configure_projections(&[vector_spec("summary")], &[])
            .expect("declare vector arm");
        opened.engine.write(&[node("N1", r#"{"summary":"settled"}"#)]).expect("write N1");
        opened.engine.drain(30_000).expect("settle N1");
        assert!(vector_kind_exists(&path, "doc"), "fixture: the kind is durably enrolled");
        opened.engine.close().expect("close runtime session");
    }

    let opened = Engine::open(&path).expect("reopen without runtime");
    let engine = &opened.engine;
    engine.write(&[node("N2", r#"{"summary":"must stay pending"}"#)]).expect("write N2");
    let n2 = cursor(&path, "N2");

    assert!(engine.drain(250).is_err(), "existing enrolled work cannot drain without a runtime");
    assert_eq!(
        readiness(engine, "summary"),
        Some(DenseReadiness::Unavailable),
        "outstanding work does not make an absent runtime usable"
    );
    assert_eq!(terminal_state(&path, n2), None, "no-runtime work stays recoverably pending");
    assert!(!vector_row_exists(&path, n2), "no runtime created a vector");
    opened.engine.close().expect("close");
}

#[test]
fn approved_open_boot_grafts_once_and_never_reopens_failed_terminals() {
    let dir = TempDir::new().expect("tempdir");
    let path = db_path(&dir, "boot_graft");

    {
        let opened = Engine::open(&path).expect("open without runtime");
        opened
            .engine
            .configure_projections(&[vector_spec("summary")], &[])
            .expect("persist cold declaration");
        opened.engine.write(&[node("N1", r#"{"summary":"repair me"}"#)]).expect("write N1");
        opened
            .engine
            .write(&[node("N2", r#"{"summary":"failed stays failed"}"#)])
            .expect("write N2");
        opened.engine.drain(500).expect("cold declaration is passive");
        opened.engine.close().expect("close cold session");
    }
    let n1 = cursor(&path, "N1");
    let n2 = cursor(&path, "N2");
    let connection = rusqlite::Connection::open(&path).expect("open terminal mutation connection");
    connection
        .execute(
            "UPDATE _fathomdb_projection_terminal SET state = 'failed' WHERE write_cursor = ?1",
            [n2],
        )
        .expect("seed a genuine failed terminal");
    drop(connection);

    let identity = stored_default_identity(&path);
    let embedder = CountingEmbedder::faithful(identity.clone());
    let calls = Arc::clone(&embedder.calls);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder))
        .expect("approved runtime open");
    let engine = &opened.engine;
    assert_eq!(readiness(engine, "summary"), Some(DenseReadiness::Embedding));
    engine.drain(30_000).expect("boot graft drains");
    assert_eq!(readiness(engine, "summary"), Some(DenseReadiness::Ready));
    assert!(vector_kind_exists(&path, "doc"), "boot graft enrolled the committable kind");
    assert!(vector_row_exists(&path, n1), "the stranded up_to_date row was repaired");
    assert_eq!(terminal_state(&path, n2).as_deref(), Some("failed"));
    assert!(!vector_row_exists(&path, n2), "failed terminal is never reopened");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        91,
        "90 prospective-arm probe calls plus exactly one repaired row"
    );
    opened.engine.close().expect("close repaired session");

    let repeat = CountingEmbedder::faithful(identity);
    let repeat_calls = Arc::clone(&repeat.calls);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(repeat)).expect("repeat open");
    opened.engine.drain(500).expect("nothing left to repair");
    assert_eq!(
        repeat_calls.load(Ordering::SeqCst),
        0,
        "idempotent reopen does no probe or repair embed"
    );
    assert!(vector_row_exists(&path, n1));
    assert_eq!(terminal_state(&path, n2).as_deref(), Some("failed"));
    opened.engine.close().expect("close repeat");
}

#[test]
fn refused_prospective_arm_is_globally_mutation_free_before_graft() {
    let dir = TempDir::new().expect("tempdir");
    let path = db_path(&dir, "refused_boot_graft");
    Engine::open(&path).expect("create profile").engine.close().expect("close create");
    let identity = stored_default_identity(&path);

    {
        let opened = Engine::open_with_embedder_for_test(
            &path,
            Arc::new(CountingEmbedder::faithful(identity.clone())),
        )
        .expect("open baseline session");
        opened
            .engine
            .configure_projections(&[vector_spec("summary")], &[])
            .expect("persist declaration");
        opened.engine.configure_vector_kind_for_test("doc").expect("register for baseline");
        opened.engine.close().expect("close baseline setup");
    }
    {
        let opened = Engine::open_with_embedder_for_test(
            &path,
            Arc::new(CountingEmbedder::faithful(identity.clone())),
        )
        .expect("persist accepted baseline");
        assert!(!opened.report.dense_disabled, "fixture: accepted baseline");
        opened.engine.close().expect("close accepted baseline");
    }
    make_probe_verdict_stale_and_leave_vector_arm_cold(&path);
    {
        let opened = Engine::open(&path).expect("cold declaration session");
        opened
            .engine
            .write(&[node("N1", r#"{"summary":"must not mutate on refusal"}"#)])
            .expect("write N1");
        opened.engine.close().expect("close cold declaration session");
    }
    let n1 = cursor(&path, "N1");
    assert_eq!(terminal_state(&path, n1).as_deref(), Some("up_to_date"));
    assert!(!vector_kind_exists(&path, "doc"));
    assert!(!vector_row_exists(&path, n1));
    let before_refusal = prospective_arm_snapshot(&path, n1);
    assert!(
        before_refusal.cached_verdict.is_some(),
        "the stale marker is part of the mutation snapshot; do not delete it before refusal"
    );

    let divergent = CountingEmbedder::divergent(identity);
    let calls = Arc::clone(&divergent.calls);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(divergent))
        .expect("degraded open is still serviceable");
    assert!(opened.report.dense_disabled, "prospective arm must be checked before grafting");
    assert_eq!(readiness(&opened.engine, "summary"), Some(DenseReadiness::Unavailable));
    assert_eq!(calls.load(Ordering::SeqCst), 45, "only the preflight probe ran");
    assert_eq!(
        prospective_arm_snapshot(&path, n1),
        before_refusal,
        "refusal may neither graft/dispatch dense work nor mutate probe/cache state"
    );
    opened.engine.close().expect("close degraded open");
}

#[test]
fn refused_cold_prospective_population_never_persists_a_baseline() {
    let dir = TempDir::new().expect("tempdir");
    let path = db_path(&dir, "refused_cold_population");
    Engine::open(&path).expect("create profile").engine.close().expect("close create");
    let identity = stored_default_identity(&path);

    {
        let opened = Engine::open(&path).expect("cold declaration session");
        opened
            .engine
            .configure_projections(&[vector_spec("summary")], &[])
            .expect("persist cold declaration");
        opened
            .engine
            .write(&[node("N1", r#"{\"summary\":\"must not persist a failed preflight\"}"#)])
            .expect("write N1");
        opened.engine.close().expect("close cold declaration session");
    }
    let n1 = cursor(&path, "N1");
    let before_refusal = prospective_arm_snapshot(&path, n1);
    assert_eq!(before_refusal.probe_rows.len(), 0, "fixture: no baseline exists yet");
    assert_eq!(before_refusal.cached_verdict, None, "fixture: no cache exists yet");

    let failing = CountingEmbedder::failing_on(identity, 46);
    let calls = Arc::clone(&failing.calls);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(failing))
        .expect("degraded open is still serviceable");
    assert!(opened.report.dense_disabled, "failed prospective preflight refuses dense");
    assert_eq!(readiness(&opened.engine, "summary"), Some(DenseReadiness::Unavailable));
    assert_eq!(calls.load(Ordering::SeqCst), 46, "population and confirmation both ran");
    assert_eq!(
        prospective_arm_snapshot(&path, n1),
        before_refusal,
        "a prospective refusal cannot leave a partially accepted baseline or cache"
    );
    opened.engine.close().expect("close degraded open");
}

#[test]
fn registered_arm_refusal_still_clears_a_stale_probe_verdict() {
    let dir = TempDir::new().expect("tempdir");
    let path = db_path(&dir, "registered_refusal_clears_cache");
    Engine::open(&path).expect("create profile").engine.close().expect("close create");
    let identity = stored_default_identity(&path);

    {
        let opened = Engine::open_with_embedder_for_test(
            &path,
            Arc::new(CountingEmbedder::faithful(identity.clone())),
        )
        .expect("open registered-arm setup");
        opened
            .engine
            .configure_projections(&[vector_spec("summary")], &[])
            .expect("persist declaration");
        opened.engine.configure_vector_kind_for_test("doc").expect("register dense arm");
        opened.engine.close().expect("close setup");
    }
    {
        let opened = Engine::open_with_embedder_for_test(
            &path,
            Arc::new(CountingEmbedder::faithful(identity.clone())),
        )
        .expect("persist accepted baseline");
        assert!(!opened.report.dense_disabled, "fixture: accepted baseline");
        opened.engine.close().expect("close accepted baseline");
    }
    make_probe_verdict_stale(&path);
    let stale = ro(&path)
        .query_row(
            "SELECT value FROM _fathomdb_open_state WHERE key = 'vector_equivalence_verified_fingerprint'",
            [],
            |row| row.get::<_, String>(0),
        )
        .expect("stale cache is present");
    assert_eq!(stale, "deliberately-stale-slice21-preflight");

    let opened =
        Engine::open_with_embedder_for_test(&path, Arc::new(CountingEmbedder::divergent(identity)))
            .expect("degraded open is still serviceable");
    assert!(opened.report.dense_disabled, "registered divergent arm remains fail-closed");
    assert!(vector_kind_exists(&path, "doc"), "registered arm remains registered");
    assert!(
        ro(&path)
            .query_row(
                "SELECT value FROM _fathomdb_open_state WHERE key = 'vector_equivalence_verified_fingerprint'",
                [],
                |row| row.get::<_, String>(0),
            )
            .is_err(),
        "registered-arm refusal still removes the stale verdict"
    );
    opened.engine.close().expect("close degraded open");
}
