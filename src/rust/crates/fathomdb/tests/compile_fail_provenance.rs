//! 0.8.20 Slice 5c (R-20-E3) — provenance is STRUCTURALLY mandatory on the
//! public write type.
//!
//! Design `0.8.20-slice0-erasure-design.md` §4 item 5: `SourceId` replaces
//! `source_id: Option<String>` on `PreparedWrite` so that "no provenance" is
//! **inexpressible**, NOT merely rejected at runtime. The distinction matters
//! because this facade crate re-exports `PreparedWrite` and `Engine::write` is
//! public — a runtime validation check is bypassable by constructing the struct
//! directly, which is exactly what `tests/ui/unprovenanced_public_write.rs`
//! attempts.
//!
//! **Maintenance note.** `trybuild` compares rustc's full diagnostic text
//! against the checked-in `.stderr` fixture, so a rustc release that rewords the
//! type-mismatch diagnostic will turn this test red. That failure is cosmetic
//! and LOUD (never silent): regenerate the fixture with
//! `TRYBUILD=overwrite cargo test -p fathomdb --test compile_fail_provenance`
//! and confirm the diagnostic is still a type error on `source_id`.

/// The public type must make an un-provenanced write a COMPILE error.
#[test]
fn unprovenanced_public_write_does_not_compile() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/unprovenanced_public_write.rs");
}
