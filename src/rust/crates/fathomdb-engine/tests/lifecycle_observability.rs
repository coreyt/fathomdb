//! Phase 8 lifecycle observability red tests for AC-001..AC-010.
//!
//! Pure-type tests exercise data-type contracts pinned by
//! `dev/design/lifecycle.md` and `dev/acceptance.md`. Behavior tests that
//! depend on Phase 6/7 wiring (event emission, counter increments,
//! statement timing) are `#[ignore]`d with the AC tag and the dep reason;
//! removing the ignore once wiring lands flips them green or surfaces a
//! real gap.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use fathomdb_engine::lifecycle::{
    Event, EventCategory, EventSource, Phase, ProfileRecord, ProjectionStatus,
    StressFailureContext, Subscriber,
};
use fathomdb_engine::{CounterSnapshot, Engine, PreparedWrite};
use tempfile::TempDir;

fn fixture() -> (TempDir, Engine) {
    let dir = TempDir::new().unwrap();
    let engine = Engine::open(dir.path().join("observability.sqlite")).expect("engine open").engine;
    (dir, engine)
}

#[derive(Default)]
struct CapturingSubscriber {
    events: Mutex<Vec<Event>>,
}

impl Subscriber for CapturingSubscriber {
    fn on_event(&self, event: &Event) {
        self.events.lock().unwrap().push(event.clone());
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
    };
    assert_eq!(event.phase, Phase::Started);
    assert_eq!(event.source, EventSource::Engine);
    assert_eq!(event.category, EventCategory::Writer);
}

// AC-002: No log files written without subscriber. Needs FS-snapshot harness
// + actual write/search/close paths emitting real work.
#[test]
fn ac_002_no_log_files_without_subscriber() {
    let dir = TempDir::new().unwrap();
    let snapshot_before: Vec<_> =
        std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();

    let opened = Engine::open(dir.path().join("nolog.sqlite")).expect("open");
    opened
        .engine
        .write(&[PreparedWrite::Node { kind: "doc".to_string(), body: "hello".to_string() }])
        .expect("write");
    let _ = opened.engine.search("hello").expect("search");
    opened.engine.close().expect("close");

    let snapshot_after: Vec<_> =
        std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();

    let new_files: Vec<_> = snapshot_after
        .iter()
        .filter(|f| !snapshot_before.contains(f))
        .map(|f| f.to_string_lossy().to_string())
        .collect();

    for file in &new_files {
        let allowed = file == "nolog.sqlite"
            || file == "nolog.sqlite.lock"
            || file == "nolog.sqlite-wal"
            || file == "nolog.sqlite-journal";
        assert!(allowed, "unexpected file created without subscriber: {file}");
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

// AC-004b: Counter delta exact for write/query keys after N=1000 mixed ops.
#[test]
fn ac_004b_counter_delta_exact_over_mixed_ops() {
    let (_dir, engine) = fixture();
    let s0 = engine.counters();
    for _ in 0..500 {
        engine
            .write(&[PreparedWrite::Node { kind: "doc".to_string(), body: "hello".to_string() }])
            .expect("write");
    }
    for _ in 0..500 {
        let _ = engine.search("hello").expect("search");
    }
    let s1 = engine.counters();
    assert_eq!(s1.writes - s0.writes, 500);
    assert_eq!(s1.write_rows - s0.write_rows, 500);
    assert_eq!(s1.queries - s0.queries, 500);
    assert!(s1.cache_hit >= s0.cache_hit);
    assert!(s1.cache_miss >= s0.cache_miss);
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
#[test]
#[ignore = "AC-005a: needs profile-record emission and retrieval surface (Phase 9)"]
fn ac_005a_profiling_toggleable_at_runtime() {
    let (_dir, engine) = fixture();
    engine.set_profiling(false).expect("disable profiling");
    let _ = engine.search("hello").expect("search");
    engine.set_profiling(true).expect("enable profiling");
    let _ = engine.search("hello").expect("search");
}

// AC-005b: Profile record schema is typed numeric.
#[test]
fn ac_005b_profile_record_typed_numeric_fields() {
    let record = ProfileRecord { wall_clock_ms: 12, step_count: 3, cache_delta: -7 };
    let _: u64 = record.wall_clock_ms;
    let _: u64 = record.step_count;
    let _: i64 = record.cache_delta;
    assert_eq!(record.wall_clock_ms, 12);
    assert_eq!(record.step_count, 3);
    assert_eq!(record.cache_delta, -7);
}

// AC-006: SQLite-internal events surfaced with typed source tag.
#[test]
#[ignore = "AC-006: needs corruption-injection harness + SqliteInternal routing"]
fn ac_006_sqlite_internal_events_typed_source() {
    let (_dir, engine) = fixture();
    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());
    let captured = sink.events.lock().unwrap();
    assert!(captured.iter().any(|e| {
        e.source == EventSource::SqliteInternal
            && matches!(
                e.category,
                EventCategory::Corruption | EventCategory::Recovery | EventCategory::Io
            )
    }));
}

// AC-007a: Slow-statement event at default 100 ms threshold.
#[test]
#[ignore = "AC-007a: needs deterministic-slow fixture + statement-level wall-clock timing"]
fn ac_007a_slow_statement_event_at_default_threshold() {
    let (_dir, engine) = fixture();
    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());
    let captured = sink.events.lock().unwrap();
    let slow_count = captured.iter().filter(|e| e.phase == Phase::Slow).count();
    assert_eq!(slow_count, 1);
}

// AC-007b: Slow threshold reconfigurable at runtime.
#[test]
#[ignore = "AC-007b: needs slow-emit wiring + fast/slow fixtures (Phase 7/9)"]
fn ac_007b_slow_threshold_reconfigurable() {
    let (_dir, engine) = fixture();
    engine.set_slow_threshold_ms(500).expect("set threshold");
    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());
    {
        let captured = sink.events.lock().unwrap();
        assert!(captured.iter().all(|e| e.phase != Phase::Slow));
    }
    let captured = sink.events.lock().unwrap();
    assert_eq!(captured.iter().filter(|e| e.phase == Phase::Slow).count(), 1);
}

// AC-008: Slow signal participates in lifecycle attribution.
#[test]
#[ignore = "AC-008: needs slow-statement signal feeding lifecycle phase (Phase 7)"]
fn ac_008_slow_signal_feeds_lifecycle() {
    let (_dir, engine) = fixture();
    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());
    let captured = sink.events.lock().unwrap();
    assert!(captured.iter().any(|e| e.phase == Phase::Slow));
}

// AC-009 supporting: Pure-type construction of StressFailureContext.
#[test]
fn ac_009_stress_failure_context_constructs() {
    let ctx = StressFailureContext {
        thread_group_id: 0,
        op_kind: "search".to_string(),
        last_error_chain: vec!["a".to_string(), "b".to_string()],
        projection_state: "UpToDate".to_string(),
    };
    let _: u64 = ctx.thread_group_id;
    let _: String = ctx.op_kind.clone();
    let _: Vec<String> = ctx.last_error_chain.clone();
    let _: String = ctx.projection_state.clone();
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
