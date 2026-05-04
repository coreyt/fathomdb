//! Operator CLI invocability + output-shape assertions for the 0.6.0
//! surface owned by `dev/interfaces/cli.md`.
//!
//! Bound ACs (invocability + shape only — no engine behavior):
//!
//! - AC-035d (CLI half): `fathomdb recover --help` exits 0.
//! - AC-040a: every `fathomdb doctor <verb> --help` exits 0.
//! - AC-040b: every `fathomdb doctor <verb> --help` prints a `Usage:` line.
//! - AC-058: `fathomdb recover --help` enumerates the six recover sub-flags
//!   plus `--accept-data-loss`.
//! - Not-implemented JSON shape: every doctor verb and `recover` print
//!   `{"status":"not_implemented","verb":"<name>"}` and exit
//!   `exit_code::UNRECOVERABLE` (70). This pins the surface contract pinned
//!   by `fathomdb_cli::run` in 0.6.0; verb bodies still do not touch a
//!   database.
//!
//! These tests invoke the built `fathomdb` binary via `env!("CARGO_BIN_EXE_*")`
//! so they exercise clap's runtime behavior, not just the parser surface
//! pinned by `tests/parser.rs`.
//!
//! The two `#[ignore]`'d not_implemented JSON shape tests are scaffold-phase
//! pins; Phase 9 verb-body wiring will retarget them to real exit-class
//! semantics.

use std::process::Command;

use fathomdb_cli::exit_code;
use serde_json::Value;

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

/// Argument vector and expected `verb` field for a doctor verb invocation.
fn doctor_invocation(verb: &str) -> (Vec<&'static str>, &'static str) {
    match verb {
        "check-integrity" => (vec!["doctor", "check-integrity"], "doctor:check-integrity"),
        "safe-export" => {
            (vec!["doctor", "safe-export", "/tmp/fathomdb-test-export"], "doctor:safe-export")
        }
        "verify-embedder" => (vec!["doctor", "verify-embedder"], "doctor:verify-embedder"),
        "trace" => (vec!["doctor", "trace", "--source-ref", "src-test"], "doctor:trace"),
        "dump-schema" => (vec!["doctor", "dump-schema"], "doctor:dump-schema"),
        "dump-row-counts" => (vec!["doctor", "dump-row-counts"], "doctor:dump-row-counts"),
        "dump-profile" => (vec!["doctor", "dump-profile"], "doctor:dump-profile"),
        other => panic!("unknown doctor verb in test fixture: {other}"),
    }
}

#[test]
#[ignore = "dep-Phase-9-verb-bodies: pins scaffold not_implemented stub; retarget when verb bodies wire up"]
fn t_doctor_verbs_emit_not_implemented_json_and_exit_70() {
    for verb in DOCTOR_VERBS {
        let (args, expected_verb) = doctor_invocation(verb);
        let output = fathomdb().args(&args).output().expect("spawn");
        assert_eq!(
            output.status.code(),
            Some(exit_code::UNRECOVERABLE),
            "fathomdb doctor {verb} must exit {} (UNRECOVERABLE); got {:?} stderr={}",
            exit_code::UNRECOVERABLE,
            output.status,
            String::from_utf8_lossy(&output.stderr),
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parsed: Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
            panic!("doctor {verb} stdout must be JSON; err={e} stdout={stdout}")
        });
        let obj = parsed.as_object().expect("expected JSON object");
        let keys: std::collections::BTreeSet<&str> = obj.keys().map(String::as_str).collect();
        let expected: std::collections::BTreeSet<&str> = ["status", "verb"].into_iter().collect();
        assert_eq!(keys, expected, "doctor {verb} JSON keys must be exactly {{status, verb}}");
        assert_eq!(obj.get("status").and_then(Value::as_str), Some("not_implemented"));
        assert_eq!(obj.get("verb").and_then(Value::as_str), Some(expected_verb));
    }
}

#[test]
#[ignore = "dep-Phase-9-verb-bodies: pins scaffold not_implemented stub; retarget when verb bodies wire up"]
fn t_recover_emits_not_implemented_json_and_exits_70() {
    let output = fathomdb()
        .args(["recover", "--accept-data-loss", "--truncate-wal"])
        .output()
        .expect("spawn");
    assert_eq!(
        output.status.code(),
        Some(exit_code::UNRECOVERABLE),
        "fathomdb recover must exit {} (UNRECOVERABLE); got {:?} stderr={}",
        exit_code::UNRECOVERABLE,
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("recover stdout must be JSON; err={e} stdout={stdout}"));
    let obj = parsed.as_object().expect("expected JSON object");
    let keys: std::collections::BTreeSet<&str> = obj.keys().map(String::as_str).collect();
    let expected: std::collections::BTreeSet<&str> = ["status", "verb"].into_iter().collect();
    assert_eq!(keys, expected, "recover JSON keys must be exactly {{status, verb}}");
    assert_eq!(obj.get("status").and_then(Value::as_str), Some("not_implemented"));
    assert_eq!(obj.get("verb").and_then(Value::as_str), Some("recover"));
}

#[test]
fn t_recover_refuses_without_accept_data_loss() {
    // CLI-layer guard per `cli.md` § Recover root and `design/recovery.md`:
    // `recover` is the only lossy root and must refuse before any engine
    // interaction unless `--accept-data-loss` is explicitly supplied.
    let output = fathomdb().args(["recover", "--truncate-wal"]).output().expect("spawn");
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
