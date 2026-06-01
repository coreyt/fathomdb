//! Corpus-driven test harness (0.7.2 PR-5).
//!
//! Evolves the Pack-4 seed module [`corpus_subset`] into a first-class,
//! reusable fixture so future perf + behavior tests can spin up a
//! real-corpus engine in one line:
//!
//! ```ignore
//! let fx = CorpusFixture::medium().with_real_embedder();
//! let Some((_dir, engine)) = fx.open_or_skip() else { return };
//! let report = fx.ingest_into(&engine);
//! let qs = fx.query_set(50, 0xC0FFEE);
//! fx.assert_search_returns_non_empty_for_each(&engine, &qs);
//! ```
//!
//! ## What it adds over `corpus_subset`
//!
//! - [`CorpusFixture`] — deterministic subset selection (`small` / `medium`
//!   / `full`, plus the Pack-4-compatible `per_source` and an explicit
//!   `from_docs`), an embedder toggle (synthetic [`VaryingEmbedder`] vs the
//!   shipped real BGE behind `default-embedder`), one-line ingest, a
//!   held-out [`query_set`](CorpusFixture::query_set) following the EU-0
//!   §1.2 methodology, and assertable invariants.
//! - A **per-(model, subset) embedding cache** on local disk (gitignored
//!   under `data/corpus-data/.cache/embeddings/`) so the inner dev loop
//!   does not re-embed thousands of docs on every run. The cache file is
//!   keyed — and INVALIDATED — by the embedder identity AND the resolved
//!   subset's doc-id manifest; either changing rebuilds it.
//!
//! Both the corpus-absent and `default-embedder`-off paths SKIP gracefully
//! (mirroring [`corpus_subset::load_subset_or_skip`]): construct the
//! fixture, then bail on [`CorpusFixture::open_or_skip`] /
//! [`CorpusFixture::skip_reason`].
//!
//! This module re-uses [`corpus_subset::ingest`] verbatim for the write
//! path, so the batched `PreparedWrite::Node` / `::Edge` pattern
//! (`4a95cfd` regression-prevention) is preserved by construction.

#![allow(dead_code)] // helpers are referenced by sibling integration tests; cargo lints each in isolation

#[path = "corpus_subset.rs"]
mod corpus_subset;

// Re-export the seed surface so harness consumers have a single `use`.
// Each test binary includes this module via `#[path]` and uses only a
// subset, so individual re-exports look "unused" per-binary.
#[allow(unused_imports)]
pub use corpus_subset::{
    extract_ground_truth_queries, load_chain_docs, load_chains_or_skip, load_subset_or_skip,
    repo_root, salient_word, Chain, Doc, IRQuery, VaryingEmbedder, CORPUS_DIM, VECTOR_KIND,
};

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::Engine;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

/// In-memory embed cache: text-sha256 digest -> embedding vector.
type VecCache = HashMap<[u8; 32], Vec<f32>>;

/// On-disk cache blob layout version. Bump on any format change so a
/// stale blob from an older harness is treated as a miss, not trusted.
const CACHE_FORMAT_VERSION: u32 = 1;
/// Tool-version stamp embedded in the cache sidecar (for diagnostics).
const TOOL_VERSION: &str = "corpus-harness/1";
/// Default revision for the synthetic embedder — matches the historical
/// `VaryingEmbedder::new` identity so Pack-4 tests migrate with no
/// behavior change.
const SYNTHETIC_REVISION: &str = "corpus-pack-4";

// ── Public value types ──────────────────────────────────────────────────

/// Outcome of an [`CorpusFixture::ingest_into`] call.
#[derive(Clone, Debug)]
pub struct IngestReport {
    /// Canonical nodes written (one per doc).
    pub nodes: usize,
    /// Canonical edges written (one per in-subset parent link).
    pub edges: usize,
    /// Edge count broken down by relation kind.
    pub edges_by_relation: BTreeMap<String, usize>,
    /// `true` when a valid cache file was found and loaded (warm run);
    /// `false` when the cache was cold or stale and had to be rebuilt.
    pub embed_cache_hit: bool,
    /// Number of distinct texts embedded LIVE (cache misses) during this
    /// ingest. Zero on a fully warm run.
    pub embedded_live: usize,
    /// `Some(reason)` when the cache was a miss at load — `"cold"` (no
    /// file), a partial-file reason, or a staleness reason (e.g.
    /// `"identity mismatch"`). `None` on a trusted warm hit. Mirrors the
    /// emitted `CORPUS_CACHE_MISS` line.
    pub cache_miss_reason: Option<String>,
    /// Path of the cache blob this fixture reads/writes.
    pub cache_path: PathBuf,
}

/// A held-out query synthesized from a corpus doc per the EU-0 §1.2
/// methodology (title-or-lead-sentence, never the verbatim body). Carries
/// the source doc so consumers can exclude the trivially-self-retrieving
/// target before measuring recall.
#[derive(Clone, Debug)]
pub struct HeldOutQuery {
    /// The query text fed to `engine.search`.
    pub text: String,
    /// doc_id of the doc this query was synthesized from.
    pub target_doc_id: String,
    /// Body of the source doc — the exclusion target.
    pub target_body: String,
}

// ── Fixture ───────────────────────────────────────────────────────────--

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EmbedderKind {
    Synthetic,
    Real,
}

/// A reusable real-corpus test fixture. Cheap to construct (resolves the
/// doc subset eagerly; builds the embedder + cache lazily on first
/// [`open_engine`](Self::open_engine) / [`ingest_into`](Self::ingest_into)).
pub struct CorpusFixture {
    label: String,
    docs: Vec<Doc>,
    corpus_absent: bool,
    kind: EmbedderKind,
    synthetic_revision: String,
    /// `true` when `with_real_embedder` was requested but the
    /// `default-embedder` feature is off — drives a graceful SKIP.
    real_unavailable: bool,
    cache_dir_override: Option<PathBuf>,
    embedder: OnceLock<Arc<CachingEmbedder>>,
}

impl CorpusFixture {
    // -- constructors --

    /// ~100 docs, deterministic: the corpus globally sorted by `doc_id`,
    /// first 100. Stable across commits on the same checkout (modulo
    /// corpus-content changes).
    pub fn small() -> Self {
        Self::global("small", 100)
    }

    /// ~1000 docs, deterministic (global sort by `doc_id`, first 1000).
    pub fn medium() -> Self {
        Self::global("medium", 1000)
    }

    /// All docs (~7667), deterministic (global sort by `doc_id`).
    pub fn full() -> Self {
        Self::global("full", usize::MAX)
    }

    /// Pack-4-compatible selection: first `per_source` docs of EACH source
    /// JSONL (sorted by `doc_id`), concatenated — identical to
    /// [`corpus_subset::load_subset_or_skip`]. Used by the migrated
    /// `corpus_vector` / `corpus_fts` tests so their doc set (and therefore
    /// pass/fail) is unchanged.
    pub fn per_source(per_source: usize) -> Self {
        match load_subset_or_skip(per_source) {
            Some(docs) => Self::from_resolved(format!("per_source_{per_source}"), docs),
            None => Self::absent(format!("per_source_{per_source}")),
        }
    }

    /// Build a fixture from an explicit doc set (e.g. chain docs resolved
    /// via [`corpus_subset::load_chain_docs`]). The `label` participates in
    /// the cache key.
    pub fn from_docs(label: impl Into<String>, docs: Vec<Doc>) -> Self {
        Self::from_resolved(label.into(), docs)
    }

    fn global(label: &str, take: usize) -> Self {
        match load_subset_or_skip(usize::MAX) {
            Some(mut docs) => {
                docs.sort_by(|a, b| a.doc_id.cmp(&b.doc_id));
                docs.dedup_by(|a, b| a.doc_id == b.doc_id);
                docs.truncate(take);
                Self::from_resolved(label.to_string(), docs)
            }
            None => Self::absent(label.to_string()),
        }
    }

    fn from_resolved(label: String, docs: Vec<Doc>) -> Self {
        Self {
            label,
            docs,
            corpus_absent: false,
            kind: EmbedderKind::Synthetic,
            synthetic_revision: SYNTHETIC_REVISION.to_string(),
            real_unavailable: false,
            cache_dir_override: None,
            embedder: OnceLock::new(),
        }
    }

    fn absent(label: String) -> Self {
        Self {
            label,
            docs: Vec::new(),
            corpus_absent: true,
            kind: EmbedderKind::Synthetic,
            synthetic_revision: SYNTHETIC_REVISION.to_string(),
            real_unavailable: false,
            cache_dir_override: None,
            embedder: OnceLock::new(),
        }
    }

    // -- builder toggles --

    /// Use the shipped real BGE embedder. Requires the `default-embedder`
    /// Cargo feature; when it is off this flags the fixture as
    /// `real_unavailable` so [`open_or_skip`](Self::open_or_skip) /
    /// [`skip_reason`](Self::skip_reason) SKIP gracefully.
    #[must_use]
    pub fn with_real_embedder(mut self) -> Self {
        self.kind = EmbedderKind::Real;
        self.real_unavailable = !cfg!(feature = "default-embedder");
        self.embedder = OnceLock::new();
        self
    }

    /// Use the always-available dense-isotropic synthetic embedder
    /// ([`VaryingEmbedder`]). This is the default.
    #[must_use]
    pub fn with_synthetic_embedder(mut self) -> Self {
        self.kind = EmbedderKind::Synthetic;
        self.real_unavailable = false;
        self.embedder = OnceLock::new();
        self
    }

    /// Test seam: override the synthetic embedder's identity revision
    /// WITHOUT touching the immutable production `EmbedderIdentity`
    /// contract. Vectors are unchanged; only the cache key shifts. Used by
    /// the cache-invalidation test to prove an identity change rebuilds the
    /// cache.
    #[must_use]
    pub fn with_synthetic_revision(mut self, revision: impl Into<String>) -> Self {
        self.kind = EmbedderKind::Synthetic;
        self.real_unavailable = false;
        self.synthetic_revision = revision.into();
        self.embedder = OnceLock::new();
        self
    }

    /// Test seam: override the cache directory (defaults to
    /// `data/corpus-data/.cache/embeddings/`, or `$FATHOMDB_CORPUS_CACHE_DIR`).
    #[must_use]
    pub fn with_cache_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cache_dir_override = Some(dir.into());
        self.embedder = OnceLock::new();
        self
    }

    // -- introspection / SKIP --

    /// Resolved doc set (deterministic, already filtered by the chosen
    /// constructor).
    pub fn docs(&self) -> &[Doc] {
        &self.docs
    }

    /// `Some(reason)` when this fixture cannot run — corpus absent on disk,
    /// or `with_real_embedder` requested without the `default-embedder`
    /// feature. Callers should emit a `SKIP:` line and return.
    pub fn skip_reason(&self) -> Option<String> {
        if self.corpus_absent {
            return Some(format!(
                "corpus not present on disk (label={}) — run tests/corpus/scripts/acquire_*.py first",
                self.label
            ));
        }
        if self.real_unavailable {
            return Some(format!(
                "with_real_embedder() requires the `default-embedder` feature (label={})",
                self.label
            ));
        }
        if self.docs.is_empty() {
            return Some(format!("fixture resolved 0 docs (label={})", self.label));
        }
        None
    }

    // -- engine + ingest --

    /// Open a fresh temp engine wired with this fixture's embedder and the
    /// canonical `doc` vector kind. Panics if the fixture is unavailable —
    /// prefer [`open_or_skip`](Self::open_or_skip) at the top of a test.
    pub fn open_engine(&self) -> (TempDir, Engine) {
        assert!(
            self.skip_reason().is_none(),
            "open_engine on unavailable fixture: {:?}",
            self.skip_reason()
        );
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("corpus.sqlite");
        let embedder = self.ensure_embedder();
        let opened = Engine::open_with_embedder_for_test(&path, embedder as Arc<dyn Embedder>)
            .expect("open engine with corpus-harness embedder");
        opened.engine.configure_vector_kind_for_test(VECTOR_KIND).expect("configure vector kind");
        (dir, opened.engine)
    }

    /// Like [`open_engine`](Self::open_engine) but returns `None` (after
    /// emitting a `SKIP:` line) when the fixture is unavailable. Mirrors
    /// the [`corpus_subset::load_subset_or_skip`] ergonomics.
    pub fn open_or_skip(&self) -> Option<(TempDir, Engine)> {
        if let Some(reason) = self.skip_reason() {
            eprintln!("SKIP: {reason}");
            return None;
        }
        Some(self.open_engine())
    }

    /// Ingest the resolved doc set into `engine` via the batched
    /// [`corpus_subset::ingest`] path (nodes then edges, drained), then
    /// persist the embedding cache. The engine MUST have been opened by
    /// this same fixture (so it shares the caching embedder); otherwise the
    /// cache cannot capture the projection embeds.
    pub fn ingest_into(&self, engine: &Engine) -> IngestReport {
        let embedder = self.ensure_embedder();
        let (nodes, edges, edges_by_relation) = corpus_subset::ingest(engine, &self.docs);
        // Projection has fully drained inside `ingest`, so every body has
        // been embedded through the caching wrapper by now.
        let (cache_hit, live) = embedder.persist();
        IngestReport {
            nodes,
            edges,
            edges_by_relation,
            embed_cache_hit: cache_hit,
            embedded_live: live,
            cache_miss_reason: embedder.miss_reason.clone(),
            cache_path: embedder.cache_path.clone(),
        }
    }

    /// Held-out query set (EU-0 §1.2): deterministically shuffle the doc
    /// pool with `seed`, synthesize a title-or-lead query per doc (never
    /// the verbatim body), and return up to `n` of them. Each query carries
    /// its source doc for target exclusion at recall time.
    pub fn query_set(&self, n: usize, seed: u64) -> Vec<HeldOutQuery> {
        if self.docs.is_empty() {
            return Vec::new();
        }
        let mut indices: Vec<usize> = (0..self.docs.len()).collect();
        // Deterministic Fisher-Yates so queries spread across sources
        // rather than clustering at the head of the sorted list.
        let mut rng = SplitMix64::new(seed);
        for i in (1..indices.len()).rev() {
            let j = rng.next_in(i + 1);
            indices.swap(i, j);
        }
        let mut out = Vec::with_capacity(n.min(self.docs.len()));
        for &idx in &indices {
            if out.len() >= n {
                break;
            }
            let doc = &self.docs[idx];
            if let Some(text) = synth_query(doc) {
                out.push(HeldOutQuery {
                    text,
                    target_doc_id: doc.doc_id.clone(),
                    target_body: doc.body.clone(),
                });
            }
        }
        out
    }

    // -- invariants --

    /// Assert `vector_default` has at least one vec0 row per ingested
    /// non-empty-body doc. (Chunked long bodies produce MORE rows, so the
    /// contract is `>=`, matching the Pack-4 `corpus_vector` gate.) Drains
    /// the engine first; opens a read-only connection without closing.
    pub fn assert_vec0_row_count_matches_ingest(&self, engine: &Engine) {
        engine.drain(15_000).expect("drain before vec0 count");
        let conn = open_readonly(engine.path());
        let vec_count: i64 = conn
            .query_row("SELECT count(*) FROM vector_default", [], |row| row.get(0))
            .expect("count vector_default");
        let expected = self.docs.iter().filter(|d| !d.body.trim().is_empty()).count() as i64;
        assert!(
            vec_count >= expected,
            "vector_default has {vec_count} rows but {expected} non-empty-body docs were ingested \
             — projection apparently dropped some; vector path is NOT wired end-to-end"
        );
    }

    /// Assert the FTS5 `search_index` shadow table is non-empty after
    /// ingest. Drains first; read-only peek without closing.
    pub fn assert_fts_index_populated(&self, engine: &Engine) {
        engine.drain(15_000).expect("drain before fts count");
        let conn = open_readonly(engine.path());
        let fts_count: i64 = conn
            .query_row("SELECT count(*) FROM search_index", [], |row| row.get(0))
            .expect("count search_index");
        assert!(fts_count > 0, "search_index is empty after ingest — FTS path is NOT wired");
    }

    /// Assert `engine.search` returns at least one result for every query
    /// in `qs`.
    pub fn assert_search_returns_non_empty_for_each(&self, engine: &Engine, qs: &[HeldOutQuery]) {
        assert!(!qs.is_empty(), "assert_search_returns_non_empty_for_each: empty query set");
        for q in qs {
            let result = engine
                .search(&q.text)
                .unwrap_or_else(|e| panic!("search failed for query {:?}: {e:?}", q.target_doc_id));
            assert!(
                !result.results.is_empty(),
                "engine.search returned empty for query from doc {} (text={:?})",
                q.target_doc_id,
                q.text
            );
        }
    }

    // -- internals --

    fn ensure_embedder(&self) -> Arc<CachingEmbedder> {
        self.embedder
            .get_or_init(|| {
                let inner: Arc<dyn Embedder> = self.build_inner_embedder();
                let identity = inner.identity();
                let cache_dir = self.resolve_cache_dir();
                Arc::new(CachingEmbedder::load(inner, identity, cache_dir, &self.label, &self.docs))
            })
            .clone()
    }

    fn build_inner_embedder(&self) -> Arc<dyn Embedder> {
        match self.kind {
            EmbedderKind::Synthetic => Arc::new(VaryingEmbedder::with_identity(
                "varying",
                &self.synthetic_revision,
                CORPUS_DIM,
            )),
            EmbedderKind::Real => build_real_embedder(),
        }
    }

    fn resolve_cache_dir(&self) -> PathBuf {
        if let Some(dir) = &self.cache_dir_override {
            return dir.clone();
        }
        if let Ok(env_dir) = std::env::var("FATHOMDB_CORPUS_CACHE_DIR") {
            if !env_dir.is_empty() {
                return PathBuf::from(env_dir);
            }
        }
        let root = repo_root().unwrap_or_else(|| PathBuf::from("."));
        root.join("data/corpus-data/.cache/embeddings")
    }
}

#[cfg(feature = "default-embedder")]
fn build_real_embedder() -> Arc<dyn Embedder> {
    // PR-9 landed engine-side embed serialization, so the harness needs NO
    // Mutex wrapper of its own (unlike eu7_real_corpus_ac.rs, whose
    // SerializedBge holdover is a noted PR-6/PR-7 follow-up). Cache hits
    // never touch the candle forward path at all.
    Arc::new(fathomdb_embedder::CandleBgeEmbedder::new().expect("construct real bge embedder"))
}

#[cfg(not(feature = "default-embedder"))]
fn build_real_embedder() -> Arc<dyn Embedder> {
    unreachable!("with_real_embedder is gated off via skip_reason when default-embedder is absent");
}

// ── Caching embedder ──────────────────────────────────────────────────--

/// Wraps any [`Embedder`] with an on-disk, per-(model, subset) vector
/// cache keyed by `sha256(identity || subset_label || doc-id manifest)`.
/// Serves repeat embeds of the same text from memory; persists the full
/// embed set deterministically (sorted by text digest) so two equivalent
/// constructions produce byte-identical cache files.
struct CachingEmbedder {
    inner: Arc<dyn Embedder>,
    identity: EmbedderIdentity,
    /// text-sha256 -> vector
    cache: RwLock<VecCache>,
    cache_path: PathBuf,
    meta_path: PathBuf,
    doc_manifest_sha: String,
    subset_label: String,
    /// `true` if a valid cache file was loaded at construction.
    loaded_from_disk: bool,
    /// `Some(reason)` when construction was a cache miss (cold/partial/
    /// stale); `None` on a trusted hit. Observable counterpart to the
    /// `CORPUS_CACHE_MISS` log line so tests can lock the loud-miss
    /// contract without scraping stderr.
    miss_reason: Option<String>,
    /// distinct texts embedded live (misses) since construction.
    live_misses: AtomicUsize,
}

impl CachingEmbedder {
    fn load(
        inner: Arc<dyn Embedder>,
        identity: EmbedderIdentity,
        cache_dir: PathBuf,
        subset_label: &str,
        docs: &[Doc],
    ) -> Self {
        let doc_manifest_sha = doc_manifest_sha(docs);
        let key = cache_key(&identity, subset_label, &doc_manifest_sha);
        let cache_path = cache_dir.join(format!("{key}.bin"));
        let meta_path = cache_dir.join(format!("{key}.meta.json"));

        // Every miss — cold (no file), partial (blob xor sidecar), or
        // stale/corrupt — is LOUD: a `CORPUS_CACHE_MISS reason=…` line plus
        // an observable `miss_reason` on the returned embedder. A miss never
        // fails; we fall through to live embedding.
        let (cache, loaded, miss_reason) =
            match read_cache(&cache_path, &meta_path, &identity, &doc_manifest_sha) {
                Ok(Some(entries)) => (entries, true, None),
                Ok(None) => (HashMap::new(), false, Some("cold".to_string())),
                Err(reason) => (HashMap::new(), false, Some(reason)),
            };
        if let Some(reason) = &miss_reason {
            eprintln!(
                "CORPUS_CACHE_MISS reason={reason} label={subset_label} path={}",
                cache_path.display()
            );
        }

        Self {
            inner,
            identity,
            cache: RwLock::new(cache),
            cache_path,
            meta_path,
            doc_manifest_sha,
            subset_label: subset_label.to_string(),
            loaded_from_disk: loaded,
            miss_reason,
            live_misses: AtomicUsize::new(0),
        }
    }

    /// Persist the current embed set atomically. Returns
    /// `(loaded_from_disk, live_miss_count)`. A no-op rewrite on a fully
    /// warm run (loaded + zero misses) is skipped so warm runs do no disk
    /// I/O; otherwise the blob is written deterministically.
    fn persist(&self) -> (bool, usize) {
        let live = self.live_misses.load(Ordering::Relaxed);
        if self.loaded_from_disk && live == 0 {
            return (true, 0);
        }
        if let Err(e) = self.write_cache() {
            eprintln!(
                "CORPUS_CACHE_WRITE_FAIL reason={e} label={} path={}",
                self.subset_label,
                self.cache_path.display()
            );
        }
        (self.loaded_from_disk, live)
    }

    fn write_cache(&self) -> Result<(), String> {
        if let Some(parent) = self.cache_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
        }
        let dim = self.identity.dimension as usize;
        let guard = self.cache.read().expect("cache read lock");

        // Deterministic order: sort entries by their text digest.
        let mut keys: Vec<&[u8; 32]> = guard.keys().collect();
        keys.sort_unstable();

        let mut blob: Vec<u8> = Vec::with_capacity(12 + keys.len() * (32 + dim * 4));
        blob.extend_from_slice(&CACHE_FORMAT_VERSION.to_le_bytes());
        blob.extend_from_slice(&(self.identity.dimension).to_le_bytes());
        blob.extend_from_slice(&(keys.len() as u32).to_le_bytes());
        for k in &keys {
            let v = &guard[*k];
            debug_assert_eq!(v.len(), dim, "cached vector dim mismatch");
            blob.extend_from_slice(&k[..]);
            for f in v {
                blob.extend_from_slice(&f.to_le_bytes());
            }
        }
        let entry_count = keys.len();
        drop(guard);

        let meta = serde_json::json!({
            "format_version": CACHE_FORMAT_VERSION,
            "tool_version": TOOL_VERSION,
            "identity": {
                "name": self.identity.name,
                "revision": self.identity.revision,
                "dimension": self.identity.dimension,
            },
            "subset_label": self.subset_label,
            "doc_manifest_sha": self.doc_manifest_sha,
            "dim": self.identity.dimension,
            "entry_count": entry_count,
        });
        let meta_bytes = serde_json::to_vec_pretty(&meta).map_err(|e| e.to_string())?;

        atomic_write(&self.cache_path, &blob)?;
        atomic_write(&self.meta_path, &meta_bytes)?;
        Ok(())
    }
}

impl Embedder for CachingEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, input: &str) -> Result<Vector, EmbedderError> {
        let key = text_sha256(input);
        if let Some(v) = self.cache.read().expect("cache read lock").get(&key) {
            return Ok(v.clone());
        }
        let v = self.inner.embed(input)?;
        self.live_misses.fetch_add(1, Ordering::Relaxed);
        self.cache.write().expect("cache write lock").insert(key, v.clone());
        Ok(v)
    }
}

// ── Cache key + IO helpers ────────────────────────────────────────────--

fn text_sha256(text: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(text.as_bytes());
    h.finalize().into()
}

/// sha256 over the sorted, deduped doc-id list (newline-joined). Stable
/// regardless of the doc set's in-memory order.
fn doc_manifest_sha(docs: &[Doc]) -> String {
    let mut ids: Vec<&str> = docs.iter().map(|d| d.doc_id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    let mut h = Sha256::new();
    for id in ids {
        h.update(id.as_bytes());
        h.update(b"\n");
    }
    hex(&h.finalize())
}

/// Cache file stem: `sha256(name \0 revision \0 dim \0 label \0 manifest)`.
/// Any of identity (name/revision/dim), subset label, or doc-manifest
/// changing yields a different file — the invalidation contract.
fn cache_key(identity: &EmbedderIdentity, label: &str, doc_manifest_sha: &str) -> String {
    let mut h = Sha256::new();
    h.update(identity.name.as_bytes());
    h.update(b"\0");
    h.update(identity.revision.as_bytes());
    h.update(b"\0");
    h.update(identity.dimension.to_le_bytes());
    h.update(b"\0");
    h.update(label.as_bytes());
    h.update(b"\0");
    h.update(doc_manifest_sha.as_bytes());
    hex(&h.finalize())
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Read + validate the cache. `Ok(Some(map))` on a trusted hit;
/// `Ok(None)` only when BOTH files are absent (the genuine cold cache);
/// `Err(reason)` when present-but-stale/corrupt OR partially present
/// (blob xor sidecar) — a partial cache is a loud miss, never silently
/// treated as cold. The caller logs every miss and rebuilds; it never
/// trusts the bytes.
fn read_cache(
    cache_path: &Path,
    meta_path: &Path,
    identity: &EmbedderIdentity,
    doc_manifest_sha: &str,
) -> Result<Option<VecCache>, String> {
    let blob_exists = cache_path.exists();
    let meta_exists = meta_path.exists();
    if !blob_exists && !meta_exists {
        return Ok(None);
    }
    if blob_exists != meta_exists {
        return Err(format!(
            "partial cache (blob={blob_exists}, sidecar={meta_exists}) — treating as miss"
        ));
    }
    let meta_bytes = fs::read(meta_path).map_err(|e| format!("read meta: {e}"))?;
    let meta: serde_json::Value =
        serde_json::from_slice(&meta_bytes).map_err(|e| format!("parse meta: {e}"))?;

    // Verify the manifest BEFORE trusting any blob bytes (codex focus:
    // no test silently uses a stale cache).
    let fmt = meta.get("format_version").and_then(serde_json::Value::as_u64);
    if fmt != Some(u64::from(CACHE_FORMAT_VERSION)) {
        return Err(format!("format_version mismatch (got {fmt:?})"));
    }
    let m_name = meta.pointer("/identity/name").and_then(serde_json::Value::as_str);
    let m_rev = meta.pointer("/identity/revision").and_then(serde_json::Value::as_str);
    let m_dim = meta.pointer("/identity/dimension").and_then(serde_json::Value::as_u64);
    if m_name != Some(identity.name.as_str())
        || m_rev != Some(identity.revision.as_str())
        || m_dim != Some(u64::from(identity.dimension))
    {
        return Err("identity mismatch".to_string());
    }
    let m_manifest = meta.get("doc_manifest_sha").and_then(serde_json::Value::as_str);
    if m_manifest != Some(doc_manifest_sha) {
        return Err("doc_manifest_sha mismatch".to_string());
    }

    let blob = fs::read(cache_path).map_err(|e| format!("read blob: {e}"))?;
    parse_blob(&blob, identity.dimension).map(Some)
}

fn parse_blob(blob: &[u8], expected_dim: u32) -> Result<VecCache, String> {
    let mut cur = blob;
    let mut take = |n: usize| -> Result<&[u8], String> {
        if cur.len() < n {
            return Err("truncated blob".to_string());
        }
        let (head, tail) = cur.split_at(n);
        cur = tail;
        Ok(head)
    };
    let fmt = u32::from_le_bytes(take(4)?.try_into().unwrap());
    if fmt != CACHE_FORMAT_VERSION {
        return Err(format!("blob format_version mismatch (got {fmt})"));
    }
    let dim = u32::from_le_bytes(take(4)?.try_into().unwrap());
    if dim != expected_dim {
        return Err(format!("blob dim {dim} != expected {expected_dim}"));
    }
    let count = u32::from_le_bytes(take(4)?.try_into().unwrap()) as usize;
    let dim = dim as usize;
    let mut map = VecCache::with_capacity(count);
    for _ in 0..count {
        let key: [u8; 32] = take(32)?.try_into().unwrap();
        let mut v = Vec::with_capacity(dim);
        let vec_bytes = take(dim * 4)?;
        for chunk in vec_bytes.chunks_exact(4) {
            v.push(f32::from_le_bytes(chunk.try_into().unwrap()));
        }
        map.insert(key, v);
    }
    if !cur.is_empty() {
        return Err("trailing bytes in blob".to_string());
    }
    Ok(map)
}

/// Atomic write: `path.tmp` → fsync-free rename. Rename is atomic on the
/// same filesystem, so a reader never observes a half-written file.
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path
        .with_extension(format!("{}.tmp", path.extension().and_then(|e| e.to_str()).unwrap_or("")));
    {
        let mut f = fs::File::create(&tmp).map_err(|e| format!("create {}: {e}", tmp.display()))?;
        f.write_all(bytes).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    }
    fs::rename(&tmp, path).map_err(|e| format!("rename -> {}: {e}", path.display()))?;
    Ok(())
}

fn open_readonly(path: &Path) -> Connection {
    Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("open read-only sqlite")
}

// ── EU-0 §1.2 query synthesis (mirrors eu7_real_corpus_ac.rs) ──────────--

/// SplitMix64 — deterministic, machine-independent RNG for query
/// selection. (`rand::thread_rng` is forbidden by the determinism
/// requirement.)
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, bound)`.
    fn next_in(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }
}

const LEAD_MAX_CHARS: usize = 140;

/// Title if usable (>= 6 chars, not "untitled", not equal to body);
/// otherwise the lead sentence. Must NOT equal the body verbatim (else
/// the query IS the document and recall is trivially self-fulfilling).
fn synth_query(doc: &Doc) -> Option<String> {
    if let Some(title) = &doc.title {
        let t = title.trim();
        if t.len() >= 6 && !t.eq_ignore_ascii_case("untitled") && t != doc.body.trim() {
            return Some(t.to_string());
        }
    }
    let body = doc.body.trim();
    if body.is_empty() {
        return None;
    }
    let lead = lead_sentence(body, LEAD_MAX_CHARS);
    if lead.trim().is_empty() || lead.trim() == body {
        return None;
    }
    Some(lead)
}

/// First sentence (up to `.`/`!`/`?`) or `max_chars` at a char boundary,
/// whichever comes first. Skips leading markdown bullet noise.
fn lead_sentence(body: &str, max_chars: usize) -> String {
    let cleaned: String = body
        .lines()
        .map(|l| l.trim_start_matches(['-', '*', '#', '>', ' ', '\t']))
        .collect::<Vec<_>>()
        .join(" ");
    let cleaned = cleaned.trim();
    let mut out = String::new();
    for (i, ch) in cleaned.chars().enumerate() {
        if i >= max_chars {
            break;
        }
        out.push(ch);
        if matches!(ch, '.' | '!' | '?') && out.trim().len() >= 12 {
            break;
        }
    }
    out.trim().to_string()
}
