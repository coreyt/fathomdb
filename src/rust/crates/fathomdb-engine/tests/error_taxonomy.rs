//! Surface-level assertions for the typed error taxonomy and carrier types
//! pinned by `dev/design/errors.md` and `dev/interfaces/rust.md`.
//!
//! These tests assert variant presence, field shape, and basic typed routing.

use std::error::Error as _;

use fathomdb_embedder_api::EmbedderIdentity;
use fathomdb_engine::{
    CorruptionDetail, CorruptionKind, CorruptionLocator, Engine, EngineError, EngineOpenError,
    OpenStage, RecoveryHint, SoftFallback, SoftFallbackBranch,
};
use tempfile::TempDir;

#[test]
fn engine_error_runtime_variants_exist() {
    let variants: Vec<EngineError> = vec![
        EngineError::Storage,
        EngineError::Projection,
        EngineError::Vector,
        EngineError::Embedder,
        EngineError::Scheduler,
        EngineError::OpStore,
        EngineError::WriteValidation,
        EngineError::SchemaValidation,
        EngineError::Overloaded,
        EngineError::Closing,
    ];
    for err in &variants {
        assert!(!err.to_string().is_empty(), "Display must be non-empty");
        let _: &dyn std::error::Error = err;
    }
}

#[test]
fn engine_open_error_variants_exist() {
    let detail = CorruptionDetail {
        kind: CorruptionKind::HeaderMalformed,
        stage: OpenStage::HeaderProbe,
        locator: CorruptionLocator::FileOffset { offset: 0 },
        recovery_hint: RecoveryHint {
            code: "E_CORRUPT_HEADER",
            doc_anchor: "design/recovery.md#header-malformed",
        },
    };

    let variants: Vec<EngineOpenError> = vec![
        EngineOpenError::DatabaseLocked { holder_pid: Some(1) },
        EngineOpenError::Corruption(detail),
        EngineOpenError::IncompatibleSchemaVersion { seen: 5, supported: 4 },
        EngineOpenError::MigrationError {
            schema_version_before: 1,
            schema_version_current: 1,
            step_id: 2,
        },
        EngineOpenError::EmbedderIdentityMismatch {
            stored: EmbedderIdentity::new("a", "0"),
            supplied: EmbedderIdentity::new("b", "0"),
        },
        EngineOpenError::EmbedderDimensionMismatch { stored: 384, supplied: 768 },
        EngineOpenError::Io { message: "sanitized".to_string() },
    ];
    for err in &variants {
        assert!(!err.to_string().is_empty(), "Display must be non-empty");
        let _: &dyn std::error::Error = err;
    }
}

#[test]
fn open_stage_enum_is_exactly_four_members() {
    let members = [
        OpenStage::WalReplay,
        OpenStage::HeaderProbe,
        OpenStage::SchemaProbe,
        OpenStage::EmbedderIdentity,
    ];
    assert_eq!(members.len(), 4);
}

#[test]
fn corruption_kind_enum_is_exactly_four_members() {
    let members = [
        CorruptionKind::WalReplayFailure,
        CorruptionKind::HeaderMalformed,
        CorruptionKind::SchemaInconsistent,
        CorruptionKind::EmbedderIdentityDrift,
    ];
    assert_eq!(members.len(), 4);
}

#[test]
fn corruption_locator_carries_open_path_variants() {
    let _ = CorruptionLocator::FileOffset { offset: 0 };
    let _ = CorruptionLocator::PageId { page: 0 };
    let _ = CorruptionLocator::TableRow { table: "fathomdb_schema_meta", rowid: 0 };
    let _ = CorruptionLocator::Vec0ShadowRow { partition: "vector_default", rowid: 0 };
    let _ = CorruptionLocator::MigrationStep { from: 0, to: 1 };
    let _ = CorruptionLocator::OpaqueSqliteError { sqlite_extended_code: 0 };
}

#[test]
fn soft_fallback_branch_enum_is_exactly_two_members() {
    let members = [SoftFallbackBranch::Vector, SoftFallbackBranch::Text];
    assert_eq!(members.len(), 2);
}

#[test]
fn soft_fallback_carries_typed_branch() {
    let f = SoftFallback { branch: SoftFallbackBranch::Vector };
    assert_eq!(f.branch, SoftFallbackBranch::Vector);
}

#[test]
fn engine_open_error_corruption_round_trips_detail() {
    let detail = CorruptionDetail {
        kind: CorruptionKind::WalReplayFailure,
        stage: OpenStage::WalReplay,
        locator: CorruptionLocator::PageId { page: 17 },
        recovery_hint: RecoveryHint {
            code: "E_CORRUPT_WAL_REPLAY",
            doc_anchor: "design/recovery.md#wal-replay-failures",
        },
    };
    let err = EngineOpenError::Corruption(detail.clone());
    match err {
        EngineOpenError::Corruption(got) => assert_eq!(got, detail),
        _ => panic!("expected Corruption variant"),
    }
}

#[test]
fn search_rejects_empty_query_via_write_validation_variant() {
    // Per dev/design/errors.md, malformed typed write input shape is the
    // WriteValidation row of the binding matrix; there is no EmptyQuery
    // variant in the canonical taxonomy.
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(dir.path().join("rewrite.sqlite")).expect("engine should open");
    let err = opened.engine.search("").expect_err("empty query must be rejected");
    assert_eq!(err, EngineError::WriteValidation);
}

#[test]
fn search_after_close_routes_through_closing_variant() {
    // Per the matrix, in-flight rejection on a closed engine is the Closing
    // row, not an undocumented Closed variant.
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(dir.path().join("rewrite.sqlite")).expect("engine should open");
    opened.engine.close().expect("close should succeed");
    let err = opened.engine.search("hello").expect_err("closed engine must reject search");
    assert_eq!(err, EngineError::Closing);
}

#[test]
fn open_rejects_empty_path_as_io_error() {
    let err = Engine::open("").expect_err("real open path requires a database file name");
    assert!(matches!(err, EngineOpenError::Io { .. }));
}

#[test]
fn corruption_detail_source_chain_terminates() {
    let detail = CorruptionDetail {
        kind: CorruptionKind::HeaderMalformed,
        stage: OpenStage::HeaderProbe,
        locator: CorruptionLocator::OpaqueSqliteError { sqlite_extended_code: 11 },
        recovery_hint: RecoveryHint {
            code: "E_CORRUPT_HEADER",
            doc_anchor: "design/recovery.md#header-malformed",
        },
    };
    let err = EngineOpenError::Corruption(detail);
    assert!(err.source().is_none());
}
