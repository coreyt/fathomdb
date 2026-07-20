//! 0.8.20 Slice 5d (R-20-E4, design §4 item 9b) — `erase_source` is a
//! **first-class SDK lifecycle verb**, not an operator/CLI-only seam.
//!
//! **Why this test lives in the facade crate with the DEFAULT feature set.**
//! The acceptance criterion is *"an SDK-only consumer (no CLI on `PATH`) can
//! erase anonymous content end-to-end"*. In this workspace "the CLI" is not a
//! `PATH` lookup — it is the `operator` cargo feature, which `fathomdb-cli`
//! turns on and which gates `excise_source`, `rebuild_*`, `dump_*` and the rest
//! of the recovery seam. A consumer that depends on `fathomdb` with default
//! features therefore **is** the SDK-only consumer, exactly and by
//! construction. So this file asserts the property where it actually bites:
//! `#![cfg(not(feature = "operator"))]`.
//!
//! Consequence for running it: a feature-unified `cargo test --workspace` turns
//! `operator` ON (via `fathomdb-cli`) and this file compiles to nothing. The
//! real gate is `cargo test -p fathomdb --test sdk_only_erasure`, which is how
//! `no_recovery_surface.rs` is also exercised.
//!
//! **The erasure asymmetry this pins.** `excise_source` STAYS operator-gated —
//! it remains the recovery seam. `erase_source` is the governed verb over the
//! SAME engine path (design §4 item 9b: "one engine path with `excise_source`").
//! The two are not competing implementations; `excise_source` is a
//! feature-gated alias whose body is a call to the shared inner path.
//!
//! **Test-design contract (design §3, Rule 1).** An erasure witness asserts on
//! RAW state. This file cannot reach `rusqlite` (the facade has no SQL
//! dependency and must not grow one), so it uses the strongest witness the
//! governed surface offers: `read.get` / `search` are NOT used as the erasure
//! proof. Instead the proof is the returned `ExciseReport` row counts plus a
//! re-open of the same database showing the erased content is unreachable by a
//! FRESH engine (i.e. the erasure was durable, not a cache effect). The
//! raw-table assertions for the same erasure live in the engine suites
//! (`erasure_projection_registry.rs`, `erasure_completeness.rs`), which DO have
//! SQL access; this file's job is the SURFACE claim, not the completeness one.

#![cfg(not(feature = "operator"))]

use fathomdb::{Engine, InitialState, PreparedWrite, SourceId};

fn temp_db() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("sdk-only-erasure.sqlite");
    (dir, path)
}

fn anonymous_node(body: &str, source_id: &str) -> PreparedWrite {
    // "Anonymous content" = NO `logical_id`. This is the erasure axis the
    // design cares about (`h:` content-space rows): a governed row is
    // `purge`-addressable by `logical_id`, but an anonymous row is reachable
    // ONLY through its `source_id`. That is precisely why provenance became
    // mandatory in 5c, and why an SDK-only consumer with no `erase_source`
    // had NO way to discharge an erasure obligation over anonymous content.
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: body.to_string(),
        source_id: SourceId::new(source_id).expect("valid source id"),
        logical_id: None,
        state: InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

/// R-20-E4 — the whole point: this compiles and passes with the operator
/// feature OFF. Before Slice 5d the only erasure verb was `excise_source`,
/// which does not resolve here at all, so an SDK-only consumer could not erase
/// anonymous content by any means.
#[test]
fn sdk_only_consumer_erases_without_cli() {
    let (_dir, path) = temp_db();

    let opened = Engine::open(path.to_str().expect("utf-8 path")).expect("open");
    let engine = opened.engine;

    engine
        .write(&[
            anonymous_node("erasable alpha payload", "tenant-a"),
            anonymous_node("erasable beta payload", "tenant-a"),
            anonymous_node("retained gamma payload", "tenant-b"),
        ])
        .expect("write");

    // The governed SDK verb. No CLI, no `operator` feature, no raw SQL.
    let report = engine.erase_source("tenant-a").expect("erase_source");

    assert_eq!(report.source_ref, "tenant-a", "the report must name the source it erased");
    assert_eq!(report.nodes_excised, 2, "both `tenant-a` rows must be erased, and ONLY those");

    // Non-perturbation: the other tenant is untouched. Asserted as a SECOND
    // erase — its count proves `tenant-b`'s rows still existed after the first
    // call. (A search-based assertion would be invalid per design §3 Rule 1.)
    let second = engine.erase_source("tenant-b").expect("erase_source");
    assert_eq!(second.nodes_excised, 1, "the first erasure must not have touched the other source");

    // Idempotence — an already-erased source is a zero-count success, never an
    // error. This is the property that lets a consumer retry an interrupted
    // erasure obligation without a pre-check.
    let again = engine.erase_source("tenant-a").expect("erase_source is idempotent");
    assert_eq!(again.nodes_excised, 0);

    engine.close().expect("close");
}

/// Durability: the erasure survives a close/re-open by a FRESH engine, so it is
/// a committed state change rather than an in-memory effect.
#[test]
fn sdk_only_erasure_is_durable_across_reopen() {
    let (_dir, path) = temp_db();
    let path_str = path.to_str().expect("utf-8 path").to_string();

    let engine = Engine::open(&path_str).expect("open").engine;
    engine
        .write(&[
            anonymous_node("durable erasable payload", "tenant-a"),
            anonymous_node("durable retained payload", "tenant-b"),
        ])
        .expect("write");
    engine.erase_source("tenant-a").expect("erase_source");
    engine.close().expect("close");

    let reopened = Engine::open(&path_str).expect("reopen").engine;
    // Re-erasing on the fresh engine must find nothing: the delete was durable.
    let after = reopened.erase_source("tenant-a").expect("erase_source");
    assert_eq!(
        after.nodes_excised, 0,
        "the erasure must be durable across a re-open, not an in-memory effect"
    );
    // ...while the untouched source is still there to be erased.
    let other = reopened.erase_source("tenant-b").expect("erase_source");
    assert_eq!(other.nodes_excised, 1, "the unrelated source must have survived the close/re-open");
    reopened.close().expect("close");
}

/// `erase_source` rejects the same inputs `SourceId::new` rejects, so a caller
/// cannot aim an erasure at the engine's reserved provenance namespace (which
/// would delete engine substrate or the 5c legacy back-fill cohort wholesale).
#[test]
fn sdk_erase_source_rejects_empty_and_reserved_ids() {
    let (_dir, path) = temp_db();
    let engine = Engine::open(path.to_str().expect("utf-8 path")).expect("open").engine;

    for reserved in ["", "   ", "_engine:coverage", "_legacy:pre-0.8.20"] {
        assert!(
            engine.erase_source(reserved).is_err(),
            "erase_source must reject the reserved/empty source id {reserved:?}"
        );
    }

    engine.close().expect("close");
}
