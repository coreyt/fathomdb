//! End-to-end CLI binary assertions for Phase 10a recovery + doctor verbs.
//!
//! These tests invoke the built `fathomdb` binary against a seeded engine,
//! parse the emitted JSON, and confirm the per-verb output contract owned
//! by `dev/interfaces/cli.md` § JSON output wrapping and the AC matrix in
//! `dev/design/recovery.md` § JSON shapes for other doctor verbs.
//!
//! ACs bound live in Phase 10a (CLI binary half):
//!
//! - AC-028a/b/c: `recover --excise-source` end-to-end CLI surface.
//! - AC-039a: `doctor safe-export` emits SHA-256 manifest path.
//! - AC-042: `doctor trace --source-ref` enumerates canonical rows.
//! - AC-043a/b: `doctor check-integrity` reports three sections, each
//!   `clean` or `findings`.
//! - AC-044, AC-063c: `recover --rebuild-projections` runs end-to-end.
//!
//! ACs still ignored with Phase-10a-specific dependencies:
//!
//! - AC-026: WAL-only-commit fixture not yet shared with CLI tests.
//! - AC-027a/b/c/d: pre/post vec0-rebuild canonical+FTS+vector parity
//!   harness lives in engine-side tests; not yet replayed through CLI.
//! - AC-039b: tampered-artifact verifier seam not yet landed.
//! - AC-043c: full-mode page-corruption fixture not exposed to CLI tests.

use std::path::PathBuf;
use std::process::Command;

use fathomdb::{Engine, PreparedWrite};
use fathomdb_cli::exit_code;
use serde_json::Value;
use tempfile::TempDir;

fn fathomdb() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fathomdb"))
}

fn seeded_db() -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("recovery.sqlite");
    let opened = Engine::open(path.clone()).expect("open");
    opened.engine.close().expect("close");
    drop(opened);
    (dir, path)
}

fn db_with_sources() -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("recovery.sqlite");
    let opened = Engine::open(path.clone()).expect("open");
    opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "alpha".to_string(),
            source_id: fathomdb::SourceId::new("src-a").expect("test source id"),
            logical_id: None,
            state: fathomdb::InitialState::Active,
            reason: None,
            valid_from: None,
            valid_until: None,
        }])
        .expect("write a");
    opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "beta".to_string(),
            source_id: fathomdb::SourceId::new("src-a").expect("test source id"),
            logical_id: None,
            state: fathomdb::InitialState::Active,
            reason: None,
            valid_from: None,
            valid_until: None,
        }])
        .expect("write a2");
    opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "gamma".to_string(),
            source_id: fathomdb::SourceId::new("src-b").expect("test source id"),
            logical_id: None,
            state: fathomdb::InitialState::Active,
            reason: None,
            valid_from: None,
            valid_until: None,
        }])
        .expect("write b");
    opened.engine.close().expect("close");
    drop(opened);
    (dir, path)
}

fn run_json(args: &[&str]) -> (Option<i32>, Value) {
    let output = fathomdb().args(args).output().expect("spawn");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("stdout must be JSON; err={e} stdout={stdout}"));
    (output.status.code(), parsed)
}

/// Raw op-store row count — design §3 Rule 1 (assert on RAW tables, never on a
/// verb's own report).
fn raw_collection_row_count(path: &std::path::Path, collection: &str) -> u64 {
    let conn = rusqlite::Connection::open(path).expect("probe connection");
    conn.query_row(
        "SELECT COUNT(*) FROM operational_mutations WHERE collection_name = ?1",
        [collection],
        |row| row.get(0),
    )
    .expect("count collection rows")
}

/// **codex §9 round-3 P1, CLI half.** codex named the CLI as the reachable path:
/// `recover --accept-data-loss --excise-collection erasure_pending_redaction
/// --excise-record-key excise_source` deleted the durable record of a telemetry
/// redaction the engine still owed, after which the next erasure verb reported
/// SUCCESS while the erased stable ids were still in the telemetry sink
/// (R-20-E5). The erasure-audit collections are refused on the same footing —
/// the HITL ruling requires an auditable record of the deletion event.
///
/// Engine-side witness (including that the obligation is still discharged
/// afterwards) is
/// `fathomdb-engine/tests/erasure_completeness.rs::erasure_bookkeeping_collections_are_not_excisable`.
#[cfg(unix)]
#[test]
fn t_erasure_bookkeeping_collections_refused_via_cli() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().expect("tempdir");
    let db = dir.path().join("recovery.sqlite");
    let sink_dir = dir.path().join("sink");
    std::fs::create_dir(&sink_dir).expect("create sink dir");
    let sink = sink_dir.join("telemetry.jsonl");

    let opened = Engine::open(db.clone()).expect("open");
    opened.engine.enable_telemetry(sink.to_str().unwrap()).expect("enable telemetry");
    opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "alpha zeta".to_string(),
            source_id: fathomdb::SourceId::new("src-a").expect("test source id"),
            logical_id: Some("victim-1".to_string()),
            state: fathomdb::InitialState::Active,
            reason: None,
            valid_from: None,
            valid_until: None,
        }])
        .expect("write");
    opened.engine.drain(10_000).expect("drain");
    opened.engine.search("zeta").expect("search");
    assert!(
        std::fs::read_to_string(&sink).expect("read sink").contains("l:victim-1"),
        "fixture: the victim id must reach the sink"
    );

    // Freeze the sink directory so the redaction cannot complete: the erasure
    // commits its deletes and leaves a DURABLE pending-redaction obligation.
    std::fs::set_permissions(&sink_dir, std::fs::Permissions::from_mode(0o555)).expect("chmod");
    let probe = sink_dir.join("write-probe");
    if std::fs::write(&probe, b"x").is_ok() {
        let _ = std::fs::remove_file(&probe);
        std::fs::set_permissions(&sink_dir, std::fs::Permissions::from_mode(0o755))
            .expect("chmod restore");
        panic!("cannot inject a redaction failure (running as root?) — this test would be vacuous");
    }
    opened.engine.excise_source("src-a").expect_err("redaction must fail");
    std::fs::set_permissions(&sink_dir, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    opened.engine.close().expect("close");
    drop(opened);

    assert_eq!(
        raw_collection_row_count(&db, "erasure_pending_redaction"),
        1,
        "fixture: one durable pending-redaction obligation"
    );

    let (_code, parsed) = run_json(&[
        "recover",
        "--accept-data-loss",
        "--excise-collection",
        "erasure_pending_redaction",
        "--excise-record-key",
        "excise_source",
        db.to_str().unwrap(),
    ]);
    assert_eq!(
        parsed.get("status").and_then(Value::as_str),
        Some("error"),
        "the CLI must REFUSE to excise the engine-internal pending-redaction queue: {parsed}"
    );
    assert_eq!(
        raw_collection_row_count(&db, "erasure_pending_redaction"),
        1,
        "the CLI deleted an outstanding erasure obligation; the next erasure verb would then \
         report success with the erased ids still in the telemetry sink (R-20-E5)"
    );

    assert_eq!(
        raw_collection_row_count(&db, "excise_source_audit"),
        1,
        "fixture: one erasure-audit row"
    );
    let (_code, parsed) = run_json(&[
        "recover",
        "--accept-data-loss",
        "--excise-collection",
        "excise_source_audit",
        "--excise-record-key",
        "src-a",
        db.to_str().unwrap(),
    ]);
    assert_eq!(
        parsed.get("status").and_then(Value::as_str),
        Some("error"),
        "the CLI must REFUSE to excise the erasure-audit collection: {parsed}"
    );
    assert_eq!(
        raw_collection_row_count(&db, "excise_source_audit"),
        1,
        "the CLI deleted the auditable record of the deletion event"
    );
    drop(dir);
}

#[test]
fn t_028a_excise_source_cli_returns_excise_report() {
    // AC-028a CLI half: `recover --accept-data-loss --excise-source <id>`
    // exits 64 (RECOVERY_ACCEPTED_LOSS) and emits a JSON envelope keyed
    // by `verb: "excise-source"` with the canonical excise report
    // counts. Engine-side audit-row contract is bound by
    // `tests/excise_source.rs::ac_028a_*`.
    let (dir, db) = db_with_sources();
    let (code, parsed) = run_json(&[
        "recover",
        "--accept-data-loss",
        "--excise-source",
        "src-a",
        db.to_str().unwrap(),
    ]);
    assert_eq!(code, Some(exit_code::RECOVERY_ACCEPTED_LOSS));
    assert_eq!(parsed.get("verb").and_then(Value::as_str), Some("excise-source"));
    assert_eq!(parsed.get("source_ref").and_then(Value::as_str), Some("src-a"));
    assert_eq!(parsed.get("nodes_excised").and_then(Value::as_u64), Some(2));
    drop(dir);
}

#[test]
fn t_028b_excise_source_cli_reports_projections_invalidated_field() {
    // AC-028b CLI half: the emitted report includes the
    // `projections_invalidated` field so binding consumers can detect
    // shadow-state removal. Engine-side residue assertion is bound by
    // `tests/excise_source.rs::ac_028b_*`.
    let (dir, db) = db_with_sources();
    let (_code, parsed) = run_json(&[
        "recover",
        "--accept-data-loss",
        "--excise-source",
        "src-a",
        db.to_str().unwrap(),
    ]);
    assert!(
        parsed.get("projections_invalidated").is_some(),
        "CLI report must include projections_invalidated; got {parsed:?}",
    );
    drop(dir);
}

#[test]
fn t_028c_excise_source_cli_isolates_non_excised_sources() {
    // AC-028c CLI half: excising one source leaves the other source's
    // rows queryable. After excise, run trace on the un-excised source
    // and confirm its canonical-row events survive.
    let (dir, db) = db_with_sources();
    let (_code, _excise) = run_json(&[
        "recover",
        "--accept-data-loss",
        "--excise-source",
        "src-a",
        db.to_str().unwrap(),
    ]);
    let (code, parsed) =
        run_json(&["doctor", "trace", "--source-ref", "src-b", db.to_str().unwrap()]);
    assert_eq!(code, Some(exit_code::OK));
    let events = parsed.get("events").and_then(Value::as_array).expect("events array");
    assert_eq!(events.len(), 1, "src-b must still have its one event after src-a excise");
    drop(dir);
}

#[test]
fn t_039a_safe_export_cli_emits_manifest_sha256_field() {
    // AC-039a CLI half: doctor safe-export emits JSON with
    // `export_path`, `manifest_path`, `manifest_sha256`. Engine-side
    // file-bytes equality with the manifest is bound by
    // `tests/safe_export.rs`.
    let (dir, db) = seeded_db();
    let out = dir.path().join("export.sqlite");
    let manifest = dir.path().join("export.manifest.json");
    let (code, parsed) = run_json(&[
        "doctor",
        "safe-export",
        out.to_str().unwrap(),
        "--manifest",
        manifest.to_str().unwrap(),
        db.to_str().unwrap(),
    ]);
    assert_eq!(code, Some(exit_code::OK));
    assert_eq!(parsed.get("verb").and_then(Value::as_str), Some("safe-export"));
    let sha = parsed.get("manifest_sha256").and_then(Value::as_str).expect("sha256 field");
    assert_eq!(sha.len(), 64, "sha256 hex must be 64 chars; got {sha}");
    assert!(out.exists(), "exported DB file must exist");
    assert!(manifest.exists(), "manifest sidecar must exist");
    drop(dir);
}

#[test]
fn t_042_trace_cli_enumerates_canonical_rows_for_source() {
    // AC-042 CLI half: `doctor trace --source-ref <id>` returns one
    // event per canonical row attributable to the source, ordered by
    // `write_cursor`. Engine-side ordering invariant is bound by
    // `tests/*` engine tests; this asserts the CLI JSON envelope.
    let (dir, db) = db_with_sources();
    let (code, parsed) =
        run_json(&["doctor", "trace", "--source-ref", "src-a", db.to_str().unwrap()]);
    assert_eq!(code, Some(exit_code::OK));
    assert_eq!(parsed.get("verb").and_then(Value::as_str), Some("trace"));
    assert_eq!(parsed.get("source_ref").and_then(Value::as_str), Some("src-a"));
    let events = parsed.get("events").and_then(Value::as_array).expect("events array");
    assert_eq!(events.len(), 2);
    let cursors: Vec<u64> =
        events.iter().filter_map(|e| e.get("write_cursor").and_then(Value::as_u64)).collect();
    assert!(cursors.windows(2).all(|w| w[0] <= w[1]), "events must be cursor-ordered: {cursors:?}");
    drop(dir);
}

#[test]
fn t_043a_check_integrity_cli_emits_three_sections() {
    // AC-043a CLI half: integrity report has top-level keys
    // `physical`, `logical`, `semantic`. Engine-side three-section
    // contract is bound by `tests/check_integrity.rs`.
    let (dir, db) = seeded_db();
    let (_code, parsed) = run_json(&["doctor", "check-integrity", db.to_str().unwrap()]);
    assert_eq!(parsed.get("verb").and_then(Value::as_str), Some("check-integrity"));
    assert!(parsed.get("physical").is_some(), "physical key missing: {parsed:?}");
    assert!(parsed.get("logical").is_some(), "logical key missing: {parsed:?}");
    assert!(parsed.get("semantic").is_some(), "semantic key missing: {parsed:?}");
    drop(dir);
}

#[test]
fn t_043b_check_integrity_cli_each_section_clean_or_findings() {
    // AC-043b CLI half: every section's machine shape is either
    // {status:"clean", findings:[]} or {status:"findings", findings:[...]}.
    // On a clean seeded DB every section is `clean`.
    let (dir, db) = seeded_db();
    let (code, parsed) = run_json(&["doctor", "check-integrity", db.to_str().unwrap()]);
    assert_eq!(code, Some(exit_code::OK), "clean DB must exit 0; got {code:?}");
    for key in ["physical", "logical", "semantic"] {
        let section = parsed.get(key).expect("section");
        let status = section.get("status").and_then(Value::as_str).expect("status");
        assert!(
            status == "clean" || status == "findings",
            "section {key} status must be clean|findings; got {status}",
        );
        let findings = section.get("findings").and_then(Value::as_array).expect("findings array");
        if status == "clean" {
            assert!(findings.is_empty(), "clean section must have empty findings");
        }
    }
    drop(dir);
}

#[test]
fn t_044_recover_rebuild_projections_cli_runs_end_to_end() {
    // AC-044 CLI half: `recover --accept-data-loss --rebuild-projections`
    // exits 64 and emits a rebuild report with the
    // `projection_cursor_after` field. Engine-side projection-rebuild
    // semantics are bound by `tests/rebuild_projections.rs`.
    let (dir, db) = seeded_db();
    let (code, parsed) =
        run_json(&["recover", "--accept-data-loss", "--rebuild-projections", db.to_str().unwrap()]);
    assert_eq!(code, Some(exit_code::RECOVERY_ACCEPTED_LOSS));
    assert_eq!(parsed.get("verb").and_then(Value::as_str), Some("rebuild-projections"));
    assert!(
        parsed.get("projection_cursor_after").is_some(),
        "rebuild report must carry projection_cursor_after",
    );
    drop(dir);
}

#[test]
fn t_063c_recover_rebuild_projections_cli_regenerate_workflow() {
    // AC-063c CLI half: `recover --rebuild-projections` is the
    // canonical regenerate workflow. Confirm the CLI binds it as the
    // landed engine seam, with `kind: "projections"` discriminator.
    let (dir, db) = seeded_db();
    let (_code, parsed) =
        run_json(&["recover", "--accept-data-loss", "--rebuild-projections", db.to_str().unwrap()]);
    assert_eq!(parsed.get("kind").and_then(Value::as_str), Some("projections"));
    drop(dir);
}

#[test]
fn t_058_recover_truncate_wal_with_accept_data_loss_succeeds() {
    // AC-058 CLI half: `recover --accept-data-loss --truncate-wal` exits
    // 64 (RECOVERY_ACCEPTED_LOSS) and emits a JSON envelope with
    // `verb: "truncate-wal"`, `status`, and the three SQLite counters.
    let (dir, db) = seeded_db();
    let (code, parsed) =
        run_json(&["recover", "--accept-data-loss", "--truncate-wal", db.to_str().unwrap()]);
    assert_eq!(code, Some(exit_code::RECOVERY_ACCEPTED_LOSS));
    assert_eq!(parsed.get("verb").and_then(Value::as_str), Some("truncate-wal"));
    let status = parsed.get("status").and_then(Value::as_str).expect("status");
    assert!(status == "done" || status == "busy", "status must be done|busy; got {status}");
    assert!(parsed.get("busy").and_then(Value::as_u64).is_some());
    assert!(parsed.get("log_frames").and_then(Value::as_u64).is_some());
    assert!(parsed.get("checkpointed_frames").and_then(Value::as_u64).is_some());
    drop(dir);
}

#[test]
fn t_058_recover_truncate_wal_refused_without_accept_data_loss() {
    // AC-058 CLI half: `recover --truncate-wal` (no --accept-data-loss)
    // is refused with the standard recover-refusal envelope.
    let (dir, db) = seeded_db();
    let output = fathomdb()
        .args(["recover", "--truncate-wal", db.to_str().unwrap()])
        .output()
        .expect("spawn");
    assert_eq!(output.status.code(), Some(exit_code::UNRECOVERABLE));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("json");
    assert_eq!(parsed.get("status").and_then(Value::as_str), Some("refused"));
    assert_eq!(parsed.get("verb").and_then(Value::as_str), Some("recover"));
    drop(dir);
}

// ---- Still ignored: fixture / engine-seam gaps remain ----

#[test]
#[ignore = "AC-026: dep-Phase-10a-wal-only-fixture; safe-export seam landed, but the WAL-only-commit setup harness is not yet shared with CLI tests"]
fn t_026_safe_export_covers_wal_only_commits() {
    let _bin = env!("CARGO_BIN_EXE_fathomdb");
    unimplemented!("AC-026 awaits a CLI-side WAL-only-commit fixture");
}

#[test]
#[ignore = "AC-027a: dep-Phase-10a-vec0-rebuild-pre-post-canonical-compare; engine-side fixture not shared with CLI"]
fn t_027a_recovery_preserves_canonical_rows() {
    let _bin = env!("CARGO_BIN_EXE_fathomdb");
    unimplemented!("AC-027a awaits CLI-side pre/post canonical-row compare fixture");
}

#[test]
#[ignore = "AC-027b: dep-Phase-10a-fts-result-equality; CLI-side FTS query parity harness not yet wired"]
fn t_027b_recovery_preserves_fts_query_result_equality() {
    let _bin = env!("CARGO_BIN_EXE_fathomdb");
    unimplemented!("AC-027b awaits CLI-side FTS pre/post compare fixture");
}

#[test]
#[ignore = "AC-027c: dep-Phase-10a-vector-profile-bit-equal; CLI-side vector-profile metadata compare not yet wired"]
fn t_027c_recovery_preserves_vector_profile_metadata_bit_equal() {
    let _bin = env!("CARGO_BIN_EXE_fathomdb");
    unimplemented!("AC-027c awaits CLI-side vector-profile metadata compare fixture");
}

#[test]
#[ignore = "AC-027d: dep-Phase-10a-vector-top-k-rank-correlation; rank-correlation harness lives in engine tests"]
fn t_027d_recovery_preserves_vector_top_k_rank_correlation() {
    let _bin = env!("CARGO_BIN_EXE_fathomdb");
    unimplemented!("AC-027d awaits CLI-side rank-correlation harness");
}

#[test]
#[ignore = "AC-039b: dep-Phase-10a-safe-export-tamper-verifier; engine has no verify-manifest seam yet"]
fn t_039b_safe_export_tampered_artifact_detected() {
    let _bin = env!("CARGO_BIN_EXE_fathomdb");
    unimplemented!("AC-039b awaits a safe-export verifier seam");
}

#[test]
#[ignore = "AC-043c: dep-Phase-10a-page-corruption-fixture; CLI-side page-corruption helper not yet exposed"]
fn t_043c_check_integrity_full_findings_carry_stable_report_fields() {
    let _bin = env!("CARGO_BIN_EXE_fathomdb");
    unimplemented!(
        "AC-043c awaits a CLI-side page-corruption helper to surface --full integrity findings"
    );
}
