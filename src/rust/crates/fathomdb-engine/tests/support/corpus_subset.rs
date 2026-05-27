//! Shared helpers for the Corpus-Pack 4 validation tests
//! (`corpus_fts.rs`, `corpus_vector.rs`, `corpus_graph.rs`).
//!
//! Loads small deterministic subsets of `data/corpus-data/raw/*.jsonl`
//! and `tests/corpus/chains/*.json` and ingests them into a temp
//! FathomDB instance via the same `PreparedWrite::Node` / `::Edge`
//! mapping as `examples/ingest_corpus.rs`.
//!
//! These helpers gracefully no-op when the corpus is not present on
//! disk (the data lives at `data/corpus-data/` per the corpus card and
//! is gitignored). Tests that depend on them call
//! [`load_subset_or_skip`] / [`load_chains_or_skip`] which return
//! `None` and emit a `SKIP:` line in that case, so `cargo test` stays
//! green in environments without the corpus checked out / restored
//! from cache.

#![allow(dead_code)] // helpers are referenced by sibling integration tests; cargo lints each in isolation

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, PreparedWrite};
use serde_json::Value;
use tempfile::TempDir;

pub const CORPUS_DIM: u32 = 768;
pub const VECTOR_KIND: &str = "doc";

const RELATION_TYPES: &[&str] = &[
    "replies_to",
    "follows_up_on",
    "summarizes",
    "action_from",
    "contradicts",
    "mentions",
    "cites",
];

/// Walks parents up from a Cargo test's CWD until it finds a directory
/// containing `tests/corpus/corpus-card.md` — that's the repo root.
/// Returns `None` if not found (e.g. when running from a packaged
/// crate dir without sibling repo state).
pub fn repo_root() -> Option<PathBuf> {
    let here = std::env::current_dir().ok()?;
    for ancestor in here.ancestors() {
        if ancestor.join("tests/corpus/corpus-card.md").exists() {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

#[derive(Clone, Debug)]
pub struct Doc {
    pub doc_id: String,
    pub source_type: String,
    pub title: Option<String>,
    pub body: String,
    pub parent_doc_id: Option<String>,
    pub tags: Vec<String>,
    pub relation_hint: Option<String>,
}

fn parse_doc(v: &Value) -> Option<Doc> {
    let doc_id = v.get("doc_id")?.as_str()?.to_string();
    let source_type = v.get("source_type")?.as_str()?.to_string();
    let body = v.get("body").and_then(Value::as_str).unwrap_or("").to_string();
    let title = v.get("title").and_then(Value::as_str).map(str::to_string);
    let parent_doc_id = v.get("parent_doc_id").and_then(Value::as_str).map(str::to_string);
    let tags: Vec<String> = v
        .get("tags")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(|t| t.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let relation_hint = tags.iter().find_map(|t| {
        t.strip_prefix("relation:").and_then(|r| {
            if RELATION_TYPES.contains(&r) {
                Some(r.to_string())
            } else {
                None
            }
        })
    });
    Some(Doc { doc_id, source_type, title, body, parent_doc_id, tags, relation_hint })
}

fn read_jsonl(path: &Path) -> Vec<Doc> {
    let Ok(text) = fs::read_to_string(path) else { return Vec::new() };
    let mut docs = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            if let Some(d) = parse_doc(&v) {
                docs.push(d);
            }
        }
    }
    docs.sort_by(|a, b| a.doc_id.cmp(&b.doc_id));
    docs
}

/// Load up to `per_source` docs from each source JSONL (sorted by
/// `doc_id` for determinism). Returns `None` if the corpus directory
/// is absent — caller should `return` (skip the test) in that case.
pub fn load_subset_or_skip(per_source: usize) -> Option<Vec<Doc>> {
    let root = repo_root()?;
    let raw_dir = root.join("data/corpus-data/raw");
    if !raw_dir.is_dir() {
        eprintln!(
            "SKIP: corpus not present at {} — run tests/corpus/scripts/acquire_*.py + generate_*.py first",
            raw_dir.display()
        );
        return None;
    }
    let entries: Vec<PathBuf> = match fs::read_dir(&raw_dir) {
        Ok(it) => it
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
            .collect(),
        Err(_) => return None,
    };
    if entries.is_empty() {
        eprintln!("SKIP: no JSONL files in {}", raw_dir.display());
        return None;
    }
    let mut paths = entries;
    paths.sort();

    let mut out = Vec::new();
    for path in paths {
        let mut docs = read_jsonl(&path);
        docs.truncate(per_source);
        out.extend(docs);
    }
    if out.is_empty() {
        eprintln!("SKIP: corpus loaded 0 docs (empty subset)");
        return None;
    }
    Some(out)
}

#[derive(Clone, Debug)]
pub struct Chain {
    pub chain_id: String,
    pub chain_shape: String,
    pub doc_ids: Vec<String>,
    pub anchor_doc_ids: Vec<String>,
    pub synthetic_doc_ids: Vec<String>,
}

/// Load up to `max_chains` chain JSONs sorted by filename. Returns
/// `None` if the chains directory is absent.
pub fn load_chains_or_skip(max_chains: usize) -> Option<Vec<Chain>> {
    let root = repo_root()?;
    let chains_dir = root.join("tests/corpus/chains");
    if !chains_dir.is_dir() {
        eprintln!("SKIP: chains dir absent at {}", chains_dir.display());
        return None;
    }
    let mut entries: Vec<PathBuf> = fs::read_dir(&chains_dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    entries.sort();
    let mut out = Vec::new();
    for path in entries.into_iter().take(max_chains) {
        let Ok(text) = fs::read_to_string(&path) else { continue };
        let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
        let chain_id = v.get("chain_id").and_then(Value::as_str).unwrap_or_default().to_string();
        let chain_shape =
            v.get("chain_shape").and_then(Value::as_str).unwrap_or_default().to_string();
        let doc_ids: Vec<String> = v
            .get("doc_ids")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        let anchor_doc_ids: Vec<String> = v
            .get("anchor_doc_ids")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        let synthetic_doc_ids: Vec<String> = v
            .get("synthetic_doc_ids")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        if !chain_id.is_empty() && !doc_ids.is_empty() {
            out.push(Chain { chain_id, chain_shape, doc_ids, anchor_doc_ids, synthetic_doc_ids });
        }
    }
    if out.is_empty() {
        return None;
    }
    Some(out)
}

/// Load docs needed to cover a set of chains: pulls every doc whose
/// `doc_id` appears in `wanted` from the per-source JSONLs.
pub fn load_chain_docs(wanted: &HashSet<String>) -> Option<Vec<Doc>> {
    let root = repo_root()?;
    let raw_dir = root.join("data/corpus-data/raw");
    if !raw_dir.is_dir() {
        return None;
    }
    let entries: Vec<PathBuf> = fs::read_dir(&raw_dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .collect();
    let mut out = Vec::new();
    let mut hit: HashSet<String> = HashSet::new();
    for path in entries {
        for d in read_jsonl(&path) {
            if wanted.contains(&d.doc_id) {
                hit.insert(d.doc_id.clone());
                out.push(d);
            }
        }
    }
    if hit.len() < wanted.len() {
        eprintln!("WARN: load_chain_docs found {}/{} requested doc_ids", hit.len(), wanted.len());
    }
    Some(out)
}

/// FNV-1a + 6-coordinate mass-placement embedder. Mirrors
/// `tests/perf_gates.rs::VaryingEmbedder` so corpus tests share its
/// determinism without depending on that crate-internal module.
#[derive(Clone, Debug)]
pub struct VaryingEmbedder {
    identity: EmbedderIdentity,
    dim: u32,
}

impl VaryingEmbedder {
    pub fn new(dim: u32) -> Self {
        Self { identity: EmbedderIdentity::new("varying", "corpus-pack-4", dim), dim }
    }

    fn vector_for(&self, text: &str) -> Vector {
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
        v
    }
}

impl Embedder for VaryingEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
        Ok(self.vector_for(text))
    }
}

/// Open a fresh engine in a tempdir wired up with [`VaryingEmbedder`]
/// at dim 768 and the canonical `doc` vector kind already configured.
pub fn fixture_engine() -> (TempDir, Engine) {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("corpus.sqlite");
    let embedder = Arc::new(VaryingEmbedder::new(CORPUS_DIM));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    opened.engine.configure_vector_kind_for_test(VECTOR_KIND).expect("configure vector kind");
    (dir, opened.engine)
}

/// Ingest a list of docs into the engine using the same node/edge
/// mapping as `examples/ingest_corpus.rs`. Returns a tuple of
/// (nodes_written, edges_written, edges_by_relation).
///
/// **One write per node.** A multi-node `engine.write(batch)` call
/// reserves one shared write_cursor for the whole batch, which
/// collapses to a single row in `vector_default` (`INSERT OR IGNORE`
/// on the cursor-rowid PK). Pack-4 tests need a vec0 row per node, so
/// each node here ships its own `engine.write` call. Documented in
/// `dev/notes/0.7.0-engine-batch-vec0-collapse.md`; once the engine
/// owner lands a fix, the workaround can be reverted to a batched
/// write.
pub fn ingest(engine: &Engine, docs: &[Doc]) -> (usize, usize, BTreeMap<String, usize>) {
    let mut nodes_written = 0usize;
    let mut edges_written = 0usize;
    let mut edges_by_relation: BTreeMap<String, usize> = BTreeMap::new();
    let doc_ids: HashSet<String> = docs.iter().map(|d| d.doc_id.clone()).collect();

    for doc in docs {
        engine
            .write(&[PreparedWrite::Node {
                // NB: tests deliberately use the locked vector kind
                // ("doc") for every doc rather than the doc's source_type.
                // The configure_vector_kind_for_test API lets us register
                // exactly one kind as vector-indexed; all corpus nodes
                // share that kind so engine.search can score them
                // uniformly. The doc's *semantic* source_type is preserved
                // in source_id-style metadata downstream.
                kind: VECTOR_KIND.to_string(),
                body: doc.body.clone(),
                source_id: Some(doc.doc_id.clone()),
            }])
            .expect("write node");
        nodes_written += 1;
    }

    for doc in docs {
        let Some(parent) = doc.parent_doc_id.as_ref() else { continue };
        if !doc_ids.contains(parent) {
            continue;
        }
        let kind = doc.relation_hint.clone().unwrap_or_else(|| "linked".to_string());
        *edges_by_relation.entry(kind.clone()).or_insert(0) += 1;
        engine
            .write(&[PreparedWrite::Edge {
                kind,
                from: parent.clone(),
                to: doc.doc_id.clone(),
                source_id: Some(doc.doc_id.clone()),
            }])
            .expect("write edge");
        edges_written += 1;
    }

    engine.drain(30_000).expect("drain after ingest");
    (nodes_written, edges_written, edges_by_relation)
}

/// Pull a "salient" phrase from a doc body — used by the FTS test to
/// pick query terms that are likely to be unique to that doc.
/// Strategy: take the first non-empty line, drop common noise tokens,
/// and return the longest remaining word (capped at 32 chars).
pub fn salient_word(body: &str) -> Option<String> {
    for line in body.lines() {
        let trimmed = line.trim_start_matches(['-', '*', '#', ' ']).trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut candidates: Vec<&str> = trimmed
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
            .filter(|w| w.len() >= 6 && w.len() <= 32 && !is_stop_word(w))
            .collect();
        candidates.sort_by_key(|w| std::cmp::Reverse(w.len()));
        if let Some(w) = candidates.first() {
            return Some((*w).to_string());
        }
    }
    None
}

fn is_stop_word(w: &str) -> bool {
    matches!(
        w.to_ascii_lowercase().as_str(),
        "their"
            | "there"
            | "these"
            | "those"
            | "which"
            | "would"
            | "could"
            | "about"
            | "after"
            | "before"
            | "where"
            | "while"
            | "subject"
            | "from"
            | "recipients"
            | "file"
            | "project"
            | "redacted"
    )
}
