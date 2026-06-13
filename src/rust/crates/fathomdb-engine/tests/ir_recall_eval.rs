//! IR-B (IR-1 **Phase 2 / CODE**) — Evidence Recall@K unit tests + a wiring
//! smoke for the retrieval-mode experiment runner.
//!
//! Spec: `dev/design/ir-recall-measure.md` (Claude↔codex consensus-signed).
//! This target is corpus-INDEPENDENT and runs in the DEFAULT `cargo test`
//! pass (no `operator`/`default-embedder` feature, no `AGENT_LONG`, no corpus):
//!   - the measure-math unit tests use synthetic in-code fixtures with KNOWN
//!     Evidence Recall@K, so they pin the math directly;
//!   - the loader/validator test reads the illustrative gold-set fixture;
//!   - the experiment-runner smoke exercises the mode×K×class loop end-to-end
//!     against the REAL `Engine::search` path using the deterministic synthetic
//!     `VaryingEmbedder`. It asserts STRUCTURAL validity only (values in [0,1],
//!     wiring runs) — the synthetic embedder has NO semantic relevance, so it is
//!     emphatically NOT a relevance measurement. Real numbers are DEFERRED to
//!     the COR-2 corpus freeze (IR-C) — see
//!     `dev/plans/runs/IR-B-deferred-on-corpus-freeze.md`.

#[path = "support/corpus_subset.rs"]
mod corpus_subset;
#[path = "support/ir_eval.rs"]
mod ir_eval;

use std::collections::HashMap;
use std::path::PathBuf;

use corpus_subset::Doc;
use ir_eval::{
    evaluate_gold_set, evidence_recall_at_k, experiment_to_json, load_gold_set, negative_abstained,
    parse_gold_set, required_doc_ids, run_experiment, run_mode_bodies, validate_gold_set,
    EvidenceUnit, GoldQuery, GoldSet, Locator, Necessity, QueryClass, QueryOrigin, RetrievalMode,
    Span, HEADLINE_K, K_LADDER, RUNNABLE_NOW_MODES,
};

// ── Test constructors (keep the unit tests terse) ───────────────────────────

fn ev(id: &str, doc: &str, nec: Necessity) -> EvidenceUnit {
    EvidenceUnit {
        evidence_id: id.to_string(),
        doc_id: doc.to_string(),
        necessity: nec,
        locator: Some(Locator { kind: "whole_body".to_string(), spans: None }),
    }
}

fn gq(id: &str, class: QueryClass, required: Vec<EvidenceUnit>, legacy: &[&str]) -> GoldQuery {
    GoldQuery {
        query: format!("synthetic query {id}"),
        query_id: Some(id.to_string()),
        query_class: class,
        required_evidence: required,
        expected_top_k_doc_ids: legacy.iter().map(|s| s.to_string()).collect(),
        relation_type: None,
        chain_shape: None,
        source: None,
        answer_type: None,
        query_origin: QueryOrigin::HumanDataset,
    }
}

fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

// ── (a) Strict all-of + graded share the required-only denominator ──────────

#[test]
fn strict_is_all_of_graded_is_fraction_same_denominator() {
    // required = {A, B}, supporting = {C}.
    let q = gq(
        "q1",
        QueryClass::Commitment,
        vec![
            ev("e-a", "A", Necessity::Required),
            ev("e-b", "B", Necessity::Required),
            ev("e-c", "C", Necessity::Supporting),
        ],
        &["A", "B", "C"],
    );
    assert_eq!(required_doc_ids(&q), ["A".to_string(), "B".to_string()].into_iter().collect());

    // Only A retrieved: strict miss, graded = 1/2; supporting C present → 1.0.
    let m = evidence_recall_at_k(&q, &ids(&["A", "C", "Z"]), 10);
    assert_eq!(m.strict, 0.0);
    assert_eq!(m.graded, 0.5);
    assert_eq!(m.supporting_coverage, 1.0);
    assert_eq!((m.required_n, m.required_hits), (2, 1));

    // Both required retrieved: strict hit, graded = 1.0.
    let m = evidence_recall_at_k(&q, &ids(&["A", "B"]), 10);
    assert_eq!(m.strict, 1.0);
    assert_eq!(m.graded, 1.0);
}

#[test]
fn strict_all_of_respects_the_k_cut() {
    // required = {A, B}; B is buried at rank 6.
    let q = gq(
        "q2",
        QueryClass::ExactFact,
        vec![ev("e-a", "A", Necessity::Required), ev("e-b", "B", Necessity::Required)],
        &[],
    );
    let ranked = ids(&["A", "x1", "x2", "x3", "x4", "B"]);
    // @5: B not in top-5 → strict 0, graded 0.5.
    let m5 = evidence_recall_at_k(&q, &ranked, 5);
    assert_eq!((m5.strict, m5.graded), (0.0, 0.5));
    // @10: B in top-10 → strict 1.
    let m10 = evidence_recall_at_k(&q, &ranked, 10);
    assert_eq!((m10.strict, m10.graded), (1.0, 1.0));
}

#[test]
fn supporting_evidence_is_in_neither_recall_number() {
    // required = {A}; supporting = {B}. Retrieve ONLY the supporting unit.
    let q = gq(
        "q3",
        QueryClass::Action,
        vec![ev("e-a", "A", Necessity::Required), ev("e-b", "B", Necessity::Supporting)],
        &[],
    );
    let m = evidence_recall_at_k(&q, &ids(&["B"]), 10);
    assert_eq!(m.strict, 0.0, "supporting hit must NOT satisfy strict recall");
    assert_eq!(m.graded, 0.0, "supporting hit must NOT inflate graded recall");
    assert_eq!(m.supporting_coverage, 1.0, "but it IS counted in supporting-coverage");
}

// ── (f) Legacy eu8 reduction: expected_top_k_doc_ids as the fallback unit ───

#[test]
fn legacy_entry_reduces_to_doc_id_qrels() {
    // No required_evidence at all → denominator falls back to the eu8 doc-ids.
    let q = gq("q4", QueryClass::ExactFact, vec![], &["R1", "R2"]);
    assert_eq!(required_doc_ids(&q), ["R1".to_string(), "R2".to_string()].into_iter().collect());
    assert_eq!(evidence_recall_at_k(&q, &ids(&["R1", "R2"]), 10).strict, 1.0);
    assert_eq!(evidence_recall_at_k(&q, &ids(&["R1"]), 10).strict, 0.0);
    // An evidence-labelled set is NEVER augmented by the legacy doc-ids:
    let q2 = gq("q4b", QueryClass::Action, vec![ev("e", "A", Necessity::Required)], &["R1", "R2"]);
    assert_eq!(required_doc_ids(&q2), ["A".to_string()].into_iter().collect());
}

// ── (d) Negative class = abstention-correctness, kept OUT of recall ─────────

#[test]
fn negative_class_is_abstention_correctness() {
    assert!(negative_abstained(&[], 10), "empty top-K = correct abstention");
    assert!(!negative_abstained(&ids(&["x"]), 10), "any result = false positive");

    let gold = GoldSet {
        corpus_hash: "deadbeef".to_string(),
        qrels_version: "v1".to_string(),
        note: None,
        queries: vec![gq("neg", QueryClass::Negative, vec![], &[])],
    };
    // Retrieval returns a result → false positive; negative is NOT in overall recall.
    let res = evaluate_gold_set(&gold, &[HEADLINE_K], |_| Ok(ids(&["x"]))).expect("no error");
    let k = &res[&HEADLINE_K];
    assert_eq!(k.overall.n, 0, "negative queries excluded from the recall mean");
    assert_eq!(k.negative.n, 1);
    assert_eq!(k.negative.abstained, 0);
    assert_eq!(k.negative.false_positive_rate(), 1.0);
}

// ── [P2] A failed retrieval is SURFACED, never scored ───────────────────────
// codex §9 [P2]: when a runnable mode's retrieval errors, the runner used to
// fold the `Err` into an empty result set — scoring it as ordinary misses, and a
// NEGATIVE query as a (wrong) "correct abstention", so a real storage/retrieval
// failure produced a valid-looking JSON report. The error must propagate
// instead; only a successful empty retrieval (`Ok(vec![])`) is scored.

#[test]
fn failed_retrieval_is_surfaced_not_scored_as_misses_or_abstention() {
    let gold = GoldSet {
        corpus_hash: "deadbeef".to_string(),
        qrels_version: "v1".to_string(),
        note: None,
        queries: vec![
            gq("pos", QueryClass::Commitment, vec![ev("e1", "A", Necessity::Required)], &[]),
            gq("neg", QueryClass::Negative, vec![], &[]),
        ],
    };

    // A retrieval that ERRORS must abort with the real error — NOT be scored as a
    // miss (positive query) nor a correct abstention (negative query).
    let err = evaluate_gold_set(&gold, &K_LADDER, |_| {
        Err("search: simulated storage failure".to_string())
    })
    .expect_err("a failed retrieval must surface, not score");
    assert!(err.contains("simulated storage failure"), "real error surfaced verbatim: {err}");

    // Contrast — the only path that changed is `Err`: a genuinely empty SUCCESS
    // (`Ok(vec![])`) is still scored normally. The positive query is an honest
    // miss; the negative query is a correct abstention.
    let res =
        evaluate_gold_set(&gold, &K_LADDER, |_| Ok(Vec::new())).expect("empty success scores");
    for &k in &K_LADDER {
        let r = &res[&k];
        assert_eq!(r.overall.n, 1, "the positive query is scored at K={k}");
        assert_eq!(r.overall.strict(), 0.0, "empty success = honest miss (not an error) at K={k}");
        assert_eq!(r.negative.n, 1, "the negative query is counted at K={k}");
        assert_eq!(r.negative.abstained, 1, "empty success = correct abstention at K={k}");
    }
}

// ── (c)/(d) K-ladder + per-class aggregation ────────────────────────────────

#[test]
fn evaluate_gold_set_aggregates_per_k_and_per_class() {
    let gold = GoldSet {
        corpus_hash: "deadbeef".to_string(),
        qrels_version: "v1".to_string(),
        note: None,
        queries: vec![
            gq("c1", QueryClass::Commitment, vec![ev("e1", "A", Necessity::Required)], &[]),
            gq("a1", QueryClass::Action, vec![ev("e2", "C", Necessity::Required)], &[]),
            gq("n1", QueryClass::Negative, vec![], &[]),
        ],
    };
    // c1 hits (A retrieved), a1 misses (C absent), n1 correctly abstains.
    let res = evaluate_gold_set(&gold, &K_LADDER, |q| {
        Ok(match q.query_id.as_deref() {
            Some("c1") => ids(&["A", "z"]),
            Some("a1") => ids(&["z"]),
            _ => Vec::new(),
        })
    })
    .expect("no error");
    for &k in &K_LADDER {
        let r = &res[&k];
        assert_eq!(r.overall.n, 2, "two non-negative queries at K={k}");
        assert_eq!(r.overall.strict(), 0.5, "1 of 2 strict at K={k}");
        assert_eq!(r.per_class[&QueryClass::Commitment].strict(), 1.0);
        assert_eq!(r.per_class[&QueryClass::Action].strict(), 0.0);
        assert_eq!(r.negative.false_positive_rate(), 0.0, "abstained correctly");
    }
}

// ── (b)/(f) Validator ───────────────────────────────────────────────────────

#[test]
fn validator_catches_schema_and_methodology_violations() {
    // non-negative with empty required denominator + negative with a non-empty
    // one + duplicate query_id + duplicate evidence_id + missing corpus_hash.
    let bad = GoldSet {
        corpus_hash: String::new(),
        qrels_version: "v1".to_string(),
        note: None,
        queries: vec![
            gq("dup", QueryClass::ExactFact, vec![], &[]), // empty required → issue
            gq("dup", QueryClass::Negative, vec![ev("x", "A", Necessity::Required)], &[]), // dup id + negative non-empty
            GoldQuery {
                query: "q".to_string(),
                query_id: Some("ev-dup".to_string()),
                query_class: QueryClass::Action,
                required_evidence: vec![
                    ev("same", "A", Necessity::Required),
                    ev("same", "B", Necessity::Required), // duplicate evidence_id
                ],
                expected_top_k_doc_ids: vec![],
                relation_type: None,
                chain_shape: None,
                source: None,
                answer_type: None,
                query_origin: QueryOrigin::HumanDataset,
            },
        ],
    };
    let issues = validate_gold_set(&bad);
    assert!(issues.iter().any(|i| i.contains("corpus_hash missing")), "{issues:?}");
    assert!(issues.iter().any(|i| i.contains("non-negative")), "{issues:?}");
    assert!(issues.iter().any(|i| i.contains("negative class must have an EMPTY")), "{issues:?}");
    assert!(issues.iter().any(|i| i.contains("duplicate query_id")), "{issues:?}");
    assert!(issues.iter().any(|i| i.contains("duplicate evidence_id")), "{issues:?}");

    // A clean, pinned set has no issues.
    let good = GoldSet {
        corpus_hash: "abc123".to_string(),
        qrels_version: "v1".to_string(),
        note: None,
        queries: vec![
            gq("ok1", QueryClass::Commitment, vec![ev("e", "A", Necessity::Required)], &[]),
            gq("ok2", QueryClass::Negative, vec![], &[]),
        ],
    };
    assert!(validate_gold_set(&good).is_empty(), "{:?}", validate_gold_set(&good));
}

// ── (b) Loader on the illustrative fixture ──────────────────────────────────

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ir_gold/synthetic_gold.json")
}

#[test]
fn synthetic_fixture_loads_parses_and_is_structurally_valid_but_unpinned() {
    let gold = load_gold_set(&fixture_path()).expect("load synthetic gold fixture");
    assert_eq!(gold.queries.len(), 6, "fixture has 6 illustrative queries");
    assert_eq!(gold.qrels_version, "ir-b-synthetic-v0");

    // every class is represented, incl. the negative ("not-found") subset.
    let classes: std::collections::BTreeSet<_> =
        gold.queries.iter().map(|q| q.query_class).collect();
    for c in [
        QueryClass::Commitment,
        QueryClass::Action,
        QueryClass::ExactFact,
        QueryClass::Preference,
        QueryClass::Exploratory,
        QueryClass::Negative,
    ] {
        assert!(classes.contains(&c), "fixture must cover class {}", c.label());
    }

    // The ONLY validation issue is the deliberate un-pinned placeholder hash:
    // structurally clean, but NOT pinned to a frozen corpus (COR-2).
    let issues = validate_gold_set(&gold);
    assert_eq!(issues.len(), 1, "expected only the unpinned-placeholder flag: {issues:?}");
    assert!(issues[0].contains("placeholder"), "{issues:?}");
}

// ── WI-2 / WI-3a: query tracers + evidence-span locators ────────────────────
// (IR-C test-query-quality instrumentation — schema half.)

/// Parse a single query from a minimal one-query gold set.
fn parse_one(query_json: &str) -> Result<GoldQuery, String> {
    let set = format!(r#"{{"corpus_hash":"h","qrels_version":"v","queries":[{query_json}]}}"#);
    Ok(parse_gold_set(&set)?.queries.into_iter().next().expect("one query"))
}

const REQ_EV: &str =
    r#""required_evidence":[{"evidence_id":"e","doc_id":"D","necessity":"required"}]"#;

#[test]
fn parse_promotes_source_and_answer_type() {
    // Promoted non-underscore keys populate the struct.
    let q = parse_one(&format!(
        r#"{{"query":"q","query_id":"a","query_class":"exploratory",
            "source":"qmsum","answer_type":"summary",{REQ_EV}}}"#
    ))
    .expect("parse");
    assert_eq!(q.source.as_deref(), Some("qmsum"));
    assert_eq!(q.answer_type.as_deref(), Some("summary"));

    // Legacy underscore tracers still resolve via fallback (back-compat).
    let q = parse_one(&format!(
        r#"{{"query":"q","query_id":"b","query_class":"exact_fact",
            "_source":"enronqa","_answer_type":"span",{REQ_EV}}}"#
    ))
    .expect("parse");
    assert_eq!(q.source.as_deref(), Some("enronqa"));
    assert_eq!(q.answer_type.as_deref(), Some("span"));
}

#[test]
fn parse_query_origin_defaults_and_rejects_unknown() {
    // Absent ⇒ human_dataset (the reuse-tier default).
    let q = parse_one(&format!(
        r#"{{"query":"q","query_id":"a","query_class":"exact_fact",{REQ_EV}}}"#
    ))
    .expect("parse");
    assert_eq!(q.query_origin, QueryOrigin::HumanDataset);

    // Explicit values round-trip.
    for (s, want) in
        [("templated", QueryOrigin::Templated), ("llm_generated", QueryOrigin::LlmGenerated)]
    {
        let q = parse_one(&format!(
            r#"{{"query":"q","query_id":"a","query_class":"exact_fact","query_origin":"{s}",{REQ_EV}}}"#
        ))
        .expect("parse");
        assert_eq!(q.query_origin, want);
    }

    // Unknown origin is a hard parse error (fail fast, like query_class).
    let err = parse_one(&format!(
        r#"{{"query":"q","query_id":"a","query_class":"exact_fact","query_origin":"made_up",{REQ_EV}}}"#
    ))
    .unwrap_err();
    assert!(err.contains("query_origin"), "{err}");
}

#[test]
fn parse_locator_spans_roundtrip() {
    let q = parse_one(
        r#"{"query":"q","query_id":"a","query_class":"exact_fact",
            "required_evidence":[{"evidence_id":"e","doc_id":"D","necessity":"required",
              "locator":{"kind":"span","spans":[{"doc_id":"D","start":10,"end":42}]}}]}"#,
    )
    .expect("parse");
    let loc = q.required_evidence[0].locator.as_ref().expect("locator");
    assert_eq!(loc.kind, "span");
    let spans = loc.spans.as_ref().expect("spans present");
    assert_eq!(spans.len(), 1);
    assert_eq!((spans[0].start, spans[0].end), (10, 42));
    assert_eq!(spans[0].doc_id, "D");

    // A whole_body locator carries no spans.
    let q = parse_one(
        r#"{"query":"q","query_id":"a","query_class":"exact_fact",
            "required_evidence":[{"evidence_id":"e","doc_id":"D","necessity":"required",
              "locator":{"kind":"whole_body"}}]}"#,
    )
    .expect("parse");
    assert!(q.required_evidence[0].locator.as_ref().unwrap().spans.is_none());
}

#[test]
fn validate_flags_bad_span_bounds_and_doc_mismatch() {
    let span = |doc: &str, start: usize, end: usize| Span { doc_id: doc.to_string(), start, end };
    let bad = GoldSet {
        corpus_hash: "h".to_string(),
        qrels_version: "v".to_string(),
        note: None,
        queries: vec![GoldQuery {
            query: "q".to_string(),
            query_id: Some("a".to_string()),
            query_class: QueryClass::ExactFact,
            required_evidence: vec![EvidenceUnit {
                evidence_id: "e".to_string(),
                doc_id: "D".to_string(),
                necessity: Necessity::Required,
                locator: Some(Locator {
                    kind: "span".to_string(),
                    spans: Some(vec![span("D", 50, 10), span("OTHER", 0, 5)]),
                }),
            }],
            expected_top_k_doc_ids: vec![],
            relation_type: None,
            chain_shape: None,
            source: None,
            answer_type: None,
            query_origin: QueryOrigin::HumanDataset,
        }],
    };
    let issues = validate_gold_set(&bad);
    assert!(issues.iter().any(|i| i.contains("end<start")), "{issues:?}");
    assert!(issues.iter().any(|i| i.contains("!= evidence doc_id")), "{issues:?}");
}

// ── (e) Retrieval-mode metadata ─────────────────────────────────────────────

#[test]
fn retrieval_mode_runnable_now_flags() {
    assert!(RetrievalMode::RrfHybrid.is_runnable_now());
    assert!(RetrievalMode::VectorOnly.is_runnable_now());
    assert!(RetrievalMode::RerankStub.is_runnable_now());
    // FTS/BM25 baselines need harness FTS5 SQL + the frozen corpus → deferred.
    assert!(!RetrievalMode::FtsWriteCursor.is_runnable_now());
    assert!(!RetrievalMode::Bm25Fts.is_runnable_now());
    assert_eq!(RUNNABLE_NOW_MODES.len(), 3);
}

// ── Experiment-runner WIRING smoke (synthetic embedder; NOT a measurement) ──

fn smoke_doc(doc_id: &str, body: &str) -> Doc {
    Doc {
        doc_id: doc_id.to_string(),
        source_type: "doc".to_string(),
        title: None,
        body: body.to_string(),
        parent_doc_id: None,
        tags: vec![],
        relation_hint: None,
    }
}

#[test]
fn experiment_runner_mode_k_class_loop_wires_end_to_end() {
    // Tiny in-code corpus + the deterministic synthetic VaryingEmbedder. This
    // proves the mode×K×class loop runs against the REAL Engine::search seam and
    // produces a STRUCTURALLY valid result. It is NOT a relevance measurement —
    // VaryingEmbedder has no semantics; real numbers are DEFERRED to COR-2.
    let docs = vec![
        smoke_doc("d-A", "alpha commitment delivery friday"),
        smoke_doc("d-B", "beta obligation parties due date"),
        smoke_doc("d-C", "gamma next action review thread"),
        smoke_doc("d-D", "delta unrelated filler body"),
    ];
    let (_dir, engine) = corpus_subset::fixture_engine();
    corpus_subset::ingest(&engine, &docs);
    let body_to_doc: HashMap<String, String> =
        docs.iter().map(|d| (d.body.clone(), d.doc_id.clone())).collect();

    let gold = GoldSet {
        corpus_hash: "smoke".to_string(),
        qrels_version: "smoke-v0".to_string(),
        note: None,
        queries: vec![
            gq("s-commit", QueryClass::Commitment, vec![ev("e1", "d-A", Necessity::Required)], &[]),
            gq("s-act", QueryClass::Action, vec![ev("e2", "d-C", Necessity::Required)], &[]),
            gq("s-neg", QueryClass::Negative, vec![], &[]),
        ],
    };

    let modes =
        [RetrievalMode::RrfHybrid, RetrievalMode::VectorOnly, RetrievalMode::FtsWriteCursor];
    let result = run_experiment(&engine, &gold, &body_to_doc, &modes, &K_LADDER)
        .expect("synthetic retrieval succeeds");

    // The two runnable modes ran; the FTS mode is recorded as deferred.
    assert_eq!(result.per_mode.len(), 2, "RrfHybrid + VectorOnly ran");
    assert!(result.per_mode.contains_key(&RetrievalMode::RrfHybrid));
    assert!(result.per_mode.contains_key(&RetrievalMode::VectorOnly));
    assert_eq!(result.deferred_modes, vec![RetrievalMode::FtsWriteCursor]);
    assert!(result.fanout >= *K_LADDER.iter().max().unwrap());

    // Every aggregate is a well-formed fraction; the full K-ladder is present.
    for by_k in result.per_mode.values() {
        for &k in &K_LADDER {
            let r = &by_k[&k];
            assert_eq!(r.overall.n, 2, "two non-negative queries");
            for v in [r.overall.strict(), r.overall.graded(), r.overall.supporting()] {
                assert!((0.0..=1.0).contains(&v), "aggregate {v} out of [0,1] at K={k}");
            }
            assert_eq!(r.negative.n, 1);
            assert!((0.0..=1.0).contains(&r.negative.false_positive_rate()));
        }
    }

    // The deferred modes return the COR-2 marker (not a silent empty result).
    let err = run_mode_bodies(&engine, "q", RetrievalMode::Bm25Fts).unwrap_err();
    assert!(err.contains("TODO(COR-2-freeze)"), "{err}");

    // JSON shape IR-C/IR-2 will consume: structure only, no thresholds.
    let v = experiment_to_json(&gold, &result);
    assert_eq!(v["headline_k"], HEADLINE_K);
    assert!(v["per_mode"]["rrf_hybrid"]["10"]["overall"].is_object());
    assert_eq!(v["deferred_modes"][0], "fts_write_cursor");
}
