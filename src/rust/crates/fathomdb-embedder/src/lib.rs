use std::path::PathBuf;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};

#[cfg(feature = "default-embedder")]
pub mod loader;

/// Structured event surfaced through `OpenReport.embedder_events`
/// (`dev/design/embedder.md` §7).
///
/// Defined unconditionally at the crate root so the engine can reference
/// it regardless of the `default-embedder` feature; the loader (under
/// `default-embedder`) emits these variants and re-exports the enum for
/// ergonomic in-module use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbedderEvent {
    /// A file was fetched from the network and written to the cache.
    DefaultEmbedderDownload {
        file: String,
        url: String,
        bytes: u64,
        sha256: String,
        cache_path: PathBuf,
        duration_ms: u64,
    },
    /// A file was found in the cache and verified by sha256. No network.
    DefaultEmbedderCacheHit { file: String, sha256: String, cache_path: PathBuf },
    /// EU-5a2 — emitted at the commit that materializes the per-workspace
    /// mean vector into `_fathomdb_embedder_profiles.mean_vec`. `dim`
    /// matches the default embedder identity's dimension; `doc_count` is
    /// the number of pre-pin rows the same transaction's re-quantize
    /// pass updated (per `dev/design/embedder.md` §0.5, §7).
    ///
    /// EU-5a2's only live identity is NoopEmbedder, which does NOT
    /// request mean-centering, so this event is dormant until EU-5b
    /// flips the default identity. Defined now so EU-5b is a no-op
    /// addition to this enum.
    MeanVecPinned { dim: u32, doc_count: u64 },
}

#[cfg(feature = "default-embedder")]
mod candle_bge;

#[cfg(feature = "default-embedder")]
pub use candle_bge::{CandleBgeEmbedder, DEFAULT_EMBEDDER_DIM, DEFAULT_EMBEDDER_NAME};

#[derive(Clone, Debug)]
pub struct NoopEmbedder {
    identity: EmbedderIdentity,
}

impl Default for NoopEmbedder {
    fn default() -> Self {
        Self { identity: EmbedderIdentity::new("fathomdb-noop", "0.6.0-scaffold", 384) }
    }
}

impl Embedder for NoopEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, _input: &str) -> Result<Vector, EmbedderError> {
        let mut vector = vec![0.0_f32; self.identity.dimension as usize];
        if let Some(first) = vector.first_mut() {
            *first = 1.0;
        }
        Ok(vector)
    }
}
