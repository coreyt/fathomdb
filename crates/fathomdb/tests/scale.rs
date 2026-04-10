#![allow(clippy::expect_used)]

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

use fathomdb::{
    ChunkInsert, ChunkPolicy, Engine, EngineOptions, NodeInsert, TelemetrySnapshot, WriteRequest,
    new_row_id,
};
use tempfile::NamedTempFile;

fn open_engine() -> (NamedTempFile, Engine) {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");
    (db, engine)
}

fn make_write(label: &str) -> WriteRequest {
    let logical_id = format!("doc:{label}");
    WriteRequest {
        label: label.to_owned(),
        nodes: vec![NodeInsert {
            row_id: new_row_id(),
            logical_id: logical_id.clone(),
            kind: "Document".to_owned(),
            properties: format!(r#"{{"title":"{label}"}}"#),
            source_ref: Some(format!("source:{label}")),
            upsert: true,
            chunk_policy: ChunkPolicy::Replace,
        }],
        node_retires: vec![],
        edges: vec![],
        edge_retires: vec![],
        chunks: vec![ChunkInsert {
            id: format!("chunk:{logical_id}:0"),
            node_logical_id: logical_id,
            text_content: format!("stress test content for {label}"),
            byte_start: None,
            byte_end: None,
        }],
        runs: vec![],
        steps: vec![],
        actions: vec![],
        optional_backfills: vec![],
        vec_inserts: vec![],
        operational_writes: vec![],
    }
}

fn seed_documents(engine: &Engine, count: usize) {
    for index in 0..count {
        engine
            .writer()
            .submit(make_write(&format!("seed-{index}")))
            .expect("seed write");
    }
}

fn stress_duration() -> Duration {
    let seconds = std::env::var("FATHOM_RUST_STRESS_DURATION_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(5);
    Duration::from_secs(seconds)
}

fn emit_success_summary(name: &str, metrics: &[(&str, String)]) {
    let rendered = metrics
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(", ");
    eprintln!("{name}: {rendered}");
}

fn spawn_telemetry_sampler(
    engine: Arc<Engine>,
    stop: Arc<AtomicBool>,
    snapshots: Arc<Mutex<Vec<TelemetrySnapshot>>>,
    errors: Arc<Mutex<Vec<String>>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            let snapshot = engine.telemetry_snapshot();
            snapshots.lock().expect("lock snapshots").push(snapshot);
            thread::sleep(Duration::from_millis(25));
        }
        let final_snapshot = engine.telemetry_snapshot();
        if final_snapshot.errors_total > 0 {
            errors.lock().expect("lock errors").push(format!(
                "telemetry errors_total was {}",
                final_snapshot.errors_total
            ));
        }
        snapshots
            .lock()
            .expect("lock snapshots")
            .push(final_snapshot);
    })
}

fn assert_monotonic_snapshots(snapshots: &[TelemetrySnapshot]) {
    for pair in snapshots.windows(2) {
        let first = &pair[0];
        let second = &pair[1];
        assert!(
            second.queries_total >= first.queries_total,
            "queries_total decreased: {:?} -> {:?}",
            first.queries_total,
            second.queries_total
        );
        assert!(
            second.writes_total >= first.writes_total,
            "writes_total decreased: {:?} -> {:?}",
            first.writes_total,
            second.writes_total
        );
        assert!(
            second.write_rows_total >= first.write_rows_total,
            "write_rows_total decreased: {:?} -> {:?}",
            first.write_rows_total,
            second.write_rows_total
        );
        assert!(
            second.errors_total >= first.errors_total,
            "errors_total decreased: {:?} -> {:?}",
            first.errors_total,
            second.errors_total
        );
        assert!(
            second.admin_ops_total >= first.admin_ops_total,
            "admin_ops_total decreased: {:?} -> {:?}",
            first.admin_ops_total,
            second.admin_ops_total
        );
        assert!(
            second.sqlite_cache.cache_hits >= 0,
            "cache_hits must be non-negative"
        );
        assert!(
            second.sqlite_cache.cache_misses >= 0,
            "cache_misses must be non-negative"
        );
        assert!(
            second.sqlite_cache.cache_writes >= 0,
            "cache_writes must be non-negative"
        );
        assert!(
            second.sqlite_cache.cache_spills >= 0,
            "cache_spills must be non-negative"
        );
    }
}

#[test]
#[ignore = "weekly stress test"]
fn sustained_concurrent_reads_under_write_load() {
    let duration = stress_duration();
    let (_db, engine) = open_engine();
    seed_documents(&engine, 100);

    let engine = Arc::new(engine);
    let stop = Arc::new(AtomicBool::new(false));
    let read_count = Arc::new(AtomicUsize::new(0));
    let write_count = Arc::new(AtomicUsize::new(0));
    let errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut handles = Vec::new();

    for thread_id in 0..5 {
        let engine = Arc::clone(&engine);
        let stop = Arc::clone(&stop);
        let write_count = Arc::clone(&write_count);
        let errors = Arc::clone(&errors);
        handles.push(thread::spawn(move || {
            let mut iteration = 0usize;
            while !stop.load(Ordering::Relaxed) {
                if let Err(err) = engine
                    .writer()
                    .submit(make_write(&format!("writer-{thread_id}-{iteration}")))
                {
                    errors
                        .lock()
                        .expect("lock errors")
                        .push(format!("writer[{thread_id}]: {err}"));
                    stop.store(true, Ordering::Relaxed);
                    break;
                }
                write_count.fetch_add(1, Ordering::Relaxed);
                iteration += 1;
            }
        }));
    }

    for thread_id in 0..20 {
        let engine = Arc::clone(&engine);
        let stop = Arc::clone(&stop);
        let read_count = Arc::clone(&read_count);
        let errors = Arc::clone(&errors);
        handles.push(thread::spawn(move || {
            let compiled = engine
                .query("Document")
                .limit(10)
                .compile()
                .expect("query compiles");
            while !stop.load(Ordering::Relaxed) {
                match engine.coordinator().execute_compiled_read(&compiled) {
                    Ok(rows) => {
                        assert!(!rows.was_degraded, "stress read must not degrade");
                        read_count.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(err) => {
                        errors
                            .lock()
                            .expect("lock errors")
                            .push(format!("reader[{thread_id}]: {err}"));
                        stop.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }
        }));
    }

    thread::sleep(duration);
    stop.store(true, Ordering::Relaxed);

    for handle in handles {
        handle.join().expect("thread joins");
    }

    let errors = errors.lock().expect("lock errors");
    assert!(errors.is_empty(), "errors during stress test: {errors:?}");
    assert!(
        write_count.load(Ordering::Relaxed) > 0,
        "no writes completed"
    );
    assert!(read_count.load(Ordering::Relaxed) > 0, "no reads completed");

    let integrity = engine
        .admin()
        .service()
        .check_integrity()
        .expect("check_integrity");
    assert!(integrity.physical_ok, "physical integrity must pass");
    assert!(integrity.foreign_keys_ok, "foreign keys must be valid");
    assert_eq!(integrity.missing_fts_rows, 0, "no missing FTS rows");
    assert_eq!(
        integrity.duplicate_active_logical_ids, 0,
        "no duplicate active logical ids"
    );

    emit_success_summary(
        "rust_stress_reads_under_write_load",
        &[
            ("duration_seconds", duration.as_secs().to_string()),
            ("writes", write_count.load(Ordering::Relaxed).to_string()),
            ("reads", read_count.load(Ordering::Relaxed).to_string()),
        ],
    );
}

#[test]
#[ignore = "weekly stress test"]
fn check_integrity_during_active_writes() {
    let (_db, engine) = open_engine();
    seed_documents(&engine, 100);

    let engine = Arc::new(engine);
    let stop = Arc::new(AtomicBool::new(false));
    let errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let duration = stress_duration();
    let writer_engine = Arc::clone(&engine);
    let writer_stop = Arc::clone(&stop);
    let writer_errors = Arc::clone(&errors);
    let writer_handle = thread::spawn(move || {
        let mut iteration = 0usize;
        while !writer_stop.load(Ordering::Relaxed) {
            if let Err(err) = writer_engine
                .writer()
                .submit(make_write(&format!("integrity-writer-{iteration}")))
            {
                writer_errors
                    .lock()
                    .expect("lock errors")
                    .push(format!("writer: {err}"));
                writer_stop.store(true, Ordering::Relaxed);
                break;
            }
            iteration += 1;
        }
    });

    let deadline = Instant::now() + duration;
    let mut check_count = 0usize;
    while Instant::now() < deadline && !stop.load(Ordering::Relaxed) {
        let integrity = engine
            .admin()
            .service()
            .check_integrity()
            .expect("check_integrity during writes");
        assert!(integrity.physical_ok, "physical integrity must pass");
        assert!(integrity.foreign_keys_ok, "foreign keys must be valid");
        check_count += 1;
        thread::sleep(Duration::from_millis(25));
    }

    stop.store(true, Ordering::Relaxed);
    writer_handle.join().expect("writer joins");

    let errors = errors.lock().expect("lock errors");
    assert!(
        errors.is_empty(),
        "errors during integrity stress test: {errors:?}"
    );
    assert!(
        check_count >= 5,
        "expected repeated integrity checks, saw {check_count}"
    );

    emit_success_summary(
        "rust_stress_integrity_during_writes",
        &[
            ("duration_seconds", duration.as_secs().to_string()),
            ("integrity_checks", check_count.to_string()),
        ],
    );
}

#[test]
#[ignore = "weekly stress test"]
fn telemetry_snapshot_is_monotonic_under_load() {
    let duration = stress_duration();
    let (_db, engine) = open_engine();
    seed_documents(&engine, 100);

    let engine = Arc::new(engine);
    let stop = Arc::new(AtomicBool::new(false));
    let read_count = Arc::new(AtomicUsize::new(0));
    let write_count = Arc::new(AtomicUsize::new(0));
    let errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let snapshots = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();

    for thread_id in 0..5 {
        let engine = Arc::clone(&engine);
        let stop = Arc::clone(&stop);
        let write_count = Arc::clone(&write_count);
        let errors = Arc::clone(&errors);
        handles.push(thread::spawn(move || {
            let mut iteration = 0usize;
            while !stop.load(Ordering::Relaxed) {
                if let Err(err) = engine.writer().submit(make_write(&format!(
                    "telemetry-writer-{thread_id}-{iteration}"
                ))) {
                    errors
                        .lock()
                        .expect("lock errors")
                        .push(format!("writer[{thread_id}]: {err}"));
                    stop.store(true, Ordering::Relaxed);
                    break;
                }
                write_count.fetch_add(1, Ordering::Relaxed);
                iteration += 1;
            }
        }));
    }

    for thread_id in 0..20 {
        let engine = Arc::clone(&engine);
        let stop = Arc::clone(&stop);
        let read_count = Arc::clone(&read_count);
        let errors = Arc::clone(&errors);
        handles.push(thread::spawn(move || {
            let compiled = engine
                .query("Document")
                .limit(10)
                .compile()
                .expect("query compiles");
            while !stop.load(Ordering::Relaxed) {
                match engine.coordinator().execute_compiled_read(&compiled) {
                    Ok(rows) => {
                        assert!(!rows.was_degraded, "telemetry read must not degrade");
                        read_count.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(err) => {
                        errors
                            .lock()
                            .expect("lock errors")
                            .push(format!("reader[{thread_id}]: {err}"));
                        stop.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }
        }));
    }

    handles.push(spawn_telemetry_sampler(
        Arc::clone(&engine),
        Arc::clone(&stop),
        Arc::clone(&snapshots),
        Arc::clone(&errors),
    ));

    thread::sleep(duration);
    stop.store(true, Ordering::Relaxed);

    for handle in handles {
        handle.join().expect("thread joins");
    }

    let errors = errors.lock().expect("lock errors");
    assert!(
        errors.is_empty(),
        "errors during telemetry stress test: {errors:?}"
    );
    assert!(
        write_count.load(Ordering::Relaxed) > 0,
        "no writes completed"
    );
    assert!(read_count.load(Ordering::Relaxed) > 0, "no reads completed");

    let snapshots = snapshots.lock().expect("lock snapshots");
    assert!(snapshots.len() >= 2, "expected multiple telemetry samples");
    assert_monotonic_snapshots(&snapshots);
    let last = snapshots.last().expect("last snapshot");
    assert!(last.queries_total > 0, "telemetry must observe reads");
    assert!(last.writes_total > 0, "telemetry must observe writes");
    assert!(
        last.write_rows_total >= last.writes_total,
        "write rows must be at least write count"
    );
    assert_eq!(
        last.errors_total, 0,
        "telemetry errors_total must stay zero"
    );
    let cache_total = last.sqlite_cache.cache_hits + last.sqlite_cache.cache_misses;
    assert!(cache_total > 0, "telemetry must observe cache activity");

    let integrity = engine
        .admin()
        .service()
        .check_integrity()
        .expect("check_integrity");
    assert!(integrity.physical_ok, "physical integrity must pass");
    assert!(integrity.foreign_keys_ok, "foreign keys must be valid");

    emit_success_summary(
        "rust_stress_telemetry",
        &[
            ("duration_seconds", duration.as_secs().to_string()),
            ("writes", write_count.load(Ordering::Relaxed).to_string()),
            ("reads", read_count.load(Ordering::Relaxed).to_string()),
            ("telemetry_samples", snapshots.len().to_string()),
            ("queries_total", last.queries_total.to_string()),
            ("writes_total", last.writes_total.to_string()),
            ("write_rows_total", last.write_rows_total.to_string()),
            ("errors_total", last.errors_total.to_string()),
            ("admin_ops_total", last.admin_ops_total.to_string()),
            ("cache_hits", last.sqlite_cache.cache_hits.to_string()),
            ("cache_misses", last.sqlite_cache.cache_misses.to_string()),
            ("cache_writes", last.sqlite_cache.cache_writes.to_string()),
            ("cache_spills", last.sqlite_cache.cache_spills.to_string()),
        ],
    );
}
