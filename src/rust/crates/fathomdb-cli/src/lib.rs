//! Operator CLI parser + verb runtime for `fathomdb`.
//!
//! Surface owned by `dev/interfaces/cli.md`. Phase 10a wires the parser
//! scaffold to real engine seam calls: `doctor check-integrity`,
//! `doctor safe-export`, `doctor trace`, `recover --rebuild-projections`,
//! `recover --rebuild-vec0`, and `recover --excise-source` invoke the
//! corresponding [`fathomdb::Engine`] methods and serialize the typed
//! report under the per-verb JSON discriminator.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use fathomdb::{
    CheckIntegrityOpts, CorruptionLocator, Engine, EngineError, EngineOpenError, ExciseReport,
    Finding, IntegrityReport, RebuildKind, RebuildReport, SafeExportArtifact, Section, TraceReport,
};
use serde_json::{json, Value};

/// Stable exit-code classes for the operator CLI.
///
/// Sourced from `dev/interfaces/cli.md` § Exit-code classes; meanings remain
/// load-bearing across `recover` + `doctor` outcomes.
pub mod exit_code {
    /// Successful completion with no findings that require a non-zero exit.
    pub const OK: i32 = 0;

    /// `recover` completed only because lossy action was explicitly accepted.
    pub const RECOVERY_ACCEPTED_LOSS: i32 = 64;

    /// Doctor / verification surface found actionable non-clean state.
    pub const DOCTOR_FOUND_ISSUES: i32 = 65;

    /// Export / materialization failure on an artifact-producing doctor verb.
    pub const EXPORT_FAILURE: i32 = 66;

    /// Unrecoverable command failure.
    pub const UNRECOVERABLE: i32 = 70;

    /// Lock-held or equivalent precondition-blocked outcome.
    pub const LOCK_HELD: i32 = 71;
}

/// Top-level CLI invocation.
#[derive(Debug, Parser)]
#[command(name = "fathomdb", version, about = "FathomDB operator CLI", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

/// Root command verbs.
///
/// 0.6.0 ships exactly two roots per `dev/interfaces/cli.md` § Roots:
/// `recover` for lossy operator workflows and `doctor` for diagnostics.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run a lossy / non-bit-preserving recovery workflow.
    Recover(RecoverArgs),
    /// Run an operator diagnostic verb.
    Doctor(DoctorArgs),
}

/// Wrapper carrying the doctor verb table beneath the `doctor` root.
#[derive(Debug, Args)]
pub struct DoctorArgs {
    #[command(subcommand)]
    pub command: DoctorCommand,
}

/// Argument set for the `recover` root command.
///
/// `--accept-data-loss` is declared on this parser only; doctor verbs reject
/// it as unknown.
#[derive(Debug, Args)]
pub struct RecoverArgs {
    /// Required acknowledgement that the workflow may discard data.
    #[arg(long)]
    pub accept_data_loss: bool,

    /// Truncate the SQLite WAL after replay.
    #[arg(long)]
    pub truncate_wal: bool,

    /// Rebuild the `vec0` shadow tables from canonical state.
    #[arg(long)]
    pub rebuild_vec0: bool,

    /// Rebuild projection materializations.
    #[arg(long)]
    pub rebuild_projections: bool,

    /// Excise the named source row from the canonical store.
    #[arg(long)]
    pub excise_source: Option<String>,

    /// Purge a logical id and all rows it owns.
    #[arg(long)]
    pub purge_logical_id: Option<String>,

    /// Restore a previously purged logical id from operator-supplied source.
    #[arg(long)]
    pub restore_logical_id: Option<String>,

    /// Emit machine-readable JSON output.
    #[arg(long)]
    pub json: bool,

    /// Path to the database file to recover.
    pub db_path: PathBuf,
}

/// Doctor verb table per `dev/interfaces/cli.md` § Doctor verbs.
#[derive(Debug, Subcommand)]
pub enum DoctorCommand {
    /// Run a structural integrity check against the database.
    CheckIntegrity(CheckIntegrityArgs),
    /// Materialize a safe export of the database.
    SafeExport(SafeExportArgs),
    /// Verify the embedder identity recorded in the database.
    VerifyEmbedder(SimpleDoctorArgs),
    /// Trace the resolution chain for a given source reference.
    Trace(TraceArgs),
    /// Dump the canonical schema definition.
    DumpSchema(SimpleDoctorArgs),
    /// Dump per-table row counts.
    DumpRowCounts(SimpleDoctorArgs),
    /// Dump the response-cycle profile recorded by the engine.
    DumpProfile(SimpleDoctorArgs),
}

/// Shared args for doctor verbs whose only options are `--json` and a
/// required `<db_path>` positional. `cli.md` § Output posture: `--json` is
/// the normative machine-readable contract on every verb.
#[derive(Debug, Args)]
pub struct SimpleDoctorArgs {
    /// Emit machine-readable JSON output.
    #[arg(long)]
    pub json: bool,

    /// Path to the database file to inspect.
    pub db_path: PathBuf,
}

/// Per-verb argument set for `doctor check-integrity`.
#[derive(Debug, Args)]
pub struct CheckIntegrityArgs {
    /// Run only the fast integrity probes.
    #[arg(long)]
    pub quick: bool,

    /// Run the full per-page integrity sweep.
    #[arg(long)]
    pub full: bool,

    /// Confirm round-trip equivalence between canonical + projection state.
    #[arg(long = "round-trip")]
    pub round_trip: bool,

    /// Format human output.
    #[arg(long)]
    pub pretty: bool,

    /// Emit machine-readable JSON output.
    #[arg(long)]
    pub json: bool,

    /// Path to the database file to inspect.
    pub db_path: PathBuf,
}

/// Per-verb argument set for `doctor safe-export`.
#[derive(Debug, Args)]
pub struct SafeExportArgs {
    /// Destination path for the exported artifact.
    pub out: PathBuf,

    /// Optional manifest sidecar describing the exported artifact.
    #[arg(long)]
    pub manifest: Option<PathBuf>,

    /// Emit machine-readable JSON output.
    #[arg(long)]
    pub json: bool,

    /// Path to the database file to export.
    pub db_path: PathBuf,
}

/// Per-verb argument set for `doctor trace`.
#[derive(Debug, Args)]
pub struct TraceArgs {
    /// Source reference to trace.
    #[arg(long = "source-ref")]
    pub source_ref: String,

    /// Emit machine-readable JSON output.
    #[arg(long)]
    pub json: bool,

    /// Path to the database file to inspect.
    pub db_path: PathBuf,
}

/// Outcome classes that map to the stable exit-code matrix in
/// `dev/interfaces/cli.md` § Exit-code classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliOutcome {
    /// Verb completed successfully with no findings.
    Clean,
    /// Doctor / verification surface found actionable non-clean state.
    Findings,
    /// Export / materialization failure on an artifact-producing doctor verb.
    ExportFailure,
    /// `recover` completed only because lossy action was explicitly accepted.
    RecoveryAcceptedLoss,
    /// Lock-held or equivalent precondition-blocked outcome.
    LockHeld,
    /// Unrecoverable command failure.
    Unrecoverable,
}

/// Map an outcome to the stable exit code defined in `cli.md`.
#[must_use]
pub fn outcome_to_exit_code(outcome: CliOutcome) -> i32 {
    match outcome {
        CliOutcome::Clean => exit_code::OK,
        CliOutcome::Findings => exit_code::DOCTOR_FOUND_ISSUES,
        CliOutcome::ExportFailure => exit_code::EXPORT_FAILURE,
        CliOutcome::RecoveryAcceptedLoss => exit_code::RECOVERY_ACCEPTED_LOSS,
        CliOutcome::LockHeld => exit_code::LOCK_HELD,
        CliOutcome::Unrecoverable => exit_code::UNRECOVERABLE,
    }
}

/// Map an [`EngineError`] to the [`CliOutcome`] class per
/// `dev/interfaces/cli.md` § Error to exit-code mapping.
#[must_use]
pub fn engine_error_to_outcome(err: &EngineError) -> CliOutcome {
    match err {
        EngineError::Closing => CliOutcome::LockHeld,
        _ => CliOutcome::Unrecoverable,
    }
}

/// Map an [`EngineOpenError`] to the [`CliOutcome`] class per
/// `dev/interfaces/cli.md` § Error to exit-code mapping.
#[must_use]
pub fn engine_open_error_to_outcome(err: &EngineOpenError) -> CliOutcome {
    match err {
        EngineOpenError::DatabaseLocked { .. } => CliOutcome::LockHeld,
        _ => CliOutcome::Unrecoverable,
    }
}

/// Run a parsed CLI command.
///
/// Phase 10a wires the six landed engine seams. The CLI opens the engine
/// at the verb's `<db_path>`, calls the seam, serializes the typed report
/// under a `verb`-discriminated JSON envelope, and maps `EngineError` to
/// the stable exit-code matrix.
///
/// `recover` invoked without `--accept-data-loss` is refused at the CLI
/// layer (no engine call) per `dev/design/recovery.md`: recovery is the
/// only lossy root and must not proceed without explicit acknowledgement.
#[must_use]
pub fn run(cli: Cli) -> i32 {
    match cli.command {
        Command::Recover(args) => run_recover(args),
        Command::Doctor(d) => run_doctor(d.command),
    }
}

fn run_recover(args: RecoverArgs) -> i32 {
    if !args.accept_data_loss {
        println!(
            r#"{{"status":"refused","verb":"recover","code":"E_RECOVER_REQUIRES_ACCEPT_DATA_LOSS"}}"#
        );
        return exit_code::UNRECOVERABLE;
    }

    // Pick the bound sub-action. Only one is wired in Phase 10a per
    // call; if multiple are passed, dispatch the first landed seam in
    // declaration order. Unsupported recover sub-flags (`--truncate-wal`,
    // `--purge-logical-id`, `--restore-logical-id`) fall through to a
    // not-implemented refusal until their engine seams land.
    if args.rebuild_projections {
        return wire_recover(&args.db_path, "rebuild-projections", |e| {
            e.rebuild_projections().map(|r| rebuild_report_json("rebuild-projections", &r))
        });
    }
    if args.rebuild_vec0 {
        return wire_recover(&args.db_path, "rebuild-vec0", |e| {
            e.rebuild_vec0().map(|r| rebuild_report_json("rebuild-vec0", &r))
        });
    }
    if let Some(source_id) = args.excise_source.as_deref() {
        return wire_recover(&args.db_path, "excise-source", |e| {
            e.excise_source(source_id).map(|r| excise_report_json(&r))
        });
    }

    // No bound sub-action selected → stub.
    println!(r#"{{"status":"not_implemented","verb":"recover"}}"#);
    exit_code::UNRECOVERABLE
}

fn run_doctor(cmd: DoctorCommand) -> i32 {
    match cmd {
        DoctorCommand::CheckIntegrity(args) => {
            let opts = CheckIntegrityOpts {
                quick: args.quick,
                full: args.full,
                round_trip: args.round_trip,
            };
            run_doctor_verb(&args.db_path, "check-integrity", |e| {
                e.check_integrity(opts).map(|r| integrity_report_outcome(&r))
            })
        }
        DoctorCommand::SafeExport(args) => {
            let manifest = args.manifest.clone().unwrap_or_else(|| {
                let mut p = args.out.clone();
                let name = p
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "export".to_string());
                p.set_file_name(format!("{name}.manifest.json"));
                p
            });
            run_doctor_verb(&args.db_path, "safe-export", |e| {
                e.safe_export(&args.out, &manifest)
                    .map(|r| (safe_export_json(&r), CliOutcome::Clean))
            })
        }
        DoctorCommand::Trace(args) => run_doctor_verb(&args.db_path, "trace", |e| {
            e.trace_source_ref(&args.source_ref).map(|r| (trace_report_json(&r), CliOutcome::Clean))
        }),
        DoctorCommand::VerifyEmbedder(args) => stub_doctor(&args.db_path, "verify-embedder"),
        DoctorCommand::DumpSchema(args) => stub_doctor(&args.db_path, "dump-schema"),
        DoctorCommand::DumpRowCounts(args) => stub_doctor(&args.db_path, "dump-row-counts"),
        DoctorCommand::DumpProfile(args) => stub_doctor(&args.db_path, "dump-profile"),
    }
}

/// Stub body for doctor verbs whose engine seam has not landed yet. Opens
/// the engine (so lock-held / open-error classes still surface) and then
/// emits a `not_implemented` JSON envelope. Returns `UNRECOVERABLE`
/// (matches the Phase 9 surface-stub posture).
fn stub_doctor(db_path: &std::path::Path, verb: &str) -> i32 {
    match Engine::open(db_path.to_path_buf()) {
        Ok(_opened) => {
            println!(r#"{{"status":"not_implemented","verb":"doctor:{verb}"}}"#);
            exit_code::UNRECOVERABLE
        }
        Err(err) => emit_engine_open_error(verb, &err),
    }
}

/// Open the engine, invoke `f`, print the resulting JSON value, and map
/// the outcome to an exit code. The closure returns `(json, outcome)`.
fn run_doctor_verb<F>(db_path: &std::path::Path, verb: &str, f: F) -> i32
where
    F: FnOnce(&Engine) -> Result<(Value, CliOutcome), EngineError>,
{
    let opened = match Engine::open(db_path.to_path_buf()) {
        Ok(o) => o,
        Err(err) => return emit_engine_open_error(verb, &err),
    };
    match f(&opened.engine) {
        Ok((value, outcome)) => {
            println!("{value}");
            outcome_to_exit_code(outcome)
        }
        Err(err) => emit_engine_error(verb, &err),
    }
}

/// Open the engine for `recover`, invoke `f`, print the JSON, and map the
/// outcome to `RECOVERY_ACCEPTED_LOSS` (64) on success.
fn wire_recover<F>(db_path: &std::path::Path, sub_verb: &str, f: F) -> i32
where
    F: FnOnce(&Engine) -> Result<Value, EngineError>,
{
    let opened = match Engine::open(db_path.to_path_buf()) {
        Ok(o) => o,
        Err(err) => return emit_engine_open_error(sub_verb, &err),
    };
    match f(&opened.engine) {
        Ok(value) => {
            println!("{value}");
            outcome_to_exit_code(CliOutcome::RecoveryAcceptedLoss)
        }
        Err(err) => emit_engine_error(sub_verb, &err),
    }
}

fn emit_engine_error(verb: &str, err: &EngineError) -> i32 {
    let outcome = engine_error_to_outcome(err);
    let payload = json!({
        "status": "error",
        "verb": verb,
        "code": engine_error_code(err),
        "detail": err.to_string(),
    });
    println!("{payload}");
    outcome_to_exit_code(outcome)
}

fn emit_engine_open_error(verb: &str, err: &EngineOpenError) -> i32 {
    let outcome = engine_open_error_to_outcome(err);
    let payload = json!({
        "status": "error",
        "verb": verb,
        "code": engine_open_error_code(err),
        "detail": err.to_string(),
    });
    println!("{payload}");
    outcome_to_exit_code(outcome)
}

fn engine_error_code(err: &EngineError) -> &'static str {
    match err {
        EngineError::Storage => "StorageError",
        EngineError::Projection => "ProjectionError",
        EngineError::Vector => "VectorError",
        EngineError::Embedder => "EmbedderError",
        EngineError::EmbedderNotConfigured => "EmbedderNotConfiguredError",
        EngineError::KindNotVectorIndexed => "KindNotVectorIndexedError",
        EngineError::EmbedderDimensionMismatch { .. } => "EmbedderDimensionMismatchError",
        EngineError::Scheduler => "SchedulerError",
        EngineError::OpStore => "OpStoreError",
        EngineError::WriteValidation => "WriteValidationError",
        EngineError::SchemaValidation => "SchemaValidationError",
        EngineError::Overloaded => "OverloadedError",
        EngineError::Closing => "ClosingError",
    }
}

fn engine_open_error_code(err: &EngineOpenError) -> &'static str {
    match err {
        EngineOpenError::DatabaseLocked { .. } => "DatabaseLockedError",
        EngineOpenError::Corruption(_) => "CorruptionError",
        EngineOpenError::IncompatibleSchemaVersion { .. } => "IncompatibleSchemaVersionError",
        EngineOpenError::MigrationError { .. } => "MigrationError",
        EngineOpenError::EmbedderIdentityMismatch { .. } => "EmbedderIdentityMismatchError",
        EngineOpenError::EmbedderDimensionMismatch { .. } => "EmbedderDimensionMismatchError",
        EngineOpenError::Io { .. } => "IoError",
    }
}

// ---- JSON serializers for engine report types ----

fn integrity_report_outcome(report: &IntegrityReport) -> (Value, CliOutcome) {
    let any_findings = matches!(report.physical, Section::Findings(_))
        || matches!(report.logical, Section::Findings(_))
        || matches!(report.semantic, Section::Findings(_));
    let body = json!({
        "verb": "check-integrity",
        "physical": section_json(&report.physical),
        "logical": section_json(&report.logical),
        "semantic": section_json(&report.semantic),
    });
    let outcome = if any_findings { CliOutcome::Findings } else { CliOutcome::Clean };
    (body, outcome)
}

fn section_json(section: &Section) -> Value {
    match section {
        Section::Clean => json!({ "status": "clean", "findings": [] }),
        Section::Findings(findings) => json!({
            "status": "findings",
            "findings": findings.iter().map(finding_json).collect::<Vec<_>>(),
        }),
    }
}

fn finding_json(f: &Finding) -> Value {
    json!({
        "code": f.code,
        "stage": f.stage,
        "locator": locator_json(&f.locator),
        "doc_anchor": f.doc_anchor,
        "detail": f.detail,
    })
}

fn locator_json(loc: &CorruptionLocator) -> Value {
    match loc {
        CorruptionLocator::FileOffset { offset } => {
            json!({ "kind": "file_offset", "offset": offset })
        }
        CorruptionLocator::PageId { page } => json!({ "kind": "page_id", "page": page }),
        CorruptionLocator::TableRow { table, rowid } => {
            json!({ "kind": "table_row", "table": table, "rowid": rowid })
        }
        CorruptionLocator::Vec0ShadowRow { partition, rowid } => {
            json!({ "kind": "vec0_shadow_row", "partition": partition, "rowid": rowid })
        }
        CorruptionLocator::MigrationStep { from, to } => {
            json!({ "kind": "migration_step", "from": from, "to": to })
        }
        CorruptionLocator::OpaqueSqliteError { sqlite_extended_code } => {
            json!({
                "kind": "opaque_sqlite_error",
                "sqlite_extended_code": sqlite_extended_code,
            })
        }
    }
}

fn safe_export_json(a: &SafeExportArtifact) -> Value {
    json!({
        "verb": "safe-export",
        "export_path": a.export_path.to_string_lossy(),
        "manifest_path": a.manifest_path.to_string_lossy(),
        "manifest_sha256": a.manifest_sha256,
    })
}

fn trace_report_json(t: &TraceReport) -> Value {
    json!({
        "verb": "trace",
        "source_ref": t.source_ref,
        "events": t.events.iter().map(|e| json!({
            "write_cursor": e.write_cursor,
            "kind": e.kind,
            "table": e.table,
        })).collect::<Vec<_>>(),
    })
}

fn rebuild_report_json(verb: &'static str, r: &RebuildReport) -> Value {
    let kind = match r.kind {
        RebuildKind::Projections => "projections",
        RebuildKind::Vec0 => "vec0",
    };
    json!({
        "verb": verb,
        "kind": kind,
        "rows_invalidated": r.rows_invalidated,
        "rows_rebuilt": r.rows_rebuilt,
        "projection_cursor_after": r.projection_cursor_after,
    })
}

fn excise_report_json(r: &ExciseReport) -> Value {
    json!({
        "verb": "excise-source",
        "source_ref": r.source_ref,
        "nodes_excised": r.nodes_excised,
        "edges_excised": r.edges_excised,
        "projections_invalidated": r.projections_invalidated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_mapping_covers_cli_md_exit_classes() {
        assert_eq!(outcome_to_exit_code(CliOutcome::Clean), 0);
        assert_eq!(outcome_to_exit_code(CliOutcome::RecoveryAcceptedLoss), 64);
        assert_eq!(outcome_to_exit_code(CliOutcome::Findings), 65);
        assert_eq!(outcome_to_exit_code(CliOutcome::ExportFailure), 66);
        assert_eq!(outcome_to_exit_code(CliOutcome::Unrecoverable), 70);
        assert_eq!(outcome_to_exit_code(CliOutcome::LockHeld), 71);
    }

    #[test]
    fn engine_error_storage_maps_to_unrecoverable() {
        assert_eq!(engine_error_to_outcome(&EngineError::Storage), CliOutcome::Unrecoverable);
        assert_eq!(engine_error_to_outcome(&EngineError::Closing), CliOutcome::LockHeld);
    }

    #[test]
    fn engine_open_database_locked_maps_to_lock_held() {
        let err = EngineOpenError::DatabaseLocked { holder_pid: Some(1234) };
        assert_eq!(engine_open_error_to_outcome(&err), CliOutcome::LockHeld);
    }
}
