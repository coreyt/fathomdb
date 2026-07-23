//! Slice 35 — G4 filter grammar tests.
//!
//! Pins the closed `Predicate` enum (`JsonPathEq` / `JsonPathCompare`),
//! path allowlist enforcement, injection-safe parameterization, AND
//! composition, `SearchFilter` struct shape invariant, and shared-type
//! vocabulary exports.
//!
//! See `dev/design/slice-35-design.md` and
//! `dev/adr/ADR-0.8.0-filter-grammar.md` for the binding spec.

use fathomdb_engine::{
    ComparisonOp, Engine, EngineError, Predicate, PreparedWrite, ReadView, ScalarValue,
    SearchFilter,
};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

// ===== Helper =========================================================

fn fresh_engine(dir: &TempDir) -> Engine {
    Engine::open(dir.path().join(format!("test{SQLITE_SUFFIX}")))
        .expect("engine open failed")
        .engine
}

fn write_node(engine: &Engine, logical_id: &str, kind: &str, body: &str) {
    engine
        .write(&[PreparedWrite::Node {
            logical_id: Some(logical_id.to_string()),
            kind: kind.to_string(),
            body: body.to_string(),
            source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
            state: fathomdb_engine::InitialState::Active,
            reason: None,
            valid_from: None,
            valid_until: None,
        }])
        .expect("write failed");
}

// ===== D-F1: Exhaustiveness ============================================

/// An exhaustiveness test: the `Predicate` enum has EXACTLY two variants.
/// If a new variant is added (e.g., `JsonPathIn`), this `match` will fail
/// to compile, catching the drift at build time.
#[test]
fn predicate_enum_is_exactly_jsoneq_and_jsoncompare() {
    // Construct one of each variant via the validated constructors.
    let eq = Predicate::json_path_eq("$.status", ScalarValue::Text("open".to_string()))
        .expect("valid eq");
    let cmp = Predicate::json_path_compare("$.priority", ComparisonOp::Gt, ScalarValue::Integer(3))
        .expect("valid cmp");

    // Exhaustive match — a third variant would cause a compile error.
    match &eq {
        Predicate::JsonPathEq { path, value } => {
            assert_eq!(path, "$.status");
            assert!(matches!(value, ScalarValue::Text(_)));
        }
        Predicate::JsonPathCompare { .. } => panic!("wrong variant"),
    }
    match &cmp {
        Predicate::JsonPathCompare { path, op, value } => {
            assert_eq!(path, "$.priority");
            assert!(matches!(op, ComparisonOp::Gt));
            assert!(matches!(value, ScalarValue::Integer(_)));
        }
        Predicate::JsonPathEq { .. } => panic!("wrong variant"),
    }

    // ComparisonOp exhaustiveness — all four ops must be handled.
    let ops = [ComparisonOp::Gt, ComparisonOp::Gte, ComparisonOp::Lt, ComparisonOp::Lte];
    for op in &ops {
        let _ = Predicate::json_path_compare("$.priority", op.clone(), ScalarValue::Integer(0))
            .expect("valid op");
        // Exhaustive match on op variants.
        match op {
            ComparisonOp::Gt | ComparisonOp::Gte | ComparisonOp::Lt | ComparisonOp::Lte => {}
        }
    }

    // ScalarValue exhaustiveness — all three variants must be constructible.
    let _ = ScalarValue::Text("hello".to_string());
    let _ = ScalarValue::Integer(42);
    let _ = ScalarValue::Bool(true);
    let sv_text = ScalarValue::Text("x".to_string());
    let sv_int = ScalarValue::Integer(1);
    let sv_bool = ScalarValue::Bool(false);
    match &sv_text {
        ScalarValue::Text(_) | ScalarValue::Integer(_) | ScalarValue::Bool(_) => {}
    }
    match &sv_int {
        ScalarValue::Text(_) | ScalarValue::Integer(_) | ScalarValue::Bool(_) => {}
    }
    match &sv_bool {
        ScalarValue::Text(_) | ScalarValue::Integer(_) | ScalarValue::Bool(_) => {}
    }
}

// ===== D-F2: Exclusions ===============================================

/// No `Fused*` or `*_unchecked` symbols exist in the public `Predicate`
/// surface. This is a compile-time guarantee: the enum has exactly two
/// variants (confirmed above) and neither is named `Fused*`. The
/// constructor surface has no `_unchecked` variants (the constructors
/// are `json_path_eq` / `json_path_compare`; no unchecked builder).
///
/// At runtime we verify the debug representation contains no fused/unchecked
/// names, which catches a hypothetical future repr change.
#[test]
fn fused_and_unchecked_absent_from_surface() {
    let eq =
        Predicate::json_path_eq("$.status", ScalarValue::Text("open".to_string())).expect("valid");
    let cmp = Predicate::json_path_compare("$.priority", ComparisonOp::Gt, ScalarValue::Integer(1))
        .expect("valid");

    let eq_debug = format!("{eq:?}");
    let cmp_debug = format!("{cmp:?}");

    assert!(!eq_debug.to_lowercase().contains("fused"), "Fused found in eq debug repr: {eq_debug}");
    assert!(
        !eq_debug.to_lowercase().contains("unchecked"),
        "_unchecked found in eq debug repr: {eq_debug}"
    );
    assert!(
        !cmp_debug.to_lowercase().contains("fused"),
        "Fused found in cmp debug repr: {cmp_debug}"
    );
    assert!(
        !cmp_debug.to_lowercase().contains("unchecked"),
        "_unchecked found in cmp debug repr: {cmp_debug}"
    );
}

// ===== D-F4: Allowlist enforcement =====================================

/// A path from the allowlist (`$.status`) is accepted without error.
#[test]
fn allowlisted_path_accepted() {
    let allowed_paths =
        ["$.status", "$.priority", "$.tags", "$.kind", "$.created_at", "$.action_kind"];
    for path in allowed_paths {
        let result = Predicate::json_path_eq(path, ScalarValue::Text("test".to_string()));
        assert!(result.is_ok(), "Expected allowlisted path {path} to be accepted, got {result:?}");
    }
}

/// A non-allowlisted path (`$.private_field`) is rejected with a typed error,
/// NOT a panic. The error must be `EngineError::InvalidFilter`.
#[test]
fn non_allowlisted_path_rejected() {
    let result = Predicate::json_path_eq("$.private_field", ScalarValue::Text("x".to_string()));
    match result {
        Err(EngineError::InvalidFilter { .. }) => {
            // Correct: typed rejection, not panic.
        }
        Ok(_) => panic!("Expected Err(InvalidFilter) for non-allowlisted path, got Ok"),
        Err(other) => panic!("Expected InvalidFilter, got {other:?}"),
    }

    // Also test with compare
    let result2 =
        Predicate::json_path_compare("$.secret", ComparisonOp::Gt, ScalarValue::Integer(0));
    assert!(
        matches!(result2, Err(EngineError::InvalidFilter { .. })),
        "Expected InvalidFilter for non-allowlisted path in compare, got {result2:?}"
    );
}

/// 0.8.11.2 A-1 — `$.action_kind` is allowlisted so Memex's WMAction
/// discriminator can be filtered server-side. The constructors must accept it
/// for both string and bool values (the discriminator may be string OR bool),
/// while a clearly-non-allowlisted path (`$.secret`) is STILL rejected — a
/// guard against a blanket-accept regression.
#[test]
fn action_kind_path_allowlisted() {
    // Equality on a string discriminator value constructs.
    let eq_text = Predicate::json_path_eq("$.action_kind", ScalarValue::Text("create".to_string()));
    assert!(eq_text.is_ok(), "Expected $.action_kind (Text) to be accepted, got {eq_text:?}");

    // Equality on a bool discriminator value constructs (ScalarValue::Bool).
    let eq_bool = Predicate::json_path_eq("$.action_kind", ScalarValue::Bool(true));
    assert!(
        matches!(eq_bool, Ok(Predicate::JsonPathEq { value: ScalarValue::Bool(true), .. })),
        "Expected $.action_kind (Bool) to construct a JsonPathEq Bool predicate, got {eq_bool:?}"
    );

    // Compare on the new path also constructs.
    let cmp =
        Predicate::json_path_compare("$.action_kind", ComparisonOp::Gt, ScalarValue::Integer(0));
    assert!(cmp.is_ok(), "Expected $.action_kind compare to be accepted, got {cmp:?}");

    // Regression guard: a non-allowlisted path is STILL rejected (no blanket accept).
    let secret = Predicate::json_path_eq("$.secret", ScalarValue::Text("x".to_string()));
    assert!(
        matches!(secret, Err(EngineError::InvalidFilter { .. })),
        "Expected InvalidFilter for non-allowlisted $.secret, got {secret:?}"
    );
}

// ===== D-F4: Injection safety ==========================================

/// SQL-injection-shaped value is bound as a parameter, NEVER interpolated.
/// The `canonical_nodes` table survives; the query runs without error.
#[test]
fn injection_safe_value_is_bound_not_interpolated() {
    let dir = TempDir::new().unwrap();
    let engine = fresh_engine(&dir);

    // Seed a node with status "open" to give read_list something to scan.
    write_node(&engine, "I1", "note", r#"{"status":"open"}"#);

    // Injection-shaped value: a classic SQL injection string.
    let injection_value = "'; DROP TABLE canonical_nodes;--";
    let pred = Predicate::json_path_eq("$.status", ScalarValue::Text(injection_value.to_string()))
        .expect("allowlisted path must be accepted");

    // read_list must NOT panic and must NOT fail with a storage error.
    // The injection value is a bound `?` parameter — SQLite treats it as a
    // literal string comparison, not SQL. No rows match (no node has that status).
    let result = engine.read_list("note", &[pred], 100, &ReadView::default());
    assert!(result.is_ok(), "read_list with injection-shaped value must not error: {result:?}");
    let rows = result.unwrap();
    assert_eq!(rows.len(), 0, "injection-shaped value matches no row");

    // Confirm the table is still there by reading the legitimate node.
    let legit = Predicate::json_path_eq("$.status", ScalarValue::Text("open".to_string())).unwrap();
    let legit_rows = engine.read_list("note", &[legit], 100, &ReadView::default()).unwrap();
    assert_eq!(legit_rows.len(), 1, "canonical_nodes still exists after injection attempt");
    assert_eq!(legit_rows[0].logical_id, "I1");
}

// ===== G4 read_list functional tests ===================================

/// `read_list` with a kind filter returns only nodes of that kind (active only).
#[test]
fn read_list_returns_active_nodes_by_kind() {
    let dir = TempDir::new().unwrap();
    let engine = fresh_engine(&dir);

    write_node(&engine, "A1", "task", r#"{"status":"open"}"#);
    write_node(&engine, "A2", "task", r#"{"status":"done"}"#);
    write_node(&engine, "B1", "note", r#"{"status":"open"}"#);
    write_node(&engine, "B2", "note", r#"{"status":"draft"}"#);

    let tasks = engine.read_list("task", &[], 100, &ReadView::default()).expect("read_list failed");
    assert_eq!(tasks.len(), 2, "expected 2 tasks, got {}", tasks.len());
    for t in &tasks {
        assert_eq!(t.kind, "task", "wrong kind: {}", t.kind);
    }

    let notes = engine.read_list("note", &[], 100, &ReadView::default()).expect("read_list failed");
    assert_eq!(notes.len(), 2, "expected 2 notes, got {}", notes.len());
    for n in &notes {
        assert_eq!(n.kind, "note", "wrong kind: {}", n.kind);
    }
}

/// `read_list` with `JsonPathEq` on `$.status` returns only matching nodes.
#[test]
fn read_list_filter_eq_matches() {
    let dir = TempDir::new().unwrap();
    let engine = fresh_engine(&dir);

    write_node(&engine, "E1", "task", r#"{"status":"open","priority":1}"#);
    write_node(&engine, "E2", "task", r#"{"status":"closed","priority":2}"#);
    write_node(&engine, "E3", "task", r#"{"status":"open","priority":3}"#);

    let pred =
        Predicate::json_path_eq("$.status", ScalarValue::Text("open".to_string())).expect("valid");
    let rows =
        engine.read_list("task", &[pred], 100, &ReadView::default()).expect("read_list failed");
    assert_eq!(rows.len(), 2, "expected 2 open tasks, got {}", rows.len());
    for r in &rows {
        let body: serde_json::Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(body["status"], "open", "wrong status in body: {}", r.body);
    }
}

/// `read_list` with `JsonPathCompare{op: Gt}` on `$.priority` returns only
/// nodes with priority > 3.
#[test]
fn read_list_filter_gt_matches() {
    let dir = TempDir::new().unwrap();
    let engine = fresh_engine(&dir);

    write_node(&engine, "G1", "task", r#"{"status":"open","priority":1}"#);
    write_node(&engine, "G2", "task", r#"{"status":"open","priority":3}"#);
    write_node(&engine, "G3", "task", r#"{"status":"open","priority":4}"#);
    write_node(&engine, "G4", "task", r#"{"status":"open","priority":5}"#);

    let pred =
        Predicate::json_path_compare("$.priority", ComparisonOp::Gt, ScalarValue::Integer(3))
            .expect("valid");
    let rows =
        engine.read_list("task", &[pred], 100, &ReadView::default()).expect("read_list failed");
    assert_eq!(rows.len(), 2, "expected 2 rows with priority > 3, got {}", rows.len());
    for r in &rows {
        let body: serde_json::Value = serde_json::from_str(&r.body).unwrap();
        let p = body["priority"].as_i64().unwrap();
        assert!(p > 3, "priority {p} is not > 3");
    }
}

/// `read_list` with two predicates (AND): only nodes satisfying BOTH are returned.
#[test]
fn read_list_and_composition() {
    let dir = TempDir::new().unwrap();
    let engine = fresh_engine(&dir);

    // seed: (open, 1), (open, 4), (closed, 5), (open, 5)
    write_node(&engine, "C1", "task", r#"{"status":"open","priority":1}"#);
    write_node(&engine, "C2", "task", r#"{"status":"open","priority":4}"#);
    write_node(&engine, "C3", "task", r#"{"status":"closed","priority":5}"#);
    write_node(&engine, "C4", "task", r#"{"status":"open","priority":5}"#);

    let p1 = Predicate::json_path_eq("$.status", ScalarValue::Text("open".to_string())).unwrap();
    let p2 = Predicate::json_path_compare("$.priority", ComparisonOp::Gt, ScalarValue::Integer(3))
        .unwrap();

    let rows =
        engine.read_list("task", &[p1, p2], 100, &ReadView::default()).expect("read_list failed");
    // Only C2 (open, priority=4) and C4 (open, priority=5) match both.
    assert_eq!(rows.len(), 2, "expected 2 rows matching both predicates, got {}", rows.len());
    let ids: std::collections::HashSet<&str> = rows.iter().map(|r| r.logical_id.as_str()).collect();
    assert!(ids.contains("C2"), "C2 (open,4) must be in result");
    assert!(ids.contains("C4"), "C4 (open,5) must be in result");
}

/// Empty predicate slice → all active nodes of the given kind (up to limit).
#[test]
fn read_list_empty_filter_returns_all() {
    let dir = TempDir::new().unwrap();
    let engine = fresh_engine(&dir);

    for i in 0..5 {
        write_node(&engine, &format!("N{i}"), "widget", &format!(r#"{{"i":{i}}}"#));
    }

    let rows =
        engine.read_list("widget", &[], 100, &ReadView::default()).expect("read_list failed");
    assert_eq!(rows.len(), 5, "expected 5 widgets, got {}", rows.len());
}

// ===== D-F3: SearchFilter struct shape unchanged =======================

/// Assert `SearchFilter` has exactly the four fields:
/// `source_type`, `kind`, `created_after`, `status`.
/// If a new field is added, this test documents the change explicitly.
#[test]
fn searchfilter_struct_shape_unchanged() {
    // Construct via Default — all None fields.
    let sf: SearchFilter = SearchFilter::default();

    // Verify the fields exist and are all None (no hidden fields added).
    assert!(sf.source_type.is_none(), "source_type should be None by default");
    assert!(sf.kind.is_none(), "kind should be None by default");
    assert!(sf.created_after.is_none(), "created_after should be None by default");
    assert!(sf.status.is_none(), "status should be None by default");

    // Verify we can set and read all four fields (field-name spelling check).
    // `SearchFilter` is `#[non_exhaustive]` (0.8.20 Slice 15e fix-2); build from
    // `default()` (downstream crates cannot use a struct literal).
    let mut sf2 = SearchFilter::default();
    sf2.source_type = Some("doc".to_string());
    sf2.kind = Some("note".to_string());
    sf2.created_after = Some(1000);
    sf2.status = Some("open".to_string());
    assert_eq!(sf2.source_type.as_deref(), Some("doc"));
    assert_eq!(sf2.kind.as_deref(), Some("note"));
    assert_eq!(sf2.created_after, Some(1000));
    assert_eq!(sf2.status.as_deref(), Some("open"));

    // Debug repr must contain exactly these four field names.
    let debug = format!("{sf2:?}");
    assert!(debug.contains("source_type"), "source_type not in debug: {debug}");
    assert!(debug.contains("kind"), "kind not in debug: {debug}");
    assert!(debug.contains("created_after"), "created_after not in debug: {debug}");
    assert!(debug.contains("status"), "status not in debug: {debug}");
    // No fifth field
    assert!(
        !debug.contains("confidence") && !debug.contains("body_filter"),
        "unexpected extra field in SearchFilter: {debug}"
    );
}

// ===== D-F3: Shared vocabulary types ==================================

/// `ScalarValue` and `ComparisonOp` are exported at the fathomdb_engine crate root
/// (same path a future G10 unification would import from). This test imports them
/// by the exact path and verifies round-trip.
#[test]
fn scalar_value_and_comparison_op_are_shared_types() {
    // Import via the crate-root path (same path reserved-gap 37 would use).
    use fathomdb_engine::ComparisonOp;
    use fathomdb_engine::ScalarValue;

    let sv = ScalarValue::Text("hello".to_string());
    assert_eq!(format!("{sv:?}"), r#"Text("hello")"#);

    let sv2 = ScalarValue::Integer(99);
    assert!(matches!(sv2, ScalarValue::Integer(99)));

    let sv3 = ScalarValue::Bool(true);
    assert!(matches!(sv3, ScalarValue::Bool(true)));

    let op = ComparisonOp::Gte;
    assert!(matches!(op, ComparisonOp::Gte));

    // Verify Clone + PartialEq (required for the shared-vocabulary contract).
    let sv4 = sv.clone();
    assert_eq!(sv, sv4);
    let op2 = ComparisonOp::Lt;
    assert_eq!(op2.clone(), ComparisonOp::Lt);
    assert_ne!(ComparisonOp::Gt, ComparisonOp::Lte);
}

// ===== Defense-in-depth: direct enum construction bypass ================

/// Callers can bypass the validated constructors by constructing `Predicate`
/// enum variants directly (enum fields are `pub`). `Engine::read_list` must
/// catch non-allowlisted paths even if the constructor was not used.
#[test]
fn direct_construction_bypass_caught_by_read_list() {
    let dir = TempDir::new().unwrap();
    let engine = fresh_engine(&dir);

    // Construct a Predicate directly, bypassing the validated constructors.
    let bad_pred = Predicate::JsonPathEq {
        path: "$.not_in_allowlist".to_string(),
        value: ScalarValue::Text("x".to_string()),
    };
    let result = engine.read_list("note", &[bad_pred], 10, &ReadView::default());
    assert!(
        matches!(result, Err(EngineError::InvalidFilter { .. })),
        "read_list must reject a directly-constructed Predicate with a non-allowlisted path; got {result:?}"
    );
}

// ===== fix-2: non-JSON body rows skipped, not errored ==========================

/// If a node of the requested kind has a plain-text (non-JSON) body, `read_list`
/// with a predicate must SKIP that row rather than surfacing a "malformed JSON"
/// storage error. The json_valid(body) guard in the WHERE clause ensures this.
#[test]
fn read_list_predicate_skips_non_json_body() {
    let dir = TempDir::new().unwrap();
    let engine = fresh_engine(&dir);

    // Seed: one node with a valid JSON body matching the predicate, one with
    // a plain-text body (non-JSON).
    write_node(&engine, "json-node", "widget", r#"{"status":"open"}"#);
    write_node(&engine, "text-node", "widget", "plain text body, not JSON");

    let pred = Predicate::json_path_eq("$.status", ScalarValue::Text("open".to_string()))
        .expect("allowlisted path");

    let result = engine
        .read_list("widget", &[pred], 10, &ReadView::default())
        .expect("read_list must not error on non-JSON body rows; should skip them silently");

    // Only the JSON-body node (which matches the predicate) should be returned.
    let ids: Vec<_> = result.iter().map(|n| n.logical_id.as_str()).collect();
    assert!(ids.contains(&"json-node"), "json-node with status=open must be returned");
    assert!(!ids.contains(&"text-node"), "text-node with non-JSON body must be skipped");
}

// ===== fix-4: nodes without logical_id included for unfiltered read.list =======

/// Nodes written without a `logical_id` (PreparedWrite::Node { logical_id: None     state: fathomdb_engine::InitialState::Active,
/// Nodes written without a `logical_id` (PreparedWrite::Node { logical_id: None     reason: None,
/// Nodes written without a `logical_id` (PreparedWrite::Node { logical_id: None })
/// are active nodes. `read_list` must not SQL-filter them out with a hard
/// `logical_id IS NOT NULL` constraint — instead it handles NULL gracefully
/// in the row mapper. Nodes WITH a logical_id must still appear in results.
#[test]
fn read_list_includes_nodes_written_without_logical_id_type_check() {
    let dir = TempDir::new().unwrap();
    let engine = fresh_engine(&dir);

    // Node WITH logical_id — must appear in unfiltered read_list results.
    write_node(&engine, "has-lid", "widget", r#"{"status":"ok"}"#);

    // Node WITHOUT logical_id — can't appear in NodeRecord results (NodeRecord
    // requires String logical_id), but must NOT cause a decode error.
    engine
        .write(&[PreparedWrite::Node {
            logical_id: None,
            kind: "widget".to_string(),
            body: r#"{"status":"ok"}"#.to_string(),
            source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
            state: fathomdb_engine::InitialState::Active,
            reason: None,
            valid_from: None,
            valid_until: None,
        }])
        .expect("write without logical_id");

    let result = engine
        .read_list("widget", &[], 100, &ReadView::default())
        .expect("read_list must not error when some rows have NULL logical_id");

    // The node with a logical_id must be present.
    let ids: Vec<_> = result.iter().map(|n| n.logical_id.as_str()).collect();
    assert!(ids.contains(&"has-lid"), "node with logical_id must appear; got {ids:?}");
    // No decode error occurred — the test itself passing proves the fix.
}

// ===== fix-19: integer predicates must not match boolean JSON fields ===========

/// An integer predicate on a field that holds a JSON boolean must NOT match.
/// SQLite's json_extract returns integer 1/0 for JSON true/false, so without a
/// `json_type = 'integer'` guard an `Integer(1)` predicate would erroneously
/// match `{"priority": true}`.
#[test]
fn read_list_integer_predicate_does_not_match_boolean_field() {
    let dir = TempDir::new().unwrap();
    let engine = fresh_engine(&dir);

    // One node where priority is a JSON integer 1 (should match).
    write_node(&engine, "INT1", "task", r#"{"status":"open","priority":1}"#);
    // One node where priority is a JSON boolean true (json_extract also returns 1 — must NOT match).
    write_node(&engine, "BOOL1", "task", r#"{"status":"open","priority":true}"#);

    let pred =
        Predicate::json_path_eq("$.priority", ScalarValue::Integer(1)).expect("allowlisted path");
    let rows =
        engine.read_list("task", &[pred], 100, &ReadView::default()).expect("read_list failed");

    let ids: Vec<_> = rows.iter().map(|n| n.logical_id.as_str()).collect();
    assert!(ids.contains(&"INT1"), "integer node must be returned; got {ids:?}");
    assert!(
        !ids.contains(&"BOOL1"),
        "boolean node must NOT match an integer predicate; got {ids:?}"
    );
}

/// Symmetric guard: a bool predicate on a field holding a JSON integer must NOT match.
/// Without `json_type IN ('true', 'false')`, `Bool(true)` would match `{"priority": 1}`
/// because json_extract also returns 1 for that integer.
#[test]
fn read_list_bool_predicate_does_not_match_integer_field() {
    let dir = TempDir::new().unwrap();
    let engine = fresh_engine(&dir);

    // priority=true (JSON boolean) — should match Bool(true)
    write_node(&engine, "BOOL_TRUE", "task", r#"{"status":"open","priority":true}"#);
    // priority=1 (JSON integer) — must NOT match Bool(true)
    write_node(&engine, "INT_ONE", "task", r#"{"status":"open","priority":1}"#);

    let pred =
        Predicate::json_path_eq("$.priority", ScalarValue::Bool(true)).expect("allowlisted path");
    let rows =
        engine.read_list("task", &[pred], 100, &ReadView::default()).expect("read_list failed");

    let ids: Vec<_> = rows.iter().map(|n| n.logical_id.as_str()).collect();
    assert!(
        ids.contains(&"BOOL_TRUE"),
        "boolean-true node must be returned by Bool(true) predicate; got {ids:?}"
    );
    assert!(
        !ids.contains(&"INT_ONE"),
        "integer-1 node must NOT match a Bool(true) predicate; got {ids:?}"
    );
}

// ===== fix-21: text comparison predicates must not cross-match integers ========

/// A text `JsonPathCompare` must NOT match integer-valued JSON fields.
/// Without `json_type = 'text'`, SQLite's type ordering (INTEGER < TEXT) means
/// `priority < "zzz"` would match rows where priority is an integer, since
/// SQLite considers INTEGER < any TEXT value.
#[test]
fn read_list_text_compare_does_not_match_integer_field() {
    let dir = TempDir::new().unwrap();
    let engine = fresh_engine(&dir);

    // Two nodes: one with string status, one with integer priority.
    // status="open" (text) should match status < "zzz"; priority=5 (integer) must NOT.
    write_node(&engine, "STR_NODE", "task", r#"{"status":"open","priority":5}"#);

    // Text comparison on $.status (a string field) — should match.
    let pred_match = Predicate::json_path_compare(
        "$.status",
        ComparisonOp::Lt,
        ScalarValue::Text("zzz".to_string()),
    )
    .expect("allowlisted path");
    let rows =
        engine.read_list("task", &[pred_match], 100, &ReadView::default()).expect("read_list");
    let ids: Vec<_> = rows.iter().map(|n| n.logical_id.as_str()).collect();
    assert!(ids.contains(&"STR_NODE"), "string-valued field must match text compare; got {ids:?}");

    // Text comparison on $.priority (an integer field in this body) — must NOT match.
    let pred_no_match = Predicate::json_path_compare(
        "$.priority",
        ComparisonOp::Lt,
        ScalarValue::Text("zzz".to_string()),
    )
    .expect("allowlisted path");
    let rows2 =
        engine.read_list("task", &[pred_no_match], 100, &ReadView::default()).expect("read_list");
    let ids2: Vec<_> = rows2.iter().map(|n| n.logical_id.as_str()).collect();
    assert!(
        !ids2.contains(&"STR_NODE"),
        "integer-priority node must NOT match text compare on that field; got {ids2:?}"
    );
}
