//! Parser-shape assertions for the operator CLI surface owned by
//! `dev/interfaces/cli.md`.
//!
//! The 0.6.0 surface-stubs slice pins root commands, doctor verbs, recover
//! flag set, and the cross-verb invariant that `--accept-data-loss` belongs
//! to `recover` only (doctor verbs reject it).

use clap::Parser;

use fathomdb_cli::{effective_dump_limit, exit_code, Cli, Command, DoctorCommand};

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
        "/tmp/db.sqlite",
    ]);

    let Command::Recover(args) = cli.command else { panic!("expected recover variant") };
    assert!(args.accept_data_loss);
    assert!(args.truncate_wal);
    assert!(args.rebuild_vec0);
    assert!(args.rebuild_projections);
    assert_eq!(args.excise_source.as_deref(), Some("src-1"));
}

#[test]
fn doctor_check_integrity_accepts_full_flag_set() {
    let cli = parse(&[
        "doctor",
        "check-integrity",
        "--quick",
        "--full",
        "--round-trip",
        "--pretty",
        "/tmp/db.sqlite",
    ]);
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
    let cli = parse(&[
        "doctor",
        "safe-export",
        "/tmp/out",
        "--manifest",
        "/tmp/manifest.json",
        "/tmp/db.sqlite",
    ]);
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
    let cli = parse(&["doctor", "trace", "--source-ref", "src-99", "/tmp/db.sqlite"]);
    let DoctorCommand::Trace(args) = doctor(cli) else {
        panic!("expected trace");
    };
    assert_eq!(args.source_ref, "src-99");

    Cli::try_parse_from(["fathomdb", "doctor", "trace"]).expect_err("trace requires --source-ref");
}

#[test]
fn doctor_simple_verbs_parse() {
    for verb in ["dump-schema", "dump-row-counts", "dump-profile"] {
        let cli = parse(&["doctor", verb, "/tmp/db.sqlite"]);
        match (verb, doctor(cli)) {
            ("dump-schema", DoctorCommand::DumpSchema(args))
            | ("dump-row-counts", DoctorCommand::DumpRowCounts(args))
            | ("dump-profile", DoctorCommand::DumpProfile(args)) => {
                assert!(!args.json, "default --json must be false for {verb}");
            }
            (other, parsed) => panic!("verb {other} parsed unexpectedly as {parsed:?}"),
        }
    }
}

#[test]
fn doctor_verify_embedder_requires_identity_and_dimension() {
    // `cli.md:52` locks `verify-embedder --identity <s> --dimension <n>`.
    let cli = parse(&[
        "doctor",
        "verify-embedder",
        "--identity",
        "model-x:rev-1",
        "--dimension",
        "384",
        "/tmp/db.sqlite",
    ]);
    let DoctorCommand::VerifyEmbedder(args) = doctor(cli) else {
        panic!("expected verify-embedder");
    };
    assert_eq!(args.identity, "model-x:rev-1");
    assert_eq!(args.dimension, 384);
    assert!(!args.json);

    Cli::try_parse_from(["fathomdb", "doctor", "verify-embedder", "/tmp/db.sqlite"])
        .expect_err("verify-embedder requires --identity and --dimension");
}

#[test]
fn every_doctor_verb_accepts_json_flag() {
    // `cli.md` § Output posture: `--json` is the normative machine-readable
    // contract on every verb.
    for verb in [
        "check-integrity",
        "safe-export",
        "verify-embedder",
        "trace",
        "dump-schema",
        "dump-row-counts",
        "dump-profile",
        // 0.8.20 Slice 5d (R-20-E8).
        "orphan-provenance",
    ] {
        let mut argv: Vec<&str> = vec!["fathomdb", "doctor", verb];
        match verb {
            "safe-export" => argv.push("/tmp/out"),
            "trace" => argv.extend(["--source-ref", "src-1"]),
            "verify-embedder" => argv.extend(["--identity", "model-x:rev-1", "--dimension", "384"]),
            _ => {}
        }
        argv.push("--json");
        argv.push("/tmp/db.sqlite");
        Cli::try_parse_from(argv)
            .unwrap_or_else(|e| panic!("doctor {verb} --json must parse; err={e}"));
    }
}

#[test]
fn doctor_rejects_accept_data_loss() {
    let res = Cli::try_parse_from(["fathomdb", "doctor", "check-integrity", "--accept-data-loss"]);
    assert!(res.is_err(), "doctor must reject --accept-data-loss");
}

#[test]
fn doctor_dump_mutations_parses_collection_db_path_and_flags() {
    // Slice 34 (F4-READ / reserved-gap-34): the CLI-only op-store read-back
    // diagnostic `doctor dump-mutations <collection> [--after-id n]
    // [--limit n] [--json] <db_path>`.
    let cli = parse(&[
        "doctor",
        "dump-mutations",
        "events",
        "--after-id",
        "42",
        "--limit",
        "10",
        "--json",
        "/tmp/db.sqlite",
    ]);
    let DoctorCommand::DumpMutations(args) = doctor(cli) else {
        panic!("expected dump-mutations");
    };
    assert_eq!(args.collection, "events");
    assert_eq!(args.after_id, Some(42));
    assert_eq!(args.limit, Some(10));
    assert!(args.json);
    assert_eq!(args.db_path, std::path::PathBuf::from("/tmp/db.sqlite"));

    // Bare form: only the positional collection + db_path, defaults elsewhere.
    let cli = parse(&["doctor", "dump-mutations", "events", "/tmp/db.sqlite"]);
    let DoctorCommand::DumpMutations(args) = doctor(cli) else {
        panic!("expected dump-mutations");
    };
    assert_eq!(args.collection, "events");
    assert_eq!(args.after_id, None);
    assert_eq!(args.limit, None);
    assert!(!args.json);

    // `<collection>` is required.
    Cli::try_parse_from(["fathomdb", "doctor", "dump-mutations"])
        .expect_err("dump-mutations requires <collection> and <db_path>");
}

#[test]
fn doctor_dump_mutations_rejects_accept_data_loss() {
    // Like every doctor verb, `dump-mutations` rejects the recover-owned
    // `--accept-data-loss` flag as unknown.
    let res = Cli::try_parse_from([
        "fathomdb",
        "doctor",
        "dump-mutations",
        "events",
        "--accept-data-loss",
        "/tmp/db.sqlite",
    ]);
    assert!(res.is_err(), "doctor dump-mutations must reject --accept-data-loss");
}

#[test]
fn unknown_root_command_is_rejected() {
    let res = Cli::try_parse_from(["fathomdb", "destroy-everything"]);
    assert!(res.is_err());
}

#[test]
fn json_flag_available_on_doctor_verbs() {
    let cli = parse(&["doctor", "check-integrity", "--json", "/tmp/db.sqlite"]);
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

#[test]
fn dump_mutations_limit_is_clamped_to_the_engine_cap() {
    // The CLI clamps `--limit` to the same ~1M cap the engine applies, so the
    // `next_after_id` "full page" decision compares `rows.len()` against the
    // EFFECTIVE limit. Without the clamp, a `--limit` above the cap makes a full
    // capped page (`rows.len() == cap < requested`) look exhausted →
    // `next_after_id: null` → pagination stops while rows remain in the log.
    assert_eq!(effective_dump_limit(None), 1000, "omitted --limit -> default page");
    assert_eq!(effective_dump_limit(Some(10)), 10, "below cap -> unchanged");
    assert_eq!(effective_dump_limit(Some(0)), 0, "zero -> zero (engine returns an empty page)");
    assert_eq!(
        effective_dump_limit(Some(5_000_000)),
        1_000_000,
        "above the ~1M engine cap -> clamped"
    );
}
