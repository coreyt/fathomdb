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
        let current: Option<(i64, String, String, i64)> = tx
            .query_row(
                "SELECT profile_id, model_identity, COALESCE(model_version, ''), dimensions \
                 FROM vector_embedding_profiles WHERE active = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?;

        let incoming_version = identity.model_version.clone();
        if let Some((profile_id, current_identity, current_version, current_dim)) = current.clone()
        {
            if current_identity == identity.model_identity
                && current_version == incoming_version
                && current_dim == dimensions
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

            // Demote current active row.
            tx.execute(
                "UPDATE vector_embedding_profiles SET active = 0 WHERE active = 1",
                [],
            )?;

            // Insert new active row.
            let new_profile_id = insert_new_active_profile(
                &tx,
                &identity.model_identity,
                &incoming_version,
                dimensions,
                &identity.normalization_policy,
                max_tokens_i64,
            )?;

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
