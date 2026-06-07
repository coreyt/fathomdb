//! Slice 6 (B2) — FTS5 tokenizer latency experiment. **THROWAWAY MEASUREMENT
//! HARNESS — NOT a gate.** Gated behind `SLICE6_EXPERIMENT=1` so it never runs
//! in `cargo test` / `agent-verify` / CI. Produces the measured data for
//! `dev/plans/runs/0.8.0-slice-6-tokenizer-experiment-*.md`.
//!
//! Faithfulness contract: it reproduces the engine's text-search path exactly —
//! the same FTS5 schema (`search_index(body, kind UNINDEXED, write_cursor
//! UNINDEXED)`), the same deterministic synthetic corpus + seeds + Zipfian body
//! generator as `perf_gates::seed_ac012_corpus`, the same single-token query
//! band, and the same SQL the engine runs (`lib.rs:3923-3927`):
//!   SELECT body, kind, write_cursor, bm25(search_index)
//!   FROM search_index WHERE search_index MATCH ?1 ORDER BY write_cursor
//! It varies ONE thing the engine fixes at migration time: the `tokenize=`
//! clause. It does NOT change any production tokenizer / migration / AC.
//!
//! It bypasses the ReaderWorkerPool (a constant per-config overhead), so its
//! absolute p50 reads slightly below the real `ac_012` gate; the cross-config
//! deltas are faithful and the current-config absolute is cross-checked against
//! the real `ac_012` run (Q1).

use rusqlite::Connection;
use std::time::{Duration, Instant};

// ── Deterministic generators (byte-copied from perf_gates.rs so the corpus is
//    identical to the real ac_012 fixture) ──────────────────────────────────
struct SeededRng {
    state: u64,
}
impl SeededRng {
    fn new(seed: u64) -> Self {
        Self { state: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1) }
    }
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.state
    }
    fn next_in(&mut self, bound: usize) -> usize {
        (self.next_u64() as usize) % bound
    }
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / ((1u64 << 53) as f64)
    }
}

fn perf_vocab() -> Vec<String> {
    let mut out = Vec::with_capacity(1024);
    for i in 0..1024 {
        let a = (b'a' + ((i / 26 / 26) % 26) as u8) as char;
        let b = (b'a' + ((i / 26) % 26) as u8) as char;
        let c = (b'a' + (i % 26) as u8) as char;
        out.push(format!("{a}{b}{c}{i:04}"));
    }
    out
}

fn zipf_index(rng: &mut SeededRng, cumulative: &[f64]) -> usize {
    let r = rng.next_f64() * cumulative[cumulative.len() - 1];
    match cumulative.binary_search_by(|w| w.partial_cmp(&r).unwrap_or(std::cmp::Ordering::Equal)) {
        Ok(idx) => idx,
        Err(idx) => idx.min(cumulative.len() - 1),
    }
}
fn zipf_cumulative(vocab_size: usize) -> Vec<f64> {
    let mut cumulative = Vec::with_capacity(vocab_size);
    let mut acc = 0.0_f64;
    for k in 1..=vocab_size {
        acc += 1.0_f64 / k as f64;
        cumulative.push(acc);
    }
    cumulative
}
fn synth_chunk_body(rng: &mut SeededRng, vocab: &[String], cumulative: &[f64]) -> String {
    let mut body = String::with_capacity(512);
    let token_count = 55 + rng.next_in(20);
    for i in 0..token_count {
        if i > 0 {
            body.push(' ');
        }
        body.push_str(&vocab[zipf_index(rng, cumulative)]);
    }
    body
}
fn ac012_query_token_band(vocab: &[String]) -> Vec<String> {
    let lo = vocab.len() / 10;
    let hi = vocab.len() / 2;
    vocab[lo..hi].to_vec()
}

const PERF_SAMPLES: usize = 1_000;

/// Build an in-memory FTS5 index with the given tokenize clause and seed it with
/// the identical synthetic AC-012 corpus (same seeds as perf_gates). Returns the
/// open connection + seed wall-clock.
fn build_index(tokenize: &str, n: usize) -> (Connection, Duration) {
    let conn = Connection::open_in_memory().expect("open in-memory");
    conn.execute_batch(&format!(
        "CREATE VIRTUAL TABLE search_index USING fts5(\
            body, kind UNINDEXED, write_cursor UNINDEXED, tokenize = '{tokenize}');"
    ))
    .expect("create fts5");

    let vocab = perf_vocab();
    let cumulative = zipf_cumulative(vocab.len());
    let mut rng = SeededRng::new(0x0AC0_12C0_12C0);
    let started = Instant::now();
    conn.execute_batch("BEGIN").unwrap();
    {
        let mut stmt = conn
            .prepare("INSERT INTO search_index(body, kind, write_cursor) VALUES(?1, 'doc', ?2)")
            .unwrap();
        for i in 0..n {
            let body = synth_chunk_body(&mut rng, &vocab, &cumulative);
            stmt.execute(rusqlite::params![body, (i + 1) as i64]).expect("seed insert");
        }
    }
    conn.execute_batch("COMMIT").unwrap();
    (conn, started.elapsed())
}

/// The held-out single-token query set, identical to ac_012.
fn ac012_queries() -> Vec<String> {
    let vocab = perf_vocab();
    let band = ac012_query_token_band(&vocab);
    let mut rng = SeededRng::new(0x0AC0_120D_EC0D_E000);
    (0..PERF_SAMPLES).map(|_| band[rng.next_in(band.len())].clone()).collect()
}

/// Run the EXACT engine text SQL for each query; warmup pass discarded, then a
/// measurement pass. Returns sorted-percentile p50/p99 (finer than the gate's
/// power-of-two `percentile_ceil`; the gate bucketing is discussed in the doc).
fn measure_latency(conn: &Connection, queries: &[String]) -> (Duration, Duration, f64) {
    let sql = "SELECT body, kind, write_cursor, bm25(search_index) FROM search_index \
               WHERE search_index MATCH ?1 ORDER BY write_cursor";
    // match_expression form from fathomdb-query::compile_text_query: a single
    // token becomes "\"token\"".
    let exprs: Vec<String> =
        queries.iter().map(|q| format!("\"{}\"", q.replace('"', "\"\""))).collect();

    // Warmup (discarded).
    for e in &exprs {
        let mut stmt = conn.prepare(sql).unwrap();
        let _ = stmt
            .query_map([e.as_str()], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
    }
    // Measurement.
    let mut samples = Vec::with_capacity(exprs.len());
    let mut total_rows = 0usize;
    for e in &exprs {
        let started = Instant::now();
        let mut stmt = conn.prepare(sql).unwrap();
        let rows: Vec<(String, String, i64, f64)> = stmt
            .query_map([e.as_str()], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        samples.push(started.elapsed());
        total_rows += rows.len();
    }
    samples.sort();
    let p = |q: f64| samples[((samples.len() as f64 * q) as usize).min(samples.len() - 1)];
    let avg_rows = total_rows as f64 / exprs.len() as f64;
    (p(0.50), p(0.99), avg_rows)
}

fn page_count(conn: &Connection) -> i64 {
    conn.query_row("PRAGMA page_count", [], |r| r.get(0)).unwrap_or(-1)
}

const CONFIGS: &[(&str, &str)] = &[
    ("porter unicode61 remove_diacritics 2", "CURRENT (0.8.0 Slice-5)"),
    ("unicode61 remove_diacritics 2", "drop porter stemmer"),
    ("porter unicode61", "porter, default diacritics(1)"),
    ("unicode61", "≈ v0.7.2 default baseline"),
    ("porter ascii", "porter over ascii tokenizer"),
    ("ascii", "ascii only (lightest)"),
];

#[test]
fn slice6_q1q2_config_tier_sweep() {
    if std::env::var_os("SLICE6_EXPERIMENT").is_none() {
        return;
    }
    let tiers: &[usize] = &[10_000, 100_000];
    let queries = ac012_queries();
    eprintln!("\n[S6][MEASURE] === Q1/Q2: config × tier latency sweep (p50/p99 over {PERF_SAMPLES} single-token queries) ===");
    eprintln!("config | tier | seed_ms | pages | avg_rows_matched | p50_ms | p99_ms");
    for &n in tiers {
        for (cfg, note) in CONFIGS {
            let (conn, seed) = build_index(cfg, n);
            let pages = page_count(&conn);
            let (p50, p99, avg_rows) = measure_latency(&conn, &queries);
            eprintln!(
                "S6ROW\t{cfg}\t{n}\t{}\t{pages}\t{:.0}\t{:.3}\t{:.3}\t({note})",
                seed.as_millis(),
                avg_rows,
                p50.as_secs_f64() * 1000.0,
                p99.as_secs_f64() * 1000.0,
            );
        }
    }
}

// ── Q2 quality proxy: an English-morphology + diacritics corpus that exposes
//    what porter stemming and remove_diacritics actually buy. ────────────────
struct QDoc {
    body: &'static str,
}
const QUALITY_CORPUS: &[QDoc] = &[
    QDoc { body: "the system is running several background processes" },
    QDoc { body: "she runs the nightly batch job" },
    QDoc { body: "he ran the migration last week" },
    QDoc { body: "optimizing the optimizer for optimal performance" },
    QDoc { body: "we met at the café near the old cathedral" },
    QDoc { body: "a naïve approach to résumé parsing" },
    QDoc { body: "tokenization tokenizes each token deterministically" },
    QDoc { body: "the connection connects connected clients" },
];
/// (query, body that SHOULD match if the tokenizer folds morphology/diacritics).
const QUALITY_QUERIES: &[(&str, &str, &str)] = &[
    ("run", "she runs the nightly batch job", "stem: run→runs"),
    ("running", "the system is running several background processes", "exact"),
    ("ran", "he ran the migration last week", "exact (porter does NOT fold ran→run)"),
    ("optimize", "optimizing the optimizer for optimal performance", "stem: optimize→optimizing"),
    ("cafe", "we met at the café near the old cathedral", "diacritics: cafe→café"),
    ("naive", "a naïve approach to résumé parsing", "diacritics: naive→naïve"),
    ("resume", "a naïve approach to résumé parsing", "diacritics: resume→résumé"),
    (
        "connect",
        "the connection connects connected clients",
        "stem: connect→connects/connected/connection",
    ),
];

#[test]
fn slice6_q2_quality_proxy() {
    if std::env::var_os("SLICE6_EXPERIMENT").is_none() {
        return;
    }
    eprintln!("\n[S6][MEASURE] === Q2: FTS-quality proxy (does config match the morphological/diacritic variant?) ===");
    for (cfg, note) in CONFIGS {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(&format!(
            "CREATE VIRTUAL TABLE search_index USING fts5(body, kind UNINDEXED, write_cursor UNINDEXED, tokenize = '{cfg}');"
        ))
        .unwrap();
        for (i, d) in QUALITY_CORPUS.iter().enumerate() {
            conn.execute(
                "INSERT INTO search_index(body, kind, write_cursor) VALUES(?1,'doc',?2)",
                rusqlite::params![d.body, (i + 1) as i64],
            )
            .unwrap();
        }
        let mut matched = 0usize;
        let mut detail = String::new();
        for (q, expected, _why) in QUALITY_QUERIES {
            let expr = format!("\"{q}\"");
            let hit: bool = conn
                .prepare("SELECT body FROM search_index WHERE search_index MATCH ?1")
                .unwrap()
                .query_map([expr.as_str()], |r| r.get::<_, String>(0))
                .unwrap()
                .filter_map(|r| r.ok())
                .any(|b| b == *expected);
            if hit {
                matched += 1;
            }
            detail.push_str(if hit { "1" } else { "0" });
        }
        eprintln!(
            "S6QUAL\t{cfg}\trecall={}/{}\tbits={detail}\t({note})",
            matched,
            QUALITY_QUERIES.len(),
        );
    }
    eprintln!("S6QUAL_LEGEND\tbits = [run running ran optimize cafe naive resume connect]");
}

// ── Q3: where is the cost? single-token vs multi-token, porter vs non-porter ─
#[test]
fn slice6_q3_cost_attribution() {
    if std::env::var_os("SLICE6_EXPERIMENT").is_none() {
        return;
    }
    let n = 100_000;
    let vocab = perf_vocab();
    let band = ac012_query_token_band(&vocab);
    let mut rng = SeededRng::new(0x0AC0_120D_EC0D_E000);
    // 1-, 2-, 3-token query sets drawn from the same band.
    let mk = |toks: usize, rng: &mut SeededRng| -> Vec<String> {
        (0..PERF_SAMPLES)
            .map(|_| {
                (0..toks)
                    .map(|_| band[rng.next_in(band.len())].clone())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect()
    };
    let q1 = mk(1, &mut rng);
    let q2 = mk(2, &mut rng);
    let q3 = mk(3, &mut rng);

    eprintln!("\n[S6][MEASURE] === Q3: cost attribution @ n={n} ===");
    for cfg in ["porter unicode61 remove_diacritics 2", "unicode61"] {
        let (conn, _seed) = build_index(cfg, n);
        for (label, qs) in [("1tok", &q1), ("2tok", &q2), ("3tok", &q3)] {
            let (p50, p99, avg_rows) = measure_latency(&conn, qs);
            eprintln!(
                "S6Q3\t{cfg}\t{label}\tavg_rows={:.0}\tp50_ms={:.3}\tp99_ms={:.3}",
                avg_rows,
                p50.as_secs_f64() * 1000.0,
                p99.as_secs_f64() * 1000.0,
            );
        }
    }
}
