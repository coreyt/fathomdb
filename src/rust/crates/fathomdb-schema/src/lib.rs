use std::fmt::{Display, Formatter};
use std::time::Instant;

use rusqlite::Connection;

pub const SCHEMA_VERSION: u32 = 12;

/// SQLite `PRAGMA` name carrying the on-disk schema-version sentinel.
///
/// Public on-disk surface per `dev/interfaces/wire.md` § Schema-version
/// sentinel; advanced by successful migrations per `dev/design/migrations.md`.
pub const PRAGMA_USER_VERSION: &str = "user_version";

/// Suffix of the canonical SQLite database file (`<db-name>.sqlite`).
pub const SQLITE_SUFFIX: &str = ".sqlite";

/// Suffix of the SQLite write-ahead log file (`<db-name>.sqlite-wal`).
pub const WAL_SUFFIX: &str = "-wal";

/// Suffix of the sidecar lock file (`<db-name>.sqlite.lock`).
///
/// Per `dev/design/bindings.md` § 7, this sidecar flock is the load-bearing
/// cross-process exclusion layer; it surfaces lock contention before SQLite
/// I/O begins.
pub const LOCK_SUFFIX: &str = ".lock";

/// Suffix of the optional SQLite rollback journal file
/// (`<db-name>.sqlite-journal`).
pub const JOURNAL_SUFFIX: &str = "-journal";

#[must_use]
pub fn bootstrap_steps() -> &'static [&'static str] {
    &["create canonical tables", "register projection metadata", "seed rewrite-era configuration"]
}

/// Canonical tables owned by the rewrite-era schema, in stable display
/// order. Excludes FTS, vec0, and projection shadow tables (re-derivable
/// from canonical state) and internal `_fathomdb_*` metadata.
///
/// `doctor dump-row-counts` enumerates this set; `doctor dump-schema`
/// uses it to order canonical tables ahead of derived/internal ones.
pub const CANONICAL_TABLES: &[&str] = &[
    "canonical_nodes",
    "canonical_edges",
    "operational_collections",
    "operational_mutations",
    "operational_state",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Migration {
    pub step_id: u32,
    pub sql: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationStepReport {
    pub step_id: u32,
    pub duration_ms: Option<u64>,
    pub failed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationReport {
    pub schema_version_before: u32,
    pub schema_version_after: u32,
    pub migration_steps: Vec<MigrationStepReport>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationFailureReport {
    pub schema_version_before: u32,
    pub schema_version_current: u32,
    pub migration_steps: Vec<MigrationStepReport>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MigrationError {
    IncompatibleSchemaVersion { seen: u32, supported: u32 },
    MigrationError(MigrationFailureReport),
    Storage { message: &'static str },
}

impl Display for MigrationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IncompatibleSchemaVersion { seen, supported } => {
                write!(f, "database schema version {seen} is incompatible with supported version {supported}")
            }
            Self::MigrationError(report) => write!(
                f,
                "schema migration failed at step {}",
                report.migration_steps.last().map_or(0, |step| step.step_id)
            ),
            Self::Storage { message } => write!(f, "schema storage error: {message}"),
        }
    }
}

impl std::error::Error for MigrationError {}

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        step_id: 1,
        sql: "CREATE TABLE IF NOT EXISTS _fathomdb_schema_meta(key TEXT PRIMARY KEY, value TEXT NOT NULL)",
    },
    Migration {
        step_id: 2,
        sql: "CREATE TABLE IF NOT EXISTS _fathomdb_migrations(step_id INTEGER PRIMARY KEY, applied_at_ms INTEGER NOT NULL);
              CREATE TABLE IF NOT EXISTS canonical_nodes(write_cursor INTEGER NOT NULL, kind TEXT NOT NULL, body TEXT NOT NULL);
              CREATE TABLE IF NOT EXISTS canonical_edges(write_cursor INTEGER NOT NULL, kind TEXT NOT NULL, from_id TEXT NOT NULL, to_id TEXT NOT NULL);",
    },
    Migration {
        step_id: 3,
        sql: "CREATE TABLE IF NOT EXISTS _fathomdb_embedder_profiles(profile TEXT PRIMARY KEY, name TEXT NOT NULL, revision TEXT NOT NULL, dimension INTEGER NOT NULL)",
    },
    Migration {
        step_id: 4,
        sql: "CREATE TABLE IF NOT EXISTS operational_collections(
                  name TEXT PRIMARY KEY,
                  kind TEXT NOT NULL CHECK(kind IN ('append_only_log', 'latest_state')),
                  schema_json TEXT NOT NULL,
                  retention_json TEXT NOT NULL,
                  format_version INTEGER NOT NULL,
                  created_at INTEGER NOT NULL
              );
              CREATE TABLE IF NOT EXISTS operational_mutations(
                  id INTEGER PRIMARY KEY AUTOINCREMENT,
                  collection_name TEXT NOT NULL,
                  record_key TEXT NOT NULL,
                  op_kind TEXT NOT NULL CHECK(op_kind = 'append'),
                  payload_json TEXT NOT NULL,
                  schema_id TEXT,
                  write_cursor INTEGER NOT NULL
              );
              CREATE TABLE IF NOT EXISTS operational_state(
                  collection_name TEXT NOT NULL,
                  record_key TEXT NOT NULL,
                  payload_json TEXT NOT NULL,
                  schema_id TEXT,
                  write_cursor INTEGER NOT NULL,
                  PRIMARY KEY(collection_name, record_key)
              );
              CREATE TABLE IF NOT EXISTS _fathomdb_open_state(key TEXT PRIMARY KEY, value TEXT NOT NULL);
              INSERT OR IGNORE INTO operational_collections(
                  name, kind, schema_json, retention_json, format_version, created_at
              ) VALUES (
                  'projection_failures',
                  'append_only_log',
                  '{\"type\":\"object\"}',
                  '{}',
                  1,
                  0
              );",
    },
    Migration {
        step_id: 5,
        sql: "CREATE VIRTUAL TABLE IF NOT EXISTS search_index USING fts5(
                  body,
                  kind UNINDEXED,
                  write_cursor UNINDEXED
              );",
    },
    Migration {
        step_id: 6,
        sql: "CREATE TABLE IF NOT EXISTS _fathomdb_projection_state(
                  kind TEXT PRIMARY KEY,
                  last_enqueued_cursor INTEGER NOT NULL DEFAULT 0,
                  updated_at INTEGER NOT NULL DEFAULT 0
              );
              CREATE TABLE IF NOT EXISTS _fathomdb_vector_kinds(
                  kind TEXT PRIMARY KEY,
                  profile TEXT NOT NULL,
                  created_at INTEGER NOT NULL DEFAULT 0
              );
              CREATE TABLE IF NOT EXISTS _fathomdb_vector_rows(
                  rowid INTEGER PRIMARY KEY,
                  kind TEXT NOT NULL,
                  write_cursor INTEGER NOT NULL UNIQUE
              );",
    },
    Migration {
        step_id: 7,
        sql: "CREATE TABLE IF NOT EXISTS _fathomdb_projection_terminal(
                  write_cursor INTEGER PRIMARY KEY,
                  state TEXT NOT NULL CHECK(state IN ('failed', 'up_to_date'))
              );",
    },
    // Phase 9 Pack B — REQ-026 / AC-028a/b/c / AC-042 recovery seam.
    // `source_id` is nullable; existing canonical rows back-fill to NULL,
    // so reads from older callers stay schema-stable. REQ-045 accretion
    // offset is documented in `migrations/008_source_id.sql` as inherently
    // impossible for this slice (every existing canonical column is
    // load-bearing for replay / projections / recovery locators); the
    // next schema-touching pack carries the offset budget for two adds.
    Migration {
        step_id: 8,
        sql: "ALTER TABLE canonical_nodes ADD COLUMN source_id TEXT;
              ALTER TABLE canonical_edges ADD COLUMN source_id TEXT;
              CREATE INDEX IF NOT EXISTS canonical_nodes_source_id_idx
                  ON canonical_nodes(source_id);
              CREATE INDEX IF NOT EXISTS canonical_edges_source_id_idx
                  ON canonical_edges(source_id);",
    },
    // 0.7.0 Pack 1 — Vector binary-quantization data encoding.
    // Per `dev/design/0.7.0-vector-quant-pack1.md` D4 (fix-3). Stages
    // the existing f32 corpus + kind mapping into a regular SQL table,
    // drops + recreates `vector_default` with the new schema (sibling
    // `embedding_bin bit[768]`, `source_type TEXT partition key`,
    // `kind TEXT`, `created_at INTEGER`), then repopulates with
    // SQL-side `vec_quantize_binary` and the D3 CASE mapping. A
    // prefix CHECK-constraint preflight aborts the migration if any
    // `_fathomdb_vector_rows.kind` is outside the locked vocabulary.
    //
    // `<dim>=768` is hardcoded against the default profile
    // (`load_default_profile` -> `DEFAULT_EMBEDDER_DIMENSION` in
    // fathomdb-engine). The design notes this constraint and defers
    // a runtime-dim migration to 0.7.1.
    Migration {
        step_id: 9,
        // SQL-side: D4 fix-3.1 preflight only. The vec0 reshape itself is
        // dim-aware and lives in the engine crate's
        // `ensure_vector_partition_pack1` (called by `ensure_vector_partition`
        // immediately after `migrate_with_event_sink` returns). Splitting the
        // preflight (SQL, in-tx with `apply_one`) from the reshape (Rust,
        // dim-driven by `_fathomdb_embedder_profiles.dimension`) is required
        // because `fathomdb-schema::Migration` is a `&'static str` with no
        // runtime parameterization, and the existing dim=8 / dim=384 test
        // suite must stay GREEN. The reshape is idempotent across crashes:
        // if open fails between this step's commit (user_version=9) and the
        // Rust reshape, the next open re-detects the old shape and replays
        // the reshape. See dev/plans/runs/0.7.0-PVQ-P1-IMPL-output.json
        // for the design-memo deviation note.
        sql: "CREATE TEMP TABLE _vec0_migration_assertion(
                  check_passes INTEGER NOT NULL CHECK(check_passes = 1)
              );
              INSERT INTO _vec0_migration_assertion(check_passes)
                  SELECT CASE WHEN EXISTS (
                      SELECT 1 FROM _fathomdb_vector_rows
                      WHERE kind NOT IN ('email','article','paper','meeting','note','todo','doc')
                  ) THEN 0 ELSE 1 END;
              DROP TABLE _vec0_migration_assertion;",
    },
    // 0.7.1 EU-5a2 — mean-centering schema column.
    // Per `dev/design/embedder.md` §0.2: nullable BLOB holding the
    // pinned per-workspace mean vector for the default profile. Byte
    // length, when non-NULL, MUST equal `4 * dimension` (f32 little-endian).
    // Pure additive ALTER; SQLite stores NULL for the pre-existing row.
    // Lifecycle (compute-once-on-first-ingest threshold-pin) is in the
    // engine crate, not the schema layer.
    Migration {
        step_id: 10,
        sql: "ALTER TABLE _fathomdb_embedder_profiles ADD COLUMN mean_vec BLOB",
    },
    // 0.8.0 Slice 5 (G1) — global FTS5 tokenizer-default upgrade.
    // Per `dev/plans/0.8.0-implementation.md` § "Slice 5" and the design
    // memo `dev/design/0.8.0-slice-5-G1-design.md`. Migrations are
    // forward-only and immutable, and FTS5 has no `ALTER … tokenize`, so the
    // tokenizer default is upgraded by dropping and recreating the
    // `search_index` virtual table rather than editing the step-5 DDL (which
    // would change the tokenizer for new DBs only). The drop+recreate leaves
    // the FTS index empty on a migrated DB; the engine re-tokenizes from the
    // canonical source rows immediately after this step lands (open path,
    // `reproject_search_index_after_tokenizer_upgrade`) — projection-only, no
    // source-record migration. `DROP TABLE` already satisfies the accretion
    // guard's `names_removal` branch; the exemption marker is carried to
    // document intent and match the established pattern.
    Migration {
        step_id: 11,
        sql: "-- MIGRATION-ACCRETION-EXEMPTION: tokenizer-default upgrade (drop+recreate FTS5 projection; no source-record migration)
              DROP TABLE IF EXISTS search_index;
              CREATE VIRTUAL TABLE search_index USING fts5(
                  body,
                  kind UNINDEXED,
                  write_cursor UNINDEXED,
                  tokenize = 'porter unicode61 remove_diacritics 2'
              );",
    },
    // 0.8.0 Slice 15 (G0 KEYSTONE) — transaction-time canonical-identity
    // substrate. Per `dev/adr/ADR-0.8.0-canonical-identity-substrate.md`
    // (SIGNED 2026-06-03) and `dev/design/slice-15-g0-design.md`. Two additive
    // nullable columns on BOTH canonical tables: `logical_id TEXT` (stable
    // cross-re-ingestion identity; NULL = legacy/own-identity row) and
    // `superseded_at INTEGER` (transaction-time tombstone; NULL = active row).
    // A partial UNIQUE INDEX `(logical_id, kind) WHERE superseded_at IS NULL`
    // per table enforces one active version per logical id — NULL-safe, so the
    // many legacy NULL-logical_id rows never collide (SQLite treats each NULL as
    // distinct; load-bearing). The folded G4/G5 read indexes
    // (`canonical_nodes(kind)`, `canonical_edges(from_id)/(to_id)`) ride this one
    // accretion offset budget. Pure additive ALTER (no DROP) → the exemption
    // marker is REQUIRED (the accretion guard rejects ADD COLUMN without it);
    // legacy rows read NULL with no data migration / re-open (in-place ALTER).
    Migration {
        step_id: 12,
        sql: "-- MIGRATION-ACCRETION-EXEMPTION: G0 transaction-time identity substrate
              ALTER TABLE canonical_nodes ADD COLUMN logical_id TEXT;
              ALTER TABLE canonical_nodes ADD COLUMN superseded_at INTEGER;
              ALTER TABLE canonical_edges ADD COLUMN logical_id TEXT;
              ALTER TABLE canonical_edges ADD COLUMN superseded_at INTEGER;
              CREATE UNIQUE INDEX IF NOT EXISTS canonical_nodes_logical_active_idx
                  ON canonical_nodes(logical_id, kind) WHERE superseded_at IS NULL;
              CREATE UNIQUE INDEX IF NOT EXISTS canonical_edges_logical_active_idx
                  ON canonical_edges(logical_id, kind) WHERE superseded_at IS NULL;
              CREATE INDEX IF NOT EXISTS canonical_nodes_kind_idx
                  ON canonical_nodes(kind);
              CREATE INDEX IF NOT EXISTS canonical_edges_from_id_idx
                  ON canonical_edges(from_id);
              CREATE INDEX IF NOT EXISTS canonical_edges_to_id_idx
                  ON canonical_edges(to_id);",
    },
];

pub fn migrate(conn: &Connection) -> Result<MigrationReport, MigrationError> {
    migrate_with_steps(conn, MIGRATIONS)
}

pub fn migrate_with_steps(
    conn: &Connection,
    migrations: &[Migration],
) -> Result<MigrationReport, MigrationError> {
    migrate_with_event_sink(conn, migrations, |_| {})
}

pub fn migrate_with_event_sink(
    conn: &Connection,
    migrations: &[Migration],
    mut emit: impl FnMut(&MigrationStepReport),
) -> Result<MigrationReport, MigrationError> {
    let before = user_version(conn)?;
    if before > SCHEMA_VERSION {
        return Err(MigrationError::IncompatibleSchemaVersion {
            seen: before,
            supported: SCHEMA_VERSION,
        });
    }

    let mut current = before;
    let mut reports = Vec::new();

    for migration in migrations.iter().filter(|migration| migration.step_id > before) {
        if migration.step_id != current.saturating_add(1) {
            return Err(MigrationError::Storage {
                message: "migration registry is not contiguous",
            });
        }

        let started = Instant::now();
        if let Err(_err) = apply_one(conn, migration) {
            reports.push(MigrationStepReport {
                step_id: migration.step_id,
                duration_ms: Some(duration_ms(started)),
                failed: true,
            });
            emit(reports.last().expect("failed step report was just pushed"));
            let schema_version_current = user_version(conn).unwrap_or(current);
            return Err(MigrationError::MigrationError(MigrationFailureReport {
                schema_version_before: before,
                schema_version_current,
                migration_steps: reports,
            }));
        }

        current = migration.step_id;
        reports.push(MigrationStepReport {
            step_id: migration.step_id,
            duration_ms: Some(duration_ms(started)),
            failed: false,
        });
        emit(reports.last().expect("successful step report was just pushed"));
    }

    Ok(MigrationReport {
        schema_version_before: before,
        schema_version_after: user_version(conn)?,
        migration_steps: reports,
    })
}

fn apply_one(conn: &Connection, migration: &Migration) -> rusqlite::Result<()> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| {
        conn.execute_batch(migration.sql)?;
        conn.pragma_update(None, PRAGMA_USER_VERSION, migration.step_id)?;
        Ok(())
    })();

    match result {
        Ok(()) => conn.execute_batch("COMMIT"),
        Err(err) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(err)
        }
    }
}

fn user_version(conn: &Connection) -> Result<u32, MigrationError> {
    conn.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
        .map_err(|_| MigrationError::Storage { message: "could not read schema version" })
}

fn duration_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationAccretionError {
    pub offender: String,
}

impl Display for MigrationAccretionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "migration accretion guard rejected {}", self.offender)
    }
}

impl std::error::Error for MigrationAccretionError {}

pub fn check_migration_accretion(name: &str, sql: &str) -> Result<(), MigrationAccretionError> {
    let upper = sql.to_ascii_uppercase();
    let adds_schema = upper.contains("CREATE TABLE") || upper.contains("ADD COLUMN");
    let names_removal = upper.contains("DROP TABLE") || upper.contains("DROP COLUMN");
    let has_exemption = sql.contains("-- MIGRATION-ACCRETION-EXEMPTION: ");

    if adds_schema && !names_removal && !has_exemption {
        return Err(MigrationAccretionError { offender: name.to_string() });
    }

    Ok(())
}
