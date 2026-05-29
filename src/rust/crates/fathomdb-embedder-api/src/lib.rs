pub type Vector = Vec<f32>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbedderIdentity {
    pub name: String,
    pub revision: String,
    pub dimension: u32,
}

impl EmbedderIdentity {
    #[must_use]
    pub fn new(name: impl Into<String>, revision: impl Into<String>, dimension: u32) -> Self {
        Self { name: name.into(), revision: revision.into(), dimension }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EmbedderError {
    Failed { message: String },
    Timeout,
}

pub trait Embedder: Send + Sync {
    fn identity(&self) -> EmbedderIdentity;

    fn embed(&self, input: &str) -> Result<Vector, EmbedderError>;
}
