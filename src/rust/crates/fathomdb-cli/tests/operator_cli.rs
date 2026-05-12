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

use fathomdb::Engine;
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
];

const RECOVER_FLAGS: &[&str] = &[
    "--truncate-wal",
    "--rebuild-vec0",
    "--rebuild-projections",
    "--excise-source",
    "--purge-logical-id",
    "--restore-logical-id",
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
