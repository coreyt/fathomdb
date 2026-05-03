//! AC-041 (Rust-facade best-effort half) — pin the intent that the
//! `fathomdb` crate re-exports zero recovery verbs into the runtime SDK
//! surface.
//!
//! Rust has no runtime symbol-table introspection for a crate (no
//! `dir(module)` equivalent), and `compile_fail` doctests only run for
//! items declared in `src/`, not for items under `tests/`. So this file
//! is a best-effort pin:
//!
//! - It positively asserts the canonical typed surface (`Engine`,
//!   `OpenedEngine`, `WriteReceipt`, `SearchResult`, `EngineError`,
//!   `EngineOpenError`) resolves through the facade. That guards against
//!   accidental removal of the typed surface; if someone replaces the
//!   facade with junk, this test breaks.
//! - The load-bearing AC-041 enforcement lives in the bindings:
//!   `src/python/tests/test_no_recovery_surface.py` and
//!   `src/ts/tests/no-recovery-surface.test.ts` enumerate the public
//!   surface at runtime and assert empty intersection with
//!   `{recover, restore, repair, fix, rebuild}`.
//!
//! Per `dev/interfaces/rust.md` § Recovery posture: "The Rust runtime
//! surface does not expose recovery verbs." Reviewers must reject any
//! `pub use fathomdb_engine::{recover, restore, ...}` addition to the
//! facade by source inspection; this file documents the contract.

#[test]
fn t_041_rust_facade_canonical_surface_resolves() {
    let _ = std::any::type_name::<fathomdb::Engine>();
    let _ = std::any::type_name::<fathomdb::OpenedEngine>();
    let _ = std::any::type_name::<fathomdb::WriteReceipt>();
    let _ = std::any::type_name::<fathomdb::SearchResult>();
    let _ = std::any::type_name::<fathomdb::EngineError>();
    let _ = std::any::type_name::<fathomdb::EngineOpenError>();
}
