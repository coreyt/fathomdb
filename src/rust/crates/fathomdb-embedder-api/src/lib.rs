pub type Vector = Vec<f32>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbedderIdentity {
    pub name: String,
    pub revision: String,
}

impl EmbedderIdentity {
    #[must_use]
    pub fn new(name: impl Into<String>, revision: impl Into<String>) -> Self {
        Self { name: name.into(), revision: revision.into() }
    }
}

pub trait Embedder {
    fn identity(&self) -> &EmbedderIdentity;

    fn embed(&self, input: &str) -> Vector;
}
