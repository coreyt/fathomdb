//! 0.8.20 Slice 5b — erasure completeness beyond the row-owned projection
//! registry: WAL bytes at rest (R-20-E5), telemetry-sink selective redaction
//! (R-20-E6), op-store record erasure (R-20-E7) and erasure-audit durability
//! (design `0.8.20-slice0-erasure-design.md` §2 defect D-A / §4 item 9a).
//!
//! **Test-design contract (design §3).**
//!
//! * **Rule 1** — erasure witnesses assert on RAW state, never via `search()`.
//! * **Rule 3 (this file's core constraint)** — the WAL requirement is a claim
//!   about **bytes at rest**. `erasure_wal_bytes_absent` therefore scans the raw
//!   `.db` **and** `-wal` files for the erased body as a byte pattern. It must
//!   NOT close the engine before asserting: SQLite checkpoints and unlinks the
//!   `-wal` when the last connection closes, which would make the assertion
//!   vacuously true on the broken code.

use std::path::{Path, PathBuf};
use std::time::Duration;

use fathomdb_engine::{Engine, EngineError, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use rusqlite::Connection;
use tempfile::TempDir;

fn db_path(dir: &TempDir, name: &str) -> PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

/// The sidecar `-wal` file SQLite maintains next to `path`.
fn wal_path(path: &Path) -> PathBuf {
    let mut raw = path.as_os_str().to_os_string();
    raw.push("-wal");
    PathBuf::from(raw)
}

/// Rule 3 — raw byte scan. Absent file counts as "not present".
fn file_contains_bytes(path: &Path, needle: &str) -> bool {
    let Ok(bytes) = std::fs::read(path) else { return false };
    let needle = needle.as_bytes();
    if needle.is_empty() || bytes.len() < needle.len() {
        return false;
    }
    bytes.windows(needle.len()).any(|window| window == needle)
}

fn write_node(engine: &Engine, body: &str, source_id: &str, logical_id: Option<&str>) -> u64 {
    engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: body.to_string(),
            source_id: fathomdb_engine::SourceId::new(source_id).expect("test source id"),
            logical_id: logical_id.map(str::to_string),
            state: fathomdb_engine::InitialState::Active,
            reason: None,
        }])
        .expect("write")
        .cursor
}

fn register_collection(engine: &Engine, name: &str, kind: &str) {
    engine
        .write(&[PreparedWrite::AdminSchema {
            name: name.to_string(),
            kind: kind.to_string(),
            schema_json: "{}".to_string(),
            retention_json: "{}".to_string(),
        }])
        .expect("register collection");
}

fn append_op_record(engine: &Engine, collection: &str, record_key: &str, body: &str) {
    engine
        .write(&[PreparedWrite::OpStore {
            collection: collection.to_string(),
            record_key: record_key.to_string(),
            schema_id: None,
            body: body.to_string(),
        }])
        .expect("op-store append");
}

// ===== R-20-E5 · WAL coverage =========================================

/// **Rule 3 — raw-byte witness.** `secure_delete=ON` zeroes pages freed inside
/// the database file, but the erased content also lives in the write-ahead log
/// as committed frames from the ORIGINAL insert. Those frames survive the
/// erasure transaction untouched: the DELETE appends new frames, it does not
/// rewrite old ones. Unless the erasure verb checkpoints the WAL with
/// `TRUNCATE`, the erased body is still readable on disk with `grep`.
///
/// The engine is deliberately left OPEN across the assertion — closing the last
/// connection checkpoints and unlinks the `-wal`, which would hide the defect.
#[test]
fn erasure_wal_bytes_absent() {
    const SECRET: &str = "QZXERASUREWALSECRETTOKENQZX";
    const CONTROL: &str = "QZXRETAINEDCONTROLTOKENQZX";

    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "wal_bytes");
    let opened = Engine::open(&path).expect("open");

    write_node(&opened.engine, &format!("classified {SECRET} payload"), "S1", Some("victim-1"));
    write_node(&opened.engine, &format!("ordinary {CONTROL} payload"), "S2", Some("control-1"));
    opened.engine.drain(10_000).expect("drain");

    let wal = wal_path(&path);
    // Seed guard: the secret really is on disk (and, on a fresh small DB, in
    // the WAL) before the erasure — otherwise the post-assertion is vacuous.
    assert!(
        file_contains_bytes(&path, SECRET) || file_contains_bytes(&wal, SECRET),
        "seed: erasable body must be on disk before erasure"
    );

    opened.engine.excise_source("S1").expect("excise_source");

    assert!(
        !file_contains_bytes(&path, SECRET),
        "erased body still present as bytes in the database file at rest"
    );
    assert!(
        !file_contains_bytes(&wal, SECRET),
        "erased body still present as bytes in the -wal file at rest: the erasure verb never \
         checkpointed the write-ahead log, so `grep` recovers the erased content"
    );
    // Non-vacuity: a WAL truncation must not be mistaken for having wiped the
    // whole database. The untouched source's body is still readable.
    assert!(
        file_contains_bytes(&path, CONTROL) || file_contains_bytes(&wal, CONTROL),
        "non-excised body must survive the erasure verb's WAL checkpoint"
    );

    opened.engine.close().unwrap();
}

/// An erasure verb must NEVER report success on an incomplete erasure. When a
/// concurrent reader pins a WAL snapshot, `PRAGMA wal_checkpoint(TRUNCATE)`
/// returns `busy != 0` and the erased bytes stay in the log; the verb must
/// surface a typed [`EngineError::ErasureIncomplete`] instead of `Ok`.
#[test]
fn erasure_busy_yields_incomplete_not_success() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "wal_busy");
    let opened = Engine::open(&path).expect("open");

    write_node(&opened.engine, "busy-path erasable body", "S1", Some("victim-1"));
    write_node(&opened.engine, "busy-path retained body", "S2", Some("control-1"));
    opened.engine.drain(10_000).expect("drain");

    // Pin a read snapshot on an independent connection. A WAL reader blocks the
    // checkpointer from resetting/truncating the log.
    let blocker = Connection::open(&path).expect("blocker connection");
    blocker.execute_batch("BEGIN").expect("begin blocker read txn");
    let _pinned: u64 = blocker
        .query_row("SELECT COUNT(*) FROM canonical_nodes", [], |row| row.get(0))
        .expect("pin a WAL read snapshot");

    let err = opened
        .engine
        .excise_source("S1")
        .expect_err("excise must not report success while the WAL cannot be truncated");
    assert!(
        matches!(err, EngineError::ErasureIncomplete { .. }),
        "expected a typed ErasureIncomplete refusal, got {err:?}"
    );

    blocker.execute_batch("COMMIT").expect("release blocker");
    drop(blocker);
    opened.engine.close().unwrap();
}

// ===== R-20-E6 · Telemetry selective redaction ========================

/// Seed a telemetry sink holding: one event referencing the id that will be
/// erased, one event referencing a retained id, and one unrelated operator
/// record the engine never wrote. Returns the sink path and its post-erasure
/// contents.
fn telemetry_fixture(name: &str) -> (TempDir, String) {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, name);
    let sink = dir.path().join("telemetry.jsonl");
    let opened = Engine::open(&path).expect("open");
    opened.engine.enable_telemetry(sink.to_str().unwrap()).expect("enable telemetry");

    write_node(&opened.engine, "erasable zeta payload", "S1", Some("victim-1"));
    write_node(&opened.engine, "retained omega payload", "S2", Some("control-1"));
    opened.engine.drain(10_000).expect("drain");

    let victim_hits = opened.engine.search("zeta").expect("search zeta");
    assert!(
        victim_hits.results.iter().any(|h| h.id.to_prefixed() == "l:victim-1"),
        "fixture: the victim id must be captured into the sink"
    );
    let control_hits = opened.engine.search("omega").expect("search omega");
    assert!(
        control_hits.results.iter().any(|h| h.id.to_prefixed() == "l:control-1"),
        "fixture: the control id must be captured into the sink"
    );

    // An unrelated record the engine did not author. The sink path is
    // CALLER-SUPPLIED and may hold operator eval history; truncating it would
    // destroy data the erasure obligation never covered.
    let existing = std::fs::read_to_string(&sink).expect("read sink");
    std::fs::write(
        &sink,
        format!("{existing}{}\n", r#"{"type":"operator_note","note":"UNRELATEDEVALHISTORY"}"#),
    )
    .expect("append operator note");

    opened.engine.excise_source("S1").expect("excise_source");
    let after = std::fs::read_to_string(&sink).expect("read sink after erasure");
    opened.engine.close().unwrap();
    (dir, after)
}

/// Telemetry persists `result_stable_ids` — `l:`/`h:` prefixed ids — into a
/// JSONL file that outlives the erased rows. `derive_logical_id` is
/// `SHA256(lowercase(kind) + ":" + lowercase(name))`: it CASE-FOLDS both
/// inputs, which shrinks the preimage space and makes a retained `l:` id
/// dictionary-attackable back to the natural key it was derived from. So an
/// erased id must not survive in the sink.
#[test]
fn purged_id_absent_from_sink() {
    let (_dir, after) = telemetry_fixture("telemetry_redact");
    assert!(
        !after.contains("l:victim-1"),
        "erased stable id still persisted in the telemetry sink:\n{after}"
    );
}

/// The redaction must be SELECTIVE, not a truncation. The sink is a
/// caller-supplied path that may hold unrelated operator eval history; the
/// engine's erasure obligation covers the erased ids and nothing else.
///
/// NOTE (honest RED accounting): this test PASSES at the slice baseline,
/// because nothing touches the sink at all. It is the anti-regression guard
/// that distinguishes redaction from the rejected truncation approach. Its
/// non-vacuity was proven positively by replacing the redaction with a
/// `File::create` truncation, which makes it fail naming the lost records.
#[test]
fn unrelated_sink_records_survive() {
    let (_dir, after) = telemetry_fixture("telemetry_survive");
    assert!(
        after.contains("UNRELATEDEVALHISTORY"),
        "redaction destroyed an unrelated operator record; the sink must not be truncated:\n{after}"
    );
    assert!(
        after.contains("l:control-1"),
        "redaction destroyed a retained id's telemetry record:\n{after}"
    );
    assert!(
        after.lines().filter(|l| !l.trim().is_empty()).count() >= 3,
        "redaction dropped whole records; expected >= 3 surviving lines:\n{after}"
    );
}

/// **codex §9 P2 — a retry must not report success on an unfinished redaction.**
///
/// The redaction runs AFTER the delete transaction commits. If it fails, the
/// verb correctly raises `ErasureIncomplete { stage: "telemetry_redaction" }` and
/// the operator is told to retry. But the retry re-derived `erased_stable_ids`
/// from the canonical tables — whose rows the FIRST call already deleted — so it
/// computed an EMPTY id set, hit the empty-id fast path in
/// `redact_telemetry_stable_ids`, and returned `Ok`. The leaked stable ids were
/// still sitting in the sink, and the verb said the erasure was complete.
///
/// That is the exact failure R-20-E5 forbids: *an erasure verb must NEVER report
/// success on an incomplete erasure.* The pending redaction has to be DURABLE, so
/// a retry finishes the job rather than forgetting what it owed.
///
/// Failure is injected by making the sink's directory read-only, so
/// `redact_jsonl_stable_ids` cannot create its `.redact.tmp` sibling
/// (`PermissionDenied`, not `NotFound` — the latter is a legitimate "sink is
/// gone, nothing to redact" success).
#[cfg(unix)]
#[test]
fn telemetry_redaction_retry_completes_after_failure() {
    use std::os::unix::fs::PermissionsExt;

    fn chmod(dir: &Path, mode: u32) {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode)).expect("chmod");
    }

    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "telemetry_retry");
    // The sink lives in its own directory so it can be made read-only WITHOUT
    // also freezing the database, its `-wal` or the engine's own scratch files.
    let sink_dir = dir.path().join("sink");
    std::fs::create_dir(&sink_dir).expect("create sink dir");
    let sink = sink_dir.join("telemetry.jsonl");

    let opened = Engine::open(&path).expect("open");
    opened.engine.enable_telemetry(sink.to_str().unwrap()).expect("enable telemetry");

    write_node(&opened.engine, "erasable zeta payload", "S1", Some("victim-1"));
    write_node(&opened.engine, "retained omega payload", "S2", Some("control-1"));
    opened.engine.drain(10_000).expect("drain");

    let hits = opened.engine.search("zeta").expect("search zeta");
    assert!(
        hits.results.iter().any(|h| h.id.to_prefixed() == "l:victim-1"),
        "fixture: the victim id must be captured into the sink"
    );
    let control_hits = opened.engine.search("omega").expect("search omega");
    assert!(
        control_hits.results.iter().any(|h| h.id.to_prefixed() == "l:control-1"),
        "fixture: the control id must be captured into the sink, so the selectivity \
         assertion below is not vacuous"
    );
    assert!(
        std::fs::read_to_string(&sink).expect("read sink").contains("l:victim-1"),
        "fixture: the victim id must be on disk in the sink before the erasure"
    );

    // --- Attempt 1: redaction fails after the delete commits. ---
    chmod(&sink_dir, 0o555);
    // Non-vacuity guard: if the sandbox ignores the mode bits (e.g. running as
    // root) no failure is injected and the whole test proves nothing.
    let probe = sink_dir.join("write-probe");
    if std::fs::write(&probe, b"x").is_ok() {
        let _ = std::fs::remove_file(&probe);
        chmod(&sink_dir, 0o755);
        panic!(
            "cannot inject a redaction failure: the sink directory is still writable at mode \
             0555 (running as root?) — this test would be vacuous"
        );
    }

    let err = opened
        .engine
        .excise_source("S1")
        .expect_err("excise must not report success while the sink cannot be redacted");
    match &err {
        EngineError::ErasureIncomplete { stage, .. } => assert_eq!(
            stage, "telemetry_redaction",
            "the erasure must fail at the redaction stage, not somewhere else"
        ),
        other => panic!("expected ErasureIncomplete{{telemetry_redaction}}, got {other:?}"),
    }

    // The canonical rows ARE gone (the delete committed) — that is precisely what
    // makes the naive retry recompute an empty id set. RAW state, per Rule 1.
    chmod(&sink_dir, 0o755);
    let probe_conn = Connection::open(&path).expect("probe connection");
    let remaining: u64 = probe_conn
        .query_row("SELECT COUNT(*) FROM canonical_nodes WHERE source_id = 'S1'", [], |row| {
            row.get(0)
        })
        .expect("count S1 rows");
    drop(probe_conn);
    assert_eq!(remaining, 0, "precondition: the delete transaction committed");
    assert!(
        std::fs::read_to_string(&sink).expect("read sink").contains("l:victim-1"),
        "precondition: the leaked id is still in the sink after the failed redaction"
    );

    // --- Attempt 2: the retry must actually finish the redaction. ---
    opened.engine.excise_source("S1").expect("the retry must complete the pending redaction");

    let after = std::fs::read_to_string(&sink).expect("read sink after retry");
    assert!(
        !after.contains("l:victim-1"),
        "the retry reported SUCCESS while the erased stable id is STILL in the telemetry \
         sink — an erasure verb must never report success on an incomplete erasure \
         (R-20-E5):\n{after}"
    );
    // The retry must stay selective: it redacts what was owed, nothing more.
    assert!(
        after.contains("l:control-1"),
        "the retry destroyed a retained id's telemetry record:\n{after}"
    );

    opened.engine.close().unwrap();
}

// ===== R-20-E7 · Op-store record erasure ==============================

/// The op-store had no record-level delete at all: `enforce_provenance_retention`
/// is a cap sweep, not an erasure verb, so a caller holding an erasure
/// obligation over an op-store record had no way to discharge it.
#[test]
fn op_store_record_erasable_by_key() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "op_store_erase");
    let opened = Engine::open(&path).expect("open");

    register_collection(&opened.engine, "events", "append_only_log");
    append_op_record(&opened.engine, "events", "subject-a", r#"{"pii":"ERASABLERECORDBODY"}"#);
    append_op_record(&opened.engine, "events", "subject-a", r#"{"pii":"ERASABLERECORDBODY2"}"#);
    append_op_record(&opened.engine, "events", "subject-b", r#"{"pii":"RETAINEDRECORDBODY"}"#);

    let report = opened
        .engine
        .excise_collection_record("events", "subject-a")
        .expect("excise_collection_record");
    assert_eq!(report.records_excised, 2, "both versions of the keyed record must be erased");

    opened.engine.close().unwrap();
    let conn = Connection::open(&path).expect("open sqlite");
    let remaining: Vec<(String, String)> = conn
        .prepare(
            "SELECT record_key, payload_json FROM operational_mutations
             WHERE collection_name = 'events' ORDER BY id",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert!(
        !remaining.iter().any(|(k, _)| k == "subject-a"),
        "erased record key survives in operational_mutations: {remaining:?}"
    );
    assert!(
        remaining.iter().any(|(k, _)| k == "subject-b"),
        "non-erased record must survive: {remaining:?}"
    );
    // The audit row must not re-introduce the erased key (it stores a digest).
    let audit_leak: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM operational_mutations WHERE record_key = 'subject-a'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(audit_leak, 0, "erasure audit must not persist the erased record key verbatim");
}

// ===== Item 9a · Erasure-audit durability (defect D-A) ================

/// **Defect D-A.** The `excise_source_audit` row is written into
/// `operational_mutations`, and `enforce_provenance_retention` sweeps THAT SAME
/// table cap-first, oldest-`id`-first, with NO collection filter. The proof of
/// erasure is therefore destructible, and shares a retention pool with the very
/// payloads it must prove erased.
///
/// The audit row is written FIRST here (lowest `id`), so an unfiltered
/// oldest-first sweep evicts it before anything else.
#[test]
fn erasure_audit_survives_retention_sweep() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "audit_durability");
    let opened = Engine::open(&path).expect("open");

    write_node(&opened.engine, "auditable erasable body", "S1", Some("victim-1"));
    opened.engine.drain(10_000).expect("drain");
    opened.engine.excise_source("S1").expect("excise_source");

    let audit_rows_before = count_audit_rows(&opened.engine);
    assert_eq!(audit_rows_before, 1, "fixture: one excise_source_audit row");

    // Drive the retention sweep hard: a small cap plus far more ordinary
    // op-store rows than the cap allows.
    register_collection(&opened.engine, "events", "append_only_log");
    opened.engine.set_provenance_row_cap_for_test(Some(4));
    for i in 0..60 {
        append_op_record(&opened.engine, "events", &format!("k{i}"), &format!(r#"{{"n":{i}}}"#));
    }

    let total = opened.engine.provenance_row_count_for_test().expect("row count");
    assert!(total <= 12, "fixture: the sweep must actually have evicted rows (total = {total})");
    assert_eq!(
        count_audit_rows(&opened.engine),
        1,
        "the erasure audit row was swept away by enforce_provenance_retention: the proof of \
         erasure is destructible and shares a retention pool with the payloads it must prove erased"
    );

    opened.engine.close().unwrap();
}

fn count_audit_rows(engine: &Engine) -> u64 {
    let rows = engine
        .read_collection("excise_source_audit", None, 1000)
        .expect("read excise_source_audit");
    rows.len() as u64
}

// ===== 0.8.20 Slice 5 fix-3 · protecting the erasure bookkeeping ======

/// Raw count of rows in an op-store collection. **Rule 1** — the witness reads
/// the physical table, never a verb's own report and never `search()`.
fn raw_collection_row_count(path: &Path, collection: &str) -> u64 {
    let conn = Connection::open(path).expect("probe connection");
    let count: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM operational_mutations WHERE collection_name = ?1",
            [collection],
            |row| row.get(0),
        )
        .expect("count collection rows");
    count
}

/// Drive an erasure to the point where a telemetry redaction is OWED but not
/// performed, leaving a durable row in `erasure_pending_redaction`.
///
/// Failure is injected by making the sink's directory read-only, so
/// `redact_jsonl_stable_ids` cannot create its `.redact.tmp` sibling
/// (`PermissionDenied`). That injection is deliberately INDEPENDENT of the
/// `NotFound` path this slice also changes, so the caller's RED is attributable.
#[cfg(unix)]
fn pending_redaction_fixture(
    name: &str,
) -> (TempDir, PathBuf, PathBuf, fathomdb_engine::OpenedEngine) {
    use std::os::unix::fs::PermissionsExt;

    fn chmod(dir: &Path, mode: u32) {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode)).expect("chmod");
    }

    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, name);
    let sink_dir = dir.path().join("sink");
    std::fs::create_dir(&sink_dir).expect("create sink dir");
    let sink = sink_dir.join("telemetry.jsonl");

    let opened = Engine::open(&path).expect("open");
    opened.engine.enable_telemetry(sink.to_str().unwrap()).expect("enable telemetry");

    write_node(&opened.engine, "erasable zeta payload", "S1", Some("victim-1"));
    write_node(&opened.engine, "retained omega payload", "S2", Some("control-1"));
    opened.engine.drain(10_000).expect("drain");
    opened.engine.search("zeta").expect("search zeta");
    opened.engine.search("omega").expect("search omega");
    assert!(
        std::fs::read_to_string(&sink).expect("read sink").contains("l:victim-1"),
        "fixture: the victim id must be on disk in the sink before the erasure"
    );

    chmod(&sink_dir, 0o555);
    let probe = sink_dir.join("write-probe");
    if std::fs::write(&probe, b"x").is_ok() {
        let _ = std::fs::remove_file(&probe);
        chmod(&sink_dir, 0o755);
        panic!(
            "cannot inject a redaction failure: the sink directory is still writable at mode \
             0555 (running as root?) — this fixture would be vacuous"
        );
    }
    let err = opened
        .engine
        .excise_source("S1")
        .expect_err("excise must not report success while the sink cannot be redacted");
    assert!(
        matches!(&err, EngineError::ErasureIncomplete { stage, .. } if stage == "telemetry_redaction"),
        "fixture: expected ErasureIncomplete{{telemetry_redaction}}, got {err:?}"
    );
    chmod(&sink_dir, 0o755);

    let pending = raw_collection_row_count(&path, "erasure_pending_redaction");
    assert_eq!(pending, 1, "fixture: exactly one durable pending-redaction obligation");
    (dir, path, sink, opened)
}

/// **codex §9 round-3 P1 — `excise_collection_record` must not be able to delete
/// the erasure-durability guarantee.**
///
/// `erasure_pending_redaction` is the DURABLE record of a telemetry redaction the
/// engine still owes. `excise_collection_record` is a generic op-store record
/// delete reachable by an operator through
/// `--excise-collection erasure_pending_redaction --excise-record-key <verb>`.
/// Aimed there it removes the obligation, after which `complete_erasure_at_rest`
/// sees an empty queue and every subsequent erasure verb reports SUCCESS while
/// the erased `l:`/`h:` ids are still sitting in the telemetry sink — the exact
/// R-20-E5 violation ("an erasure verb must NEVER report success on an
/// incomplete erasure") that the queue was introduced to close, re-opened
/// through an operator-reachable path.
///
/// The erasure-AUDIT collections are protected on the same footing: the HITL
/// ruling of 2026-07-19 is that *"there must be an auditable record of deletion
/// event"*, and a per-record delete of audit rows would defeat a ruled-on
/// guarantee just as surely as a retention sweep would.
///
/// Precedent for the shape: `erase_source` refuses the reserved `_`-prefixed
/// provenance namespace while `excise_source` stays permissive. Refusal is a
/// TYPED error, never a silent no-op.
#[cfg(unix)]
#[test]
fn erasure_bookkeeping_collections_are_not_excisable() {
    let (_dir, path, sink, opened) = pending_redaction_fixture("bookkeeping_protected");

    // --- The pending queue must refuse to be excised. ---
    let err = opened
        .engine
        .excise_collection_record("erasure_pending_redaction", "excise_source")
        .expect_err(
            "excise_collection_record must REFUSE the engine-internal pending-redaction queue: \
             deleting an outstanding erasure obligation lets the next verb report success on an \
             incomplete erasure (R-20-E5)",
        );
    assert!(
        format!("{err:?}").to_lowercase().contains("erasure"),
        "the refusal must name the erasure-bookkeeping reason, got {err:?}"
    );
    assert_eq!(
        raw_collection_row_count(&path, "erasure_pending_redaction"),
        1,
        "the durable pending-redaction obligation was deleted by excise_collection_record"
    );

    // --- The erasure audit must refuse to be excised. ---
    let audit_before = raw_collection_row_count(&path, "excise_source_audit");
    assert_eq!(audit_before, 1, "fixture: one excise_source_audit row");
    opened
        .engine
        .excise_collection_record("excise_source_audit", "S1")
        .expect_err("excise_collection_record must REFUSE the erasure-audit collection");
    assert_eq!(
        raw_collection_row_count(&path, "excise_source_audit"),
        1,
        "the erasure audit row was deleted one-by-one by excise_collection_record; the HITL \
         ruling requires an auditable record of the deletion event"
    );
    opened
        .engine
        .excise_collection_record("excise_record_audit", "anything")
        .expect_err("excise_collection_record must REFUSE the record-erasure audit collection");

    // --- The obligation is still dischargeable, and IS discharged. ---
    opened.engine.excise_source("S1").expect("the retry must complete the pending redaction");
    let after = std::fs::read_to_string(&sink).expect("read sink after retry");
    assert!(
        !after.contains("l:victim-1"),
        "an erasure verb reported SUCCESS while the erased stable id is STILL in the telemetry \
         sink — the pending obligation was lost (R-20-E5):\n{after}"
    );
    assert!(after.contains("l:control-1"), "the retry destroyed a retained id's record:\n{after}");
    assert_eq!(
        raw_collection_row_count(&path, "erasure_pending_redaction"),
        0,
        "a discharged obligation must be retired from the queue"
    );

    // Ordinary op-store collections stay excisable — this is a targeted refusal,
    // not a disabling of the verb.
    register_collection(&opened.engine, "events", "append_only_log");
    append_op_record(&opened.engine, "events", "subject-a", r#"{"pii":"BODY"}"#);
    opened
        .engine
        .excise_collection_record("events", "subject-a")
        .expect("ordinary op-store records must remain excisable");

    opened.engine.close().unwrap();
}

/// **codex §9 round-3 P2 — a MOVED (rotated) sink is not a redacted sink.**
///
/// `redact_jsonl_stable_ids` surfaces `NotFound` when the sink path does not
/// exist. Treating that as "nothing to redact" and clearing the pending queue is
/// unsound: `enable_telemetry` CREATES the sink, so within the life of an armed
/// engine `NotFound` means the file was created and then vanished — and a path
/// cannot distinguish `rm` from `mv`. Log rotation is an ordinary operational
/// event, and after it the erased `l:`/`h:` ids are fully readable under the
/// rotated name. The burden of proof is on DISCHARGING the obligation, so
/// `NotFound` must remain `ErasureIncomplete`.
#[test]
fn rotated_telemetry_sink_is_not_treated_as_redacted() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "sink_rotated");
    let sink_dir = dir.path().join("sink");
    std::fs::create_dir(&sink_dir).expect("create sink dir");
    let sink = sink_dir.join("telemetry.jsonl");

    let opened = Engine::open(&path).expect("open");
    opened.engine.enable_telemetry(sink.to_str().unwrap()).expect("enable telemetry");
    write_node(&opened.engine, "erasable zeta payload", "S1", Some("victim-1"));
    write_node(&opened.engine, "retained omega payload", "S2", Some("control-1"));
    opened.engine.drain(10_000).expect("drain");
    opened.engine.search("zeta").expect("search zeta");
    opened.engine.search("omega").expect("search omega");
    assert!(
        std::fs::read_to_string(&sink).expect("read sink").contains("l:victim-1"),
        "fixture: the victim id must be in the sink before rotation"
    );

    // Log rotation: the sink is MOVED aside, not deleted. Its contents — the
    // erased ids included — remain fully readable under the rotated name.
    let rotated = sink_dir.join("telemetry.jsonl.1");
    std::fs::rename(&sink, &rotated).expect("rotate sink");

    let err = opened.engine.excise_source("S1").expect_err(
        "a moved/rotated sink must NOT be treated as successfully redacted: the erased stable \
         ids are still readable under the rotated name, so the erasure is incomplete",
    );
    match &err {
        EngineError::ErasureIncomplete { stage, .. } => {
            assert_eq!(stage, "telemetry_redaction", "wrong stage for a rotated sink")
        }
        other => panic!("expected ErasureIncomplete{{telemetry_redaction}}, got {other:?}"),
    }

    // The leak is real, not hypothetical: the ids are readable right now.
    let rotated_body = std::fs::read_to_string(&rotated).expect("read rotated sink");
    assert!(
        rotated_body.contains("l:victim-1"),
        "fixture: the rotated file must still hold the erased id, else the test proves nothing"
    );
    // And the obligation survived, so it can still be discharged.
    assert_eq!(
        raw_collection_row_count(&path, "erasure_pending_redaction"),
        1,
        "the pending obligation must survive a NotFound sink, not be cleared by it"
    );

    // Restoring the sink at the expected path lets the retry finish the job.
    std::fs::rename(&rotated, &sink).expect("restore sink");
    opened.engine.excise_source("S1").expect("retry after restoring the sink");
    let after = std::fs::read_to_string(&sink).expect("read sink after retry");
    assert!(!after.contains("l:victim-1"), "the retry left the erased id in the sink:\n{after}");
    assert!(after.contains("l:control-1"), "the retry destroyed a retained id's record:\n{after}");

    opened.engine.close().unwrap();
}

/// Guard: the bounded WAL retry must not wedge a verb for an unbounded time.
#[test]
fn erasure_wal_retry_is_bounded() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "wal_bounded");
    let opened = Engine::open(&path).expect("open");
    write_node(&opened.engine, "bounded retry body", "S1", Some("victim-1"));
    opened.engine.drain(10_000).expect("drain");

    let blocker = Connection::open(&path).expect("blocker connection");
    blocker.execute_batch("BEGIN").expect("begin");
    let _pinned: u64 = blocker
        .query_row("SELECT COUNT(*) FROM canonical_nodes", [], |row| row.get(0))
        .expect("pin snapshot");

    let started = std::time::Instant::now();
    let _ = opened.engine.excise_source("S1");
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "the bounded WAL retry must give up quickly; took {elapsed:?}"
    );

    blocker.execute_batch("COMMIT").expect("release");
    drop(blocker);
    opened.engine.close().unwrap();
}
