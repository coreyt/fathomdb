//! IR-C — R0 candidate-recall CDF runner + schema validation gate.
//!
//! # What this file does
//!
//! 1. **`ir_c_cdf_schema_shape`** (default pass, no corpus needed): a schema
//!    gate that validates `IR-C-recall-cdf.json` if the file exists. Passes
//!    vacuously when the artifact is absent; fails if the artifact exists but
//!    is malformed. This is the falsifiable bar from ADR-0.8.1 §2.5.
//!
//! 2. **`ir_c_recall_cdf`** (gated `IRC_RUN=1` + `default-embedder` feature):
//!    the full candidate-recall CDF runner. Seeds the frozen corpus into a
//!    fresh temp engine, measures `found@K` for K ∈ {50,100,200,500,1000}
//!    across all four retrieval arms and all gold query classes, writes
//!    `dev/plans/runs/IR-C-recall-cdf.json` to the canonical path.
//!
//! # Arms
//! - `bm25_text`: content-OR FTS5 + bm25() ranking (direct SQLite seam)
//! - `dense`: vector-stage-only engine.search via set_vector_stage_only_for_test
//! - `rrf_fused`: production RRF-hybrid engine.search
//! - `oracle_union`: union of bm25_text top-K and dense top-K (upper bound)
//!
//! # Run instructions
//! ```text
//! # Schema gate (no corpus):
//! cargo test -p fathomdb-engine --test ir_c_cdf_run
//!
//! # Full CDF run (requires frozen corpus + real embedder weights):
//! IRC_RUN=1 cargo test --release -p fathomdb-engine \
//!   --features default-embedder --test ir_c_cdf_run -- --nocapture
//! ```

// ── Shared module declarations (all consumers share these) ───────────────────
#[path = "support/corpus_subset.rs"]
mod corpus_subset;
#[path = "support/ir_eval.rs"]
mod ir_eval;
#[path = "support/ir_retrieval.rs"]
mod ir_retrieval;

use std::collections::HashSet;

use serde_json::Value;

/// R0 K-ladder: K ∈ {50,100,200,500,1000}. Distinct from `K_LADDER` in
/// `ir_eval.rs` ({5,10,20,50}) which is the Evidence Recall@K ladder.
#[cfg(feature = "default-embedder")]
const CDF_K_LADDER: [usize; 5] = [50, 100, 200, 500, 1000];

/// Frozen corpus hash prefix. The full hash is pinned in the artifact.
const CORPUS_HASH_PREFIX: &str = "fe973fcd";

// ── Schema validation test (default pass, no corpus needed) ─────────────────

/// Schema gate: validates `IR-C-recall-cdf.json` if the file exists.
/// Vacuously passes when the artifact is absent (no corpus required).
/// Fails loudly if the artifact exists but is malformed.
/// Falsifiable bar: ADR-0.8.1 §2.5.
#[test]
fn ir_c_cdf_schema_shape() {
    let Some(root) = corpus_subset::repo_root() else {
        eprintln!("[skip] repo_root() not found — schema test vacuously passes");
        return;
    };
    let artifact = root.join("dev/plans/runs/IR-C-recall-cdf.json");
    if !artifact.exists() {
        eprintln!(
            "[skip] {} absent — schema test vacuously passes (artifact not yet generated)",
            artifact.display()
        );
        return;
    }

    let text = std::fs::read_to_string(&artifact).expect("read IR-C-recall-cdf.json (it exists)");
    let v: Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("IR-C-recall-cdf.json is not valid JSON: {e}"));

    // ── Required top-level fields ────────────────────────────────────────────
    let corpus_hash = v
        .get("corpus_hash")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("IR-C-recall-cdf.json: missing `corpus_hash`"));
    assert!(
        corpus_hash.starts_with(CORPUS_HASH_PREFIX),
        "corpus_hash `{corpus_hash}` does not start with `{CORPUS_HASH_PREFIX}`"
    );
    assert!(
        v.get("gold_set_path").and_then(Value::as_str).is_some(),
        "IR-C-recall-cdf.json: missing `gold_set_path`"
    );
    assert!(
        v.get("generated_at").and_then(Value::as_str).is_some(),
        "IR-C-recall-cdf.json: missing `generated_at`"
    );

    // ── recall_cdf array ─────────────────────────────────────────────────────
    let recall_cdf = v
        .get("recall_cdf")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("IR-C-recall-cdf.json: missing or non-array `recall_cdf`"));
    assert!(
        recall_cdf.len() >= 40,
        "recall_cdf has {} rows but must have ≥ 40 (5 K × 2 query_class × 4 arms)",
        recall_cdf.len()
    );

    // ── Row schema ───────────────────────────────────────────────────────────
    for (i, row) in recall_cdf.iter().enumerate() {
        let at = format!("recall_cdf[{i}]");
        assert!(
            row.get("query_class").and_then(Value::as_str).is_some(),
            "{at}: missing `query_class`"
        );
        assert!(row.get("arm").and_then(Value::as_str).is_some(), "{at}: missing `arm`");
        assert!(row.get("k").and_then(Value::as_u64).is_some(), "{at}: missing or non-integer `k`");
        let fak = row
            .get("found_at_k")
            .and_then(Value::as_f64)
            .unwrap_or_else(|| panic!("{at}: missing `found_at_k`"));
        assert!((0.0..=1.0).contains(&fak), "{at}: found_at_k={fak} outside [0,1]");
        assert!(
            row.get("n_queries").and_then(Value::as_u64).is_some(),
            "{at}: missing `n_queries`"
        );
    }

    // ── Arm coverage ─────────────────────────────────────────────────────────
    let arms: HashSet<&str> =
        recall_cdf.iter().filter_map(|r| r.get("arm").and_then(Value::as_str)).collect();
    let required_arms = ["bm25_text", "dense", "oracle_union", "rrf_fused"];
    let mut missing: Vec<&&str> = required_arms.iter().filter(|a| !arms.contains(*a)).collect();
    missing.sort();
    assert!(missing.is_empty(), "recall_cdf missing required arms: {missing:?}");

    // ── K coverage ───────────────────────────────────────────────────────────
    let ks: HashSet<u64> =
        recall_cdf.iter().filter_map(|r| r.get("k").and_then(Value::as_u64)).collect();
    let required_ks = [50u64, 100, 200, 500, 1000];
    let mut missing_ks: Vec<u64> =
        required_ks.iter().filter(|&&k| !ks.contains(&k)).copied().collect();
    missing_ks.sort();
    assert!(missing_ks.is_empty(), "recall_cdf missing required K values: {missing_ks:?}");

    // ── latency field present (may be null placeholder) ──────────────────────
    assert!(
        v.get("latency").is_some(),
        "IR-C-recall-cdf.json: missing `latency` field (null placeholder is OK)"
    );

    let mut arm_list: Vec<&str> = arms.into_iter().collect();
    arm_list.sort();
    eprintln!(
        "[SCHEMA-PASS] IR-C-recall-cdf.json: {} recall_cdf rows, arms={arm_list:?}, corpus_hash={corpus_hash}",
        recall_cdf.len()
    );
}

// ── Full CDF runner (IRC_RUN=1, requires default-embedder) ──────────────────

#[cfg(feature = "default-embedder")]
mod cdf_runner_impl {
    //! Full candidate-recall CDF runner. Seeds the frozen corpus with the
    //! real BGE embedder, measures found@K for all arms / Ks / query classes,
    //! writes the artifact to the canonical path.

    use std::collections::{BTreeSet, HashMap, HashSet};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use super::{corpus_subset, ir_eval, ir_retrieval, CDF_K_LADDER, CORPUS_HASH_PREFIX};

    use corpus_subset::{load_subset_or_skip, Doc, VECTOR_KIND};
    use fathomdb_embedder::CandleBgeEmbedder;
    use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
    use fathomdb_engine::{EmbedderChoice, Engine, PreparedWrite};
    use ir_eval::{load_gold_set, required_doc_ids, validate_gold_set, QueryClass};
    use ir_retrieval::{compile_content_or, fts_bodies, map_bodies};
    use rusqlite::{Connection, OpenFlags};
    use serde_json::Value;

    // ── Thread-safe BGE wrapper (mirrors ir_c_recall_run.rs::SerializedBge) ──
    struct SerializedBge {
        inner: Mutex<CandleBgeEmbedder>,
        identity: EmbedderIdentity,
    }
    impl SerializedBge {
        fn new(inner: CandleBgeEmbedder) -> Self {
            let identity = inner.identity();
            Self { inner: Mutex::new(inner), identity }
        }
    }
    impl Embedder for SerializedBge {
        fn identity(&self) -> EmbedderIdentity {
            self.identity.clone()
        }
        fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
            self.inner.lock().expect("embedder mutex poisoned").embed(text)
        }
    }

    fn env_usize(key: &str, default: usize) -> usize {
        std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
    }

    /// First-occurrence body→doc_id map (mirrors ir_c_recall_run.rs).
    fn body_to_doc_map(docs: &[Doc]) -> HashMap<String, String> {
        let mut m = HashMap::with_capacity(docs.len());
        for d in docs {
            m.entry(d.body.clone()).or_insert_with(|| d.doc_id.clone());
        }
        m
    }

    /// True if any required gold doc appears in `results[..k]`.
    fn found_at_k(results: &[String], gold: &BTreeSet<String>, k: usize) -> bool {
        results.iter().take(k).any(|id| gold.contains(id))
    }

    /// Per-query retrieval output (all three independent arms; oracle derived).
    struct QResult {
        query_class: QueryClass,
        gold: BTreeSet<String>,
        bm25: Vec<String>,
        dense: Vec<String>,
        fused: Vec<String>,
    }

    /// Aggregate found@K for an arm closure + optional class filter.
    fn agg<F>(results: &[QResult], arm: F, cls: Option<QueryClass>, k: usize) -> (f64, usize)
    where
        F: Fn(&QResult) -> &[String],
    {
        let (n, found) = results
            .iter()
            .filter(|qr| cls.map_or(true, |c| qr.query_class == c))
            .fold((0usize, 0usize), |(n, f), qr| {
                (n + 1, f + found_at_k(arm(qr), &qr.gold, k) as usize)
            });
        (if n == 0 { 0.0 } else { found as f64 / n as f64 }, n)
    }

    /// Oracle union: found in bm25 top-K OR dense top-K.
    fn oracle_agg(results: &[QResult], cls: Option<QueryClass>, k: usize) -> (f64, usize) {
        let (n, found) = results
            .iter()
            .filter(|qr| cls.map_or(true, |c| qr.query_class == c))
            .fold((0usize, 0usize), |(n, f), qr| {
                let hit = found_at_k(&qr.bm25, &qr.gold, k) || found_at_k(&qr.dense, &qr.gold, k);
                (n + 1, f + hit as usize)
            });
        (if n == 0 { 0.0 } else { found as f64 / n as f64 }, n)
    }

    /// ISO-8601 UTC timestamp (seconds precision, no external dependency).
    fn now_iso8601() -> String {
        use std::time::SystemTime;
        let secs =
            SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs();
        let s = secs % 60;
        let mi = (secs / 60) % 60;
        let h = (secs / 3600) % 24;
        let days = (secs / 86400) as u32;
        let (y, mo, d) = days_to_ymd(days);
        format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
    }

    fn days_to_ymd(mut d: u32) -> (u32, u32, u32) {
        let mut y = 1970u32;
        loop {
            let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
            if d < if leap { 366 } else { 365 } {
                break;
            }
            d -= if leap { 366 } else { 365 };
            y += 1;
        }
        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        let mdays = [31u32, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        let mut mo = 1u32;
        for &md in &mdays {
            if d < md {
                break;
            }
            d -= md;
            mo += 1;
        }
        (y, mo, d + 1)
    }

    #[test]
    pub fn ir_c_recall_cdf() {
        if std::env::var_os("IRC_RUN").is_none() {
            eprintln!("[skip] IRC_RUN not set; IR-C CDF run is opt-in (set IRC_RUN=1)");
            return;
        }
        if std::env::var_os("FATHOMDB_SKIP_NETWORK_TESTS").is_some() {
            eprintln!("[skip] FATHOMDB_SKIP_NETWORK_TESTS set; embedder weights unavailable");
            return;
        }

        let Some(root) = corpus_subset::repo_root() else {
            eprintln!("[skip] repo_root() not found");
            return;
        };

        // ── Load + validate gold set ──────────────────────────────────────────
        let gold_path = root.join("data/corpus-data/eval/ir_gold/all.gold.json");
        if !gold_path.exists() {
            eprintln!("[skip] {} absent (run build_ir_gold.py)", gold_path.display());
            return;
        }
        let gold = load_gold_set(&gold_path).expect("load gold set");
        let issues = validate_gold_set(&gold);
        assert!(issues.is_empty(), "gold set invalid: {issues:?}");

        // ── Verify corpus_hash ────────────────────────────────────────────────
        let snap_path = root.join("tests/corpus/snapshot.json");
        let snapshot_hash = std::fs::read_to_string(&snap_path)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v.get("corpus_hash").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_default();
        assert!(!snapshot_hash.is_empty(), "snapshot.json missing corpus_hash");
        assert_eq!(
            gold.corpus_hash, snapshot_hash,
            "gold corpus_hash ≠ snapshot — re-run build_ir_gold.py on the frozen corpus"
        );
        assert!(
            gold.corpus_hash.starts_with(CORPUS_HASH_PREFIX),
            "corpus_hash prefix mismatch: expected `{CORPUS_HASH_PREFIX}…` got `{}`",
            gold.corpus_hash
        );
        eprintln!("CDF_HASH corpus_hash={}", gold.corpus_hash);

        // ── Load full corpus ──────────────────────────────────────────────────
        let max_docs = env_usize("IRC_CDF_MAXDOCS", usize::MAX);
        let Some(mut docs) = load_subset_or_skip(usize::MAX) else { return };
        if docs.len() > max_docs {
            docs.truncate(max_docs);
        }
        eprintln!("CDF_CORPUS docs={}", docs.len());

        let btod = body_to_doc_map(&docs);
        let in_corpus: HashSet<String> = docs.iter().map(|d| d.doc_id.clone()).collect();

        // ── Engine: real BGE embedder ─────────────────────────────────────────
        let embedder = Arc::new(SerializedBge::new(
            CandleBgeEmbedder::new().expect("construct real bge embedder"),
        ));
        let dir = tempfile::TempDir::new().expect("tempdir");
        let db_path = dir.path().join("ir_c_cdf.sqlite");
        let opened = Engine::open_with_choice(
            &db_path,
            EmbedderChoice::Caller(embedder.clone() as Arc<dyn Embedder>),
        )
        .expect("open engine with real bge embedder");
        assert_eq!(
            opened.report.default_embedder.name, "fathomdb-bge-small-en-v1.5",
            "CDF must run against the real bge-small identity"
        );
        let engine = opened.engine;
        engine.configure_vector_kind_for_test(VECTOR_KIND).expect("configure vector kind");

        // ── Seed corpus with drain (FTS + vectors) ────────────────────────────
        {
            const BATCH: usize = 256;
            let t0 = Instant::now();
            let mut last = Instant::now();
            let mut done = 0usize;
            while done < docs.len() {
                let n = BATCH.min(docs.len() - done);
                let batch: Vec<PreparedWrite> = docs[done..done + n]
                    .iter()
                    .map(|d| PreparedWrite::Node {
                        kind: VECTOR_KIND.to_string(),
                        body: d.body.clone(),
                        source_id: fathomdb_engine::SourceId::new(d.doc_id.clone())
                            .expect("test source id"),
                        logical_id: None,
                        state: fathomdb_engine::InitialState::Active,
                        reason: None,
                        valid_from: None,
                        valid_until: None,
                    })
                    .collect();
                engine.write(&batch).expect("seed write");
                engine.drain(600_000).expect("seed drain");
                done += n;
                if last.elapsed() >= Duration::from_secs(30) {
                    let rate = done as f64 / t0.elapsed().as_secs_f64().max(1e-3);
                    eprintln!("CDF_SEED {done}/{} {rate:.1} docs/s", docs.len());
                    last = Instant::now();
                }
            }
            eprintln!("CDF_SEEDED docs={} in {:.1}s", docs.len(), t0.elapsed().as_secs_f64());
        }

        // ── Read-only FTS connection for bm25_text arm ────────────────────────
        let fts_conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("open read-only FTS connection");

        // ── Raise search limit to K=1000 for dense + fused arms ───────────────
        engine.set_search_limit_for_test(1000);

        // ── Filter: positive queries whose evidence is in corpus ───────────────
        let eligible: Vec<_> = gold
            .queries
            .iter()
            .filter(|q| {
                if q.query_class == QueryClass::Negative {
                    return false;
                }
                let req = required_doc_ids(q);
                !req.is_empty() && req.iter().any(|id| in_corpus.contains(id))
            })
            .collect();
        eprintln!("CDF_QUERIES eligible={} of {}", eligible.len(), gold.queries.len());
        if eligible.is_empty() {
            eprintln!("[skip] no eligible queries");
            return;
        }

        // ── Per-query retrieval ───────────────────────────────────────────────
        let t_run = Instant::now();
        let mut results: Vec<QResult> = Vec::with_capacity(eligible.len());

        for (i, q) in eligible.iter().enumerate() {
            if i % 500 == 0 && i > 0 {
                eprintln!(
                    "CDF_PROGRESS {i}/{} ({:.1}s)",
                    eligible.len(),
                    t_run.elapsed().as_secs_f64()
                );
            }
            let gold_ids = required_doc_ids(q);
            let qt = q.query.as_str();

            // bm25_text: content-OR FTS5 at K=1000
            let bm25 = map_bodies(
                &fts_bodies(&fts_conn, &compile_content_or(qt), "bm25(search_index)", 1000),
                &btod,
            );

            // dense: vector-stage-only
            engine.set_vector_stage_only_for_test(true);
            let dense = engine
                .search(qt)
                .map(|r| r.results.into_iter().filter_map(|h| btod.get(&h.body).cloned()).collect())
                .unwrap_or_default();
            engine.set_vector_stage_only_for_test(false);

            // rrf_fused: production RRF-hybrid
            let fused = engine
                .search(qt)
                .map(|r| r.results.into_iter().filter_map(|h| btod.get(&h.body).cloned()).collect())
                .unwrap_or_default();

            results.push(QResult {
                query_class: q.query_class,
                gold: gold_ids,
                bm25,
                dense,
                fused,
            });
        }
        eprintln!("CDF_RUN_DONE n={} in {:.1}s", results.len(), t_run.elapsed().as_secs_f64());

        // ── Collect classes present ───────────────────────────────────────────
        let classes: Vec<QueryClass> = {
            let mut s = std::collections::BTreeSet::new();
            for qr in &results {
                s.insert(qr.query_class);
            }
            s.into_iter().collect()
        };
        eprintln!("CDF_CLASSES {:?}", classes.iter().map(|c| c.label()).collect::<Vec<_>>());

        // ── Build recall_cdf rows ─────────────────────────────────────────────
        let arms = ["bm25_text", "dense", "oracle_union", "rrf_fused"];
        let mut cdf: Vec<serde_json::Value> = Vec::new();

        for &cls in &classes {
            for &arm in &arms {
                for &k in &CDF_K_LADDER {
                    let (fak, n) = match arm {
                        "bm25_text" => agg(&results, |r| &r.bm25, Some(cls), k),
                        "dense" => agg(&results, |r| &r.dense, Some(cls), k),
                        "rrf_fused" => agg(&results, |r| &r.fused, Some(cls), k),
                        "oracle_union" => oracle_agg(&results, Some(cls), k),
                        _ => unreachable!(),
                    };
                    if n == 0 {
                        continue;
                    }
                    cdf.push(serde_json::json!({
                        "query_class": cls.label(),
                        "arm": arm,
                        "k": k,
                        "found_at_k": (fak * 10000.0).round() / 10000.0,
                        "n_queries": n,
                    }));
                }
            }
        }

        eprintln!("CDF_ROWS rows={}", cdf.len());
        assert!(cdf.len() >= 40, "expected ≥ 40 CDF rows, got {} — check classes/arms", cdf.len());

        // ── Headline numbers ──────────────────────────────────────────────────
        for arm in &arms {
            for cls in &["exact_fact", "exploratory"] {
                if let Some(row) = cdf.iter().find(|r| {
                    r["arm"].as_str() == Some(arm)
                        && r["query_class"].as_str() == Some(cls)
                        && r["k"].as_u64() == Some(1000)
                }) {
                    eprintln!(
                        "CDF_HEADLINE arm={arm} class={cls} k=1000 found_at_k={}",
                        row["found_at_k"].as_f64().unwrap_or(0.0)
                    );
                }
            }
        }

        // ── Artifact ─────────────────────────────────────────────────────────
        let artifact = serde_json::json!({
            "corpus_hash": gold.corpus_hash,
            "gold_set_path": "data/corpus-data/eval/ir_gold/all.gold.json",
            "generated_at": now_iso8601(),
            "recall_cdf": cdf,
            // latency null placeholder — populate by running dev/scripts/ir_c_ce_latency.py
            "latency": null,
        });

        // ── Write to canonical path ───────────────────────────────────────────
        let out = root.join("dev/plans/runs/IR-C-recall-cdf.json");
        if let Some(p) = out.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        std::fs::write(&out, serde_json::to_string_pretty(&artifact).expect("serialize"))
            .expect("write IR-C-recall-cdf.json");
        eprintln!("CDF_WROTE {}", out.display());
        eprintln!("[S5][GREEN] CDF artifact written successfully");
    }
}
