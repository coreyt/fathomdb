use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};

#[cfg(feature = "default-embedder")]
pub mod loader;

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
