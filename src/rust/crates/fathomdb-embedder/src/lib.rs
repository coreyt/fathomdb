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
