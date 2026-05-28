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
const MAX_ATTEMPTS: u32 = 3;
/// Default lock acquisition timeout (design §10).
const DEFAULT_LOCK_TIMEOUT: Duration = Duration::from_secs(120);

/// Env var consulted for the lock timeout override.
pub(crate) const ENV_LOCK_TIMEOUT: &str = "FATHOMDB_EMBEDDER_LOCK_TIMEOUT_SECS";

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

    #[error("HTTP error fetching {file}: status {status}")]
    HttpStatus { file: String, status: u16 },

    #[error("timed out acquiring embedder cache lock after {0:?}")]
    LockTimeout(Duration),
}

// ----- Loader configuration -------------------------------------------------

/// Configuration for the loader. Production code constructs this via
/// `LoaderConfig::production()` (called from `load_pinned_default_embedder`).
/// Tests construct via `LoaderConfig::for_tests()` and then override only the
/// surfaces explicitly designed for testing (base URL, cache root, pinned
/// shas, HF token).
///
/// **Scope guardrail**: this type is `pub` so the test integration file can
/// name it, but every setter is similarly only useful inside tests; in
/// production the loader is invoked exclusively through the zero-arg entry
/// point. None of the public surface accepts a caller-controlled URL,
/// repo, or model name.
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
    pub fn production() -> Result<Self, EmbedderLoadError> {
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

    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    pub fn with_cache_root(mut self, cache_root: PathBuf) -> Self {
        self.cache_root = cache_root;
        self
    }

    pub fn with_hf_token(mut self, token: Option<String>) -> Self {
        self.hf_token = token;
        self
    }

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
    pub fn expected_cache_dir(&self) -> PathBuf {
        // Use the first 12 hex of the pinned model sha so the cache key
        // changes whenever the underlying weights change (design §4).
        let prefix = &self.model_sha[..12.min(self.model_sha.len())];
        self.cache_root.join("fathomdb").join("embedders").join(prefix)
    }
}

// ----- Public entry points --------------------------------------------------

/// Zero-arg production entry point. The only function a caller outside the
/// crate ever needs.
pub fn load_pinned_default_embedder() -> Result<LoadedWeights, EmbedderLoadError> {
    load_with_config(LoaderConfig::production()?)
}

/// Test/integration entry point. Same body as the production path but takes
/// an explicit `LoaderConfig`. Kept `pub` so the loader integration tests can
/// pin it.
pub fn load_with_config(cfg: LoaderConfig) -> Result<LoadedWeights, EmbedderLoadError> {
    let cache_dir = cfg.expected_cache_dir();
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
                last_err = Some(e);
                if attempt + 1 < MAX_ATTEMPTS {
                    std::thread::sleep(Duration::from_millis(100 * (1 << attempt) as u64));
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
        return Err(EmbedderLoadError::HttpStatus { file: file_name.to_string(), status });
    }

    // Open in append mode for resume (206); truncate for 200 to start fresh.
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
        OpenOptions::new().create(true).write(true).truncate(true).open(partial_path).map_err(
            |source| EmbedderLoadError::CacheIoError { path: partial_path.to_path_buf(), source },
        )?
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
