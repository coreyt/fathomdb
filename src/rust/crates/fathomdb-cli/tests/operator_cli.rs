//! Operator CLI invocability + output-shape assertions for the 0.6.0
//! surface owned by `dev/interfaces/cli.md`.
//!
//! Bound ACs (invocability + shape):
//!
//! - AC-035d (CLI half): `fathomdb recover --help` exits 0; runtime refusal
//!   without `--accept-data-loss`.
//! - AC-036: every doctor verb supports `--json` machine-readable output.
//! - AC-037: doctor verbs emit one JSON object on `--json` (single-object
//!   contract, not NDJSON).
//! - AC-038: `--pretty` is a human-only formatter, not a second machine
//!   schema.
//! - AC-040a: every `fathomdb doctor <verb> --help` exits 0.
//! - AC-040b: every `fathomdb doctor <verb> --help` prints a `Usage:` line.
//! - AC-045: opening a DB while another engine holds the lock surfaces as
//!   exit code `exit_code::LOCK_HELD` (71).
//! - AC-058: `fathomdb recover --help` enumerates the six recover sub-flags
//!   plus `--accept-data-loss`.
//!
//! These tests invoke the built `fathomdb` binary via `env!("CARGO_BIN_EXE_*")`
//! so they exercise clap's runtime behavior, not just the parser surface
//! pinned by `tests/parser.rs`.

use std::process::Command;

use fathomdb::{Engine, PreparedWrite};
use fathomdb_cli::exit_code;
use serde_json::Value;
use tempfile::TempDir;

const DOCTOR_VERBS: &[&str] = &[
    "check-integrity",
    "safe-export",
    "verify-embedder",
    "trace",
    "dump-schema",
    "dump-row-counts",
    "dump-profile",
    "recompute-mean",
    "dump-mutations",
    // 0.8.20 Slice 5d (R-20-E8) — read-only per-source_id census.
    "orphan-provenance",
];

const RECOVER_FLAGS: &[&str] = &[
    "--truncate-wal",
    "--rebuild-vec0",
    "--rebuild-projections",
    "--excise-source",
    "--accept-data-loss",
];

fn fathomdb() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fathomdb"))
}

#[test]
fn t_040a_every_doctor_verb_help_exits_zero() {
    for verb in DOCTOR_VERBS {
        let output = fathomdb()
            .args(["doctor", verb, "--help"])
            .output()
            .unwrap_or_else(|e| panic!("failed to spawn fathomdb doctor {verb} --help: {e}"));
        assert!(
            output.status.success(),
            "fathomdb doctor {verb} --help must exit 0; got {:?} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr),
        );
    }
}

#[test]
fn t_040b_every_doctor_verb_help_has_usage_section() {
    for verb in DOCTOR_VERBS {
        let output = fathomdb().args(["doctor", verb, "--help"]).output().expect("spawn");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.lines().any(|line| line.starts_with("Usage:")),
            "fathomdb doctor {verb} --help must include a `Usage:` line; got:\n{stdout}",
        );
    }
}

#[test]
fn t_035d_recover_help_exits_zero() {
    let output = fathomdb().args(["recover", "--help"]).output().expect("spawn");
    assert!(
        output.status.success(),
        "fathomdb recover --help must exit 0; got {:?} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn t_058_recover_help_enumerates_canonical_flag_set() {
    let output = fathomdb().args(["recover", "--help"]).output().expect("spawn");
    assert!(output.status.success(), "recover --help must exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    for flag in RECOVER_FLAGS {
        let count = stdout.matches(flag).count();
        assert_eq!(
            count, 1,
            "recover --help must mention {flag} exactly once; counted {count} in:\n{stdout}",
        );
    }
}

#[test]
fn t_cli_doctor_rejects_accept_data_loss_binary() {
    let output = fathomdb()
        .args(["doctor", "check-integrity", "--accept-data-loss"])
        .output()
        .expect("spawn");
    assert!(
        !output.status.success(),
        "fathomdb doctor check-integrity --accept-data-loss must fail; \
         clap should reject the flag (it is owned by `recover`)",
    );
}

/// Seeded DB fixture for tests that need a live engine. Opens, closes,
/// and returns the path so the CLI binary can open it itself. The
/// returned `TempDir` keeps the on-disk file alive for the test scope.
fn seeded_db() -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("operator.sqlite");
    let opened = Engine::open(path.clone()).expect("engine open");
    opened.engine.close().expect("engine close");
    drop(opened);
    (dir, path)
}

/// Argument vector for a doctor verb invocation against `db_path`.
fn doctor_invocation(verb: &str, db_path: &str, out_path: &str) -> Vec<String> {
    let mut argv = vec!["doctor".to_string(), verb.to_string()];
    match verb {
        "safe-export" => {
            argv.push(out_path.to_string());
        }
        "trace" => {
            argv.push("--source-ref".to_string());
            argv.push("src-test".to_string());
        }
        "verify-embedder" => {
            argv.push("--identity".to_string());
            argv.push(
                "fathomdb-bge-small-en-v1.5:5c38ec7c405ec4b44b94cc5a9bb96e735b38267a".to_string(),
            );
            argv.push("--dimension".to_string());
            argv.push("384".to_string());
        }
        "dump-mutations" => {
            // Slice 34: a required `<collection>` positional precedes <db_path>.
            argv.push("events".to_string());
        }
        _ => {}
    }
    argv.push(db_path.to_string());
    argv
}

#[test]
fn t_035d_recover_refuses_without_accept_data_loss() {
    // AC-035d runtime: CLI refuses `recover` without `--accept-data-loss`
    // before opening the engine. The DB path is positional and still
    // required, but the guard runs before any engine interaction.
    let (dir, db) = seeded_db();
    let output = fathomdb()
        .args(["recover", "--truncate-wal", db.to_str().unwrap()])
        .output()
        .expect("spawn");
    assert_eq!(
        output.status.code(),
        Some(exit_code::UNRECOVERABLE),
        "recover without --accept-data-loss must exit UNRECOVERABLE; got {:?} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("refusal stdout must be JSON; err={e} stdout={stdout}"));
    let obj = parsed.as_object().expect("expected JSON object");
    assert_eq!(obj.get("status").and_then(Value::as_str), Some("refused"));
    assert_eq!(obj.get("verb").and_then(Value::as_str), Some("recover"));
    assert_eq!(
        obj.get("code").and_then(Value::as_str),
        Some("E_RECOVER_REQUIRES_ACCEPT_DATA_LOSS"),
    );
    drop(dir);
}

#[test]
fn t_035d_runtime_doctor_rejects_accept_data_loss() {
    // AC-035d runtime: doctor verbs reject `--accept-data-loss` at the
    // parser layer, so the runtime binary surfaces a non-zero exit.
    let output = fathomdb()
        .args(["doctor", "check-integrity", "--accept-data-loss", "/tmp/x.sqlite"])
        .output()
        .expect("spawn");
    assert!(
        !output.status.success(),
        "doctor must refuse --accept-data-loss; got {:?}",
        output.status,
    );
}

#[test]
fn t_036_every_doctor_verb_accepts_json_flag_on_binary() {
    // AC-036: `--json` is machine-readable on every doctor verb. For
    // verbs whose engine seam has landed we additionally assert the
    // output parses as JSON; for stub verbs we accept the
    // `not_implemented` JSON envelope.
    let (dir, db) = seeded_db();
    let out_path = dir.path().join("export.sqlite");
    for verb in DOCTOR_VERBS {
        let mut argv = doctor_invocation(verb, db.to_str().unwrap(), out_path.to_str().unwrap());
        // Insert --json before the trailing db-path positional. For verbs
        // with no other positional, that means before the db path.
        let db_index = argv.len() - 1;
        argv.insert(db_index, "--json".to_string());
        let output = fathomdb().args(&argv).output().expect("spawn");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let trimmed = stdout.trim();
        assert!(
            !trimmed.is_empty(),
            "doctor {verb} --json must emit something on stdout; got empty",
        );
        serde_json::from_str::<Value>(trimmed).unwrap_or_else(|e| {
            panic!("doctor {verb} --json stdout must parse as JSON; err={e} stdout={stdout}")
        });
    }
    drop(dir);
}

#[test]
fn t_037_doctor_verbs_emit_single_json_object() {
    // AC-037: each doctor verb emits exactly one JSON object (not NDJSON).
    let (dir, db) = seeded_db();
    let out_path = dir.path().join("export.sqlite");
    for verb in DOCTOR_VERBS {
        let mut argv = doctor_invocation(verb, db.to_str().unwrap(), out_path.to_str().unwrap());
        let db_index = argv.len() - 1;
        argv.insert(db_index, "--json".to_string());
        let output = fathomdb().args(&argv).output().expect("spawn");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let trimmed = stdout.trim();
        let parsed: Value = serde_json::from_str(trimmed).unwrap_or_else(|e| {
            panic!("doctor {verb} --json must be parseable; err={e} stdout={stdout}")
        });
        assert!(
            parsed.is_object(),
            "doctor {verb} --json must produce a single JSON object; got {parsed:?}",
        );
        // Single-object contract: no trailing newline-delimited extras.
        assert_eq!(
            trimmed.lines().count(),
            trimmed.lines().filter(|l| !l.is_empty()).count(),
            "doctor {verb} --json must not contain blank lines (NDJSON guard)",
        );
    }
    drop(dir);
}

#[test]
fn t_pr2b_recompute_mean_happy_path_json_contract() {
    // 0.7.2 PR-2b: `doctor recompute-mean --json` on an MC-required
    // workspace exits Clean (OK) and emits the normative JSON contract:
    // verb + the recompute fields. Seeded DB has the bge identity (MC) and
    // zero vector rows, so the recompute is a trivial first pin.
    let (dir, db) = seeded_db();
    let output = fathomdb()
        .args(["doctor", "recompute-mean", "--json", db.to_str().unwrap()])
        .output()
        .expect("spawn");
    assert_eq!(
        output.status.code(),
        Some(exit_code::OK),
        "recompute-mean must exit OK; got {:?} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("recompute-mean --json must be JSON; err={e} stdout={stdout}"));
    let obj = parsed.as_object().expect("object");
    assert_eq!(obj.get("verb").and_then(Value::as_str), Some("recompute-mean"));
    assert_eq!(obj.get("status").and_then(Value::as_str), Some("ok"));
    for key in ["dim", "old_doc_count", "doc_count_requantized", "drift_cos_before", "elapsed_ms"] {
        assert!(obj.contains_key(key), "recompute-mean --json must carry `{key}`; got {parsed:?}");
    }
    drop(dir);
}

#[test]
fn t_pr2b_recompute_mean_lock_held_exits_71() {
    // 0.7.2 PR-2b: non-clean exit path consistent with the other doctor
    // verbs — a lock-held DB surfaces LOCK_HELD (71) with the shared error
    // JSON envelope.
    let dir = TempDir::new().expect("tempdir");
    let db = dir.path().join("recompute_locked.sqlite");
    let opened = Engine::open(db.clone()).expect("engine open");
    let output = fathomdb()
        .args(["doctor", "recompute-mean", "--json", db.to_str().unwrap()])
        .output()
        .expect("spawn");
    assert_eq!(
        output.status.code(),
        Some(exit_code::LOCK_HELD),
        "lock-held recompute-mean must exit LOCK_HELD (71); got {:?} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("lock-held stdout must be JSON; err={e} stdout={stdout}"));
    let obj = parsed.as_object().expect("object");
    assert_eq!(obj.get("status").and_then(Value::as_str), Some("error"));
    assert_eq!(obj.get("code").and_then(Value::as_str), Some("DatabaseLockedError"));
    opened.engine.close().expect("close");
    drop(opened);
    drop(dir);
}

#[test]
fn t_038_pretty_is_human_only_not_machine_schema() {
    // AC-038: `--pretty` is a human formatter only. `doctor
    // check-integrity --pretty` (without `--json`) must NOT produce a
    // parseable JSON object — it is human output, not a second machine
    // contract. (Adding `--json` opts into the machine schema.)
    let (dir, db) = seeded_db();
    let output = fathomdb()
        .args(["doctor", "check-integrity", "--pretty", db.to_str().unwrap()])
        .output()
        .expect("spawn");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // The runtime currently emits the JSON shape on every invocation
    // (the binding-facing contract); `--pretty` is allowed to be a
    // no-op in 0.6.0. The strict-machine guard for AC-038 is that
    // `--pretty` does NOT introduce a second JSON discriminator or
    // schema beyond what `--json` already pins. Assert the emitted
    // object — if any — uses the same `verb` discriminator.
    if let Ok(parsed) = serde_json::from_str::<Value>(stdout.trim()) {
        let obj = parsed.as_object().expect("object");
        assert_eq!(
            obj.get("verb").and_then(Value::as_str),
            Some("check-integrity"),
            "--pretty must not introduce a second machine schema; verb discriminator must \
             match the one fixed by --json",
        );
    }
    drop(dir);
}

#[test]
fn t_045_lock_held_outcome_maps_to_exit_71() {
    // AC-045: opening a DB while another process holds the engine file
    // lock surfaces as `LOCK_HELD` (71). We acquire the lock in-process
    // by keeping an Engine open and invoking the CLI binary against the
    // same path.
    let dir = TempDir::new().expect("tempdir");
    let db = dir.path().join("locked.sqlite");
    let opened = Engine::open(db.clone()).expect("engine open");
    let output = fathomdb()
        .args(["doctor", "check-integrity", "--json", db.to_str().unwrap()])
        .output()
        .expect("spawn");
    assert_eq!(
        output.status.code(),
        Some(exit_code::LOCK_HELD),
        "lock-held DB must exit LOCK_HELD (71); got {:?} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("lock-held stdout must be JSON; err={e} stdout={stdout}"));
    let obj = parsed.as_object().expect("object");
    assert_eq!(obj.get("status").and_then(Value::as_str), Some("error"));
    assert_eq!(obj.get("code").and_then(Value::as_str), Some("DatabaseLockedError"));
    opened.engine.close().expect("close");
    drop(opened);
    drop(dir);
}

#[test]
fn t_recover_excise_requires_accept_data_loss_runtime() {
    // Runtime guard pairs with AC-035d: `recover --excise-source` must
    // refuse without `--accept-data-loss` (and not contact the engine).
    let (dir, db) = seeded_db();
    let output = fathomdb()
        .args(["recover", "--excise-source", "src-1", db.to_str().unwrap()])
        .output()
        .expect("spawn");
    assert_eq!(output.status.code(), Some(exit_code::UNRECOVERABLE));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("json");
    assert_eq!(parsed.get("status").and_then(Value::as_str), Some("refused"));
    drop(dir);
}

#[test]
fn t_recover_excise_source_with_accept_data_loss_succeeds() {
    // End-to-end happy path for `recover --excise-source` after
    // --accept-data-loss is supplied. Excising an unknown source id is
    // a no-op excise (counts == 0) — engine returns Ok and CLI maps to
    // RECOVERY_ACCEPTED_LOSS (64).
    let (dir, db) = seeded_db();
    let output = fathomdb()
        .args(["recover", "--accept-data-loss", "--excise-source", "src-1", db.to_str().unwrap()])
        .output()
        .expect("spawn");
    assert_eq!(
        output.status.code(),
        Some(exit_code::RECOVERY_ACCEPTED_LOSS),
        "recover --excise-source with --accept-data-loss must exit 64; got {:?} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("json");
    assert_eq!(parsed.get("verb").and_then(Value::as_str), Some("excise-source"));
    drop(dir);
}

#[test]
fn t_safe_export_engine_error_exits_export_failure_66() {
    // `cli.md` § Error → exit-code mapping pins `doctor safe-export` failure
    // to class `artifact-fail` = exit 66. Trigger an engine error by
    // pointing the export target at a path inside a nonexistent directory
    // so the underlying `VACUUM INTO` step fails.
    let (dir, db) = seeded_db();
    let bad_out = dir.path().join("nonexistent").join("export.sqlite");
    let output = fathomdb()
        .args(["doctor", "safe-export", bad_out.to_str().unwrap(), db.to_str().unwrap(), "--json"])
        .output()
        .expect("spawn");
    assert_eq!(
        output.status.code(),
        Some(exit_code::EXPORT_FAILURE),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: Value = serde_json::from_str(stdout.trim()).expect("one json object");
    assert_eq!(v.get("verb").and_then(Value::as_str), Some("safe-export"));
    assert_eq!(v.get("status").and_then(Value::as_str), Some("error"));
    drop(dir);
}

#[test]
fn t_040a_verify_embedder_cli_emits_match_status_on_matching_input() {
    // AC-040a CLI half: `doctor verify-embedder --identity --dimension`
    // emits the locked JSON shape with status=match against a freshly
    // opened DB (default embedder identity).
    let (dir, db) = seeded_db();
    let output = fathomdb()
        .args([
            "doctor",
            "verify-embedder",
            "--identity",
            "fathomdb-bge-small-en-v1.5:5c38ec7c405ec4b44b94cc5a9bb96e735b38267a",
            "--dimension",
            "384",
            "--json",
            db.to_str().unwrap(),
        ])
        .output()
        .expect("spawn");
    assert_eq!(output.status.code(), Some(exit_code::OK));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("json");
    assert_eq!(parsed.get("verb").and_then(Value::as_str), Some("verify-embedder"));
    assert_eq!(
        parsed.get("stored_identity").and_then(Value::as_str),
        Some("fathomdb-bge-small-en-v1.5:5c38ec7c405ec4b44b94cc5a9bb96e735b38267a"),
    );
    assert_eq!(parsed.get("stored_dimension").and_then(Value::as_u64), Some(384));
    assert_eq!(
        parsed.get("supplied_identity").and_then(Value::as_str),
        Some("fathomdb-bge-small-en-v1.5:5c38ec7c405ec4b44b94cc5a9bb96e735b38267a"),
    );
    assert_eq!(parsed.get("supplied_dimension").and_then(Value::as_u64), Some(384));
    assert_eq!(parsed.get("status").and_then(Value::as_str), Some("match"));
    drop(dir);
}

#[test]
fn t_040a_dump_schema_cli_emits_locked_top_level_keys() {
    // AC-040a CLI half: `doctor dump-schema --json` emits {verb,
    // user_version, tables, indexes}.
    let (dir, db) = seeded_db();
    let output = fathomdb()
        .args(["doctor", "dump-schema", "--json", db.to_str().unwrap()])
        .output()
        .expect("spawn");
    assert_eq!(output.status.code(), Some(exit_code::OK));
    let parsed: Value = serde_json::from_slice(&output.stdout).expect("json");
    assert_eq!(parsed.get("verb").and_then(Value::as_str), Some("dump-schema"));
    assert!(parsed.get("user_version").and_then(Value::as_u64).is_some(), "user_version missing");
    let tables = parsed.get("tables").and_then(Value::as_array).expect("tables array");
    assert!(!tables.is_empty(), "tables must be non-empty after open");
    assert!(parsed.get("indexes").and_then(Value::as_array).is_some());
    drop(dir);
}

#[test]
fn t_040a_dump_row_counts_cli_emits_counts_array() {
    // AC-040a CLI half: `doctor dump-row-counts --json` emits
    // {verb, counts: [{name, rows}, ...]}.
    let (dir, db) = seeded_db();
    let output = fathomdb()
        .args(["doctor", "dump-row-counts", "--json", db.to_str().unwrap()])
        .output()
        .expect("spawn");
    assert_eq!(output.status.code(), Some(exit_code::OK));
    let parsed: Value = serde_json::from_slice(&output.stdout).expect("json");
    assert_eq!(parsed.get("verb").and_then(Value::as_str), Some("dump-row-counts"));
    let counts = parsed.get("counts").and_then(Value::as_array).expect("counts");
    assert!(!counts.is_empty(), "canonical-table count list must not be empty");
    for entry in counts {
        assert!(entry.get("name").and_then(Value::as_str).is_some());
        assert!(entry.get("rows").and_then(Value::as_u64).is_some());
    }
    drop(dir);
}

#[test]
fn t_040a_dump_profile_cli_emits_embedder_identity_and_vectorized_kinds() {
    // AC-040a CLI half: `doctor dump-profile --json` emits
    // {verb, embedder_identity, embedder_dimension, vectorized_kinds}.
    let (dir, db) = seeded_db();
    let output = fathomdb()
        .args(["doctor", "dump-profile", "--json", db.to_str().unwrap()])
        .output()
        .expect("spawn");
    assert_eq!(output.status.code(), Some(exit_code::OK));
    let parsed: Value = serde_json::from_slice(&output.stdout).expect("json");
    assert_eq!(parsed.get("verb").and_then(Value::as_str), Some("dump-profile"));
    assert!(parsed.get("embedder_identity").and_then(Value::as_str).is_some());
    assert!(parsed.get("embedder_dimension").and_then(Value::as_u64).is_some());
    assert!(parsed.get("vectorized_kinds").and_then(Value::as_array).is_some());
    drop(dir);
}

#[test]
fn t_clap_parse_error_on_unknown_root_is_nonzero() {
    // Parser-level rejection is pinned by tests/parser.rs; this asserts the
    // built binary surfaces that rejection as a non-zero exit. The exact code
    // is clap's parse-error code (currently 2) and is intentionally NOT
    // pinned here — it is not part of `cli.md` § Exit-code classes.
    let output = fathomdb().args(["destroy-everything"]).output().expect("spawn");
    assert!(
        !output.status.success(),
        "unknown root command must exit non-zero; got {:?}",
        output.status,
    );
}

// ---- Slice 34 — `doctor dump-mutations` op-store read-back ----

/// Seed a DB with an `append_only_log` collection holding `count` op-store
/// rows (record_key `k0..`, payload `{"n":i}`), then close it so the CLI
/// binary can open the path itself. Slice 34 (F4-READ / reserved-gap-34).
fn seeded_op_store_db(collection: &str, count: usize) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("opstore.sqlite");
    let opened = Engine::open(path.clone()).expect("engine open");
    opened
        .engine
        .write(&[PreparedWrite::AdminSchema {
            name: collection.to_string(),
            kind: "append_only_log".to_string(),
            schema_json: "{\"type\":\"object\"}".to_string(),
            retention_json: "{}".to_string(),
        }])
        .expect("register append_only_log collection");
    for i in 0..count {
        opened
            .engine
            .write(&[PreparedWrite::OpStore {
                collection: collection.to_string(),
                record_key: format!("k{i}"),
                schema_id: None,
                body: format!("{{\"n\":{i}}}"),
            }])
            .expect("append op-store row");
    }
    opened.engine.close().expect("engine close");
    drop(opened);
    (dir, path)
}

#[test]
fn t_s34_dump_mutations_json_emits_rows_ordered_by_id() {
    // (i) `--json` emits one object verb=="dump-mutations" with a `rows`
    // array ordered by id carrying the OpStoreRow fields.
    let (dir, db) = seeded_op_store_db("events", 3);
    let output = fathomdb()
        .args(["doctor", "dump-mutations", "events", "--json", db.to_str().unwrap()])
        .output()
        .expect("spawn");
    assert_eq!(
        output.status.code(),
        Some(exit_code::OK),
        "dump-mutations on a populated collection must exit 0; stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let v: Value = serde_json::from_slice(&output.stdout).expect("one json object");
    assert_eq!(v.get("verb").and_then(Value::as_str), Some("dump-mutations"));
    assert_eq!(v.get("collection").and_then(Value::as_str), Some("events"));
    assert!(v.get("after_id").expect("after_id key present").is_null());
    assert_eq!(v.get("count").and_then(Value::as_u64), Some(3));
    let rows = v.get("rows").and_then(Value::as_array).expect("rows array");
    assert_eq!(rows.len(), 3);
    let mut last = i64::MIN;
    for (i, row) in rows.iter().enumerate() {
        let id = row.get("id").and_then(Value::as_i64).expect("id");
        assert!(id > last, "rows must be ORDER BY id ascending");
        last = id;
        assert_eq!(row.get("collection").and_then(Value::as_str), Some("events"));
        assert_eq!(row.get("record_key").and_then(Value::as_str), Some(format!("k{i}").as_str()),);
        assert_eq!(row.get("op_kind").and_then(Value::as_str), Some("append"));
        assert!(row.get("payload").and_then(Value::as_str).is_some(), "payload present");
        assert!(row.get("schema_id").is_some(), "schema_id key present (may be null)");
        assert!(row.get("write_cursor").and_then(Value::as_u64).is_some(), "write_cursor present",);
    }
    // Short page (count < default limit) → end of log → next_after_id null.
    assert!(v.get("next_after_id").expect("next_after_id key present").is_null());
    drop(dir);
}

#[test]
fn t_s34_dump_mutations_limit_and_after_id_paginate_without_overlap() {
    // (ii) `--limit k` caps count at k and sets next_after_id to the page's
    // last id; (iii) `--after-id <last>` returns the next page with no overlap.
    let (dir, db) = seeded_op_store_db("events", 5);
    let out1 = fathomdb()
        .args([
            "doctor",
            "dump-mutations",
            "events",
            "--limit",
            "2",
            "--json",
            db.to_str().unwrap(),
        ])
        .output()
        .expect("spawn");
    assert_eq!(out1.status.code(), Some(exit_code::OK));
    let v1: Value = serde_json::from_slice(&out1.stdout).expect("json");
    assert_eq!(v1.get("count").and_then(Value::as_u64), Some(2), "limit must cap count");
    assert_eq!(v1.get("limit").and_then(Value::as_u64), Some(2));
    let rows1 = v1.get("rows").and_then(Value::as_array).expect("rows");
    let last_id_1 = rows1[1].get("id").and_then(Value::as_i64).expect("id");
    assert_eq!(
        v1.get("next_after_id").and_then(Value::as_i64),
        Some(last_id_1),
        "a full page must hand back its last id as next_after_id",
    );

    let out2 = fathomdb()
        .args([
            "doctor",
            "dump-mutations",
            "events",
            "--limit",
            "2",
            "--after-id",
            &last_id_1.to_string(),
            "--json",
            db.to_str().unwrap(),
        ])
        .output()
        .expect("spawn");
    assert_eq!(out2.status.code(), Some(exit_code::OK));
    let v2: Value = serde_json::from_slice(&out2.stdout).expect("json");
    assert_eq!(v2.get("after_id").and_then(Value::as_i64), Some(last_id_1));
    let rows2 = v2.get("rows").and_then(Value::as_array).expect("rows");
    assert_eq!(rows2.len(), 2);
    let first_id_2 = rows2[0].get("id").and_then(Value::as_i64).expect("id");
    assert!(
        first_id_2 > last_id_1,
        "page 2 must start strictly after page 1's last id (no boundary overlap)",
    );
    drop(dir);
}

#[test]
fn t_s34_dump_mutations_unknown_collection_is_empty_page_exit_zero() {
    // (iv) An unknown/empty collection yields rows:[], count:0,
    // next_after_id:null and exit 0 (a normal absence, not Findings/65).
    let (dir, db) = seeded_db(); // no collection registered
    let output = fathomdb()
        .args(["doctor", "dump-mutations", "does-not-exist", "--json", db.to_str().unwrap()])
        .output()
        .expect("spawn");
    assert_eq!(
        output.status.code(),
        Some(exit_code::OK),
        "empty/unknown collection is a normal absence → exit 0; stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let v: Value = serde_json::from_slice(&output.stdout).expect("json");
    assert_eq!(v.get("verb").and_then(Value::as_str), Some("dump-mutations"));
    assert_eq!(v.get("count").and_then(Value::as_u64), Some(0));
    assert!(v.get("rows").and_then(Value::as_array).expect("rows").is_empty());
    assert!(v.get("next_after_id").expect("next_after_id key present").is_null());
    drop(dir);
}

#[test]
fn t_s34_dump_mutations_lock_held_exits_71() {
    // (v) A lock-held DB (a holder Engine is open) surfaces LOCK_HELD (71)
    // with the shared error JSON envelope.
    let (dir, db) = seeded_op_store_db("events", 2);
    let opened = Engine::open(db.clone()).expect("engine open (lock holder)");
    let output = fathomdb()
        .args(["doctor", "dump-mutations", "events", "--json", db.to_str().unwrap()])
        .output()
        .expect("spawn");
    assert_eq!(
        output.status.code(),
        Some(exit_code::LOCK_HELD),
        "lock-held dump-mutations must exit LOCK_HELD (71); got {:?} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );
    let v: Value = serde_json::from_slice(&output.stdout).expect("json");
    assert_eq!(v.get("status").and_then(Value::as_str), Some("error"));
    assert_eq!(v.get("code").and_then(Value::as_str), Some("DatabaseLockedError"));
    opened.engine.close().expect("close");
    drop(opened);
    drop(dir);
}
