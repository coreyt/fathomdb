//! Surface-level assertions for the typed error taxonomy and carrier types
//! pinned by `dev/design/errors.md` and `dev/interfaces/rust.md`.
//!
//! These tests assert variant presence, field shape, and basic typed routing.

use std::error::Error as _;

use fathomdb_embedder_api::EmbedderIdentity;
use fathomdb_engine::{
    CorruptionDetail, CorruptionKind, CorruptionLocator, Engine, EngineError, EngineOpenError,
    InitialState, OpenStage, PreparedWrite, RecoveryHint, SoftFallback, SoftFallbackBranch,
    SourceId,
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
        // 0.8.20 Slice 5b (R-20-E5) — an erasure verb refusing to report success
        // over an erasure it could not complete at rest.
        EngineError::ErasureIncomplete {
            stage: "wal_checkpoint".to_string(),
            detail: "checkpoint reported BUSY".to_string(),
        },
    ];
    for err in &variants {
        assert!(!err.to_string().is_empty(), "Display must be non-empty");
        let _: &dyn std::error::Error = err;
    }
}

/// 0.8.20 Slice 5b (R-20-E5) — the incomplete-erasure refusal must carry the
/// uncompleted STAGE, not just an opaque failure: the remedy differs
/// (`wal_checkpoint` ⇒ retry once the concurrent reader is gone;
/// `telemetry_redaction` ⇒ fix the sink path).
#[test]
fn erasure_incomplete_carries_stage_and_detail() {
    let err = EngineError::ErasureIncomplete {
        stage: "wal_checkpoint".to_string(),
        detail: "reader pinned a WAL snapshot".to_string(),
    };
    let rendered = err.to_string();
    assert!(rendered.contains("wal_checkpoint"), "Display must name the stage: {rendered}");
    assert!(rendered.contains("reader pinned"), "Display must carry the detail: {rendered}");
    match err {
        EngineError::ErasureIncomplete { stage, .. } => assert_eq!(stage, "wal_checkpoint"),
        other => panic!("expected ErasureIncomplete, got {other:?}"),
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
            stored: EmbedderIdentity::new("a", "0", 384),
            supplied: EmbedderIdentity::new("b", "0", 384),
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

// ---------------------------------------------------------------------------
// 0.8.20 Slice 22 (R-20-VC) — decision #18: ONE family at the write-validation
// boundary.
// ---------------------------------------------------------------------------

/// `dev/design/errors.md` defines `WriteValidationError` as "malformed typed
/// write shape" / "the submitted typed write is malformed **before**
/// schema-sensitive payload checks run" — which is exactly what `validate_write`
/// is. Before this slice ONE of its seven rejection sites (the unsatisfiable
/// temporal window) returned `EngineError::InvalidArgument { msg }` instead, so
/// the SAME call could raise `InvalidArgumentError` for an inverted window and
/// `WriteValidationError` for a non-integer bound.
///
/// Decision #18's DoD is literally "one family, tests updated". This pins the
/// family for every write-SHAPE rejection: `EngineError::WriteValidation`, never
/// `InvalidArgument`.
///
/// **The one documented exception**, asserted below so it can never drift
/// silently: `validate_write`'s Edge branch delegates to
/// `reject_unrenderable_edge_epoch`, which still returns
/// `InvalidArgument { msg }` naming the offending field and bound. That message
/// is a required contract from TC-33 fix-1 (a codex §9 finding — an unrenderable
/// epoch renders to `null` on the consolidation wire and silently resurrects an
/// invalidated edge, so the caller must be told WHICH field). Collapsing it onto
/// the message-less `WriteValidation` would destroy the diagnostic, so decision
/// #18 deliberately does not touch it. See the `dev/design/errors.md` 2026-07-28
/// amendment.
#[test]
fn write_validation_boundary_is_exactly_one_error_family() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("one_family.sqlite");
    let opened = Engine::open(&path).expect("open");
    let engine = &opened.engine;

    let src = || SourceId::new("test:decision-18").expect("source id");
    let node = |body: &str, logical: Option<&str>, from: Option<i64>, until: Option<i64>| {
        PreparedWrite::Node {
            kind: "doc".to_string(),
            body: body.to_string(),
            source_id: src(),
            logical_id: logical.map(str::to_string),
            state: InitialState::Active,
            reason: None,
            valid_from: from,
            valid_until: until,
        }
    };

    let rejections: Vec<(&str, Result<fathomdb_engine::WriteReceipt, EngineError>)> = vec![
        ("empty body", engine.write(&[node("   ", None, None, None)])),
        ("empty logical_id", engine.write(&[node("ok", Some(""), None, None)])),
        (
            "record-separator in logical_id",
            engine.write(&[node("ok", Some("a\u{1e}b"), None, None)]),
        ),
        (
            "inverted validity window",
            engine.write(&[node("ok", Some("W1"), Some(2000), Some(1000))]),
        ),
        (
            "empty half-open validity window",
            engine.write(&[node("ok", Some("W2"), Some(1500), Some(1500))]),
        ),
    ];

    for (label, result) in rejections {
        let err = result.expect_err(&format!("{label} must be refused"));
        assert_eq!(
            err,
            EngineError::WriteValidation,
            "decision #18: `{label}` must be WriteValidation, the ONE write-validation \
             family — got {err:?}"
        );
        assert!(
            !matches!(err, EngineError::InvalidArgument { .. }),
            "`{label}` must not use InvalidArgument, which is reserved for caller-argument \
             rejections OUTSIDE the write-validation boundary"
        );
    }

    // THE DOCUMENTED EXCEPTION — an unrenderable edge epoch still raises
    // `InvalidArgument`, and its message still names the field. Pinned here so a
    // later "tidy-up" cannot quietly collapse it and lose the TC-33 fix-1
    // diagnostic without this test going red.
    let unrenderable = engine.write(&[PreparedWrite::Edge {
        kind: "link".to_string(),
        from: "A".to_string(),
        to: "B".to_string(),
        source_id: src(),
        logical_id: Some("E1".to_string()),
        body: None,
        t_valid: Some(i64::MAX),
        t_invalid: None,
        confidence: None,
        extractor_model_id: None,
        temporal_fallback: None,
    }]);
    match unrenderable.expect_err("an unrenderable edge epoch must be refused") {
        EngineError::InvalidArgument { msg } => assert!(
            msg.contains("t_valid"),
            "the retained exception must still NAME the offending field (TC-33 fix-1): {msg}"
        ),
        other => panic!(
            "decision #18 must NOT collapse the edge-epoch guard — its message is the \
             TC-33 fix-1 contract; got {other:?}"
        ),
    }

    engine.close().expect("close");
}

/// `EngineError::InvalidArgument` survives decision #18 — the settlement narrows
/// WHERE it is used, it does not delete the variant. It remains the message-
/// carrying family for caller-argument rejections outside the write-validation
/// boundary (~15 engine sites, plus a live `InvalidArgumentError` /
/// `FDB_INVALID_ARGUMENT` class in both SDKs). This pins that it still exists and
/// still carries an actionable message — the property `WriteValidation`, a unit
/// variant, cannot provide.
#[test]
fn invalid_argument_survives_and_still_carries_a_message() {
    let err = EngineError::InvalidArgument { msg: "depth must be 1-3, got 9".to_string() };
    let rendered = err.to_string();
    assert!(rendered.contains("depth must be 1-3"), "Display must carry the message: {rendered}");
    match err {
        EngineError::InvalidArgument { msg } => assert!(!msg.is_empty()),
        other => panic!("expected InvalidArgument, got {other:?}"),
    }
    // The contrast decision #18 accepts: WriteValidation is message-less.
    assert!(
        !EngineError::WriteValidation.to_string().contains("valid_from"),
        "WriteValidation is a unit variant and carries no caller-supplied detail"
    );
}
