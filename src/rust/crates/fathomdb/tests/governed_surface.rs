//! AC-074 (Rust-facade measurement, Q5 = BIND-RUST) — the Rust-facade
//! governed-surface positive-allowlist / parity pin.
//!
//! Binds **AC-074** (`dev/acceptance.md`, REQ-053), NOT a new AC id. Under the
//! signed `ADR-0.8.0-supersede-five-verb-surface-cap` (Q5 = BIND-RUST) AC-074
//! *also* binds the Rust facade; Slice 25 landed the Python + TypeScript half
//! and this reserved-gap Slice 27 lands the Rust half. See
//! `dev/design/slice-27-rust-allowlist-design.md` and
//! `dev/interfaces/rust.md` § Governed-surface contract (the owner of the set).
//!
//! The Rust facade is a **different consumer contract** than the Py/TS 5-verb +
//! `read.*` SDK: its application verbs are methods on `Engine`, and its public
//! surface is a set of re-exported *types*, not free verbs. Rust also has no
//! runtime symbol-table introspection (no `dir(module)`; `compile_fail`
//! doctests only run for `src/` items, not `tests/`). So — exactly like
//! `reexports.rs` / `no_recovery_surface.rs` — this pin is a **compile-time
//! resolves-check** (`type_name::<…>()` over the allowlisted types) plus a
//! source-inspection-documented contract.
//!
//! Three load-bearing properties (mirroring the Py+TS governed surface, adapted
//! to Rust):
//!
//! - **P1 positive allowlist** — every member of `GOVERNED_SURFACE_ALLOWLIST`
//!   (the curated governed application surface `dev/interfaces/rust.md` owns)
//!   resolves through the `fathomdb` facade.
//! - **P2 parity-in-intent** — posture-consistent with the Py/TS governed
//!   surface (governed allowlist, recovery-denylist-absent, typed/no-raw-SQL),
//!   but NOT membership-identical (a type set, not a verb set). The one shared
//!   element is the recovery denylist, declared once in
//!   `src/conformance/governed-surface-allowlist.json` (`recovery_denylist`);
//!   `RECOVERY_DENYLIST` below pins the same five names.
//! - **P3 denylist-absence** — no governed-surface symbol *is* a recovery verb
//!   in `{recover, restore, repair, fix, rebuild}` (exact, case-insensitive —
//!   NOT substring: the denylist targets recovery *verbs*, while `RecoveryHint`
//!   is a typed open-error hint). The CANONICAL denylist enforcement remains the
//!   byte-frozen `no_recovery_surface.rs`; this file adds the *positive*
//!   allowlist half + an allowlist-scope denylist check.
//!
//! **Slice 27 fix-1 (method level).** The codex [P1] found the type-only audit
//! missed the inherent **methods** on the re-exported `fathomdb::Engine`
//! (`rebuild_projections`/`rebuild_vec0` carried a recovery-denylist name;
//! `execute_for_test` was raw SQL). Per HITL Option B the operator/recovery seam
//! is now feature-gated (`operator`), so the **default** facade `Engine` exposes
//! no recovery-name method and no raw-SQL method. Method **absence** is pinned by
//! the `compile_fail` doctests in `fathomdb/src/lib.rs`
//! (`governed_surface_method_absence_proof` / `release_surface_raw_sql_absence_proof`)
//! — the only mechanism that can assert a method does *not* resolve. This file's
//! `t_074_operator_seam_resolves_with_feature` is the positive counterpart:
//! the seam DOES resolve with `--features operator` (the CLI substrate).

use std::any::type_name;

/// The curated governed **application surface** the `fathomdb` facade owns
/// (`dev/interfaces/rust.md`). The 20 recovery/integrity/dump operator-seam
/// report types the facade also re-exports for `fathomdb-cli` are deliberately
/// NOT here (they are CLI-only ergonomic symbols, not runtime SDK surface — the
/// Rust analogue of "recovery is CLI-only, not an SDK verb").
///
/// 17 original + 7 added by Slices 20 (G5/G6) + 35 (G4):
///   Slice 20/35: ComparisonOp, NodeRecord, Predicate, ScalarValue,
///                SearchExpandResult, SearchFilter, TraversalDirection
const GOVERNED_SURFACE_ALLOWLIST: &[&str] = &[
    "Engine",
    "OpenedEngine",
    "OpenReport",
    "WriteReceipt",
    "SearchResult",
    "PreparedWrite",
    "EngineError",
    "EngineOpenError",
    "CorruptionDetail",
    "CorruptionKind",
    "CorruptionLocator",
    "OpenStage",
    "RecoveryHint",
    "SoftFallback",
    "SoftFallbackBranch",
    "CounterSnapshot",
    "Subscription",
    // Slice 20 (G5/G6) — graph traversal types
    "TraversalDirection",
    "NodeRecord",
    "SearchExpandResult",
    "SearchFilter",
    // Slice 35 (G4) — filter grammar types
    "Predicate",
    "ScalarValue",
    "ComparisonOp",
    // Slice 15 (G11) — BYO-LLM ingest types (fix-29)
    "ExtractDocument",
    "IngestWithExtractorReceipt",
    // 0.8.8 Slice 5 (EXP-OBS) — explain sidecar types
    "Explanation",
    "QueryTrace",
    "PerHitExplain",
    // 0.8.20 Slice 5c/5d (R-20-E3/E4, erasure) — AWAITING HITL SIGN-OFF (AC-079).
    // `SourceId` is the provenance newtype that replaced `source_id:
    // Option<String>` on `PreparedWrite`; it MUST be here because without the
    // constructor a facade consumer cannot perform a canonical write at all.
    // `ExciseReport` is the return type of the net-new governed
    // `Engine::erase_source`, so it moved out of the operator-gated block.
    "SourceId",
    "ExciseReport",
];

/// The permanent five-name recovery denylist. Identical to the single shared
/// contract `src/conformance/governed-surface-allowlist.json` (`recovery_denylist`)
/// that the Python + TypeScript suites read — pinned here verbatim because Rust
/// is a different consumer contract with no runtime introspection (P2).
const RECOVERY_DENYLIST: &[&str] = &["recover", "restore", "repair", "fix", "rebuild"];

/// Names in `allowlist` that ARE a recovery verb (exact, case-insensitive).
/// Exact — not substring — so `RecoveryHint` (a typed open-error hint, not a
/// verb) is correctly NOT flagged, mirroring the Py/TS P3 set-intersection.
fn denylist_hits(allowlist: &[&str], denylist: &[&str]) -> Vec<String> {
    allowlist
        .iter()
        .filter(|name| denylist.iter().any(|verb| name.eq_ignore_ascii_case(verb)))
        .map(|name| name.to_string())
        .collect()
}

/// P1 — every governed-surface allowlist member resolves through the facade.
/// The explicit `type_name` calls are the compile-time resolves proof; the
/// length assertion keeps the const and the resolves-checks in lock-step.
#[test]
fn t_074_rust_governed_surface_resolves() {
    let _ = type_name::<fathomdb::Engine>();
    let _ = type_name::<fathomdb::OpenedEngine>();
    let _ = type_name::<fathomdb::OpenReport>();
    let _ = type_name::<fathomdb::WriteReceipt>();
    let _ = type_name::<fathomdb::SearchResult>();
    let _ = type_name::<fathomdb::PreparedWrite>();
    let _ = type_name::<fathomdb::EngineError>();
    let _ = type_name::<fathomdb::EngineOpenError>();
    let _ = type_name::<fathomdb::CorruptionDetail>();
    let _ = type_name::<fathomdb::CorruptionKind>();
    let _ = type_name::<fathomdb::CorruptionLocator>();
    let _ = type_name::<fathomdb::OpenStage>();
    let _ = type_name::<fathomdb::RecoveryHint>();
    let _ = type_name::<fathomdb::SoftFallback>();
    let _ = type_name::<fathomdb::SoftFallbackBranch>();
    let _ = type_name::<fathomdb::CounterSnapshot>();
    let _ = type_name::<fathomdb::Subscription>();
    // Slice 20 (G5/G6)
    let _ = type_name::<fathomdb::TraversalDirection>();
    let _ = type_name::<fathomdb::NodeRecord>();
    let _ = type_name::<fathomdb::SearchExpandResult>();
    let _ = type_name::<fathomdb::SearchFilter>();
    // Slice 35 (G4)
    let _ = type_name::<fathomdb::Predicate>();
    let _ = type_name::<fathomdb::ScalarValue>();
    let _ = type_name::<fathomdb::ComparisonOp>();
    // Slice 15 (G11) — BYO-LLM ingest types (fix-29)
    let _ = type_name::<fathomdb::ExtractDocument>();
    let _ = type_name::<fathomdb::IngestWithExtractorReceipt>();
    // 0.8.8 Slice 5 (EXP-OBS) — explain sidecar types
    let _ = type_name::<fathomdb::Explanation>();
    let _ = type_name::<fathomdb::QueryTrace>();
    let _ = type_name::<fathomdb::PerHitExplain>();
    // 0.8.20 Slice 5c/5d (R-20-E3/E4) — erasure types
    let _ = type_name::<fathomdb::SourceId>();
    let _ = type_name::<fathomdb::ExciseReport>();

    assert_eq!(
        GOVERNED_SURFACE_ALLOWLIST.len(),
        31,
        "GOVERNED_SURFACE_ALLOWLIST must list exactly the 31 resolved governed types"
    );
}

/// P3 — the governed surface contains no recovery verb (allowlist-scope; the
/// byte-frozen `no_recovery_surface.rs` is the canonical denylist pin).
#[test]
fn t_074_recovery_denylist_absent_from_governed_surface() {
    let hits = denylist_hits(GOVERNED_SURFACE_ALLOWLIST, RECOVERY_DENYLIST);
    assert!(
        hits.is_empty(),
        "governed Rust surface must not contain recovery-denylist verbs, found: {hits:?}"
    );
}

/// Non-vacuous guard (Slice-25 vacuous-green lesson): prove the P3 detector
/// actually bites — a poisoned allowlist containing a denylist verb MUST be
/// flagged, and a clean one MUST NOT. Guards against the check passing vacuously.
#[test]
fn t_074_denylist_detector_is_not_vacuous() {
    assert_eq!(
        denylist_hits(&["Engine", "rebuild"], RECOVERY_DENYLIST),
        vec!["rebuild".to_string()],
        "the denylist detector must flag an injected recovery verb"
    );
    assert!(
        denylist_hits(&["Engine", "WriteReceipt", "RecoveryHint"], RECOVERY_DENYLIST).is_empty(),
        "the denylist detector must NOT flag typed names like RecoveryHint (exact-match, not substring)"
    );
}

/// Slice 27 fix-1 positive counterpart: WITH `--features operator` the operator
/// seam (20 report types + the operator/recovery methods) resolves through the
/// facade — proving the gate hides the seam from the *default* build without
/// deleting it (the `fathomdb-cli` substrate). The default-build **absence** of
/// these methods is pinned by the `compile_fail` doctests in `src/lib.rs`.
#[cfg(feature = "operator")]
#[test]
fn t_074_operator_seam_resolves_with_feature() {
    // The 20 operator-seam re-export TYPES resolve.
    let _ = type_name::<fathomdb::CheckIntegrityOpts>();
    let _ = type_name::<fathomdb::IntegrityReport>();
    let _ = type_name::<fathomdb::SafeExportArtifact>();
    let _ = type_name::<fathomdb::TraceReport>();
    let _ = type_name::<fathomdb::TraceEvent>();
    let _ = type_name::<fathomdb::RebuildReport>();
    let _ = type_name::<fathomdb::RebuildKind>();
    let _ = type_name::<fathomdb::ExciseReport>();
    let _ = type_name::<fathomdb::VerifyEmbedderReport>();
    let _ = type_name::<fathomdb::VerifyEmbedderStatus>();
    let _ = type_name::<fathomdb::DumpSchemaReport>();
    let _ = type_name::<fathomdb::SchemaObject>();
    let _ = type_name::<fathomdb::DumpRowCountsReport>();
    let _ = type_name::<fathomdb::TableRowCount>();
    let _ = type_name::<fathomdb::DumpProfileReport>();
    let _ = type_name::<fathomdb::TruncateWalReport>();
    let _ = type_name::<fathomdb::TruncateWalStatus>();
    let _ = type_name::<fathomdb::Finding>();
    let _ = type_name::<fathomdb::MeanRecomputeReport>();
    let _ = type_name::<fathomdb::Section>();
    // 0.8.20 Slice 5d (R-20-E8) — `doctor orphan-provenance` report types.
    // CLI-only, no SDK parity (same posture as `dump-mutations`).
    let _ = type_name::<fathomdb::OrphanProvenanceReport>();
    let _ = type_name::<fathomdb::OrphanProvenanceSource>();

    // The operator/recovery METHODS resolve (as fn-item paths; not called).
    let _ = fathomdb::Engine::rebuild_projections;
    let _ = fathomdb::Engine::rebuild_vec0;
    let _ = fathomdb::Engine::excise_source;
    let _ = fathomdb::Engine::orphan_provenance;
    let _ = fathomdb::Engine::check_integrity;
    let _ = fathomdb::Engine::safe_export;
    let _ = fathomdb::Engine::trace_source_ref;
    let _ = fathomdb::Engine::verify_embedder;
    let _ = fathomdb::Engine::dump_schema;
    let _ = fathomdb::Engine::dump_row_counts;
    let _ = fathomdb::Engine::dump_profile;
    let _ = fathomdb::Engine::truncate_wal;
    let _ = fathomdb::Engine::recompute_mean;
}
