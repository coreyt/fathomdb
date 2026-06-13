//! Corpus-Pack 3 ingest harness.
//!
//! Reads the Pack-1 + Pack-2 corpus artifacts and ingests every document
//! into a FathomDB instance via the public engine.write API.
//!
//! Inputs (all paths relative to the repo root, override via flags):
//!   --jsonl-dir   data/corpus-data/raw     per-source canonical JSONL
//!   --chains-dir  tests/corpus/chains      chain JSON ground-truth specs
//!   --db          tests/corpus/.cache/db   FathomDB instance dir
//!
//! Mapping (per the handoff §"Corpus-Pack 3"):
//!   doc.body         -> PreparedWrite::Node.body
//!   doc.source_type  -> PreparedWrite::Node.kind
//!   doc.doc_id       -> PreparedWrite::Node.source_id (recovery seam)
//!   doc.parent_doc_id (when set + present in corpus) ->
//!                       PreparedWrite::Edge {from: parent, to: child,
//!                       kind: <relation>, source_id: child doc_id}
//!   chain.ground_truth_queries -> NOT ingested (eval-only signal).
//!
//! Relation kind for an edge is taken from the child doc's tags:
//! `relation:<rel>` where <rel> is one of the 7 locked relation types.
//! Chain connectives + QMSum query-summary docs carry this tag; docs
//! without it default to `linked`.
//!
//! Idempotency: each doc is checked via engine.trace_source_ref before
//! write. Re-running on an existing DB skips already-ingested docs and
//! emits no duplicate edges.
//!
//! Volume guards: writes are batched (NODE_BATCH / EDGE_BATCH) to keep
//! transaction sizes bounded.
//!
//! Run:
//!     cargo run --example ingest_corpus -- \
//!       --db tests/corpus/.cache/db \
//!       --jsonl-dir data/corpus-data/raw \
//!       --chains-dir tests/corpus/chains
//!
//! Closure summary is printed as JSON on stdout; progress lines on
//! stderr.

use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use fathomdb_engine::{EmbedderChoice, Engine, PreparedWrite};
use serde_json::Value;

const NODE_BATCH: usize = 200;
const EDGE_BATCH: usize = 200;
const VALID_SOURCE_TYPES: &[&str] = &["email", "meeting", "paper", "article", "note", "todo"];
const RELATION_TYPES: &[&str] = &[
    "replies_to",
    "follows_up_on",
    "summarizes",
    "action_from",
    "contradicts",
    "mentions",
    "cites",
];

struct Args {
    db: PathBuf,
    jsonl_dir: PathBuf,
    chains_dir: PathBuf,
    /// "default" → `Engine::open` (identity-only, no vectors; FTS/graph
    /// ingest, the historical behaviour). "bge" → `open_with_choice(Default)`,
    /// materializing the pinned `CandleBgeEmbedder` so vectors are computed at
    /// ingest. The "bge" path requires the `default-embedder` Cargo feature
    /// (else it fails closed at open with a typed embedder error).
    embedder: String,
}

fn parse_args() -> Result<Args, String> {
    let mut db = PathBuf::from("tests/corpus/.cache/db");
    let mut jsonl_dir = PathBuf::from("data/corpus-data/raw");
    let mut chains_dir = PathBuf::from("tests/corpus/chains");
    let mut embedder = String::from("default");
    let mut it = env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--db" => db = it.next().ok_or("--db needs a path")?.into(),
            "--jsonl-dir" => jsonl_dir = it.next().ok_or("--jsonl-dir needs a path")?.into(),
            "--chains-dir" => chains_dir = it.next().ok_or("--chains-dir needs a path")?.into(),
            "--embedder" => {
                embedder = it.next().ok_or("--embedder needs a value (default|bge)")?;
                if embedder != "default" && embedder != "bge" {
                    return Err(format!("--embedder must be `default` or `bge`, got `{embedder}`"));
                }
            }
            "--help" | "-h" => {
                eprintln!(
                    "Usage: ingest_corpus [--db PATH] [--jsonl-dir PATH] [--chains-dir PATH] \
                     [--embedder default|bge]"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown arg: {other}")),
        }
    }
    Ok(Args { db, jsonl_dir, chains_dir, embedder })
}

#[derive(Debug)]
struct Doc {
    doc_id: String,
    source_type: String,
    body: String,
    parent_doc_id: Option<String>,
    relation_hint: Option<String>,
}

fn relation_from_tags(tags: &[Value]) -> Option<String> {
    for t in tags {
        if let Some(s) = t.as_str() {
            if let Some(rest) = s.strip_prefix("relation:") {
                if RELATION_TYPES.contains(&rest) {
                    return Some(rest.to_string());
                }
            }
        }
    }
    None
}

fn parse_doc(line: &str) -> Result<Doc, String> {
    let v: Value = serde_json::from_str(line).map_err(|e| format!("bad JSONL row: {e}"))?;
    let doc_id = v.get("doc_id").and_then(Value::as_str).ok_or("missing doc_id")?.to_string();
    let source_type =
        v.get("source_type").and_then(Value::as_str).ok_or("missing source_type")?.to_string();
    if !VALID_SOURCE_TYPES.contains(&source_type.as_str()) {
        return Err(format!("doc {doc_id}: unknown source_type {source_type}"));
    }
    let body = v.get("body").and_then(Value::as_str).unwrap_or("").to_string();
    let parent_doc_id = v.get("parent_doc_id").and_then(Value::as_str).map(str::to_string);
    let tags = v.get("tags").and_then(Value::as_array).cloned().unwrap_or_default();
    let relation_hint = relation_from_tags(&tags);
    Ok(Doc { doc_id, source_type, body, parent_doc_id, relation_hint })
}

fn jsonl_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("read {dir:?}: {e}"))?
        .filter_map(|r| r.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .collect();
    paths.sort();
    Ok(paths)
}

fn load_all_docs(jsonl_dir: &Path) -> Result<Vec<Doc>, String> {
    let mut out = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();
    for path in jsonl_files(jsonl_dir)? {
        eprintln!("reading {}", path.display());
        let f = fs::File::open(&path).map_err(|e| format!("open {path:?}: {e}"))?;
        let rdr = BufReader::new(f);
        let mut n = 0usize;
        for line in rdr.lines() {
            let line = line.map_err(|e| format!("read {path:?}: {e}"))?;
            if line.trim().is_empty() {
                continue;
            }
            let doc = parse_doc(&line).map_err(|e| format!("{}: {e}", path.display()))?;
            if !seen_ids.insert(doc.doc_id.clone()) {
                // Duplicate across JSONL files — skip silently. Common when
                // a chain connective and a base source emit the same doc_id
                // (shouldn't happen, but defensive).
                continue;
            }
            out.push(doc);
            n += 1;
        }
        eprintln!("  {n} docs");
    }
    Ok(out)
}

/// Returns the set of doc_ids already present in the database
/// (canonical_nodes.source_id NOT NULL). The check uses the public
/// trace_source_ref API per source_id; we batch by sweeping the
/// in-memory doc set and asking the engine for each.
///
/// O(N_docs) public-API calls; ~1ms each on a warm SQLite, so ~10s for
/// 10k docs — well under the harness's 10-min budget.
fn existing_source_ids(
    engine: &fathomdb_engine::Engine,
    docs: &[Doc],
) -> Result<HashSet<String>, String> {
    let mut seen = HashSet::new();
    for (i, doc) in docs.iter().enumerate() {
        let report = engine
            .trace_source_ref(&doc.doc_id)
            .map_err(|e| format!("trace_source_ref({}): {e:?}", doc.doc_id))?;
        if !report.events.is_empty() {
            seen.insert(doc.doc_id.clone());
        }
        if (i + 1) % 2000 == 0 {
            eprintln!("  idempotency-check: {}/{}", i + 1, docs.len());
        }
    }
    Ok(seen)
}

fn flush_nodes(engine: &Engine, batch: &mut Vec<PreparedWrite>) -> Result<usize, String> {
    if batch.is_empty() {
        return Ok(0);
    }
    let n = batch.len();
    engine.write(batch).map_err(|e| format!("write nodes ({n}): {e:?}"))?;
    batch.clear();
    Ok(n)
}

fn flush_edges(engine: &Engine, batch: &mut Vec<PreparedWrite>) -> Result<usize, String> {
    if batch.is_empty() {
        return Ok(0);
    }
    let n = batch.len();
    engine.write(batch).map_err(|e| format!("write edges ({n}): {e:?}"))?;
    batch.clear();
    Ok(n)
}

fn run(args: Args) -> Result<(), String> {
    let started = Instant::now();
    // Engine::open expects a path to a .sqlite file (created if missing).
    // If --db points at an existing dir, place the DB inside as
    // "corpus.sqlite"; otherwise use --db itself (appending .sqlite if
    // missing).
    let db_path = if args.db.is_dir() {
        args.db.join("corpus.sqlite")
    } else if args.db.extension().is_some_and(|e| e == "sqlite") {
        args.db.clone()
    } else {
        let mut p = args.db.clone();
        let name = p.file_name().map(|n| n.to_owned()).unwrap_or_else(|| "corpus".into());
        let mut owned = name.to_owned();
        owned.push(".sqlite");
        p.set_file_name(owned);
        p
    };
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create db parent: {e}"))?;
    }
    eprintln!("opening engine at {} (embedder={})", db_path.display(), args.embedder);
    let opened = match args.embedder.as_str() {
        // Materialize the pinned BGE embedder so vectors are computed at ingest
        // (requires --features default-embedder; downloads ~135 MB on a cold
        // cache, then local-IO). This is the path that produces a real,
        // vector-searchable corpus DB.
        "bge" => Engine::open_with_choice(&db_path, EmbedderChoice::Default)
            .map_err(|e| format!("open (bge): {e:?}"))?,
        // Historical identity-only ingest (nodes/edges/FTS, no vectors).
        _ => Engine::open(&db_path).map_err(|e| format!("open: {e:?}"))?,
    };
    let engine = opened.engine;

    eprintln!("loading docs from {}", args.jsonl_dir.display());
    let docs = load_all_docs(&args.jsonl_dir)?;
    eprintln!("loaded {} unique docs", docs.len());

    // On the BGE path, register every source_type as a vector-indexed kind
    // BEFORE writing nodes — the engine only embeds (via the open embedder's
    // async projection) for kinds present in _fathomdb_vector_kinds. Without
    // this, nodes ingest with FTS only and NO vectors are computed. Registering
    // each kind (rather than remapping to a single "doc" kind like EU-7) keeps
    // the corpus searchable across all six source types. `configure_vector_kind`
    // is the same (hidden) API EU-7 uses; vector-indexing arbitrary kinds is not
    // yet production-surfaced.
    if args.embedder == "bge" {
        let mut kinds: Vec<String> = docs
            .iter()
            .map(|d| d.source_type.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        kinds.sort();
        for kind in &kinds {
            engine
                .configure_vector_kind_for_test(kind)
                .map_err(|e| format!("configure_vector_kind({kind}): {e:?}"))?;
        }
        eprintln!("registered {} vector-indexed kinds: {kinds:?}", kinds.len());
    }

    let doc_ids: HashSet<String> = docs.iter().map(|d| d.doc_id.clone()).collect();
    eprintln!("checking which docs are already in the DB (idempotency)...");
    let existing = existing_source_ids(&engine, &docs)?;
    eprintln!("  {} of {} docs already ingested", existing.len(), docs.len());

    // --- Pass 1: write nodes ---
    let mut nodes_written = 0usize;
    let mut node_batch: Vec<PreparedWrite> = Vec::with_capacity(NODE_BATCH);
    let mut by_type: BTreeMap<String, usize> = BTreeMap::new();
    for doc in &docs {
        if existing.contains(&doc.doc_id) {
            continue;
        }
        *by_type.entry(doc.source_type.clone()).or_insert(0) += 1;
        node_batch.push(PreparedWrite::Node {
            kind: doc.source_type.clone(),
            body: doc.body.clone(),
            source_id: Some(doc.doc_id.clone()),
            logical_id: None,
        });
        if node_batch.len() >= NODE_BATCH {
            nodes_written += flush_nodes(&engine, &mut node_batch)?;
            if nodes_written.is_multiple_of(2000) {
                eprintln!("  nodes written: {nodes_written}");
            }
        }
    }
    nodes_written += flush_nodes(&engine, &mut node_batch)?;
    eprintln!("nodes: wrote {nodes_written} new (per source_type: {by_type:?})");

    // --- Pass 2: write edges for parent_doc_id where parent is also in the corpus ---
    // The edge's source_id is the child doc_id so a follow-up re-ingest
    // skipping the child also skips its edges (the trace check covers both
    // canonical_nodes and canonical_edges).
    let mut edges_written = 0usize;
    let mut edge_batch: Vec<PreparedWrite> = Vec::with_capacity(EDGE_BATCH);
    let mut by_relation: BTreeMap<String, usize> = BTreeMap::new();
    let mut orphan_parents = 0usize;
    for doc in &docs {
        let Some(parent) = doc.parent_doc_id.as_ref() else { continue };
        if !doc_ids.contains(parent) {
            orphan_parents += 1;
            continue;
        }
        if existing.contains(&doc.doc_id) {
            continue;
        }
        let kind = doc.relation_hint.clone().unwrap_or_else(|| "linked".to_string());
        *by_relation.entry(kind.clone()).or_insert(0) += 1;
        edge_batch.push(PreparedWrite::Edge {
            kind,
            from: parent.clone(),
            to: doc.doc_id.clone(),
            source_id: Some(doc.doc_id.clone()),
            logical_id: None,
            body: None,
            t_valid: None,
            t_invalid: None,
            confidence: None,
            extractor_model_id: None,
        });
        if edge_batch.len() >= EDGE_BATCH {
            edges_written += flush_edges(&engine, &mut edge_batch)?;
        }
    }
    edges_written += flush_edges(&engine, &mut edge_batch)?;
    eprintln!("edges: wrote {edges_written} new (per relation: {by_relation:?}); {orphan_parents} parent_doc_id refs skipped (parent not in corpus)");

    // --- Pass 3: chain validation (read-only) ---
    // We don't ingest chain JSONs themselves — they are eval ground truth.
    // We DO sanity-check that every chain's doc_ids are present in the
    // corpus (would catch a Pack-1/Pack-2 mismatch early).
    let chains_present = args.chains_dir.exists();
    let mut chains_validated = 0usize;
    let mut chains_missing_doc = 0usize;
    if chains_present {
        for entry in fs::read_dir(&args.chains_dir).map_err(|e| format!("read chains: {e}"))? {
            let path = entry.map_err(|e| format!("chains entry: {e}"))?.path();
            if path.extension().is_some_and(|e| e == "json") {
                let s = fs::read_to_string(&path).map_err(|e| format!("read {path:?}: {e}"))?;
                let v: Value =
                    serde_json::from_str(&s).map_err(|e| format!("parse {path:?}: {e}"))?;
                if let Some(ids) = v.get("doc_ids").and_then(Value::as_array) {
                    let mut ok = true;
                    for id in ids {
                        let s = id.as_str().unwrap_or("");
                        if !doc_ids.contains(s) {
                            ok = false;
                            break;
                        }
                    }
                    if ok {
                        chains_validated += 1;
                    } else {
                        chains_missing_doc += 1;
                    }
                }
            }
        }
    }

    // The noop/identity ingest projects ~instantly; the BGE path must embed
    // every node through the async projection workers, so give it a generous
    // (but bounded) budget — drain returns as soon as the queue is empty.
    let drain_ms: u64 = if args.embedder == "bge" { 3_600_000 } else { 60_000 };
    engine.drain(drain_ms).map_err(|e| format!("drain: {e:?}"))?;
    let counters = engine.counters();
    engine.close().map_err(|e| format!("close: {e:?}"))?;

    let elapsed = started.elapsed();
    let summary = serde_json::json!({
        "db": db_path.display().to_string(),
        "docs_total": docs.len(),
        "docs_already_present": existing.len(),
        "nodes_written": nodes_written,
        "nodes_by_source_type": by_type,
        "edges_written": edges_written,
        "edges_by_relation": by_relation,
        "orphan_parent_refs": orphan_parents,
        "chains_validated": chains_validated,
        "chains_missing_doc": chains_missing_doc,
        "elapsed_seconds": elapsed.as_secs_f64(),
        "engine_counters": {
            "writes": counters.writes,
            "write_rows": counters.write_rows,
            "queries": counters.queries,
        },
    });
    println!("{}", serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?);
    Ok(())
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ingest_corpus failed: {e}");
            ExitCode::FAILURE
        }
    }
}
