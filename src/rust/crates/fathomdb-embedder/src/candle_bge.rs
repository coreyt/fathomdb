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

/// Sentence-vector pooling strategy. `bge-small-en-v1.5` ships
/// `1_Pooling/config.json` with `pooling_mode_cls_token: true`, i.e. it is a
/// CLS-pooled model; BGE's docs warn that mean-pooling causes "a significant
/// decrease in performance". `Mean` is the historical default (design §0.4);
/// `Cls` is the model-correct mode, under evaluation behind the 1-bit binary
/// recall-floor gate (it changes the embedding-space geometry that sign-bit
/// quantization is sensitive to). See `dev/notes/IR-C-embedder-options-research.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pooling {
    /// Mean over the attention mask (historical default; design §0.4).
    Mean,
    /// `[CLS]` token (position 0) — the mode bge-small was trained for.
    Cls,
}

/// L2-normalize a `(1, D)` pooled tensor.
fn l2_normalize(pooled: &Tensor) -> candle_core::Result<Tensor> {
    let norm = pooled.sqr()?.sum_keepdim(1)?.sqrt()?;
    let norm = norm.clamp(1e-12_f32, f32::INFINITY)?;
    pooled.broadcast_div(&norm)
}

/// Mean-pool `(1, L, D)` hidden states over the attention mask → `(1, D)`. Pad
/// positions contribute zero; divide by the non-pad token count, not by L.
fn mean_pool(hidden: &Tensor, attn_mask_u32: &Tensor) -> candle_core::Result<Tensor> {
    let mask_f = attn_mask_u32.to_dtype(DType::F32)?.unsqueeze(2)?; // (1, L, 1)
    let mask_f = mask_f.broadcast_as(hidden.shape())?; // (1, L, D)
    let summed = (hidden * &mask_f)?.sum(1)?; // (1, D)
    let counts = mask_f.sum(1)?.clamp(1e-9_f32, f32::INFINITY)?; // (1, D)
    summed / counts
}

/// CLS-pool: take the `[CLS]` token at position 0 of `(1, L, D)` → `(1, D)`.
fn cls_pool(hidden: &Tensor) -> candle_core::Result<Tensor> {
    hidden.narrow(1, 0, 1)?.squeeze(1)
}

/// Default embedder backed by `candle-transformers` BERT.
pub struct CandleBgeEmbedder {
    identity: EmbedderIdentity,
    tokenizer: Tokenizer,
    model: BertModel,
    device: Device,
    pooling: Pooling,
}

/// A parsed `FATHOMDB_EMBED_DEVICE` request, independent of which backends are
/// compiled in. Keeping the env-grammar parse PURE (no `Device` construction, no
/// `#[cfg]` gating, no I/O) makes it unit-testable without a GPU or a feature
/// build — see `resolve_device` for the (feature- and hardware-dependent) mapping
/// from a request to an actual [`Device`].
#[derive(Debug, Clone, PartialEq, Eq)]
enum DeviceRequest {
    /// Default — explicit `cpu`, empty/unset, or whitespace-only.
    Cpu,
    /// `cuda` (index 0) or `cuda:N`. A non-numeric index (`cuda:x`) clamps to 0,
    /// matching the original `.unwrap_or(0)` behavior.
    Cuda(usize),
    /// `metal`.
    Metal,
    /// Anything else — honored as a loud CPU fallback, never silently.
    Unknown(String),
}

/// Parse `FATHOMDB_EMBED_DEVICE` into a [`DeviceRequest`]. Pure + total:
/// case-insensitive, trims surrounding whitespace, and never panics.
fn parse_device_request(raw: &str) -> DeviceRequest {
    let requested = raw.trim().to_ascii_lowercase();
    if requested.is_empty() || requested == "cpu" {
        return DeviceRequest::Cpu;
    }
    if requested == "cuda" {
        return DeviceRequest::Cuda(0);
    }
    if let Some(idx) = requested.strip_prefix("cuda:") {
        return DeviceRequest::Cuda(idx.parse::<usize>().unwrap_or(0));
    }
    if requested == "metal" {
        return DeviceRequest::Metal;
    }
    DeviceRequest::Unknown(requested)
}

/// Resolve the candle device from `FATHOMDB_EMBED_DEVICE` (default CPU).
///
/// Accepts `cpu` | `cuda` | `cuda:N` | `metal`. GPU variants are only honored when
/// the corresponding feature (`embed-cuda` / `embed-metal`) is compiled in; otherwise
/// (or on init failure) it falls back to CPU and emits a LOUD stderr warning rather
/// than silently running 100x slower on CPU when GPU was requested (the
/// silent-slow-fallback trap). Device is NOT part of `EmbedderIdentity` — see
/// `dev/design/0.8.1-embedder-gpu-and-portability.md` §3 on cross-backend vector
/// equivalence (a 0.8.x guard).
#[allow(clippy::print_stderr)] // construction-time error path only (not in `embed()`)
fn resolve_device() -> Device {
    match parse_device_request(&std::env::var("FATHOMDB_EMBED_DEVICE").unwrap_or_default()) {
        DeviceRequest::Cpu => Device::Cpu,
        DeviceRequest::Cuda(_idx) => {
            #[cfg(feature = "embed-cuda")]
            match Device::new_cuda(_idx) {
                Ok(d) => return d,
                Err(e) => eprintln!(
                    "fathomdb-embedder: FATHOMDB_EMBED_DEVICE=cuda:{_idx} but CUDA init failed ({e}); using CPU"
                ),
            }
            #[cfg(not(feature = "embed-cuda"))]
            eprintln!(
                "fathomdb-embedder: FATHOMDB_EMBED_DEVICE=cuda requested but this build lacks the `embed-cuda` feature; using CPU"
            );
            Device::Cpu
        }
        DeviceRequest::Metal => {
            #[cfg(feature = "embed-metal")]
            match Device::new_metal(0) {
                Ok(d) => return d,
                Err(e) => eprintln!(
                    "fathomdb-embedder: FATHOMDB_EMBED_DEVICE=metal but Metal init failed ({e}); using CPU"
                ),
            }
            #[cfg(not(feature = "embed-metal"))]
            eprintln!(
                "fathomdb-embedder: FATHOMDB_EMBED_DEVICE=metal requested but this build lacks the `embed-metal` feature; using CPU"
            );
            Device::Cpu
        }
        DeviceRequest::Unknown(req) => {
            eprintln!(
                "fathomdb-embedder: FATHOMDB_EMBED_DEVICE={req} not recognized (expected cpu|cuda|cuda:N|metal); using CPU"
            );
            Device::Cpu
        }
    }
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
        // Device is resolved from FATHOMDB_EMBED_DEVICE (default CPU); GPU backends
        // are compiled in only under the `embed-cuda`/`embed-metal` features, so the
        // default build is byte-identical CPU. `embed()` already runs on `self.device`.
        let device = resolve_device();
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

        Ok(Self { identity, tokenizer, model, device, pooling: Pooling::Mean })
    }

    /// Select the pooling strategy (default [`Pooling::Mean`]). Does NOT change
    /// the embedder identity (pooling is not part of the model identity), so
    /// stored vectors from a different pooling are incompatible — use only on a
    /// fresh workspace / in measurement harnesses until the CLS↔binary-floor gate
    /// is settled.
    pub fn with_pooling(mut self, pooling: Pooling) -> Self {
        self.pooling = pooling;
        self
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

            // Pool (design §0.4 = Mean; Cls is the model-native mode, gated) then
            // L2-normalize.
            let pooled = match self.pooling {
                Pooling::Mean => mean_pool(&hidden, &attn_mask_u32)?,
                Pooling::Cls => cls_pool(&hidden)?,
            };
            let normed = l2_normalize(&pooled)?; // (1, D)

            let v: Vec<f32> = normed.squeeze(0)?.to_vec1::<f32>()?;
            Ok(v)
        };

        embed_impl().map_err(|e| EmbedderError::Failed { message: format!("forward: {e}") })
    }

    fn embed_batch(&self, inputs: &[&str]) -> Result<Vec<Vector>, EmbedderError> {
        // Invariant 3 (ADR-0.6.0-embedder-protocol.md): no log/tracing in this fn.
        // One padded (B, L) forward instead of B single-row forwards. The attention
        // mask zeros padded positions, so the mean/cls pooling + L2-norm produce the
        // SAME per-row vectors as `embed()` (parity-locked in tests). On GPU this
        // turns minutes into seconds; on CPU it is a modest win.
        if inputs.is_empty() {
            return Ok(Vec::new());
        }

        let mut encodings = Vec::with_capacity(inputs.len());
        for input in inputs {
            let enc = self
                .tokenizer
                .encode(*input, true)
                .map_err(|e| EmbedderError::Failed { message: format!("tokenize: {e}") })?;
            encodings.push(enc);
        }
        let batch = inputs.len();
        // tokenizer truncation pins each row <= MAX_SEQUENCE_TOKENS, so max_len <= 512.
        let max_len = encodings.iter().map(|e| e.get_ids().len()).max().unwrap_or(0).max(1);

        // Right-pad ids+mask to a common length; pad ids=0, mask=0 (mean/cls pooling
        // ignore masked positions, so padding cannot affect a row's vector).
        let mut ids = vec![0u32; batch * max_len];
        let mut attn = vec![0u32; batch * max_len];
        for (row, enc) in encodings.iter().enumerate() {
            let base = row * max_len;
            for (col, (&id, &mask)) in
                enc.get_ids().iter().zip(enc.get_attention_mask()).enumerate()
            {
                ids[base + col] = id;
                attn[base + col] = mask;
            }
        }

        let embed_impl = || -> candle_core::Result<Vec<Vec<f32>>> {
            let input_ids = Tensor::from_vec(ids, (batch, max_len), &self.device)?;
            let attn_mask_u32 = Tensor::from_vec(attn, (batch, max_len), &self.device)?;
            let token_type_ids = input_ids.zeros_like()?;

            let hidden = self.model.forward(&input_ids, &token_type_ids, Some(&attn_mask_u32))?;
            let pooled = match self.pooling {
                Pooling::Mean => mean_pool(&hidden, &attn_mask_u32)?,
                Pooling::Cls => cls_pool(&hidden)?,
            };
            let normed = l2_normalize(&pooled)?; // (B, D)
            normed.to_vec2::<f32>() // Vec<Vec<f32>>, one row per input
        };

        embed_impl().map_err(|e| EmbedderError::Failed { message: format!("batch forward: {e}") })
    }
}

impl CandleBgeEmbedder {
    /// Measurement-only: ONE forward pass → both the mean-pooled and CLS-pooled
    /// L2-normalized vectors, so an A/B harness can compare pooling strategies
    /// without paying the (dominant) embedding cost twice. Returns
    /// `(mean_pooled, cls_pooled)`. Not part of the `Embedder` trait.
    pub fn embed_dual_for_test(&self, input: &str) -> Result<(Vector, Vector), EmbedderError> {
        let encoding = self
            .tokenizer
            .encode(input, true)
            .map_err(|e| EmbedderError::Failed { message: format!("tokenize: {e}") })?;
        let ids: Vec<u32> = encoding.get_ids().to_vec();
        let attn: Vec<u32> = encoding.get_attention_mask().to_vec();
        let len = ids.len();

        let dual = || -> candle_core::Result<(Vec<f32>, Vec<f32>)> {
            let input_ids = Tensor::from_vec(ids, (1, len), &self.device)?;
            let attn_mask_u32 = Tensor::from_vec(attn, (1, len), &self.device)?;
            let token_type_ids = input_ids.zeros_like()?;
            let hidden = self.model.forward(&input_ids, &token_type_ids, Some(&attn_mask_u32))?;
            let mean = l2_normalize(&mean_pool(&hidden, &attn_mask_u32)?)?.squeeze(0)?.to_vec1()?;
            let cls = l2_normalize(&cls_pool(&hidden)?)?.squeeze(0)?.to_vec1()?;
            Ok((mean, cls))
        };
        dual().map_err(|e| EmbedderError::Failed { message: format!("forward: {e}") })
    }
}

#[cfg(test)]
mod device_request_tests {
    //! R-GPU-1 — pin the `FATHOMDB_EMBED_DEVICE` grammar. These exercise the
    //! PURE parse (`parse_device_request`), not `resolve_device`, so they run on
    //! the default (CPU) build with no GPU and no `embed-cuda`/`embed-metal`
    //! feature. The request→`Device` mapping is feature- and hardware-dependent
    //! and is covered by the GPU validation harness (Slice 10), not unit tests.
    use super::{parse_device_request, DeviceRequest};

    #[test]
    fn unset_or_empty_is_cpu() {
        // unset env decodes through `unwrap_or_default()` to "" at the call site.
        assert_eq!(parse_device_request(""), DeviceRequest::Cpu);
        assert_eq!(parse_device_request("   "), DeviceRequest::Cpu);
    }

    #[test]
    fn explicit_cpu_is_cpu_case_and_space_insensitive() {
        assert_eq!(parse_device_request("cpu"), DeviceRequest::Cpu);
        assert_eq!(parse_device_request("CPU"), DeviceRequest::Cpu);
        assert_eq!(parse_device_request("  Cpu  "), DeviceRequest::Cpu);
    }

    #[test]
    fn bare_cuda_is_device_zero() {
        assert_eq!(parse_device_request("cuda"), DeviceRequest::Cuda(0));
        assert_eq!(parse_device_request("CUDA"), DeviceRequest::Cuda(0));
    }

    #[test]
    fn cuda_n_selects_the_index() {
        assert_eq!(parse_device_request("cuda:0"), DeviceRequest::Cuda(0));
        assert_eq!(parse_device_request("cuda:1"), DeviceRequest::Cuda(1));
        assert_eq!(parse_device_request(" cuda:2 "), DeviceRequest::Cuda(2));
    }

    #[test]
    fn cuda_with_garbage_index_clamps_to_zero() {
        // Preserves the original `.unwrap_or(0)` behavior — a malformed index
        // is a GPU-0 request, never a panic.
        assert_eq!(parse_device_request("cuda:x"), DeviceRequest::Cuda(0));
        assert_eq!(parse_device_request("cuda:"), DeviceRequest::Cuda(0));
    }

    #[test]
    fn metal_is_metal() {
        assert_eq!(parse_device_request("metal"), DeviceRequest::Metal);
        assert_eq!(parse_device_request("Metal"), DeviceRequest::Metal);
    }

    #[test]
    fn unrecognized_is_a_named_unknown() {
        // Honored as a loud CPU fallback in `resolve_device`, never silent.
        assert_eq!(parse_device_request("rocm"), DeviceRequest::Unknown("rocm".to_string()));
        assert_eq!(parse_device_request("gpu"), DeviceRequest::Unknown("gpu".to_string()));
    }
}
