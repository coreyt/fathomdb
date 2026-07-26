//! 0.8.20 Slice 25 (R-20-SUR) — D3: **registering a kind does not alter any
//! pre-existing row's `IdSpace`**.
//!
//! The second half of R-20-SUR's acceptance signal. The first half (the
//! migration guard: rows transitioning `logical_id` NULL → NOT NULL == 0) lives
//! in `fathomdb-schema/tests/slice25_logical_id_pin.rs`.
//!
//! **The ruling being enforced** (TC-11 pin A, HITL-ratified 2026-07-12; plan
//! `dev/plans/plan-0.8.20.md` §2.1) — do not re-open:
//!
//! - A stored row's id-space is **NEVER re-derived**. Supplying a `logical_id`
//!   at WRITE time is what makes a record governed; nothing after the write can
//!   change that, in either direction.
//! - Anonymous / doc-seeded nodes stay `h:<content-hash>` PERMANENTLY. The
//!   anonymous-surrogate leg is CANCELLED, not deferred.
//! - Accepted corollary, documented rather than "fixed": an anonymous row and a
//!   later governed row for the same real-world thing BOTH stay active and BOTH
//!   surface in search. The partial-unique index is
//!   `ON canonical_nodes(logical_id) WHERE superseded_at IS NULL`, and SQLite
//!   treats every NULL as distinct, so a NULL `logical_id` never collides.
//!   `anonymous_and_governed_twins_both_stay_active` pins that corollary.
//!
//! **"Registration"** is the projection registry (Slice 15d, R-20-PR):
//! `configure_projections` (add / re-register / drop) plus the boot re-derive
//! that re-drives the engine's cached `ProjectionSpec`s from the durable
//! registry. The registry is keyed on ATTRIBUTE NAME, never on entity kind, and
//! it must be provably INERT with respect to identity: it may build and drop
//! projections all it likes, but no canonical row's `logical_id` — and therefore
//! no row's [`IdSpace`] — may move.
//!
//! Anti-vacuity: every "nothing changed" assertion in this file is paired with a
//! negative control that makes the SAME comparator fire on a real mutation. A
//! green here is only worth something if red is reachable.

use fathomdb_engine::{
    Engine, ExtractDocument, IdSpace, IdSpaceKind, InitialState, PreparedWrite, ProjectionFts,
    ProjectionRole, ProjectionSpec, SourceId,
};
use fathomdb_schema::SQLITE_SUFFIX;
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn db_path(dir: &TempDir, name: &str) -> PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

fn stub_harness_cmd() -> Vec<String> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/slice15_byo_llm/stub_harness.py");
    assert!(script.exists(), "stub harness must exist at {}", script.display());
    vec!["python3".to_string(), script.to_string_lossy().to_string()]
}

/// An ANONYMOUS / doc-seeded node: `logical_id: None`. Permanently `h:`.
fn anonymous(source: &str, body_json: &str) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: body_json.to_string(),
        source_id: SourceId::new(source).expect("source id"),
        logical_id: None,
        state: InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

/// A GOVERNED node: the caller supplied a `logical_id` at WRITE time, which is
/// exactly what admission means under the pin. Permanently `l:`.
fn governed(logical_id: &str, source: &str, body_json: &str) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: body_json.to_string(),
        source_id: SourceId::new(source).expect("source id"),
        logical_id: Some(logical_id.to_string()),
        state: InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

fn spec(name: &str, roles: &[ProjectionRole], fts: bool) -> ProjectionSpec {
    ProjectionSpec {
        name: name.to_string(),
        roles: roles.iter().copied().collect::<BTreeSet<_>>(),
        fts: fts.then_some(ProjectionFts { tokenizer: None }),
        vector: None,
    }
}

fn ro(path: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("open read-only")
}

/// The engine's `derive_stable_id` rule, restated against the REAL public
/// [`IdSpace`] type: `Some(non-empty)` → `IdSpace::logical`, otherwise
/// `IdSpace::content(sha256(body))`. `derive_stable_id` itself is private, so
/// the test reproduces the documented contract and cross-checks it against live
/// `SearchHit::id` values in `live_search_hit_id_spaces_survive_registration`.
fn id_space_of(logical_id: Option<&str>, body: &str) -> IdSpace {
    match logical_id {
        Some(lid) if !lid.is_empty() => IdSpace::logical(lid),
        _ => {
            let mut hasher = Sha256::new();
            hasher.update(body.as_bytes());
            IdSpace::content(
                hasher.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>(),
            )
        }
    }
}

/// Every canonical node row's identity state, at rest, plus its DERIVED
/// prefixed id: `(write_cursor, logical_id, body, IdSpace::to_prefixed())`.
/// Ordered by `write_cursor` so the vec compares positionally.
type IdentitySnapshot = Vec<(i64, Option<String>, String, String)>;

fn snapshot(path: &Path) -> IdentitySnapshot {
    let conn = ro(path);
    let mut stmt = conn
        .prepare("SELECT write_cursor, logical_id, body FROM canonical_nodes ORDER BY write_cursor")
        .unwrap();
    let rows: Vec<(i64, Option<String>, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows.into_iter()
        .map(|(cursor, lid, body)| {
            let prefixed = id_space_of(lid.as_deref(), &body).to_prefixed();
            (cursor, lid, body, prefixed)
        })
        .collect()
}

fn spaces(snap: &IdentitySnapshot) -> (usize, usize) {
    let content = snap.iter().filter(|(_, _, _, id)| id.starts_with("h:")).count();
    let logical = snap.iter().filter(|(_, _, _, id)| id.starts_with("l:")).count();
    (content, logical)
}

/// Seed the mixed corpus every test in this file shares: three ANONYMOUS
/// doc-seeded nodes (JSON bodies, so the attribute projections have something
/// to derive from), two caller-GOVERNED nodes, and two entity nodes MINTED by
/// the extractor arm of `ingest_with_extractor` (the only in-engine minting
/// site). Returns the post-seed snapshot.
fn seed_mixed_corpus(engine: &Engine) -> usize {
    engine
        .write(&[
            anonymous("src:doc-a", r#"{"status":"open","title":"the quick brown fox"}"#),
            anonymous("src:doc-b", r#"{"status":"closed","title":"a doc-seeded chunk"}"#),
            anonymous("src:doc-c", r#"{"status":"open","title":"lorem ipsum dolor"}"#),
            governed("ent-zephyr", "src:doc-d", r#"{"status":"open","title":"zephyr"}"#),
            governed("ent-boreas", "src:doc-e", r#"{"status":"closed","title":"boreas"}"#),
        ])
        .expect("mixed corpus write");

    let cmd_strings = stub_harness_cmd();
    let cmd_refs: Vec<&str> = cmd_strings.iter().map(String::as_str).collect();
    let docs = vec![ExtractDocument {
        source_doc_id: "doc-simple".to_string(),
        body: "Alice owns the project".to_string(),
    }];
    let receipt = engine.ingest_with_extractor(&cmd_refs, &docs).expect("extractor ingest");
    assert!(
        receipt.nodes_written >= 2,
        "the extractor arm must MINT governed entity nodes (the governed minting path already \
         exists — Slice 25 proves it is restricted, it does not build a new one)"
    );
    receipt.nodes_written as usize
}

// ===========================================================================
// D3 — registration is inert with respect to identity
// ===========================================================================

/// **The acceptance signal.** Write a mixed corpus (anonymous `logical_id =
/// NULL` + caller-governed + extractor-minted), snapshot every node's
/// `logical_id` and derived `IdSpace::to_prefixed()`, then REGISTER: a fresh
/// projection add, an identical re-registration (the idempotent no-op), an
/// explicit drop, and a boot re-derive (close + re-open, which re-drives the
/// cached specs from the durable registry). Every pre-existing row's
/// `logical_id` and `to_prefixed()` must be BYTE-IDENTICAL throughout, the
/// anonymous rows still `h:` and the governed rows still `l:`.
#[test]
fn registering_projections_never_alters_a_pre_existing_row_id_space() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "registration_inert");

    let baseline = {
        let opened = Engine::open_without_embedder_for_test(&path).expect("open");
        let minted = seed_mixed_corpus(&opened.engine);

        let baseline = snapshot(&path);
        assert_eq!(baseline.len(), 5 + minted, "corpus is 5 hand-written rows + minted entities");
        let (content, logical) = spaces(&baseline);
        assert_eq!(content, 3, "the three anonymous doc-seeded rows are in the `h:` space");
        assert_eq!(
            logical,
            2 + minted,
            "the two caller-governed rows plus every minted entity are in the `l:` space"
        );

        // --- REGISTRATION, every shape of it ---------------------------------
        let status =
            spec("status", &[ProjectionRole::Filterable, ProjectionRole::Searchable], true);
        let title = spec("title", &[ProjectionRole::Searchable], true);

        let first = opened
            .engine
            .configure_projections(&[status.clone(), title.clone()], &[])
            .expect("register two projections");
        assert!(!first.unchanged, "the first apply must do REAL work (else this test is vacuous)");
        assert_eq!(snapshot(&path), baseline, "a projection add must not move any row's IdSpace");

        // Re-registration: the idempotent no-op.
        let again = opened
            .engine
            .configure_projections(&[status.clone(), title.clone()], &[])
            .expect("re-register");
        assert!(again.unchanged, "identical re-registration diffs to a no-op");
        assert_eq!(snapshot(&path), baseline, "re-registration must not move any row's IdSpace");

        // Explicit drop.
        let dropped = opened
            .engine
            .configure_projections(&[], &["title".to_string()])
            .expect("drop one projection");
        assert_eq!(
            dropped.dropped,
            vec!["title".to_string()],
            "the drop must drop exactly `title`"
        );
        assert_eq!(snapshot(&path), baseline, "a projection drop must not move any row's IdSpace");

        baseline
    };

    // --- BOOT RE-DERIVE: close, re-open, re-drive from the durable registry ---
    let reopened = Engine::open_without_embedder_for_test(&path).expect("re-open");
    let after_boot = snapshot(&path);
    assert_eq!(
        after_boot, baseline,
        "boot re-derive of the projection registry must not move any row's IdSpace"
    );
    assert_eq!(
        reopened.engine.read_projections().unwrap().len(),
        1,
        "the registration really did survive the boot (so the inertness above is not vacuous)"
    );

    // --- Anti-vacuity: the comparator IS sensitive to the transition we ban ---
    let mut mutated = baseline.clone();
    let anonymous_row = mutated
        .iter_mut()
        .find(|(_, lid, _, _)| lid.is_none())
        .expect("an anonymous row must exist");
    anonymous_row.1 = Some("minted-surrogate".to_string());
    anonymous_row.3 = IdSpace::logical("minted-surrogate").to_prefixed();
    assert_ne!(
        mutated, baseline,
        "the snapshot comparator must FAIL when a row's id-space moves h: → l: — otherwise every \
         `assert_eq!(snapshot, baseline)` above proves nothing"
    );
}

/// **Negative control, executed for real.** The same comparator, the same
/// engine, but the forbidden `UPDATE … SET logical_id` is run directly against
/// the writer connection. Both the at-rest snapshot AND the derived id-space
/// must flip. This is what makes the green above falsifiable rather than
/// decorative.
#[test]
fn a_real_backfill_does_move_the_id_space_which_is_why_it_is_banned() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "negative_control");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    opened
        .engine
        .write(&[anonymous("src:doc-a", r#"{"status":"open","title":"anonymous body"}"#)])
        .unwrap();

    let before = snapshot(&path);
    assert_eq!(spaces(&before), (1, 0), "one anonymous row, `h:` space");

    opened
        .engine
        .execute_for_test(
            "UPDATE canonical_nodes SET logical_id = 'minted-' || write_cursor \
             WHERE logical_id IS NULL",
        )
        .expect("the rogue backfill runs (nothing in SQLite stops it — the guard is the point)");

    let after = snapshot(&path);
    assert_ne!(before, after, "a backfill DOES change the snapshot");
    assert_eq!(spaces(&after), (0, 1), "…by moving the row out of the `h:` space into `l:`");
    assert_ne!(before[0].3, after[0].3, "…and the derived prefixed id is no longer byte-identical");
}

/// The live-functional half: the id-space is observable on `SearchHit::id`, not
/// just in the raw table, and registration does not perturb it. An anonymous
/// doc-seeded hit comes back [`IdSpaceKind::Content`] (`h:`); a governed hit
/// comes back [`IdSpaceKind::Logical`] (`l:`) — before AND after
/// `configure_projections`, with byte-identical `to_prefixed()`.
#[test]
fn live_search_hit_id_spaces_survive_registration() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "live_hits");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    let engine = &opened.engine;

    engine
        .write(&[
            anonymous("src:doc-a", r#"{"status":"open","title":"peculiar wombat telemetry"}"#),
            governed("ent-zephyr", "src:doc-d", r#"{"status":"open","title":"zephyr aurora"}"#),
        ])
        .unwrap();

    // `id_of` is the LIVE oracle: the engine's own derived hit id.
    let id_of = |query: &str| -> IdSpace {
        let result = engine.search(query).expect("search");
        result.results.first().expect("a hit for the seeded body").id.clone()
    };

    let anon_before = id_of("wombat");
    let gov_before = id_of("aurora");
    assert_eq!(anon_before.space, IdSpaceKind::Content, "a doc-seeded hit is `h:`");
    assert_eq!(gov_before.space, IdSpaceKind::Logical, "a governed hit is `l:`");
    assert_eq!(gov_before.value, "ent-zephyr", "the `l:` value IS the write-time logical_id");

    // Cross-check the test's local `id_space_of` against the engine's own
    // derivation, so the at-rest snapshots in this file are anchored to the
    // real rule rather than to a restatement that could drift.
    let snap = snapshot(&path);
    let derived: Vec<&String> = snap.iter().map(|(_, _, _, id)| id).collect();
    assert!(
        derived.contains(&&anon_before.to_prefixed())
            && derived.contains(&&gov_before.to_prefixed()),
        "the local id-space rule must agree with the engine's live SearchHit::id"
    );

    let delta = engine
        .configure_projections(
            &[
                spec("status", &[ProjectionRole::Filterable, ProjectionRole::Searchable], true),
                spec("title", &[ProjectionRole::Searchable], true),
            ],
            &[],
        )
        .expect("register");
    assert!(!delta.unchanged, "registration must do real work");

    assert_eq!(id_of("wombat"), anon_before, "the anonymous hit id is byte-identical after");
    assert_eq!(id_of("aurora"), gov_before, "the governed hit id is byte-identical after");
}

/// **The accepted corollary, pinned as documentation-in-code** (TC-11 pin:
/// document it, do NOT "fix" it). An anonymous row and a later governed row for
/// the same real-world thing BOTH stay active and BOTH surface — the
/// partial-unique index is `ON canonical_nodes(logical_id) WHERE superseded_at
/// IS NULL`, and SQLite treats every NULL as distinct, so the anonymous row is
/// never superseded by the governed one. Writing the governed twin does NOT
/// retro-mint an id onto the anonymous one: minting is decided at WRITE time,
/// per row, and never re-derived.
#[test]
fn anonymous_and_governed_twins_both_stay_active_and_both_surface() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "twins");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    let engine = &opened.engine;

    engine
        .write(&[anonymous("src:doc-a", r#"{"title":"a memo about the peculiar wombat"}"#)])
        .unwrap();
    let anonymous_only = snapshot(&path);
    assert_eq!(spaces(&anonymous_only), (1, 0));

    // Later, the SAME real-world thing arrives governed.
    engine.write(&[governed("ent-wombat", "src:doc-b", r#"{"title":"wombat"}"#)]).unwrap();

    let both = snapshot(&path);
    assert_eq!(both.len(), 2, "both rows exist");
    assert_eq!(spaces(&both), (1, 1), "one stays `h:`, the new one is `l:` — no split, no merge");
    assert_eq!(
        both[0], anonymous_only[0],
        "writing the governed twin must not retro-mint an id onto the anonymous row"
    );

    let active: i64 = ro(&path)
        .query_row("SELECT COUNT(*) FROM canonical_nodes WHERE superseded_at IS NULL", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(active, 2, "NULL never collides on the partial-unique index — both stay ACTIVE");

    // …and both surface in search. This is the accepted duplicate.
    let spaces_seen: HashSet<IdSpaceKind> =
        engine.search("wombat").expect("search").results.iter().map(|h| h.id.space).collect();
    assert!(
        spaces_seen.contains(&IdSpaceKind::Content) && spaces_seen.contains(&IdSpaceKind::Logical),
        "the accepted corollary: BOTH the anonymous and the governed twin surface, got \
         {spaces_seen:?}"
    );
}

/// The one wrinkle in the corollary, recorded rather than "fixed": when the twins
/// have BYTE-IDENTICAL bodies, fusion's documented **dedup-on-body** ordering
/// (see `SearchResult`) collapses them to a single hit and the first-wins rule
/// keeps the ANONYMOUS one (lower `write_cursor`). Both rows are still ACTIVE at
/// rest and neither id-space moved — the collapse is a retrieval-presentation
/// property of body-dedup, NOT a re-derivation. Pinned here so the corollary
/// above is not read as a promise that identical-body twins each get a hit.
#[test]
fn identical_body_twins_collapse_under_fusion_body_dedup_but_both_persist() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "identical_twins");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    let engine = &opened.engine;

    let body = r#"{"title":"peculiar wombat telemetry"}"#;
    engine.write(&[anonymous("src:doc-a", body)]).unwrap();
    engine.write(&[governed("ent-wombat", "src:doc-b", body)]).unwrap();

    let at_rest = snapshot(&path);
    assert_eq!(spaces(&at_rest), (1, 1), "at rest, both twins exist in their own id-spaces");

    let hits = engine.search("wombat").expect("search").results;
    let bodies: HashSet<&str> = hits.iter().map(|h| h.body.as_str()).collect();
    assert_eq!(bodies.len(), hits.len(), "fusion dedups on body — one hit per distinct body");
    assert_eq!(
        hits.first().map(|h| h.id.space),
        Some(IdSpaceKind::Content),
        "first-wins keeps the earlier (anonymous) row; its id-space is unchanged"
    );
}

/// Supersession is the one legitimate way a governed row's ACTIVE version
/// changes — and even it never re-derives an id-space: the superseded row keeps
/// its `logical_id`, and the new active row carries the SAME one. Pinned so a
/// future supersession change cannot quietly become a re-minting path.
#[test]
fn supersession_reuses_the_write_time_logical_id_rather_than_re_deriving_one() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "supersede");
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    let engine = &opened.engine;

    engine.write(&[governed("ent-zephyr", "src:1", r#"{"status":"open"}"#)]).unwrap();
    let first = snapshot(&path);
    engine.write(&[governed("ent-zephyr", "src:2", r#"{"status":"closed"}"#)]).unwrap();
    let second = snapshot(&path);

    assert_eq!(second.len(), 2, "tombstone-then-insert leaves both versions on disk");
    assert_eq!(first[0], second[0], "the superseded row is untouched, id-space included");
    assert!(
        second
            .iter()
            .all(|(_, lid, _, id)| lid.as_deref() == Some("ent-zephyr") && id == "l:ent-zephyr"),
        "every version carries the SAME write-time logical_id — nothing is re-derived"
    );
}
