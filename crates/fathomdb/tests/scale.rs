#![allow(clippy::expect_used)]

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

use fathomdb::{
    ChunkInsert, ChunkPolicy, Engine, EngineOptions, FtsPropertyPathSpec, NodeInsert, NodeRetire,
    SearchHit, SearchRows, TelemetrySnapshot, WriteRequest, new_row_id,
};
use std::sync::Barrier;
use tempfile::NamedTempFile;

fn open_engine() -> (NamedTempFile, Engine) {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");
    (db, engine)
}

fn make_write(label: &str) -> WriteRequest {
    make_write_with_content(label, None, None)
}

fn make_write_with_content(
    label: &str,
    content_ref: Option<String>,
    content_hash: Option<String>,
) -> WriteRequest {
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
            content_ref,
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
            content_hash,
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

#[allow(clippy::print_stderr)]
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
#[allow(clippy::too_many_lines)]
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

/// Stress test for external content objects: mixed writes (some with `content_ref`
/// and `content_hash`, some without) alongside concurrent reads that filter on
/// `content_ref`. Exercises the partial index, nullable column handling, and new
/// query predicates under sustained concurrent load.
#[test]
#[ignore = "weekly stress test"]
#[allow(clippy::too_many_lines)]
fn concurrent_external_content_writes_and_filtered_reads() {
    let duration = stress_duration();
    let (_db, engine) = open_engine();

    // Seed a mix of content and non-content nodes.
    for index in 0..50 {
        let content_ref = if index % 2 == 0 {
            Some(format!("s3://docs/seed-{index}.pdf"))
        } else {
            None
        };
        let content_hash = content_ref.as_ref().map(|_| format!("sha256:seed{index}"));
        engine
            .writer()
            .submit(make_write_with_content(
                &format!("seed-{index}"),
                content_ref,
                content_hash,
            ))
            .expect("seed write");
    }

    let engine = Arc::new(engine);
    let stop = Arc::new(AtomicBool::new(false));
    let content_write_count = Arc::new(AtomicUsize::new(0));
    let plain_write_count = Arc::new(AtomicUsize::new(0));
    let filtered_read_count = Arc::new(AtomicUsize::new(0));
    let unfiltered_read_count = Arc::new(AtomicUsize::new(0));
    let errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut handles = Vec::new();

    // 3 writer threads producing content nodes (with content_ref + content_hash).
    for thread_id in 0..3 {
        let engine = Arc::clone(&engine);
        let stop = Arc::clone(&stop);
        let count = Arc::clone(&content_write_count);
        let errors = Arc::clone(&errors);
        handles.push(thread::spawn(move || {
            let mut iteration = 0usize;
            while !stop.load(Ordering::Relaxed) {
                let label = format!("ext-{thread_id}-{iteration}");
                let request = make_write_with_content(
                    &label,
                    Some(format!("s3://docs/{label}.pdf")),
                    Some(format!("sha256:{label}")),
                );
                if let Err(err) = engine.writer().submit(request) {
                    errors
                        .lock()
                        .expect("lock errors")
                        .push(format!("content-writer[{thread_id}]: {err}"));
                    stop.store(true, Ordering::Relaxed);
                    break;
                }
                count.fetch_add(1, Ordering::Relaxed);
                iteration += 1;
            }
        }));
    }

    // 2 writer threads producing plain nodes (no content_ref).
    for thread_id in 0..2 {
        let engine = Arc::clone(&engine);
        let stop = Arc::clone(&stop);
        let count = Arc::clone(&plain_write_count);
        let errors = Arc::clone(&errors);
        handles.push(thread::spawn(move || {
            let mut iteration = 0usize;
            while !stop.load(Ordering::Relaxed) {
                if let Err(err) = engine
                    .writer()
                    .submit(make_write(&format!("plain-{thread_id}-{iteration}")))
                {
                    errors
                        .lock()
                        .expect("lock errors")
                        .push(format!("plain-writer[{thread_id}]: {err}"));
                    stop.store(true, Ordering::Relaxed);
                    break;
                }
                count.fetch_add(1, Ordering::Relaxed);
                iteration += 1;
            }
        }));
    }

    // 10 reader threads using content_ref_not_null filter.
    for thread_id in 0..10 {
        let engine = Arc::clone(&engine);
        let stop = Arc::clone(&stop);
        let count = Arc::clone(&filtered_read_count);
        let errors = Arc::clone(&errors);
        handles.push(thread::spawn(move || {
            let compiled = engine
                .query("Document")
                .filter_content_ref_not_null()
                .limit(10)
                .compile()
                .expect("filtered query compiles");
            while !stop.load(Ordering::Relaxed) {
                match engine.coordinator().execute_compiled_read(&compiled) {
                    Ok(rows) => {
                        // Every returned node must have content_ref set.
                        for node in &rows.nodes {
                            assert!(
                                node.content_ref.is_some(),
                                "filtered read returned node without content_ref: {}",
                                node.logical_id
                            );
                        }
                        count.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(err) => {
                        errors
                            .lock()
                            .expect("lock errors")
                            .push(format!("filtered-reader[{thread_id}]: {err}"));
                        stop.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }
        }));
    }

    // 10 reader threads doing unfiltered reads.
    for thread_id in 0..10 {
        let engine = Arc::clone(&engine);
        let stop = Arc::clone(&stop);
        let count = Arc::clone(&unfiltered_read_count);
        let errors = Arc::clone(&errors);
        handles.push(thread::spawn(move || {
            let compiled = engine
                .query("Document")
                .limit(10)
                .compile()
                .expect("unfiltered query compiles");
            while !stop.load(Ordering::Relaxed) {
                match engine.coordinator().execute_compiled_read(&compiled) {
                    Ok(_) => {
                        count.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(err) => {
                        errors
                            .lock()
                            .expect("lock errors")
                            .push(format!("unfiltered-reader[{thread_id}]: {err}"));
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
        content_write_count.load(Ordering::Relaxed) > 0,
        "no content writes completed"
    );
    assert!(
        plain_write_count.load(Ordering::Relaxed) > 0,
        "no plain writes completed"
    );
    assert!(
        filtered_read_count.load(Ordering::Relaxed) > 0,
        "no filtered reads completed"
    );
    assert!(
        unfiltered_read_count.load(Ordering::Relaxed) > 0,
        "no unfiltered reads completed"
    );

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
        "rust_stress_external_content",
        &[
            ("duration_seconds", duration.as_secs().to_string()),
            (
                "content_writes",
                content_write_count.load(Ordering::Relaxed).to_string(),
            ),
            (
                "plain_writes",
                plain_write_count.load(Ordering::Relaxed).to_string(),
            ),
            (
                "filtered_reads",
                filtered_read_count.load(Ordering::Relaxed).to_string(),
            ),
            (
                "unfiltered_reads",
                unfiltered_read_count.load(Ordering::Relaxed).to_string(),
            ),
        ],
    );
}

/// Helper: create a structured-only Goal write request (no chunks).
fn make_goal_write(label: &str, upsert: bool) -> WriteRequest {
    WriteRequest {
        label: label.to_owned(),
        nodes: vec![NodeInsert {
            row_id: new_row_id(),
            logical_id: format!("goal:{label}"),
            kind: "Goal".to_owned(),
            properties: format!(
                r#"{{"name":"Goal {label}","description":"Structured projection stress test for {label}"}}"#
            ),
            source_ref: Some(format!("source:{label}")),
            upsert,
            chunk_policy: ChunkPolicy::Preserve,
            content_ref: None,
        }],
        node_retires: vec![],
        edges: vec![],
        edge_retires: vec![],
        chunks: vec![],
        runs: vec![],
        steps: vec![],
        actions: vec![],
        optional_backfills: vec![],
        vec_inserts: vec![],
        operational_writes: vec![],
    }
}

/// Helper: create a retire request for a Goal.
fn make_goal_retire(label: &str) -> WriteRequest {
    WriteRequest {
        label: format!("retire-{label}"),
        nodes: vec![],
        node_retires: vec![NodeRetire {
            logical_id: format!("goal:{label}"),
            source_ref: Some(format!("retire-source:{label}")),
        }],
        edges: vec![],
        edge_retires: vec![],
        chunks: vec![],
        runs: vec![],
        steps: vec![],
        actions: vec![],
        optional_backfills: vec![],
        vec_inserts: vec![],
        operational_writes: vec![],
    }
}

/// Stress test for structured node full-text projections: concurrent writes
/// (insert, upsert, retire) of projection-enabled kinds alongside concurrent
/// `text_search(...)` reads through the UNION query path. Also mixes in
/// chunk-backed Document writes to exercise the mixed workload.
///
/// Verifies at the end that:
/// - property FTS rows were actually created (new code exercised)
/// - `text_search(...)` returns property-backed hits
/// - integrity reports zero missing property FTS rows
/// - semantics reports zero drift, duplicates, and orphans
#[test]
#[ignore = "weekly stress test"]
#[allow(clippy::too_many_lines)]
fn property_fts_projections_under_concurrent_load() {
    let duration = stress_duration();
    let (_db, engine) = open_engine();

    // Register property FTS schema BEFORE any writes.
    engine
        .register_fts_property_schema(
            "Goal",
            &["$.name".to_owned(), "$.description".to_owned()],
            None,
        )
        .expect("register property schema");

    // Seed structured-only Goal nodes (no chunks).
    for index in 0..50 {
        engine
            .writer()
            .submit(make_goal_write(&format!("seed-{index}"), false))
            .expect("seed goal write");
    }
    // Seed chunk-backed Document nodes for mixed workload.
    seed_documents(&engine, 50);

    // Verify setup: property FTS rows must already exist from seeding.
    {
        let integrity = engine
            .admin()
            .service()
            .check_integrity()
            .expect("check_integrity after seed");
        assert_eq!(
            integrity.missing_property_fts_rows, 0,
            "seed must create property FTS rows"
        );
    }

    let engine = Arc::new(engine);
    let stop = Arc::new(AtomicBool::new(false));
    let goal_insert_count = Arc::new(AtomicUsize::new(0));
    let goal_upsert_count = Arc::new(AtomicUsize::new(0));
    let goal_retire_count = Arc::new(AtomicUsize::new(0));
    let doc_write_count = Arc::new(AtomicUsize::new(0));
    let goal_search_count = Arc::new(AtomicUsize::new(0));
    let doc_search_count = Arc::new(AtomicUsize::new(0));
    let errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut handles = Vec::new();

    // 2 threads inserting new Goal nodes.
    for thread_id in 0..2 {
        let engine = Arc::clone(&engine);
        let stop = Arc::clone(&stop);
        let count = Arc::clone(&goal_insert_count);
        let errors = Arc::clone(&errors);
        handles.push(thread::spawn(move || {
            let mut iteration = 0usize;
            while !stop.load(Ordering::Relaxed) {
                let label = format!("insert-{thread_id}-{iteration}");
                if let Err(err) = engine.writer().submit(make_goal_write(&label, false)) {
                    errors
                        .lock()
                        .expect("lock errors")
                        .push(format!("goal-inserter[{thread_id}]: {err}"));
                    stop.store(true, Ordering::Relaxed);
                    break;
                }
                count.fetch_add(1, Ordering::Relaxed);
                iteration += 1;
            }
        }));
    }

    // 2 threads upserting existing seed Goal nodes (repeated upserts of same IDs).
    for thread_id in 0..2 {
        let engine = Arc::clone(&engine);
        let stop = Arc::clone(&stop);
        let count = Arc::clone(&goal_upsert_count);
        let errors = Arc::clone(&errors);
        handles.push(thread::spawn(move || {
            let mut iteration = 0usize;
            while !stop.load(Ordering::Relaxed) {
                let seed_index = iteration % 50;
                let label = format!("seed-{seed_index}");
                if let Err(err) = engine.writer().submit(make_goal_write(&label, true)) {
                    errors
                        .lock()
                        .expect("lock errors")
                        .push(format!("goal-upsert[{thread_id}]: {err}"));
                    stop.store(true, Ordering::Relaxed);
                    break;
                }
                count.fetch_add(1, Ordering::Relaxed);
                iteration += 1;
            }
        }));
    }

    // 1 thread retiring Goal nodes (cycles through newly inserted ones).
    {
        let engine = Arc::clone(&engine);
        let stop = Arc::clone(&stop);
        let count = Arc::clone(&goal_retire_count);
        let errors = Arc::clone(&errors);
        handles.push(thread::spawn(move || {
            let mut iteration = 0usize;
            while !stop.load(Ordering::Relaxed) {
                // Retire insert-0-N nodes; some may not exist yet, which is fine
                // (retire of non-existent node is a no-op).
                let label = format!("insert-0-{iteration}");
                if let Err(err) = engine.writer().submit(make_goal_retire(&label)) {
                    errors
                        .lock()
                        .expect("lock errors")
                        .push(format!("goal-retire: {err}"));
                    stop.store(true, Ordering::Relaxed);
                    break;
                }
                count.fetch_add(1, Ordering::Relaxed);
                iteration += 1;
                // Slow down retires to keep net node count positive.
                thread::sleep(Duration::from_millis(5));
            }
        }));
    }

    // 2 threads writing chunk-backed Documents (mixed workload).
    for thread_id in 0..2 {
        let engine = Arc::clone(&engine);
        let stop = Arc::clone(&stop);
        let count = Arc::clone(&doc_write_count);
        let errors = Arc::clone(&errors);
        handles.push(thread::spawn(move || {
            let mut iteration = 0usize;
            while !stop.load(Ordering::Relaxed) {
                if let Err(err) = engine
                    .writer()
                    .submit(make_write(&format!("doc-{thread_id}-{iteration}")))
                {
                    errors
                        .lock()
                        .expect("lock errors")
                        .push(format!("doc-writer[{thread_id}]: {err}"));
                    stop.store(true, Ordering::Relaxed);
                    break;
                }
                count.fetch_add(1, Ordering::Relaxed);
                iteration += 1;
            }
        }));
    }

    // 10 threads searching Goals via text_search (property FTS UNION path).
    for thread_id in 0..10 {
        let engine = Arc::clone(&engine);
        let stop = Arc::clone(&stop);
        let count = Arc::clone(&goal_search_count);
        let errors = Arc::clone(&errors);
        handles.push(thread::spawn(move || {
            let compiled = engine
                .query("Goal")
                .text_search("stress", 10)
                .limit(10)
                .compile()
                .expect("goal text_search compiles");
            while !stop.load(Ordering::Relaxed) {
                match engine.coordinator().execute_compiled_read(&compiled) {
                    Ok(_rows) => {
                        count.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(err) => {
                        errors
                            .lock()
                            .expect("lock errors")
                            .push(format!("goal-reader[{thread_id}]: {err}"));
                        stop.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }
        }));
    }

    // 5 threads searching Documents via text_search (chunk FTS path).
    for thread_id in 0..5 {
        let engine = Arc::clone(&engine);
        let stop = Arc::clone(&stop);
        let count = Arc::clone(&doc_search_count);
        let errors = Arc::clone(&errors);
        handles.push(thread::spawn(move || {
            let compiled = engine
                .query("Document")
                .text_search("stress", 10)
                .limit(10)
                .compile()
                .expect("doc text_search compiles");
            while !stop.load(Ordering::Relaxed) {
                match engine.coordinator().execute_compiled_read(&compiled) {
                    Ok(_rows) => {
                        count.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(err) => {
                        errors
                            .lock()
                            .expect("lock errors")
                            .push(format!("doc-reader[{thread_id}]: {err}"));
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
    assert!(
        errors.is_empty(),
        "errors during property FTS stress test: {errors:?}"
    );

    // Verify throughput: all thread groups must have completed work.
    let goal_inserts = goal_insert_count.load(Ordering::Relaxed);
    let goal_upserts = goal_upsert_count.load(Ordering::Relaxed);
    let goal_retires = goal_retire_count.load(Ordering::Relaxed);
    let doc_writes = doc_write_count.load(Ordering::Relaxed);
    let goal_searches = goal_search_count.load(Ordering::Relaxed);
    let doc_searches = doc_search_count.load(Ordering::Relaxed);
    assert!(goal_inserts > 0, "no goal inserts completed");
    assert!(goal_upserts > 0, "no goal upserts completed");
    assert!(goal_retires > 0, "no goal retires completed");
    assert!(doc_writes > 0, "no doc writes completed");
    assert!(goal_searches > 0, "no goal text_search reads completed");
    assert!(doc_searches > 0, "no doc text_search reads completed");

    // --- Verify new property FTS code was actually exercised ---

    // 1. Property FTS rows must exist in the database.
    let admin = engine.admin().service();
    let integrity = admin.check_integrity().expect("check_integrity");
    assert!(integrity.physical_ok, "physical integrity must pass");
    assert!(integrity.foreign_keys_ok, "foreign keys must be valid");
    assert_eq!(integrity.missing_fts_rows, 0, "no missing chunk FTS rows");
    assert_eq!(
        integrity.missing_property_fts_rows, 0,
        "no missing property FTS rows after stress"
    );
    assert_eq!(
        integrity.duplicate_active_logical_ids, 0,
        "no duplicate active logical ids"
    );

    // 2. Semantic checks: all new drift counters must be zero.
    let semantics = admin.check_semantics().expect("check_semantics");
    assert_eq!(
        semantics.drifted_property_fts_rows, 0,
        "no drifted property FTS text after stress"
    );
    assert_eq!(
        semantics.duplicate_property_fts_rows, 0,
        "no duplicate property FTS rows after stress"
    );
    assert_eq!(
        semantics.mismatched_kind_property_fts_rows, 0,
        "no kind-mismatched property FTS rows"
    );
    assert_eq!(
        semantics.stale_property_fts_rows, 0,
        "no stale property FTS rows"
    );

    // 3. text_search(...) must actually return property-backed Goal results
    //    (not just zero rows). The seed and insert threads guarantee active Goals
    //    with "stress" in their description.
    let compiled = engine
        .query("Goal")
        .text_search("stress", 100)
        .limit(100)
        .compile()
        .expect("final goal search compiles");
    let final_rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("final goal search executes");
    assert!(
        !final_rows.nodes.is_empty(),
        "text_search must return property-backed Goal hits after stress"
    );

    emit_success_summary(
        "rust_stress_property_fts_projections",
        &[
            ("duration_seconds", duration.as_secs().to_string()),
            ("goal_inserts", goal_inserts.to_string()),
            ("goal_upserts", goal_upserts.to_string()),
            ("goal_retires", goal_retires.to_string()),
            ("doc_writes", doc_writes.to_string()),
            ("goal_searches", goal_searches.to_string()),
            ("doc_searches", doc_searches.to_string()),
            ("final_goal_hits", final_rows.nodes.len().to_string()),
        ],
    );
}

// --- Pack P8c: adaptive text search concurrency & determinism ------------
//
// The tests below pin the "Concurrency contract" section of the adaptive
// text search design:
//   1. Reads never block on background writes (p99 stays flat under load).
//   2. Repeated identical searches are byte-deterministic on a frozen DB.
//   3. Concurrent identical fallback_search reads are byte-deterministic.
//   4. A scalar -> recursive property FTS rebuild preserves search results
//      and populates match attribution correctly after heavy writes.

/// Helper: register a property FTS schema on `Note` with a recursive
/// `$.payload` path. Used by the adaptive-search stress tests below to
/// exercise the property FTS UNION arm via `text_search` / `fallback_search`.
fn register_note_recursive_schema(engine: &Engine) {
    engine
        .register_fts_property_schema_with_entries(
            "Note",
            &[FtsPropertyPathSpec::recursive("$.payload".to_owned())],
            None,
            &[],
        )
        .expect("register Note recursive schema");
}

/// Helper: submit a single `Note` with a recursive-payload body. The
/// payload varies enough per-seed that relaxed fallback queries find
/// strict-miss / relaxed-hit rows, but the text always contains the
/// shared token `"budget"` so `text_search("budget", ...)` returns hits.
fn submit_note(engine: &Engine, label: &str) {
    let logical_id = format!("note:{label}");
    let props = format!(
        r#"{{"title":"budget Note {label}","payload":{{"body":"budget quarterly plan for {label}","tags":["stress","adaptive","{label}"]}}}}"#
    );
    engine
        .writer()
        .submit(WriteRequest {
            label: label.to_owned(),
            nodes: vec![NodeInsert {
                row_id: new_row_id(),
                logical_id,
                kind: "Note".to_owned(),
                properties: props,
                source_ref: Some(format!("source:{label}")),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("submit note");
}

/// Stable, byte-exact rendering of a `SearchRows` result used for
/// determinism comparisons. Floats go through `{:?}` so two runs that
/// produced the same IEEE-754 bits render identically without relying on
/// `PartialEq` for `f64`.
fn format_search_rows_stable(rows: &SearchRows) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    writeln!(
        &mut out,
        "strict={} relaxed={} vector={} fallback_used={} was_degraded={} hits={}",
        rows.strict_hit_count,
        rows.relaxed_hit_count,
        rows.vector_hit_count,
        rows.fallback_used,
        rows.was_degraded,
        rows.hits.len()
    )
    .expect("format into String never fails");
    for (idx, hit) in rows.hits.iter().enumerate() {
        writeln!(
            &mut out,
            "[{idx}] logical_id={} row_id={} kind={} properties={:?} content_ref={:?} last_accessed_at={:?} score={:?} modality={:?} source={:?} match_mode={:?} snippet={:?} written_at={} projection_row_id={:?} vector_distance={:?} attribution={:?}",
            hit.node.logical_id,
            hit.node.row_id,
            hit.node.kind,
            hit.node.properties,
            hit.node.content_ref,
            hit.node.last_accessed_at,
            hit.score,
            hit.modality,
            hit.source,
            hit.match_mode,
            hit.snippet,
            hit.written_at,
            hit.projection_row_id,
            hit.vector_distance,
            hit.attribution,
        )
        .expect("format into String never fails");
    }
    out
}

/// Compute the p99 latency (in microseconds) of a slice of durations.
/// `Vec` is cloned+sorted internally so the caller's vector is untouched.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn p99_micros(samples: &[Duration]) -> f64 {
    assert!(!samples.is_empty(), "p99 requires at least one sample");
    let mut sorted: Vec<Duration> = samples.to_vec();
    sorted.sort();
    // 1-based 99th percentile rank (ceil(n * 0.99)) → 0-based slice index
    // (subtract 1). For n=1200 this yields index 1187, the 1188th smallest
    // sample. The `.min(len - 1)` clamp guards against n < 100 edge cases
    // where ceil(n*0.99) could equal n.
    let idx = ((sorted.len() as f64) * 0.99).ceil() as usize;
    let idx = idx.saturating_sub(1).min(sorted.len() - 1);
    sorted[idx].as_micros() as f64
}

/// Seed N Notes for the adaptive-search tests. Each note has a distinct
/// `budget` mention so `text_search("budget", limit)` returns hits.
fn seed_notes(engine: &Engine, count: usize) {
    for i in 0..count {
        submit_note(engine, &format!("seed-{i:04}"));
    }
}

#[test]
fn adaptive_search_deterministic_hit_ordering_under_repeated_runs() {
    // P8c-2: on a frozen DB, the same `text_search(q, limit)` must
    // produce byte-identical SearchRows every time — same hit order,
    // same scores, same sources, same match_modes, same snippets,
    // same projection_row_ids.
    let (_db, engine) = open_engine();
    register_note_recursive_schema(&engine);
    seed_notes(&engine, 120);

    let baseline = engine
        .query("Note")
        .text_search("budget", 25)
        .with_match_attribution()
        .execute()
        .expect("baseline search");
    assert!(
        !baseline.hits.is_empty(),
        "seed must produce at least one budget hit"
    );
    let baseline_formatted = format_search_rows_stable(&baseline);

    for run in 0..50 {
        let rows = engine
            .query("Note")
            .text_search("budget", 25)
            .with_match_attribution()
            .execute()
            .expect("repeated search");
        let formatted = format_search_rows_stable(&rows);
        assert_eq!(
            formatted, baseline_formatted,
            "run {run} diverged from baseline determinism snapshot"
        );
    }

    emit_success_summary(
        "rust_stress_adaptive_search_determinism",
        &[
            ("runs", 50.to_string()),
            ("hits", baseline.hits.len().to_string()),
        ],
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn fallback_search_stable_under_repeated_concurrent_reads() {
    // P8c-3: seed a DB that forces the relaxed branch to actually fire
    // (strict query returns 0 hits on its own), then spawn M reader
    // threads running the SAME fallback_search and assert every
    // reader produces the same byte-exact SearchRows.
    const READERS: usize = 16;
    const ITERATIONS: usize = 64;
    let (_db, engine) = open_engine();
    register_note_recursive_schema(&engine);
    seed_notes(&engine, 100);

    // Baseline single-threaded call: confirm the relaxed branch fires
    // (strict "zzznonexistent" misses, relaxed "budget" hits).
    let engine = Arc::new(engine);
    let baseline_rows = engine
        .fallback_search("zzznonexistent".to_owned(), Some("budget".to_owned()), 20)
        .with_match_attribution()
        .execute()
        .expect("baseline fallback search");
    assert!(
        baseline_rows.fallback_used,
        "seed must exercise the relaxed branch"
    );
    assert!(
        !baseline_rows.hits.is_empty(),
        "relaxed branch must return at least one hit"
    );
    let baseline_formatted = format_search_rows_stable(&baseline_rows);

    let barrier = Arc::new(Barrier::new(READERS));
    let divergence = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut handles = Vec::with_capacity(READERS);

    for reader_id in 0..READERS {
        let engine = Arc::clone(&engine);
        let barrier = Arc::clone(&barrier);
        let divergence = Arc::clone(&divergence);
        let expected = baseline_formatted.clone();
        handles.push(thread::spawn(move || {
            barrier.wait();
            for iter in 0..ITERATIONS {
                let rows = engine
                    .fallback_search(
                        "zzznonexistent".to_owned(),
                        Some("budget".to_owned()),
                        20,
                    )
                    .with_match_attribution()
                    .execute()
                    .expect("concurrent fallback search");
                let formatted = format_search_rows_stable(&rows);
                if formatted != expected {
                    divergence.lock().expect("lock").push(format!(
                        "reader {reader_id} iter {iter} diverged:\nexpected:\n{expected}\nactual:\n{formatted}"
                    ));
                    return;
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("reader joins");
    }

    let divergence = divergence.lock().expect("lock");
    assert!(
        divergence.is_empty(),
        "fallback_search divergence under concurrent reads:\n{}",
        divergence.join("\n---\n")
    );

    emit_success_summary(
        "rust_stress_fallback_search_concurrent_stable",
        &[
            ("readers", READERS.to_string()),
            ("iterations_per_reader", ITERATIONS.to_string()),
            ("hits", baseline_rows.hits.len().to_string()),
        ],
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn property_fts_rebuild_then_search_remains_correct_after_heavy_writes() {
    // P8c-4: start with a scalar-only property FTS schema; seed notes
    // and record baseline search. Then register a NEW recursive schema
    // (triggering an eager transactional rebuild) while heavy writes
    // continue. After rebuild, the same search must return at least
    // the baseline set, and `with_match_attribution()` must populate
    // `matched_paths` with real leaf paths under the recursive schema.
    let (_db, engine) = open_engine();

    // Scalar-only schema on $.title: recursive body content is NOT indexed yet.
    engine
        .register_fts_property_schema_with_entries(
            "Note",
            &[FtsPropertyPathSpec::scalar("$.title".to_owned())],
            None,
            &[],
        )
        .expect("register scalar schema");

    // Seed: notes whose title contains "budget" (so the scalar schema
    // finds them) AND whose body.payload contains "quarterly" (which
    // only the recursive schema can index).
    for i in 0..120 {
        submit_note(&engine, &format!("seed-{i:04}"));
    }

    // Search uses the `seed` token which only appears in seeded node labels
    // (heavy writers use `heavy-*` labels), so the scalar baseline set is
    // stable across the subsequent rebuild + concurrent-write workload.
    let scalar_baseline = engine
        .query("Note")
        .text_search("seed", 200)
        .execute()
        .expect("scalar baseline search for seed");
    assert!(
        !scalar_baseline.hits.is_empty(),
        "scalar schema must already return seed hits via title"
    );
    let scalar_logical_ids: std::collections::BTreeSet<String> = scalar_baseline
        .hits
        .iter()
        .map(|h| h.node.logical_id.clone())
        .collect();

    // Under the scalar schema, "quarterly" (which only appears inside
    // $.payload.body) MUST NOT match — that would mean the scalar
    // schema silently indexed recursive content.
    let quarterly_before = engine
        .query("Note")
        .text_search("quarterly", 200)
        .execute()
        .expect("scalar quarterly search");
    assert!(
        quarterly_before.hits.is_empty(),
        "scalar schema must not index $.payload.body, got {} hits",
        quarterly_before.hits.len()
    );

    // Heavy concurrent writes while we trigger the recursive rebuild.
    let engine = Arc::new(engine);
    let stop = Arc::new(AtomicBool::new(false));
    let write_count = Arc::new(AtomicUsize::new(0));
    let first_write_started = Arc::new(AtomicBool::new(false));
    let errors = Arc::new(Mutex::new(Vec::<String>::new()));

    let mut handles = Vec::new();
    for thread_id in 0..3 {
        let engine = Arc::clone(&engine);
        let stop = Arc::clone(&stop);
        let write_count = Arc::clone(&write_count);
        let first_write_started = Arc::clone(&first_write_started);
        let errors = Arc::clone(&errors);
        handles.push(thread::spawn(move || {
            let mut iteration = 0usize;
            while !stop.load(Ordering::Relaxed) {
                let label = format!("heavy-{thread_id}-{iteration:05}");
                let logical_id = format!("note:{label}");
                let props = format!(
                    r#"{{"title":"budget Note {label}","payload":{{"body":"budget quarterly plan for {label}","tags":["stress"]}}}}"#
                );
                let request = WriteRequest {
                    label: label.clone(),
                    nodes: vec![NodeInsert {
                        row_id: new_row_id(),
                        logical_id,
                        kind: "Note".to_owned(),
                        properties: props,
                        source_ref: Some(format!("source:{label}")),
                        upsert: false,
                        chunk_policy: ChunkPolicy::Preserve,
                        content_ref: None,
                    }],
                    node_retires: vec![],
                    edges: vec![],
                    edge_retires: vec![],
                    chunks: vec![],
                    runs: vec![],
                    steps: vec![],
                    actions: vec![],
                    optional_backfills: vec![],
                    vec_inserts: vec![],
                    operational_writes: vec![],
                };
                // Signal write-in-flight BEFORE submit so the main thread's
                // rebuild race-start barrier observes at least one live
                // writer. Release-ordered so the main thread's Acquire load
                // synchronises with the pre-submit state.
                first_write_started.store(true, Ordering::Release);
                if let Err(err) = engine.writer().submit(request) {
                    errors
                        .lock()
                        .expect("lock errors")
                        .push(format!("heavy-writer[{thread_id}]: {err}"));
                    stop.store(true, Ordering::Relaxed);
                    break;
                }
                write_count.fetch_add(1, Ordering::Relaxed);
                iteration += 1;
                if iteration >= 200 {
                    break;
                }
            }
        }));
    }

    // Block until at least one heavy writer has entered its first submit.
    // Without this barrier the rebuild could land before any writer runs
    // on a fast machine, silently vacating the "rebuild under concurrent
    // writes" invariant this test exists to prove.
    while !first_write_started.load(Ordering::Acquire) {
        std::hint::spin_loop();
    }

    // Writers are now racing; re-register with the recursive shape. The
    // rebuild is transactional; writers continue after it. We bound total
    // work by capping writer iterations above, not by sleep.
    engine
        .register_fts_property_schema_with_entries(
            "Note",
            &[
                FtsPropertyPathSpec::scalar("$.title".to_owned()),
                FtsPropertyPathSpec::recursive("$.payload".to_owned()),
            ],
            None,
            &[],
        )
        .expect("register recursive schema");

    stop.store(true, Ordering::Relaxed);
    for handle in handles {
        handle.join().expect("heavy writer joins");
    }

    let errors_snapshot = errors.lock().expect("lock errors");
    assert!(
        errors_snapshot.is_empty(),
        "writer errors during rebuild: {errors_snapshot:?}"
    );
    drop(errors_snapshot);
    assert!(
        write_count.load(Ordering::Relaxed) > 0,
        "no heavy writes completed before rebuild"
    );

    // Search for `seed`: every logical_id in the scalar baseline must
    // still be present (the rebuild did not drop rows). The `seed` token
    // is only present on seeded nodes, so concurrent heavy writers do
    // not crowd the limit.
    let rebuilt = engine
        .query("Note")
        .text_search("seed", 500)
        .execute()
        .expect("rebuilt seed search");
    let rebuilt_ids: std::collections::BTreeSet<String> = rebuilt
        .hits
        .iter()
        .map(|h| h.node.logical_id.clone())
        .collect();
    for expected_id in &scalar_logical_ids {
        assert!(
            rebuilt_ids.contains(expected_id),
            "rebuilt search lost scalar-baseline hit {expected_id}"
        );
    }

    // Under the recursive schema, "quarterly" (inside $.payload.body)
    // must now return hits — this proves the rebuild actually landed.
    let quarterly_after = engine
        .query("Note")
        .text_search("quarterly", 50)
        .with_match_attribution()
        .execute()
        .expect("rebuilt quarterly search");
    assert!(
        !quarterly_after.hits.is_empty(),
        "recursive rebuild must surface quarterly hits from $.payload.body"
    );
    let first = &quarterly_after.hits[0];
    let attribution = first
        .attribution
        .as_ref()
        .expect("attribution populated when requested");
    assert!(
        !attribution.matched_paths.is_empty(),
        "matched_paths must be populated on recursive hit"
    );
    assert!(
        attribution
            .matched_paths
            .iter()
            .any(|p| p == "$.payload.body"),
        "expected $.payload.body in matched_paths, got {:?}",
        attribution.matched_paths
    );

    let integrity = engine
        .admin()
        .service()
        .check_integrity()
        .expect("integrity");
    assert_eq!(
        integrity.missing_property_fts_rows, 0,
        "no missing property FTS rows after rebuild"
    );

    emit_success_summary(
        "rust_stress_property_fts_rebuild_then_search",
        &[
            ("scalar_baseline_hits", scalar_logical_ids.len().to_string()),
            ("rebuilt_budget_hits", rebuilt.hits.len().to_string()),
            (
                "rebuilt_quarterly_hits",
                quarterly_after.hits.len().to_string(),
            ),
            (
                "heavy_writes",
                write_count.load(Ordering::Relaxed).to_string(),
            ),
        ],
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn adaptive_search_reads_never_block_on_background_writes() {
    // P8c-1: the adaptive text-search "reads never block on background
    // writes" invariant. Measure reader p99 latency with NO writers
    // (baseline), then with background writers actively ingesting and
    // registering property FTS schemas (under-load). Assert:
    //
    //     under_load_p99 <= max(10.0 * baseline_p99, 100ms absolute)
    //
    // A 100ms absolute ceiling is applied when baseline_p99 is very
    // small (sub-ms), because a 10x multiplier on a noisy 100us
    // baseline is itself noisy. Ratios and absolute values are logged.
    const READERS: usize = 8;
    const READ_ITERATIONS: usize = 150;

    let (_db, engine) = open_engine();
    register_note_recursive_schema(&engine);
    seed_notes(&engine, 150);
    let engine = Arc::new(engine);

    // --- Baseline: readers only, no writers. ---
    let baseline_samples = Arc::new(Mutex::new(Vec::<Duration>::new()));
    let barrier = Arc::new(Barrier::new(READERS));
    let mut handles = Vec::with_capacity(READERS);
    for _reader_id in 0..READERS {
        let engine = Arc::clone(&engine);
        let barrier = Arc::clone(&barrier);
        let baseline_samples = Arc::clone(&baseline_samples);
        handles.push(thread::spawn(move || {
            let mut local = Vec::with_capacity(READ_ITERATIONS);
            barrier.wait();
            for _ in 0..READ_ITERATIONS {
                let start = Instant::now();
                let rows = engine
                    .query("Note")
                    .text_search("budget", 20)
                    .execute()
                    .expect("baseline read");
                let elapsed = start.elapsed();
                assert!(!rows.hits.is_empty(), "baseline read returned zero hits");
                local.push(elapsed);
            }
            baseline_samples.lock().expect("lock").extend(local);
        }));
    }
    for handle in handles {
        handle.join().expect("baseline reader joins");
    }
    let baseline_samples = Arc::try_unwrap(baseline_samples)
        .expect("unique baseline arc")
        .into_inner()
        .expect("poison-free");
    let baseline_p99_us = p99_micros(&baseline_samples);

    // --- Under load: 3 writer threads churn new Notes while readers run. ---
    let stop = Arc::new(AtomicBool::new(false));
    let write_count = Arc::new(AtomicUsize::new(0));
    let errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut writer_handles = Vec::new();
    for thread_id in 0..3 {
        let engine = Arc::clone(&engine);
        let stop = Arc::clone(&stop);
        let write_count = Arc::clone(&write_count);
        let errors = Arc::clone(&errors);
        writer_handles.push(thread::spawn(move || {
            let mut iteration = 0usize;
            while !stop.load(Ordering::Relaxed) {
                let label = format!("p99-writer-{thread_id}-{iteration:05}");
                let logical_id = format!("note:{label}");
                let props = format!(
                    r#"{{"title":"budget Note {label}","payload":{{"body":"budget quarterly plan for {label}","tags":["p99"]}}}}"#
                );
                let request = WriteRequest {
                    label: label.clone(),
                    nodes: vec![NodeInsert {
                        row_id: new_row_id(),
                        logical_id,
                        kind: "Note".to_owned(),
                        properties: props,
                        source_ref: Some(format!("source:{label}")),
                        upsert: false,
                        chunk_policy: ChunkPolicy::Preserve,
                        content_ref: None,
                    }],
                    node_retires: vec![],
                    edges: vec![],
                    edge_retires: vec![],
                    chunks: vec![],
                    runs: vec![],
                    steps: vec![],
                    actions: vec![],
                    optional_backfills: vec![],
                    vec_inserts: vec![],
                    operational_writes: vec![],
                };
                if let Err(err) = engine.writer().submit(request) {
                    errors
                        .lock()
                        .expect("lock")
                        .push(format!("writer[{thread_id}]: {err}"));
                    stop.store(true, Ordering::Relaxed);
                    break;
                }
                write_count.fetch_add(1, Ordering::Relaxed);
                iteration += 1;
            }
        }));
    }

    let under_load_samples = Arc::new(Mutex::new(Vec::<Duration>::new()));
    let barrier = Arc::new(Barrier::new(READERS));
    let mut reader_handles = Vec::with_capacity(READERS);
    for _reader_id in 0..READERS {
        let engine = Arc::clone(&engine);
        let barrier = Arc::clone(&barrier);
        let under_load_samples = Arc::clone(&under_load_samples);
        reader_handles.push(thread::spawn(move || {
            let mut local = Vec::with_capacity(READ_ITERATIONS);
            barrier.wait();
            for _ in 0..READ_ITERATIONS {
                let start = Instant::now();
                let rows = engine
                    .query("Note")
                    .text_search("budget", 20)
                    .execute()
                    .expect("under-load read");
                let elapsed = start.elapsed();
                // Use `rows` to prevent the compiler from eliding work.
                let _: &[SearchHit] = &rows.hits;
                assert!(!rows.hits.is_empty(), "under-load read returned zero hits");
                local.push(elapsed);
            }
            under_load_samples.lock().expect("lock").extend(local);
        }));
    }
    for handle in reader_handles {
        handle.join().expect("under-load reader joins");
    }
    stop.store(true, Ordering::Relaxed);
    for handle in writer_handles {
        handle.join().expect("writer joins");
    }

    let writer_errors = errors.lock().expect("lock");
    assert!(
        writer_errors.is_empty(),
        "writer errors during p99 test: {writer_errors:?}"
    );
    drop(writer_errors);
    let writes_done = write_count.load(Ordering::Relaxed);
    assert!(writes_done > 0, "no writes completed under load");

    let under_load_samples = Arc::try_unwrap(under_load_samples)
        .expect("unique under-load arc")
        .into_inner()
        .expect("poison-free");
    let under_load_p99_us = p99_micros(&under_load_samples);

    // Threshold: max(10x baseline, 100ms absolute ceiling).
    let ten_x_baseline_us = baseline_p99_us * 10.0;
    let absolute_ceiling_us = 100_000.0_f64; // 100ms
    let threshold_us = ten_x_baseline_us.max(absolute_ceiling_us);

    #[allow(clippy::print_stderr)]
    {
        eprintln!(
            "adaptive_search_reads_never_block_on_background_writes: baseline_p99={:.0}us under_load_p99={:.0}us ratio={:.2}x threshold={:.0}us writes_under_load={} baseline_samples={} under_load_samples={}",
            baseline_p99_us,
            under_load_p99_us,
            under_load_p99_us / baseline_p99_us.max(1.0),
            threshold_us,
            writes_done,
            baseline_samples.len(),
            under_load_samples.len(),
        );
    }

    assert!(
        under_load_p99_us <= threshold_us,
        "under_load p99 {under_load_p99_us:.0}us exceeded threshold {threshold_us:.0}us (baseline p99 {baseline_p99_us:.0}us)"
    );

    emit_success_summary(
        "rust_stress_adaptive_search_reads_p99",
        &[
            ("readers", READERS.to_string()),
            ("iterations_per_reader", READ_ITERATIONS.to_string()),
            ("baseline_p99_us", format!("{baseline_p99_us:.0}")),
            ("under_load_p99_us", format!("{under_load_p99_us:.0}")),
            ("threshold_us", format!("{threshold_us:.0}")),
            ("writes_under_load", writes_done.to_string()),
        ],
    );
}
