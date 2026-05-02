use fathomdb_embedder_api::{Embedder, EmbedderIdentity, Vector};

#[derive(Clone, Debug)]
pub struct NoopEmbedder {
    identity: EmbedderIdentity,
}

impl Default for NoopEmbedder {
    fn default() -> Self {
        Self { identity: EmbedderIdentity::new("fathomdb-noop", "0.6.0-scaffold") }
    }
}

impl Embedder for NoopEmbedder {
    fn identity(&self) -> &EmbedderIdentity {
        &self.identity
    }

    fn embed(&self, input: &str) -> Vector {
        vec![input.len() as f32]
    }
}
