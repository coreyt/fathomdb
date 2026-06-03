//! Regression — a batched `engine.write` of N vector-indexed nodes must
//! produce N rows in `vector_default`, not 1.
//!
//! Pre-fix behavior: `write_inner` reserved one cursor for the entire
//! batch and shared it across every row, so `INSERT OR IGNORE INTO
//! vector_default(rowid, ...)` collapsed all batched nodes onto a single
//! rowid. See `dev/notes/0.7.0-engine-batch-vec0-collapse.md` for the
//! full root-cause writeup.
//!
//! Post-fix behavior: each row in the batch gets its own monotonically
//! increasing cursor; vec0 holds one row per node. The `WriteReceipt`
//! cursor remains the last (max) cursor in the batch so existing
//! cursor-equality readers stay correct.

use std::sync::Arc;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, PreparedWrite};
use tempfile::TempDir;

#[derive(Debug)]
struct VaryingEmbedder {
    identity: EmbedderIdentity,
    dim: u32,
}

impl VaryingEmbedder {
    fn new(dim: u32) -> Self {
        Self { identity: EmbedderIdentity::new("varying", "batch-cursor-regression", dim), dim }
    }
}

impl Embedder for VaryingEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
        let dim = self.dim as usize;
        let mut v = vec![0.0_f32; dim];
        let mut h: u64 = 0xcbf29ce4_84222325;
        for &b in text.as_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        for k in 0..6 {
            let coord = ((h >> (k * 8)) as usize) % dim;
            let sign = if (h >> (k * 8 + 7)) & 1 == 0 { 1.0 } else { -1.0 };
            v[coord] += sign * 0.5_f32;
        }
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-6);
        for x in &mut v {
            *x /= norm;
        }
        Ok(v)
    }
}

fn fresh_engine() -> (TempDir, Engine) {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("repro.sqlite");
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(VaryingEmbedder::new(768)))
        .expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("configure vector kind");
    (dir, opened.engine)
}

#[test]
fn batched_write_produces_one_vec0_row_per_node() {
    let (_dir, engine) = fresh_engine();
    let batch: Vec<PreparedWrite> = (0..21)
        .map(|i| PreparedWrite::Node {
            kind: "doc".to_string(),
            body: format!("body-{i} unique-token-{i}"),
            source_id: Some(format!("id-{i}")),
            logical_id: None,
        })
        .collect();
    engine.write(&batch).expect("write batch");
    engine.drain(15_000).expect("drain");

    let count = engine.vector_row_count_for_test().expect("count vector rows");
    assert_eq!(count, 21, "expected 21 vec0 rows, got {count}");
}

#[test]
fn batched_write_each_node_searchable_by_body() {
    // Stronger check: every body should be findable via engine.search
    // after a batched write. Pre-fix this returned hits for at most one
    // body in the batch.
    let (_dir, engine) = fresh_engine();
    let bodies: Vec<String> =
        (0..16).map(|i| format!("alpha bravo charlie unique-body-{i} payload")).collect();
    let batch: Vec<PreparedWrite> = bodies
        .iter()
        .enumerate()
        .map(|(i, body)| PreparedWrite::Node {
            kind: "doc".to_string(),
            body: body.clone(),
            source_id: Some(format!("id-{i}")),
            logical_id: None,
        })
        .collect();
    engine.write(&batch).expect("write batch");
    engine.drain(15_000).expect("drain");

    let mut hits = 0usize;
    for body in &bodies {
        let result = engine.search(body).expect("search");
        if !result.results.is_empty() {
            hits += 1;
        }
    }
    assert_eq!(hits, bodies.len(), "only {hits}/{} bodies were searchable", bodies.len());
}
