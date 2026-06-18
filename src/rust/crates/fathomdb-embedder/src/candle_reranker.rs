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
        //    fallback) + pooler dense + classifier head. CPU only (the
        //    reranker is latency-budgeted for CPU per Decision 2).
        let device = Device::Cpu;
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
        // [CLS] at position 0 → (1, H).
        let cls = hidden.narrow(1, 0, 1)?.squeeze(1)?;
        // Pooler: tanh(dense(cls)).
        let pooled = self.pooler.forward(&cls)?.tanh()?;
        // Classifier: (1, 1) logit.
        let logit = self.classifier.forward(&pooled)?;
        logit.squeeze(1)?.squeeze(0)?.to_scalar::<f32>()
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
    let prefix = format!("{:x}", h.finalize());
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
    Ok(format!("{:x}", hasher.finalize()))
}
