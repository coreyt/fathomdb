//! **FathomDB** — a local-first retrieval and graph-oriented data system for
//! application and agent workloads.
//!
//! This is the crate to depend on from Rust. It is a thin facade that
//! re-exports the public surface of the `fathomdb-engine` runtime, so you get
//! the supported API without depending on engine internals.
//!
//! FathomDB embeds SQLite (FTS5 + `sqlite-vec`) in your process — there is no
//! server and no sidecar. One `Engine` owns the writer thread, a reader pool,
//! the projection scheduler and (optionally) an in-process embedder.
//!
//! # Do you want this crate?
//!
//! - **Yes**, if you want hybrid retrieval — a vector branch and an FTS5 branch
//!   fused by Reciprocal Rank Fusion — over data you also want to address by a
//!   stable identity, traverse as a graph, and be able to *delete on request*.
//! - **No**, if you want a client for a remote database, or an approximate
//!   nearest-neighbour index at very large scale: vector retrieval here is a
//!   full scan, so latency grows with corpus size.
//!
//! Related crates: `fathomdb-cli` (the operator binary — `doctor` / `recover`),
//! `fathomdb-embedder-api` (the semver-stable embedder trait, versioned
//! independently), and `fathomdb-engine` (the runtime this crate re-exports;
//! prefer this facade).
//!
//! # Example
//!
//! ```no_run
//! use fathomdb::{Engine, PreparedWrite, SourceId};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let opened = Engine::open("./example.fdb")?;
//! let engine = opened.engine;
//!
//! engine.write(&[PreparedWrite::Node {
//!     kind: "note".into(),
//!     body: "the sky is blue".into(),
//!     // Provenance is MANDATORY — see below.
//!     source_id: SourceId::new("doc-42")?,
//!     logical_id: Some("note:sky".into()),
//!     state: Default::default(),
//!     reason: None,
//!     valid_from: None,
//!     valid_until: None,
//! }])?;
//!
//! for hit in engine.search("sky")?.results {
//!     // `hit.id` is a typed id-space carrier, not a row number.
//!     println!("{:?} {} {}", hit.id.space, hit.id.value, hit.body);
//! }
//!
//! engine.close()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Provenance is mandatory
//!
//! Every canonical node and edge carries a [`SourceId`]. This is a *type*
//! rather than a validation check on purpose: [`Engine::erase_source`] addresses
//! rows **by** their `source_id`, so a row written without one could never be
//! erased on request. [`SourceId::new`] is the only public constructor and
//! refuses an empty id and the engine's reserved `_`-prefixed namespace, which
//! makes an un-provenanced write a **compile error** rather than a runtime
//! surprise.
//!
//! Treat a `source_id` as a public identifier: it is echoed on every search hit
//! and recorded in a retention-exempt erasure-audit row, so keep personal data
//! out of it.
//!
//! # Deletion on request
//!
//! Three verbs, differing in what they address, all on the default surface:
//!
//! - [`Engine::transition`] — move a governed node between existence states
//!   (promote, soft-delete, undelete).
//! - [`Engine::purge`] — irreversibly hard-erase one governed node, addressed by
//!   its `logical_id`. Deleted-first and idempotent. There is no restore.
//! - [`Engine::erase_source`] — erase every row carrying one `source_id`,
//!   including *anonymous* rows that have no `logical_id` and that
//!   [`Engine::purge`] therefore cannot reach.
//!
//! # Feature flags
//!
//! - **default** — the *governed application surface*
//!   (`dev/interfaces/rust.md` § Governed-surface contract): recovery-name-free
//!   and raw-SQL-free at the **method** level. No method named `recover`,
//!   `restore`, `repair`, `fix` or `rebuild` resolves.
//! - **`operator`** — un-gates the operator/recovery seam (`rebuild_*`,
//!   `excise_source`, `dump_*`, `trace_source_ref`, `truncate_wal`,
//!   `verify_embedder`, `check_integrity`, `safe_export`, `recompute_mean` and
//!   their report types). `fathomdb-cli` enables it. Gating, not deletion:
//!   engine behaviour is identical with the feature on. See AC-074
//!   (`dev/acceptance.md`) and
//!   `dev/design/slice-27-fix1-operator-gate-design.md`.
//!
//! # Stability
//!
//! Pre-1.0, so **beta**: the surface may change between micro releases. The
//! governed surface is pinned by
//! `src/conformance/governed-surface-allowlist.json` and any change to it is a
//! reviewed delta, but that is a change-control promise, not a semver one.
//! [`PreparedWrite`] and [`SearchFilter`] are `#[non_exhaustive]`.

// The 26 governed application-surface types (`dev/interfaces/rust.md` § 2a) —
// always present on the default facade.
//
// 17 original types + 7 new types from Slices 20 (G5/G6) and 35 (G4):
//   Slice 20: ComparisonOp, NodeRecord, Predicate, ScalarValue, SearchExpandResult,
//             SearchFilter, TraversalDirection
// + 2 new types from Slice 15 (G11 BYO-LLM ingest — fix-29):
//   ExtractDocument, IngestWithExtractorReceipt
// + 3 new types from 0.8.8 Slice 5 (EXP-OBS explain sidecar):
//   Explanation, QueryTrace, PerHitExplain
// + 1 new type from 0.8.20 Slice 5c (R-20-E3, erasure): SourceId — the
//   provenance newtype that replaced `source_id: Option<String>` on
//   `PreparedWrite`. It MUST be re-exported here: this crate re-exports
//   `PreparedWrite` and `Engine::write` is public, so without the constructor a
//   facade consumer could not perform a canonical write at all. Its presence is
//   also what makes the un-provenanced write a COMPILE error for facade
//   consumers rather than a runtime rejection (see `tests/ui/`).
// + 1 new type from 0.8.20 Slice 5d (R-20-E4, erasure): ExciseReport — the
//   outcome of the net-new governed `Engine::erase_source` lifecycle verb. It
//   MOVED here from the operator-gated block below: `erase_source` is governed
//   surface (an SDK-only consumer must be able to erase anonymous content it
//   wrote), so its return type cannot stay behind the CLI feature gate. The
//   operator build is unaffected — it now gets the type from this block instead.
// + 2 new types from 0.8.20 Slice 10b (R-20-RV / R-20-NV): `ReadView` — the
//   read-mode / validity selector every one of the five read verbs now takes —
//   and `BoundaryCrossing`, the return shape of `Engine::crossed_boundary_since`.
//   Threading `ReadView` as a parameter (rather than shipping five `*_with_view`
//   sibling verbs) is what keeps this delta at TWO TYPES and ZERO new verbs.
//   HITL-SIGNED 2026-07-29 (steward seq-157) — recorded in
//   `src/conformance/governed-surface-allowlist.json`.
// + 1 new type from 0.8.20 Slice 20 / 0.8.22 Slice 21 F5 (R-20-DR):
//   `DenseReadiness` — the `{unavailable, embedding, ready}` READ-METADATA
//   flag the engine hangs off `ProjectionSpec.vector` (`read.projections`
//   populates it; it is never a caller declaration). ZERO net-new commands;
//   the vocabulary was HITL-signed in steward-ledger seq-246.
pub use fathomdb_engine::{
    BoundaryCrossing, ComparisonOp, CorruptionDetail, CorruptionKind, CorruptionLocator,
    CounterSnapshot, DenseReadiness, Engine, EngineError, EngineOpenError, ExciseReport,
    Explanation, ExtractDocument, IngestWithExtractorReceipt, InitialState, LifecycleState,
    NodeRecord, OpenReport, OpenStage, OpenedEngine, PerHitExplain, Predicate, PreparedWrite,
    ProjectionDelta, ProjectionFts, ProjectionRole, ProjectionSpec, ProjectionVector, QueryTrace,
    ReadView, RecoveryHint, ScalarValue, SearchExpandResult, SearchFilter, SearchResult,
    SoftFallback, SoftFallbackBranch, SourceId, Subscription, TraversalDirection, WriteReceipt,
};

// The operator-seam report types (`dev/interfaces/rust.md` § 2b) — CLI-only,
// gated behind `operator`. The backing `Engine` methods are operator-gated in
// `fathomdb-engine`, so the default facade is recovery-clean at the method level.
//
// 0.8.20 Slice 5d keeps the count at 20 via two offsetting moves:
//   - `ExciseReport` moved OUT (up to the always-present block): it is the
//     return type of the governed `Engine::erase_source`, so it cannot sit
//     behind the CLI feature gate.
//   - `OrphanProvenanceReport` + `OrphanProvenanceSource` moved IN: the
//     `doctor orphan-provenance` diagnostic is CLI-only with no SDK parity,
//     the same posture as `dump-mutations` (Slice 34).
#[cfg(feature = "operator")]
pub use fathomdb_engine::{
    CheckIntegrityOpts, DumpProfileReport, DumpRowCountsReport, DumpSchemaReport,
    ExciseRecordReport, Finding, IntegrityReport, MeanRecomputeReport, OrphanProvenanceReport,
    OrphanProvenanceSource, RebuildKind, RebuildReport, SafeExportArtifact, SchemaObject, Section,
    TableRowCount, TraceEvent, TraceReport, TruncateWalReport, TruncateWalStatus,
    VerifyEmbedderReport, VerifyEmbedderStatus,
};

/// AC-074 method-level pin (Q5=BIND-RUST, Slice 27 fix-1): in a **default**
/// (operator-OFF) build the governed `fathomdb::Engine` exposes **no**
/// recovery-denylist-named method. Rust has no runtime method reflection, so the
/// guarantee is pinned by `compile_fail` doctests (the only mechanism that can
/// assert a method does *not* resolve). This module is
/// `#[cfg(not(feature = "operator"))]`, so feature-unified `--workspace` builds
/// (which turn `operator` ON via `fathomdb-cli`) correctly skip it.
///
/// `rebuild_projections` does not resolve on the default facade:
/// ```compile_fail
/// fn _no_rebuild_projections(e: &fathomdb::Engine) {
///     let _ = e.rebuild_projections();
/// }
/// ```
///
/// `rebuild_vec0` does not resolve on the default facade:
/// ```compile_fail
/// fn _no_rebuild_vec0(e: &fathomdb::Engine) {
///     let _ = e.rebuild_vec0();
/// }
/// ```
///
/// `excise_source` (operator seam) does not resolve on the default facade:
/// ```compile_fail
/// fn _no_excise(e: &fathomdb::Engine) {
///     let _ = e.excise_source("s");
/// }
/// ```
#[cfg(not(feature = "operator"))]
#[doc(hidden)]
pub mod governed_surface_method_absence_proof {}

/// AC-074 no-raw-SQL release-surface pin: the shipped (release) facade exposes
/// no raw-SQL method. The **canonical** guarantee is the engine's
/// `Engine::execute_for_test` gate — `#[cfg(debug_assertions)] #[doc(hidden)]`,
/// so the symbol is absent from release builds (verified: it is not present in
/// `target/release/libfathomdb_engine.rlib`). This `#[cfg(not(debug_assertions))]`
/// module is the release-surface pin in the best-effort `no_recovery_surface.rs`
/// style; it is compiled out of debug builds, and a true no-debug-assertions doc
/// build runs the `compile_fail` below. (Plain `cargo test --release` may not
/// re-run rustdoc with debug-assertions off, so this is a documented pin backed
/// by the engine cfg-gate, not a load-bearing CI assertion.)
///
/// `execute_for_test` (raw SQL) does not resolve in a release build:
/// ```compile_fail
/// fn _no_raw_sql(e: &fathomdb::Engine) {
///     let _ = e.execute_for_test("SELECT 1");
/// }
/// ```
#[cfg(not(debug_assertions))]
#[doc(hidden)]
pub mod release_surface_raw_sql_absence_proof {}
