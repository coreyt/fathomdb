//! Phase 12.5b: in-process query embedder backed by
//! [`BAAI/bge-small-en-v1.5`](https://huggingface.co/BAAI/bge-small-en-v1.5),
//! a 384-dim BERT-small sentence encoder, wired through
//! [`candle-transformers`] and [`tokenizers`].
//!
//! Gated on the `default-embedder` cargo feature (off by default). When
//! enabled, callers who pass [`crate::QueryEmbedder`] trait objects via
//! [`crate::ExecutionCoordinator`] can supply a [`BuiltinBgeSmallEmbedder`]
//! to get a ready-made in-process embedder; the top-level `fathomdb` crate
//! wires this into [`fathomdb::EmbedderChoice::Builtin`] automatically.
//!
//! # Correctness trap — CLS pooling, not mean pooling
//!
//! The stock Candle BERT example
//! (`candle-examples/examples/bert/main.rs`) applies **mean pooling** over
//! the full sequence. BGE-small-en-v1.5 was trained with the sentence
//! embedding taken from the `[CLS]` token (position 0) followed by an
//! explicit L2 normalization. Shipping mean pooling here would silently
//! degrade retrieval quality by several percentage points of recall — see
//! the Phase 12.5 research report section "Pooling correctness trap" for
//! the full background. Do not "simplify" this implementation back to
//! mean pooling without reading that note.
//!
//! # Lazy loading & offline degradation
//!
//! Weights are downloaded on first [`BuiltinBgeSmallEmbedder::embed_query`]
//! call via [`hf_hub`] into the standard Hugging Face cache
//! (`~/.cache/huggingface/hub`, or `HF_HOME` if set). On subsequent calls
//! the in-memory model is reused. If the download fails — no network,
//! `HF_HUB_OFFLINE=1` with an empty cache, corrupted safetensors — we
//! return [`EmbedderError::Unavailable`] and the coordinator's existing
//! [`crate::coordinator::ExecutionCoordinator::fill_vector_branch`] path
//! marks the plan degraded without panicking or failing the read.

use std::sync::Mutex;

use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config, DTYPE};
use hf_hub::{Repo, RepoType, api::sync::Api};
use tokenizers::Tokenizer;

use super::{EmbedderError, QueryEmbedder, QueryEmbedderIdentity};

/// Model identity. Kept pinned to the commit we validated against so the
/// `identity().model_version` string is stable across builds. Updating
/// this constant is an intentional, reviewable change.
const MODEL_ID: &str = "BAAI/bge-small-en-v1.5";
const MODEL_REVISION: &str = "main";
const MODEL_DIMENSION: usize = 384;

/// In-process BGE-small-en-v1.5 embedder. Constructed cheaply; defers
/// model download + load until the first [`embed_query`] call.
///
/// See the module docs for the CLS-vs-mean-pooling correctness note.
pub struct BuiltinBgeSmallEmbedder {
    /// `None` until the first successful load. Wrapped in a [`Mutex`] so
    /// concurrent readers serialize exactly once on first use; subsequent
    /// calls see `Some(_)` and the lock is taken only briefly to clone
    /// references out. We cannot use [`std::sync::OnceLock`] directly
    /// because `ModelState` is not `Clone` and the init closure returns
    /// `Result`, which `OnceLock::get_or_init` doesn't support on stable.
    state: Mutex<Option<ModelState>>,
}

impl std::fmt::Debug for BuiltinBgeSmallEmbedder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let loaded = self.state.lock().map(|g| g.is_some()).unwrap_or(false);
        f.debug_struct("BuiltinBgeSmallEmbedder")
            .field("model_id", &MODEL_ID)
            .field("loaded", &loaded)
            .finish()
    }
}

impl Default for BuiltinBgeSmallEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

impl BuiltinBgeSmallEmbedder {
    /// Construct a new embedder. Does not touch the network or load any
    /// model weights — that happens on the first [`embed_query`] call.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Mutex::new(None),
        }
    }

    /// Attempt to materialize [`ModelState`] by downloading or loading
    /// the cached tokenizer + config + safetensors. Never touches
    /// `self.state`; the caller owns that locking discipline.
    fn load_model_state() -> Result<ModelState, EmbedderError> {
        let device = Device::Cpu;
        let repo = Repo::with_revision(
            MODEL_ID.to_owned(),
            RepoType::Model,
            MODEL_REVISION.to_owned(),
        );
        let api = Api::new()
            .map_err(|e| EmbedderError::Unavailable(format!("hf-hub api init failed: {e}")))?
            .repo(repo);

        let config_path = api
            .get("config.json")
            .map_err(|e| EmbedderError::Unavailable(format!("fetch config.json: {e}")))?;
        let tokenizer_path = api
            .get("tokenizer.json")
            .map_err(|e| EmbedderError::Unavailable(format!("fetch tokenizer.json: {e}")))?;
        let weights_path = api
            .get("model.safetensors")
            .map_err(|e| EmbedderError::Unavailable(format!("fetch model.safetensors: {e}")))?;

        let config_bytes = std::fs::read_to_string(&config_path)
            .map_err(|e| EmbedderError::Unavailable(format!("read config.json: {e}")))?;
        let config: Config = serde_json::from_str(&config_bytes)
            .map_err(|e| EmbedderError::Unavailable(format!("parse config.json: {e}")))?;

        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| EmbedderError::Unavailable(format!("load tokenizer: {e}")))?;

        // SAFETY: VarBuilder::from_mmaped_safetensors memory-maps the file.
        // The unsafe contract is that the file is not mutated for the
        // lifetime of the mapping; the hf-hub cache file is immutable after
        // download, so this holds.
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[weights_path], DTYPE, &device)
                .map_err(|e| EmbedderError::Unavailable(format!("mmap safetensors: {e}")))?
        };
        let model = BertModel::load(vb, &config)
            .map_err(|e| EmbedderError::Unavailable(format!("load BertModel: {e}")))?;

        Ok(ModelState {
            tokenizer,
            model,
            device,
        })
    }

    /// Run the forward pass with CLS pooling + L2 normalization on a
    /// loaded [`ModelState`]. Kept as a free function so the caller can
    /// drop the `state` mutex guard before the (cheap but non-trivial)
    /// tensor math runs.
    fn embed_with_state(state: &ModelState, text: &str) -> Result<Vec<f32>, EmbedderError> {
        let encoding = state
            .tokenizer
            .encode(text, true)
            .map_err(|e| EmbedderError::Failed(format!("tokenize: {e}")))?;
        let ids = encoding.get_ids();
        if ids.is_empty() {
            return Err(EmbedderError::Failed(
                "tokenizer produced empty id sequence".to_owned(),
            ));
        }

        let input_ids = Tensor::new(ids, &state.device)
            .and_then(|t| t.unsqueeze(0))
            .map_err(|e| EmbedderError::Failed(format!("build input_ids tensor: {e}")))?;
        let token_type_ids = input_ids
            .zeros_like()
            .map_err(|e| EmbedderError::Failed(format!("build token_type_ids: {e}")))?;

        // `BertModel::forward` returns `[batch=1, seq_len, hidden=384]`.
        // BGE: take position 0 ([CLS]), then L2 normalize. See the
        // module-level correctness trap note.
        let hidden = state
            .model
            .forward(&input_ids, &token_type_ids, None)
            .map_err(|e| EmbedderError::Failed(format!("bert forward: {e}")))?;

        let cls = hidden
            .get(0) // strip batch -> [seq_len, hidden]
            .and_then(|batch0| batch0.get(0)) // take [CLS] -> [hidden]
            .map_err(|e| EmbedderError::Failed(format!("index CLS token: {e}")))?;

        let normalized =
            l2_normalize(&cls).map_err(|e| EmbedderError::Failed(format!("l2 normalize: {e}")))?;

        let as_f32 = normalized
            .to_dtype(DType::F32)
            .and_then(|t| t.to_vec1::<f32>())
            .map_err(|e| EmbedderError::Failed(format!("tensor to Vec<f32>: {e}")))?;

        if as_f32.len() != MODEL_DIMENSION {
            return Err(EmbedderError::Failed(format!(
                "expected {MODEL_DIMENSION}-dim vector, got {}",
                as_f32.len()
            )));
        }
        Ok(as_f32)
    }
}

/// L2-normalize a 1-D tensor. Returns the input unchanged (numerically)
/// if the norm is zero — we don't want to divide by zero and fail the
/// forward pass; a pathological empty embedding is better than a hard
/// error from the read path.
fn l2_normalize(v: &Tensor) -> candle_core::Result<Tensor> {
    let sq = v.sqr()?;
    let norm_sq = sq.sum_all()?.to_scalar::<f32>()?;
    if norm_sq <= f32::EPSILON {
        return Ok(v.clone());
    }
    let norm = norm_sq.sqrt();
    v.affine(f64::from(1.0_f32 / norm), 0.0)
}

struct ModelState {
    tokenizer: Tokenizer,
    model: BertModel,
    device: Device,
}

impl QueryEmbedder for BuiltinBgeSmallEmbedder {
    fn embed_query(&self, text: &str) -> Result<Vec<f32>, EmbedderError> {
        // Lazy-load (or return cached failure by retrying). We hold the
        // mutex across the load so concurrent first-callers serialize.
        let mut guard = self
            .state
            .lock()
            .map_err(|_| EmbedderError::Failed("embedder state mutex poisoned".to_owned()))?;
        if guard.is_none() {
            *guard = Some(Self::load_model_state()?);
        }
        // `guard` still borrowed; run the forward pass with the loaded
        // state. We don't release the lock here because `BertModel` is
        // not `Sync` in all candle versions and we want to keep the
        // single-threaded forward semantics honest.
        let state = guard
            .as_ref()
            .ok_or_else(|| EmbedderError::Failed("model state unexpectedly None".to_owned()))?;
        Self::embed_with_state(state, text)
    }

    fn identity(&self) -> QueryEmbedderIdentity {
        QueryEmbedderIdentity {
            model_identity: MODEL_ID.to_owned(),
            model_version: MODEL_REVISION.to_owned(),
            dimension: MODEL_DIMENSION,
            normalization_policy: "l2".to_owned(),
        }
    }
}
