//! Default-embedder weight loader.
//!
//! Implements the contract specified in
//! `dev/plans/prompts/0.7.1-EMBEDDER-UNDEFER-HANDOFF.md` §EU-3 (which
//! itself materialises `dev/design/embedder.md` §§1–10):
//!
//! 1. HF resolve URL pattern, three pinned files, sha256 pinned in Rust.
//! 2. `ureq` blocking transport, 10s connect / 60s read, 3-attempt backoff,
//!    Range resume on partial.
//! 3. `HF_TOKEN` env var → `Authorization: Bearer`. No keychain. No file.
//! 4. `<dirs::cache_dir>/fathomdb/embedders/<model-sha-prefix>/<file>`;
//!    best-effort read-only probe of `~/.cache/huggingface` for users who
//!    already have the file.
//! 5. `.partial` → fsync → rename.
//! 6. sha256-verify; on mismatch remove `.partial` and fail with
//!    `EmbedderLoadError::ChecksumMismatch`. No trust-on-first-use.
//! 7. Returns `Vec<EmbedderEvent>` so EU-5 can splice it into
//!    `OpenReport.embedder_events` without re-running the work.
//! 9. `EmbedderLoadError` taxonomy mirrors the design.
//! 10. `fs2::FileExt::lock_exclusive` on a `.lock` sibling for the cache
//!     directory; held only during fetch+verify+rename; cache-hit path does
//!     NOT take the lock.
//!
//! Candle / `BertModel` construction is **EU-4** and is intentionally NOT
//! in this slice. The loader's deliverable is byte-buffer-backed file
//! paths plus an events log.
//!
//! ## Scope guardrails (ADR-0.7.1-default-embedder-weight-fetch)
//!
//! Pinned constants are `pub(crate) const`, not `pub const`. No public
//! function takes a `&str` model name, URL, or repo. The only way to
//! reach the loader from outside this module is the zero-arg entry
//! point `load_pinned_default_embedder()`. Tests reach the override
//! surface via `LoaderConfig::for_tests()` which is `cfg(any(test, ...))`
//! gated — see the impl below.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use fs2::FileExt;
use sha2::{Digest, Sha256};
use thiserror::Error;

// ----- Pinned constants (design §1) -----------------------------------------

/// Hugging Face model repository hosting the default embedder weights.
pub(crate) const HF_REPO: &str = "BAAI/bge-small-en-v1.5";

/// Pinned revision (commit SHA) on the HF repo. Bumping this is a deliberate
/// release-engineering action, not a runtime input.
pub(crate) const HF_REVISION: &str = "5c38ec7c405ec4b44b94cc5a9bb96e735b38267a";

/// Production HF base URL. Tests override via `LoaderConfig::with_base_url`.
pub(crate) const HF_BASE_URL: &str = "https://huggingface.co";

/// sha256 of `config.json` at `HF_REVISION`. Computed once via
/// `curl -sL <resolve-url> | sha256sum` and pinned here.
pub(crate) const CONFIG_JSON_SHA256: &str =
    "094f8e891b932f2000c92cfc663bac4c62069f5d8af5b5278c4306aef3084750";

/// sha256 of `tokenizer.json` at `HF_REVISION`.
pub(crate) const TOKENIZER_JSON_SHA256: &str =
    "d241a60d5e8f04cc1b2b3e9ef7a4921b27bf526d9f6050ab90f9267a1f9e5c66";

/// sha256 of `model.safetensors` at `HF_REVISION`.
pub(crate) const MODEL_SAFETENSORS_SHA256: &str =
    "3c9f31665447c8911517620762200d2245a2518d6e7208acc78cd9db317e21ad";

/// Default connect timeout (design §2).
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Default read timeout (design §2).
const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(60);
/// Total attempts including the first (design §2).
///
/// Design §2 calls out "three attempts per file, with exponential backoff
/// of 1s, 2s, 4s between attempts." We interpret that literally as **3
/// attempt slots** (initial + 2 retries), with sleeps of **1s and 2s**
/// between them — the trailing "4s" in the design phrasing is the
/// hypothetical sleep that would precede a fourth slot we never make.
/// See Issue 4 in the EU-3 FIX-1 review notes.
const MAX_ATTEMPTS: u32 = 3;
/// Default lock acquisition timeout (design §10).
const DEFAULT_LOCK_TIMEOUT: Duration = Duration::from_secs(120);

/// Env var consulted for the lock timeout override.
pub(crate) const ENV_LOCK_TIMEOUT: &str = "FATHOMDB_EMBEDDER_LOCK_TIMEOUT_SECS";

/// Returns the 12-hex-char model-sha-prefix used in the cache layout
/// (design §4): `sha256("<repo>@<revision>")[..12]`. Computed once and
/// memoized — the inputs are compile-time constants but `sha2::Sha256`
/// has no const-eval path, so we lazy-cache.
fn model_sha_prefix() -> &'static str {
    static PREFIX: OnceLock<String> = OnceLock::new();
    PREFIX.get_or_init(|| {
        let mut h = Sha256::new();
        h.update(format!("{HF_REPO}@{HF_REVISION}").as_bytes());
        let hex = format!("{:x}", h.finalize());
        hex[..12].to_string()
    })
}

// ----- Public types ---------------------------------------------------------

/// Handles into the verified weight cache.
///
/// EU-4 will accept this and construct `BertModel` + `Tokenizer` from these
/// paths. The loader's contract is "the paths exist on disk and their bytes
/// hash to the pinned constants" — nothing more.
#[derive(Debug, Clone)]
pub struct LoadedWeights {
    pub config_json_path: PathBuf,
    pub tokenizer_json_path: PathBuf,
    pub model_safetensors_path: PathBuf,
    /// Bytes pulled over the network during this call. `0` on a full cache
    /// hit (cold-start of a process whose cache is already populated).
    pub bytes_downloaded: u64,
    /// Structured events surfaced into `OpenReport.embedder_events` by
    /// EU-5. Ordering matches the order the loader observed them.
    pub events: Vec<EmbedderEvent>,
}

/// Structured event surfaced through `OpenReport.embedder_events` (design §7).
///
/// Only two variants ship in this slice; EU-5 may add more as the engine
/// wiring matures. Field types are kept JSON-friendly (`String`, `u64`,
/// `PathBuf`-as-`String`) so the binding layer can round-trip them without
/// schema work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbedderEvent {
    /// A file was fetched from the network and written to the cache.
    DefaultEmbedderDownload {
        file: String,
        url: String,
        bytes: u64,
        sha256: String,
        cache_path: PathBuf,
        duration_ms: u64,
    },
    /// A file was found in the cache and verified by sha256. No network.
    DefaultEmbedderCacheHit { file: String, sha256: String, cache_path: PathBuf },
}

/// Failure taxonomy (design §9). Engine-level mapping is owned by EU-5.
#[derive(Debug, Error)]
pub enum EmbedderLoadError {
    #[error("network unavailable while fetching {file}: {source}")]
    NetworkUnavailable {
        file: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("checksum mismatch for {file}: expected {expected}, observed {observed}")]
    ChecksumMismatch { file: String, expected: String, observed: String },

    #[error("cache I/O error at {path:?}: {source}")]
    CacheIoError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Reserved for EU-4 (model byte → `BertModel` parse failures).
    #[error("model deserialize: {0}")]
    ModelDeserialize(String),

    /// Reserved for EU-4 (`tokenizer.json` parse failures).
    #[error("tokenizer load: {0}")]
    TokenizerLoad(String),

    #[error("timed out acquiring embedder cache lock after {0:?}")]
    LockTimeout(Duration),
}

/// Whether a given error should be retried within `download_with_retries`
/// (design §2). Connect failures, 5xx, read timeouts, 408, and 429 are
/// retryable; everything else fails fast (including 4xx other than 408/429,
/// `CacheIoError`, `ChecksumMismatch`, and `LockTimeout`).
fn retry_decision(err: &EmbedderLoadError) -> RetryDecision {
    match err {
        EmbedderLoadError::NetworkUnavailable { source, .. } => {
            // Inspect the message; the `ureq::Error` Display surfaces the
            // status code on `Status(code, _)` errors. Transport errors
            // (DNS/TCP/timeout) are `Transport(_)` — those always retry.
            let s = source.to_string();
            // Heuristic: numeric status preceded by a non-digit and followed
            // by a non-digit. Cheap and good enough for the small set we care
            // about, without taking a concrete `ureq::Error` dependency in
            // this dispatch.
            if let Some(code) = extract_http_status(&s) {
                if (500..=599).contains(&code) || code == 408 || code == 429 {
                    RetryDecision::Retry
                } else {
                    RetryDecision::FailFast
                }
            } else {
                // No status → transport-level error (DNS/connect/read timeout).
                RetryDecision::Retry
            }
        }
        // CacheIoError, ChecksumMismatch, LockTimeout, ModelDeserialize,
        // TokenizerLoad are all fail-fast per design §2.
        _ => RetryDecision::FailFast,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetryDecision {
    Retry,
    FailFast,
}

/// Pulls the first `100..=599` token out of an error message. Used to
/// classify retryability of an HTTP error without coupling to `ureq`'s
/// concrete error enum at this layer.
fn extract_http_status(msg: &str) -> Option<u16> {
    let bytes = msg.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let mut j = i;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            // Require a 3-digit run, not flanked by another digit.
            if j - i == 3 {
                if let Ok(s) = std::str::from_utf8(&bytes[i..j]) {
                    if let Ok(n) = s.parse::<u16>() {
                        if (100..=599).contains(&n) {
                            return Some(n);
                        }
                    }
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    None
}

// ----- Loader configuration -------------------------------------------------

/// Configuration for the loader. Production code constructs this via
/// `LoaderConfig::production()` (called from `load_pinned_default_embedder`).
/// Tests construct via `LoaderConfig::for_tests()` and then override only the
/// surfaces explicitly designed for testing (base URL, cache root, pinned
/// shas, HF token).
///
/// **Scope guardrail (ADR-0.7.1 #1)**: outside the `loader-test-hooks`
/// feature, this type's constructors, setters, and the
/// `load_with_config` entry point are unreachable from external crates.
/// In production, `load_pinned_default_embedder()` is the only public
/// surface — base URL, repo, and pinned shas cannot be substituted.
#[derive(Debug, Clone)]
pub struct LoaderConfig {
    base_url: String,
    cache_root: PathBuf,
    hf_token: Option<String>,
    config_sha: String,
    tokenizer_sha: String,
    model_sha: String,
    connect_timeout: Duration,
    read_timeout: Duration,
    lock_timeout: Duration,
}

impl LoaderConfig {
    /// Production constructor: real HF base URL, OS cache dir, real pinned
    /// shas, `HF_TOKEN` env var (if set).
    ///
    /// `pub(crate)` so only `load_pinned_default_embedder()` can build a
    /// production-config — downstream callers cannot reach a `LoaderConfig`
    /// without enabling `loader-test-hooks` (ADR-0.7.1 scope guardrail #1).
    pub(crate) fn production() -> Result<Self, EmbedderLoadError> {
        let cache_root = dirs::cache_dir().ok_or_else(|| EmbedderLoadError::CacheIoError {
            path: PathBuf::from("<dirs::cache_dir>"),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "platform cache dir unavailable",
            ),
        })?;
        let lock_timeout = std::env::var(ENV_LOCK_TIMEOUT)
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .map(Duration::from_secs)
            .unwrap_or(DEFAULT_LOCK_TIMEOUT);
        Ok(Self {
            base_url: HF_BASE_URL.to_string(),
            cache_root,
            hf_token: std::env::var("HF_TOKEN").ok(),
            config_sha: CONFIG_JSON_SHA256.to_string(),
            tokenizer_sha: TOKENIZER_JSON_SHA256.to_string(),
            model_sha: MODEL_SAFETENSORS_SHA256.to_string(),
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            read_timeout: DEFAULT_READ_TIMEOUT,
            lock_timeout,
        })
    }

    /// Test constructor: dummy base URL / cache, placeholder shas. Callers
    /// (tests only — see module docs) override what they care about via the
    /// builder setters below.
    #[cfg(any(test, feature = "loader-test-hooks"))]
    pub fn for_tests() -> Self {
        Self {
            base_url: "http://127.0.0.1:0".to_string(),
            cache_root: PathBuf::from("/tmp/fathomdb-embedder-tests"),
            hf_token: None,
            config_sha: String::new(),
            tokenizer_sha: String::new(),
            model_sha: String::new(),
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            read_timeout: DEFAULT_READ_TIMEOUT,
            lock_timeout: DEFAULT_LOCK_TIMEOUT,
        }
    }

    #[cfg(any(test, feature = "loader-test-hooks"))]
    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    #[cfg(any(test, feature = "loader-test-hooks"))]
    pub fn with_cache_root(mut self, cache_root: PathBuf) -> Self {
        self.cache_root = cache_root;
        self
    }

    #[cfg(any(test, feature = "loader-test-hooks"))]
    pub fn with_hf_token(mut self, token: Option<String>) -> Self {
        self.hf_token = token;
        self
    }

    #[cfg(any(test, feature = "loader-test-hooks"))]
    pub fn with_test_pins(
        mut self,
        config_sha: String,
        tokenizer_sha: String,
        model_sha: String,
    ) -> Self {
        self.config_sha = config_sha;
        self.tokenizer_sha = tokenizer_sha;
        self.model_sha = model_sha;
        self
    }

    /// Directory where the three files will live. Exposed for tests so they
    /// can pre-stage `.partial` fixtures.
    #[cfg(any(test, feature = "loader-test-hooks"))]
    pub fn expected_cache_dir(&self) -> PathBuf {
        self.cache_dir_internal()
    }

    /// Internal cache-dir resolver — uses `sha256("<repo>@<revision>")[..12]`
    /// per design §4 (Issue 2 fix). Note: this depends only on the repo +
    /// revision identity, NOT on the file's own sha — so an empty
    /// `model_sha` (as in `for_tests()`) no longer collapses the prefix to
    /// `""`, and a future revision bump that touches only `config.json`
    /// still lands in a distinct cache dir.
    fn cache_dir_internal(&self) -> PathBuf {
        self.cache_root.join("fathomdb").join("embedders").join(model_sha_prefix())
    }
}

// ----- Public entry points --------------------------------------------------

/// Zero-arg production entry point. The only function a caller outside the
/// crate ever needs.
pub fn load_pinned_default_embedder() -> Result<LoadedWeights, EmbedderLoadError> {
    load_with_config_internal(LoaderConfig::production()?)
}

/// Test/integration entry point. Same body as the production path but takes
/// an explicit `LoaderConfig`. Gated behind `loader-test-hooks` so
/// downstream crates cannot substitute base URL / pinned shas in
/// production builds (ADR-0.7.1 scope guardrail #1).
#[cfg(any(test, feature = "loader-test-hooks"))]
pub fn load_with_config(cfg: LoaderConfig) -> Result<LoadedWeights, EmbedderLoadError> {
    load_with_config_internal(cfg)
}

fn load_with_config_internal(cfg: LoaderConfig) -> Result<LoadedWeights, EmbedderLoadError> {
    let cache_dir = cfg.cache_dir_internal();
    fs::create_dir_all(&cache_dir)
        .map_err(|source| EmbedderLoadError::CacheIoError { path: cache_dir.clone(), source })?;

    let mut events = Vec::new();
    let mut bytes_downloaded: u64 = 0;

    let files = [
        ("config.json", cfg.config_sha.clone()),
        ("tokenizer.json", cfg.tokenizer_sha.clone()),
        ("model.safetensors", cfg.model_sha.clone()),
    ];

    let mut paths = Vec::with_capacity(3);
    for (file_name, expected_sha) in &files {
        let final_path = cache_dir.join(file_name);

        // Fast path: cache already valid; no lock needed (design §10).
        if file_matches_sha(&final_path, expected_sha)? {
            events.push(EmbedderEvent::DefaultEmbedderCacheHit {
                file: (*file_name).to_string(),
                sha256: expected_sha.clone(),
                cache_path: final_path.clone(),
            });
            paths.push(final_path);
            continue;
        }

        // Cold or stale: lock, re-check, fetch.
        let (n, fetched_event) = fetch_under_lock(&cfg, &cache_dir, file_name, expected_sha)?;
        bytes_downloaded = bytes_downloaded.saturating_add(n);
        match fetched_event {
            FetchOutcome::Downloaded(ev) => events.push(ev),
            FetchOutcome::CacheHitAfterLock(ev) => events.push(ev),
        }
        paths.push(final_path);
    }

    Ok(LoadedWeights {
        config_json_path: paths[0].clone(),
        tokenizer_json_path: paths[1].clone(),
        model_safetensors_path: paths[2].clone(),
        bytes_downloaded,
        events,
    })
}

// ----- Internals ------------------------------------------------------------

enum FetchOutcome {
    Downloaded(EmbedderEvent),
    CacheHitAfterLock(EmbedderEvent),
}

fn fetch_under_lock(
    cfg: &LoaderConfig,
    cache_dir: &Path,
    file_name: &str,
    expected_sha: &str,
) -> Result<(u64, FetchOutcome), EmbedderLoadError> {
    let lock_path = cache_dir.join(".lock");
    let lock_file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|source| EmbedderLoadError::CacheIoError { path: lock_path.clone(), source })?;

    acquire_exclusive_with_timeout(&lock_file, cfg.lock_timeout)?;

    // RAII lock release on drop (fs2 unlocks on close).
    let _guard = LockGuard(&lock_file);

    let final_path = cache_dir.join(file_name);

    // Double-checked locking (design §10): another thread may have completed
    // the fetch while we were queued behind the lock.
    if file_matches_sha(&final_path, expected_sha)? {
        return Ok((
            0,
            FetchOutcome::CacheHitAfterLock(EmbedderEvent::DefaultEmbedderCacheHit {
                file: file_name.to_string(),
                sha256: expected_sha.to_string(),
                cache_path: final_path,
            }),
        ));
    }

    let partial_path = cache_dir.join(format!("{file_name}.partial"));
    let url = format!("{}/{}/resolve/{}/{}", cfg.base_url, HF_REPO, HF_REVISION, file_name);

    let start = Instant::now();
    let bytes = download_with_retries(cfg, &url, &partial_path, file_name)?;
    let duration_ms = start.elapsed().as_millis() as u64;

    // Verify before rename (design §5/§6).
    let observed_sha = sha256_file(&partial_path)
        .map_err(|source| EmbedderLoadError::CacheIoError { path: partial_path.clone(), source })?;
    if observed_sha != expected_sha {
        // Fail-closed: remove the partial (design §6).
        let _ = fs::remove_file(&partial_path);
        return Err(EmbedderLoadError::ChecksumMismatch {
            file: file_name.to_string(),
            expected: expected_sha.to_string(),
            observed: observed_sha,
        });
    }

    // Atomic rename (design §5).
    fs::rename(&partial_path, &final_path)
        .map_err(|source| EmbedderLoadError::CacheIoError { path: final_path.clone(), source })?;

    Ok((
        bytes,
        FetchOutcome::Downloaded(EmbedderEvent::DefaultEmbedderDownload {
            file: file_name.to_string(),
            url,
            bytes,
            sha256: observed_sha,
            cache_path: final_path,
            duration_ms,
        }),
    ))
}

fn acquire_exclusive_with_timeout(f: &File, timeout: Duration) -> Result<(), EmbedderLoadError> {
    let deadline = Instant::now() + timeout;
    loop {
        match f.try_lock_exclusive() {
            Ok(()) => return Ok(()),
            Err(_) => {
                if Instant::now() >= deadline {
                    return Err(EmbedderLoadError::LockTimeout(timeout));
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

fn download_with_retries(
    cfg: &LoaderConfig,
    url: &str,
    partial_path: &Path,
    file_name: &str,
) -> Result<u64, EmbedderLoadError> {
    let mut last_err: Option<EmbedderLoadError> = None;
    for attempt in 0..MAX_ATTEMPTS {
        match download_once(cfg, url, partial_path, file_name) {
            Ok(n) => return Ok(n),
            Err(e) => {
                // Fail-fast errors (4xx other than 408/429, CacheIoError,
                // ChecksumMismatch, LockTimeout) abort retries immediately
                // per design §2.
                if retry_decision(&e) == RetryDecision::FailFast {
                    return Err(e);
                }
                last_err = Some(e);
                if attempt + 1 < MAX_ATTEMPTS {
                    // Design §2: 1s, 2s, (4s) — for MAX_ATTEMPTS=3 that's
                    // 1s then 2s before the second and third tries.
                    let secs = 1u64 << attempt;
                    std::thread::sleep(Duration::from_secs(secs));
                }
            }
        }
    }
    Err(last_err.expect("at least one attempt"))
}

fn download_once(
    cfg: &LoaderConfig,
    url: &str,
    partial_path: &Path,
    file_name: &str,
) -> Result<u64, EmbedderLoadError> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(cfg.connect_timeout)
        .timeout_read(cfg.read_timeout)
        .build();

    // Resume support (design §2): if a `.partial` exists, request the suffix.
    let existing = fs::metadata(partial_path).map(|m| m.len()).unwrap_or(0);

    let mut req = agent.get(url);
    if let Some(token) = &cfg.hf_token {
        req = req.set("Authorization", &format!("Bearer {token}"));
    }
    if existing > 0 {
        req = req.set("Range", &format!("bytes={existing}-"));
    }

    let resp = req.call().map_err(|e| EmbedderLoadError::NetworkUnavailable {
        file: file_name.to_string(),
        source: Box::new(e),
    })?;

    let status = resp.status();
    if !(status == 200 || status == 206) {
        // Fold HTTP status failures into NetworkUnavailable so the retry
        // discriminator (`retry_decision`) classifies them uniformly.
        // The numeric status appears in the message so `extract_http_status`
        // can read it.
        return Err(EmbedderLoadError::NetworkUnavailable {
            file: file_name.to_string(),
            source: format!("HTTP status {status}").into(),
        });
    }

    // Non-resume path uses `create_new` per design §5 step 2: a stale
    // `.partial` from a crashed prior run that didn't pass sha verification
    // must NOT be silently appended-to. Issue 3 (FIX-1): if a stale partial
    // is present here, we are by definition not in the resume path
    // (`existing == 0` OR server returned 200, discarding the old bytes), so
    // the partial is stale and we delete-then-recreate. The alternative
    // would be to fail; we pick delete-and-retry because it self-heals.
    let mut f = if status == 206 && existing > 0 {
        let mut f = OpenOptions::new().write(true).open(partial_path).map_err(|source| {
            EmbedderLoadError::CacheIoError { path: partial_path.to_path_buf(), source }
        })?;
        f.seek(SeekFrom::End(0)).map_err(|source| EmbedderLoadError::CacheIoError {
            path: partial_path.to_path_buf(),
            source,
        })?;
        f
    } else {
        match OpenOptions::new().write(true).create_new(true).open(partial_path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // Stale partial from a crashed prior run. Design §5 step 2
                // forbids silent overwrite — clean up explicitly, then
                // retry create_new.
                fs::remove_file(partial_path).map_err(|source| {
                    EmbedderLoadError::CacheIoError { path: partial_path.to_path_buf(), source }
                })?;
                OpenOptions::new().write(true).create_new(true).open(partial_path).map_err(
                    |source| EmbedderLoadError::CacheIoError {
                        path: partial_path.to_path_buf(),
                        source,
                    },
                )?
            }
            Err(source) => {
                return Err(EmbedderLoadError::CacheIoError {
                    path: partial_path.to_path_buf(),
                    source,
                });
            }
        }
    };

    let mut reader = resp.into_reader();
    let mut buf = [0u8; 64 * 1024];
    let mut written: u64 = 0;
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                f.write_all(&buf[..n]).map_err(|source| EmbedderLoadError::CacheIoError {
                    path: partial_path.to_path_buf(),
                    source,
                })?;
                written += n as u64;
            }
            Err(source) => {
                return Err(EmbedderLoadError::NetworkUnavailable {
                    file: file_name.to_string(),
                    source: Box::new(source),
                });
            }
        }
    }

    f.sync_all().map_err(|source| EmbedderLoadError::CacheIoError {
        path: partial_path.to_path_buf(),
        source,
    })?;

    Ok(written)
}

fn file_matches_sha(path: &Path, expected: &str) -> Result<bool, EmbedderLoadError> {
    if !path.is_file() {
        return Ok(false);
    }
    let observed = sha256_file(path)
        .map_err(|source| EmbedderLoadError::CacheIoError { path: path.to_path_buf(), source })?;
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
