//! EU-5a2 RED tests — schema + mean-centering machinery + K bump.
//!
//! Per `dev/plans/prompts/0.7.1-EMBEDDER-UNDEFER-HANDOFF.md` §EU-5
//! steps 4, 5, 6 and `dev/design/embedder.md` §§0.2–0.6, §7.
//!
//! These tests assert the structural half of EU-5a:
//!   1. Schema migration step 10 adds `mean_vec BLOB NULL` to
//!      `_fathomdb_embedder_profiles`.
//!   2. `OpenReport.embedder_mean_vec_pinned` reads from that column
//!      (no longer hard-coded `false`).
//!   3. Pack 2 binary-quant rerank K is 192 (HITL 2026-05-29 revision).
//!   4. Mean-centering apply path no-ops for identities whose
//!      `embedder_mean_centering_required` is false.
//!   5. Streaming f64 mean accumulator matches batch-computed mean.
//!   6. At-pin re-quantize pass touches the expected row count and
//!      emits a `MeanVecPinned` event with the right `doc_count`.

use std::sync::Arc;

use fathomdb_embedder::{EmbedderEvent, NoopEmbedder};
use fathomdb_engine::{
    mean_centering_internals_for_test, EmbedderChoice, Engine, EngineOpenError,
    MEAN_VEC_PIN_THRESHOLD, TOP_K_BIT_CANDIDATES,
};
use rusqlite::Connection;
use tempfile::TempDir;

fn fixture_path(name: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("{name}.sqlite"));
    (dir, path)
}

fn open_noop(name: &str) -> (TempDir, std::path::PathBuf, fathomdb_engine::OpenedEngine) {
    let (dir, path) = fixture_path(name);
    let opened =
        Engine::open_with_choice(&path, EmbedderChoice::Caller(Arc::new(NoopEmbedder::default())))
            .expect("open noop");
    (dir, path, opened)
}

#[test]
fn migration_step_10_adds_mean_vec_column() {
    let (_dir, path, opened) = open_noop("eu5a2_step10_col");
    // Drop the engine so the connection is released.
    drop(opened);
    let connection = Connection::open(&path).expect("reopen");
    let mut statement = connection
        .prepare("PRAGMA table_info(_fathomdb_embedder_profiles)")
        .expect("table_info prepare");
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?, // name
                row.get::<_, String>(2)?, // type
                row.get::<_, i64>(3)?,    // notnull
            ))
        })
        .expect("query_map");
    let mut found = false;
    for row in rows.flatten() {
        if row.0 == "mean_vec" {
            assert_eq!(row.1.to_ascii_uppercase(), "BLOB", "mean_vec column type must be BLOB");
            assert_eq!(row.2, 0, "mean_vec column must be nullable (notnull = 0)");
            found = true;
        }
    }
    assert!(found, "mean_vec column missing from _fathomdb_embedder_profiles");
}

#[test]
fn mean_vec_pinned_false_when_column_is_null() {
    // NoopEmbedder doesn't trigger pinning; the column should remain NULL
    // and `OpenReport.embedder_mean_vec_pinned` should read `false` from
    // the schema (no longer the EU-5a1 hard-coded `false`).
    let (_dir, _path, opened) = open_noop("eu5a2_pinned_false");
    assert!(!opened.report.embedder_mean_vec_pinned);
}

#[test]
fn mean_vec_pinned_true_when_column_is_set() {
    let (_dir, path, opened) = open_noop("eu5a2_pinned_true");
    drop(opened);
    // Manually populate mean_vec with the correct dimension (NoopEmbedder = 384 → 1536 bytes).
    let blob = vec![0u8; (384 * 4) as usize];
    {
        let connection = Connection::open(&path).expect("open for poke");
        connection
            .execute(
                "UPDATE _fathomdb_embedder_profiles SET mean_vec = ?1 WHERE profile = 'default'",
                rusqlite::params![blob],
            )
            .expect("update mean_vec");
    }
    let opened =
        Engine::open_with_choice(&path, EmbedderChoice::Caller(Arc::new(NoopEmbedder::default())))
            .expect("reopen");
    assert!(opened.report.embedder_mean_vec_pinned);
}

#[test]
fn mean_vec_dimension_mismatch_fails_closed_on_read() {
    let (_dir, path, opened) = open_noop("eu5a2_pinned_mismatch");
    drop(opened);
    // Wrong byte length (1532 instead of 384*4 = 1536).
    let blob = vec![0u8; 1532];
    {
        let connection = Connection::open(&path).expect("open for poke");
        connection
            .execute(
                "UPDATE _fathomdb_embedder_profiles SET mean_vec = ?1 WHERE profile = 'default'",
                rusqlite::params![blob],
            )
            .expect("update mean_vec");
    }
    let err =
        Engine::open_with_choice(&path, EmbedderChoice::Caller(Arc::new(NoopEmbedder::default())))
            .expect_err("dimension mismatch must fail closed");
    match err {
        EngineOpenError::EmbedderIdentityMismatch { .. } => {}
        other => panic!("expected EmbedderIdentityMismatch, got {other:?}"),
    }
}

#[test]
fn pack2_rerank_constant_is_192() {
    assert_eq!(TOP_K_BIT_CANDIDATES, 192, "Pack 2 K constant must be 192 (HITL 2026-05-29)");
}

#[test]
fn mean_vec_pin_threshold_is_256() {
    assert_eq!(MEAN_VEC_PIN_THRESHOLD, 256, "MEAN_VEC_PIN_THRESHOLD must be 256 per design §0.3");
}

#[test]
fn mean_centering_apply_path_noop_when_identity_not_mc_required() {
    // NoopEmbedder identity is NOT mean-centering required. A vector
    // write must produce the same sign-bits as the un-centered f32 even
    // if a (spurious, non-default-identity) mean_vec were present. The
    // simpler observable invariant: the stored f32 BLOB equals the
    // little-endian bytes of the embedder's raw output.
    let (_dir, _path, opened) = open_noop("eu5a2_apply_noop");
    opened.engine.configure_vector_kind_for_test("doc").expect("configure kind");
    let receipt = opened.engine.write_vector_for_test("doc", "hello").expect("vector write");
    let blob =
        opened.engine.read_vector_blob_for_test(receipt.cursor as i64).expect("read f32 blob");
    // NoopEmbedder emits [1.0, 0.0, 0.0, ...] of dim 384. The stored
    // BLOB must be that exact byte sequence (un-centered, no MC apply).
    let mut expected = Vec::with_capacity(384 * 4);
    expected.extend_from_slice(&1.0f32.to_le_bytes());
    for _ in 1..384 {
        expected.extend_from_slice(&0.0f32.to_le_bytes());
    }
    assert_eq!(blob, expected, "NoopEmbedder MC must no-op; stored bytes = raw embedder output");
}

#[test]
fn streaming_f64_accumulator_matches_batch_mean() {
    // Feed 500 deterministic f32 vectors into the streaming accumulator
    // and assert the materialized mean matches batch f64 computation
    // within 1e-6.
    let dim: usize = 16;
    let mut accumulator = mean_centering_internals_for_test::new_mean_accumulator(dim);
    let mut batch: Vec<Vec<f32>> = Vec::with_capacity(500);
    for i in 0..500 {
        let mut v = vec![0.0f32; dim];
        for (j, slot) in v.iter_mut().enumerate() {
            *slot = ((i * 7 + j * 13) as f32) * 0.001 - 1.0;
        }
        mean_centering_internals_for_test::accumulator_add(&mut accumulator, &v);
        batch.push(v);
    }
    let streaming_mean = mean_centering_internals_for_test::accumulator_materialize(&accumulator);
    // Batch-compute via f64.
    let mut batch_mean = vec![0.0f64; dim];
    for v in &batch {
        for (j, value) in v.iter().enumerate() {
            batch_mean[j] += *value as f64;
        }
    }
    for slot in &mut batch_mean {
        *slot /= batch.len() as f64;
    }
    assert_eq!(streaming_mean.len(), dim);
    for (j, &s) in streaming_mean.iter().enumerate() {
        let b = batch_mean[j];
        assert!(
            ((s as f64) - b).abs() < 1e-6,
            "streaming vs batch mean drift at index {j}: streaming={s} batch={b}"
        );
    }
    assert_eq!(mean_centering_internals_for_test::accumulator_count(&accumulator), 500);
}

#[test]
fn requantize_pass_is_bounded_by_count() {
    // Given 200 stored un-centered f32 BLOBs and a pinned mean, the
    // re-quantize pass updates exactly 200 sign-bit rows and emits one
    // `MeanVecPinned { dim, doc_count: 200 }` event.
    let dim: usize = 8;
    let mean = vec![0.5f32; dim];
    let mut rows: Vec<(i64, Vec<u8>)> = Vec::with_capacity(200);
    for i in 0..200 {
        let mut v = vec![0.0f32; dim];
        for (j, slot) in v.iter_mut().enumerate() {
            *slot = ((i + j) as f32) * 0.01;
        }
        let blob: Vec<u8> = v.iter().flat_map(|f| f.to_le_bytes()).collect();
        rows.push((i as i64 + 1, blob));
    }
    let (updated, events) = mean_centering_internals_for_test::run_requantize_pass(&rows, &mean);
    assert_eq!(updated, 200, "re-quantize pass must touch exactly 200 rows");
    assert_eq!(events.len(), 1, "exactly one MeanVecPinned event must be emitted");
    match &events[0] {
        EmbedderEvent::MeanVecPinned { dim: ev_dim, doc_count } => {
            assert_eq!(*ev_dim, dim as u32);
            assert_eq!(*doc_count, 200);
        }
        other => panic!("expected MeanVecPinned, got {other:?}"),
    }
}
