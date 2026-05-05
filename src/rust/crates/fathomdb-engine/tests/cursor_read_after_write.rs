//! Concurrent reader/writer test that pins the read-after-write cursor
//! invariant from `dev/design/engine.md` § Cursor contract and AC-059b /
//! REQ-013 / REQ-055.
//!
//! Invariant under test: the number of canonical rows visible to a
//! `search` MUST be ≤ the search's reported `projection_cursor`. Because
//! every committed write advances the engine cursor by exactly one and
//! inserts exactly one canonical row in this fixture, the row count the
//! search returned is a lower bound on "how many writes the reader's
//! snapshot saw"; that value can never exceed the cursor the reader
//! also returned without violating the cursor contract.
//!
//! Failure mode under test: the engine loads `next_cursor` from the
//! writer-side atomic before the reader connection runs its query; a
//! writer commit between those two events appears in the reader's WAL
//! snapshot but not in the reported cursor.
//!
//! Bounded operationally: 1,000 search calls, hard wall-clock cap of
//! 30 s, writer throttled.
//!
//! Runtime-budget category: long-run only (~1000-iteration race fixture).
//! `agent-verify.sh` skips this test entirely for runtime budget; the
//! ~1000-iteration race fixture is exercised only by `scripts/check.sh`
//! with `AGENT_LONG=1`. There is no smoke variant — the AC-059b evidence
//! comes exclusively from the long-run gate.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use fathomdb_engine::{Engine, PreparedWrite};
use tempfile::TempDir;

fn long_run_enabled() -> bool {
    std::env::var_os("AGENT_LONG").is_some()
}

#[test]
fn projection_cursor_bounds_observed_row_count() {
    if !long_run_enabled() {
        return;
    }

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("cursor_race.sqlite");
    let opened = Engine::open(&path).expect("open");
    let engine = Arc::new(opened.engine);

    let stop = Arc::new(AtomicBool::new(false));
    let writer = {
        let engine = Arc::clone(&engine);
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let _ = engine.write(&[PreparedWrite::Node {
                    kind: "doc".to_string(),
                    body: "needle".to_string(),
                    source_id: None,
                }]);
                thread::sleep(Duration::from_micros(50));
            }
        })
    };

    let iterations = 1000usize;
    let mut violations = 0usize;
    let started = Instant::now();
    for i in 0..iterations {
        if started.elapsed() > Duration::from_secs(30) {
            stop.store(true, Ordering::Relaxed);
            writer.join().expect("writer thread");
            panic!("cursor invariant test exceeded 30 s wall clock at iteration {i}");
        }
        let result = engine.search("needle").expect("search");
        // results[0] is the compiled SQL string from `compile_text_query`;
        // every entry after that is a body row from canonical_nodes.
        let row_count = result.results.iter().filter(|s| s.as_str() == "needle").count() as u64;
        if row_count > result.projection_cursor {
            violations += 1;
            if violations <= 5 {
                eprintln!(
                    "iter {i}: rows_returned={row_count} > projection_cursor={}",
                    result.projection_cursor,
                );
            }
        }
    }

    stop.store(true, Ordering::Relaxed);
    writer.join().expect("writer thread");

    assert_eq!(
        violations, 0,
        "cursor invariant violated {violations}/{iterations} times: search returned more \
         matching rows than the reported projection_cursor permits — projection_cursor was \
         derived from a writer-side atomic that races ahead of the reader's WAL snapshot",
    );
}
