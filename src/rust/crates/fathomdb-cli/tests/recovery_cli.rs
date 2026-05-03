//! Phase-9-deferred operator CLI scaffolds.
//!
//! These tests pin the existence of test slots for ACs that depend on
//! engine-side behavior landing in Phase 9 (projection runtime, vec0 rebuild,
//! source excision, op-store/registry lifecycle) and on artifact-producing
//! doctor verbs that need a seeded engine. They are `#[ignore]` with an
//! explicit `dep-Phase-9-*` reason so `agent-verify.sh` stays green; Phase 11+
//! flips them to live as the engine surfaces wire up.
//!
//! Grouped in this single file (recovery + doctor skeletons together) to
//! avoid splitting one-AC-per-file fan-out — the briefing explicitly allows
//! either `recovery_cli.rs` alone or a sibling `doctor_cli.rs`. They share
//! the same Phase-9 dependency posture, so they live together.
//!
//! Bound (skeleton-only) ACs:
//!
//! - AC-026: `doctor safe-export` covers WAL-only commits.
//! - AC-027a/b/c/d: recovery preserves canonical rows / FTS query result
//!   equality / vector profile metadata bit-equal / vector top-k rank
//!   correlation.
//! - AC-028a/b/c: `excise_source` audit row / projection-residue removal /
//!   non-excised projection isolation.
//! - AC-039a/b: `doctor safe-export` SHA-256 manifest + tampered-artifact
//!   detection.
//! - AC-042: `doctor trace --source-ref` blast-radius enumeration exact.
//! - AC-043a/b/c: `check-integrity` structured report shape (three sections /
//!   per-section findings list / `--full` finding fields).
//! - AC-044: physical recovery rebuilds projections from canonical state.
//! - AC-063c: `recover --rebuild-projections` performs the regenerate
//!   workflow.

#[test]
#[ignore = "AC-026: dep-Phase-9-wal-only-export"]
fn t_026_safe_export_covers_wal_only_commits() {
    let _bin = env!("CARGO_BIN_EXE_fathomdb");
    unimplemented!("AC-026 behavior lands after Phase 9 wires WAL-only safe-export");
}

#[test]
#[ignore = "AC-027a: dep-Phase-9-recovery-canonical-rows"]
fn t_027a_recovery_preserves_canonical_rows() {
    let _bin = env!("CARGO_BIN_EXE_fathomdb");
    unimplemented!("AC-027a behavior lands after Phase 9 wires shadow-corruption recovery");
}

#[test]
#[ignore = "AC-027b: dep-Phase-9-fts-result-equality"]
fn t_027b_recovery_preserves_fts_query_result_equality() {
    let _bin = env!("CARGO_BIN_EXE_fathomdb");
    unimplemented!("AC-027b behavior lands after Phase 9 wires FTS rebuild from canonical state");
}

#[test]
#[ignore = "AC-027c: dep-Phase-9-vector-profile-bit-equal"]
fn t_027c_recovery_preserves_vector_profile_metadata_bit_equal() {
    let _bin = env!("CARGO_BIN_EXE_fathomdb");
    unimplemented!("AC-027c behavior lands after Phase 9 wires vector profile metadata recovery");
}

#[test]
#[ignore = "AC-027d: dep-Phase-9-vector-top-k-rank-correlation"]
fn t_027d_recovery_preserves_vector_top_k_rank_correlation() {
    let _bin = env!("CARGO_BIN_EXE_fathomdb");
    unimplemented!(
        "AC-027d behavior lands after Phase 9 wires vec0 rebuild + rank-correlation harness"
    );
}

#[test]
#[ignore = "AC-028a: dep-Phase-9-source-excision-audit"]
fn t_028a_excise_source_writes_audit_row() {
    let _bin = env!("CARGO_BIN_EXE_fathomdb");
    unimplemented!("AC-028a behavior lands after Phase 9 wires excise_source audit logging");
}

#[test]
#[ignore = "AC-028b: dep-Phase-9-source-excision-projection-residue"]
fn t_028b_excise_source_removes_residue_from_projections() {
    let _bin = env!("CARGO_BIN_EXE_fathomdb");
    unimplemented!(
        "AC-028b behavior lands after Phase 9 wires source excision through projection runtime"
    );
}

#[test]
#[ignore = "AC-028c: dep-Phase-9-source-excision-isolation"]
fn t_028c_excise_source_does_not_perturb_non_excised_projections() {
    let _bin = env!("CARGO_BIN_EXE_fathomdb");
    unimplemented!(
        "AC-028c behavior lands after Phase 9 wires source excision projection isolation"
    );
}

#[test]
#[ignore = "AC-039a: dep-Phase-9-safe-export-manifest"]
fn t_039a_safe_export_artifact_ships_sha256_manifest() {
    let _bin = env!("CARGO_BIN_EXE_fathomdb");
    unimplemented!(
        "AC-039a behavior lands after Phase 9 wires safe-export artifact + SHA-256 manifest"
    );
}

#[test]
#[ignore = "AC-039b: dep-Phase-9-safe-export-tamper-detection"]
fn t_039b_safe_export_tampered_artifact_detected() {
    let _bin = env!("CARGO_BIN_EXE_fathomdb");
    unimplemented!(
        "AC-039b behavior lands after Phase 9 wires safe-export verifier + manifest check"
    );
}

#[test]
#[ignore = "AC-042: dep-Phase-9-source-ref-blast-radius"]
fn t_042_source_ref_blast_radius_enumeration_exact() {
    let _bin = env!("CARGO_BIN_EXE_fathomdb");
    unimplemented!(
        "AC-042 behavior lands after Phase 9 wires source-ref trace through canonical rows"
    );
}

#[test]
#[ignore = "AC-043a: dep-Phase-9-check-integrity-report-shape"]
fn t_043a_check_integrity_produces_structured_report_with_three_sections() {
    let _bin = env!("CARGO_BIN_EXE_fathomdb");
    unimplemented!(
        "AC-043a behavior lands after Phase 9 wires check-integrity physical/logical/semantic sections"
    );
}

#[test]
#[ignore = "AC-043b: dep-Phase-9-check-integrity-section-findings"]
fn t_043b_check_integrity_populates_each_section() {
    let _bin = env!("CARGO_BIN_EXE_fathomdb");
    unimplemented!(
        "AC-043b behavior lands after Phase 9 wires per-section finding lists / clean markers"
    );
}

#[test]
#[ignore = "AC-043c: dep-Phase-9-check-integrity-full-finding-fields"]
fn t_043c_check_integrity_full_findings_carry_stable_report_fields() {
    let _bin = env!("CARGO_BIN_EXE_fathomdb");
    unimplemented!("AC-043c behavior lands after Phase 9 wires --full finding payload schema");
}

#[test]
#[ignore = "AC-044: dep-Phase-9-physical-recovery-projection-rebuild"]
fn t_044_physical_recovery_rebuilds_projections_from_canonical_state() {
    let _bin = env!("CARGO_BIN_EXE_fathomdb");
    unimplemented!(
        "AC-044 behavior lands after Phase 9 wires physical recovery + projection rebuild"
    );
}

#[test]
#[ignore = "AC-063c: dep-Phase-9-rebuild-projections-regenerate"]
fn t_063c_recover_rebuild_projections_performs_regenerate_workflow() {
    let _bin = env!("CARGO_BIN_EXE_fathomdb");
    unimplemented!(
        "AC-063c behavior lands after Phase 9 wires --rebuild-projections regenerate workflow"
    );
}
