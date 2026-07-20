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
    CheckIntegrityOpts, CorruptionLocator, DumpProfileReport, DumpRowCountsReport,
    DumpSchemaReport, Engine, EngineError, EngineOpenError, ExciseRecordReport, ExciseReport,
    Finding, IntegrityReport, MeanRecomputeReport, OrphanProvenanceReport, RebuildKind,
    RebuildReport, SafeExportArtifact, SchemaObject, Section, TraceReport, TruncateWalReport,
    TruncateWalStatus, VerifyEmbedderReport, VerifyEmbedderStatus,
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

    /// 0.8.20 Slice 5b (R-20-E7) — op-store collection holding the record to
    /// excise. Paired with `--excise-record-key`; both are required together.
    #[arg(long, requires = "excise_record_key")]
    pub excise_collection: Option<String>,

    /// 0.8.20 Slice 5b (R-20-E7) — record key to excise from
    /// `--excise-collection`. Erases every append-only-log version of the key
    /// plus its latest-state row.
    #[arg(long, requires = "excise_collection")]
    pub excise_record_key: Option<String>,

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
    VerifyEmbedder(VerifyEmbedderArgs),
    /// Trace the resolution chain for a given source reference.
    Trace(TraceArgs),
    /// Dump the canonical schema definition.
    DumpSchema(SimpleDoctorArgs),
    /// Dump per-table row counts.
    DumpRowCounts(SimpleDoctorArgs),
    /// Dump the response-cycle profile recorded by the engine.
    DumpProfile(SimpleDoctorArgs),
    /// EU-5b — fetch and verify the pinned default embedder weights so the
    /// next `Engine::open` with `EmbedderChoice::Default` runs against a
    /// warm cache without touching the network.
    WarmCache(WarmCacheArgs),
    /// 0.7.2 PR-2b — re-derive and re-pin the corpus mean from the current
    /// vectors, re-quantizing every row in one transaction. Always allowed
    /// (exempt from the automatic-path 200k cap).
    RecomputeMean(SimpleDoctorArgs),
    /// Slice 34 (F4-READ / reserved-gap-34) — read back op-store
    /// (`operational_mutations`) rows for one `append_only_log` collection
    /// over the existing `Engine::read_mutations` seam. A read-only operator
    /// diagnostic over the mutation log (the `dump-*` family, per
    /// `ADR-0.6.0-cli-scope`), NOT the rejected `search`/`get`/`list`
    /// application query surface. CLI-only; no SDK parity.
    DumpMutations(DumpMutationsArgs),
    /// 0.8.20 Slice 5d (R-20-E8) — read-only per-`source_id` census over the
    /// canonical tables. Reports which provenance buckets exist and, load-
    /// bearingly, how many rows are reachable by NO erasure verb (neither a
    /// `source_id` for `erase_source` nor a `logical_id` for `purge`). A
    /// non-zero un-erasable count exits `DOCTOR_FOUND_ISSUES` (65).
    /// CLI-only; no SDK parity.
    OrphanProvenance(SimpleDoctorArgs),
}

/// EU-5b — `fathomdb doctor warm-cache` argument set.
#[derive(Debug, Args)]
pub struct WarmCacheArgs {
    /// Emit machine-readable JSON output.
    #[arg(long)]
    pub json: bool,
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

/// Default `--limit` page size for `doctor dump-mutations` when the operator
/// omits it. A sane page that bounds output; the engine still clamps the
/// effective SQL `LIMIT` to `READ_COLLECTION_MAX_LIMIT` (~1M), so no read is
/// ever unbounded. See `dev/design/slice-34-cli-op-store-readback-design.md`.
const DUMP_MUTATIONS_DEFAULT_LIMIT: usize = 1000;

/// CLI-side mirror of the engine's `read_collection`/`read_mutations` page cap
/// (`READ_COLLECTION_MAX_LIMIT`, ~1M — private to `fathomdb-engine`). The CLI
/// clamps `--limit` to the SAME value via [`effective_dump_limit`] so the
/// `next_after_id` "full page ⇒ maybe more" decision compares `rows.len()`
/// against the EFFECTIVE limit the engine actually honors. Without this mirror,
/// a `--limit` above the engine cap would make a full capped page
/// (`rows.len() == cap < requested`) look exhausted → `next_after_id: null` →
/// pagination would silently stop while rows remain. Keep in lockstep with
/// `fathomdb-engine`'s `READ_COLLECTION_MAX_LIMIT`.
const DUMP_MUTATIONS_MAX_LIMIT: usize = 1_000_000;

/// Resolve the effective `doctor dump-mutations` page limit: the operator's
/// `--limit` (or the default when omitted), clamped to the engine page cap
/// [`DUMP_MUTATIONS_MAX_LIMIT`]. Pure + total so the clamp is unit-pinned
/// without seeding a >1M-row log (`tests/parser.rs`).
#[must_use]
pub fn effective_dump_limit(requested: Option<usize>) -> usize {
    requested.unwrap_or(DUMP_MUTATIONS_DEFAULT_LIMIT).min(DUMP_MUTATIONS_MAX_LIMIT)
}

/// Slice 34 — argument set for `doctor dump-mutations <collection>
/// [--after-id <n>] [--limit <n>] [--json] <db_path>`. A read-only operator
/// diagnostic that pages the op-store mutation log over the existing
/// `Engine::read_mutations` seam.
#[derive(Debug, Args)]
pub struct DumpMutationsArgs {
    /// The `append_only_log` collection whose appended rows to read back.
    pub collection: String,

    /// Exclusive cursor: return only rows with `id` strictly greater than this
    /// value. A negative value is normalized to the start of the log; a value
    /// past the last id yields an empty page.
    #[arg(long = "after-id")]
    pub after_id: Option<i64>,

    /// Maximum rows in this page (default 1000). The engine clamps the
    /// effective SQL `LIMIT` to the ~1M cap, so the read is never unbounded.
    #[arg(long)]
    pub limit: Option<usize>,

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

/// Per-verb argument set for `doctor verify-embedder`. `cli.md`
/// (amended 2026-05-15) locks the invocation as
/// `verify-embedder --identity <s> --dimension <n> <db_path>`.
#[derive(Debug, Args)]
pub struct VerifyEmbedderArgs {
    /// Stored-embedder identity string the operator expects (typically
    /// `<name>:<revision>`).
    #[arg(long)]
    pub identity: String,

    /// Stored-embedder dimension the operator expects.
    #[arg(long)]
    pub dimension: u32,

    /// Emit machine-readable JSON output.
    #[arg(long)]
    pub json: bool,

    /// Path to the database file to inspect.
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
        // 0.8.20 Slice 5b (R-20-E5) — `LOCK_HELD` (71), NOT `UNRECOVERABLE` (70).
        //
        // `LOCK_HELD` is documented as "lock-held or equivalent
        // precondition-blocked outcome", and that is exactly this case: the
        // erasure verb's row deletions committed, and the only step that failed
        // — `wal_checkpoint(TRUNCATE)` — failed because a CONCURRENT READER is
        // pinning a WAL snapshot. It is precondition-blocked and retryable, so
        // `UNRECOVERABLE` would tell an operator's script the opposite of the
        // truth and invite a destructive escalation instead of a retry.
        //
        // Non-zero either way, so the "an erasure verb must never report success
        // on an incomplete erasure" contract holds under both mappings; 71 is
        // chosen for the ACTIONABILITY of the signal. Caveat (accepted): the
        // rarer `stage = "telemetry_redaction"` failure is a sink I/O error
        // rather than a lock, and it also lands on 71; the JSON envelope carries
        // `code` + `detail` naming the real stage, so the precise cause is not
        // lost.
        EngineError::ErasureIncomplete { .. } => CliOutcome::LockHeld,
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
    // 0.8.20 Slice 5b (R-20-E7) — op-store record erasure. Clap's `requires`
    // pairing guarantees both flags arrive together.
    if let (Some(collection), Some(record_key)) =
        (args.excise_collection.as_deref(), args.excise_record_key.as_deref())
    {
        return wire_recover(&args.db_path, "excise-record", |e| {
            e.excise_collection_record(collection, record_key)
                .map(|r| excise_record_report_json(&r))
        });
    }
    if args.truncate_wal {
        return wire_recover(&args.db_path, "truncate-wal", |e| {
            e.truncate_wal().map(|r| truncate_wal_report_json(&r))
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
            run_doctor_verb_with_error_outcome(
                &args.db_path,
                "safe-export",
                CliOutcome::ExportFailure,
                |e| {
                    e.safe_export(&args.out, &manifest)
                        .map(|r| (safe_export_json(&r), CliOutcome::Clean))
                },
            )
        }
        DoctorCommand::Trace(args) => run_doctor_verb(&args.db_path, "trace", |e| {
            e.trace_source_ref(&args.source_ref).map(|r| (trace_report_json(&r), CliOutcome::Clean))
        }),
        DoctorCommand::VerifyEmbedder(args) => {
            let identity = args.identity.clone();
            let dimension = args.dimension;
            run_doctor_verb(&args.db_path, "verify-embedder", |e| {
                e.verify_embedder(&identity, dimension)
                    .map(|r| (verify_embedder_report_json(&r), CliOutcome::Clean))
            })
        }
        DoctorCommand::DumpSchema(args) => run_doctor_verb(&args.db_path, "dump-schema", |e| {
            e.dump_schema().map(|r| (dump_schema_report_json(&r), CliOutcome::Clean))
        }),
        DoctorCommand::DumpRowCounts(args) => {
            run_doctor_verb(&args.db_path, "dump-row-counts", |e| {
                e.dump_row_counts().map(|r| (dump_row_counts_report_json(&r), CliOutcome::Clean))
            })
        }
        DoctorCommand::DumpProfile(args) => run_doctor_verb(&args.db_path, "dump-profile", |e| {
            e.dump_profile().map(|r| (dump_profile_report_json(&r), CliOutcome::Clean))
        }),
        DoctorCommand::OrphanProvenance(args) => {
            run_doctor_verb(&args.db_path, "orphan-provenance", |e| {
                e.orphan_provenance().map(|r| {
                    // An un-erasable row is actionable non-clean state: the
                    // database holds content no erasure request can reach.
                    // Everything else (including `_legacy:` rows, which ARE
                    // erasable through the operator seam) is merely reported.
                    let outcome = if r.unerasable_rows > 0 {
                        CliOutcome::Findings
                    } else {
                        CliOutcome::Clean
                    };
                    (orphan_provenance_report_json(&r), outcome)
                })
            })
        }
        DoctorCommand::WarmCache(args) => run_doctor_warm_cache(args),
        DoctorCommand::RecomputeMean(args) => {
            run_doctor_verb(&args.db_path, "recompute-mean", |e| {
                e.recompute_mean().map(|r| (recompute_mean_report_json(&r), CliOutcome::Clean))
            })
        }
        DoctorCommand::DumpMutations(args) => {
            let limit = effective_dump_limit(args.limit);
            run_doctor_verb(&args.db_path, "dump-mutations", |e| {
                // Read over the EXISTING Slice-30 seam (Slice-33 index-driven).
                // The rows are serialized INLINE below so `OpStoreRow` is never
                // named / re-exported — the facade public-type set is untouched.
                e.read_mutations(&args.collection, args.after_id, limit).map(|rows| {
                    let row_values = rows
                        .iter()
                        .map(|r| {
                            json!({
                                "id": r.id,
                                "collection": r.collection,
                                "record_key": r.record_key,
                                "op_kind": r.op_kind,
                                "payload": r.payload,
                                "schema_id": r.schema_id,
                                "write_cursor": r.write_cursor,
                            })
                        })
                        .collect::<Vec<_>>();
                    // `next_after_id` = the last row's id iff a full page was
                    // returned (more rows may follow); else null (the log is
                    // exhausted at this cursor). The engine cursor is exclusive,
                    // so resuming with `--after-id <next_after_id>` never overlaps.
                    let next_after_id =
                        if rows.len() == limit { rows.last().map(|r| r.id) } else { None };
                    let body = json!({
                        "verb": "dump-mutations",
                        "collection": args.collection,
                        "after_id": args.after_id,
                        "limit": limit,
                        "count": row_values.len(),
                        "rows": row_values,
                        "next_after_id": next_after_id,
                    });
                    (body, CliOutcome::Clean)
                })
            })
        }
    }
}

/// EU-5b — invoke the default-embedder loader directly (no engine open)
/// so users + CI can warm the on-disk cache before the first
/// `Engine::open` triggers a download.
fn run_doctor_warm_cache(args: WarmCacheArgs) -> i32 {
    #[cfg(feature = "default-embedder")]
    {
        match fathomdb_embedder::loader::load_pinned_default_embedder() {
            Ok(weights) => {
                if args.json {
                    let payload = json!({
                        "verb": "warm-cache",
                        "status": "ok",
                        "config_json": weights.config_json_path.to_string_lossy(),
                        "tokenizer_json": weights.tokenizer_json_path.to_string_lossy(),
                        "model_safetensors": weights.model_safetensors_path.to_string_lossy(),
                        "bytes_downloaded": weights.bytes_downloaded,
                        "events": weights
                            .events
                            .iter()
                            .map(warm_cache_event_json)
                            .collect::<Vec<_>>(),
                    });
                    println!("{payload}");
                } else {
                    let kind = if weights.bytes_downloaded > 0 { "cold" } else { "warm" };
                    println!("warm-cache: ok ({kind})");
                    println!("  config.json:       {}", weights.config_json_path.display());
                    println!("  tokenizer.json:    {}", weights.tokenizer_json_path.display());
                    println!("  model.safetensors: {}", weights.model_safetensors_path.display());
                    println!("  bytes downloaded:  {}", weights.bytes_downloaded);
                    println!("  events:            {}", weights.events.len());
                }
                exit_code::OK
            }
            Err(err) => {
                if args.json {
                    let payload = json!({
                        "verb": "warm-cache",
                        "status": "error",
                        "code": "EmbedderLoadError",
                        "detail": err.to_string(),
                    });
                    println!("{payload}");
                } else {
                    eprintln!("warm-cache: error: {err}");
                }
                exit_code::UNRECOVERABLE
            }
        }
    }
    #[cfg(not(feature = "default-embedder"))]
    {
        let detail = "fathomdb CLI was built without the `default-embedder` feature; rebuild with --features default-embedder";
        if args.json {
            let payload = json!({
                "verb": "warm-cache",
                "status": "error",
                "code": "DefaultEmbedderFeatureDisabled",
                "detail": detail,
            });
            println!("{payload}");
        } else {
            eprintln!("warm-cache: error: {detail}");
        }
        exit_code::UNRECOVERABLE
    }
}

#[cfg(feature = "default-embedder")]
fn warm_cache_event_json(ev: &fathomdb_embedder::EmbedderEvent) -> Value {
    use fathomdb_embedder::EmbedderEvent;
    match ev {
        EmbedderEvent::DefaultEmbedderDownload {
            file,
            url,
            bytes,
            sha256,
            cache_path,
            duration_ms,
        } => json!({
            "kind": "download",
            "file": file,
            "url": url,
            "bytes": bytes,
            "sha256": sha256,
            "cache_path": cache_path.to_string_lossy(),
            "duration_ms": duration_ms,
        }),
        EmbedderEvent::DefaultEmbedderCacheHit { file, sha256, cache_path } => json!({
            "kind": "cache_hit",
            "file": file,
            "sha256": sha256,
            "cache_path": cache_path.to_string_lossy(),
        }),
        EmbedderEvent::MeanVecPinned { dim, doc_count } => json!({
            "kind": "mean_vec_pinned",
            "dim": dim,
            "doc_count": doc_count,
        }),
        EmbedderEvent::MeanVecRecomputed { dim, doc_count, trigger } => json!({
            "kind": "mean_vec_recomputed",
            "dim": dim,
            "doc_count": doc_count,
            "trigger": trigger.as_str(),
        }),
    }
}

/// Open the engine, invoke `f`, print the resulting JSON value, and map
/// the outcome to an exit code. The closure returns `(json, outcome)`.
fn run_doctor_verb<F>(db_path: &std::path::Path, verb: &str, f: F) -> i32
where
    F: FnOnce(&Engine) -> Result<(Value, CliOutcome), EngineError>,
{
    run_doctor_verb_inner(db_path, verb, None, f)
}

/// Variant that overrides the `EngineError` → outcome mapping for verbs
/// with a dedicated failure class (per `cli.md § Error → exit-code
/// mapping`). Example: `doctor safe-export` maps engine errors to
/// `ExportFailure` (66), not the default `Unrecoverable` (70).
fn run_doctor_verb_with_error_outcome<F>(
    db_path: &std::path::Path,
    verb: &str,
    error_outcome: CliOutcome,
    f: F,
) -> i32
where
    F: FnOnce(&Engine) -> Result<(Value, CliOutcome), EngineError>,
{
    run_doctor_verb_inner(db_path, verb, Some(error_outcome), f)
}

fn run_doctor_verb_inner<F>(
    db_path: &std::path::Path,
    verb: &str,
    error_outcome: Option<CliOutcome>,
    f: F,
) -> i32
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
        Err(err) => match error_outcome {
            Some(outcome) => emit_engine_error_with_outcome(verb, &err, outcome),
            None => emit_engine_error(verb, &err),
        },
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
    emit_engine_error_with_outcome(verb, err, engine_error_to_outcome(err))
}

fn emit_engine_error_with_outcome(verb: &str, err: &EngineError, outcome: CliOutcome) -> i32 {
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
        EngineError::Extractor => "ExtractorError",
        EngineError::Consolidator => "ConsolidatorError",
        // G4 (Slice 35) — filter predicate construction error.
        EngineError::InvalidFilter { .. } => "InvalidFilterError",
        EngineError::InvalidArgument { .. } => "InvalidArgumentError",
        // 0.8.18 Slice 5 (#5 vector-equivalence probe) — query-time dense refusal.
        EngineError::VectorEquivalenceMismatch { .. } => "VectorEquivalenceMismatchError",
        // OPP-12 Phase-1 (0.8.19 Slice 10) — lifecycle-verb typed errors.
        EngineError::IllegalTransition { .. } => "IllegalTransitionError",
        EngineError::NotLifecycleAddressable { .. } => "NotLifecycleAddressableError",
        // 0.8.20 Slice 5b (R-20-E5) — erasure verb could not finish at rest.
        EngineError::ErasureIncomplete { .. } => "ErasureIncompleteError",
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
        EngineOpenError::Embedder(_) => "EmbedderError",
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

/// 0.8.20 Slice 5b (R-20-E7). Emits the audit DIGEST, never the erased
/// `record_key` — a record key is arbitrary caller-supplied text and may itself
/// be the identifier being erased, so echoing it into CLI output (and thence
/// into an operator's shell history or log pipeline) would defeat the erasure.
fn excise_record_report_json(r: &ExciseRecordReport) -> Value {
    json!({
        "verb": "excise-record",
        "collection": r.collection,
        "record_digest": r.record_digest,
        "records_excised": r.records_excised,
        "state_rows_excised": r.state_rows_excised,
    })
}

fn verify_embedder_report_json(r: &VerifyEmbedderReport) -> Value {
    let status = match r.status {
        VerifyEmbedderStatus::Match => "match",
        VerifyEmbedderStatus::IdentityMismatch => "identity_mismatch",
        VerifyEmbedderStatus::DimensionMismatch => "dimension_mismatch",
        VerifyEmbedderStatus::BothMismatch => "both_mismatch",
    };
    json!({
        "verb": "verify-embedder",
        "stored_identity": r.stored_identity,
        "stored_dimension": r.stored_dimension,
        "supplied_identity": r.supplied_identity,
        "supplied_dimension": r.supplied_dimension,
        "status": status,
    })
}

fn schema_object_json(o: &SchemaObject) -> Value {
    json!({ "name": o.name, "sql": o.sql })
}

fn dump_schema_report_json(r: &DumpSchemaReport) -> Value {
    json!({
        "verb": "dump-schema",
        "user_version": r.user_version,
        "tables": r.tables.iter().map(schema_object_json).collect::<Vec<_>>(),
        "indexes": r.indexes.iter().map(schema_object_json).collect::<Vec<_>>(),
    })
}

/// 0.7.2 PR-2b — `doctor recompute-mean` `--json` normative contract.
fn recompute_mean_report_json(r: &MeanRecomputeReport) -> Value {
    json!({
        "verb": "recompute-mean",
        "status": "ok",
        "dim": r.dim,
        "old_doc_count": r.old_doc_count,
        "doc_count_requantized": r.doc_count_requantized,
        "drift_cos_before": r.drift_cos_before,
        "mean_was_pinned": r.mean_was_pinned,
        "elapsed_ms": r.elapsed_ms,
    })
}

fn dump_row_counts_report_json(r: &DumpRowCountsReport) -> Value {
    json!({
        "verb": "dump-row-counts",
        "counts": r.counts.iter().map(|c| json!({
            "name": c.name,
            "rows": c.rows,
        })).collect::<Vec<_>>(),
    })
}

fn orphan_provenance_report_json(r: &OrphanProvenanceReport) -> Value {
    json!({
        "verb": "orphan-provenance",
        "sources": r.sources.iter().map(|s| json!({
            "source_id": s.source_id,
            "rows": s.rows,
            "governed_rows": s.governed_rows,
            "reserved": s.reserved,
        })).collect::<Vec<_>>(),
        "total_rows": r.total_rows,
        "unerasable_rows": r.unerasable_rows,
    })
}

fn dump_profile_report_json(r: &DumpProfileReport) -> Value {
    json!({
        "verb": "dump-profile",
        "embedder_identity": r.embedder_identity,
        "embedder_dimension": r.embedder_dimension,
        "vectorized_kinds": r.vectorized_kinds,
    })
}

fn truncate_wal_report_json(r: &TruncateWalReport) -> Value {
    let status = match r.status {
        TruncateWalStatus::Done => "done",
        TruncateWalStatus::Busy => "busy",
    };
    json!({
        "verb": "truncate-wal",
        "status": status,
        "busy": r.busy,
        "log_frames": r.log_frames,
        "checkpointed_frames": r.checkpointed_frames,
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
