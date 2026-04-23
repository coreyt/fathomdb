use std::path::Path;

use fathomdb_schema::SchemaError;
use rusqlite::{OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::Digest;

use super::{
    AdminService, CURRENT_VECTOR_CONTRACT_FORMAT_VERSION, EngineError, MAX_AUDIT_METADATA_BYTES,
    MAX_CONTRACT_JSON_BYTES, MAX_POLICY_LEN, MAX_PROFILE_LEN, ProjectionRepairReport,
    ProjectionTarget, VecProfile, VectorRegenerationConfig, VectorRegenerationReport,
};
use crate::embedder::{BatchEmbedder, QueryEmbedder, QueryEmbedderIdentity};
use crate::ids::new_id;

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct VectorEmbeddingContractRecord {
    profile: String,
    table_name: String,
    model_identity: String,
    model_version: String,
    dimension: usize,
    normalization_policy: String,
    chunking_policy: String,
    preprocessing_policy: String,
    generator_command_json: String,
    applied_at: i64,
    snapshot_hash: String,
    contract_format_version: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(super) struct VectorRegenerationInputChunk {
    pub(super) chunk_id: String,
    pub(super) node_logical_id: String,
    pub(super) kind: String,
    pub(super) text_content: String,
    pub(super) byte_start: Option<i64>,
    pub(super) byte_end: Option<i64>,
    pub(super) source_ref: Option<String>,
    pub(super) created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(super) struct VectorRegenerationInput {
    pub(super) profile: String,
    pub(super) table_name: String,
    pub(super) model_identity: String,
    pub(super) model_version: String,
    pub(super) dimension: usize,
    pub(super) normalization_policy: String,
    pub(super) chunking_policy: String,
    pub(super) preprocessing_policy: String,
    pub(super) chunks: Vec<VectorRegenerationInputChunk>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VectorRegenerationFailureClass {
    InvalidContract,
    EmbedderFailure,
    InvalidEmbedderOutput,
    SnapshotDrift,
    UnsupportedVecCapability,
}

impl VectorRegenerationFailureClass {
    fn label(self) -> &'static str {
        match self {
            Self::InvalidContract => "invalid contract",
            Self::EmbedderFailure => "embedder failure",
            Self::InvalidEmbedderOutput => "invalid embedder output",
            Self::SnapshotDrift => "snapshot drift",
            Self::UnsupportedVecCapability => "unsupported vec capability",
        }
    }

    fn retryable(self) -> bool {
        matches!(self, Self::SnapshotDrift)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VectorRegenerationFailure {
    class: VectorRegenerationFailureClass,
    detail: String,
}

impl VectorRegenerationFailure {
    pub(crate) fn new(class: VectorRegenerationFailureClass, detail: impl Into<String>) -> Self {
        Self {
            class,
            detail: detail.into(),
        }
    }

    pub(super) fn to_engine_error(&self) -> EngineError {
        let retry_suffix = if self.class.retryable() {
            " [retryable]"
        } else {
            ""
        };
        EngineError::Bridge(format!(
            "vector regeneration {}: {}{}",
            self.class.label(),
            self.detail,
            retry_suffix
        ))
    }

    pub(super) fn failure_class_label(&self) -> &'static str {
        self.class.label()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(super) struct VectorRegenerationAuditMetadata {
    pub(super) profile: String,
    pub(super) model_identity: String,
    pub(super) model_version: String,
    pub(super) chunk_count: usize,
    pub(super) snapshot_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) failure_class: Option<String>,
}

/// Source of content for a managed per-kind vector index.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VectorSource {
    /// Use existing `chunks` rows belonging to nodes of the configured kind.
    Chunks,
}

/// Outcome of [`AdminService::configure_vec_kind`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ConfigureVecOutcome {
    /// Node kind that was configured.
    pub kind: String,
    /// Number of backfill rows newly enqueued in `vector_projection_work`.
    pub enqueued_backfill_rows: usize,
    /// True if this kind already had an enabled vector index schema row
    /// before the call.
    pub was_already_enabled: bool,
}

/// Managed-projection status snapshot for a given node `kind`.
///
/// Returned by [`AdminService::get_vec_index_status`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct VecIndexStatus {
    /// Node kind queried.
    pub kind: String,
    /// True if `vector_index_schemas.enabled = 1` for this kind.
    pub enabled: bool,
    /// Lifecycle state stored in `vector_index_schemas.state`, or
    /// `"unconfigured"` when there is no schema row for this kind.
    pub state: String,
    /// Pending work rows with `priority >= 1000` (incremental writes).
    pub pending_incremental: u64,
    /// Pending work rows with `priority < 1000` (backfill).
    pub pending_backfill: u64,
    /// Last recorded error, if any.
    pub last_error: Option<String>,
    /// Unix timestamp when the kind last completed rebuild, if any.
    pub last_completed_at: Option<i64>,
    /// `model_identity` of the currently-active embedding profile, if any.
    pub embedding_identity: Option<String>,
}

impl AdminService {
    /// Retrieve the vector embedding profile for a specific node `kind`.
    ///
    /// Reads from `projection_profiles` under `(kind=<kind>, facet='vec')`.
    /// Returns `None` if no vector profile has been persisted for this kind yet.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the database query fails.
    pub fn get_vec_profile(&self, kind: &str) -> Result<Option<VecProfile>, EngineError> {
        let conn = self.connect()?;
        let result = conn
            .query_row(
                "SELECT \
                   json_extract(config_json, '$.model_identity'), \
                   json_extract(config_json, '$.model_version'), \
                   CAST(json_extract(config_json, '$.dimensions') AS INTEGER), \
                   active_at, \
                   created_at \
                 FROM projection_profiles WHERE kind = ?1 AND facet = 'vec'",
                rusqlite::params![kind],
                |row| {
                    Ok(VecProfile {
                        model_identity: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                        model_version: row.get(1)?,
                        dimensions: {
                            let d: i64 = row.get::<_, Option<i64>>(2)?.unwrap_or(0);
                            u32::try_from(d).unwrap_or(0)
                        },
                        active_at: row.get(3)?,
                        created_at: row.get(4)?,
                    })
                },
            )
            .optional()?;
        Ok(result)
    }

    /// Write or update the global vector profile from a JSON identity string.
    ///
    /// This is a private helper called after a successful vector regeneration.
    /// Errors are logged as warnings and not propagated to the caller.
    #[allow(dead_code)]
    fn set_vec_profile_inner(
        conn: &rusqlite::Connection,
        identity_json: &str,
    ) -> Result<VecProfile, rusqlite::Error> {
        conn.execute(
            r"INSERT INTO projection_profiles (kind, facet, config_json, active_at, created_at)
              VALUES ('*', 'vec', ?1, unixepoch(), unixepoch())
              ON CONFLICT(kind, facet) DO UPDATE SET
                  config_json = ?1,
                  active_at   = unixepoch()",
            rusqlite::params![identity_json],
        )?;
        conn.query_row(
            "SELECT \
               json_extract(config_json, '$.model_identity'), \
               json_extract(config_json, '$.model_version'), \
               CAST(json_extract(config_json, '$.dimensions') AS INTEGER), \
               active_at, \
               created_at \
             FROM projection_profiles WHERE kind = '*' AND facet = 'vec'",
            [],
            |row| {
                Ok(VecProfile {
                    model_identity: row.get(0)?,
                    model_version: row.get(1)?,
                    dimensions: {
                        let d: i64 = row.get(2)?;
                        u32::try_from(d).unwrap_or(0)
                    },
                    active_at: row.get(3)?,
                    created_at: row.get(4)?,
                })
            },
        )
    }

    /// Persist or update the global vector profile from a JSON config string.
    ///
    /// `config_json` must be valid JSON with at least a `model_identity`
    /// field and `dimensions`.  The JSON is stored verbatim in the
    /// `projection_profiles` table under `kind='*'`, `facet='vec'`.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the database write fails.
    pub fn set_vec_profile(&self, config_json: &str) -> Result<VecProfile, EngineError> {
        let conn = self.connect()?;
        Self::set_vec_profile_inner(&conn, config_json).map_err(EngineError::Sqlite)
    }

    /// Estimate the cost of rebuilding a projection.
    ///
    /// For facet `"fts"`: counts active nodes of `kind`.
    /// For facet `"vec"`: counts all chunks.
    ///
    /// # Errors
    /// Returns [`EngineError`] for unknown facets or database errors.
    pub fn preview_projection_impact(
        &self,
        kind: &str,
        facet: &str,
    ) -> Result<super::ProjectionImpact, EngineError> {
        let conn = self.connect()?;
        match facet {
            "fts" => {
                let rows: u64 = conn
                    .query_row(
                        "SELECT count(*) FROM nodes WHERE kind = ?1 AND superseded_at IS NULL",
                        rusqlite::params![kind],
                        |row| row.get::<_, i64>(0),
                    )
                    .map(i64::cast_unsigned)?;
                let current_tokenizer = self.get_fts_profile(kind)?.map(|p| p.tokenizer);
                Ok(super::ProjectionImpact {
                    rows_to_rebuild: rows,
                    estimated_seconds: rows / 5000,
                    temp_db_size_bytes: rows * 200,
                    current_tokenizer,
                    target_tokenizer: None,
                })
            }
            "vec" => {
                let rows: u64 = conn
                    .query_row("SELECT count(*) FROM chunks", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .map(i64::cast_unsigned)?;
                Ok(super::ProjectionImpact {
                    rows_to_rebuild: rows,
                    estimated_seconds: rows / 100,
                    temp_db_size_bytes: rows * 1536,
                    current_tokenizer: None,
                    target_tokenizer: None,
                })
            }
            other => Err(EngineError::Bridge(format!(
                "unknown projection facet: {other:?}"
            ))),
        }
    }

    /// Recreate enabled vector profiles from persisted `vector_profiles` metadata.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the database connection fails, reading metadata fails,
    /// or sqlite-vec support is unavailable while enabled profiles are present.
    pub fn restore_vector_profiles(&self) -> Result<ProjectionRepairReport, EngineError> {
        let conn = self.connect()?;
        let profiles: Vec<(String, String, i64)> = {
            let mut stmt = conn.prepare(
                "SELECT profile, table_name, dimension \
                 FROM vector_profiles WHERE enabled = 1 ORDER BY profile",
            )?;
            stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
        };

        for (profile, table_name, dimension) in &profiles {
            let dimension = usize::try_from(*dimension).map_err(|_| {
                EngineError::Bridge(format!("invalid vector profile dimension: {dimension}"))
            })?;
            self.schema_manager
                .ensure_vector_profile(&conn, profile, table_name, dimension)?;
        }

        Ok(ProjectionRepairReport {
            targets: vec![ProjectionTarget::Vec],
            rebuilt_rows: profiles.len(),
            notes: vec![],
        })
    }

    /// Rebuild vector embeddings using an application-supplied regeneration
    /// contract and generator command.
    ///
    /// The config is persisted in `vector_embedding_contracts` so the metadata
    /// required for recovery survives future repair runs.
    ///
    /// Vector identity is stamped from [`QueryEmbedder::identity`] — the
    /// caller supplies the embedder and cannot override its identity. This
    /// makes drift between the read-path and write-path identity stories
    /// structurally impossible.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the database connection fails, the config is
    /// invalid, the embedder fails, or the regenerated embeddings are
    /// malformed.
    #[allow(clippy::too_many_lines)]
    pub fn regenerate_vector_embeddings(
        &self,
        embedder: &dyn QueryEmbedder,
        config: &VectorRegenerationConfig,
    ) -> Result<VectorRegenerationReport, EngineError> {
        let conn = self.connect()?;
        let identity = embedder.identity();
        let config = validate_vector_regeneration_config(&conn, config, &identity)
            .map_err(|failure| failure.to_engine_error())?;
        let chunks = collect_regeneration_chunks(&conn)?;
        let payload = build_regeneration_input(&config, &identity, chunks.clone());
        let snapshot_hash = compute_snapshot_hash(&payload)?;
        let audit_metadata = VectorRegenerationAuditMetadata {
            profile: config.profile.clone(),
            model_identity: identity.model_identity.clone(),
            model_version: identity.model_version.clone(),
            chunk_count: chunks.len(),
            snapshot_hash: snapshot_hash.clone(),
            failure_class: None,
        };
        persist_vector_regeneration_event(
            &conn,
            "vector_regeneration_requested",
            &config.profile,
            &audit_metadata,
        )?;
        let notes = vec!["vector embeddings regenerated via configured embedder".to_owned()];

        let mut embedding_map: std::collections::HashMap<String, Vec<u8>> =
            std::collections::HashMap::with_capacity(chunks.len());
        for chunk in &chunks {
            let vector = match embedder.embed_query(&chunk.text_content) {
                Ok(vector) => vector,
                Err(error) => {
                    let failure = VectorRegenerationFailure::new(
                        VectorRegenerationFailureClass::EmbedderFailure,
                        format!("embedder failed for chunk '{}': {error}", chunk.chunk_id),
                    );
                    self.persist_vector_regeneration_failure_best_effort(
                        &config.profile,
                        &audit_metadata,
                        &failure,
                    );
                    return Err(failure.to_engine_error());
                }
            };
            if vector.len() != identity.dimension {
                let failure = VectorRegenerationFailure::new(
                    VectorRegenerationFailureClass::InvalidEmbedderOutput,
                    format!(
                        "embedder produced {} values for chunk '{}', expected {}",
                        vector.len(),
                        chunk.chunk_id,
                        identity.dimension
                    ),
                );
                self.persist_vector_regeneration_failure_best_effort(
                    &config.profile,
                    &audit_metadata,
                    &failure,
                );
                return Err(failure.to_engine_error());
            }
            if vector.iter().any(|value| !value.is_finite()) {
                let failure = VectorRegenerationFailure::new(
                    VectorRegenerationFailureClass::InvalidEmbedderOutput,
                    format!(
                        "embedder returned non-finite values for chunk '{}'",
                        chunk.chunk_id
                    ),
                );
                self.persist_vector_regeneration_failure_best_effort(
                    &config.profile,
                    &audit_metadata,
                    &failure,
                );
                return Err(failure.to_engine_error());
            }
            let bytes: Vec<u8> = vector
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect();
            embedding_map.insert(chunk.chunk_id.clone(), bytes);
        }

        let table_name = fathomdb_schema::vec_kind_table_name(&config.kind);
        let mut conn = conn;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        match self
            .schema_manager
            .ensure_vec_kind_profile(&tx, &config.kind, identity.dimension)
        {
            Ok(()) => {}
            Err(SchemaError::MissingCapability(message)) => {
                let failure = VectorRegenerationFailure::new(
                    VectorRegenerationFailureClass::UnsupportedVecCapability,
                    message,
                );
                drop(tx);
                self.persist_vector_regeneration_failure_best_effort(
                    &config.profile,
                    &audit_metadata,
                    &failure,
                );
                return Err(failure.to_engine_error());
            }
            Err(error) => return Err(EngineError::Schema(error)),
        }
        let apply_chunks = collect_regeneration_chunks(&tx)?;
        let apply_payload = build_regeneration_input(&config, &identity, apply_chunks.clone());
        let apply_hash = compute_snapshot_hash(&apply_payload)?;
        if apply_hash != snapshot_hash {
            let failure = VectorRegenerationFailure::new(
                VectorRegenerationFailureClass::SnapshotDrift,
                "chunk snapshot changed during generation; retry".to_owned(),
            );
            drop(tx);
            self.persist_vector_regeneration_failure_best_effort(
                &config.profile,
                &audit_metadata,
                &failure,
            );
            return Err(failure.to_engine_error());
        }
        persist_vector_contract(&tx, &config, &table_name, &identity, &snapshot_hash)?;
        tx.execute(&format!("DELETE FROM {table_name}"), [])?;
        let mut stmt = tx.prepare_cached(&format!(
            "INSERT INTO {table_name} (chunk_id, embedding) VALUES (?1, ?2)"
        ))?;
        let mut regenerated_rows = 0usize;
        for chunk in &apply_chunks {
            let Some(embedding) = embedding_map.remove(&chunk.chunk_id) else {
                drop(stmt);
                drop(tx);
                let failure = VectorRegenerationFailure::new(
                    VectorRegenerationFailureClass::InvalidEmbedderOutput,
                    format!(
                        "embedder did not produce a vector for chunk '{}'",
                        chunk.chunk_id
                    ),
                );
                self.persist_vector_regeneration_failure_best_effort(
                    &config.profile,
                    &audit_metadata,
                    &failure,
                );
                return Err(failure.to_engine_error());
            };
            stmt.execute(rusqlite::params![chunk.chunk_id.as_str(), embedding])?;
            regenerated_rows += 1;
        }
        drop(stmt);
        persist_vector_regeneration_event(
            &tx,
            "vector_regeneration_apply",
            &config.profile,
            &audit_metadata,
        )?;
        tx.commit()?;

        Ok(VectorRegenerationReport {
            profile: config.profile.clone(),
            table_name,
            dimension: identity.dimension,
            total_chunks: chunks.len(),
            regenerated_rows,
            contract_persisted: true,
            notes,
        })
    }

    /// Regenerate vector embeddings in-process using a [`BatchEmbedder`].
    ///
    /// Functionally equivalent to [`regenerate_vector_embeddings`] but uses
    /// `BatchEmbedder::batch_embed` to process all chunks in one call. This
    /// is the intended path for [`BuiltinBgeSmallEmbedder`] — it keeps the
    /// forward pass in-process without requiring an external subprocess.
    ///
    /// The subprocess-based path ([`regenerate_vector_embeddings`]) remains
    /// intact for callers who supply their own generator binary.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the database connection fails, the config is
    /// invalid, the embedder fails, or the regenerated embeddings are malformed.
    #[allow(clippy::too_many_lines)]
    pub fn regenerate_vector_embeddings_in_process(
        &self,
        embedder: &dyn BatchEmbedder,
        config: &VectorRegenerationConfig,
    ) -> Result<VectorRegenerationReport, EngineError> {
        let conn = self.connect()?;
        let identity = embedder.identity();
        let config = validate_vector_regeneration_config(&conn, config, &identity)
            .map_err(|failure| failure.to_engine_error())?;
        let chunks = collect_regeneration_chunks(&conn)?;
        let payload = build_regeneration_input(&config, &identity, chunks.clone());
        let snapshot_hash = compute_snapshot_hash(&payload)?;
        let audit_metadata = VectorRegenerationAuditMetadata {
            profile: config.profile.clone(),
            model_identity: identity.model_identity.clone(),
            model_version: identity.model_version.clone(),
            chunk_count: chunks.len(),
            snapshot_hash: snapshot_hash.clone(),
            failure_class: None,
        };
        persist_vector_regeneration_event(
            &conn,
            "vector_regeneration_requested",
            &config.profile,
            &audit_metadata,
        )?;
        let notes = vec!["vector embeddings regenerated via in-process batch embedder".to_owned()];

        // Collect texts and call batch_embed once for all chunks.
        let chunk_texts: Vec<String> = chunks.iter().map(|c| c.text_content.clone()).collect();
        let batch_vectors = match embedder.batch_embed(&chunk_texts) {
            Ok(vecs) => vecs,
            Err(error) => {
                let failure = VectorRegenerationFailure::new(
                    VectorRegenerationFailureClass::EmbedderFailure,
                    format!("batch embedder failed: {error}"),
                );
                self.persist_vector_regeneration_failure_best_effort(
                    &config.profile,
                    &audit_metadata,
                    &failure,
                );
                return Err(failure.to_engine_error());
            }
        };
        if batch_vectors.len() != chunks.len() {
            let failure = VectorRegenerationFailure::new(
                VectorRegenerationFailureClass::InvalidEmbedderOutput,
                format!(
                    "batch embedder returned {} vectors for {} chunks",
                    batch_vectors.len(),
                    chunks.len()
                ),
            );
            self.persist_vector_regeneration_failure_best_effort(
                &config.profile,
                &audit_metadata,
                &failure,
            );
            return Err(failure.to_engine_error());
        }

        let mut embedding_map: std::collections::HashMap<String, Vec<u8>> =
            std::collections::HashMap::with_capacity(chunks.len());
        for (chunk, vector) in chunks.iter().zip(batch_vectors) {
            if vector.len() != identity.dimension {
                let failure = VectorRegenerationFailure::new(
                    VectorRegenerationFailureClass::InvalidEmbedderOutput,
                    format!(
                        "embedder produced {} values for chunk '{}', expected {}",
                        vector.len(),
                        chunk.chunk_id,
                        identity.dimension
                    ),
                );
                self.persist_vector_regeneration_failure_best_effort(
                    &config.profile,
                    &audit_metadata,
                    &failure,
                );
                return Err(failure.to_engine_error());
            }
            if vector.iter().any(|value| !value.is_finite()) {
                let failure = VectorRegenerationFailure::new(
                    VectorRegenerationFailureClass::InvalidEmbedderOutput,
                    format!(
                        "embedder returned non-finite values for chunk '{}'",
                        chunk.chunk_id
                    ),
                );
                self.persist_vector_regeneration_failure_best_effort(
                    &config.profile,
                    &audit_metadata,
                    &failure,
                );
                return Err(failure.to_engine_error());
            }
            let bytes: Vec<u8> = vector
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect();
            embedding_map.insert(chunk.chunk_id.clone(), bytes);
        }

        let mut conn = conn;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let table_name = fathomdb_schema::vec_kind_table_name(&config.kind);
        match self
            .schema_manager
            .ensure_vec_kind_profile(&tx, &config.kind, identity.dimension)
        {
            Ok(()) => {}
            Err(SchemaError::MissingCapability(message)) => {
                let failure = VectorRegenerationFailure::new(
                    VectorRegenerationFailureClass::UnsupportedVecCapability,
                    message,
                );
                drop(tx);
                self.persist_vector_regeneration_failure_best_effort(
                    &config.profile,
                    &audit_metadata,
                    &failure,
                );
                return Err(failure.to_engine_error());
            }
            Err(error) => return Err(EngineError::Schema(error)),
        }
        let apply_chunks = collect_regeneration_chunks(&tx)?;
        let apply_payload = build_regeneration_input(&config, &identity, apply_chunks.clone());
        let apply_hash = compute_snapshot_hash(&apply_payload)?;
        if apply_hash != snapshot_hash {
            let failure = VectorRegenerationFailure::new(
                VectorRegenerationFailureClass::SnapshotDrift,
                "chunk snapshot changed during generation; retry".to_owned(),
            );
            drop(tx);
            self.persist_vector_regeneration_failure_best_effort(
                &config.profile,
                &audit_metadata,
                &failure,
            );
            return Err(failure.to_engine_error());
        }
        persist_vector_contract(&tx, &config, &table_name, &identity, &snapshot_hash)?;
        tx.execute(&format!("DELETE FROM {table_name}"), [])?;
        let mut stmt = tx.prepare_cached(&format!(
            "INSERT INTO {table_name} (chunk_id, embedding) VALUES (?1, ?2)"
        ))?;
        let mut regenerated_rows = 0usize;
        for chunk in &apply_chunks {
            let Some(embedding) = embedding_map.remove(&chunk.chunk_id) else {
                drop(stmt);
                drop(tx);
                let failure = VectorRegenerationFailure::new(
                    VectorRegenerationFailureClass::InvalidEmbedderOutput,
                    format!(
                        "embedder did not produce a vector for chunk '{}'",
                        chunk.chunk_id
                    ),
                );
                self.persist_vector_regeneration_failure_best_effort(
                    &config.profile,
                    &audit_metadata,
                    &failure,
                );
                return Err(failure.to_engine_error());
            };
            stmt.execute(rusqlite::params![chunk.chunk_id.as_str(), embedding])?;
            regenerated_rows += 1;
        }
        drop(stmt);
        persist_vector_regeneration_event(
            &tx,
            "vector_regeneration_apply",
            &config.profile,
            &audit_metadata,
        )?;
        tx.commit()?;

        Ok(VectorRegenerationReport {
            profile: config.profile.clone(),
            table_name,
            dimension: identity.dimension,
            total_chunks: chunks.len(),
            regenerated_rows,
            contract_persisted: true,
            notes,
        })
    }

    pub(super) fn persist_vector_regeneration_failure_best_effort(
        &self,
        profile: &str,
        metadata: &VectorRegenerationAuditMetadata,
        failure: &VectorRegenerationFailure,
    ) {
        let Ok(conn) = self.connect() else {
            return;
        };
        let failure_metadata = VectorRegenerationAuditMetadata {
            profile: metadata.profile.clone(),
            model_identity: metadata.model_identity.clone(),
            model_version: metadata.model_version.clone(),
            chunk_count: metadata.chunk_count,
            snapshot_hash: metadata.snapshot_hash.clone(),
            failure_class: Some(failure.failure_class_label().to_owned()),
        };
        let _ = persist_vector_regeneration_event(
            &conn,
            "vector_regeneration_failed",
            profile,
            &failure_metadata,
        );
    }

    /// Configure per-kind vector indexing for `kind`, sourced from `source`.
    ///
    /// Requires at least one active row in `vector_embedding_profiles`. On
    /// first call, creates the `vec_<kind>` sqlite-vec table, inserts a
    /// `vector_index_schemas` row, and enqueues backfill rows in
    /// `vector_projection_work` (one per existing chunk of that kind).
    /// Subsequent calls are idempotent: no duplicate pending work rows are
    /// created for the (`chunk_id`, `embedding_profile_id`) pair.
    ///
    /// # Errors
    /// Returns [`EngineError::InvalidConfig`] if no active embedding profile
    /// exists; [`EngineError::Sqlite`]/[`EngineError::Schema`] on storage
    /// failures.
    pub fn configure_vec_kind(
        &self,
        kind: &str,
        source: VectorSource,
    ) -> Result<ConfigureVecOutcome, EngineError> {
        match source {
            VectorSource::Chunks => {}
        }
        let mut conn = self.connect()?;

        let profile: Option<(i64, i64)> = conn
            .query_row(
                "SELECT profile_id, dimensions FROM vector_embedding_profiles WHERE active = 1",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;
        let (profile_id, dimensions) = profile.ok_or_else(|| {
            EngineError::InvalidConfig(
                "no active embedding profile configured; call configure_embedding first".to_owned(),
            )
        })?;
        let dimensions = usize::try_from(dimensions).map_err(|_| {
            EngineError::Bridge(format!(
                "invalid embedding profile dimensions: {dimensions}"
            ))
        })?;

        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

        let was_already_enabled: bool = tx
            .query_row(
                "SELECT enabled FROM vector_index_schemas WHERE kind = ?1",
                rusqlite::params![kind],
                |row| row.get::<_, i64>(0).map(|v| v == 1),
            )
            .optional()?
            .unwrap_or(false);

        tx.execute(
            "INSERT INTO vector_index_schemas \
             (kind, enabled, source_mode, source_config_json, state, created_at, updated_at) \
             VALUES (?1, 1, 'chunks', NULL, 'fresh', unixepoch(), unixepoch()) \
             ON CONFLICT(kind) DO UPDATE SET \
                 enabled = 1, \
                 source_mode = 'chunks', \
                 source_config_json = NULL, \
                 updated_at = unixepoch()",
            rusqlite::params![kind],
        )?;

        self.schema_manager
            .ensure_vec_kind_profile(&tx, kind, dimensions)?;

        let chunks = collect_kind_chunks(&tx, kind)?;
        let mut enqueued: usize = 0;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO vector_projection_work \
                 (kind, node_logical_id, chunk_id, canonical_hash, priority, \
                  embedding_profile_id, state, created_at, updated_at) \
                 SELECT ?1, ?2, ?3, ?4, 0, ?5, 'pending', unixepoch(), unixepoch() \
                 WHERE NOT EXISTS ( \
                     SELECT 1 FROM vector_projection_work \
                     WHERE chunk_id = ?3 AND embedding_profile_id = ?5 AND state = 'pending' \
                 )",
            )?;
            for chunk in &chunks {
                let canonical_hash = canonical_chunk_hash(&chunk.chunk_id, &chunk.text_content);
                let inserted = stmt.execute(rusqlite::params![
                    kind,
                    chunk.node_logical_id.as_str(),
                    chunk.chunk_id.as_str(),
                    canonical_hash,
                    profile_id,
                ])?;
                enqueued += inserted;
            }
        }

        tx.commit()?;

        Ok(ConfigureVecOutcome {
            kind: kind.to_owned(),
            enqueued_backfill_rows: enqueued,
            was_already_enabled,
        })
    }

    /// Batch form of [`Self::configure_vec_kind`]. Loops over each
    /// `(kind, source)` in input order and returns one outcome per entry.
    ///
    /// Per-kind atomicity matches [`Self::configure_vec_kind`]: each call
    /// runs its own transaction. The batch as a whole is **not**
    /// atomic — if the third call fails, the first two remain committed.
    ///
    /// # Errors
    /// Returns the first [`EngineError`] encountered; already-committed
    /// entries remain committed.
    pub fn configure_vec_kinds(
        &self,
        items: &[(String, VectorSource)],
    ) -> Result<Vec<ConfigureVecOutcome>, EngineError> {
        let mut outcomes = Vec::with_capacity(items.len());
        for (kind, source) in items {
            outcomes.push(self.configure_vec_kind(kind, *source)?);
        }
        Ok(outcomes)
    }

    /// Return the managed vector indexing status for `kind`.
    ///
    /// If no `vector_index_schemas` row exists for `kind`, returns
    /// `enabled = false` and `state = "unconfigured"` with zero counts.
    ///
    /// # Errors
    /// Returns [`EngineError`] on database failures.
    pub fn get_vec_index_status(&self, kind: &str) -> Result<VecIndexStatus, EngineError> {
        let conn = self.connect()?;

        let schema_row: Option<(bool, String, Option<String>, Option<i64>)> = conn
            .query_row(
                "SELECT enabled, state, last_error, last_completed_at \
                 FROM vector_index_schemas WHERE kind = ?1",
                rusqlite::params![kind],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)? == 1,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                    ))
                },
            )
            .optional()?;

        let Some((enabled, state, last_error, last_completed_at)) = schema_row else {
            return Ok(VecIndexStatus {
                kind: kind.to_owned(),
                enabled: false,
                state: "unconfigured".to_owned(),
                pending_incremental: 0,
                pending_backfill: 0,
                last_error: None,
                last_completed_at: None,
                embedding_identity: None,
            });
        };

        let pending_backfill: u64 = conn
            .query_row(
                "SELECT count(*) FROM vector_projection_work \
                 WHERE kind = ?1 AND state = 'pending' AND priority < 1000",
                rusqlite::params![kind],
                |row| row.get::<_, i64>(0),
            )
            .map(i64::cast_unsigned)?;

        let pending_incremental: u64 = conn
            .query_row(
                "SELECT count(*) FROM vector_projection_work \
                 WHERE kind = ?1 AND state = 'pending' AND priority >= 1000",
                rusqlite::params![kind],
                |row| row.get::<_, i64>(0),
            )
            .map(i64::cast_unsigned)?;

        let embedding_identity: Option<String> = conn
            .query_row(
                "SELECT model_identity FROM vector_embedding_profiles WHERE active = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        Ok(VecIndexStatus {
            kind: kind.to_owned(),
            enabled,
            state,
            pending_incremental,
            pending_backfill,
            last_error,
            last_completed_at,
            embedding_identity,
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if the database connection fails or the projection rebuild fails.
    pub fn rebuild_projections(
        &self,
        target: ProjectionTarget,
    ) -> Result<ProjectionRepairReport, EngineError> {
        self.projections.rebuild_projections(target)
    }

    /// # Errors
    /// Returns [`EngineError`] if the database connection fails or the projection rebuild fails.
    pub fn rebuild_missing_projections(&self) -> Result<ProjectionRepairReport, EngineError> {
        self.projections.rebuild_missing_projections()
    }

    /// Activate, replace, or confirm the database-wide embedding identity.
    ///
    /// Vector identity belongs to the embedder: the `model_identity`,
    /// `model_version`, `dimensions`, and `normalization_policy` persisted in
    /// `vector_embedding_profiles` are read directly from
    /// `embedder.identity()`. Callers cannot supply an identity string.
    ///
    /// Semantics:
    /// - If no active profile exists: insert a new active row.
    ///   Returns [`ConfigureEmbeddingOutcome::Activated`].
    /// - If an active profile exists and the identity matches exactly: no-op.
    ///   Returns [`ConfigureEmbeddingOutcome::Unchanged`].
    /// - If an active profile exists and the identity differs:
    ///   * If any `vector_index_schemas.enabled = 1` rows exist and
    ///     `acknowledge_rebuild_impact = false`: return
    ///     [`EngineError::EmbeddingChangeRequiresAck`] without mutating state.
    ///   * Otherwise, within a single transaction: demote the current active
    ///     profile, insert the new active profile, and mark every enabled
    ///     vector index schema `state = 'stale'`. Returns
    ///     [`ConfigureEmbeddingOutcome::Replaced`].
    ///
    /// This method never triggers a rebuild itself. Affected kinds are marked
    /// `stale` so later rebuild flows can pick them up.
    ///
    /// # Errors
    /// - [`EngineError::EmbeddingChangeRequiresAck`] if the identity change
    ///   would invalidate enabled vector index kinds and the caller did not
    ///   acknowledge the rebuild impact.
    /// - [`EngineError::Sqlite`] if any underlying SQL fails.
    #[allow(clippy::too_many_lines)]
    pub fn configure_embedding(
        &self,
        embedder: &dyn QueryEmbedder,
        acknowledge_rebuild_impact: bool,
    ) -> Result<ConfigureEmbeddingOutcome, EngineError> {
        let identity = embedder.identity();
        let max_tokens = embedder.max_tokens();
        let dimensions = i64::try_from(identity.dimension).map_err(|_| {
            EngineError::InvalidConfig(format!(
                "embedder dimension {} exceeds i64 range",
                identity.dimension
            ))
        })?;
        let max_tokens_i64 = i64::try_from(max_tokens).ok();

        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

        // Look up the current active profile, if any.
        let current: Option<(i64, String, String, i64, String)> = tx
            .query_row(
                "SELECT profile_id, model_identity, COALESCE(model_version, ''), dimensions, \
                        COALESCE(normalization_policy, '') \
                 FROM vector_embedding_profiles WHERE active = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()?;

        let incoming_version = identity.model_version.clone();
        if let Some((profile_id, current_identity, current_version, current_dim, current_norm)) =
            current.clone()
        {
            if current_identity == identity.model_identity
                && current_version == incoming_version
                && current_dim == dimensions
                && current_norm == identity.normalization_policy
            {
                // Identical — no-op.
                tx.commit()?;
                return Ok(ConfigureEmbeddingOutcome::Unchanged { profile_id });
            }

            // Identity differs: count enabled kinds.
            let affected_kinds: i64 = tx.query_row(
                "SELECT COUNT(*) FROM vector_index_schemas WHERE enabled = 1",
                [],
                |row| row.get::<_, i64>(0),
            )?;
            let affected = usize::try_from(affected_kinds).unwrap_or(0);
            if affected > 0 && !acknowledge_rebuild_impact {
                // No mutation — drop the transaction.
                drop(tx);
                return Err(EngineError::EmbeddingChangeRequiresAck {
                    affected_kinds: affected,
                });
            }

            let identity_triple_changed = current_identity != identity.model_identity
                || current_version != incoming_version
                || current_dim != dimensions;

            let new_profile_id = if identity_triple_changed {
                // Demote current active row.
                tx.execute(
                    "UPDATE vector_embedding_profiles SET active = 0 WHERE active = 1",
                    [],
                )?;

                // Insert new active row.
                insert_new_active_profile(
                    &tx,
                    &identity.model_identity,
                    &incoming_version,
                    dimensions,
                    &identity.normalization_policy,
                    max_tokens_i64,
                )?
            } else {
                // Normalization-only change: the unique index on
                // (model_identity, model_version, dimensions) prevents inserting
                // a second row with the same triple, so update in place. The
                // profile row still reflects the new policy and enabled kinds
                // still get staled so rebuilds pick up the change.
                let normalization_opt: Option<&str> = if identity.normalization_policy.is_empty() {
                    None
                } else {
                    Some(identity.normalization_policy.as_str())
                };
                tx.execute(
                    "UPDATE vector_embedding_profiles \
                     SET normalization_policy = ?1, max_tokens = ?2 \
                     WHERE profile_id = ?3",
                    rusqlite::params![normalization_opt, max_tokens_i64, profile_id],
                )?;
                profile_id
            };

            // Mark enabled kinds stale.
            let stale_kinds = if affected > 0 {
                tx.execute(
                    "UPDATE vector_index_schemas \
                     SET state = 'stale', updated_at = unixepoch() \
                     WHERE enabled = 1",
                    [],
                )?
            } else {
                0
            };

            tx.commit()?;
            return Ok(ConfigureEmbeddingOutcome::Replaced {
                old_profile_id: profile_id,
                new_profile_id,
                stale_kinds,
            });
        }

        // No active profile: activate a new one.
        let new_profile_id = insert_new_active_profile(
            &tx,
            &identity.model_identity,
            &incoming_version,
            dimensions,
            &identity.normalization_policy,
            max_tokens_i64,
        )?;
        tx.commit()?;
        Ok(ConfigureEmbeddingOutcome::Activated {
            profile_id: new_profile_id,
        })
    }

    /// Return the active `vector_embedding_profiles.profile_id`, or `None`
    /// if no active profile is configured.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the database read fails.
    pub fn active_embedding_profile_id(&self) -> Result<Option<i64>, EngineError> {
        let conn = self.connect()?;
        let id = conn
            .query_row(
                "SELECT profile_id FROM vector_embedding_profiles WHERE active = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        Ok(id)
    }

    /// Run projection-worker scheduling ticks until no more work remains or
    /// `timeout` elapses.  Intended for tests and for admin-driven catch-up
    /// flows.
    ///
    /// # Errors
    /// Returns [`EngineError`] on writer or database failures, or
    /// [`EngineError::InvalidConfig`] if the admin service was constructed
    /// without a writer handle.
    pub fn drain_vector_projection(
        &self,
        embedder: &dyn BatchEmbedder,
        timeout: std::time::Duration,
    ) -> Result<crate::vector_projection_actor::DrainReport, EngineError> {
        let deadline = std::time::Instant::now() + timeout;
        let mut report = crate::vector_projection_actor::DrainReport::default();
        let writer = self.require_writer()?;
        loop {
            if std::time::Instant::now() >= deadline {
                break;
            }
            let tick = crate::vector_projection_actor::run_tick(self, &writer, embedder)?;
            if tick.embedder_unavailable {
                report.embedder_unavailable_ticks += 1;
                break;
            }
            report.incremental_processed += tick.processed_incremental;
            report.backfill_processed += tick.processed_backfill;
            report.failed += tick.failed;
            report.discarded_stale += tick.discarded_stale;
            if tick.idle {
                break;
            }
        }
        Ok(report)
    }

    /// Run exactly one projection scheduling tick; used by tests that need
    /// to assert priority ordering.
    ///
    /// # Errors
    /// Returns [`EngineError`] on writer or database failures.
    pub fn drain_vector_projection_single_tick(
        &self,
        embedder: &dyn BatchEmbedder,
    ) -> Result<crate::vector_projection_actor::DrainReport, EngineError> {
        let writer = self.require_writer()?;
        let tick = crate::vector_projection_actor::run_tick(self, &writer, embedder)?;
        let mut report = crate::vector_projection_actor::DrainReport::default();
        if tick.embedder_unavailable {
            report.embedder_unavailable_ticks = 1;
        }
        report.incremental_processed = tick.processed_incremental;
        report.backfill_processed = tick.processed_backfill;
        report.failed = tick.failed;
        report.discarded_stale = tick.discarded_stale;
        Ok(report)
    }

    fn require_writer(&self) -> Result<std::sync::Arc<crate::WriterActor>, EngineError> {
        self.writer.clone().ok_or_else(|| {
            EngineError::InvalidConfig(
                "drain_vector_projection requires an engine-wired AdminService".to_owned(),
            )
        })
    }

    /// Probe the supplied embedder by attempting a fixed short embed call.
    ///
    /// Used as an availability check for the active embedder. Does not touch
    /// persistent state.
    ///
    /// # Errors
    /// Returns [`EngineError::CapabilityMissing`] wrapping the embedder
    /// diagnostic if the embedder is unavailable or its call fails.
    pub fn check_embedding(&self, embedder: &dyn QueryEmbedder) -> Result<(), EngineError> {
        match embedder.embed_query("fathomdb embedder health probe") {
            Ok(_) => Ok(()),
            Err(err) => Err(EngineError::CapabilityMissing(format!(
                "embedder probe failed: {err}"
            ))),
        }
    }
}

/// Outcome of [`AdminService::configure_embedding`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ConfigureEmbeddingOutcome {
    /// No active embedding profile existed; a new one was inserted.
    Activated {
        /// Newly inserted `vector_embedding_profiles.profile_id`.
        profile_id: i64,
    },
    /// The requested identity matched the active profile exactly; nothing
    /// was changed.
    Unchanged {
        /// The existing active `vector_embedding_profiles.profile_id`.
        profile_id: i64,
    },
    /// The active profile was replaced and any enabled vector index
    /// schemas were marked `stale`.
    Replaced {
        /// The previously-active `profile_id` (now demoted).
        old_profile_id: i64,
        /// The newly-inserted active `profile_id`.
        new_profile_id: i64,
        /// Number of `vector_index_schemas` rows newly marked `stale`.
        stale_kinds: usize,
    },
}

fn insert_new_active_profile(
    tx: &rusqlite::Transaction<'_>,
    model_identity: &str,
    model_version: &str,
    dimensions: i64,
    normalization_policy: &str,
    max_tokens: Option<i64>,
) -> Result<i64, rusqlite::Error> {
    // `profile_name` is NOT NULL in the schema; derive it from identity so the
    // row is self-describing without inventing a user-facing name surface.
    let profile_name = format!("{model_identity}@{model_version}");
    let model_version_opt: Option<&str> = if model_version.is_empty() {
        None
    } else {
        Some(model_version)
    };
    let normalization_opt: Option<&str> = if normalization_policy.is_empty() {
        None
    } else {
        Some(normalization_policy)
    };
    tx.execute(
        "INSERT INTO vector_embedding_profiles \
            (profile_name, model_identity, model_version, dimensions, normalization_policy, \
             max_tokens, active, activated_at, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, unixepoch(), unixepoch())",
        rusqlite::params![
            profile_name,
            model_identity,
            model_version_opt,
            dimensions,
            normalization_opt,
            max_tokens,
        ],
    )?;
    Ok(tx.last_insert_rowid())
}

/// # Errors
/// Returns [`EngineError`] if the file cannot be read or the config is invalid.
pub fn load_vector_regeneration_config(
    path: impl AsRef<Path>,
) -> Result<VectorRegenerationConfig, EngineError> {
    let path = path.as_ref();
    let raw = std::fs::read_to_string(path)?;
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("toml") => {
            toml::from_str(&raw).map_err(|error| EngineError::Bridge(error.to_string()))
        }
        Some("json") | None => {
            serde_json::from_str(&raw).map_err(|error| EngineError::Bridge(error.to_string()))
        }
        Some(other) => Err(EngineError::Bridge(format!(
            "unsupported vector regeneration config extension: {other}"
        ))),
    }
}

fn validate_vector_regeneration_config(
    conn: &rusqlite::Connection,
    config: &VectorRegenerationConfig,
    identity: &QueryEmbedderIdentity,
) -> Result<VectorRegenerationConfig, VectorRegenerationFailure> {
    let kind = validate_bounded_text("kind", &config.kind, MAX_PROFILE_LEN)?;
    let profile = validate_bounded_text("profile", &config.profile, MAX_PROFILE_LEN)?;
    if identity.dimension == 0 {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            "embedder reports dimension 0".to_owned(),
        ));
    }
    let chunking_policy =
        validate_bounded_text("chunking_policy", &config.chunking_policy, MAX_POLICY_LEN)?;
    let preprocessing_policy = validate_bounded_text(
        "preprocessing_policy",
        &config.preprocessing_policy,
        MAX_POLICY_LEN,
    )?;

    if let Some(existing_dimension) = current_vector_profile_dimension(conn, &profile)?
        && existing_dimension != identity.dimension
    {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            format!(
                "embedder dimension {} does not match existing vector profile dimension {}",
                identity.dimension, existing_dimension
            ),
        ));
    }

    validate_existing_contract_version(conn, &profile)?;

    let normalized = VectorRegenerationConfig {
        kind,
        profile,
        chunking_policy,
        preprocessing_policy,
    };
    let serialized = serde_json::to_vec(&normalized).map_err(|error| {
        VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            error.to_string(),
        )
    })?;
    if serialized.len() > MAX_CONTRACT_JSON_BYTES {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            format!("serialized contract exceeds {MAX_CONTRACT_JSON_BYTES} bytes"),
        ));
    }

    Ok(normalized)
}

#[allow(clippy::cast_possible_wrap)]
fn persist_vector_contract(
    conn: &rusqlite::Connection,
    config: &VectorRegenerationConfig,
    table_name: &str,
    identity: &QueryEmbedderIdentity,
    snapshot_hash: &str,
) -> Result<(), EngineError> {
    conn.execute(
        r"
        INSERT OR REPLACE INTO vector_embedding_contracts (
            profile,
            table_name,
            model_identity,
            model_version,
            dimension,
            normalization_policy,
            chunking_policy,
            preprocessing_policy,
            generator_command_json,
            applied_at,
            snapshot_hash,
            contract_format_version,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, unixepoch(), ?10, ?11, unixepoch())
        ",
        rusqlite::params![
            config.profile.as_str(),
            table_name,
            identity.model_identity.as_str(),
            identity.model_version.as_str(),
            identity.dimension as i64,
            identity.normalization_policy.as_str(),
            config.chunking_policy.as_str(),
            config.preprocessing_policy.as_str(),
            "[]",
            snapshot_hash,
            CURRENT_VECTOR_CONTRACT_FORMAT_VERSION,
        ],
    )?;
    Ok(())
}

fn persist_vector_regeneration_event(
    conn: &rusqlite::Connection,
    event_type: &str,
    subject: &str,
    metadata: &VectorRegenerationAuditMetadata,
) -> Result<(), EngineError> {
    let metadata_json = serialize_audit_metadata(metadata)?;
    conn.execute(
        "INSERT INTO provenance_events (id, event_type, subject, metadata_json) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![new_id(), event_type, subject, metadata_json],
    )?;
    Ok(())
}

fn validate_bounded_text(
    field: &str,
    value: &str,
    max_len: usize,
) -> Result<String, VectorRegenerationFailure> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            format!("{field} must not be empty"),
        ));
    }
    if trimmed.len() > max_len {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            format!("{field} exceeds max length {max_len}"),
        ));
    }
    Ok(trimmed.to_owned())
}

fn current_vector_profile_dimension(
    conn: &rusqlite::Connection,
    profile: &str,
) -> Result<Option<usize>, VectorRegenerationFailure> {
    let dimension: Option<i64> = conn
        .query_row(
            "SELECT dimension FROM vector_profiles WHERE profile = ?1 AND enabled = 1",
            [profile],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| {
            VectorRegenerationFailure::new(
                VectorRegenerationFailureClass::InvalidContract,
                error.to_string(),
            )
        })?;
    dimension
        .map(|value| {
            usize::try_from(value).map_err(|_| {
                VectorRegenerationFailure::new(
                    VectorRegenerationFailureClass::InvalidContract,
                    format!("stored vector profile dimension is invalid: {value}"),
                )
            })
        })
        .transpose()
}

fn validate_existing_contract_version(
    conn: &rusqlite::Connection,
    profile: &str,
) -> Result<(), VectorRegenerationFailure> {
    let version: Option<i64> = conn
        .query_row(
            "SELECT contract_format_version FROM vector_embedding_contracts WHERE profile = ?1",
            [profile],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| {
            VectorRegenerationFailure::new(
                VectorRegenerationFailureClass::InvalidContract,
                error.to_string(),
            )
        })?;
    if let Some(version) = version
        && version > CURRENT_VECTOR_CONTRACT_FORMAT_VERSION
    {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            format!(
                "persisted contract format version {version} is unsupported; supported version is {CURRENT_VECTOR_CONTRACT_FORMAT_VERSION}"
            ),
        ));
    }
    Ok(())
}

fn serialize_audit_metadata(
    metadata: &VectorRegenerationAuditMetadata,
) -> Result<String, EngineError> {
    let json =
        serde_json::to_string(metadata).map_err(|error| EngineError::Bridge(error.to_string()))?;
    if json.len() > MAX_AUDIT_METADATA_BYTES {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            format!("audit metadata exceeds {MAX_AUDIT_METADATA_BYTES} bytes"),
        )
        .to_engine_error());
    }
    Ok(json)
}

pub(super) fn build_regeneration_input(
    config: &VectorRegenerationConfig,
    identity: &QueryEmbedderIdentity,
    chunks: Vec<VectorRegenerationInputChunk>,
) -> VectorRegenerationInput {
    VectorRegenerationInput {
        profile: config.profile.clone(),
        table_name: fathomdb_schema::vec_kind_table_name(&config.kind),
        model_identity: identity.model_identity.clone(),
        model_version: identity.model_version.clone(),
        dimension: identity.dimension,
        normalization_policy: identity.normalization_policy.clone(),
        chunking_policy: config.chunking_policy.clone(),
        preprocessing_policy: config.preprocessing_policy.clone(),
        chunks,
    }
}

pub(super) fn compute_snapshot_hash(
    payload: &VectorRegenerationInput,
) -> Result<String, EngineError> {
    let bytes =
        serde_json::to_vec(payload).map_err(|error| EngineError::Bridge(error.to_string()))?;
    let mut hasher = sha2::Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

/// Per-kind chunk enumeration used by [`AdminService::configure_vec_kind`].
///
/// Mirrors [`collect_regeneration_chunks`] but filters by `nodes.kind`.
pub(super) fn collect_kind_chunks(
    conn: &rusqlite::Connection,
    kind: &str,
) -> Result<Vec<VectorRegenerationInputChunk>, EngineError> {
    let mut stmt = conn.prepare(
        r"
        SELECT c.id, c.node_logical_id, n.kind, c.text_content, c.byte_start, c.byte_end, n.source_ref, c.created_at
        FROM chunks c
        JOIN nodes n
          ON n.logical_id = c.node_logical_id
         AND n.superseded_at IS NULL
        WHERE n.kind = ?1
        ORDER BY c.created_at, c.id
        ",
    )?;
    let chunks = stmt
        .query_map(rusqlite::params![kind], |row| {
            Ok(VectorRegenerationInputChunk {
                chunk_id: row.get(0)?,
                node_logical_id: row.get(1)?,
                kind: row.get(2)?,
                text_content: row.get(3)?,
                byte_start: row.get(4)?,
                byte_end: row.get(5)?,
                source_ref: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(chunks)
}

/// Canonical hash of a chunk's text content, scoped to the chunk id.
#[must_use]
pub(crate) fn canonical_chunk_hash(chunk_id: &str, text: &str) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(chunk_id.as_bytes());
    hasher.update([0u8]);
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(super) fn collect_regeneration_chunks(
    conn: &rusqlite::Connection,
) -> Result<Vec<VectorRegenerationInputChunk>, EngineError> {
    let mut stmt = conn.prepare(
        r"
        SELECT c.id, c.node_logical_id, n.kind, c.text_content, c.byte_start, c.byte_end, n.source_ref, c.created_at
        FROM chunks c
        JOIN nodes n
          ON n.logical_id = c.node_logical_id
         AND n.superseded_at IS NULL
        ORDER BY c.created_at, c.id
        ",
    )?;
    let chunks = stmt
        .query_map([], |row| {
            Ok(VectorRegenerationInputChunk {
                chunk_id: row.get(0)?,
                node_logical_id: row.get(1)?,
                kind: row.get(2)?,
                text_content: row.get(3)?,
                byte_start: row.get(4)?,
                byte_end: row.get(5)?,
                source_ref: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(chunks)
}
