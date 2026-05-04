use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

const PERF_SAMPLES: usize = 1_000;
const AC020_THREADS: usize = 8;
const AC020_ROUNDS_PER_THREAD: usize = 50;

#[derive(Clone, Debug)]
struct DeterministicEmbedder {
    identity: EmbedderIdentity,
    vector: Vector,
    delay: Duration,
}

impl DeterministicEmbedder {
    fn new(dimension: u32, delay: Duration) -> Self {
        Self {
            identity: EmbedderIdentity::new("deterministic", "perf-gates", dimension),
            vector: unit_vector(dimension as usize),
            delay,
        }
    }
}

impl Embedder for DeterministicEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        thread::sleep(self.delay);
        Ok(self.vector.clone())
    }
}

#[derive(Clone, Debug)]
struct RoutedEmbedder {
    identity: EmbedderIdentity,
}

impl RoutedEmbedder {
    fn new(dimension: u32) -> Self {
        Self { identity: EmbedderIdentity::new("routed", "perf-gates", dimension) }
    }
}

impl Embedder for RoutedEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
        let mut vector = vec![0.0_f32; self.identity.dimension as usize];
        let slot = if text.starts_with("semantic-") || text.starts_with("vector-doc-") {
            0
        } else if text.starts_with("hybrid-") || text.starts_with("hybrid doc") {
            1
        } else {
            2
        };
        vector[slot] = 1.0;
        Ok(vector)
    }
}

fn fixture_path(name: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("{name}{SQLITE_SUFFIX}"));
    (dir, path)
}

fn percentile_ceil(samples: &[Duration], numerator: usize, denominator: usize) -> Duration {
    assert!(!samples.is_empty());
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let index = ((sorted.len() * numerator).div_ceil(denominator)).saturating_sub(1);
    sorted[index]
}

fn unit_vector(dimension: usize) -> Vector {
    let mut values = vec![0.0_f32; dimension];
    if dimension > 0 {
        values[0] = 1.0;
    }
    values
}

fn long_run_enabled() -> bool {
    std::env::var_os("AGENT_LONG").is_some()
}

fn ac020_queries() -> [&'static str; 4] {
    ["semantic-0", "hybrid-0", "semantic-1", "hybrid-1"]
}

fn seed_ac020_fixture(engine: &Engine) {
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    for i in 0..2 {
        engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: format!("vector-doc-{i}"),
            }])
            .expect("vector-only write");
        engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: format!("hybrid doc hybrid-{i}"),
            }])
            .expect("hybrid write");
    }
    engine.drain(10_000).expect("drain");
}

fn run_ac020_mix(engine: &Engine) {
    for _ in 0..AC020_ROUNDS_PER_THREAD {
        for query in ac020_queries() {
            let result = engine.search(query).expect("search");
            assert!(!result.results.is_empty(), "read-mix query {query} must yield a result");
        }
    }
}

#[test]
#[ignore = "protocol-incomplete: 1M text-query fixture from dev/acceptance.md is not landed yet"]
fn ac_012_text_query_latency_on_fts5_path() {}

#[test]
#[ignore = "blocked on a protocol-complete vector-latency fixture and retrieval-path evidence"]
fn ac_013_vector_retrieval_latency() {}

#[test]
fn ac_017_vector_projection_freshness_p99_le_five_seconds() {
    let (_dir, path) = fixture_path("projection_freshness");
    let embedder = Arc::new(DeterministicEmbedder::new(384, Duration::from_millis(1)));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");

    let mut samples = Vec::with_capacity(PERF_SAMPLES);
    for i in 0..PERF_SAMPLES {
        let commit_started = Instant::now();
        let receipt = opened
            .engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: format!("projection doc {i}"),
            }])
            .expect("write");

        loop {
            let result = opened.engine.search("projection").expect("search");
            if result.projection_cursor >= receipt.cursor {
                samples.push(commit_started.elapsed());
                break;
            }
            assert!(
                commit_started.elapsed() < Duration::from_secs(5),
                "projection cursor did not reach commit cursor within 5 s for write {}",
                receipt.cursor
            );
            thread::sleep(Duration::from_millis(1));
        }
    }

    let p99 = percentile_ceil(&samples, 99, 100);
    assert!(
        p99 <= Duration::from_secs(5),
        "AC-017 failed: p99 freshness {:?} exceeded 5 s over {} samples",
        p99,
        samples.len()
    );
}

#[test]
fn ac_018_drain_of_100_vectors_le_two_seconds() {
    let (_dir, path) = fixture_path("drain_100_vectors");
    let embedder = Arc::new(DeterministicEmbedder::new(384, Duration::from_millis(1)));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");

    for i in 0..100 {
        opened
            .engine
            .write(&[PreparedWrite::Node { kind: "doc".to_string(), body: format!("doc {i}") }])
            .expect("write");
    }

    let started = Instant::now();
    opened.engine.drain(5_000).expect("drain");
    let elapsed = started.elapsed();

    eprintln!("AC018_NUMBERS drain_ms={}", elapsed.as_millis());

    assert!(
        elapsed <= Duration::from_secs(2),
        "AC-018 failed: drain took {:?}, expected <= 2 s",
        elapsed
    );
    assert_eq!(opened.engine.vector_row_count_for_test().expect("vector rows"), 100);
}

#[test]
#[ignore = "blocked on a protocol-complete mixed-retrieval workload that exercises a non-synthetic second branch"]
fn ac_019_mixed_retrieval_stress_workload_tail() {}

#[test]
fn ac_020_reads_do_not_serialize_on_a_single_reader_connection() {
    if !long_run_enabled() {
        return;
    }

    let (_dir, path) = fixture_path("ac020_read_mix");
    let embedder = Arc::new(RoutedEmbedder::new(3));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    seed_ac020_fixture(&opened.engine);

    let sequential_started = Instant::now();
    for _ in 0..AC020_THREADS {
        run_ac020_mix(&opened.engine);
    }
    let sequential = sequential_started.elapsed();

    let engine = Arc::new(opened.engine);
    let barrier = Arc::new(Barrier::new(AC020_THREADS + 1));
    let mut handles = Vec::with_capacity(AC020_THREADS);
    for _ in 0..AC020_THREADS {
        let engine = Arc::clone(&engine);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            barrier.wait();
            run_ac020_mix(&engine);
        }));
    }
    let concurrent_started = Instant::now();
    barrier.wait();
    for handle in handles {
        handle.join().expect("reader thread");
    }
    let concurrent = concurrent_started.elapsed();

    let bound = sequential.mul_f32(1.5 / AC020_THREADS as f32);
    eprintln!(
        "AC020_NUMBERS sequential_ms={} concurrent_ms={} bound_ms={}",
        sequential.as_millis(),
        concurrent.as_millis(),
        bound.as_millis(),
    );
    assert!(
        concurrent <= bound,
        "AC-020 failed: concurrent={concurrent:?} bound={bound:?} sequential={sequential:?}"
    );
}

#[test]
#[ignore = "profiling harness: set AC020_PHASE=sequential to opt in"]
fn ac_020_sequential_only() {
    if std::env::var("AC020_PHASE").as_deref() != Ok("sequential") {
        return;
    }

    let (_dir, path) = fixture_path("ac020_sequential_only");
    let embedder = Arc::new(RoutedEmbedder::new(3));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    seed_ac020_fixture(&opened.engine);

    let started = Instant::now();
    for _ in 0..AC020_THREADS {
        run_ac020_mix(&opened.engine);
    }
    let elapsed = started.elapsed();

    eprintln!("AC020_PHASE_SEQUENTIAL_MS={}", elapsed.as_millis());
}

#[test]
#[ignore = "profiling harness: set AC020_PHASE=concurrent to opt in"]
fn ac_020_concurrent_only() {
    if std::env::var("AC020_PHASE").as_deref() != Ok("concurrent") {
        return;
    }

    let (_dir, path) = fixture_path("ac020_concurrent_only");
    let embedder = Arc::new(RoutedEmbedder::new(3));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    seed_ac020_fixture(&opened.engine);

    let engine = Arc::new(opened.engine);
    let barrier = Arc::new(Barrier::new(AC020_THREADS + 1));
    let mut handles = Vec::with_capacity(AC020_THREADS);
    for _ in 0..AC020_THREADS {
        let engine = Arc::clone(&engine);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            barrier.wait();
            run_ac020_mix(&engine);
        }));
    }
    let started = Instant::now();
    barrier.wait();
    for handle in handles {
        handle.join().expect("reader thread");
    }
    let elapsed = started.elapsed();

    eprintln!("AC020_PHASE_CONCURRENT_MS={}", elapsed.as_millis());
}

// ── G.3.5 cache-pressure telemetry ───────────────────────────────────────────

/// Pack 6.G G.3.5 — read-only screening test that captures per-worker
/// `SQLITE_DBSTATUS_CACHE_HIT` / `_CACHE_MISS` / `_CACHE_USED` deltas
/// across one AC-020 concurrent body. Writes a sidecar JSON to the
/// path given by `G3_5_OUTPUT_PATH` env var so the orchestrator can
/// assemble the final per-phase JSON without re-running.
///
/// Run with:
///   `G3_5_OUTPUT_PATH=/tmp/foo.json cargo test --release \
///    -p fathomdb-engine --test perf_gates -- --ignored \
///    g3_5_cache_pressure_telemetry --nocapture`
#[cfg(debug_assertions)]
#[test]
#[ignore = "G.3.5 diagnostic: read-only cache-pressure telemetry"]
fn g3_5_cache_pressure_telemetry() {
    let output_path = std::env::var("G3_5_OUTPUT_PATH")
        .expect("G3_5_OUTPUT_PATH env var required for the G.3.5 sidecar JSON");

    let (_dir, path) = fixture_path("g3_5_cache_pressure_telemetry");
    let embedder = Arc::new(RoutedEmbedder::new(3));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    seed_ac020_fixture(&opened.engine);
    let worker_count = opened.engine.reader_worker_count_for_test();

    // Warmup: 16 dispatched searches so the round-robin reaches every
    // worker at least twice and the page cache reaches steady state on
    // the seeded fixture before the pre snapshot.
    for _ in 0..16 {
        let _ = opened.engine.search("semantic-0").expect("warmup search");
    }

    let pre = opened.engine.cache_status_per_worker_for_test("pre");
    assert_eq!(pre.len(), worker_count);

    // Run the AC-020 concurrent body once (8 threads x 50 rounds x 4
    // queries = 1600 dispatched searches). Same shape as
    // `ac_020_concurrent_only` but inlined so we don't depend on env-
    // gated test ordering.
    let engine = Arc::new(opened.engine);
    let barrier = Arc::new(Barrier::new(AC020_THREADS + 1));
    let mut handles = Vec::with_capacity(AC020_THREADS);
    for _ in 0..AC020_THREADS {
        let engine = Arc::clone(&engine);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            barrier.wait();
            run_ac020_mix(&engine);
        }));
    }
    let started = Instant::now();
    barrier.wait();
    for handle in handles {
        handle.join().expect("reader thread");
    }
    let concurrent_ms = started.elapsed().as_millis() as u64;

    let post = engine.cache_status_per_worker_for_test("post");
    assert_eq!(post.len(), worker_count);

    // Build per-worker telemetry as JSON-encoded bytes by hand so we
    // do not pull serde_json into the test crate. Field order matches
    // §6 of the G.3.5 prompt.
    let mut per_worker = String::from("[");
    for (idx, (p, q)) in pre.iter().zip(post.iter()).enumerate() {
        let delta_hit = i64::from(q.cache_hit) - i64::from(p.cache_hit);
        let delta_miss = i64::from(q.cache_miss) - i64::from(p.cache_miss);
        let delta_total = delta_hit + delta_miss;
        let delta_miss_rate =
            if delta_total > 0 { (delta_miss as f64) / (delta_total as f64) } else { 0.0 };
        // SQLite default cache_size is -2000 (KiB) => 2 MiB per
        // connection. No production override is in place on the F.0
        // reader connections (only `journal_mode=WAL` and `query_only=ON`
        // PRAGMAs run at open time), so the limit assumed here is the
        // canonical default.
        let cache_size_limit_bytes: f64 = 2.0 * 1024.0 * 1024.0;
        let pct = (q.cache_used_bytes as f64) / cache_size_limit_bytes;
        if idx > 0 {
            per_worker.push(',');
        }
        per_worker.push_str(&format!(
            "{{\"worker_idx\":{wi},\"pre_hit\":{ph},\"pre_miss\":{pm},\"pre_used_bytes\":{pu},\
\"post_hit\":{qh},\"post_miss\":{qm},\"post_used_bytes\":{qu},\"delta_hit\":{dh},\
\"delta_miss\":{dm},\"delta_total\":{dt},\"delta_miss_rate\":{dmr:.6},\
\"cache_used_post_pct_of_limit\":{pct:.6}}}",
            wi = idx,
            ph = p.cache_hit,
            pm = p.cache_miss,
            pu = p.cache_used_bytes,
            qh = q.cache_hit,
            qm = q.cache_miss,
            qu = q.cache_used_bytes,
            dh = delta_hit,
            dm = delta_miss,
            dt = delta_total,
            dmr = delta_miss_rate,
            pct = pct,
        ));
    }
    per_worker.push(']');

    let body = format!(
        "{{\"worker_count\":{wc},\"concurrent_ms\":{cm},\"cache_size_limit_bytes_assumed\":{lim},\
\"cache_size_limit_source\":\"sqlite default (-2000 KiB = 2 MiB per connection); no PRAGMA cache_size override on F.0 reader open path\",\
\"per_worker_telemetry\":{pw}}}",
        wc = worker_count,
        cm = concurrent_ms,
        lim = 2 * 1024 * 1024,
        pw = per_worker,
    );

    eprintln!("G3_5_TELEMETRY_JSON={body}");
    std::fs::write(&output_path, body).expect("write G.3.5 sidecar JSON");
}

// ── A.3 secondary diagnostics ────────────────────────────────────────────────

const A3_EVIDENCE_DIR: &str = "dev/plan/runs/A3-evidence";

fn a3_evidence_path(name: &str) -> std::path::PathBuf {
    // Resolve relative to repo root (two levels up from tests/).
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest.ancestors().nth(4).expect("repo root").to_path_buf();
    let dir = repo_root.join(A3_EVIDENCE_DIR);
    std::fs::create_dir_all(&dir).expect("create evidence dir");
    dir.join(name)
}

/// A.3.2 — In-process timing counters for the concurrent read path.
/// Measures total wall time per `Engine::search()`. Since `RoutedEmbedder` has no
/// delay, search_us ≈ borrow_wait + read_search_in_tx. Splitting those requires
/// production hooks; counters_collection_status is `partial`.
#[test]
#[ignore = "A.3 diagnostic: set AC020_PHASE=concurrent to opt in"]
fn ac_a3_counters_concurrent() {
    if std::env::var("AC020_PHASE").as_deref() != Ok("concurrent") {
        return;
    }

    let (_dir, path) = fixture_path("a3_counters_concurrent");
    let embedder = Arc::new(RoutedEmbedder::new(3));
    let opened = Engine::open_with_embedder_for_test(&path, embedder.clone()).expect("open");
    seed_ac020_fixture(&opened.engine);

    let engine = Arc::new(opened.engine);
    let barrier = Arc::new(Barrier::new(AC020_THREADS + 1));
    let all_search_ms: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));
    let all_embed_ms: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));

    let mut handles = Vec::with_capacity(AC020_THREADS);
    for _ in 0..AC020_THREADS {
        let engine = Arc::clone(&engine);
        let barrier = Arc::clone(&barrier);
        let search_sink = Arc::clone(&all_search_ms);
        let embed_sink = Arc::clone(&all_embed_ms);
        let embedder = embedder.clone();
        handles.push(thread::spawn(move || {
            barrier.wait();
            let mut local_search = Vec::new();
            let mut local_embed = Vec::new();
            for _ in 0..AC020_ROUNDS_PER_THREAD {
                for query in ac020_queries() {
                    let t_embed = Instant::now();
                    let _ = embedder.embed(query);
                    local_embed.push(t_embed.elapsed().as_micros() as u64);

                    let t_search = Instant::now();
                    engine.search(query).expect("search");
                    local_search.push(t_search.elapsed().as_micros() as u64);
                }
            }
            search_sink.lock().unwrap().extend(local_search);
            embed_sink.lock().unwrap().extend(local_embed);
        }));
    }
    barrier.wait();
    for h in handles {
        h.join().expect("thread");
    }

    let search_us = all_search_ms.lock().unwrap();
    let embed_us = all_embed_ms.lock().unwrap();
    let queries_total = search_us.len() as u64;
    let search_total_us: u64 = search_us.iter().sum();
    let embed_total_us: u64 = embed_us.iter().sum();
    // proxy: borrow+read ≈ search - embed (embed is ~0 µs for RoutedEmbedder)
    let proxy_read_total_us = search_total_us.saturating_sub(embed_total_us);

    let search_per_query_us = search_total_us.checked_div(queries_total).unwrap_or(0);
    let embed_per_query_us = embed_total_us.checked_div(queries_total).unwrap_or(0);
    let proxy_per_query_us = proxy_read_total_us.checked_div(queries_total).unwrap_or(0);

    // 4 SQL statements per search (vec0 match, canonical lookup, soft-fallback probe, fts match)
    // — constant by code inspection of read_search_in_tx.
    let prepares_per_search: u64 = 4;

    let json = format!(
        r#"{{
  "reader_borrow_ms_total": "n/a: requires production hook",
  "reader_borrow_ms_per_query": "n/a: requires production hook",
  "embedder_us_total": {embed_total_us},
  "embedder_us_per_query": {embed_per_query_us},
  "search_us_total": {search_total_us},
  "search_us_per_query": {search_per_query_us},
  "proxy_borrow_plus_read_us_total": {proxy_read_total_us},
  "proxy_borrow_plus_read_us_per_query": {proxy_per_query_us},
  "prepares_per_search": {prepares_per_search},
  "queries_total": {queries_total},
  "counters_collection_status": "partial: borrow_wait and read_search_in_tx split requires production hooks; search_us covers both",
  "note": "embed is RoutedEmbedder (instant), so proxy_borrow_plus_read_us ≈ read_search_in_tx_us + borrow_wait_us"
}}"#
    );

    let out_path = a3_evidence_path("counters.json");
    std::fs::write(&out_path, &json).expect("write counters.json");
    eprintln!("A3_COUNTERS written to {}", out_path.display());
    eprintln!("  queries_total={queries_total}");
    eprintln!("  search_us_total={search_total_us}  per_query={search_per_query_us}");
    eprintln!("  embed_us_total={embed_total_us}  per_query={embed_per_query_us}");
    eprintln!("  proxy_read_us_total={proxy_read_total_us}  per_query={proxy_per_query_us}");
}

/// A.3.3 — EXPLAIN QUERY PLAN for the four read-path SQL statements.
#[test]
#[ignore = "A.3 diagnostic: opt-in with AC020_PHASE=concurrent"]
fn ac_a3_explain_query_plan() {
    if std::env::var("AC020_PHASE").as_deref() != Ok("concurrent") {
        return;
    }

    let (_dir, path) = fixture_path("a3_explain");
    let embedder = Arc::new(RoutedEmbedder::new(3));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    seed_ac020_fixture(&opened.engine);
    // Engine must stay alive while we open a raw connection (WAL, shared cache).
    let db_path = opened.engine.path().to_path_buf();

    // Open a raw rusqlite connection — sqlite_vec auto-extension is process-global
    // after the first Engine::open, so vec0 virtual tables are accessible.
    let conn = rusqlite::Connection::open(&db_path).expect("raw conn");
    conn.pragma_update(None, "query_only", "ON").ok();

    // (label, sql-with-literal-placeholders-for-EXPLAIN, explain-literal-substituted)
    // EXPLAIN QUERY PLAN requires parameter binding even though it doesn't execute.
    // Use rusqlite::params! with one dummy value per ?1 slot.
    let statements: &[(&str, &str, &str)] = &[
        (
            "vec0_match",
            "SELECT rowid FROM vector_default WHERE embedding MATCH vec_f32(?1) ORDER BY distance LIMIT 10",
            "SELECT rowid FROM vector_default WHERE embedding MATCH vec_f32('[1.0,0.0,0.0]') ORDER BY distance LIMIT 10",
        ),
        (
            "canonical_lookup",
            "SELECT body FROM canonical_nodes WHERE write_cursor = ?1 LIMIT 1",
            "SELECT body FROM canonical_nodes WHERE write_cursor = 1 LIMIT 1",
        ),
        (
            "soft_fallback_probe",
            "SELECT 1
             FROM search_index
             JOIN _fathomdb_vector_kinds ON _fathomdb_vector_kinds.kind = search_index.kind
             LEFT JOIN _fathomdb_projection_terminal
               ON _fathomdb_projection_terminal.write_cursor = search_index.write_cursor
             WHERE search_index MATCH ?1
              AND _fathomdb_projection_terminal.write_cursor IS NULL
             LIMIT 1",
            "SELECT 1
             FROM search_index
             JOIN _fathomdb_vector_kinds ON _fathomdb_vector_kinds.kind = search_index.kind
             LEFT JOIN _fathomdb_projection_terminal
               ON _fathomdb_projection_terminal.write_cursor = search_index.write_cursor
             WHERE search_index MATCH 'dummy'
              AND _fathomdb_projection_terminal.write_cursor IS NULL
             LIMIT 1",
        ),
        (
            "fts_match",
            "SELECT body FROM search_index WHERE search_index MATCH ?1 ORDER BY write_cursor",
            "SELECT body FROM search_index WHERE search_index MATCH 'dummy' ORDER BY write_cursor",
        ),
    ];

    let mut out = String::new();
    let mut regression = false;

    for (label, _parametric_sql, explain_sql) in statements {
        out.push_str(&format!("=== {label} ===\n"));
        let explain = format!("EXPLAIN QUERY PLAN {explain_sql}");
        let mut stmt = conn.prepare(&explain).expect("prepare explain");
        let rows: Vec<String> = stmt
            .query_map([], |row| {
                let detail: String = row.get(3)?;
                Ok(detail)
            })
            .expect("query_map")
            .flatten()
            .collect();
        for row in &rows {
            out.push_str(&format!("  {row}\n"));
            // Flag SCAN on canonical_nodes or search_index without SEARCH — potential regression.
            if row.contains("SCAN") && !row.contains("vec0") && !row.contains("fts5") {
                regression = true;
                out.push_str("  *** REGRESSION CANDIDATE: unexpected SCAN ***\n");
            }
        }
        out.push('\n');
    }

    out.push_str(&format!("regression_observed: {regression}\n"));

    let out_path = a3_evidence_path("explain-query-plan.txt");
    std::fs::write(&out_path, &out).expect("write explain-query-plan.txt");
    eprintln!("A3_EXPLAIN written to {}", out_path.display());
    eprintln!("{out}");
}

/// A.3.4 — sqlite3_threadsafe integer + PRAGMA compile_options.
///
/// Also probes the reader-connection pragma profile (cache_size, mmap_size,
/// page_size, synchronous, journal_mode, query_only).
#[test]
#[ignore = "A.3 diagnostic: opt-in with AC020_PHASE=concurrent"]
fn ac_a3_threadsafe_and_compile_options() {
    if std::env::var("AC020_PHASE").as_deref() != Ok("concurrent") {
        return;
    }

    let (_dir, path) = fixture_path("a3_threadsafe");
    // Open via Engine to register extension and create schema.
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    let db_path = opened.engine.path().to_path_buf();
    drop(opened);

    let conn = rusqlite::Connection::open(&db_path).expect("conn");

    // A.3.4a — THREADSAFE
    let threadsafe_val: i32 = unsafe { rusqlite::ffi::sqlite3_threadsafe() };
    std::fs::write(a3_evidence_path("threadsafe.txt"), format!("{threadsafe_val}\n"))
        .expect("write threadsafe.txt");
    eprintln!("A3_THREADSAFE={threadsafe_val}");

    // A.3.4b — compile_options
    let mut stmt = conn.prepare("PRAGMA compile_options").expect("prepare compile_options");
    let opts: Vec<String> =
        stmt.query_map([], |r| r.get::<_, String>(0)).expect("query_map").flatten().collect();
    let opts_text = opts.join("\n") + "\n";
    std::fs::write(a3_evidence_path("compile_options.txt"), &opts_text)
        .expect("write compile_options.txt");
    eprintln!("A3_COMPILE_OPTIONS ({} lines):\n{opts_text}", opts.len());

    // A.3.4c — reader pragma profile (WAL + query_only reader mimicking production)
    conn.pragma_update(None, "journal_mode", "WAL").ok();
    conn.pragma_update(None, "query_only", "ON").ok();
    let journal_mode: String =
        conn.pragma_query_value(None, "journal_mode", |r| r.get(0)).unwrap_or_default();
    let query_only: i64 = conn.pragma_query_value(None, "query_only", |r| r.get(0)).unwrap_or(0);
    let cache_size: i64 = conn.pragma_query_value(None, "cache_size", |r| r.get(0)).unwrap_or(0);
    let mmap_size: i64 = conn.pragma_query_value(None, "mmap_size", |r| r.get(0)).unwrap_or(0);
    let page_size: i64 = conn.pragma_query_value(None, "page_size", |r| r.get(0)).unwrap_or(0);
    let synchronous: i64 = conn.pragma_query_value(None, "synchronous", |r| r.get(0)).unwrap_or(0);

    let pragma_json = format!(
        r#"{{
  "journal_mode": "{journal_mode}",
  "query_only": {query_only},
  "cache_size": {cache_size},
  "mmap_size": {mmap_size},
  "page_size": {page_size},
  "synchronous": {synchronous}
}}"#
    );
    std::fs::write(a3_evidence_path("reader_pragmas.json"), &pragma_json)
        .expect("write reader_pragmas.json");
    eprintln!("A3_READER_PRAGMAS: {pragma_json}");
}
