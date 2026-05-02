//! Operator CLI parser for `fathomdb`.
//!
//! Surface owned by `dev/interfaces/cli.md`. The 0.6.0 surface-stubs slice
//! pins the verb table, flag spelling, and exit-code class set; verb bodies
//! intentionally do not touch any database state.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

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
}

/// Doctor verb table per `dev/interfaces/cli.md` § Doctor verbs.
#[derive(Debug, Subcommand)]
pub enum DoctorCommand {
    /// Run a structural integrity check against the database.
    CheckIntegrity(CheckIntegrityArgs),
    /// Materialize a safe export of the database.
    SafeExport(SafeExportArgs),
    /// Verify the embedder identity recorded in the database.
    VerifyEmbedder,
    /// Trace the resolution chain for a given source reference.
    Trace(TraceArgs),
    /// Dump the canonical schema definition.
    DumpSchema,
    /// Dump per-table row counts.
    DumpRowCounts,
    /// Dump the response-cycle profile recorded by the engine.
    DumpProfile,
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
}

/// Run a parsed CLI command.
///
/// 0.6.0 verb bodies intentionally do not touch a database; each prints the
/// `{"status":"not_implemented","verb":"<name>"}` JSON shape and returns the
/// `UNRECOVERABLE` exit code so parser-shape tests can pin the surface
/// without asserting unimplemented happy-path semantics.
#[must_use]
pub fn run(cli: Cli) -> i32 {
    let verb = match &cli.command {
        Command::Recover(_) => "recover",
        Command::Doctor(d) => match &d.command {
            DoctorCommand::CheckIntegrity(_) => "doctor:check-integrity",
            DoctorCommand::SafeExport(_) => "doctor:safe-export",
            DoctorCommand::VerifyEmbedder => "doctor:verify-embedder",
            DoctorCommand::Trace(_) => "doctor:trace",
            DoctorCommand::DumpSchema => "doctor:dump-schema",
            DoctorCommand::DumpRowCounts => "doctor:dump-row-counts",
            DoctorCommand::DumpProfile => "doctor:dump-profile",
        },
    };
    println!(r#"{{"status":"not_implemented","verb":"{verb}"}}"#);
    exit_code::UNRECOVERABLE
}
