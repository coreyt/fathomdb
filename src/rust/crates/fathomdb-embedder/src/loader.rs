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
///
/// Exposed as `pub` only under `cfg(any(test, feature = "loader-test-hooks"))`
/// so the integration test crate can reference it directly (avoiding the
/// drift hazard of a duplicated constant). Production callers see
/// `pub(crate)` and cannot reach this symbol from another crate.
#[cfg(any(test, feature = "loader-test-hooks"))]
pub const HF_REVISION: &str = "5c38ec7c405ec4b44b94cc5a9bb96e735b38267a";
#[cfg(not(any(test, feature = "loader-test-hooks")))]
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

/// Env var consulted for the lock timeout override (design §10).
pub(crate) const ENV_LOCK_TIMEOUT: &str = "FATHOMDB_EMBEDDER_LOCK_TIMEOUT_S";

/// Env var consulted for the HTTP connect timeout override (design §2).
pub(crate) const ENV_CONNECT_TIMEOUT: &str = "FATHOMDB_EMBEDDER_CONNECT_TIMEOUT_S";

/// Env var consulted for the HTTP read timeout override (design §2).
pub(crate) const ENV_READ_TIMEOUT: &str = "FATHOMDB_EMBEDDER_READ_TIMEOUT_S";

/// Parse a `u64`-seconds env var, returning the default on missing/invalid.
/// On invalid input emits a `stderr` warning (no panic, no `unwrap`).
fn parse_secs_env_or_default(var: &str, default: Duration) -> Duration {
    match std::env::var(var) {
        Ok(s) => match s.parse::<u64>() {
            Ok(n) => Duration::from_secs(n),
            Err(_) => {
                eprintln!(
                    "fathomdb-embedder: invalid value for {var} ({s:?}); falling back to default \
                     {default:?}"
                );
                default
            }
        },
        Err(_) => default,
    }
}

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

// `EmbedderEvent` lives at the crate root (`super::EmbedderEvent`) so the
// engine can reference it without enabling the `default-embedder` feature.
// Re-exported here for ergonomic use in loader.rs.
pub use super::EmbedderEvent;

/// Failure taxonomy (design §9). Engine-level mapping is owned by EU-5.
#[derive(Debug, Error)]
pub enum EmbedderLoadError {
    /// Network unavailable after retry exhaustion.
    ///
    /// `source` is widened from `ureq::Error` to `Box<dyn Error + Send + Sync>`
    /// so it can carry both real `ureq::Error` values from connect / status
    /// failures AND wrapped `io::Error`s from mid-stream response-body reads
    /// (ureq's `Transport` has no public constructor, so we cannot forge a
    /// `ureq::Error::Transport` from an `io::Error`). Design §9 reflects
    /// this shape.
    #[error("network unavailable after {attempts} attempts: {source}")]
    NetworkUnavailable {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
        attempts: u32,
    },

    #[error("checksum mismatch for {file:?}: expected {expected}, actual {actual}")]
    ChecksumMismatch { file: PathBuf, expected: String, actual: String },

    /// Pinned model config violates the protocol expected by the embedder
    /// (e.g. `hidden_size` does not match `DEFAULT_EMBEDDER_DIM`). This is
    /// distinct from `ModelDeserialize` (a parse failure) — the bytes
    /// parsed cleanly but their content disagreed with a hard-coded
    /// invariant. Always points at a deliberate model/version drift.
    #[error("model dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch { expected: u32, actual: u32 },

    #[error("cache I/O error at {path:?}: {source}")]
    CacheIoError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Model byte → `BertModel` parse failure (EU-4). Wraps the candle
    /// error verbatim so the engine layer (EU-5) can pattern-match on
    /// concrete variants when classifying.
    #[error("model deserialize: {source}")]
    ModelDeserialize {
        #[source]
        source: candle_core::Error,
    },

    /// `tokenizer.json` parse failure (EU-4). Wraps the tokenizers error
    /// verbatim. Boxed because `tokenizers::Error` is a
    /// `Box<dyn Error + Send + Sync>` alias, and we need a sized type.
    #[error("tokenizer load: {source}")]
    TokenizerLoad {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("timed out acquiring embedder cache lock at {lock_path:?} after {waited_s}s")]
    LockTimeout { lock_path: PathBuf, waited_s: u64 },
}

/// Per-attempt download error. This is the internal error type used by
/// `download_once`/`download_with_retries`: it preserves the concrete
/// `ureq::Error` for HTTP-layer failures and a raw `io::Error` for
/// mid-stream body reads. After retry exhaustion both are boxed into
/// `EmbedderLoadError::NetworkUnavailable.source` (design §9 widened
/// to `Box<dyn Error + Send + Sync>` for exactly this reason — see
/// `NetworkStreamIo` doc). Fatal cache I/O failures abort retries
/// immediately.
enum DownloadAttemptError {
    /// Network-class failure (connect, read, HTTP status). Classified into
    /// `RetryDecision` via `retry_decision_ureq`.
    Network(ureq::Error),
    /// Mid-stream read failure on the response body. ureq does not expose
    /// a public `Transport` constructor, so we cannot package this as a
    /// `ureq::Error::Transport` directly; instead we keep the raw
    /// `io::Error` and the retry loop maps it to the same retry-class as
    /// a `Transport`-level error per design §2 (read timeouts retry).
    NetworkStreamIo(std::io::Error),
    /// Cache I/O failure during the attempt (writing the `.partial`,
    /// opening, fsync, etc.). Always fail-fast.
    CacheIo { path: PathBuf, source: std::io::Error },
}

/// Whether a given network error should be retried within
/// `download_with_retries` (design §2). Connect failures, 5xx, read
/// timeouts, 408, and 429 are retryable; 4xx other than 408/429 fail fast.
fn retry_decision_ureq(err: &ureq::Error) -> RetryDecision {
    match err {
        ureq::Error::Status(code, _) => {
            if (500..=599).contains(code) || *code == 408 || *code == 429 {
                RetryDecision::Retry
            } else {
                RetryDecision::FailFast
            }
        }
        // Transport-level errors (DNS, connect, read timeout) always retry.
        ureq::Error::Transport(_) => RetryDecision::Retry,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetryDecision {
    Retry,
    FailFast,
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
    /// HF-hub root used by the design-§4 compat probe.
    ///
    /// `None` → production behavior: probe `$HF_HOME` (or
    /// `~/.cache/huggingface` if unset). `Some(p)` → probe only `p`.
    /// Tests use `Some(<tempdir>)` (or `Some(<nonexistent>)`) to keep
    /// the probe deterministic and prevent the user's actual HF cache
    /// from leaking into the test harness.
    hf_hub_root: Option<PathBuf>,
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
        let lock_timeout = parse_secs_env_or_default(ENV_LOCK_TIMEOUT, DEFAULT_LOCK_TIMEOUT);
        let connect_timeout =
            parse_secs_env_or_default(ENV_CONNECT_TIMEOUT, DEFAULT_CONNECT_TIMEOUT);
        let read_timeout = parse_secs_env_or_default(ENV_READ_TIMEOUT, DEFAULT_READ_TIMEOUT);
        Ok(Self {
            base_url: HF_BASE_URL.to_string(),
            cache_root,
            hf_token: std::env::var("HF_TOKEN").ok(),
            config_sha: CONFIG_JSON_SHA256.to_string(),
            tokenizer_sha: TOKENIZER_JSON_SHA256.to_string(),
            model_sha: MODEL_SAFETENSORS_SHA256.to_string(),
            connect_timeout,
            read_timeout,
            lock_timeout,
            hf_hub_root: None,
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
            // Default test posture: point hub probe at a guaranteed-empty
            // path so a developer's real `~/.cache/huggingface` cannot
            // shadow the mock server in the test harness.
            hf_hub_root: Some(PathBuf::from("/nonexistent-fathomdb-embedder-test-hub")),
        }
    }

    /// Override the HF-hub root for the design-§4 compat probe. Tests
    /// pass `Some(<tempdir>)` to verify the probe; production callers
    /// have no way to reach this setter (it is `loader-test-hooks`-gated).
    #[cfg(any(test, feature = "loader-test-hooks"))]
    pub fn with_hf_hub_root(mut self, root: Option<PathBuf>) -> Self {
        self.hf_hub_root = root;
        self
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

    /// Construct a `LoaderConfig` reading `connect_timeout`/`read_timeout`
    /// from the design-§2 env vars (with the standard defaults on
    /// missing/invalid). Exposed for tests that assert the env-override
    /// parsing path produces the expected `Duration` values.
    #[cfg(any(test, feature = "loader-test-hooks"))]
    pub fn for_tests_reading_timeout_env() -> Self {
        let connect_timeout =
            parse_secs_env_or_default(ENV_CONNECT_TIMEOUT, DEFAULT_CONNECT_TIMEOUT);
        let read_timeout = parse_secs_env_or_default(ENV_READ_TIMEOUT, DEFAULT_READ_TIMEOUT);
        let mut cfg = Self::for_tests();
        cfg.connect_timeout = connect_timeout;
        cfg.read_timeout = read_timeout;
        cfg
    }

    /// Returns the loader's effective HTTP connect timeout. Exposed for
    /// tests asserting env-override behavior.
    #[cfg(any(test, feature = "loader-test-hooks"))]
    pub fn connect_timeout(&self) -> Duration {
        self.connect_timeout
    }

    /// Returns the loader's effective HTTP read timeout. Exposed for
    /// tests asserting env-override behavior.
    #[cfg(any(test, feature = "loader-test-hooks"))]
    pub fn read_timeout(&self) -> Duration {
        self.read_timeout
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

        // HF-hub read-only compat probe (design §4): if the user already
        // has the file under `$HF_HOME/hub/...` and its sha matches the
        // pinned constant, copy/hard-link it into our cache and short-circuit
        // network. The HF-hub layout is NEVER written to.
        if let Some(hub_path) = hf_hub_candidate_path(&cfg, file_name) {
            if file_matches_sha(&hub_path, expected_sha)? {
                materialize_from_hf_hub(&hub_path, &final_path)?;
                events.push(EmbedderEvent::DefaultEmbedderCacheHit {
                    file: (*file_name).to_string(),
                    sha256: expected_sha.clone(),
                    cache_path: final_path.clone(),
                });
                paths.push(final_path);
                continue;
            }
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

    acquire_exclusive_with_timeout(&lock_file, &lock_path, cfg.lock_timeout)?;

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
            file: partial_path.clone(),
            expected: expected_sha.to_string(),
            actual: observed_sha,
        });
    }

    // Atomic rename (design §5).
    fs::rename(&partial_path, &final_path)
        .map_err(|source| EmbedderLoadError::CacheIoError { path: final_path.clone(), source })?;

    // fsync the parent directory so the rename survives a power loss
    // between the rename and the next implicit fsync. POSIX only;
    // Windows journaling already covers this (design §5 step 6).
    #[cfg(unix)]
    fsync_parent_dir(&final_path)?;

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

/// `fsync` the parent directory of `path`. POSIX only — used after a
/// rename to make the directory-entry change durable across a power loss
/// before the next file-level fsync (design §5 step 6).
#[cfg(unix)]
fn fsync_parent_dir(path: &Path) -> Result<(), EmbedderLoadError> {
    if let Some(parent) = path.parent() {
        let dir = File::open(parent).map_err(|source| EmbedderLoadError::CacheIoError {
            path: parent.to_path_buf(),
            source,
        })?;
        dir.sync_all().map_err(|source| EmbedderLoadError::CacheIoError {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    Ok(())
}

/// Compute the HF-hub layout path for `file_name` at the pinned repo +
/// revision: `$HF_HOME/hub/models--<owner>--<repo>/snapshots/<rev>/<file>`.
/// `$HF_HOME` defaults to `~/.cache/huggingface`. Returns `None` if no
/// home directory can be resolved (and `HF_HOME` is unset).
fn hf_hub_candidate_path(cfg: &LoaderConfig, file_name: &str) -> Option<PathBuf> {
    let hf_home = if let Some(root) = &cfg.hf_hub_root {
        root.clone()
    } else {
        match std::env::var_os("HF_HOME") {
            Some(p) => PathBuf::from(p),
            None => dirs::home_dir()?.join(".cache").join("huggingface"),
        }
    };
    let repo_encoded = format!("models--{}", HF_REPO.replace('/', "--"));
    Some(hf_home.join("hub").join(repo_encoded).join("snapshots").join(HF_REVISION).join(file_name))
}

/// Copy `src` into `dst`, preferring a POSIX hard-link when possible
/// (same filesystem; saves disk + lets the kernel share inodes). Falls
/// back to a byte copy on any error from `hard_link` (different
/// filesystem, permission, Windows, etc.). The HF-hub source is never
/// modified. Surfaces failures as `CacheIoError`.
fn materialize_from_hf_hub(src: &Path, dst: &Path) -> Result<(), EmbedderLoadError> {
    // Hardlink first; copy as fallback. `fs::hard_link` errors on
    // cross-filesystem and on Windows for non-NTFS volumes; either way the
    // byte copy is correct.
    #[cfg(unix)]
    {
        if fs::hard_link(src, dst).is_ok() {
            return Ok(());
        }
    }
    fs::copy(src, dst)
        .map(|_| ())
        .map_err(|source| EmbedderLoadError::CacheIoError { path: dst.to_path_buf(), source })
}

fn acquire_exclusive_with_timeout(
    f: &File,
    lock_path: &Path,
    timeout: Duration,
) -> Result<(), EmbedderLoadError> {
    let deadline = Instant::now() + timeout;
    loop {
        match f.try_lock_exclusive() {
            Ok(()) => return Ok(()),
            Err(e) => {
                // Only `WouldBlock` means "another holder; retry". Real
                // I/O errors (permission denied, EIO, ...) are fatal —
                // surface immediately rather than polling until timeout.
                if e.kind() != std::io::ErrorKind::WouldBlock {
                    return Err(EmbedderLoadError::CacheIoError {
                        path: lock_path.to_path_buf(),
                        source: e,
                    });
                }
                if Instant::now() >= deadline {
                    return Err(EmbedderLoadError::LockTimeout {
                        lock_path: lock_path.to_path_buf(),
                        waited_s: timeout.as_secs(),
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

fn download_with_retries(
    cfg: &LoaderConfig,
    url: &str,
    partial_path: &Path,
    _file_name: &str,
) -> Result<u64, EmbedderLoadError> {
    let mut last_net_err: Option<Box<dyn std::error::Error + Send + Sync>> = None;
    let mut completed_attempts: u32 = 0;
    for attempt in 0..MAX_ATTEMPTS {
        completed_attempts = attempt + 1;
        match download_once(cfg, url, partial_path) {
            Ok(n) => return Ok(n),
            Err(DownloadAttemptError::CacheIo { path, source }) => {
                // Fail-fast: cache I/O is not a transient network condition.
                return Err(EmbedderLoadError::CacheIoError { path, source });
            }
            Err(DownloadAttemptError::Network(e)) => {
                if retry_decision_ureq(&e) == RetryDecision::FailFast {
                    return Err(EmbedderLoadError::NetworkUnavailable {
                        source: Box::new(e),
                        attempts: completed_attempts,
                    });
                }
                last_net_err = Some(Box::new(e));
                if attempt + 1 < MAX_ATTEMPTS {
                    // Design §2: 1s, 2s, (4s) — for MAX_ATTEMPTS=3 that's
                    // 1s then 2s before the second and third tries.
                    let secs = 1u64 << attempt;
                    std::thread::sleep(Duration::from_secs(secs));
                }
            }
            Err(DownloadAttemptError::NetworkStreamIo(io)) => {
                // Read-timeout / connection-reset mid-body: design §2
                // classes as a retryable read error. With the §9
                // `NetworkUnavailable.source` widened to a boxed dyn
                // Error, we now box the raw io::Error directly — no need
                // to drop it onto the CacheIoError path.
                last_net_err = Some(Box::new(io));
                if attempt + 1 < MAX_ATTEMPTS {
                    let secs = 1u64 << attempt;
                    std::thread::sleep(Duration::from_secs(secs));
                }
            }
        }
    }
    // All attempts exhausted. The boxed `source` carries whichever of
    // `ureq::Error` or mid-stream `io::Error` was observed last.
    Err(EmbedderLoadError::NetworkUnavailable {
        source: last_net_err.expect("at least one retryable attempt produced an error"),
        attempts: completed_attempts,
    })
}

fn download_once(
    cfg: &LoaderConfig,
    url: &str,
    partial_path: &Path,
) -> Result<u64, DownloadAttemptError> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(cfg.connect_timeout)
        .timeout_read(cfg.read_timeout)
        // Design §2: explicit redirect budget (≥3). ureq's default is 5,
        // which satisfies the design floor incidentally; we set it
        // deliberately so the value is part of the contract.
        .redirects(3)
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

    let resp = req.call().map_err(DownloadAttemptError::Network)?;

    let status = resp.status();
    if !(status == 200 || status == 206) {
        // Convert into a synthetic ureq::Status error so it goes through
        // the same retry-decision path as a directly-surfaced HTTP error.
        // `resp.into()` builds the Status variant with the response payload.
        return Err(DownloadAttemptError::Network(ureq::Error::Status(status, resp)));
    }

    let mk_io = |source: std::io::Error| DownloadAttemptError::CacheIo {
        path: partial_path.to_path_buf(),
        source,
    };

    // Non-resume path uses `create_new` per design §5 step 2: a stale
    // `.partial` from a crashed prior run that didn't pass sha verification
    // must NOT be silently appended-to. Issue 3 (FIX-1): if a stale partial
    // is present here, we are by definition not in the resume path
    // (`existing == 0` OR server returned 200, discarding the old bytes), so
    // the partial is stale and we delete-then-recreate. The alternative
    // would be to fail; we pick delete-and-retry because it self-heals.
    let mut f = if status == 206 && existing > 0 {
        let mut f = OpenOptions::new().write(true).open(partial_path).map_err(mk_io)?;
        f.seek(SeekFrom::End(0)).map_err(mk_io)?;
        f
    } else {
        match OpenOptions::new().write(true).create_new(true).open(partial_path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // Stale partial from a crashed prior run. Design §5 step 2
                // forbids silent overwrite — clean up explicitly, then
                // retry create_new.
                fs::remove_file(partial_path).map_err(mk_io)?;
                OpenOptions::new().write(true).create_new(true).open(partial_path).map_err(mk_io)?
            }
            Err(source) => {
                return Err(mk_io(source));
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
                f.write_all(&buf[..n]).map_err(mk_io)?;
                written += n as u64;
            }
            Err(source) => {
                // Mid-stream read failure on the response body. ureq's
                // `Transport` has no public constructor so we cannot
                // forge a `ureq::Error::Transport`; instead we carry the
                // raw `io::Error` through a dedicated variant so the
                // retry loop treats it as a read-timeout-class retryable
                // failure (design §2).
                return Err(DownloadAttemptError::NetworkStreamIo(source));
            }
        }
    }

    f.sync_all().map_err(mk_io)?;

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
