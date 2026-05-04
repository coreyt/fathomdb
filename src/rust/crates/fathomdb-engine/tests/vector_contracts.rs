use std::sync::Arc;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, EngineError, EngineOpenError};
use proptest::prelude::*;
use tempfile::TempDir;

#[derive(Clone, Debug)]
struct FixedEmbedder {
    identity: EmbedderIdentity,
    vector: Vector,
}

impl FixedEmbedder {
    fn new(name: &str, revision: &str, dimension: u32, vector: Vector) -> Self {
        Self { identity: EmbedderIdentity::new(name, revision, dimension), vector }
    }
}

impl Embedder for FixedEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        Ok(self.vector.clone())
    }
}

fn fixture_path(name: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("{name}.sqlite"));
    (dir, path)
}

#[test]
fn ac_030a_vector_write_without_embedder_is_rejected_at_call_boundary() {
    let (_dir, path) = fixture_path("no_embedder");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("configure vector kind");

    let err = opened.engine.write_vector_for_test("doc", "hello").expect_err("must reject");

    assert_eq!(err, EngineError::EmbedderNotConfigured);
    assert_eq!(opened.engine.vector_row_count_for_test().unwrap(), 0);
}

#[test]
fn ac_030b_vector_write_for_non_vector_kind_is_rejected_at_call_boundary() {
    let (_dir, path) = fixture_path("non_vector_kind");
    let embedder =
        Arc::new(FixedEmbedder::new("fathomdb-noop", "0.6.0-scaffold", 384, unit_vector(384)));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");

    let err = opened.engine.write_vector_for_test("doc", "hello").expect_err("must reject");

    assert_eq!(err, EngineError::KindNotVectorIndexed);
    assert_eq!(opened.engine.vector_row_count_for_test().unwrap(), 0);
}

#[test]
fn ac_030c_runtime_embedder_dimension_mismatch_is_rejected_before_vec_write() {
    let (_dir, path) = fixture_path("runtime_dim_mismatch");
    let embedder = Arc::new(FixedEmbedder::new("deterministic", "rev-a", 768, unit_vector(384)));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("configure vector kind");

    let err = opened.engine.write_vector_for_test("doc", "hello").expect_err("must reject");

    assert_eq!(err, EngineError::EmbedderDimensionMismatch { expected: 768, actual: 384 });
    assert_eq!(opened.engine.vector_row_count_for_test().unwrap(), 0);
}

#[test]
fn ac_066_wrong_dimension_embedder_return_rolls_back_without_vec_write() {
    let (_dir, path) = fixture_path("rollback_dim_mismatch");
    let bad = Arc::new(FixedEmbedder::new("deterministic", "rev-a", 384, unit_vector(383)));
    let opened = Engine::open_with_embedder_for_test(&path, bad).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("configure vector kind");

    let err = opened.engine.write_vector_for_test("doc", "hello").expect_err("must reject");
    assert_eq!(err, EngineError::EmbedderDimensionMismatch { expected: 384, actual: 383 });
    assert_eq!(opened.engine.vector_row_count_for_test().unwrap(), 0);

    opened.engine.close().expect("close");

    let good = Arc::new(FixedEmbedder::new("deterministic", "rev-a", 384, unit_vector(384)));
    let reopened = Engine::open_with_embedder_for_test(&path, good).expect("reopen");
    let receipt = reopened.engine.write_vector_for_test("doc", "hello").expect("success");
    assert_eq!(receipt.cursor, 1);
    assert_eq!(reopened.engine.vector_row_count_for_test().unwrap(), 1);
}

#[test]
fn open_persists_embedder_identity_exactly_as_supplied() {
    let (_dir, path) = fixture_path("identity_persisted");
    let embedder = Arc::new(FixedEmbedder::new("custom-name", "rev-z", 384, unit_vector(384)));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");

    let stored = opened.engine.default_embedder_profile_for_test().expect("profile");
    assert_eq!(stored, EmbedderIdentity::new("custom-name", "rev-z", 384));
}

#[test]
fn open_rejects_stored_identity_dimension_mismatch_with_supplied_embedder() {
    let (_dir, path) = fixture_path("stored_profile_mismatch");
    let first = Arc::new(FixedEmbedder::new("custom-name", "rev-a", 384, unit_vector(384)));
    let opened = Engine::open_with_embedder_for_test(&path, first).expect("open");
    opened.engine.close().expect("close");

    let second = Arc::new(FixedEmbedder::new("custom-name", "rev-a", 512, unit_vector(512)));
    let err = Engine::open_with_embedder_for_test(&path, second).expect_err("must fail open");

    assert_eq!(err, EngineOpenError::EmbedderDimensionMismatch { stored: 384, supplied: 512 });
}

proptest! {
    #[test]
    fn vector_blob_round_trip_preserves_le_f32_encoding(first in any::<[f32; 4]>()) {
        let mut values = vec![0.0_f32; 384];
        values[..4].copy_from_slice(&first);
        for value in &mut values {
            if !value.is_finite() {
                *value = 0.0;
            }
        }

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("blob_roundtrip.sqlite");
        let embedder = Arc::new(FixedEmbedder::new("blob", "rev", 384, values.clone()));
        let opened = Engine::open_with_embedder_for_test(&path, embedder).unwrap();
        opened.engine.configure_vector_kind_for_test("doc").unwrap();
        let receipt = opened.engine.write_vector_for_test("doc", "hello").unwrap();
        let blob = opened.engine.read_vector_blob_for_test(receipt.cursor as i64).unwrap();

        prop_assert_eq!(blob.len(), 384 * 4);
        let decoded: Vec<f32> = blob
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
            .collect();
        prop_assert_eq!(decoded, values);
    }
}

fn unit_vector(dimension: usize) -> Vector {
    let mut values = vec![0.0_f32; dimension];
    if dimension > 0 {
        values[0] = 1.0;
    }
    values
}
