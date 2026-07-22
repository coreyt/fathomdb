//! 0.8.11 Slice 40 (#17) — filter-grammar unification (G4 + G10).
//!
//! ADR-0.8.11-filter-grammar-unification (Option A): ONE unified `Filter` /
//! `FilterTerm` type dispatched to TWO compilation backends. This suite pins:
//!   - `FilterTerm` exhaustiveness (exactly the 5 D1 variants; no Fused/unchecked
//!     leak — inherit D-F2);
//!   - the D4 sugar lowering `SearchFilter -> Filter` in canonical order, and the
//!     lossless round-trip back (the byte-identical-SQL guarantee);
//!   - **total backend dispatch + typed rejection** (D3): `search_filter`
//!     typed-rejects a `Json` term (no post-KNN `json_extract` demotion), while
//!     `read.list` accepts the full set incl. the `SourceType`/`Kind`
//!     constant-folds;
//!   - a **shared-fixture parity** assertion: one DB, one logical predicate,
//!     asserted on BOTH `search_filter` (vec0 pre-KNN) and `read_list_filter`
//!     (canonical_nodes json_extract).
//!
//! No mocking of the database.

use std::sync::Arc;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{
    ComparisonOp, Engine, EngineError, Filter, FilterTerm, Predicate, PreparedWrite, ReadView,
    ScalarValue, SearchFilter,
};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

#[derive(Clone, Debug)]
struct FixedEmbedder;

impl Embedder for FixedEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new("deterministic", "rev-a", 8)
    }
    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        let mut v = vec![0.0_f32; 8];
        v[0] = 1.0;
        Ok(v)
    }
}

fn fixture(name: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("{name}{SQLITE_SUFFIX}"));
    (dir, path)
}

// ===== D1: FilterTerm exhaustiveness (exactly five variants) ==============

/// Exhaustiveness pin: `FilterTerm` is EXACTLY the five D1 variants. A sixth
/// variant (or a `Fused`/`_unchecked` leak) breaks this `match` at compile time.
#[test]
fn filterterm_enum_is_exactly_five_variants() {
    let terms = [
        FilterTerm::SourceType("todo".to_string()),
        FilterTerm::Kind("todo".to_string()),
        FilterTerm::CreatedAfter(0),
        FilterTerm::Status("open".to_string()),
        FilterTerm::Json(
            Predicate::json_path_eq("$.status", ScalarValue::Text("open".to_string()))
                .expect("valid"),
        ),
    ];
    for t in &terms {
        // Exhaustive match — a sixth variant would fail to compile.
        match t {
            FilterTerm::SourceType(_)
            | FilterTerm::Kind(_)
            | FilterTerm::CreatedAfter(_)
            | FilterTerm::Status(_)
            | FilterTerm::Json(_) => {}
        }
        // D-F2: no fused/unchecked names leak into the unified surface.
        let debug = format!("{t:?}").to_lowercase();
        assert!(!debug.contains("fused"), "Fused leaked into FilterTerm: {debug}");
        assert!(!debug.contains("unchecked"), "_unchecked leaked into FilterTerm: {debug}");
    }
}

// ===== D4: SearchFilter -> Filter sugar + lossless round-trip =============

/// The shipped G10 `SearchFilter` lowers to the unified `Filter` in canonical
/// field order, and the round-trip back yields the identical `SearchFilter`
/// (the byte-identical-vec0-SQL guarantee — same fields, same canonical order).
#[test]
fn searchfilter_sugar_lowers_and_round_trips() {
    // `SearchFilter` is `#[non_exhaustive]` (0.8.20 Slice 15e fix-2); build from
    // `default()` (downstream crates cannot use a struct literal).
    let mut sf = SearchFilter::default();
    sf.source_type = Some("todo".to_string());
    sf.kind = Some("todo".to_string());
    sf.created_after = Some(1000);
    sf.status = Some("open".to_string());
    let unified = Filter::from(&sf);
    assert_eq!(
        unified.terms,
        vec![
            FilterTerm::SourceType("todo".to_string()),
            FilterTerm::Kind("todo".to_string()),
            FilterTerm::CreatedAfter(1000),
            FilterTerm::Status("open".to_string()),
        ],
        "canonical order: source_type, kind, created_after, status"
    );
    // Lossless round-trip (D4): Filter::from(&sf).to_search_filter() == sf.
    let back = unified.to_search_filter().expect("metadata-only never rejects");
    assert_eq!(back, sf, "round-trip must be identity");

    // An all-None SearchFilter lowers to an empty unified Filter (unfiltered).
    let empty = Filter::from(&SearchFilter::default());
    assert!(empty.terms.is_empty(), "all-None SearchFilter -> empty terms");
    assert_eq!(empty.to_search_filter().unwrap(), SearchFilter::default());
}

// ===== D3: search_filter typed-rejects an arbitrary Json term =============

/// THE core no-demotion guarantee (D3): a `FilterTerm::Json` term on the vec0
/// search backend is **typed-rejected** with `InvalidFilter` — never silently
/// demoted to a post-KNN `json_extract`. Asserted both at the lowering layer
/// (`Filter::to_search_filter`) and through `Engine::search_filter`.
#[test]
fn search_filter_typed_rejects_json_term() {
    let json_filter = Filter {
        terms: vec![FilterTerm::Json(
            Predicate::json_path_compare("$.priority", ComparisonOp::Gt, ScalarValue::Integer(3))
                .expect("valid"),
        )],
    };

    // Lowering layer rejects with a clear, stated reason.
    match json_filter.to_search_filter() {
        Err(EngineError::InvalidFilter { reason }) => {
            assert!(
                reason.contains("post-KNN") || reason.contains("json-path"),
                "rejection reason names the no-demotion cause: {reason}"
            );
        }
        other => panic!("expected InvalidFilter, got {other:?}"),
    }

    // End-to-end through the engine surface.
    let (_dir, path) = fixture("s40_reject_json");
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(FixedEmbedder)).expect("open");
    opened.engine.configure_vector_kind_for_test("todo").expect("vector kind");
    opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "todo".to_string(),
            body: r#"{"status":"open","priority":5}"#.to_string(),
            source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
            logical_id: Some("T1".to_string()),
            state: fathomdb_engine::InitialState::Active,
            reason: None,
            valid_from: None,
            valid_until: None,
        }])
        .expect("write");
    opened.engine.drain(10_000).expect("drain");

    let result = opened.engine.search_filter("semantic", &json_filter);
    assert!(
        matches!(result, Err(EngineError::InvalidFilter { .. })),
        "search_filter must typed-reject a Json term; got {result:?}"
    );

    // A metadata-only Filter is accepted on the same surface.
    let meta = Filter { terms: vec![FilterTerm::Kind("todo".to_string())] };
    assert!(opened.engine.search_filter("semantic", &meta).is_ok(), "metadata subset accepted");

    opened.engine.close().unwrap();
}

// ===== D3: read.list accepts the FULL set + constant-folds ================

/// `read_list_filter` accepts the full term set: `Json`, `Status`, and
/// `CreatedAfter` all lower to allowlisted `json_extract` clauses over the body.
#[test]
fn read_list_filter_accepts_full_set() {
    let dir = TempDir::new().unwrap();
    let engine = Engine::open(dir.path().join(format!("rl{SQLITE_SUFFIX}"))).expect("open").engine;

    let seed = |lid: &str, body: &str| {
        engine
            .write(&[PreparedWrite::Node {
                kind: "todo".to_string(),
                body: body.to_string(),
                source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
                logical_id: Some(lid.to_string()),
                state: fathomdb_engine::InitialState::Active,
                reason: None,
                valid_from: None,
                valid_until: None,
            }])
            .expect("write");
    };
    seed("A", r#"{"status":"open","created_at":100,"priority":5}"#);
    seed("B", r#"{"status":"done","created_at":200,"priority":1}"#);
    seed("C", r#"{"status":"open","created_at":300,"priority":9}"#);

    // Status("open") AND CreatedAfter(150) AND Json(priority>3): only C.
    let filter = Filter {
        terms: vec![
            FilterTerm::Status("open".to_string()),
            FilterTerm::CreatedAfter(150),
            FilterTerm::Json(
                Predicate::json_path_compare(
                    "$.priority",
                    ComparisonOp::Gt,
                    ScalarValue::Integer(3),
                )
                .expect("valid"),
            ),
        ],
    };
    let rows = engine
        .read_list_filter("todo", &filter, 100, &ReadView::default())
        .expect("read_list_filter");
    let ids: Vec<&str> = rows.iter().map(|r| r.logical_id.as_str()).collect();
    assert_eq!(ids, vec!["C"], "open AND created_at>=150 AND priority>3 => only C; got {ids:?}");
}

/// `Kind` and `SourceType` terms constant-fold against the partition `kind`
/// argument on `read.list` (D2/D3): a matching fold is a no-op (returns all),
/// a non-matching fold is guaranteed-empty (returns nothing, no SQL).
#[test]
fn read_list_filter_kind_and_source_type_constant_fold() {
    let dir = TempDir::new().unwrap();
    let engine = Engine::open(dir.path().join(format!("cf{SQLITE_SUFFIX}"))).expect("open").engine;
    engine
        .write(&[
            PreparedWrite::Node {
                kind: "todo".to_string(),
                body: r#"{"status":"open"}"#.to_string(),
                source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
                logical_id: Some("T1".to_string()),
                state: fathomdb_engine::InitialState::Active,
                reason: None,
                valid_from: None,
                valid_until: None,
            },
            PreparedWrite::Node {
                kind: "todo".to_string(),
                body: r#"{"status":"open"}"#.to_string(),
                source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
                logical_id: Some("T2".to_string()),
                state: fathomdb_engine::InitialState::Active,
                reason: None,
                valid_from: None,
                valid_until: None,
            },
        ])
        .expect("write");

    // Kind matching the partition: pass-all (no-op).
    let kind_match = Filter { terms: vec![FilterTerm::Kind("todo".to_string())] };
    assert_eq!(
        engine.read_list_filter("todo", &kind_match, 100, &ReadView::default()).unwrap().len(),
        2,
        "Kind(todo) on partition todo is a no-op => all rows"
    );
    // Kind mismatching the partition: guaranteed-empty.
    let kind_mismatch = Filter { terms: vec![FilterTerm::Kind("note".to_string())] };
    assert!(
        engine
            .read_list_filter("todo", &kind_mismatch, 100, &ReadView::default())
            .unwrap()
            .is_empty(),
        "Kind(note) on partition todo constant-folds to empty"
    );

    // SourceType: resolve_source_type("todo") == "todo" => pass-all; else empty.
    let st_match = Filter { terms: vec![FilterTerm::SourceType("todo".to_string())] };
    assert_eq!(
        engine.read_list_filter("todo", &st_match, 100, &ReadView::default()).unwrap().len(),
        2,
        "SourceType(todo) folds pass-all on partition todo (resolve_source_type)"
    );
    let st_mismatch = Filter { terms: vec![FilterTerm::SourceType("email".to_string())] };
    assert!(
        engine
            .read_list_filter("todo", &st_mismatch, 100, &ReadView::default())
            .unwrap()
            .is_empty(),
        "SourceType(email) constant-folds to empty on partition todo"
    );
}

// ===== D5: shared-fixture parity (one predicate, both backends) ===========

/// The load-bearing parity assertion: one DB seeded with both canonical_nodes
/// bodies AND vec0 metadata; a single logical predicate (`kind = "todo"`) is
/// asserted on BOTH backends — `search_filter` (vec0 pre-KNN) and
/// `read_list_filter` (canonical_nodes) — from the same rows.
#[test]
fn parity_kind_predicate_both_backends() {
    let (_dir, path) = fixture("s40_parity");
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(FixedEmbedder)).expect("open");
    opened.engine.configure_vector_kind_for_test("todo").expect("vector kind todo");
    opened.engine.configure_vector_kind_for_test("note").expect("vector kind note");
    opened
        .engine
        .write(&[
            PreparedWrite::Node {
                kind: "todo".to_string(),
                body: r#"{"status":"open"}"#.to_string(),
                source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
                logical_id: Some("TODO1".to_string()),
                state: fathomdb_engine::InitialState::Active,
                reason: None,
                valid_from: None,
                valid_until: None,
            },
            PreparedWrite::Node {
                kind: "note".to_string(),
                body: r#"{"status":"open"}"#.to_string(),
                source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
                logical_id: Some("NOTE1".to_string()),
                state: fathomdb_engine::InitialState::Active,
                reason: None,
                valid_from: None,
                valid_until: None,
            },
        ])
        .expect("write");
    opened.engine.drain(10_000).expect("drain");

    let filter = Filter { terms: vec![FilterTerm::Kind("todo".to_string())] };

    // vec0 backend: only todo-kind hits survive (pruned pre-KNN).
    let hits = opened.engine.search_filter("semantic", &filter).expect("search_filter");
    assert!(!hits.results.is_empty(), "todo hits present on vec0 backend");
    assert!(
        hits.results.iter().all(|h| h.kind == "todo"),
        "search_filter prunes non-todo: {:?}",
        hits.results.iter().map(|h| h.kind.clone()).collect::<Vec<_>>()
    );

    // canonical_nodes backend: the same Kind(todo) term constant-folds (no-op on
    // partition todo) and returns the todo node.
    let rows = opened
        .engine
        .read_list_filter("todo", &filter, 100, &ReadView::default())
        .expect("read_list_filter");
    let ids: Vec<&str> = rows.iter().map(|r| r.logical_id.as_str()).collect();
    assert_eq!(ids, vec!["TODO1"], "read_list_filter returns the todo node; got {ids:?}");

    // Cross-partition: the Kind(todo) term folds to empty on the note partition.
    assert!(
        opened
            .engine
            .read_list_filter("note", &filter, 100, &ReadView::default())
            .unwrap()
            .is_empty(),
        "Kind(todo) folds empty on the note partition"
    );

    opened.engine.close().unwrap();
}
