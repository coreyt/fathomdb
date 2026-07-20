//! OPP-12 record-lifecycle Phase-1 (0.8.19 Slice 10) — `transition` / `purge`
//! verb tests.
//!
//! Covers `dev/design/0.8.19-slice-0-opp12-phase1-design.md` §2 (legal-transition
//! table) + §3 (the verb design) + §8 gap-3 (purge edge cascade), gap-4
//! (secure_delete), gap-6 (reason semantics), and `dev/plans/plan-0.8.19.md` §2
//! (R-TR-1/2, R-PG-1/2):
//!   * R-TR-1 — each legal move succeeds; each illegal move → a typed
//!     `IllegalTransitionError { from_state, to_state, legal }` enumerating the
//!     legal targets.
//!   * gap-6 — promote/undelete CLEAR `reason`; reject/soft-delete SET it.
//!   * R-ID-2/§3 — a non-`l:` (`h:`/`p:`) id → `NotLifecycleAddressableError`.
//!   * R-PG-1 — `purge` erases every ROW-OWNED target (post-purge sweep finds
//!     nothing); edges touching the node are CASCADE-REMOVED; the global
//!     registries (`_fathomdb_projection_state`, `_fathomdb_vector_kinds`) are
//!     untouched.
//!   * R-PG-2 — deleted-first precondition; idempotent; `secure_delete=ON`.

use std::path::Path;
use std::sync::Arc;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{
    Engine, EngineError, InitialState, LifecycleState, OpenedEngine, PreparedWrite, ReadView,
};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

/// A deterministic dim-8 embedder (non-bge → no mean-centering) so `doc` writes
/// land real `vector_default` rows to prove the purge sweep is not vacuous.
#[derive(Clone, Debug)]
struct DetEmbedder;

impl Embedder for DetEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new("det", "rev-a", 8)
    }
    fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
        let mut v = vec![0.0_f32; 8];
        let mut h: u64 = 0xcbf29ce4_84222325;
        for &b in text.as_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        for k in 0..4 {
            let coord = ((h >> (k * 8)) as usize) % 8;
            v[coord] += 0.5_f32;
        }
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-6);
        for x in &mut v {
            *x /= norm;
        }
        Ok(v)
    }
}

fn open(name: &str) -> (TempDir, OpenedEngine) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("{name}{SQLITE_SUFFIX}"));
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(DetEmbedder)).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("configure vector kind");
    (dir, opened)
}

fn node(body: &str, logical_id: &str) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: body.to_string(),
        source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
        logical_id: Some(logical_id.to_string()),
        state: InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

fn edge(logical_id: &str, from: &str, to: &str, body: &str) -> PreparedWrite {
    PreparedWrite::Edge {
        kind: "link".to_string(),
        from: from.to_string(),
        to: to.to_string(),
        source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
        logical_id: Some(logical_id.to_string()),
        body: Some(body.to_string()),
        t_valid: None,
        t_invalid: None,
        confidence: Some(0.9),
        extractor_model_id: None,
        temporal_fallback: None,
    }
}

fn read_state_reason(path: &Path, logical_id: &str) -> Option<(String, Option<String>)> {
    let conn = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("open read-only");
    conn.query_row(
        "SELECT state, reason FROM canonical_nodes \
         WHERE logical_id = ?1 AND superseded_at IS NULL",
        [logical_id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
    )
    .ok()
}

fn count(conn: &rusqlite::Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).expect("count query")
}

/// R-TR-1 / gap-6 — every legal `transition` move succeeds and applies the
/// clear-on-admit / set-on-exclude `reason` semantics.
#[test]
fn transition_legal_moves_and_reason_semantics() {
    let (dir, opened) = open("tr_legal");
    let path = dir.path().join(format!("tr_legal{SQLITE_SUFFIX}"));
    let engine = &opened.engine;

    // A pending node with a create-time reason; promote clears it.
    engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "quarantined body".to_string(),
            source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
            logical_id: Some("p1".to_string()),
            state: InitialState::Pending,
            reason: Some("awaiting-review".to_string()),
            valid_from: None,
            valid_until: None,
        }])
        .expect("write pending");
    assert_eq!(read_state_reason(&path, "p1").unwrap().0, "pending");
    engine.transition("p1", LifecycleState::Active, None).expect("promote");
    assert_eq!(
        read_state_reason(&path, "p1"),
        Some(("active".to_string(), None)),
        "promote → active clears reason to NULL"
    );

    // reject: pending → deleted sets reason.
    engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "spam body".to_string(),
            source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
            logical_id: Some("p2".to_string()),
            state: InitialState::Pending,
            reason: None,
            valid_from: None,
            valid_until: None,
        }])
        .expect("write pending 2");
    engine
        .transition("p2", LifecycleState::Deleted, Some("rejected-spam".to_string()))
        .expect("reject");
    assert_eq!(
        read_state_reason(&path, "p2"),
        Some(("deleted".to_string(), Some("rejected-spam".to_string()))),
        "reject → deleted sets the supplied reason"
    );

    // active → deleted (soft-delete) sets reason; deleted → active (undelete) clears it.
    engine.write(&[node("live body", "a1")]).expect("write active");
    engine
        .transition("a1", LifecycleState::Deleted, Some("user-deleted".to_string()))
        .expect("soft-delete");
    assert_eq!(
        read_state_reason(&path, "a1"),
        Some(("deleted".to_string(), Some("user-deleted".to_string())))
    );
    engine.transition("a1", LifecycleState::Active, None).expect("undelete");
    assert_eq!(
        read_state_reason(&path, "a1"),
        Some(("active".to_string(), None)),
        "undelete → active clears reason"
    );
}

/// R-TR-1 — every illegal move returns a typed `IllegalTransition` naming the
/// `from_state`/`to_state` and enumerating the legal targets from `from_state`.
#[test]
fn illegal_transitions_return_typed_error_with_legal_targets() {
    let (_dir, opened) = open("tr_illegal");
    let engine = &opened.engine;
    engine.write(&[node("body", "a1")]).expect("write");

    // pending → purged: verb-specific legal set from `pending` is [active, deleted]
    // (promote / reject) and EXCLUDES `purged` (purge-only) and `pending` (self).
    engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "quarantined body".to_string(),
            source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
            logical_id: Some("pend".to_string()),
            state: InitialState::Pending,
            reason: None,
            valid_from: None,
            valid_until: None,
        }])
        .expect("write pending");
    let err = engine.transition("pend", LifecycleState::Purged, None).unwrap_err();
    assert_eq!(
        err,
        EngineError::IllegalTransition {
            from_state: LifecycleState::Pending,
            to_state: LifecycleState::Purged,
            legal: vec![LifecycleState::Active, LifecycleState::Deleted],
        }
    );

    // active → purged (purge-only target). Verb-specific legal set is [deleted].
    let err = engine.transition("a1", LifecycleState::Purged, None).unwrap_err();
    assert_eq!(
        err,
        EngineError::IllegalTransition {
            from_state: LifecycleState::Active,
            to_state: LifecycleState::Purged,
            legal: vec![LifecycleState::Deleted],
        }
    );
    // active → active (self-loop).
    assert!(matches!(
        engine.transition("a1", LifecycleState::Active, None).unwrap_err(),
        EngineError::IllegalTransition { from_state: LifecycleState::Active, .. }
    ));
    // active → pending (create-only target).
    assert!(matches!(
        engine.transition("a1", LifecycleState::Pending, None).unwrap_err(),
        EngineError::IllegalTransition { to_state: LifecycleState::Pending, .. }
    ));

    // deleted → purged via `transition` is illegal (purge is its own verb). The
    // `legal` enumeration is VERB-SPECIFIC to `transition`: it names ONLY
    // [active] (undelete) and EXCLUDES `purged` — reporting `purged` here would
    // falsely tell the caller it is a legal `transition` target (codex §9 P2).
    engine.transition("a1", LifecycleState::Deleted, None).expect("soft-delete");
    let err = engine.transition("a1", LifecycleState::Purged, None).unwrap_err();
    assert_eq!(
        err,
        EngineError::IllegalTransition {
            from_state: LifecycleState::Deleted,
            to_state: LifecycleState::Purged,
            legal: vec![LifecycleState::Active],
        }
    );
    // deleted → deleted (self-loop).
    assert!(matches!(
        engine.transition("a1", LifecycleState::Deleted, None).unwrap_err(),
        EngineError::IllegalTransition { from_state: LifecycleState::Deleted, .. }
    ));

    // Absent / never-created id → treated as the terminal (purged) state: no
    // legal targets.
    let err = engine.transition("ghost", LifecycleState::Active, None).unwrap_err();
    assert_eq!(
        err,
        EngineError::IllegalTransition {
            from_state: LifecycleState::Purged,
            to_state: LifecycleState::Active,
            legal: vec![],
        }
    );
}

/// §3 — lifecycle verbs are `Logical`(`l:`)-only; a `Content`(`h:`) or
/// `Passage`(`p:`) id raises `NotLifecycleAddressable`. An `l:`-prefixed id is
/// accepted (stripped to its bare value); an untagged id is a bare logical_id.
#[test]
fn non_logical_ids_are_refused() {
    let (_dir, opened) = open("addr");
    let engine = &opened.engine;
    engine.write(&[node("body", "a1")]).expect("write");

    for bad in ["h:deadbeef", "p:7"] {
        assert!(
            matches!(
                engine.transition(bad, LifecycleState::Deleted, None).unwrap_err(),
                EngineError::NotLifecycleAddressable { .. }
            ),
            "transition({bad}) must refuse a non-logical id"
        );
        assert!(
            matches!(engine.purge(bad).unwrap_err(), EngineError::NotLifecycleAddressable { .. }),
            "purge({bad}) must refuse a non-logical id"
        );
    }

    // An explicit `l:` prefix addresses the same node as the bare form.
    engine.transition("l:a1", LifecycleState::Deleted, Some("via-prefix".to_string())).expect("l:");
}

/// R-EX-2 (Slice-10 half) — soft-delete removes a node from default reads;
/// undelete restores it without re-projection (the row stayed indexed).
#[test]
fn soft_delete_excludes_from_default_reads_and_undelete_restores() {
    let (_dir, opened) = open("soft_delete");
    let engine = &opened.engine;
    engine.write(&[node("zephyrunique payload", "a1")]).expect("write");
    engine.drain(15_000).expect("drain");

    assert!(engine.read_get("a1", &ReadView::default()).expect("get").is_some());
    engine.transition("a1", LifecycleState::Deleted, Some("x".to_string())).expect("soft-delete");
    assert!(
        engine.read_get("a1", &ReadView::default()).expect("get").is_none(),
        "deleted node absent from read.get"
    );
    let hits = engine.search("zephyrunique").expect("search");
    assert!(
        !hits.results.iter().any(|h| h.body.contains("zephyrunique payload")),
        "deleted node excluded from default search"
    );

    engine.transition("a1", LifecycleState::Active, None).expect("undelete");
    assert!(
        engine.read_get("a1", &ReadView::default()).expect("get").is_some(),
        "undelete restores read.get visibility"
    );
}

/// R-PG-2 — `purge` requires deleted-first and is idempotent.
#[test]
fn purge_requires_deleted_first_and_is_idempotent() {
    let (_dir, opened) = open("pg_precond");
    let engine = &opened.engine;
    engine.write(&[node("body", "a1")]).expect("write");

    // active (not deleted) → precondition failure.
    let err = engine.purge("a1").unwrap_err();
    assert_eq!(
        err,
        EngineError::IllegalTransition {
            from_state: LifecycleState::Active,
            to_state: LifecycleState::Purged,
            legal: vec![LifecycleState::Deleted],
        }
    );

    engine.transition("a1", LifecycleState::Deleted, None).expect("soft-delete");
    engine.purge("a1").expect("purge from deleted");
    // Idempotent: a second purge on the now-absent id is a no-op success.
    engine.purge("a1").expect("idempotent re-purge");
    // Purging a never-created id is also a no-op success.
    engine.purge("never-existed").expect("idempotent absent purge");
}

/// R-PG-1 / gap-3 — `purge` erases every ROW-OWNED target and cascade-removes
/// edges touching the node, while the global registries are untouched.
#[test]
fn purge_erases_all_row_owned_targets_and_cascades_edges() {
    let (dir, opened) = open("pg_sweep");
    let path = dir.path().join(format!("pg_sweep{SQLITE_SUFFIX}"));
    let engine = &opened.engine;

    let receipt = engine
        .write(&[node("alpha purge-target body", "a"), node("beta survivor body", "b")])
        .expect("write nodes");
    let cursor_a = receipt.row_cursors[0] as i64;
    let cursor_b = receipt.row_cursors[1] as i64;
    let edge_receipt =
        engine.write(&[edge("e-ab", "a", "b", "alpha relates to beta")]).expect("write edge");
    let cursor_e = edge_receipt.row_cursors[0] as i64;
    engine.drain(15_000).expect("drain");

    // Pre-purge: prove the sweep is non-vacuous — A has FTS + vector shadow rows.
    let conn = rusqlite::Connection::open_with_flags(
        &path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("read-only conn");
    assert!(
        count(&conn, &format!("SELECT COUNT(*) FROM search_index WHERE write_cursor = {cursor_a}"))
            > 0,
        "precondition: node A must have a search_index row before purge"
    );
    assert!(
        count(&conn, &format!("SELECT COUNT(*) FROM vector_default WHERE rowid = {cursor_a}")) > 0,
        "precondition: node A must have a vector_default row before purge"
    );
    let vector_kinds_before = count(&conn, "SELECT COUNT(*) FROM _fathomdb_vector_kinds");
    let projection_state_before = count(&conn, "SELECT COUNT(*) FROM _fathomdb_projection_state");
    drop(conn);

    engine.transition("a", LifecycleState::Deleted, None).expect("soft-delete");
    engine.purge("a").expect("purge");

    let conn = rusqlite::Connection::open_with_flags(
        &path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("read-only conn 2");

    // ROW-OWNED sweep: NO row keyed to A / the touching edge remains anywhere.
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM canonical_nodes WHERE logical_id = 'a'"), 0);
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM canonical_edges WHERE from_id = 'a' OR to_id = 'a'"),
        0,
        "edges touching the purged node are cascade-removed (no stubs)"
    );
    for cursor in [cursor_a, cursor_e] {
        for table in ["search_index", "search_index_edges", "search_index_v2"] {
            assert_eq!(
                count(
                    &conn,
                    &format!("SELECT COUNT(*) FROM {table} WHERE write_cursor = {cursor}")
                ),
                0,
                "{table} must have no row for cursor {cursor} after purge"
            );
        }
        assert_eq!(
            count(&conn, &format!("SELECT COUNT(*) FROM vector_default WHERE rowid = {cursor}")),
            0,
            "vector_default must have no row for cursor {cursor} after purge"
        );
        assert_eq!(
            count(
                &conn,
                &format!(
                    "SELECT COUNT(*) FROM _fathomdb_vector_rows WHERE write_cursor = {cursor}"
                )
            ),
            0
        );
        assert_eq!(
            count(
                &conn,
                &format!(
                    "SELECT COUNT(*) FROM _fathomdb_projection_terminal WHERE write_cursor = {cursor}"
                )
            ),
            0
        );
    }

    // The survivor B is fully intact.
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM canonical_nodes WHERE logical_id = 'b'"), 1);
    assert!(
        count(&conn, &format!("SELECT COUNT(*) FROM vector_default WHERE rowid = {cursor_b}")) > 0,
        "the survivor node's vector row is untouched"
    );

    // Global / kind-level registries are NOT purge targets — untouched.
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM _fathomdb_vector_kinds"),
        vector_kinds_before,
        "_fathomdb_vector_kinds is a kind registry, not a purge target"
    );
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM _fathomdb_projection_state"),
        projection_state_before,
        "_fathomdb_projection_state is global high-water state, not a purge target"
    );
}

/// gap-4 — the standing `secure_delete=ON` PRAGMA is applied at open on the
/// WRITER connection.
#[test]
fn secure_delete_is_enabled_on_open() {
    let (_dir, opened) = open("secure_delete");
    assert!(
        opened.engine.secure_delete_enabled_for_test().expect("pragma read"),
        "PRAGMA secure_delete must be ON on the writer connection at open"
    );
}

/// gap-4 (codex §9 P1) — the standing `secure_delete=ON` PRAGMA must be set at
/// EVERY connection open, not just the writer. The reader-pool and the
/// projection/runtime connections perform DELETEs (vector-rewrite / projection
/// shadow rewrites) whose freed pages would otherwise leak erased content.
///
/// Gated `debug_assertions` because the per-worker reader probe seam
/// (`reader_secure_delete_enabled_for_test`) is debug-only, matching the
/// existing lookaside / cache-status reader introspection seams.
#[cfg(debug_assertions)]
#[test]
fn secure_delete_is_enabled_on_reader_pool_and_runtime_connections() {
    let (_dir, opened) = open("secure_delete_all");
    assert!(
        opened.engine.reader_secure_delete_enabled_for_test().expect("reader pragma read"),
        "PRAGMA secure_delete must be ON on EVERY reader-pool connection at open"
    );
    assert!(
        opened.engine.runtime_secure_delete_enabled_for_test().expect("runtime pragma read"),
        "PRAGMA secure_delete must be ON on the projection/runtime connection at open"
    );
}
