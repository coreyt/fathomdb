//! PR-9 diagnostic micro-benchmark (NOT a gate; opt-in via AGENT_LONG).
//!
//! Isolates two questions the PR-9 pre-flight raised:
//!   1. Does the watchdog's spawn-a-thread-per-embed add material overhead vs
//!      a direct `embed()` call? (Hypothesis under test: per-call std::thread
//!      spawn is perf-neutral because candle's matmul fans out onto a single
//!      process-wide rayon pool regardless of the caller thread.)
//!   2. Is the ~seconds-per-embed seen in the seed a debug-build artifact and
//!      a function of document length? (short vs long input, same build.)
//!
//! Run:
//!   AGENT_LONG=1 cargo test -p fathomdb-engine --features default-embedder \
//!     --test pr9_embed_microbench -- --nocapture
//! Compare debug vs release by adding `--release`.

#![cfg(feature = "default-embedder")]

use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use fathomdb_embedder::CandleBgeEmbedder;
use fathomdb_embedder_api::Embedder;

/// Mirror of `embed_with_watchdog` in lib.rs: spawn a thread per embed, wait
/// on a channel with a timeout. Used to measure the spawn overhead in
/// isolation against a direct call.
fn watchdog_embed(embedder: &Arc<dyn Embedder>, body: &str, timeout: Duration) -> Vec<f32> {
    let (tx, rx) = mpsc::channel();
    let e = Arc::clone(embedder);
    let b = body.to_string();
    thread::spawn(move || {
        let _ = tx.send(e.embed(&b));
    });
    rx.recv_timeout(timeout).expect("recv").expect("embed ok")
}

fn mean_ms(times: &[Duration]) -> f64 {
    times.iter().map(|d| d.as_secs_f64() * 1000.0).sum::<f64>() / times.len() as f64
}

#[test]
fn pr9_microbench_watchdog_overhead() {
    if std::env::var_os("AGENT_LONG").is_none() {
        eprintln!("[skip] AGENT_LONG not set; PR-9 micro-benchmark is opt-in");
        return;
    }
    if std::env::var("FATHOMDB_SKIP_NETWORK_TESTS").is_ok() {
        eprintln!("[skip] FATHOMDB_SKIP_NETWORK_TESTS set");
        return;
    }
    let build = if cfg!(debug_assertions) { "debug" } else { "release" };
    let embedder: Arc<dyn Embedder> =
        Arc::new(CandleBgeEmbedder::new().expect("construct real bge embedder"));

    let short = "the quick brown fox jumps over the lazy dog";
    // ~ a few hundred tokens, to approach the 512-token truncation ceiling
    // that the long corpus docs (cnn_dailymail / qmsum) hit.
    let long = short.repeat(80);

    // Warm up (first call pays any one-time init).
    let _ = embedder.embed(short).expect("warmup");

    const N: usize = 30;
    let mut direct = Vec::with_capacity(N);
    for _ in 0..N {
        let t = Instant::now();
        let _ = embedder.embed(short).expect("direct embed");
        direct.push(t.elapsed());
    }
    let mut watchdog = Vec::with_capacity(N);
    for _ in 0..N {
        let t = Instant::now();
        let _ = watchdog_embed(&embedder, short, Duration::from_secs(30));
        watchdog.push(t.elapsed());
    }

    const LN: usize = 8;
    let mut long_direct = Vec::with_capacity(LN);
    for _ in 0..LN {
        let t = Instant::now();
        let _ = embedder.embed(&long).expect("long embed");
        long_direct.push(t.elapsed());
    }

    let d = mean_ms(&direct);
    let w = mean_ms(&watchdog);
    let l = mean_ms(&long_direct);
    eprintln!("PR9_BENCH build={build}");
    eprintln!("PR9_BENCH short_direct_ms={d:.1} (n={N})");
    eprintln!("PR9_BENCH short_watchdog_ms={w:.1} (n={N})");
    eprintln!("PR9_BENCH watchdog_overhead_ms={:.2} ratio={:.3}", w - d, w / d);
    eprintln!("PR9_BENCH long_direct_ms={l:.1} (n={LN}) long/short={:.1}x", l / d);
}
