//! `NomicEmbedder` — experimental embedder for `nomic-ai/nomic-embed-text-v1.5`
//! via `candle_transformers::models::nomic_bert`. Used for the IR-C model A/B
//! (`dev/notes/IR-C-embedder-options-research.md`); NOT the pinned production
//! default. 768-d, mean-pool + L2-norm (the nomic recipe). nomic REQUIRES task
//! prefixes — `"search_document: "` for passages, `"search_query: "` for queries
//! — which the CALLER prepends (this `embed()` embeds the text it is given).

use std::path::Path;

use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::nomic_bert::{l2_normalize, mean_pooling, Config, NomicBertModel};
use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use tokenizers::{Tokenizer, TruncationParams};

/// nomic-embed-text-v1.5 output dimension (full, pre-MRL-truncation).
pub const NOMIC_DIM: u32 = 768;

pub struct NomicEmbedder {
    identity: EmbedderIdentity,
    tokenizer: Tokenizer,
    model: NomicBertModel,
    device: Device,
}

impl NomicEmbedder {
    /// Load from a directory containing `tokenizer.json` + `model.safetensors`.
    /// The architecture is candle's `Config::default()`, which is exactly
    /// nomic-embed-text-v1.5 (768/12/12, 8192-ctx, RoPE, SwiGLU), so no
    /// `config.json` parsing is needed.
    pub fn from_dir(dir: &Path) -> Result<Self, EmbedderError> {
        let device = Device::Cpu;
        let mut tokenizer = Tokenizer::from_file(dir.join("tokenizer.json"))
            .map_err(|e| EmbedderError::Failed { message: format!("nomic tokenizer: {e}") })?;
        // Chunks are short; cap generously below the 8192 ceiling.
        let _ = tokenizer
            .with_truncation(Some(TruncationParams { max_length: 2048, ..Default::default() }));
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(
                &[dir.join("model.safetensors").as_path()],
                DType::F32,
                &device,
            )
        }
        .map_err(|e| EmbedderError::Failed { message: format!("nomic safetensors: {e}") })?;
        let model = NomicBertModel::load(vb, &Config::default())
            .map_err(|e| EmbedderError::Failed { message: format!("nomic model load: {e}") })?;
        let identity = EmbedderIdentity::new("nomic-embed-text-v1.5", "main", NOMIC_DIM);
        Ok(Self { identity, tokenizer, model, device })
    }
}

impl Embedder for NomicEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, input: &str) -> Result<Vector, EmbedderError> {
        let enc = self
            .tokenizer
            .encode(input, true)
            .map_err(|e| EmbedderError::Failed { message: format!("tokenize: {e}") })?;
        let ids: Vec<u32> = enc.get_ids().to_vec();
        let attn: Vec<u32> = enc.get_attention_mask().to_vec();
        let len = ids.len();

        let go = || -> candle_core::Result<Vec<f32>> {
            let input_ids = Tensor::from_vec(ids, (1, len), &self.device)?;
            let attn_t = Tensor::from_vec(attn, (1, len), &self.device)?;
            let token_type = input_ids.zeros_like()?;
            let hidden = self.model.forward(&input_ids, Some(&token_type), Some(&attn_t))?;
            let pooled = mean_pooling(&hidden, &attn_t)?; // (1, D)
            let normed = l2_normalize(&pooled)?;
            normed.squeeze(0)?.to_vec1::<f32>()
        };
        go().map_err(|e| EmbedderError::Failed { message: format!("forward: {e}") })
    }
}
