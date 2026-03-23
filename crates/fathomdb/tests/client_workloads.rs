mod helpers;
mod injection;

use fathomdb::{Engine, EngineOptions};
use tempfile::NamedTempFile;

fn open_engine() -> (NamedTempFile, Engine) {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");
    (db, engine)
}

// ── Memex workloads ──────────────────────────────────────────────────────────

/// M-1: Ingest a meeting transcript as a node with text chunks.
#[test]
#[ignore = "Layer 4 stub — not yet implemented"]
fn m1_meeting_transcript_ingestion() {
    let (_db, _engine) = open_engine();
    todo!()
}

/// M-2: Correct a meeting note via upsert (supersession).
#[test]
#[ignore = "Layer 4 stub — not yet implemented"]
fn m2_meeting_note_correction_via_upsert() {
    let (_db, _engine) = open_engine();
    todo!()
}

/// M-3: Verify FTS search returns the ingested transcript.
#[test]
#[ignore = "Layer 4 stub — not yet implemented"]
fn m3_fts_search_returns_meeting_transcript() {
    let (_db, _engine) = open_engine();
    todo!()
}

/// M-4: Verify historical versions are preserved after upsert.
#[test]
#[ignore = "Layer 4 stub — not yet implemented"]
fn m4_history_preserved_after_upsert() {
    let (_db, _engine) = open_engine();
    todo!()
}

/// M-5: Excise a meeting by source_ref and verify all descendants are removed.
#[test]
#[ignore = "Layer 4 stub — not yet implemented"]
fn m5_excise_by_source_ref() {
    let (_db, _engine) = open_engine();
    todo!()
}

/// M-6: Rebuild FTS projections after deletion and verify integrity.
#[test]
#[ignore = "Layer 4 stub — not yet implemented"]
fn m6_fts_rebuild_restores_integrity() {
    let (_db, _engine) = open_engine();
    todo!()
}

// ── OpenClaw workloads ───────────────────────────────────────────────────────

/// OC-1: Persist agent context as a node and retrieve it by kind.
#[test]
#[ignore = "Layer 4 stub — not yet implemented"]
fn oc1_persist_and_retrieve_agent_context() {
    let (_db, _engine) = open_engine();
    todo!()
}

/// OC-2: Append a new context version and verify old version is superseded.
#[test]
#[ignore = "Layer 4 stub — not yet implemented"]
fn oc2_context_versioning_via_supersession() {
    let (_db, _engine) = open_engine();
    todo!()
}

/// OC-3: Write provenance-tagged run/step/action records.
#[test]
#[ignore = "Layer 4 stub — not yet implemented"]
fn oc3_write_provenance_run_step_action() {
    let (_db, _engine) = open_engine();
    todo!()
}

/// OC-4: Traverse edges to walk a task dependency graph.
#[test]
#[ignore = "Layer 4 stub — not yet implemented"]
fn oc4_traverse_task_dependency_graph() {
    let (_db, _engine) = open_engine();
    todo!()
}

/// OC-5: Retire an edge and verify it no longer appears in traversal results.
#[test]
#[ignore = "Layer 4 stub — not yet implemented"]
fn oc5_edge_retire_removes_from_traversal() {
    let (_db, _engine) = open_engine();
    todo!()
}

/// OC-6: Verify check_semantics is clean after a full agent workload.
#[test]
#[ignore = "Layer 4 stub — not yet implemented"]
fn oc6_check_semantics_clean_after_workload() {
    let (_db, _engine) = open_engine();
    todo!()
}

// ── HermesClaw workloads ─────────────────────────────────────────────────────

/// HC-1: Persist an agent self-evaluation node and verify retrieval.
#[test]
#[ignore = "Layer 4 stub — not yet implemented"]
fn hc1_self_evaluation_node_round_trip() {
    let (_db, _engine) = open_engine();
    todo!()
}

/// HC-2: Update an evaluation result via upsert and confirm supersession chain.
#[test]
#[ignore = "Layer 4 stub — not yet implemented"]
fn hc2_evaluation_update_supersession_chain() {
    let (_db, _engine) = open_engine();
    todo!()
}

/// HC-3: Excise a flagged evaluation and verify no orphans remain.
#[test]
#[ignore = "Layer 4 stub — not yet implemented"]
fn hc3_excise_flagged_evaluation() {
    let (_db, _engine) = open_engine();
    todo!()
}

/// HC-4: Rebuild projections after evaluation data loss.
#[test]
#[ignore = "Layer 4 stub — not yet implemented"]
fn hc4_projection_rebuild_after_data_loss() {
    let (_db, _engine) = open_engine();
    todo!()
}

/// HC-5: Verify FTS search finds an evaluation note after rebuild.
#[test]
#[ignore = "Layer 4 stub — not yet implemented"]
fn hc5_fts_search_after_rebuild() {
    let (_db, _engine) = open_engine();
    todo!()
}

// ── NemoClaw workloads ───────────────────────────────────────────────────────

/// NC-1: Bulk-ingest enterprise document nodes and verify count.
#[test]
#[ignore = "Layer 4 stub — not yet implemented"]
fn nc1_bulk_ingest_documents() {
    let (_db, _engine) = open_engine();
    todo!()
}

/// NC-2: Verify FTS search across bulk-ingested documents.
#[test]
#[ignore = "Layer 4 stub — not yet implemented"]
fn nc2_fts_search_bulk_documents() {
    let (_db, _engine) = open_engine();
    todo!()
}

/// NC-3: Excise documents by source_ref and confirm no residual data.
#[test]
#[ignore = "Layer 4 stub — not yet implemented"]
fn nc3_excise_documents_by_source_ref() {
    let (_db, _engine) = open_engine();
    todo!()
}

/// NC-4: Safe export of enterprise data and verify manifest completeness.
#[test]
#[ignore = "Layer 4 stub — not yet implemented"]
fn nc4_safe_export_manifest_completeness() {
    let (_db, _engine) = open_engine();
    todo!()
}

/// NC-5: check_integrity returns clean report after full enterprise workload.
#[test]
#[ignore = "Layer 4 stub — not yet implemented"]
fn nc5_check_integrity_clean_after_enterprise_workload() {
    let (_db, _engine) = open_engine();
    todo!()
}
