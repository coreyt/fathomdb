//! 0.8.20 Slice 5c (R-20-E3) — compile-fail case: "no provenance" must be
//! INEXPRESSIBLE on the public `PreparedWrite`, not merely rejected at runtime.
//!
//! A validation-only fix would leave a hole: the facade re-exports
//! `PreparedWrite` and `Engine::write` is public, so a caller could construct
//! the struct directly and bypass any check. Making the field a `SourceId`
//! newtype closes that: `None` no longer type-checks.
//!
//! This file MUST NOT COMPILE. It is driven by `tests/compile_fail_provenance.rs`.

use fathomdb::{InitialState, PreparedWrite};

fn main() {
    let _unprovenanced = PreparedWrite::Node {
        kind: "doc".to_string(),
        body: "a body with no provenance".to_string(),
        source_id: None,
        logical_id: None,
        state: InitialState::Active,
        reason: None,
    };
}
