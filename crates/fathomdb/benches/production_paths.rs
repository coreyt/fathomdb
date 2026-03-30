#![allow(clippy::expect_used)]

use std::sync::atomic::{AtomicU64, Ordering};

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
#[cfg(feature = "sqlite-vec")]
use fathomdb::VecInsert;
use fathomdb::{
    ChunkInsert, ChunkPolicy, Engine, EngineOptions, NodeInsert, QueryBuilder, SafeExportOptions,
    WriteRequest,
};
use tempfile::TempDir;

fn open_engine(vector_dimension: Option<usize>) -> (TempDir, Engine) {
    let dir = TempDir::new().expect("temp dir");
    let mut options = EngineOptions::new(dir.path().join("bench.db"));
    options.vector_dimension = vector_dimension;
    let engine = Engine::open(options).expect("engine opens");
    (dir, engine)
}

fn single_node_chunk_request(index: u64) -> WriteRequest {
    WriteRequest {
        label: format!("bench-write-{index}"),
        nodes: vec![NodeInsert {
            row_id: format!("row-{index}"),
            logical_id: format!("meeting-{index}"),
            kind: "Meeting".to_owned(),
            properties: format!(r#"{{"title":"Meeting {index}"}}"#),
            source_ref: Some(format!("bench-src-{index}")),
            upsert: false,
            chunk_policy: ChunkPolicy::Preserve,
        }],
        node_retires: vec![],
        edges: vec![],
        edge_retires: vec![],
        chunks: vec![ChunkInsert {
            id: format!("chunk-{index}"),
            node_logical_id: format!("meeting-{index}"),
            text_content: format!("quarterly planning notes {index}"),
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

#[cfg(feature = "sqlite-vec")]
fn single_vector_request(index: u64) -> WriteRequest {
    WriteRequest {
        label: format!("bench-vector-{index}"),
        nodes: vec![NodeInsert {
            row_id: format!("vec-row-{index}"),
            logical_id: format!("vec-meeting-{index}"),
            kind: "Meeting".to_owned(),
            properties: format!(r#"{{"title":"Vector Meeting {index}"}}"#),
            source_ref: Some(format!("bench-vec-src-{index}")),
            upsert: false,
            chunk_policy: ChunkPolicy::Preserve,
        }],
        node_retires: vec![],
        edges: vec![],
        edge_retires: vec![],
        chunks: vec![ChunkInsert {
            id: format!("vec-chunk-{index}"),
            node_logical_id: format!("vec-meeting-{index}"),
            text_content: format!("semantic planning notes {index}"),
            byte_start: None,
            byte_end: None,
        }],
        runs: vec![],
        steps: vec![],
        actions: vec![],
        optional_backfills: vec![],
        vec_inserts: vec![VecInsert {
            chunk_id: format!("vec-chunk-{index}"),
            embedding: vec![0.1, 0.2, 0.3, 0.4],
        }],
        operational_writes: vec![],
    }
}

fn benchmark_write_submit(c: &mut Criterion) {
    let (_dir, engine) = open_engine(None);
    let counter = AtomicU64::new(0);
    c.bench_function("write_submit_single_node_chunk", |b| {
        b.iter(|| {
            let index = counter.fetch_add(1, Ordering::Relaxed);
            engine
                .writer()
                .submit(single_node_chunk_request(index))
                .expect("write succeeds");
        });
    });
}

fn benchmark_text_query(c: &mut Criterion) {
    let (_dir, engine) = open_engine(None);
    for index in 0..250 {
        engine
            .writer()
            .submit(single_node_chunk_request(index))
            .expect("seed write succeeds");
    }

    let compiled = QueryBuilder::nodes("Meeting")
        .text_search("quarterly", 10)
        .limit(10)
        .compile()
        .expect("compile");

    c.bench_function("query_execute_text_search", |b| {
        b.iter(|| {
            let rows = engine
                .coordinator()
                .execute_compiled_read(&compiled)
                .expect("query succeeds");
            assert!(!rows.nodes.is_empty());
        });
    });
}

fn benchmark_safe_export(c: &mut Criterion) {
    let (dir, engine) = open_engine(None);
    for index in 0..50 {
        engine
            .writer()
            .submit(single_node_chunk_request(index))
            .expect("seed write succeeds");
    }

    let counter = AtomicU64::new(0);
    c.bench_function("admin_safe_export", |b| {
        b.iter_batched(
            || {
                dir.path().join(format!(
                    "export-{}.db",
                    counter.fetch_add(1, Ordering::Relaxed)
                ))
            },
            |export_path| {
                let manifest = engine
                    .admin()
                    .service()
                    .safe_export(
                        &export_path,
                        SafeExportOptions {
                            force_checkpoint: false,
                        },
                    )
                    .expect("safe export succeeds");
                assert_eq!(manifest.protocol_version, 1);
            },
            BatchSize::SmallInput,
        );
    });
}

#[cfg(feature = "sqlite-vec")]
fn benchmark_vector_query(c: &mut Criterion) {
    let (_dir, engine) = open_engine(Some(4));
    for index in 0..250 {
        engine
            .writer()
            .submit(single_vector_request(index))
            .expect("seed vector write succeeds");
    }

    let compiled = QueryBuilder::nodes("Meeting")
        .vector_search("[0.1, 0.2, 0.3, 0.4]", 10)
        .limit(10)
        .compile()
        .expect("compile");

    c.bench_function("query_execute_vector_search", |b| {
        b.iter(|| {
            let rows = engine
                .coordinator()
                .execute_compiled_read(&compiled)
                .expect("vector query succeeds");
            assert!(!rows.nodes.is_empty());
        });
    });
}

#[cfg(not(feature = "sqlite-vec"))]
criterion_group!(
    production_paths,
    benchmark_write_submit,
    benchmark_text_query,
    benchmark_safe_export,
);

#[cfg(feature = "sqlite-vec")]
criterion_group!(
    production_paths,
    benchmark_write_submit,
    benchmark_text_query,
    benchmark_safe_export,
    benchmark_vector_query,
);
criterion_main!(production_paths);
