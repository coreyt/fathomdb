//! Pack H.1 follow-up Part B: FFI-level coverage for
//! `configure_embedding_json` / `configure_vec_kind_json`.
//!
//! The napi wrappers in `node.rs` are pure delegation over these two
//! `admin_ffi::*_json` functions, so exercising them directly is
//! sufficient coverage for the TS napi surface. The TS parity test in
//! `typescript/.../configure_embedding_napi.test.ts` verifies the
//! JavaScript-side glue end-to-end.
#![cfg(feature = "sqlite-vec")]
#![allow(clippy::expect_used, clippy::panic)]

use fathomdb::admin_ffi::{configure_embedding_json, configure_vec_kind_json};
use fathomdb::{Engine, EngineOptions};
use serde_json::Value;
use tempfile::TempDir;

fn open_engine() -> (TempDir, Engine) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut opts = EngineOptions::new(dir.path().join("test.db"));
    opts.vector_dimension = Some(4);
    let engine = Engine::open(opts).expect("engine opens");
    (dir, engine)
}

#[test]
fn test_configure_embedding_json_activates_profile() {
    let (_dir, engine) = open_engine();
    let req = r#"{
        "model_identity": "test-model",
        "model_version": "1",
        "dimensions": 4,
        "normalization_policy": "none",
        "max_tokens": 512,
        "acknowledge_rebuild_impact": false
    }"#;
    let json = configure_embedding_json(&engine, req).expect("configure_embedding_json");
    let v: Value = serde_json::from_str(&json).expect("parse outcome");
    let outcome = v["outcome"].as_str().expect("outcome field");
    assert!(
        matches!(outcome, "activated" | "unchanged" | "replaced"),
        "unexpected outcome: {outcome}"
    );
}

#[test]
fn test_configure_vec_kind_json_chunks_roundtrip() {
    let (_dir, engine) = open_engine();
    // First activate an embedding profile — configure_vec_kind requires it.
    let emb_req = r#"{
        "model_identity": "test-model",
        "model_version": "1",
        "dimensions": 4,
        "max_tokens": 512
    }"#;
    configure_embedding_json(&engine, emb_req).expect("configure_embedding_json");

    let req = r#"{"kind": "KnowledgeItem", "source": "chunks"}"#;
    let json = configure_vec_kind_json(&engine, req).expect("configure_vec_kind_json");
    let v: Value = serde_json::from_str(&json).expect("parse outcome");
    assert_eq!(v["kind"], "KnowledgeItem");
    // outcome shape: kind, enqueued_backfill_rows, was_already_enabled
    assert!(v.get("enqueued_backfill_rows").is_some());
    assert!(v.get("was_already_enabled").is_some());
}

#[test]
fn test_configure_vec_kind_json_rejects_invalid_source() {
    let (_dir, engine) = open_engine();
    let req = r#"{"kind": "K", "source": "bogus"}"#;
    let err = configure_vec_kind_json(&engine, req).expect_err("should reject bogus source");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("unsupported vector source mode") || msg.to_lowercase().contains("bogus"),
        "unexpected error: {msg}"
    );
}
