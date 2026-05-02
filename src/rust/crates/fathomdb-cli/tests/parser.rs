//! Parser-shape assertions for the operator CLI surface owned by
//! `dev/interfaces/cli.md`.
//!
//! The 0.6.0 surface-stubs slice pins root commands, doctor verbs, recover
//! flag set, and the cross-verb invariant that `--accept-data-loss` belongs
//! to `recover` only (doctor verbs reject it).

use clap::Parser;

use fathomdb_cli::{exit_code, Cli, Command, DoctorCommand};

fn parse(args: &[&str]) -> Cli {
    let mut argv = vec!["fathomdb"];
    argv.extend_from_slice(args);
    Cli::try_parse_from(argv).expect("parse should succeed")
}

fn doctor(cli: Cli) -> DoctorCommand {
    let Command::Doctor(d) = cli.command else { panic!("expected doctor variant") };
    d.command
}

#[test]
fn recover_accepts_all_six_subflags() {
    let cli = parse(&[
        "recover",
        "--accept-data-loss",
        "--truncate-wal",
        "--rebuild-vec0",
        "--rebuild-projections",
        "--excise-source",
        "src-1",
        "--purge-logical-id",
        "lid-1",
        "--restore-logical-id",
        "lid-2",
    ]);

    let Command::Recover(args) = cli.command else { panic!("expected recover variant") };
    assert!(args.accept_data_loss);
    assert!(args.truncate_wal);
    assert!(args.rebuild_vec0);
    assert!(args.rebuild_projections);
    assert_eq!(args.excise_source.as_deref(), Some("src-1"));
    assert_eq!(args.purge_logical_id.as_deref(), Some("lid-1"));
    assert_eq!(args.restore_logical_id.as_deref(), Some("lid-2"));
}

#[test]
fn doctor_check_integrity_accepts_full_flag_set() {
    let cli =
        parse(&["doctor", "check-integrity", "--quick", "--full", "--round-trip", "--pretty"]);
    let DoctorCommand::CheckIntegrity(args) = doctor(cli) else {
        panic!("expected check-integrity")
    };
    assert!(args.quick);
    assert!(args.full);
    assert!(args.round_trip);
    assert!(args.pretty);
}

#[test]
fn doctor_safe_export_accepts_out_and_manifest() {
    let cli = parse(&["doctor", "safe-export", "/tmp/out", "--manifest", "/tmp/manifest.json"]);
    let DoctorCommand::SafeExport(args) = doctor(cli) else {
        panic!("expected safe-export");
    };
    assert_eq!(args.out, std::path::PathBuf::from("/tmp/out"));
    assert_eq!(
        args.manifest.as_ref().map(|p| p.to_string_lossy().into_owned()).as_deref(),
        Some("/tmp/manifest.json")
    );
}

#[test]
fn doctor_trace_requires_source_ref() {
    let cli = parse(&["doctor", "trace", "--source-ref", "src-99"]);
    let DoctorCommand::Trace(args) = doctor(cli) else {
        panic!("expected trace");
    };
    assert_eq!(args.source_ref, "src-99");

    Cli::try_parse_from(["fathomdb", "doctor", "trace"]).expect_err("trace requires --source-ref");
}

#[test]
fn doctor_simple_verbs_parse() {
    for verb in ["verify-embedder", "dump-schema", "dump-row-counts", "dump-profile"] {
        let cli = parse(&["doctor", verb]);
        match (verb, doctor(cli)) {
            ("verify-embedder", DoctorCommand::VerifyEmbedder) => {}
            ("dump-schema", DoctorCommand::DumpSchema) => {}
            ("dump-row-counts", DoctorCommand::DumpRowCounts) => {}
            ("dump-profile", DoctorCommand::DumpProfile) => {}
            (other, parsed) => panic!("verb {other} parsed unexpectedly as {parsed:?}"),
        }
    }
}

#[test]
fn doctor_rejects_accept_data_loss() {
    let res = Cli::try_parse_from(["fathomdb", "doctor", "check-integrity", "--accept-data-loss"]);
    assert!(res.is_err(), "doctor must reject --accept-data-loss");
}

#[test]
fn unknown_root_command_is_rejected() {
    let res = Cli::try_parse_from(["fathomdb", "destroy-everything"]);
    assert!(res.is_err());
}

#[test]
fn json_flag_available_on_doctor_verbs() {
    let cli = parse(&["doctor", "check-integrity", "--json"]);
    let DoctorCommand::CheckIntegrity(args) = doctor(cli) else {
        panic!("expected check-integrity");
    };
    assert!(args.json);
}

#[test]
fn exit_code_constants_match_design() {
    assert_eq!(exit_code::OK, 0);
    assert_eq!(exit_code::RECOVERY_ACCEPTED_LOSS, 64);
    assert_eq!(exit_code::DOCTOR_FOUND_ISSUES, 65);
    assert_eq!(exit_code::EXPORT_FAILURE, 66);
    assert_eq!(exit_code::UNRECOVERABLE, 70);
    assert_eq!(exit_code::LOCK_HELD, 71);
}
