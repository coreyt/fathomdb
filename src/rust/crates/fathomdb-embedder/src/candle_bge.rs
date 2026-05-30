//! `CandleBgeEmbedder` — the default embedder shipped with fathomdb.
//!
//! Wraps `candle_transformers::models::bert::BertModel` loaded with the
//! pinned `BAAI/bge-small-en-v1.5` weights (revision
//! `5c38ec7c405ec4b44b94cc5a9bb96e735b38267a`, dim 384) and implements
//! the `fathomdb_embedder_api::Embedder` trait.
//!
//! Contract sources:
//! - `dev/design/embedder.md` §0.4 — mean-pool over attention mask, then
//!   L2-normalize; store un-centered (centering is the engine's concern).
//! - `dev/design/embedder.md` §1 — pinned identity.
//! - `dev/design/embedder.md` §7 — embedder does NOT append to
//!   `embedder_events`; that is the engine's job.
//! - `dev/design/embedder.md` §8 — little-endian invariant; enforced as
//!   a `compile_error!` at the top of this module so BE builds fail at
//!   `cargo build` rather than at runtime.
//! - `ADR-0.6.0-embedder-protocol.md` Invariants 1–4 — unit-norm output,
//!   no re-entrancy into the engine, no log/tracing/println in `embed()`,
//!   trait method is synchronous.

// Design §8: safetensors weight format is little-endian; pinned
// workspace targets (x86_64, aarch64) are LE on all supported OSes.
// BE platforms are explicitly out of scope for 0.7.1 — they would need
// an additional byte-swap pass during weight load. Enforce at compile
// time so a BE build fails fast rather than silently producing garbage
// f32 vectors.
#[cfg(target_endian = "big")]
compile_error!("fathomdb-embedder default path requires a little-endian target");

use std::path::Path;

use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use tokenizers::{Tokenizer, TruncationParams};

use crate::loader::{load_pinned_default_embedder, EmbedderLoadError, LoadedWeights, HF_REVISION};

/// Engine-facing identity name (per
/// `dev/plans/prompts/0.7.1-EMBEDDER-UNDEFER-HANDOFF.md` §0.5). EU-5 will
/// reconcile this with `fathomdb-engine::default_embedder_identity()` so
/// the engine stops returning the noop name.
pub const DEFAULT_EMBEDDER_NAME: &str = "fathomdb-bge-small-en-v1.5";

/// Output dimension for `bge-small-en-v1.5`. Must match the hidden_size
/// field of the pinned `config.json`; `new()` enforces this at runtime.
pub const DEFAULT_EMBEDDER_DIM: u32 = 384;

/// Maximum sequence length (in tokens, INCLUDING the `[CLS]`/`[SEP]`
/// specials) the tokenizer truncates to. BGE-small's learned position
/// embeddings have exactly 512 slots (`config.json` `max_position_
/// embeddings`); feeding a longer sequence trips an out-of-bounds
/// index-select inside `BertModel::forward`. Per
/// `dev/design/embedder-decision.md` §2.1 the tokenizer runs with
/// `truncation True` / max 512.
const MAX_SEQUENCE_TOKENS: usize = 512;

/// Default embedder backed by `candle-transformers` BERT.
pub struct CandleBgeEmbedder {
    identity: EmbedderIdentity,
    tokenizer: Tokenizer,
    model: BertModel,
    device: Device,
}

impl CandleBgeEmbedder {
    /// Fetch (or read from cache) the pinned weights and construct a ready
    /// embedder. First call on a cold cache downloads ~135 MB from
    /// huggingface.co; subsequent calls are local-IO.
    ///
    /// Per `dev/design/embedder.md` §8 this constructor asserts the host is
    /// little-endian (safetensors layout assumption).
    pub fn new() -> Result<Self, EmbedderLoadError> {
        let weights = load_pinned_default_embedder()?;
        Self::new_from_weights(weights)
    }

    /// EU-5b — construct from already-fetched weights. The engine path
    /// invokes the loader directly (so it can splice the loader's
    /// `bytes_downloaded` + `events` into `OpenReport`), then hands the
    /// `LoadedWeights` to this constructor.
    pub fn new_from_weights(weights: LoadedWeights) -> Result<Self, EmbedderLoadError> {
        // Design §8: safetensors are little-endian; we never run on BE.
        // Tightened in EU-5d follow-up from a debug_assert into a
        // compile-time error so BE builds fail at `cargo build` rather
        // than panicking at runtime in debug mode.

        // 1. Parse config.json.
        let config_bytes = std::fs::read(&weights.config_json_path).map_err(|source| {
            EmbedderLoadError::CacheIoError { path: weights.config_json_path.clone(), source }
        })?;
        let config: BertConfig =
            serde_json::from_slice(&config_bytes).map_err(|e| EmbedderLoadError::CacheIoError {
                path: weights.config_json_path.clone(),
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()),
            })?;

        // Runtime dim sanity (codex-checklist item): the pinned shape must
        // match our public dim constant. If a future revision bump changes
        // hidden_size, fail loudly here.
        if config.hidden_size != DEFAULT_EMBEDDER_DIM as usize {
            // Distinct from `ModelDeserialize`: the bytes parsed cleanly
            // but the pinned model's `hidden_size` disagrees with our
            // compile-time `DEFAULT_EMBEDDER_DIM`. This always points at
            // a deliberate model/version drift — see design §9 row
            // "DimensionMismatch".
            return Err(EmbedderLoadError::DimensionMismatch {
                expected: DEFAULT_EMBEDDER_DIM,
                actual: config.hidden_size as u32,
            });
        }

        // 2. Load tokenizer.json and pin truncation to the model's 512-slot
        // position-embedding window (design §2.1). `tokenizer.json` does not
        // always carry a truncation policy, so set it programmatically; the
        // tokenizer truncates AND re-applies `[SEP]` correctly (a raw token
        // truncate would drop the trailing special token). Without this a
        // >512-token document errors with "index-select invalid index 512".
        let mut tokenizer = Tokenizer::from_file(&weights.tokenizer_json_path)
            .map_err(|e| EmbedderLoadError::TokenizerLoad { source: e })?;
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: MAX_SEQUENCE_TOKENS,
                ..Default::default()
            }))
            .map_err(|e| EmbedderLoadError::TokenizerLoad { source: e })?;

        // 3. mmap safetensors and build a BertModel via VarBuilder.
        let device = Device::Cpu;
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(
                &[weights.model_safetensors_path.as_path() as &Path],
                DType::F32,
                &device,
            )
        }
        .map_err(|source| EmbedderLoadError::ModelDeserialize { source })?;

        let model = BertModel::load(vb, &config)
            .map_err(|source| EmbedderLoadError::ModelDeserialize { source })?;

        let identity =
            EmbedderIdentity::new(DEFAULT_EMBEDDER_NAME, HF_REVISION, DEFAULT_EMBEDDER_DIM);

        Ok(Self { identity, tokenizer, model, device })
    }
}

impl Embedder for CandleBgeEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, input: &str) -> Result<Vector, EmbedderError> {
        // Invariant 3 (ADR-0.6.0-embedder-protocol.md): no log/tracing/
        // println/eprintln/dbg in this function. Errors travel up via
        // the return value only.

        let encoding = self
            .tokenizer
            .encode(input, true)
            .map_err(|e| EmbedderError::Failed { message: format!("tokenize: {e}") })?;

        let ids: Vec<u32> = encoding.get_ids().to_vec();
        let attn: Vec<u32> = encoding.get_attention_mask().to_vec();
        let len = ids.len();

        let embed_impl = || -> candle_core::Result<Vec<f32>> {
            let input_ids = Tensor::from_vec(ids, (1, len), &self.device)?;
            let attn_mask_u32 = Tensor::from_vec(attn, (1, len), &self.device)?;
            let token_type_ids = input_ids.zeros_like()?;

            // BertModel::forward takes the raw (B, L) attention mask and
            // internally builds the additive mask. (B, L, D) f32 out.
            let hidden = self.model.forward(&input_ids, &token_type_ids, Some(&attn_mask_u32))?;

            // Mean-pool over the attention mask (design §0.4). Pad
            // positions contribute zero; we divide by the count of
            // non-pad tokens, not by L.
            let mask_f = attn_mask_u32.to_dtype(DType::F32)?.unsqueeze(2)?; // (1, L, 1)
            let mask_f = mask_f.broadcast_as(hidden.shape())?; // (1, L, D)
            let summed = (hidden * &mask_f)?.sum(1)?; // (1, D)
            let counts = mask_f.sum(1)?.clamp(1e-9_f32, f32::INFINITY)?; // (1, D)
            let pooled = (summed / counts)?; // (1, D)

            // L2-normalize (design §0.4).
            let norm = pooled.sqr()?.sum_keepdim(1)?.sqrt()?; // (1, 1)
            let norm = norm.clamp(1e-12_f32, f32::INFINITY)?;
            let normed = pooled.broadcast_div(&norm)?; // (1, D)

            let v: Vec<f32> = normed.squeeze(0)?.to_vec1::<f32>()?;
            Ok(v)
        };

        embed_impl().map_err(|e| EmbedderError::Failed { message: format!("forward: {e}") })
    }
}
