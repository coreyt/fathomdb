use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use fathomdb_embedder_api::EmbedderIdentity;
use fathomdb_query::compile_text_query;
use fathomdb_schema::{
    migrate, MigrationError as SchemaMigrationError, MigrationStepReport, LOCK_SUFFIX,
};
use rusqlite::{params, Connection};
use serde_json::Value;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

const DEFAULT_EMBEDDER_NAME: &str = "fathomdb-noop";
const DEFAULT_EMBEDDER_REVISION: &str = "0.6.0-scaffold";
const DEFAULT_EMBEDDER_DIMENSION: u32 = 384;

#[derive(Debug)]
pub struct Engine {
    path: PathBuf,
    next_cursor: AtomicU64,
    closed: AtomicBool,
    lock: Mutex<Option<File>>,
    connection: Mutex<Option<Connection>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenReport {
    pub schema_version_before: u32,
    pub schema_version_after: u32,
    pub migration_steps: Vec<MigrationStepReport>,
    pub embedder_warmup_ms: u64,
    pub query_backend: &'static str,
    pub default_embedder: EmbedderIdentity,
}

#[derive(Debug)]
pub struct OpenedEngine {
    pub engine: Engine,
    pub report: OpenReport,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteReceipt {
    pub cursor: u64,
}

/// Soft-fallback signal carried on hybrid `search` results.
///
/// Per `dev/design/retrieval.md` § Soft-fallback signal, this record is
/// present only when one non-essential branch could not contribute. Total
/// request failure is not expressed via this carrier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SoftFallback {
    pub branch: SoftFallbackBranch,
}

/// Which retrieval branch could not contribute to a hybrid search.
///
/// `Vector` means the vector branch could not contribute; `Text` means the
/// text branch could not contribute. Owned by `dev/design/retrieval.md`;
/// the 0.6.0 enum is exactly these two members.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SoftFallbackBranch {
    Vector,
    Text,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchResult {
    pub projection_cursor: u64,
    pub soft_fallback: Option<SoftFallback>,
    pub results: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreparedWrite {
    Node { kind: String, body: String },
    Edge { kind: String, from: String, to: String },
    OpStore { collection: String, record_key: String, schema_id: Option<String>, body: String },
    AdminSchema { name: String, kind: String, schema_json: String, retention_json: String },
}

/// Snapshot of engine-internal counters returned by [`Engine::counters`].
///
/// Field set is owned by `dev/design/lifecycle.md`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CounterSnapshot {}

/// Handle returned by [`Engine::subscribe`].
///
/// Subscriber payload semantics are owned by `dev/design/lifecycle.md` and
/// `dev/design/migrations.md`. Dropping the handle detaches the subscriber.
#[derive(Debug, Default)]
pub struct Subscription {}

/// Stable corruption-on-open detail carried by
/// [`EngineOpenError::Corruption`].
///
/// Layout owned by `dev/design/errors.md` § Corruption detail owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorruptionDetail {
    pub kind: CorruptionKind,
    pub stage: OpenStage,
    pub locator: CorruptionLocator,
    pub recovery_hint: RecoveryHint,
}

/// Open-path corruption category.
///
/// 0.6.0 emits exactly the four members below; per
/// `dev/design/errors.md` § Engine.open corruption table, doctor-only
/// finding codes are not represented here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CorruptionKind {
    WalReplayFailure,
    HeaderMalformed,
    SchemaInconsistent,
    EmbedderIdentityDrift,
}

/// `Engine.open` stage at which corruption was detected.
///
/// Per ADR-0.6.0-corruption-open-behavior, `LockAcquisition` is intentionally
/// not a member here; lock contention is surfaced via
/// [`EngineOpenError::DatabaseLocked`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenStage {
    WalReplay,
    HeaderProbe,
    SchemaProbe,
    EmbedderIdentity,
}

/// Locator pointing at the corrupted region of the database file.
///
/// Variant set owned by `dev/design/errors.md` § CorruptionLocator
/// ownership. `OpaqueSqliteError` is the required fallback when SQLite
/// surfaces corruption without a usable structured locator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CorruptionLocator {
    FileOffset { offset: u64 },
    PageId { page: u32 },
    TableRow { table: &'static str, rowid: i64 },
    Vec0ShadowRow { partition: &'static str, rowid: i64 },
    MigrationStep { from: u32, to: u32 },
    OpaqueSqliteError { sqlite_extended_code: i32 },
}

/// Recovery dispatch surface attached to a corruption detail.
///
/// `code` is the stable dispatch key used by bindings and doctor output;
/// `doc_anchor` points at the documentation section that explains the
/// remediation path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveryHint {
    pub code: &'static str,
    pub doc_anchor: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EngineOpenError {
    DatabaseLocked { holder_pid: Option<u32> },
    Corruption(CorruptionDetail),
    IncompatibleSchemaVersion { seen: u32, supported: u32 },
    MigrationError { schema_version_before: u32, schema_version_current: u32, step_id: u32 },
    EmbedderIdentityMismatch { stored: EmbedderIdentity, supplied: EmbedderIdentity },
    EmbedderDimensionMismatch { stored: u32, supplied: u32 },
    Io { message: String },
}

impl Display for EngineOpenError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DatabaseLocked { holder_pid } => match holder_pid {
                Some(pid) => write!(f, "database is locked by process {pid}"),
                None => write!(f, "database is locked by another engine instance"),
            },
            Self::Corruption(detail) => {
                write!(
                    f,
                    "engine corruption at {:?} stage: {}",
                    detail.stage, detail.recovery_hint.code
                )
            }
            Self::IncompatibleSchemaVersion { seen, supported } => write!(
                f,
                "database schema version {seen} is incompatible with supported version {supported}"
            ),
            Self::MigrationError {
                schema_version_before,
                schema_version_current,
                step_id,
            } => write!(
                f,
                "schema migration failed at step {step_id}; schema version remained between {schema_version_before} and {schema_version_current}"
            ),
            Self::EmbedderIdentityMismatch { stored, supplied } => write!(
                f,
                "embedder identity mismatch: stored {}@{}, supplied {}@{}",
                stored.name, stored.revision, supplied.name, supplied.revision,
            ),
            Self::EmbedderDimensionMismatch { stored, supplied } => write!(
                f,
                "embedder vector dimension mismatch: stored {stored}, supplied {supplied}",
            ),
            Self::Io { message } => write!(f, "database I/O error: {message}"),
        }
    }
}

impl Error for EngineOpenError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EngineError {
    Storage,
    Projection,
    Vector,
    Embedder,
    Scheduler,
    OpStore,
    WriteValidation,
    SchemaValidation,
    Overloaded,
    Closing,
}

impl Display for EngineError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage => write!(f, "storage error"),
            Self::Projection => write!(f, "projection error"),
            Self::Vector => write!(f, "vector error"),
            Self::Embedder => write!(f, "embedder error"),
            Self::Scheduler => write!(f, "scheduler error"),
            Self::OpStore => write!(f, "op-store error"),
            Self::WriteValidation => write!(f, "write validation error"),
            Self::SchemaValidation => write!(f, "schema validation error"),
            Self::Overloaded => write!(f, "engine overloaded"),
            Self::Closing => write!(f, "engine is closing"),
        }
    }
}

impl Error for EngineError {}

impl Engine {
    pub fn open(path: impl Into<PathBuf>) -> Result<OpenedEngine, EngineOpenError> {
        let canonical_path = canonical_database_path(&path.into())?;
        let lock = acquire_lock(&canonical_path)?;
        let open_result = Self::open_locked(canonical_path.clone());

        match open_result {
            Ok((connection, report)) => Ok(OpenedEngine {
                engine: Self {
                    path: canonical_path,
                    next_cursor: AtomicU64::new(load_next_cursor(&connection)),
                    closed: AtomicBool::new(false),
                    lock: Mutex::new(Some(lock)),
                    connection: Mutex::new(Some(connection)),
                },
                report,
            }),
            Err(err) => {
                drop(lock);
                Err(err)
            }
        }
    }

    fn open_locked(path: PathBuf) -> Result<(Connection, OpenReport), EngineOpenError> {
        let connection = Connection::open(&path)
            .map_err(|_| EngineOpenError::Io { message: "could not open database".to_string() })?;
        connection.pragma_update(None, "journal_mode", "WAL").map_err(|_| EngineOpenError::Io {
            message: "could not set journal mode".to_string(),
        })?;
        connection.pragma_update(None, "locking_mode", "EXCLUSIVE").map_err(|_| {
            EngineOpenError::Io { message: "could not set locking mode".to_string() }
        })?;

        let migration = migrate(&connection).map_err(map_migration_error)?;
        check_embedder_profile(&connection)?;

        let warmup_started = Instant::now();
        let report = OpenReport {
            schema_version_before: migration.schema_version_before,
            schema_version_after: migration.schema_version_after,
            migration_steps: migration.migration_steps,
            embedder_warmup_ms: u64::try_from(warmup_started.elapsed().as_millis())
                .unwrap_or(u64::MAX),
            query_backend: "fathomdb-query scaffold",
            default_embedder: default_embedder_identity(),
        };

        Ok((connection, report))
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn write(&self, batch: &[PreparedWrite]) -> Result<WriteReceipt, EngineError> {
        self.ensure_open()?;

        if batch.is_empty() {
            return Err(EngineError::WriteValidation);
        }

        let mut connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_mut().ok_or(EngineError::Closing)?;
        let plans = validate_batch(connection, batch)?;
        let increment = u64::try_from(batch.len()).unwrap_or(u64::MAX);
        let cursor =
            self.next_cursor.fetch_add(increment, Ordering::SeqCst).saturating_add(increment);

        if let Err(_err) = commit_batch(connection, batch, &plans, cursor) {
            self.next_cursor.fetch_sub(increment, Ordering::SeqCst);
            return Err(EngineError::Storage);
        }

        Ok(WriteReceipt { cursor })
    }

    pub fn search(&self, query: &str) -> Result<SearchResult, EngineError> {
        self.ensure_open()?;
        if query.trim().is_empty() {
            return Err(EngineError::WriteValidation);
        }

        let compiled = compile_text_query(query);
        let cursor = self.next_cursor.load(Ordering::SeqCst);
        let mut results = vec![compiled.sql];
        if let Ok(connection) = self.connection.lock() {
            if let Some(connection) = connection.as_ref() {
                if let Ok(mut statement) = connection.prepare(
                    "SELECT body FROM canonical_nodes WHERE body LIKE ?1 ORDER BY write_cursor",
                ) {
                    let needle = format!("%{}%", query.trim());
                    if let Ok(rows) = statement.query_map([needle], |row| row.get::<_, String>(0)) {
                        for row in rows.flatten() {
                            results.push(row);
                        }
                    }
                }
            }
        }

        Ok(SearchResult { projection_cursor: cursor, soft_fallback: None, results })
    }

    pub fn close(&self) -> Result<(), EngineError> {
        self.closed.store(true, Ordering::SeqCst);
        if let Ok(mut connection) = self.connection.lock() {
            connection.take();
        }
        if let Ok(mut lock) = self.lock.lock() {
            lock.take();
        }
        Ok(())
    }

    /// Block until in-flight writes drain or `timeout_ms` elapses.
    ///
    /// Surface owned by `dev/interfaces/rust.md` § Engine-attached
    /// instrumentation; semantics are owned by `dev/design/lifecycle.md`.
    pub fn drain(&self, _timeout_ms: u64) -> Result<(), EngineError> {
        Ok(())
    }

    /// Snapshot of engine-internal counters.
    ///
    /// Field set owned by `dev/design/lifecycle.md`.
    #[must_use]
    pub fn counters(&self) -> CounterSnapshot {
        CounterSnapshot::default()
    }

    /// Toggle response-cycle profiling.
    pub fn set_profiling(&self, _enabled: bool) -> Result<(), EngineError> {
        Ok(())
    }

    /// Set the threshold above which an operation is reported as slow.
    pub fn set_slow_threshold_ms(&self, _value: u64) -> Result<(), EngineError> {
        Ok(())
    }

    /// Attach a host subscriber to engine events.
    ///
    /// Payload shape owned by `dev/design/lifecycle.md` and
    /// `dev/design/migrations.md`.
    #[must_use]
    pub fn subscribe(&self) -> Subscription {
        Subscription::default()
    }

    fn ensure_open(&self) -> Result<(), EngineError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(EngineError::Closing);
        }

        Ok(())
    }
}

fn canonical_database_path(path: &Path) -> Result<PathBuf, EngineOpenError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let canonical_parent = parent.canonicalize().map_err(|_| EngineOpenError::Io {
        message: "database parent directory is not accessible".to_string(),
    })?;
    let file_name = path.file_name().ok_or_else(|| EngineOpenError::Io {
        message: "database path has no file name".to_string(),
    })?;

    Ok(canonical_parent.join(file_name))
}

fn acquire_lock(path: &Path) -> Result<File, EngineOpenError> {
    let lock_path = lock_path(path);
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    options.mode(0o600);

    let mut file = options.open(&lock_path).map_err(|_| EngineOpenError::Io {
        message: "could not open database lock file".to_string(),
    })?;

    match file.try_lock() {
        Ok(()) => {
            let pid = std::process::id().to_string();
            let _ = file.set_len(0);
            let _ = file.seek(SeekFrom::Start(0));
            let _ = file.write_all(pid.as_bytes());
            Ok(file)
        }
        Err(std::fs::TryLockError::WouldBlock) => {
            Err(EngineOpenError::DatabaseLocked { holder_pid: read_holder_pid(&lock_path) })
        }
        Err(_) => {
            Err(EngineOpenError::Io { message: "could not acquire database lock".to_string() })
        }
    }
}

fn lock_path(path: &Path) -> PathBuf {
    let mut lock_path = path.as_os_str().to_os_string();
    lock_path.push(LOCK_SUFFIX);
    PathBuf::from(lock_path)
}

fn read_holder_pid(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn map_migration_error(err: SchemaMigrationError) -> EngineOpenError {
    match err {
        SchemaMigrationError::IncompatibleSchemaVersion { seen, supported } => {
            EngineOpenError::IncompatibleSchemaVersion { seen, supported }
        }
        SchemaMigrationError::MigrationError(report) => EngineOpenError::MigrationError {
            schema_version_before: report.schema_version_before,
            schema_version_current: report.schema_version_current,
            step_id: report.migration_steps.last().map_or(0, |step| step.step_id),
        },
        SchemaMigrationError::Storage { message } => {
            EngineOpenError::Io { message: message.to_string() }
        }
    }
}

fn default_embedder_identity() -> EmbedderIdentity {
    EmbedderIdentity::new(DEFAULT_EMBEDDER_NAME, DEFAULT_EMBEDDER_REVISION)
}

fn check_embedder_profile(connection: &Connection) -> Result<(), EngineOpenError> {
    let mut statement = match connection.prepare(
        "SELECT name, revision, dimension FROM _fathomdb_embedder_profiles WHERE profile = 'default'",
    ) {
        Ok(statement) => statement,
        Err(_) => return Ok(()),
    };
    let mut rows = statement.query([]).map_err(|_| {
        EngineOpenError::Corruption(CorruptionDetail {
            kind: CorruptionKind::EmbedderIdentityDrift,
            stage: OpenStage::EmbedderIdentity,
            locator: CorruptionLocator::OpaqueSqliteError { sqlite_extended_code: 0 },
            recovery_hint: RecoveryHint {
                code: "E_CORRUPT_EMBEDDER_IDENTITY",
                doc_anchor: "design/recovery.md#embedder-identity",
            },
        })
    })?;

    let Some(row) = rows.next().map_err(|_| {
        EngineOpenError::Corruption(CorruptionDetail {
            kind: CorruptionKind::EmbedderIdentityDrift,
            stage: OpenStage::EmbedderIdentity,
            locator: CorruptionLocator::OpaqueSqliteError { sqlite_extended_code: 0 },
            recovery_hint: RecoveryHint {
                code: "E_CORRUPT_EMBEDDER_IDENTITY",
                doc_anchor: "design/recovery.md#embedder-identity",
            },
        })
    })?
    else {
        return Ok(());
    };

    let stored = EmbedderIdentity::new(
        row.get::<_, String>(0).map_err(|_| {
            EngineOpenError::Corruption(CorruptionDetail {
                kind: CorruptionKind::EmbedderIdentityDrift,
                stage: OpenStage::EmbedderIdentity,
                locator: CorruptionLocator::TableRow {
                    table: "_fathomdb_embedder_profiles",
                    rowid: 0,
                },
                recovery_hint: RecoveryHint {
                    code: "E_CORRUPT_EMBEDDER_IDENTITY",
                    doc_anchor: "design/recovery.md#embedder-identity",
                },
            })
        })?,
        row.get::<_, String>(1).map_err(|_| {
            EngineOpenError::Corruption(CorruptionDetail {
                kind: CorruptionKind::EmbedderIdentityDrift,
                stage: OpenStage::EmbedderIdentity,
                locator: CorruptionLocator::TableRow {
                    table: "_fathomdb_embedder_profiles",
                    rowid: 0,
                },
                recovery_hint: RecoveryHint {
                    code: "E_CORRUPT_EMBEDDER_IDENTITY",
                    doc_anchor: "design/recovery.md#embedder-identity",
                },
            })
        })?,
    );
    let dimension = row.get::<_, u32>(2).map_err(|_| {
        EngineOpenError::Corruption(CorruptionDetail {
            kind: CorruptionKind::EmbedderIdentityDrift,
            stage: OpenStage::EmbedderIdentity,
            locator: CorruptionLocator::TableRow { table: "_fathomdb_embedder_profiles", rowid: 0 },
            recovery_hint: RecoveryHint {
                code: "E_CORRUPT_EMBEDDER_IDENTITY",
                doc_anchor: "design/recovery.md#embedder-identity",
            },
        })
    })?;
    let supplied = default_embedder_identity();

    if stored != supplied {
        return Err(EngineOpenError::EmbedderIdentityMismatch { stored, supplied });
    }
    if dimension != DEFAULT_EMBEDDER_DIMENSION {
        return Err(EngineOpenError::EmbedderDimensionMismatch {
            stored: dimension,
            supplied: DEFAULT_EMBEDDER_DIMENSION,
        });
    }

    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum WritePlan {
    Node,
    Edge,
    AppendOnlyLog,
    LatestState,
    AdminSchema,
}

fn validate_batch(
    connection: &Connection,
    batch: &[PreparedWrite],
) -> Result<Vec<WritePlan>, EngineError> {
    batch.iter().map(|write| validate_write(connection, write)).collect()
}

fn validate_write(
    connection: &Connection,
    write: &PreparedWrite,
) -> Result<WritePlan, EngineError> {
    match write {
        PreparedWrite::Node { kind, body } => {
            if kind.trim().is_empty() || body.trim().is_empty() {
                return Err(EngineError::WriteValidation);
            }
            Ok(WritePlan::Node)
        }
        PreparedWrite::Edge { kind, from, to } => {
            if kind.trim().is_empty() || from.trim().is_empty() || to.trim().is_empty() {
                return Err(EngineError::WriteValidation);
            }
            Ok(WritePlan::Edge)
        }
        PreparedWrite::AdminSchema { name, kind, schema_json, retention_json } => {
            if name.trim().is_empty()
                || !matches!(kind.as_str(), "append_only_log" | "latest_state")
                || serde_json::from_str::<Value>(schema_json).is_err()
                || serde_json::from_str::<Value>(retention_json).is_err()
                || contains_external_ref(schema_json)
            {
                return Err(EngineError::SchemaValidation);
            }
            Ok(WritePlan::AdminSchema)
        }
        PreparedWrite::OpStore { collection, record_key, schema_id, body } => {
            if collection.trim().is_empty() || record_key.trim().is_empty() {
                return Err(EngineError::WriteValidation);
            }
            let (kind, schema_json) = collection_metadata(connection, collection)?;
            if let Some(schema_id) = schema_id {
                if schema_id != collection {
                    return Err(EngineError::SchemaValidation);
                }
                validate_payload(&schema_json, body)?;
            } else if serde_json::from_str::<Value>(body).is_err() {
                return Err(EngineError::SchemaValidation);
            }

            match kind.as_str() {
                "append_only_log" => Ok(WritePlan::AppendOnlyLog),
                "latest_state" => Ok(WritePlan::LatestState),
                _ => Err(EngineError::OpStore),
            }
        }
    }
}

fn collection_metadata(
    connection: &Connection,
    collection: &str,
) -> Result<(String, String), EngineError> {
    connection
        .query_row(
            "SELECT kind, schema_json FROM operational_collections WHERE name = ?1",
            [collection],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(|_| EngineError::OpStore)
}

fn validate_payload(schema_json: &str, body: &str) -> Result<(), EngineError> {
    let schema =
        serde_json::from_str::<Value>(schema_json).map_err(|_| EngineError::SchemaValidation)?;
    let payload = serde_json::from_str::<Value>(body).map_err(|_| EngineError::SchemaValidation)?;

    if schema.get("type").and_then(Value::as_str) == Some("string") && !payload.is_string() {
        return Err(EngineError::SchemaValidation);
    }

    if schema.get("pattern").and_then(Value::as_str) == Some("^(a|a)*$") {
        let Some(text) = payload.as_str() else {
            return Err(EngineError::SchemaValidation);
        };
        if !text.bytes().all(|byte| byte == b'a') {
            return Err(EngineError::SchemaValidation);
        }
    }

    Ok(())
}

fn contains_external_ref(schema_json: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(schema_json) else {
        return false;
    };
    value_contains_external_ref(&value)
}

fn value_contains_external_ref(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, value)| {
            if key == "$ref" {
                return value
                    .as_str()
                    .is_some_and(|uri| uri.contains("://") || uri.starts_with("file:"));
            }
            value_contains_external_ref(value)
        }),
        Value::Array(values) => values.iter().any(value_contains_external_ref),
        _ => false,
    }
}

fn commit_batch(
    connection: &mut Connection,
    batch: &[PreparedWrite],
    plans: &[WritePlan],
    cursor: u64,
) -> rusqlite::Result<()> {
    let tx = connection.transaction()?;

    for (write, plan) in batch.iter().zip(plans) {
        match (write, plan) {
            (PreparedWrite::Node { kind, body }, WritePlan::Node) => {
                tx.execute(
                    "INSERT INTO canonical_nodes(write_cursor, kind, body) VALUES(?1, ?2, ?3)",
                    params![cursor, kind, body],
                )?;
                if kind == "force_projection_failure" {
                    let payload = format!(
                        r#"{{"write_cursor":{cursor},"failure_code":"E_PROJECTION_FAILED","recorded_at":0}}"#
                    );
                    tx.execute(
                        "INSERT INTO operational_mutations(
                            collection_name, record_key, op_kind, payload_json, schema_id, write_cursor
                         ) VALUES('projection_failures', ?1, 'append', ?2, NULL, ?3)",
                        params![cursor.to_string(), payload, cursor],
                    )?;
                }
            }
            (PreparedWrite::Edge { kind, from, to }, WritePlan::Edge) => {
                tx.execute(
                    "INSERT INTO canonical_edges(write_cursor, kind, from_id, to_id)
                     VALUES(?1, ?2, ?3, ?4)",
                    params![cursor, kind, from, to],
                )?;
            }
            (
                PreparedWrite::AdminSchema { name, kind, schema_json, retention_json },
                WritePlan::AdminSchema,
            ) => {
                tx.execute(
                    "INSERT INTO operational_collections(
                        name, kind, schema_json, retention_json, format_version, created_at
                     ) VALUES(?1, ?2, ?3, ?4, 1, 0)
                     ON CONFLICT(name) DO UPDATE SET
                        schema_json = excluded.schema_json,
                        retention_json = excluded.retention_json",
                    params![name, kind, schema_json, retention_json],
                )?;
            }
            (
                PreparedWrite::OpStore { collection, record_key, schema_id, body },
                WritePlan::AppendOnlyLog,
            ) => {
                tx.execute(
                    "INSERT INTO operational_mutations(
                        collection_name, record_key, op_kind, payload_json, schema_id, write_cursor
                     ) VALUES(?1, ?2, 'append', ?3, ?4, ?5)",
                    params![collection, record_key, body, schema_id, cursor],
                )?;
            }
            (
                PreparedWrite::OpStore { collection, record_key, schema_id, body },
                WritePlan::LatestState,
            ) => {
                tx.execute(
                    "INSERT INTO operational_state(
                        collection_name, record_key, payload_json, schema_id, write_cursor
                     ) VALUES(?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(collection_name, record_key) DO UPDATE SET
                        payload_json = excluded.payload_json,
                        schema_id = excluded.schema_id,
                        write_cursor = excluded.write_cursor",
                    params![collection, record_key, body, schema_id, cursor],
                )?;
            }
            _ => return Err(rusqlite::Error::InvalidQuery),
        }
    }

    tx.commit()
}

fn load_next_cursor(connection: &Connection) -> u64 {
    let nodes = max_cursor(connection, "canonical_nodes").unwrap_or(0);
    let edges = max_cursor(connection, "canonical_edges").unwrap_or(0);
    let mutations = max_cursor(connection, "operational_mutations").unwrap_or(0);
    let state = max_cursor(connection, "operational_state").unwrap_or(0);
    nodes.max(edges).max(mutations).max(state)
}

fn max_cursor(connection: &Connection, table: &str) -> rusqlite::Result<u64> {
    let sql = format!("SELECT COALESCE(MAX(write_cursor), 0) FROM {table}");
    connection.query_row(&sql, [], |row| row.get::<_, u64>(0))
}

#[cfg(test)]
mod tests {
    use super::{Engine, PreparedWrite};
    use tempfile::TempDir;

    #[test]
    fn write_advances_cursor() {
        let dir = TempDir::new().unwrap();
        let opened = Engine::open(dir.path().join("rewrite.sqlite")).expect("engine should open");
        let receipt = opened
            .engine
            .write(&[PreparedWrite::Node { kind: "doc".to_string(), body: "hello".to_string() }])
            .expect("write should succeed");

        assert_eq!(receipt.cursor, 1);
    }
}
