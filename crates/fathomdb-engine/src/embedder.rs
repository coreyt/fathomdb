//! Read-time query embedder trait and identity types.
//!
//! Phase 12.5a defines the always-on scaffolding that Phase 12.5b (the
//! Candle + bge-small-en-v1.5 default implementation) plugs into behind the
//! `default-embedder` feature flag. The trait lives in `fathomdb-engine`
//! rather than `fathomdb-query` so that `fathomdb-query` stays a pure
//! AST-to-plan compiler with no dyn trait objects or runtime state.
//!
//! The coordinator owns an `Option<Arc<dyn QueryEmbedder>>`. When present,
//! `ExecutionCoordinator::fill_vector_branch` invokes `embed_query` on the
//! raw natural-language query, serializes the returned `Vec<f32>` via
//! `serde_json` into the JSON float-array literal that
//! `CompiledVectorSearch::query_text` already expects, and drops a fully
//! constructed `CompiledVectorSearch` into `plan.vector`. When absent, the
//! plan's vector slot stays `None` and the Phase 12 v1 dormancy invariant
//! on `search()` is preserved unchanged.

use thiserror::Error;

/// A read-time query embedder.
///
/// Implementations must be `Send + Sync` so the coordinator can share a
/// single `Arc<dyn QueryEmbedder>` across reader threads without cloning
/// per call. All methods are `&self` — embedders are expected to be
/// internally immutable or to manage their own interior mutability.
pub trait QueryEmbedder: Send + Sync + std::fmt::Debug {
    /// Embed a single query string into a dense vector.
    ///
    /// # Errors
    /// Returns [`EmbedderError::Unavailable`] if the embedder cannot
    /// produce a vector right now (e.g. the model weights failed to load
    /// under a feature-flag stub), or [`EmbedderError::Failed`] if the
    /// embedding pipeline itself errored. The coordinator treats either
    /// variant as a graceful degradation, NOT a hard query failure.
    fn embed_query(&self, text: &str) -> Result<Vec<f32>, EmbedderError>;

    /// Model identity / version / dimension / normalization identity.
    ///
    /// Must match the write-time contract for the corresponding vec table.
    /// Phase 12.5a does not yet enforce the match at runtime; Phase 12.5b
    /// will gate the vector branch on `identity()` equality with the
    /// active vector profile.
    fn identity(&self) -> QueryEmbedderIdentity;
}

/// Identity metadata for a [`QueryEmbedder`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryEmbedderIdentity {
    /// Stable model identifier (e.g. `"bge-small-en-v1.5"`).
    pub model_identity: String,
    /// Model version (e.g. `"1.5"`).
    pub model_version: String,
    /// Output dimension. Must match the active vector profile's dimension
    /// or the vector branch will never fire.
    pub dimension: usize,
    /// Normalization policy identifier (e.g. `"l2"`, `"none"`).
    pub normalization_policy: String,
}

/// Errors reported by a [`QueryEmbedder`].
///
/// Both variants are treated as capability misses by the coordinator:
/// `plan.was_degraded_at_plan_time` is set and the vector branch is
/// skipped, but the rest of the search pipeline proceeds normally.
#[derive(Debug, Error)]
pub enum EmbedderError {
    /// The embedder is not available at all (e.g. the default-embedder
    /// feature flag is disabled, or the model weights failed to load).
    #[error("embedder unavailable: {0}")]
    Unavailable(String),
    /// The embedder is present but failed to embed this particular query.
    #[error("embedding failed: {0}")]
    Failed(String),
}
