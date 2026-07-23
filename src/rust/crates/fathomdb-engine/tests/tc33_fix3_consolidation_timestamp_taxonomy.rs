//! 0.8.20 Slice 15c — TC-33 fix-3 (codex §9 P2): error-taxonomy regression on
//! malformed consolidation verdicts.
//!
//! The consolidation verdict is a BYO-LLM PROVIDER boundary whose documented
//! error leaf is `EngineError::Consolidator` ("ConsolidatorError"). When the
//! provider (the LLM consolidator) emits a malformed `t_invalid` on an
//! `invalidate` verdict, that is a PROVIDER protocol fault — NOT a user
//! bad-argument. The two sibling failure modes on the same value already return
//! `Consolidator` (missing key; null/unparseable-to-None), but the malformed /
//! non-string case leaked `EngineError::InvalidArgument` because it propagated
//! `normalize_extractor_timestamp`'s extractor-boundary error verbatim.
//!
//! FOOTPRINT / NO-EGRESS (R-CON-3): every harness here is a LOCAL, DETERMINISTIC
//! Python script — CALLER-SIDE BYO-LLM / OFFLINE-BUILD. No network, no LLM.

use fathomdb_engine::{ConsolidateAxis, Engine, EngineError, PreparedWrite};
use tempfile::TempDir;

use fathomdb_schema::SQLITE_SUFFIX;

#[allow(clippy::too_many_arguments)]
fn fact_edge(
    kind: &str,
    from: &str,
    to: &str,
    logical_id: &str,
    body: &str,
    t_valid: i64,
    confidence: f64,
) -> PreparedWrite {
    PreparedWrite::Edge {
        kind: kind.to_string(),
        from: from.to_string(),
        to: to.to_string(),
        source_id: fathomdb_engine::SourceId::new(format!("doc-{to}")).expect("test source id"),
        logical_id: Some(logical_id.to_string()),
        body: Some(body.to_string()),
        t_valid: Some(t_valid),
        t_invalid: None,
        confidence: Some(confidence),
        extractor_model_id: Some("stub-extractor-v1".to_string()),
        temporal_fallback: None,
    }
}

/// Two competing active edges on one (subject=`bob`, relation=`works_for`) axis.
fn seed_competing_edges(engine: &Engine) {
    let older = fact_edge(
        "works_for",
        "bob",
        "acme",
        "edge-acme",
        "Bob works for Acme",
        1_546_300_800, // 2019-01-01T00:00:00Z
        0.90,
    );
    let newer = fact_edge(
        "works_for",
        "bob",
        "globex",
        "edge-globex",
        "Bob works for Globex",
        1_640_995_200, // 2022-01-01T00:00:00Z
        0.80,
    );
    engine.write(&[older, newer]).expect("seed two competing edges");
}

fn axes() -> Vec<ConsolidateAxis> {
    vec![ConsolidateAxis {
        subject_logical_id: "bob".to_string(),
        relation: "works_for".to_string(),
    }]
}

/// Drive `consolidate_with_provider` against an inline harness that rules
/// `invalidate` on edge-acme with the given raw JSON `t_invalid` literal, and
/// return the result. `t_invalid_literal` is spliced into the verdict JSON
/// verbatim (e.g. `"\"not-a-timestamp\""` for a malformed string, `12345` for a
/// non-string JSON number).
fn run_with_bad_t_invalid(
    name: &str,
    t_invalid_literal: &str,
) -> Result<fathomdb_engine::ConsolidateReceipt, EngineError> {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("{name}{SQLITE_SUFFIX}"));
    let opened = Engine::open_without_embedder_for_test(&path).expect("open");
    seed_competing_edges(&opened.engine);

    // Inline harness: keep globex, invalidate acme with a caller-supplied
    // (malformed) t_invalid literal. CALLER-SIDE BYO-LLM / OFFLINE-BUILD.
    let harness = format!(
        r#"
import json, sys
P = "fathomdb.consolidate.v1"
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    if msg.get("type") == "hello":
        print(json.dumps({{"protocol": P, "type": "ready", "schema_version": 1,
                          "model": "stub-consolidate-v1", "supported_tasks": ["consolidate"],
                          "max_docs_per_request": 8}}), flush=True)
    elif msg.get("type") == "consolidate":
        edges = msg.get("cluster", {{}}).get("edges", [])
        verdicts = []
        for e in edges:
            ref = e.get("edge_ref")
            if ref == "edge-acme":
                verdicts.append(json.loads('{{"edge_ref": "edge-acme", "verdict": "invalidate", "t_invalid": {t_invalid_literal}}}'))
            else:
                verdicts.append({{"edge_ref": ref, "verdict": "keep"}})
        print(json.dumps({{"protocol": P, "type": "result",
                          "request_id": msg.get("request_id"), "verdicts": verdicts}}), flush=True)
"#
    );
    let cmd = ["python3".to_string(), "-c".to_string(), harness];
    let cmd_refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
    opened.engine.consolidate_with_provider(&cmd_refs, &axes())
}

/// A malformed (non-ISO-8601) STRING `t_invalid` on an `invalidate` verdict is a
/// PROVIDER protocol fault → `Err(EngineError::Consolidator)`, not the
/// user/extractor `InvalidArgument`.
#[test]
fn malformed_string_t_invalid_is_a_consolidator_fault() {
    let result = run_with_bad_t_invalid("fix3_malformed_string", "\"not-a-timestamp\"");
    assert!(
        matches!(result, Err(EngineError::Consolidator)),
        "a malformed consolidation t_invalid must return Err(Consolidator) (provider boundary), \
         got {result:?}"
    );
}

/// A non-string (JSON number) `t_invalid` on an `invalidate` verdict is likewise
/// a PROVIDER protocol fault → `Err(EngineError::Consolidator)`.
#[test]
fn non_string_t_invalid_is_a_consolidator_fault() {
    let result = run_with_bad_t_invalid("fix3_non_string", "12345");
    assert!(
        matches!(result, Err(EngineError::Consolidator)),
        "a non-string consolidation t_invalid must return Err(Consolidator) (provider boundary), \
         got {result:?}"
    );
}
