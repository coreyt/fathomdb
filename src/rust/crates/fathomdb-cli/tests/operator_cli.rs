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
//!
//! These tests invoke the built `fathomdb` binary via `env!("CARGO_BIN_EXE_*")`
//! so they exercise clap's runtime behavior, not just the parser surface
//! pinned by `tests/parser.rs`.

use std::process::Command;

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
fn t_035d_doctor_rejects_accept_data_loss_runtime() {
    // Cross-checks the parser-level assertion in tests/parser.rs by exercising
    // the actual built binary: clap must reject `--accept-data-loss` on doctor
    // verbs at runtime, not just at parse-call time.
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
