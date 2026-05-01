//! Pack H: admin introspection APIs.
//!
//! Read-side aggregation surfaces that let callers detect per-kind
//! vector / FTS configuration drift. fathomdb deliberately has no
//! client-side "expected kinds" registry — these methods expose what
//! has actually been configured in the database so that callers can
//! cross-reference against their own kind list.

use std::collections::BTreeMap;

use rusqlite::OptionalExtension;
use serde::Serialize;

use crate::EngineError;

use super::AdminService;

/// Static install/build surface: feature flags, presets, and versions.
///
/// Pure function — does NOT touch the database. Intended for
/// `admin.capabilities()` to let clients assert what the running binary
/// supports without opening a connection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Capabilities {
    /// `sqlite-vec` feature compiled in.
    pub sqlite_vec: bool,
    /// FTS tokenizer preset names (matches the first column of
    /// [`crate::TOKENIZER_PRESETS`]).
    pub fts_tokenizers: Vec<String>,
    /// Known embedder slots. `"builtin"` is always present; its
    /// `available` flag reflects the `default-embedder` feature.
    pub embedders: BTreeMap<String, EmbedderCapability>,
    /// Latest schema version this binary knows how to apply.
    pub schema_version: u32,
    /// `CARGO_PKG_VERSION` of the `fathomdb-engine` crate at build time.
    pub fathomdb_version: String,
}

/// Per-embedder capability entry on [`Capabilities::embedders`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EmbedderCapability {
    /// True if this embedder is compiled in and could be constructed by
    /// the engine at `open()` time.
    pub available: bool,
    /// Model identity the embedder reports (populated only when
    /// `available`). e.g. `"BAAI/bge-small-en-v1.5"`.
    pub model_identity: Option<String>,
    /// Vector dimension the embedder produces.
    pub dimensions: Option<usize>,
    /// Maximum tokens per single embed call.
    pub max_tokens: Option<usize>,
}

/// Snapshot of the runtime configuration that drives vector / FTS
/// projection behaviour.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CurrentConfig {
    /// Currently active embedding profile row, if any.
    pub active_embedding_profile: Option<EmbeddingProfileSummary>,
    /// All rows in `vector_index_schemas`, keyed by `kind`.
    pub vec_kinds: BTreeMap<String, VecKindConfig>,
    /// All FTS profiles (from `projection_profiles` where facet='fts'),
    /// keyed by `kind`.
    pub fts_kinds: BTreeMap<String, FtsKindConfig>,
    /// Bulk counts across `vector_projection_work`.
    pub work_queue: WorkQueueSummary,
}

/// Slim projection of `vector_embedding_profiles` WHERE active=1.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EmbeddingProfileSummary {
    pub profile_id: i64,
    pub model_identity: String,
    pub model_version: Option<String>,
    pub dimensions: i64,
    pub normalization_policy: Option<String>,
    pub max_tokens: Option<i64>,
    pub activated_at: Option<i64>,
}

/// Per-kind vector index configuration (one row of `vector_index_schemas`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct VecKindConfig {
    pub kind: String,
    pub enabled: bool,
    pub source_mode: String,
    pub state: String,
    pub last_error: Option<String>,
    pub last_completed_at: Option<i64>,
    pub updated_at: i64,
}

/// Slim per-kind FTS view — enough for a drift check.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FtsKindConfig {
    pub kind: String,
    pub tokenizer: String,
    pub property_schema_present: bool,
}

/// Aggregated counts across `vector_projection_work`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct WorkQueueSummary {
    pub pending_incremental: u64,
    pub pending_backfill: u64,
    pub inflight: u64,
    pub failed: u64,
    pub discarded: u64,
}

/// Per-kind view produced by [`AdminService::describe_kind`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct KindDescription {
    pub kind: String,
    pub vec: Option<VecKindConfig>,
    pub fts: Option<FtsKindConfig>,
    /// Count of canonical chunks belonging to active nodes of this kind.
    pub chunk_count: u64,
    /// Row count in `vec_<kind>` if the table exists, else `None`.
    pub vec_rows: Option<u64>,
    /// Active embedding profile identity, for convenience.
    pub embedding_identity: Option<String>,
}

impl AdminService {
    /// Return the static install/build surface. Does not open the DB.
    #[must_use]
    pub fn capabilities() -> Capabilities {
        let fts_tokenizers: Vec<String> = super::TOKENIZER_PRESETS
            .iter()
            .map(|(name, _)| (*name).to_owned())
            .collect();

        let mut embedders: BTreeMap<String, EmbedderCapability> = BTreeMap::new();
        embedders.insert("builtin".to_owned(), builtin_embedder_capability());

        let schema_version = fathomdb_schema::SchemaManager::new().current_version().0;

        Capabilities {
            sqlite_vec: cfg!(feature = "sqlite-vec"),
            fts_tokenizers,
            embedders,
            schema_version,
            fathomdb_version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }

    /// Return a snapshot of runtime configuration: active embedding
    /// profile, all `vector_index_schemas` rows, all FTS profiles, and
    /// aggregate work-queue counts.
    ///
    /// Aggregates only — all underlying tables are already individually
    /// queryable via other admin methods. Single read transaction.
    ///
    /// # Errors
    /// Returns [`EngineError`] on database failure.
    pub fn current_config(&self) -> Result<CurrentConfig, EngineError> {
        let conn = self.connect()?;

        let active_embedding_profile = conn
            .query_row(
                "SELECT profile_id, model_identity, model_version, dimensions, \
                        normalization_policy, max_tokens, activated_at \
                 FROM vector_embedding_profiles WHERE active = 1",
                [],
                |row| {
                    Ok(EmbeddingProfileSummary {
                        profile_id: row.get(0)?,
                        model_identity: row.get(1)?,
                        model_version: row.get(2)?,
                        dimensions: row.get(3)?,
                        normalization_policy: row.get(4)?,
                        max_tokens: row.get(5)?,
                        activated_at: row.get(6)?,
                    })
                },
            )
            .optional()?;

        let mut vec_kinds: BTreeMap<String, VecKindConfig> = BTreeMap::new();
        {
            let mut stmt = conn.prepare(
                "SELECT kind, enabled, source_mode, state, last_error, last_completed_at, updated_at \
                 FROM vector_index_schemas ORDER BY kind",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(VecKindConfig {
                    kind: row.get(0)?,
                    enabled: row.get::<_, i64>(1)? == 1,
                    source_mode: row.get(2)?,
                    state: row.get(3)?,
                    last_error: row.get(4)?,
                    last_completed_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })?;
            for r in rows {
                let v = r?;
                vec_kinds.insert(v.kind.clone(), v);
            }
        }

        let mut fts_kinds: BTreeMap<String, FtsKindConfig> = BTreeMap::new();
        {
            let mut stmt = conn.prepare(
                "SELECT kind, json_extract(config_json, '$.tokenizer') \
                 FROM projection_profiles WHERE facet = 'fts' ORDER BY kind",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                ))
            })?;
            for r in rows {
                let (kind, tokenizer) = r?;
                let property_schema_present: bool = conn
                    .query_row(
                        "SELECT 1 FROM fts_property_schemas WHERE kind = ?1",
                        rusqlite::params![kind],
                        |_| Ok(true),
                    )
                    .optional()?
                    .unwrap_or(false);
                fts_kinds.insert(
                    kind.clone(),
                    FtsKindConfig {
                        kind,
                        tokenizer,
                        property_schema_present,
                    },
                );
            }
        }

        let work_queue = aggregate_work_queue(&conn)?;

        Ok(CurrentConfig {
            active_embedding_profile,
            vec_kinds,
            fts_kinds,
            work_queue,
        })
    }

    /// Return a per-kind view: vector config, FTS config, chunk count,
    /// and vec-row count (if the per-kind vec table exists).
    ///
    /// # Errors
    /// Returns [`EngineError`] on database failure.
    pub fn describe_kind(&self, kind: &str) -> Result<KindDescription, EngineError> {
        let conn = self.connect()?;

        let vec: Option<VecKindConfig> = conn
            .query_row(
                "SELECT kind, enabled, source_mode, state, last_error, last_completed_at, updated_at \
                 FROM vector_index_schemas WHERE kind = ?1",
                rusqlite::params![kind],
                |row| {
                    Ok(VecKindConfig {
                        kind: row.get(0)?,
                        enabled: row.get::<_, i64>(1)? == 1,
                        source_mode: row.get(2)?,
                        state: row.get(3)?,
                        last_error: row.get(4)?,
                        last_completed_at: row.get(5)?,
                        updated_at: row.get(6)?,
                    })
                },
            )
            .optional()?;

        let fts: Option<FtsKindConfig> = conn
            .query_row(
                "SELECT kind, json_extract(config_json, '$.tokenizer') \
                 FROM projection_profiles WHERE kind = ?1 AND facet = 'fts'",
                rusqlite::params![kind],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    ))
                },
            )
            .optional()?
            .map(|(kind, tokenizer)| {
                let property_schema_present = conn
                    .query_row(
                        "SELECT 1 FROM fts_property_schemas WHERE kind = ?1",
                        rusqlite::params![&kind],
                        |_| Ok(true),
                    )
                    .optional()
                    .ok()
                    .flatten()
                    .is_some();
                FtsKindConfig {
                    kind,
                    tokenizer,
                    property_schema_present,
                }
            });

        let chunk_count: u64 = conn
            .query_row(
                "SELECT count(*) FROM chunks c \
                 JOIN nodes n ON n.logical_id = c.node_logical_id AND n.superseded_at IS NULL \
                 WHERE n.kind = ?1",
                rusqlite::params![kind],
                |row| row.get::<_, i64>(0),
            )
            .map_or(0, i64::cast_unsigned);

        let table_name = fathomdb_schema::vec_kind_table_name(kind);
        let vec_rows: Option<u64> = table_exists(&conn, &table_name)?
            .then(|| -> Result<u64, EngineError> {
                Ok(conn
                    .query_row(&format!("SELECT count(*) FROM {table_name}"), [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .map(i64::cast_unsigned)?)
            })
            .transpose()?;

        let embedding_identity = conn
            .query_row(
                "SELECT model_identity FROM vector_embedding_profiles WHERE active = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        Ok(KindDescription {
            kind: kind.to_owned(),
            vec,
            fts,
            chunk_count,
            vec_rows,
            embedding_identity,
        })
    }
}

fn aggregate_work_queue(conn: &rusqlite::Connection) -> Result<WorkQueueSummary, EngineError> {
    let mut summary = WorkQueueSummary::default();
    let mut stmt = conn.prepare(
        "SELECT state, \
                SUM(CASE WHEN priority >= 1000 THEN 1 ELSE 0 END), \
                SUM(CASE WHEN priority <  1000 THEN 1 ELSE 0 END), \
                COUNT(*) \
         FROM vector_projection_work GROUP BY state",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<i64>>(1)?.unwrap_or(0),
            row.get::<_, Option<i64>>(2)?.unwrap_or(0),
            row.get::<_, i64>(3)?,
        ))
    })?;
    for r in rows {
        let (state, incr, back, total) = r?;
        let total_u = i64::cast_unsigned(total);
        match state.as_str() {
            "pending" => {
                summary.pending_incremental = i64::cast_unsigned(incr);
                summary.pending_backfill = i64::cast_unsigned(back);
            }
            "inflight" => summary.inflight = total_u,
            "failed" => summary.failed = total_u,
            "discarded" => summary.discarded = total_u,
            _ => {}
        }
    }
    Ok(summary)
}

fn table_exists(conn: &rusqlite::Connection, name: &str) -> Result<bool, EngineError> {
    let exists: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type IN ('table','view') AND name = ?1",
            rusqlite::params![name],
            |row| row.get(0),
        )
        .optional()?;
    Ok(exists.is_some())
}

#[cfg(feature = "default-embedder")]
fn builtin_embedder_capability() -> EmbedderCapability {
    use crate::embedder::{BatchEmbedder, BuiltinBgeSmallEmbedder};
    let embedder = BuiltinBgeSmallEmbedder::new();
    let id = BatchEmbedder::identity(&embedder);
    EmbedderCapability {
        available: true,
        model_identity: Some(id.model_identity),
        dimensions: Some(id.dimension),
        max_tokens: Some(BatchEmbedder::max_tokens(&embedder)),
    }
}

#[cfg(not(feature = "default-embedder"))]
fn builtin_embedder_capability() -> EmbedderCapability {
    EmbedderCapability {
        available: false,
        model_identity: None,
        dimensions: None,
        max_tokens: None,
    }
}
