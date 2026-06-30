//! `CandleTinyBertReranker` — the default CPU cross-encoder (CE) reranker.
//!
//! Wraps `candle_transformers::models::bert::BertModel` loaded with the pinned
//! `cross-encoder/ms-marco-TinyBERT-L2-v2` weights (2-layer BERT,
//! `hidden_size = 128`, `BertForSequenceClassification` head, `num_labels = 1`).
//! Given a `(query, passage)` pair it returns the raw relevance **logit** — the
//! engine's `ce_rerank` (Slice 10 design Decision 5) sigmoids + blends it with
//! the RRF score (α = 0.3); this module does NOT sigmoid.
//!
//! ## Provenance (pinned)
//! - Repo: `cross-encoder/ms-marco-TinyBERT-L2-v2`
//!   (the canonical reranker the `ms-marco-TinyBERT-L-2-v2` name redirects to).
//! - Revision: `81d1926f67cb8eee2c2be17ca9f793c7c3bd20cc`.
//! - Architecture: `BertForSequenceClassification`, `model_type = "bert"`,
//!   2 hidden layers, 128 hidden, 2 attention heads, 512-d intermediate,
//!   `sbert_ce_default_activation_function = Identity` (the head logit is the
//!   score; no activation baked in).
//! - Weights (~17 MB `model.safetensors`) + `config.json` + `tokenizer.json`,
//!   sha256-pinned below.
//!
//! ## Footprint contract (mirrors `dev/design/0.8.1-slice-10-reranker-design.md`)
//! This module compiles ONLY under the `default-reranker` feature. The default
//! (feature-off) build pulls in zero ML code. A network fetch happens only when
//! `try_load()` is called (i.e. the engine's `rerank_depth > 0` path) AND the
//! weights are not already cached. The weight loader follows the
//! `default-embedder` pattern exactly: `ureq` fetch → sha256 verify → atomic
//! rename, cached under `~/.cache/fathomdb/reranker/<model-sha-prefix>/`.

// Design §8 (shared with the embedder): safetensors are little-endian; the
// supported targets are all LE. Fail a BE build at compile time rather than
// silently producing garbage logits.
#[cfg(target_endian = "big")]
compile_error!("fathomdb-reranker default path requires a little-endian target");

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use candle_core::{DType, Device, Tensor};
use candle_nn::{linear, Linear, Module, VarBuilder};
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
use fs2::FileExt;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokenizers::{Tokenizer, TruncationParams};

// 0.8.12 — the device grammar (`cpu`|`cuda`|`cuda:N`|`metal`) is shared with the
// embedder via `crate::device`; the reranker reads its OWN env knob
// (`FATHOMDB_RERANK_DEVICE`) and gates GPU on its OWN features
// (`rerank-cuda`/`rerank-metal`), so embedder and reranker placement stay
// independent. `DeviceRequest` is used by `resolve_device` only under a GPU
// feature; allow it to be unused on the default CPU build.
#[cfg_attr(not(any(feature = "rerank-cuda", feature = "rerank-metal")), allow(unused_imports))]
use crate::device::{parse_device_request, DeviceRequest};

/// Env var selecting the reranker compute device (default CPU). Mirrors the
/// embedder's `FATHOMDB_EMBED_DEVICE` but is a SEPARATE knob so the CE reranker
/// and the embedder can target different devices independently.
pub(crate) const ENV_RERANK_DEVICE: &str = "FATHOMDB_RERANK_DEVICE";

/// Resolve the candle device for the CE reranker from `FATHOMDB_RERANK_DEVICE`
/// (default CPU). Accepts `cpu` | `cuda` | `cuda:N` | `metal`. GPU variants are
/// only honored when the corresponding feature (`rerank-cuda` / `rerank-metal`)
/// is compiled in; otherwise (or on init failure) it falls back to CPU and emits
/// a LOUD stderr warning rather than silently running on CPU when GPU was
/// requested (the silent-slow-fallback trap). Mirrors the embedder's
/// `resolve_device` exactly, differing only in the env var + feature gates.
///
/// When neither GPU feature is on, this compiles down to "always CPU" — so the
/// default `default-reranker` build is byte-identical to the prior hard-coded
/// `Device::Cpu` (Decision 2 default preserved).
#[allow(clippy::print_stderr)] // construction-time error path only (not in `score()`)
fn resolve_device() -> Device {
    match parse_device_request(&std::env::var(ENV_RERANK_DEVICE).unwrap_or_default()) {
        DeviceRequest::Cpu => Device::Cpu,
        DeviceRequest::Cuda(_idx) => {
            #[cfg(feature = "rerank-cuda")]
            match Device::new_cuda(_idx) {
                Ok(d) => return d,
                Err(e) => eprintln!(
                    "fathomdb-reranker: FATHOMDB_RERANK_DEVICE=cuda:{_idx} but CUDA init failed ({e}); using CPU"
                ),
            }
            #[cfg(not(feature = "rerank-cuda"))]
            eprintln!(
                "fathomdb-reranker: FATHOMDB_RERANK_DEVICE=cuda requested but this build lacks the `rerank-cuda` feature; using CPU"
            );
            Device::Cpu
        }
        DeviceRequest::Metal => {
            #[cfg(feature = "rerank-metal")]
            match Device::new_metal(0) {
                Ok(d) => return d,
                Err(e) => eprintln!(
                    "fathomdb-reranker: FATHOMDB_RERANK_DEVICE=metal but Metal init failed ({e}); using CPU"
                ),
            }
            #[cfg(not(feature = "rerank-metal"))]
            eprintln!(
                "fathomdb-reranker: FATHOMDB_RERANK_DEVICE=metal requested but this build lacks the `rerank-metal` feature; using CPU"
            );
            Device::Cpu
        }
        DeviceRequest::Unknown(req) => {
            eprintln!(
                "fathomdb-reranker: FATHOMDB_RERANK_DEVICE={req} not recognized (expected cpu|cuda|cuda:N|metal); using CPU"
            );
            Device::Cpu
        }
    }
}

// ----- Pinned identity (Decision 2 + 3) -------------------------------------

/// Hugging Face repo hosting the pinned cross-encoder weights.
pub(crate) const RERANKER_REPO: &str = "cross-encoder/ms-marco-TinyBERT-L2-v2";

/// Pinned revision (commit SHA). Bumping this is a deliberate release action.
/// Exposed `pub` only under test hooks so the engine's integration test can
/// reference the pinned identity without duplicating the constant.
#[cfg(any(test, feature = "loader-test-hooks"))]
pub const RERANKER_REVISION: &str = "81d1926f67cb8eee2c2be17ca9f793c7c3bd20cc";
#[cfg(not(any(test, feature = "loader-test-hooks")))]
pub(crate) const RERANKER_REVISION: &str = "81d1926f67cb8eee2c2be17ca9f793c7c3bd20cc";

/// Engine-facing identity name (parallels `DEFAULT_EMBEDDER_NAME`).
pub const DEFAULT_RERANKER_NAME: &str = "fathomdb-ms-marco-TinyBERT-L2-v2";

const HF_BASE_URL: &str = "https://huggingface.co";

/// sha256 of `config.json` at `RERANKER_REVISION`.
const CONFIG_JSON_SHA256: &str = "2144195e107cd7ea61556478e7add12986ebfbc3085f924fc0b90c2410604879";
/// sha256 of `tokenizer.json` at `RERANKER_REVISION`.
const TOKENIZER_JSON_SHA256: &str =
    "d241a60d5e8f04cc1b2b3e9ef7a4921b27bf526d9f6050ab90f9267a1f9e5c66";
/// sha256 of `model.safetensors` at `RERANKER_REVISION`.
const MODEL_SAFETENSORS_SHA256: &str =
    "a0e7364ddf91ff7028f1102e1b91ac7a72e3db4061241bd84efe45c72c9af03a";

/// Hidden size of the pinned model (`config.json` `hidden_size`). Enforced at
/// load so a future revision drift fails loudly rather than mis-shaping the head.
const RERANKER_HIDDEN_SIZE: usize = 128;

/// Max sequence length (incl. specials) the pair tokenizer truncates to — the
/// model's 512-slot learned position embeddings.
const MAX_SEQUENCE_TOKENS: usize = 512;

/// Maximum number of `(query, passage)` pairs scored in a single `model.forward`.
///
/// One forward's self-attention allocates on the order of
/// `batch * num_heads * L_max^2` activation entries, so an unbounded,
/// caller-controlled `rerank_depth` (the candidate pool can be hundreds or
/// thousands of ~512-token passages) could consume GBs and OOM/kill the process
/// before the engine's per-pair fallback can run. Capping the per-forward batch
/// at 32 bounds peak attention memory at ~`32 * num_heads * 512^2` while still
/// amortizing kernel-launch overhead ~32× versus per-pair forwards.
///
/// Chunking never changes a score: each pair is still scored with its own mask,
/// `L_max` is computed *per chunk* (which only REDUCES memory vs a whole-batch
/// `L_max`), and `[CLS]` stays at position 0 — so the padded positions a chunk's
/// mask removes are exactly the ones a per-pair (or whole-batch) forward removes.
const MAX_CE_BATCH: usize = 32;

/// Env var overriding the cache root (otherwise `dirs::cache_dir()`).
pub(crate) const ENV_RERANKER_CACHE: &str = "FATHOMDB_RERANKER_CACHE";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(60);
const LOCK_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_ATTEMPTS: u32 = 3;

// ----- Errors ---------------------------------------------------------------

/// Failure taxonomy for reranker weight load + model construction. Mirrors the
/// embedder's `EmbedderLoadError` shape; never panics out of `try_load`.
#[derive(Debug, Error)]
pub enum RerankerLoadError {
    #[error("reranker cache dir unavailable")]
    CacheRootUnavailable,
    #[error("reranker cache I/O error at {path:?}: {source}")]
    CacheIo {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("reranker network unavailable after {attempts} attempts: {source}")]
    NetworkUnavailable {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
        attempts: u32,
    },
    #[error("reranker checksum mismatch for {file}: expected {expected}, actual {actual}")]
    ChecksumMismatch { file: String, expected: String, actual: String },
    #[error("reranker config hidden_size mismatch: expected {expected}, got {actual}")]
    HiddenSizeMismatch { expected: usize, actual: usize },
    #[error("reranker tokenizer load: {0}")]
    TokenizerLoad(String),
    #[error("reranker model deserialize: {0}")]
    ModelDeserialize(#[source] candle_core::Error),
    #[error("reranker lock timeout at {path:?} after {waited_s}s")]
    LockTimeout { path: PathBuf, waited_s: u64 },
}

// ----- Loaded model ---------------------------------------------------------

/// Default CPU cross-encoder reranker. `Send + Sync` (candle tensors are
/// `Arc`-backed), so the engine can hold one in a process-wide `OnceLock`.
pub struct CandleTinyBertReranker {
    tokenizer: Tokenizer,
    model: BertModel,
    /// `bert.pooler.dense` (128→128, tanh applied in `score`).
    pooler: Linear,
    /// `classifier` (128→1) — the relevance logit head.
    classifier: Linear,
    device: Device,
}

impl CandleTinyBertReranker {
    /// Attempt to construct a ready reranker: probe the cache, download the
    /// pinned weights on a cache miss (sha256-verified), then build the BERT +
    /// pooler + classifier. Returns `Err` (never panics) on any failure so the
    /// engine can soft-fallback to RRF order.
    pub fn try_load() -> Result<Self, RerankerLoadError> {
        let weights = load_pinned_reranker_weights()?;
        Self::from_weights(&weights)
    }

    fn from_weights(w: &LoadedWeights) -> Result<Self, RerankerLoadError> {
        // 1. config.json → bert Config.
        let config_bytes = fs::read(&w.config_json_path).map_err(|source| {
            RerankerLoadError::CacheIo { path: w.config_json_path.clone(), source }
        })?;
        let config: BertConfig =
            serde_json::from_slice(&config_bytes).map_err(|e| RerankerLoadError::CacheIo {
                path: w.config_json_path.clone(),
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()),
            })?;
        if config.hidden_size != RERANKER_HIDDEN_SIZE {
            return Err(RerankerLoadError::HiddenSizeMismatch {
                expected: RERANKER_HIDDEN_SIZE,
                actual: config.hidden_size,
            });
        }

        // 2. tokenizer.json — pin pair truncation to the 512-slot window.
        let mut tokenizer = Tokenizer::from_file(&w.tokenizer_json_path)
            .map_err(|e| RerankerLoadError::TokenizerLoad(e.to_string()))?;
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: MAX_SEQUENCE_TOKENS,
                ..Default::default()
            }))
            .map_err(|e| RerankerLoadError::TokenizerLoad(e.to_string()))?;

        // 3. mmap safetensors → BertModel (bert.* prefix via model_type
        //    fallback) + pooler dense + classifier head. Default CPU — the
        //    reranker is latency-budgeted for CPU per Decision 2, and that
        //    stays the default. 0.8.12 makes GPU an ADDITIVE opt-in knob:
        //    `FATHOMDB_RERANK_DEVICE=cuda|cuda:N|metal` is honored ONLY when the
        //    matching `rerank-cuda`/`rerank-metal` feature is compiled in;
        //    otherwise `resolve_device()` returns `Device::Cpu`, so the default
        //    build is byte-identical to the prior hard-coded `Device::Cpu`.
        let device = resolve_device();
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(
                &[w.model_safetensors_path.as_path() as &Path],
                DType::F32,
                &device,
            )
        }
        .map_err(RerankerLoadError::ModelDeserialize)?;

        let model =
            BertModel::load(vb.clone(), &config).map_err(RerankerLoadError::ModelDeserialize)?;
        let pooler = linear(
            RERANKER_HIDDEN_SIZE,
            RERANKER_HIDDEN_SIZE,
            vb.pp("bert").pp("pooler").pp("dense"),
        )
        .map_err(RerankerLoadError::ModelDeserialize)?;
        let classifier = linear(RERANKER_HIDDEN_SIZE, 1, vb.pp("classifier"))
            .map_err(RerankerLoadError::ModelDeserialize)?;

        Ok(Self { tokenizer, model, pooler, classifier, device })
    }

    /// Score a `(query, passage)` pair → the raw cross-encoder relevance logit.
    ///
    /// Tokenizes the pair (`[CLS] query [SEP] passage [SEP]` with the model's
    /// segment ids), runs the 2-layer BERT, takes the `[CLS]` hidden state,
    /// applies the pooler (`tanh(dense(cls))`) and the classification head.
    /// Deterministic: no dropout/sampling at inference (Decision 8).
    ///
    /// Returns `Err` on any tokenize/forward failure so the engine can treat a
    /// failed pair as a neutral score rather than panic in the reader thread.
    pub fn score(&self, query: &str, passage: &str) -> Result<f32, candle_core::Error> {
        let enc = self
            .tokenizer
            .encode((query, passage), true)
            .map_err(|e| candle_core::Error::Msg(format!("tokenize pair: {e}")))?;
        let ids: Vec<u32> = enc.get_ids().to_vec();
        let type_ids: Vec<u32> = enc.get_type_ids().to_vec();
        let attn: Vec<u32> = enc.get_attention_mask().to_vec();
        let len = ids.len();

        let input_ids = Tensor::from_vec(ids, (1, len), &self.device)?;
        let token_type_ids = Tensor::from_vec(type_ids, (1, len), &self.device)?;
        let attn_mask = Tensor::from_vec(attn, (1, len), &self.device)?;

        // (1, L, H) sequence output.
        let hidden = self.model.forward(&input_ids, &token_type_ids, Some(&attn_mask))?;
        // [CLS] at position 0 → (1, H). `.contiguous()` because `narrow` yields a
        // strided view and the CUDA matmul backend (the pooler's `dense`) rejects
        // non-contiguous operands; a no-op on CPU / when already contiguous.
        let cls = hidden.narrow(1, 0, 1)?.squeeze(1)?.contiguous()?;
        // Pooler: tanh(dense(cls)).
        let pooled = self.pooler.forward(&cls)?.tanh()?;
        // Classifier: (1, 1) logit.
        let logit = self.classifier.forward(&pooled)?;
        logit.squeeze(1)?.squeeze(0)?.to_scalar::<f32>()
    }

    /// Batched variant of [`score`](Self::score): score every `(query, passage_i)`
    /// pair with far fewer kernel launches than `N` per-pair forwards on the hot
    /// rerank path, while **bounding peak memory**.
    ///
    /// The pairs are split into chunks of at most `MAX_CE_BATCH` and each chunk
    /// runs in ONE `model.forward`; the per-chunk `Vec<f32>` results are
    /// concatenated in input order. Chunking caps peak self-attention memory at
    /// ~`MAX_CE_BATCH * num_heads * L_max^2` regardless of how large the
    /// caller-controlled pool is, so a big `rerank_depth` cannot OOM the process.
    /// For `passages.len() <= MAX_CE_BATCH` (incl. the single-pair case) this is a
    /// single forward, behaving exactly as before.
    ///
    /// Within each chunk every pair is tokenized identically to
    /// [`score`](Self::score) (`encode((query, passage), true)`), then
    /// **right-padded** to that chunk's `L_max = max(len)`: real tokens are
    /// left-aligned (so `[CLS]` stays at position 0, never padded) and pad slots
    /// get the tokenizer's pad id, `token_type_id = 0`, and `attention_mask = 0`.
    /// Candle's BERT lifts the 2-D mask through `get_extended_attention_mask`,
    /// adding `f32::MIN` to padded positions so the softmax zeroes them — making
    /// the CLS hidden state attend ONLY to that pair's real tokens. Computing
    /// `L_max` per chunk only REDUCES padding (and thus memory) versus a
    /// whole-batch `L_max`, and never perturbs a logit. Consequently
    /// `score_batch(q, [p_i..])[i] == score(q, p_i)` within floating-point
    /// tolerance for any batch size.
    ///
    /// Output order matches the input `passages` order. An empty `passages`
    /// returns `Ok(vec![])` with NO forward. Any tokenize/forward failure
    /// propagates as `Err` so the caller can map it to the neutral-score contract.
    pub fn score_batch(
        &self,
        query: &str,
        passages: &[&str],
    ) -> Result<Vec<f32>, candle_core::Error> {
        if passages.is_empty() {
            return Ok(vec![]);
        }
        // Bound peak attention memory: at most MAX_CE_BATCH pairs per forward.
        // `chunks` of a non-empty slice yields only non-empty chunks; results are
        // concatenated in input order so the output index matches `passages`.
        let mut out: Vec<f32> = Vec::with_capacity(passages.len());
        for chunk in passages.chunks(MAX_CE_BATCH) {
            out.extend(self.score_chunk(query, chunk)?);
        }
        Ok(out)
    }

    /// Score one bounded chunk (`1..=MAX_CE_BATCH` pairs) in a single forward.
    /// Pads to this chunk's `L_max`; see [`score_batch`](Self::score_batch) for
    /// the masking/correctness contract.
    fn score_chunk(&self, query: &str, passages: &[&str]) -> Result<Vec<f32>, candle_core::Error> {
        // Tokenize each pair exactly as `score` does, retaining per-pair ids,
        // segment ids, and attention masks.
        let mut all_ids: Vec<Vec<u32>> = Vec::with_capacity(passages.len());
        let mut all_types: Vec<Vec<u32>> = Vec::with_capacity(passages.len());
        let mut all_attn: Vec<Vec<u32>> = Vec::with_capacity(passages.len());
        let mut l_max = 0usize;
        for passage in passages {
            let enc = self
                .tokenizer
                .encode((query, *passage), true)
                .map_err(|e| candle_core::Error::Msg(format!("tokenize pair: {e}")))?;
            let ids = enc.get_ids().to_vec();
            l_max = l_max.max(ids.len());
            all_types.push(enc.get_type_ids().to_vec());
            all_attn.push(enc.get_attention_mask().to_vec());
            all_ids.push(ids);
        }

        // BERT `[PAD]` token id (config `pad_token_id = 0`); the value is inert
        // because the attention mask removes pad positions, but use the real id
        // so embeddings stay in-vocab.
        let pad_id = self.tokenizer.token_to_id("[PAD]").unwrap_or(0);
        let n = passages.len();

        // Right-pad each row to L_max into flat (N * L_max) buffers.
        let mut ids_buf: Vec<u32> = Vec::with_capacity(n * l_max);
        let mut type_buf: Vec<u32> = Vec::with_capacity(n * l_max);
        let mut attn_buf: Vec<u32> = Vec::with_capacity(n * l_max);
        for i in 0..n {
            let ids = &all_ids[i];
            let types = &all_types[i];
            let attn = &all_attn[i];
            let len = ids.len();
            ids_buf.extend_from_slice(ids);
            type_buf.extend_from_slice(types);
            attn_buf.extend_from_slice(attn);
            for _ in len..l_max {
                ids_buf.push(pad_id);
                type_buf.push(0);
                attn_buf.push(0);
            }
        }

        let input_ids = Tensor::from_vec(ids_buf, (n, l_max), &self.device)?;
        let token_type_ids = Tensor::from_vec(type_buf, (n, l_max), &self.device)?;
        let attn_mask = Tensor::from_vec(attn_buf, (n, l_max), &self.device)?;

        // (N, L_max, H) sequence output → [CLS] at position 0 → (N, H).
        let hidden = self.model.forward(&input_ids, &token_type_ids, Some(&attn_mask))?;
        // `.contiguous()`: for N>1 the narrowed CLS slice is strided (rows L_max*H
        // apart) and the CUDA matmul backend (the pooler's `dense`) rejects
        // non-contiguous operands; a no-op on CPU / when already contiguous.
        let cls = hidden.narrow(1, 0, 1)?.squeeze(1)?.contiguous()?;
        // Pooler: tanh(dense(cls)) → classifier → (N, 1) → (N,).
        let pooled = self.pooler.forward(&cls)?.tanh()?;
        let logits = self.classifier.forward(&pooled)?.squeeze(1)?;
        logits.to_vec1::<f32>()
    }
}

// ----- Tests (default-reranker only; uses the locally cached pinned model) ----

#[cfg(test)]
mod tests {
    use super::*;

    /// Load the pinned reranker from the local cache, or skip the test if it is
    /// unavailable (offline CI with a cold cache). Locally the model is cached so
    /// these tests DO execute the real forward — they are not vacuously green.
    fn load_or_skip() -> Option<CandleTinyBertReranker> {
        match CandleTinyBertReranker::try_load() {
            Ok(m) => Some(m),
            Err(e) => {
                eprintln!("SKIP: reranker model unavailable ({e})");
                None
            }
        }
    }

    const QUERY: &str = "What is the capital of France?";
    // Three passages of deliberately DIFFERENT token lengths so that batching
    // forces right-padding to L_max — this is what exercises the pad/mask path.
    const PASSAGES: [&str; 3] = [
        "Paris.",
        "Paris is the capital and most populous city of France.",
        "France is a country in Western Europe with several overseas regions and \
         territories; its capital and largest city is Paris, a major global center \
         for art, fashion, gastronomy, and culture, situated on the river Seine.",
    ];

    /// THE load-bearing test: batched scoring must equal per-pair scoring within
    /// tolerance on pairs of differing token lengths (proves pad + mask).
    #[test]
    fn score_batch_matches_per_pair() {
        let Some(model) = load_or_skip() else { return };

        let per_pair: Vec<f32> =
            PASSAGES.iter().map(|p| model.score(QUERY, p).expect("per-pair score")).collect();
        let batched = model.score_batch(QUERY, &PASSAGES).expect("batched score");

        assert_eq!(batched.len(), PASSAGES.len(), "one logit per passage, order preserved");
        let mut max_abs = 0f32;
        for (i, (b, p)) in batched.iter().zip(per_pair.iter()).enumerate() {
            let d = (b - p).abs();
            max_abs = max_abs.max(d);
            assert!(
                d < 1e-3,
                "pair {i}: batched {b} vs per-pair {p} differ by {d} (>1e-3); pad/mask is wrong"
            );
        }
        eprintln!("score_batch vs per-pair max abs diff = {max_abs:e}");
    }

    /// Determinism: identical input → byte-identical output across two calls.
    #[test]
    fn score_batch_is_deterministic() {
        let Some(model) = load_or_skip() else { return };
        let a = model.score_batch(QUERY, &PASSAGES).expect("batch a");
        let b = model.score_batch(QUERY, &PASSAGES).expect("batch b");
        assert_eq!(a, b, "batched scoring must be deterministic");
    }

    /// A very short passage mixed with a ~512-cap (truncated) passage in one batch
    /// still matches per-pair — exercises the maximum padding span.
    #[test]
    fn score_batch_short_plus_capped() {
        let Some(model) = load_or_skip() else { return };
        let long = "lorem ipsum ".repeat(2000); // far exceeds 512 tokens → truncated
        let passages: Vec<&str> = vec!["hi", long.as_str()];

        let per_pair: Vec<f32> =
            passages.iter().map(|p| model.score(QUERY, p).expect("per-pair")).collect();
        let batched = model.score_batch(QUERY, &passages).expect("batched");
        for (i, (b, p)) in batched.iter().zip(per_pair.iter()).enumerate() {
            assert!((b - p).abs() < 1e-3, "pair {i}: {b} vs {p} (short+capped batch)");
        }
    }

    /// Chunk-boundary correctness: a batch of N > `MAX_CE_BATCH` pairs (spanning
    /// two full chunks plus a partial trailing chunk), of DIFFERING token lengths
    /// so each chunk pads to a different per-chunk `L_max`, must still equal
    /// per-pair `score` within tolerance AND preserve input order across chunk
    /// boundaries. This is the load-bearing test for the OOM-bounding fix: it
    /// proves the chunk loop concatenates per-chunk results in order and that
    /// per-chunk (rather than whole-batch) padding does not perturb any logit.
    #[test]
    fn score_batch_spans_chunk_boundaries() {
        let Some(model) = load_or_skip() else { return };

        // Spans 2 full chunks + a 3-element partial chunk (e.g. 67 at MAX=32).
        let n = MAX_CE_BATCH * 2 + 3;
        // Differing lengths so per-chunk L_max varies across chunk boundaries.
        let owned: Vec<String> =
            (0..n).map(|i| "Paris is the capital of France. ".repeat(1 + (i % 7))).collect();
        let passages: Vec<&str> = owned.iter().map(String::as_str).collect();

        let per_pair: Vec<f32> =
            passages.iter().map(|p| model.score(QUERY, p).expect("per-pair score")).collect();
        let batched = model.score_batch(QUERY, &passages).expect("batched score");

        assert_eq!(batched.len(), n, "one logit per passage across all chunks, order preserved");
        let mut max_abs = 0f32;
        for (i, (b, p)) in batched.iter().zip(per_pair.iter()).enumerate() {
            let d = (b - p).abs();
            max_abs = max_abs.max(d);
            assert!(
                d < 1e-3,
                "pair {i}: batched {b} vs per-pair {p} differ by {d} (>1e-3) across chunk boundary"
            );
        }
        eprintln!("score_batch_spans_chunk_boundaries: N={n} (MAX_CE_BATCH={MAX_CE_BATCH}), max abs diff = {max_abs:e}");
    }

    /// Empty batch → Ok(vec![]) with no forward.
    #[test]
    fn score_batch_empty() {
        let Some(model) = load_or_skip() else { return };
        let out = model.score_batch(QUERY, &[]).expect("empty batch");
        assert!(out.is_empty(), "empty passages → empty output");
    }
}

// ----- GPU smoke + CPU↔GPU closeness (rerank-cuda only) ---------------------

/// 0.8.12 — these only build/run under `--features rerank-cuda` on a CUDA host;
/// the default CPU test suite never compiles them. They prove (1) the GPU device
/// resolves + a forward runs end-to-end returning a FINITE logit, and (2) the
/// GPU port is numerically faithful: GPU logits match the CPU logits within a
/// tolerance, so the opt-in does not silently change scores. If the pinned model
/// is not cached the tests skip (no network in CI), exactly like the CPU suite.
#[cfg(all(test, feature = "rerank-cuda"))]
mod gpu_tests {
    use super::*;
    use std::sync::Mutex;

    const Q: &str = "What is the capital of France?";
    const PASSAGES: [&str; 3] = [
        "Paris.",
        "Paris is the capital and most populous city of France.",
        "France is a country in Western Europe; its capital and largest city is \
         Paris, a major global center for art, fashion, and culture.",
    ];

    /// `FATHOMDB_RERANK_DEVICE` is process-global, and `cargo test` runs `#[test]`
    /// fns in parallel within one binary. Each test takes this lock for its WHOLE
    /// body so the env var it sets cannot be observed/overwritten by a sibling
    /// test's load (which would resolve the wrong device and either compare the
    /// wrong backends or build a CPU model where CUDA was expected). `unwrap_or_else
    /// (into_inner)` so one panicking test does not poison-cascade into the other
    /// (the real failure still surfaces in the test that panicked).
    static DEVICE_ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Load a reranker with `FATHOMDB_RERANK_DEVICE` set to `dev` for the duration
    /// of construction, then RESTORE the previous value (not blindly remove it).
    /// The device is read once at load time and stored in the struct, so the env
    /// only needs to be stable across this call. Caller MUST hold `DEVICE_ENV_LOCK`.
    /// Returns None (skip) if the model is uncached.
    fn load_on(dev: &str) -> Option<CandleTinyBertReranker> {
        let prev = std::env::var_os(ENV_RERANK_DEVICE);
        std::env::set_var(ENV_RERANK_DEVICE, dev);
        let m = match CandleTinyBertReranker::try_load() {
            Ok(m) => Some(m),
            Err(e) => {
                eprintln!("SKIP gpu_tests: reranker model unavailable ({e})");
                None
            }
        };
        match prev {
            Some(v) => std::env::set_var(ENV_RERANK_DEVICE, v),
            None => std::env::remove_var(ENV_RERANK_DEVICE),
        }
        m
    }

    /// Smoke: the reranker loads on cuda:0 and scores one pair to a finite logit.
    #[test]
    fn gpu_loads_and_scores_finite() {
        let _guard = DEVICE_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let Some(model) = load_on("cuda:0") else { return };
        assert!(
            model.device.is_cuda(),
            "FATHOMDB_RERANK_DEVICE=cuda:0 must resolve to a CUDA device"
        );
        let logit = model.score(Q, PASSAGES[1]).expect("gpu score");
        assert!(logit.is_finite(), "gpu logit must be finite, got {logit}");
        eprintln!("gpu_loads_and_scores_finite: cuda:0 logit = {logit}");
    }

    /// CPU↔GPU numeric closeness: the same pairs scored on CPU and on cuda:0 must
    /// agree within tolerance (different BLAS/kernels → not bit-identical, but the
    /// port must not change the ranking-relevant logit). Tolerance is generous
    /// relative to the ~unit-scale logits but far tighter than any score gap that
    /// would flip a ranking.
    #[test]
    fn cpu_gpu_logits_close() {
        let _guard = DEVICE_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let Some(cpu) = load_on("cpu") else { return };
        let Some(gpu) = load_on("cuda:0") else { return };
        assert!(!cpu.device.is_cuda() && gpu.device.is_cuda());

        let cpu_logits: Vec<f32> =
            PASSAGES.iter().map(|p| cpu.score(Q, p).expect("cpu score")).collect();
        let gpu_logits: Vec<f32> =
            PASSAGES.iter().map(|p| gpu.score(Q, p).expect("gpu score")).collect();

        let mut max_abs = 0f32;
        for (i, (c, g)) in cpu_logits.iter().zip(gpu_logits.iter()).enumerate() {
            let d = (c - g).abs();
            max_abs = max_abs.max(d);
            assert!(d < 1e-2, "pair {i}: cpu {c} vs gpu {g} differ by {d} (>1e-2)");
        }
        eprintln!("cpu_gpu_logits_close: max abs diff = {max_abs:e}");
    }
}

// ----- Weight loader (mirrors loader.rs; reranker-pinned) --------------------

struct LoadedWeights {
    config_json_path: PathBuf,
    tokenizer_json_path: PathBuf,
    model_safetensors_path: PathBuf,
}

/// Cache dir: `<cache-root>/fathomdb/reranker/<sha256(repo@rev)[..12]>/`.
fn cache_dir() -> Result<PathBuf, RerankerLoadError> {
    let root = match std::env::var_os(ENV_RERANKER_CACHE) {
        Some(p) => PathBuf::from(p),
        None => dirs::cache_dir().ok_or(RerankerLoadError::CacheRootUnavailable)?,
    };
    let mut h = Sha256::new();
    h.update(format!("{RERANKER_REPO}@{RERANKER_REVISION}").as_bytes());
    // digest 0.11's `Array` output drops the `LowerHex` impl `GenericArray`
    // had; format explicitly to the same lowercase, zero-padded hex.
    let prefix: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    Ok(root.join("fathomdb").join("reranker").join(&prefix[..12]))
}

fn load_pinned_reranker_weights() -> Result<LoadedWeights, RerankerLoadError> {
    let dir = cache_dir()?;
    fs::create_dir_all(&dir)
        .map_err(|source| RerankerLoadError::CacheIo { path: dir.clone(), source })?;

    let files = [
        ("config.json", CONFIG_JSON_SHA256),
        ("tokenizer.json", TOKENIZER_JSON_SHA256),
        ("model.safetensors", MODEL_SAFETENSORS_SHA256),
    ];
    let mut paths = Vec::with_capacity(3);
    for (name, sha) in files {
        let final_path = dir.join(name);
        // Cache fast-path: verified locally → no lock, no network.
        if file_matches_sha(&final_path, sha)? {
            paths.push(final_path);
            continue;
        }
        fetch_under_lock(&dir, name, sha)?;
        paths.push(final_path);
    }
    Ok(LoadedWeights {
        config_json_path: paths[0].clone(),
        tokenizer_json_path: paths[1].clone(),
        model_safetensors_path: paths[2].clone(),
    })
}

fn fetch_under_lock(dir: &Path, name: &str, sha: &str) -> Result<(), RerankerLoadError> {
    let lock_path = dir.join(".lock");
    let lock_file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|source| RerankerLoadError::CacheIo { path: lock_path.clone(), source })?;
    acquire_lock(&lock_file, &lock_path)?;
    let _guard = LockGuard(&lock_file);

    let final_path = dir.join(name);
    // Double-checked: another process may have completed the fetch.
    if file_matches_sha(&final_path, sha)? {
        return Ok(());
    }

    let partial = dir.join(format!("{name}.partial"));
    let url = format!("{HF_BASE_URL}/{RERANKER_REPO}/resolve/{RERANKER_REVISION}/{name}");
    download_with_retries(&url, &partial)?;

    let observed = sha256_file(&partial)
        .map_err(|source| RerankerLoadError::CacheIo { path: partial.clone(), source })?;
    if observed != sha {
        let _ = fs::remove_file(&partial);
        return Err(RerankerLoadError::ChecksumMismatch {
            file: name.to_string(),
            expected: sha.to_string(),
            actual: observed,
        });
    }
    fs::rename(&partial, &final_path)
        .map_err(|source| RerankerLoadError::CacheIo { path: final_path.clone(), source })?;
    Ok(())
}

fn download_with_retries(url: &str, partial: &Path) -> Result<(), RerankerLoadError> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout_read(READ_TIMEOUT)
        .redirects(5)
        .build();
    let token = std::env::var("HF_TOKEN").ok();

    let mut last_err: Option<Box<dyn std::error::Error + Send + Sync>> = None;
    for attempt in 0..MAX_ATTEMPTS {
        match download_once(&agent, token.as_deref(), url, partial) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = Some(e);
                if attempt + 1 < MAX_ATTEMPTS {
                    std::thread::sleep(Duration::from_secs(1u64 << attempt));
                }
            }
        }
    }
    Err(RerankerLoadError::NetworkUnavailable {
        source: last_err.expect("at least one attempt error"),
        attempts: MAX_ATTEMPTS,
    })
}

fn download_once(
    agent: &ureq::Agent,
    token: Option<&str>,
    url: &str,
    partial: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut req = agent.get(url);
    if let Some(t) = token {
        req = req.set("Authorization", &format!("Bearer {t}"));
    }
    let resp = req.call()?;
    if resp.status() != 200 {
        return Err(format!("unexpected status {}", resp.status()).into());
    }
    // Fresh download each attempt: a stale partial must not be appended to.
    let mut f = OpenOptions::new().write(true).create(true).truncate(true).open(partial)?;
    let mut reader = resp.into_reader();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        f.write_all(&buf[..n])?;
    }
    f.sync_all()?;
    Ok(())
}

fn acquire_lock(f: &File, lock_path: &Path) -> Result<(), RerankerLoadError> {
    let deadline = std::time::Instant::now() + LOCK_TIMEOUT;
    loop {
        match f.try_lock_exclusive() {
            Ok(()) => return Ok(()),
            Err(e) => {
                if e.kind() != std::io::ErrorKind::WouldBlock {
                    return Err(RerankerLoadError::CacheIo {
                        path: lock_path.to_path_buf(),
                        source: e,
                    });
                }
                if std::time::Instant::now() >= deadline {
                    return Err(RerankerLoadError::LockTimeout {
                        path: lock_path.to_path_buf(),
                        waited_s: LOCK_TIMEOUT.as_secs(),
                    });
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    }
}

struct LockGuard<'a>(&'a File);
impl Drop for LockGuard<'_> {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(self.0);
    }
}

fn file_matches_sha(path: &Path, expected: &str) -> Result<bool, RerankerLoadError> {
    if !path.is_file() {
        return Ok(false);
    }
    let observed = sha256_file(path)
        .map_err(|source| RerankerLoadError::CacheIo { path: path.to_path_buf(), source })?;
    Ok(observed == expected)
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    // digest 0.11 `Array` output: format to identical lowercase, zero-padded hex.
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}
