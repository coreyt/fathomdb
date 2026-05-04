//! Lifecycle observability tests bound to AC-001..AC-010.
//!
//! Pure-type tests exercise data-type contracts pinned by
//! `dev/design/lifecycle.md` and `dev/acceptance.md`. Behavior tests that
//! depend on fixtures not yet pinned (slow-cte, corruption injection,
//! one-thread poison) are `#[ignore]`d with the AC tag and the dep reason;
//! removing the ignore once the fixture lands flips them green or surfaces
//! a real gap.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::sync::{Arc, Mutex};

use fathomdb_engine::lifecycle::{
    Event, EventCategory, EventSource, Phase, ProfileRecord, ProjectionStatus, SlowStatement,
    StressFailureContext, Subscriber,
};
use fathomdb_engine::{CounterSnapshot, Engine, EngineOpenError, PreparedWrite};
use tempfile::TempDir;

fn fixture() -> (TempDir, Engine) {
    let dir = TempDir::new().unwrap();
    let engine = Engine::open(dir.path().join("observability.sqlite")).expect("engine open").engine;
    (dir, engine)
}

#[derive(Default)]
struct CapturingSubscriber {
    events: Mutex<Vec<Event>>,
    profile_records: Mutex<Vec<ProfileRecord>>,
    slow_statements: Mutex<Vec<SlowStatement>>,
    stress_failures: Mutex<Vec<StressFailureContext>>,
}

impl Subscriber for CapturingSubscriber {
    fn on_event(&self, event: &Event) {
        self.events.lock().unwrap().push(event.clone());
    }

    fn on_profile(&self, record: &ProfileRecord) {
        self.profile_records.lock().unwrap().push(*record);
    }

    fn on_slow_statement(&self, signal: &SlowStatement) {
        self.slow_statements.lock().unwrap().push(signal.clone());
    }

    fn on_stress_failure(&self, context: &StressFailureContext) {
        self.stress_failures.lock().unwrap().push(context.clone());
    }
}

// AC-001: Phase enum is a typed value, not a substring of free text.
#[test]
fn ac_001_phase_enum_has_five_typed_variants() {
    let variants = [Phase::Started, Phase::Slow, Phase::Heartbeat, Phase::Finished, Phase::Failed];

    // Pattern-match each variant exhaustively to confirm enum shape.
    for phase in variants {
        match phase {
            Phase::Started | Phase::Slow | Phase::Heartbeat | Phase::Finished | Phase::Failed => {}
        }
    }

    assert_ne!(Phase::Started, Phase::Slow);
    assert_ne!(Phase::Started, Phase::Heartbeat);
    assert_ne!(Phase::Started, Phase::Finished);
    assert_ne!(Phase::Started, Phase::Failed);
    assert_ne!(Phase::Finished, Phase::Failed);
    assert_ne!(Phase::Slow, Phase::Heartbeat);
}

// AC-001 supporting: Event struct exposes typed source + category.
#[test]
fn ac_001_event_struct_carries_typed_source_and_category() {
    let event = Event {
        phase: Phase::Started,
        source: EventSource::Engine,
        category: EventCategory::Writer,
        code: None,
    };
    assert_eq!(event.phase, Phase::Started);
    assert_eq!(event.source, EventSource::Engine);
    assert_eq!(event.category, EventCategory::Writer);
    assert_eq!(event.code, None);
}

// AC-002: No log files written without subscriber.
//
// Measurement (`dev/acceptance.md`): "Snapshot recursive directory tree
// of `$PWD`, `$HOME`, `$XDG_*`, `$TMPDIR` pre+post; assert diff = subset
// of allow-list paths."
//
// Two-part assertion:
// 1. Every new file inside the DB's parent directory matches the
//    documented allow-list (DB file, `.lock`, `-wal`, `-shm`, optional
//    `-journal`) per
//    ADR-0.6.0-database-lock-mechanism-reader-pool-revision.
// 2. No new file in `$PWD` / `$HOME` / `$XDG_*` whose path or name
//    contains a fathomdb signature appears — i.e. the engine does not
//    create a private log / telemetry spool outside the DB path.
//    Pre/post diff under noisy roots is unavoidable in a parallel test
//    process; the signature filter is the strongest assertion we can
//    bind without serializing the whole runner.
#[test]
fn ac_002_no_log_files_without_subscriber() {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    fn walk(root: &std::path::Path, out: &mut BTreeSet<PathBuf>, depth: u32) {
        if depth > 6 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(root) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            out.insert(path.clone());
            if meta.is_dir() && !meta.file_type().is_symlink() {
                walk(&path, out, depth + 1);
            }
        }
    }

    fn snapshot(roots: &[PathBuf]) -> BTreeSet<PathBuf> {
        let mut out = BTreeSet::new();
        for root in roots {
            walk(root, &mut out, 0);
        }
        out
    }

    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("nolog.sqlite");
    let db_parent = db_path.parent().expect("db parent").to_path_buf();

    // AC-002 measurement-protocol roots: $PWD, $HOME, $XDG_*, $TMPDIR.
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(p) = std::env::current_dir() {
        roots.push(p);
    }
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home));
    }
    for var in ["XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME", "XDG_STATE_HOME"] {
        if let Some(v) = std::env::var_os(var) {
            roots.push(PathBuf::from(v));
        }
    }
    roots.push(std::env::temp_dir());

    let before = snapshot(&roots);

    let opened = Engine::open(&db_path).expect("open");
    opened
        .engine
        .write(&[PreparedWrite::Node { kind: "doc".to_string(), body: "hello".to_string() }])
        .expect("write");
    let _ = opened.engine.search("hello").expect("search");
    opened.engine.close().expect("close");

    let after = snapshot(&roots);
    let new: Vec<&PathBuf> = after.difference(&before).collect();

    let allowed_names = [
        "nolog.sqlite",
        "nolog.sqlite.lock",
        "nolog.sqlite-wal",
        "nolog.sqlite-shm",
        "nolog.sqlite-journal",
    ];

    for path in &new {
        // Part 1: every new file inside the DB parent must be on the
        // allow-list.
        if path.starts_with(&db_parent) {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // Allow the DB parent itself if it shows up as "new" (it
            // was created as part of the TempDir).
            if *path == &db_parent {
                continue;
            }
            assert!(
                allowed_names.contains(&name),
                "engine created unexpected artifact inside DB parent: {}",
                path.display(),
            );
            continue;
        }

        // Part 2: no fathomdb-signature file appears anywhere in the
        // measurement roots.
        let lossy = path.to_string_lossy().to_lowercase();
        assert!(
            !lossy.contains("fathomdb") && !lossy.contains("fathom_"),
            "engine created a fathomdb-named artifact outside the DB path: {}",
            path.display(),
        );
    }
}

// AC-003a: Writer events flow to host subscriber.
#[test]
fn ac_003a_writer_events_flow_to_subscriber() {
    let (_dir, engine) = fixture();
    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());
    let _ =
        engine.write(&[PreparedWrite::Node { kind: "doc".to_string(), body: "hello".to_string() }]);
    let captured = sink.events.lock().unwrap();
    assert!(captured
        .iter()
        .any(|e| e.source == EventSource::Engine && e.category == EventCategory::Writer));
}

// AC-003b: Search events flow to host subscriber.
#[test]
fn ac_003b_search_events_flow_to_subscriber() {
    let (_dir, engine) = fixture();
    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());
    let _ = engine.search("hello");
    let captured = sink.events.lock().unwrap();
    assert!(captured
        .iter()
        .any(|e| e.source == EventSource::Engine && e.category == EventCategory::Search));
}

// AC-003c: Admin events flow to host subscriber.
#[test]
fn ac_003c_admin_events_flow_to_subscriber() {
    let (_dir, engine) = fixture();
    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());
    let _ = engine.write(&[PreparedWrite::AdminSchema {
        name: "things".to_string(),
        kind: "latest_state".to_string(),
        schema_json: "{}".to_string(),
        retention_json: "{}".to_string(),
    }]);
    let captured = sink.events.lock().unwrap();
    assert!(captured
        .iter()
        .any(|e| e.source == EventSource::Engine && e.category == EventCategory::Admin));
}

// AC-003d: Error events flow to host subscriber before failure raises.
#[test]
fn ac_003d_error_events_flow_to_subscriber() {
    let (_dir, engine) = fixture();
    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());
    let _ = engine.write(&[]); // empty batch -> WriteValidation
    let captured = sink.events.lock().unwrap();
    assert!(captured
        .iter()
        .any(|e| e.source == EventSource::Engine && e.category == EventCategory::Error));
}

// AC-004a: Counter snapshot exposes documented seven-key set, fresh-engine zero.
#[test]
fn ac_004a_counter_snapshot_key_set() {
    let (_dir, engine) = fixture();
    let snapshot = engine.counters();
    // Compile-level shape lock — these field accesses must compile.
    assert_eq!(snapshot.queries, 0);
    assert_eq!(snapshot.writes, 0);
    assert_eq!(snapshot.write_rows, 0);
    assert_eq!(snapshot.admin_ops, 0);
    assert_eq!(snapshot.cache_hit, 0);
    assert_eq!(snapshot.cache_miss, 0);
    assert!(snapshot.errors_by_code.is_empty());
    let _: BTreeMap<String, u64> = snapshot.errors_by_code.clone();
}

// AC-004b: Counter delta exact for write/query/admin keys after N=1,000
// mixed ops.
//
// Measurement (`dev/acceptance.md`): "Snapshot delta over N=1,000 mixed
// ops equals issued op counts exactly for `queries`, `writes`,
// `write_rows`, `admin_ops`. `cache_hit` / `cache_miss` are monotonic
// non-decreasing." Mix is 400 writes + 400 searches + 200 admin ops =
// 1,000 ops.
#[test]
fn ac_004b_counter_delta_exact_over_mixed_ops() {
    let (_dir, engine) = fixture();
    let s0 = engine.counters();
    for _ in 0..400 {
        engine
            .write(&[PreparedWrite::Node { kind: "doc".to_string(), body: "hello".to_string() }])
            .expect("write");
    }
    for _ in 0..400 {
        let _ = engine.search("hello").expect("search");
    }
    for i in 0..200 {
        engine
            .write(&[PreparedWrite::AdminSchema {
                name: format!("things_{}", i % 4),
                kind: "latest_state".to_string(),
                schema_json: "{}".to_string(),
                retention_json: "{}".to_string(),
            }])
            .expect("admin");
    }
    let s1 = engine.counters();
    assert_eq!(s1.writes - s0.writes, 400, "writes");
    assert_eq!(s1.write_rows - s0.write_rows, 400, "write_rows");
    assert_eq!(s1.queries - s0.queries, 400, "queries");
    assert_eq!(s1.admin_ops - s0.admin_ops, 200, "admin_ops");
    assert!(s1.cache_hit >= s0.cache_hit, "cache_hit monotonic non-decreasing");
    assert!(s1.cache_miss >= s0.cache_miss, "cache_miss monotonic non-decreasing");
}

// AC-004c: Counter snapshot read does not perturb counters.
#[test]
fn ac_004c_counter_snapshot_does_not_perturb() {
    let (_dir, engine) = fixture();
    let s0 = engine.counters();
    let s1 = engine.counters();
    assert_eq!(s0, s1);
}

// AC-005a: Per-statement profiling toggleable at runtime.
//
// Measurement (`dev/acceptance.md`): "Open engine; assert profiling
// disabled (no profile records on a fixture query); call enable-profiling
// API; assert subsequent fixture query emits ≥ 1 profile record."
#[test]
fn ac_005a_profiling_toggleable_at_runtime() {
    let (_dir, engine) = fixture();
    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());

    // Profiling disabled by default — fixture query emits no records.
    engine.set_profiling(false).expect("disable profiling");
    let _ = engine.search("hello").expect("search");
    assert_eq!(
        sink.profile_records.lock().unwrap().len(),
        0,
        "no profile records expected while profiling disabled"
    );

    // Enabling profiling makes the next fixture query emit ≥ 1 record.
    engine.set_profiling(true).expect("enable profiling");
    let _ = engine.search("hello").expect("search");
    let after = sink.profile_records.lock().unwrap().len();
    assert!(after >= 1, "expected ≥ 1 profile record after enabling profiling, saw {after}");

    // Disabling profiling stops further records (sanity check on the
    // runtime-toggle contract).
    engine.set_profiling(false).expect("disable profiling again");
    let frozen = sink.profile_records.lock().unwrap().len();
    let _ = engine.search("hello").expect("search");
    assert_eq!(sink.profile_records.lock().unwrap().len(), frozen);
}

// AC-005b: Profile record schema is typed numeric.
//
// Measurement: emit one profile record via AC-005a's protocol; assert
// all three fields present and numeric. We deliberately do not pin
// non-zero values — `step_count` and `cache_delta` are emitted as 0
// in 0.6.0 because `sqlite3_profile` does not surface them in its
// callback. AC-005b contract is "typed numeric", not "non-zero".
#[test]
fn ac_005b_profile_record_typed_numeric_fields() {
    let (_dir, engine) = fixture();
    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());
    engine.set_profiling(true).expect("enable profiling");
    let _ = engine.search("hello").expect("search");

    let records = sink.profile_records.lock().unwrap();
    let record = records.first().expect("at least one profile record");
    let _: u64 = record.wall_clock_ms;
    let _: u64 = record.step_count;
    let _: i64 = record.cache_delta;
}

// AC-006: SQLite-internal events surfaced with typed source tag.
#[test]
fn ac_006_sqlite_internal_events_typed_source() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("corrupt-open.sqlite");
    let opened = Engine::open(&path).expect("seed database");
    opened.engine.close().expect("close before corruption");
    corrupt_header(&path);

    let sink = Arc::new(CapturingSubscriber::default());
    let err = Engine::open_with_subscriber_for_test(&path, sink.clone())
        .expect_err("corrupted database must fail open");

    assert!(matches!(err, EngineOpenError::Corruption(_)));

    let captured = sink.events.lock().unwrap();
    assert!(captured.iter().any(|e| {
        e.source == EventSource::SqliteInternal && e.category == EventCategory::Corruption
    }));
}

// AC-007a: Slow-statement event when wall-clock crosses default threshold.
//
// Measurement: default threshold = 100 ms (REQ-006a). The
// deterministic-slow-cte fixture (≥ 200 ms guaranteed by recursive-CTE
// counter) emits exactly one slow-statement signal identifying the SQL.
#[test]
fn ac_007a_slow_statement_event_at_default_threshold() {
    let (_dir, engine) = fixture();
    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());
    engine.execute_for_test(SLOW_CTE).expect("slow cte");

    let signals = sink.slow_statements.lock().unwrap();
    assert_eq!(
        signals.len(),
        1,
        "expected exactly one slow-statement signal at default threshold, saw {}",
        signals.len(),
    );
    assert!(
        signals[0].statement.contains("RECURSIVE"),
        "slow signal must identify the statement; got: {:?}",
        signals[0].statement,
    );
    assert!(
        signals[0].wall_clock_ms >= 100,
        "slow signal wall_clock_ms must be ≥ 100 ms (default threshold); got {} ms",
        signals[0].wall_clock_ms,
    );
}

// AC-007b: Slow threshold reconfigurable at runtime.
//
// Measurement: set threshold = 500 ms; fast-fixture (≤ 200 ms) emits no
// slow-statement signal; slow-fixture (≥ 600 ms) emits exactly one.
#[test]
fn ac_007b_slow_threshold_reconfigurable() {
    let (_dir, engine) = fixture();
    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());
    engine.set_slow_threshold_ms(500).expect("set threshold");

    engine.execute_for_test(FAST_CTE).expect("fast cte");
    assert_eq!(
        sink.slow_statements.lock().unwrap().len(),
        0,
        "fast fixture must not emit a slow-statement signal at threshold=500 ms"
    );

    engine.execute_for_test(SLOW_CTE).expect("slow cte");
    let signals = sink.slow_statements.lock().unwrap();
    assert_eq!(
        signals.len(),
        1,
        "slow fixture must emit exactly one slow-statement signal at threshold=500 ms"
    );
    assert!(signals[0].wall_clock_ms >= 500);
}

// Deterministic-slow-cte fixture (AC-007a/b). Recursive CTE counter
// scales linearly with N. N values pinned to this runner's measured
// baseline (probe captured 2026-05-02 on aarch64 Linux):
//
// - N=100_000 → ~89 ms (FAST: < 200 ms required by AC-007b)
// - N=1_000_000 → ~800 ms (SLOW: ≥ 200 ms for AC-007a, ≥ 600 ms for AC-007b)
const FAST_CTE: &str = "WITH RECURSIVE c(x) AS (VALUES(1) UNION ALL \
                        SELECT x + 1 FROM c WHERE x < 100000) \
                        SELECT count(*) FROM c";
const SLOW_CTE: &str = "WITH RECURSIVE c(x) AS (VALUES(1) UNION ALL \
                        SELECT x + 1 FROM c WHERE x < 1000000) \
                        SELECT count(*) FROM c";

// AC-008: Slow signal participates in lifecycle attribution.
//
// Measurement: per `dev/design/lifecycle.md` § Slow and heartbeat
// policy, crossing the threshold produces TWO correlated facts —
// (i) a statement-level slow-statement signal, (ii) ≥ 1 lifecycle
// `phase == Slow` event during the operation's wall-clock window. The
// slow CTE fixture from AC-007a satisfies both.
#[test]
fn ac_008_slow_signal_feeds_lifecycle() {
    let (_dir, engine) = fixture();
    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());

    engine.execute_for_test(SLOW_CTE).expect("slow cte");

    let signals = sink.slow_statements.lock().unwrap();
    assert!(!signals.is_empty(), "expected at least one slow-statement signal");

    let events = sink.events.lock().unwrap();
    assert!(
        events.iter().any(|e| e.phase == Phase::Slow),
        "expected at least one lifecycle event with phase == Slow"
    );
}

// AC-009 supporting: Pure-type construction of StressFailureContext.
#[test]
fn ac_009_stress_failure_context_constructs() {
    let (_dir, engine) = fixture();
    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());

    engine.run_one_thread_poison_for_test().expect("poison fixture should emit");

    let captured = sink.stress_failures.lock().unwrap();
    let ctx = captured.first().expect("one-thread poison must emit a stress failure context");
    let _: u64 = ctx.thread_group_id;
    let _: String = ctx.op_kind.clone();
    let _: Vec<String> = ctx.last_error_chain.clone();
    let _: String = ctx.projection_state.clone();
    assert!(!ctx.op_kind.is_empty());
    assert!(!ctx.last_error_chain.is_empty());
    assert!(!ctx.projection_state.is_empty());
}

// AC-010: Projection-status enum coverage.
#[test]
fn ac_010_projection_status_enum_three_values() {
    let variants =
        [ProjectionStatus::Pending, ProjectionStatus::Failed, ProjectionStatus::UpToDate];
    for status in variants {
        match status {
            ProjectionStatus::Pending | ProjectionStatus::Failed | ProjectionStatus::UpToDate => {}
        }
    }
    assert_ne!(ProjectionStatus::Pending, ProjectionStatus::Failed);
    assert_ne!(ProjectionStatus::Pending, ProjectionStatus::UpToDate);
    assert_ne!(ProjectionStatus::Failed, ProjectionStatus::UpToDate);
}

fn corrupt_header(path: &std::path::Path) {
    let mut file = OpenOptions::new().read(true).write(true).open(path).unwrap();
    file.seek(SeekFrom::Start(0)).unwrap();
    file.write_all(b"not-a-sqlite-db!").unwrap();
    file.flush().unwrap();
}

// Compile-level: CounterSnapshot Default produces zeroed snapshot.
#[test]
fn counter_snapshot_default_is_zero() {
    let s = CounterSnapshot::default();
    assert_eq!(s.queries, 0);
    assert_eq!(s.writes, 0);
    assert_eq!(s.write_rows, 0);
    assert_eq!(s.admin_ops, 0);
    assert_eq!(s.cache_hit, 0);
    assert_eq!(s.cache_miss, 0);
    assert!(s.errors_by_code.is_empty());
}
