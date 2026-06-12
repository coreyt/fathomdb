//! IR-B (IR-1 **Phase 2 / CODE**) — Evidence Recall@K measure + gold-set schema.
//!
//! Implements the **corpus-INDEPENDENT** half of the Phase-1 consensus measure
//! `dev/design/ir-recall-measure.md`: the gold-set encoding schema (§(b)), the
//! Evidence Recall@K math (§(a) — strict all-of headline + graded diagnostic,
//! single `required`-only denominator), the K-ladder (§(c) — @5/@10/@20/@50,
//! headline @10), the per-class stratification (§(d)), and the retrieval-mode
//! plumbing (§(e)). It computes the metric GIVEN a gold set + retrieved results;
//! it MINTS NO AC, PICKS NO THRESHOLD, and touches NOTHING in the eu7/AC-075
//! fidelity gate or the engine vector path.
//!
//! ## What is DEFERRED to the COR-2 corpus freeze (NOT in this module)
//! - the **real** fact-level gold-set LABELS (which facts are required for which
//!   query) — Phase 3 / human-HITL labeling on the FROZEN corpus;
//! - the **real-corpus experiment RUNS** + any real recall numbers — IR-C;
//! - the production **FTS write-cursor** and **BM25-ordered** retrieval modes,
//!   which need harness-level FTS5 SQL + the frozen corpus to be meaningful
//!   (`TODO(COR-2-freeze)` in [`run_mode_bodies`]).
//!
//! The fixture under `tests/fixtures/ir_gold/synthetic_gold.json` is a tiny
//! **illustrative** gold set (synthetic, NOT real labels) used only to exercise
//! the loader/validator and pin the schema shape.
//!
//! Zero new crate dependencies: the gold set is parsed manually from
//! `serde_json::Value` (mirroring `support/corpus_subset.rs`), since the engine
//! test crate carries `serde_json` but not `serde` derive.

#![allow(dead_code)] // referenced by the sibling `ir_recall_eval.rs` test; cargo lints each include in isolation

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

use fathomdb_engine::{rerank_fused, Engine};
use serde_json::{json, Value};

// ── K-ladder (§(c)) ──────────────────────────────────────────────────────
/// The reported K-ladder: @5 (UX-proximal), @10 (HEADLINE / reporting
/// convention), @20 (reranker band), @50 (retriever-health). No threshold is
/// attached to any K here — pass/fail lines are Phase-4 experiment + IR-2/HITL.
pub const K_LADDER: [usize; 4] = [5, 10, 20, 50];
/// The headline K — the eval/reporting convention (NOT an API-enforced cap; the
/// eval applies the @K cut itself, see measure doc §(c) codex round-7).
pub const HEADLINE_K: usize = 10;
/// Vector phase-2 rerank fanout floor for the eval (≥ the deepest ladder K) so
/// @20/@50 vector/hybrid depths actually see >10 candidates (measure doc §(c)
/// codex round-8). A measurement-harness setting via `set_search_limit_for_test`
/// — never a production-behavior change.
pub const DEFAULT_FANOUT: usize = 50;

// ── Gold-set schema (§(b)) — additive superset of eu8 ground_truth_queries ──

/// Whether an evidence unit is part of the recall denominator (`required`) or a
/// separate corroboration diagnostic (`supporting`). Only `required` units gate
/// strict recall AND form the graded recall denominator; `supporting` is in
/// neither recall number (measure doc §(a)/(b)).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Necessity {
    Required,
    Supporting,
}

impl Necessity {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "required" => Some(Self::Required),
            "supporting" => Some(Self::Supporting),
            _ => None,
        }
    }
}

/// The six query classes (measure doc §(d)). `Negative` ("not-found") is scored
/// as abstention-correctness, NOT recall, and is reported separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum QueryClass {
    Commitment,
    Action,
    ExactFact,
    Preference,
    Exploratory,
    Negative,
}

impl QueryClass {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Commitment => "commitment",
            Self::Action => "action",
            Self::ExactFact => "exact_fact",
            Self::Preference => "preference",
            Self::Exploratory => "exploratory",
            Self::Negative => "negative",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "commitment" => Self::Commitment,
            "action" => Self::Action,
            "exact_fact" => Self::ExactFact,
            "preference" => Self::Preference,
            "exploratory" => Self::Exploratory,
            "negative" => Self::Negative,
            _ => return None,
        })
    }
}

/// Where the query text came from — *leakage provenance*. `human_dataset` is a
/// human/benchmark-authored question (no doc-text leakage); `templated` is
/// derived from the evidence doc (HIGH lexical-leakage risk — must be held to a
/// higher validation bar); `llm_generated` is model-synthesized (needs the
/// label-audit caveats in `dev/notes/IR-C-fact-level-gold-labels-research.md`).
/// Locked now so any synthetic query is flagged the moment it appears.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryOrigin {
    HumanDataset,
    LlmGenerated,
    Templated,
}

impl QueryOrigin {
    pub fn label(&self) -> &'static str {
        match self {
            Self::HumanDataset => "human_dataset",
            Self::LlmGenerated => "llm_generated",
            Self::Templated => "templated",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "human_dataset" => Self::HumanDataset,
            "llm_generated" => Self::LlmGenerated,
            "templated" => Self::Templated,
            _ => return None,
        })
    }
}

/// A character span within a doc body, carried through from the source QA
/// `evidence_spans`. NOT load-bearing for the doc-body-granularity score; it
/// powers the passage↔evidence-span overlap diagnostic (IR-C instrumentation
/// plan WI-3 — `dev/plans/IR-C-test-query-quality-instrumentation-plan.md`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub doc_id: String,
    pub start: usize,
    pub end: usize,
}

/// Provenance of an evidence unit. In a non-chunking store this is recorded for
/// label audit but is **NOT load-bearing for the score** (presence is at
/// doc-body granularity, measure doc §(b)).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Locator {
    /// "span" | "whole_body".
    pub kind: String,
    /// Char spans within the doc body (present when `kind == "span"`); `None` for
    /// `whole_body`. Carried from the dataset `evidence_spans` for span-level
    /// diagnostics — never affects the recall score.
    pub spans: Option<Vec<Span>>,
}

/// One atomic evidence unit — the unit of relevance (measure doc §(b)).
#[derive(Clone, Debug)]
pub struct EvidenceUnit {
    pub evidence_id: String,
    /// The node body that carries this fact (presence is tested at doc-body
    /// granularity).
    pub doc_id: String,
    pub necessity: Necessity,
    pub locator: Option<Locator>,
}

/// One gold query — an additive superset of today's eu8 `ground_truth_queries`
/// entry (the `query` / `expected_top_k_doc_ids` / `relation_type` /
/// `chain_shape` keys stay parseable by the eu8 reader).
#[derive(Clone, Debug)]
pub struct GoldQuery {
    pub query: String,
    pub query_id: Option<String>,
    pub query_class: QueryClass,
    /// The atomic-evidence layer (additive). May be empty for a legacy/eu8 entry
    /// (then the denominator falls back to `expected_top_k_doc_ids`, §(f)) or for
    /// a `Negative` query (abstention class).
    pub required_evidence: Vec<EvidenceUnit>,
    /// PRESERVED eu8 doc-id view — the legacy/fallback denominator.
    pub expected_top_k_doc_ids: Vec<String>,
    pub relation_type: Option<String>,
    pub chain_shape: Option<String>,
    /// Source dataset of the query (e.g. `qmsum`/`enronqa`/`qaconv`) — promoted
    /// from the build script's `_source` tracer (legacy `_source` still parses).
    /// `None` for legacy/synthetic sets. Enables per-source stratified reporting.
    pub source: Option<String>,
    /// Dataset answer type (e.g. `span`/`free_form`/`summary`/`abstain`),
    /// promoted from `_answer_type`. A difficulty signal — a `summary` over a
    /// long doc is lexically easy at whole-doc granularity.
    pub answer_type: Option<String>,
    /// Leakage provenance of the query text; defaults to `human_dataset` when the
    /// field is absent (the reuse tier). See [`QueryOrigin`].
    pub query_origin: QueryOrigin,
}

/// A versioned, corpus-pinned gold set. The pinning PRINCIPLE (measure doc §(f),
/// the GA-halt lesson): `corpus_hash` + `qrels_version` are recorded with every
/// set so a relevance number can never silently drift under corpus expansion.
/// (This module does NOT pick the snapshot — that is the downstream B-1 ruling +
/// corpus freeze; the fixture carries a `TODO(COR-2-freeze)` placeholder hash.)
#[derive(Clone, Debug)]
pub struct GoldSet {
    pub corpus_hash: String,
    pub qrels_version: String,
    pub note: Option<String>,
    pub queries: Vec<GoldQuery>,
}

/// Sentinel used by the synthetic fixture until the COR-2 freeze pins a real
/// corpus snapshot hash. The validator flags any set still carrying it as
/// fixture-only (un-pinned).
pub const UNPINNED_PLACEHOLDER: &str = "TODO(COR-2-freeze)";

// ── Loader (§(b)) ──────────────────────────────────────────────────────────

/// Load + parse a gold set from a JSON file on disk.
pub fn load_gold_set(path: &Path) -> Result<GoldSet, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    parse_gold_set(&text)
}

/// Parse a gold set from a JSON string. Manual `serde_json::Value` walk (no
/// `serde` derive dep), mirroring `corpus_subset::extract_ground_truth_queries`.
pub fn parse_gold_set(text: &str) -> Result<GoldSet, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("gold set JSON: {e}"))?;
    let corpus_hash = v.get("corpus_hash").and_then(Value::as_str).unwrap_or_default().to_string();
    let qrels_version =
        v.get("qrels_version").and_then(Value::as_str).unwrap_or_default().to_string();
    let note = v.get("note").and_then(Value::as_str).map(str::to_string);
    let arr = v
        .get("queries")
        .and_then(Value::as_array)
        .ok_or_else(|| "gold set: missing `queries` array".to_string())?;
    let mut queries = Vec::with_capacity(arr.len());
    for (i, q) in arr.iter().enumerate() {
        queries.push(parse_query(q).map_err(|e| format!("queries[{i}]: {e}"))?);
    }
    Ok(GoldSet { corpus_hash, qrels_version, note, queries })
}

fn parse_query(q: &Value) -> Result<GoldQuery, String> {
    // SAME `query` key the eu8 parser reads (additive-superset invariant §(b)).
    let query = q.get("query").and_then(Value::as_str).ok_or("missing `query`")?.to_string();
    let query_id = q.get("query_id").and_then(Value::as_str).map(str::to_string);
    let class_str = q.get("query_class").and_then(Value::as_str).ok_or("missing `query_class`")?;
    let query_class =
        QueryClass::parse(class_str).ok_or_else(|| format!("unknown query_class `{class_str}`"))?;

    let mut required_evidence = Vec::new();
    if let Some(units) = q.get("required_evidence").and_then(Value::as_array) {
        for (j, u) in units.iter().enumerate() {
            required_evidence
                .push(parse_evidence(u).map_err(|e| format!("required_evidence[{j}]: {e}"))?);
        }
    }
    let expected_top_k_doc_ids = q
        .get("expected_top_k_doc_ids")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let relation_type = q.get("relation_type").and_then(Value::as_str).map(str::to_string);
    let chain_shape = q.get("chain_shape").and_then(Value::as_str).map(str::to_string);
    // Tracers (WI-2): prefer the promoted non-underscore keys, fall back to the
    // build script's legacy `_source`/`_answer_type` for older gold files.
    let source =
        q.get("source").or_else(|| q.get("_source")).and_then(Value::as_str).map(str::to_string);
    let answer_type = q
        .get("answer_type")
        .or_else(|| q.get("_answer_type"))
        .and_then(Value::as_str)
        .map(str::to_string);
    // Unknown origin is a hard error (fail fast, like query_class); absent ⇒
    // human_dataset (the reuse tier — back-compat for pre-WI-2 gold).
    let query_origin = match q.get("query_origin").and_then(Value::as_str) {
        Some(s) => QueryOrigin::parse(s).ok_or_else(|| format!("unknown query_origin `{s}`"))?,
        None => QueryOrigin::HumanDataset,
    };

    Ok(GoldQuery {
        query,
        query_id,
        query_class,
        required_evidence,
        expected_top_k_doc_ids,
        relation_type,
        chain_shape,
        source,
        answer_type,
        query_origin,
    })
}

fn parse_evidence(u: &Value) -> Result<EvidenceUnit, String> {
    let evidence_id =
        u.get("evidence_id").and_then(Value::as_str).ok_or("missing `evidence_id`")?.to_string();
    let doc_id = u.get("doc_id").and_then(Value::as_str).ok_or("missing `doc_id`")?.to_string();
    let nec_str = u.get("necessity").and_then(Value::as_str).ok_or("missing `necessity`")?;
    let necessity =
        Necessity::parse(nec_str).ok_or_else(|| format!("unknown necessity `{nec_str}`"))?;
    let locator = u.get("locator").and_then(parse_locator);
    Ok(EvidenceUnit { evidence_id, doc_id, necessity, locator })
}

/// Parse a `locator` object: required `kind`, optional `spans` (WI-3a). A
/// malformed span entry is skipped (lenient, like the rest of the loader); span
/// *bounds* are checked by [`validate_gold_set`], not here.
fn parse_locator(v: &Value) -> Option<Locator> {
    let m = v.as_object()?;
    let kind = m.get("kind").and_then(Value::as_str)?.to_string();
    let spans = m
        .get("spans")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(parse_span).collect::<Vec<Span>>());
    Some(Locator { kind, spans })
}

fn parse_span(v: &Value) -> Option<Span> {
    let m = v.as_object()?;
    let doc_id = m.get("doc_id").and_then(Value::as_str)?.to_string();
    let start = m.get("start").and_then(Value::as_u64)? as usize;
    let end = m.get("end").and_then(Value::as_u64)? as usize;
    Some(Span { doc_id, start, end })
}

// ── Denominator derivation — the single seed unit-of-relevance (§(f)) ───────

/// The authored REQUIRED-evidence denominator for a query (the doc_ids that must
/// be present), as ONE unit of relevance (§(f) seed rule, codex round-5):
/// - if `required_evidence` is present, the `necessity=required` units ARE the
///   denominator (full stop) — even if that yields the empty set;
/// - ONLY when `required_evidence` is entirely absent do `expected_top_k_doc_ids`
///   map exactly once to degenerate whole-body required units (the eu8 reduction
///   fallback). They are NEVER added on top of an evidence-labelled set.
pub fn required_doc_ids(q: &GoldQuery) -> BTreeSet<String> {
    if q.required_evidence.is_empty() {
        return q.expected_top_k_doc_ids.iter().cloned().collect();
    }
    q.required_evidence
        .iter()
        .filter(|e| e.necessity == Necessity::Required)
        .map(|e| e.doc_id.clone())
        .collect()
}

/// The `supporting` doc_ids — corroborating context, in NEITHER recall number;
/// reported only as the separate supporting-coverage diagnostic (§(a)/(b)).
pub fn supporting_doc_ids(q: &GoldQuery) -> BTreeSet<String> {
    q.required_evidence
        .iter()
        .filter(|e| e.necessity == Necessity::Supporting)
        .map(|e| e.doc_id.clone())
        .collect()
}

// ── Per-query Evidence Recall@K (§(a)) ───────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerQueryRecall {
    /// 1.0 iff EVERY required unit is present in top-K, else 0.0 (the headline,
    /// all-or-nothing). An empty required set is vacuously 1.0 — the aggregator
    /// routes `Negative` queries away from recall and the validator rejects a
    /// non-negative query with an empty denominator, so this only bites
    /// mislabeled data the validator catches.
    pub strict: f64,
    /// `|required ∩ retrieved@K| / |required|` — the graded diagnostic over the
    /// SAME required-only denominator as `strict` (directly comparable).
    pub graded: f64,
    /// `|supporting ∩ retrieved@K| / |supporting|` — separate diagnostic, never
    /// in a recall number. NaN-free: 0.0 when there are no supporting units.
    pub supporting_coverage: f64,
    pub required_n: usize,
    pub required_hits: usize,
}

/// Evidence Recall@K for one query given the mode's ranked `retrieved_doc_ids`
/// (the harness cuts at `k`). Presence is at doc-body granularity (§(b)).
pub fn evidence_recall_at_k(
    q: &GoldQuery,
    retrieved_doc_ids: &[String],
    k: usize,
) -> PerQueryRecall {
    let topk: HashSet<&String> = retrieved_doc_ids.iter().take(k).collect();
    let required = required_doc_ids(q);
    let supporting = supporting_doc_ids(q);

    let required_n = required.len();
    let required_hits = required.iter().filter(|d| topk.contains(d)).count();
    let strict = if required_n == 0 || required_hits == required_n { 1.0 } else { 0.0 };
    let graded = if required_n == 0 { 1.0 } else { required_hits as f64 / required_n as f64 };

    let sup_hits = supporting.iter().filter(|d| topk.contains(d)).count();
    let supporting_coverage =
        if supporting.is_empty() { 0.0 } else { sup_hits as f64 / supporting.len() as f64 };

    PerQueryRecall { strict, graded, supporting_coverage, required_n, required_hits }
}

/// Abstention-correctness for a `Negative` ("not-found") query (§(d)): correct
/// iff the retrieval returned NOTHING in top-K. A non-empty top-K is a false
/// positive. Reported separately from recall.
pub fn negative_abstained(retrieved_doc_ids: &[String], k: usize) -> bool {
    retrieved_doc_ids.iter().take(k).next().is_none()
}

// ── Aggregation (per-class + overall + negative), per K (§(c)/(d)) ──────────

#[derive(Clone, Debug, Default)]
pub struct ClassAgg {
    pub n: usize,
    pub strict_sum: f64,
    pub graded_sum: f64,
    pub supporting_sum: f64,
}

impl ClassAgg {
    fn add(&mut self, m: &PerQueryRecall) {
        self.n += 1;
        self.strict_sum += m.strict;
        self.graded_sum += m.graded;
        self.supporting_sum += m.supporting_coverage;
    }
    pub fn strict(&self) -> f64 {
        if self.n == 0 {
            0.0
        } else {
            self.strict_sum / self.n as f64
        }
    }
    pub fn graded(&self) -> f64 {
        if self.n == 0 {
            0.0
        } else {
            self.graded_sum / self.n as f64
        }
    }
    pub fn supporting(&self) -> f64 {
        if self.n == 0 {
            0.0
        } else {
            self.supporting_sum / self.n as f64
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct NegativeAgg {
    pub n: usize,
    pub abstained: usize,
}

impl NegativeAgg {
    /// Fraction of negative queries that WRONGLY returned ≥1 result.
    pub fn false_positive_rate(&self) -> f64 {
        if self.n == 0 {
            0.0
        } else {
            (self.n - self.abstained) as f64 / self.n as f64
        }
    }
}

/// Evidence Recall aggregated at one K: overall (non-negative queries), per
/// class, and the negative-class abstention bucket.
#[derive(Clone, Debug)]
pub struct KResult {
    pub k: usize,
    pub overall: ClassAgg,
    pub per_class: BTreeMap<QueryClass, ClassAgg>,
    pub negative: NegativeAgg,
}

impl KResult {
    fn new(k: usize) -> Self {
        Self {
            k,
            overall: ClassAgg::default(),
            per_class: BTreeMap::new(),
            negative: NegativeAgg::default(),
        }
    }
}

/// Compute Evidence Recall@K across the K-ladder for the whole gold set, given a
/// retrieval closure that returns the mode's ranked doc_ids for a query (called
/// ONCE per query; cut at each K internally). `Negative` queries are routed to
/// the abstention bucket and kept OUT of the recall means.
///
/// The closure returns a `Result`: a successful **empty** retrieval (`Ok(vec![])`)
/// is scored normally (an honest miss, or a correct abstention for a negative
/// query), but a **failed** retrieval (`Err`) is NEVER scored — it propagates and
/// aborts the run so a storage/retrieval failure or a malformed gold query can
/// never be silently folded into "empty results scored as misses/abstention".
pub fn evaluate_gold_set<F>(
    gold: &GoldSet,
    ladder: &[usize],
    mut retrieve: F,
) -> Result<BTreeMap<usize, KResult>, String>
where
    F: FnMut(&GoldQuery) -> Result<Vec<String>, String>,
{
    let mut out: BTreeMap<usize, KResult> = ladder.iter().map(|&k| (k, KResult::new(k))).collect();
    for q in &gold.queries {
        let retrieved = retrieve(q)?;
        for &k in ladder {
            let r = out.get_mut(&k).expect("ladder key");
            if q.query_class == QueryClass::Negative {
                r.negative.n += 1;
                if negative_abstained(&retrieved, k) {
                    r.negative.abstained += 1;
                }
            } else {
                let m = evidence_recall_at_k(q, &retrieved, k);
                r.overall.add(&m);
                r.per_class.entry(q.query_class).or_default().add(&m);
            }
        }
    }
    Ok(out)
}

// ── Validator (§(b)/(f)) ─────────────────────────────────────────────────────

/// Validate a gold set against the schema + methodology invariants. Returns a
/// list of human-readable issues (empty ⇒ valid). Checks:
/// - pinning principle (§(f)): `corpus_hash` + `qrels_version` present (the
///   `TODO(COR-2-freeze)` placeholder is flagged as fixture-only, not fatal);
/// - non-empty query text; unique `query_id`; unique `evidence_id` within a
///   query; non-empty `doc_id` on each unit;
/// - class/denominator coherence: a non-`Negative` query MUST have a non-empty
///   REQUIRED denominator; a `Negative` query MUST have an empty one (abstention).
pub fn validate_gold_set(gold: &GoldSet) -> Vec<String> {
    let mut issues = Vec::new();
    if gold.corpus_hash.trim().is_empty() {
        issues.push("corpus_hash missing (pinning principle §(f))".to_string());
    } else if gold.corpus_hash == UNPINNED_PLACEHOLDER {
        issues.push(format!(
            "corpus_hash is the `{UNPINNED_PLACEHOLDER}` placeholder — fixture-only, NOT pinned to a frozen snapshot"
        ));
    }
    if gold.qrels_version.trim().is_empty() {
        issues.push("qrels_version missing (pinning principle §(f))".to_string());
    }

    let mut seen_qids: HashSet<&str> = HashSet::new();
    for (i, q) in gold.queries.iter().enumerate() {
        let qid = q.query_id.as_deref().unwrap_or("<no query_id>");
        let where_ = format!("query[{i}] ({qid})");
        if q.query.trim().is_empty() {
            issues.push(format!("{where_}: empty query text"));
        }
        if let Some(id) = q.query_id.as_deref() {
            if !seen_qids.insert(id) {
                issues.push(format!("{where_}: duplicate query_id `{id}`"));
            }
        }
        let mut seen_ev: HashSet<&str> = HashSet::new();
        for e in &q.required_evidence {
            if e.doc_id.trim().is_empty() {
                issues.push(format!("{where_}: evidence `{}` has empty doc_id", e.evidence_id));
            }
            if !seen_ev.insert(&e.evidence_id) {
                issues.push(format!("{where_}: duplicate evidence_id `{}`", e.evidence_id));
            }
            // WI-3a: span bounds + the span's doc_id must match its evidence unit.
            for s in e.locator.iter().flat_map(|l| l.spans.iter().flatten()) {
                if s.end < s.start {
                    issues.push(format!(
                        "{where_}: evidence `{}` span has end<start ({}..{})",
                        e.evidence_id, s.start, s.end
                    ));
                }
                if s.doc_id != e.doc_id {
                    issues.push(format!(
                        "{where_}: evidence `{}` span doc_id `{}` != evidence doc_id `{}`",
                        e.evidence_id, s.doc_id, e.doc_id
                    ));
                }
            }
        }
        let req = required_doc_ids(q);
        match q.query_class {
            QueryClass::Negative => {
                if !req.is_empty() {
                    issues.push(format!(
                        "{where_}: negative class must have an EMPTY required denominator (abstention)"
                    ));
                }
            }
            _ => {
                if req.is_empty() {
                    issues.push(format!(
                        "{where_}: non-negative class `{}` has an EMPTY required denominator",
                        q.query_class.label()
                    ));
                }
            }
        }
    }
    issues
}

// ── Retrieval-mode plumbing (§(e)) ───────────────────────────────────────────

/// The retrieval modes the measure compares (§(e)). `is_runnable_now` flags the
/// modes wired in Phase 2 via existing engine seams vs. those deferred to the
/// COR-2 freeze (they need harness-level FTS5 SQL + the frozen corpus).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RetrievalMode {
    /// Production RRF-hybrid fused ranking — the HEADLINE mode (`Engine::search`).
    RrfHybrid,
    /// Vector-only (bit-KNN K=192 → f32 rerank) via `set_vector_stage_only_for_test`.
    VectorOnly,
    /// +reranker seam — `rerank_fused` is an IDENTITY stub today, so this is
    /// aspirational / report-only until a real reranker lands (NOT a gate input).
    RerankStub,
    /// Production FTS `MATCH` branch (write-cursor order). TODO(COR-2-freeze):
    /// needs harness FTS5 SQL + the frozen corpus.
    FtsWriteCursor,
    /// BM25-ranked FTS-only baseline (`ORDER BY bm25(search_index) ASC`).
    /// TODO(COR-2-freeze): needs harness FTS5 SQL + the frozen corpus.
    Bm25Fts,
}

impl RetrievalMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::RrfHybrid => "rrf_hybrid",
            Self::VectorOnly => "vector_only",
            Self::RerankStub => "rerank_stub",
            Self::FtsWriteCursor => "fts_write_cursor",
            Self::Bm25Fts => "bm25_fts",
        }
    }
    /// True for the modes wired through existing engine seams in Phase 2.
    pub fn is_runnable_now(&self) -> bool {
        matches!(self, Self::RrfHybrid | Self::VectorOnly | Self::RerankStub)
    }
}

/// The Phase-2 runnable-now modes (the others are TODO(COR-2-freeze)).
pub const RUNNABLE_NOW_MODES: [RetrievalMode; 3] =
    [RetrievalMode::RrfHybrid, RetrievalMode::VectorOnly, RetrievalMode::RerankStub];

/// Run a single retrieval mode for `query` against `engine`, returning the
/// ranked list of result BODIES (the caller maps bodies→doc_ids, mirroring eu8).
/// Reuses existing engine seams ONLY — no engine behavior change:
/// - `RrfHybrid`: `Engine::search` (the unconditional fused ranking);
/// - `VectorOnly`: toggles `set_vector_stage_only_for_test` around `search`;
/// - `RerankStub`: fused hits then the `rerank_fused` identity seam;
/// - `FtsWriteCursor` / `Bm25Fts`: **deferred** — return `Err` carrying the
///   `TODO(COR-2-freeze)` marker (need harness FTS5 SQL + the frozen corpus).
///
/// Before calling, the experiment runner raises the vector fanout to ≥ the
/// deepest ladder K via `set_search_limit_for_test` (see [`DEFAULT_FANOUT`]) —
/// a measurement-harness setting, never a production change (measure doc §(c)).
pub fn run_mode_bodies(
    engine: &Engine,
    query: &str,
    mode: RetrievalMode,
) -> Result<Vec<String>, String> {
    match mode {
        RetrievalMode::RrfHybrid => search_bodies(engine, query),
        RetrievalMode::VectorOnly => {
            engine.set_vector_stage_only_for_test(true);
            let r = search_bodies(engine, query);
            engine.set_vector_stage_only_for_test(false);
            r
        }
        RetrievalMode::RerankStub => {
            let res = engine.search(query).map_err(|e| format!("search: {e:?}"))?;
            // rerank_fused is an identity stub today (lib.rs) — documents the
            // seam; produces the same order as RrfHybrid until a real reranker.
            Ok(rerank_fused(res.results).into_iter().map(|h| h.body).collect())
        }
        RetrievalMode::FtsWriteCursor | RetrievalMode::Bm25Fts => Err(format!(
            "mode `{}` deferred: TODO(COR-2-freeze) — needs harness FTS5 SQL + frozen corpus",
            mode.label()
        )),
    }
}

fn search_bodies(engine: &Engine, query: &str) -> Result<Vec<String>, String> {
    Ok(engine
        .search(query)
        .map_err(|e| format!("search: {e:?}"))?
        .results
        .into_iter()
        .map(|h| h.body)
        .collect())
}

// ── Experiment runner scaffold (§ the mode×K×class loop) ─────────────────────

/// Result of one experiment: per runnable mode, the per-K aggregates.
pub struct ExperimentResult {
    pub fanout: usize,
    pub per_mode: BTreeMap<RetrievalMode, BTreeMap<usize, KResult>>,
    /// Modes that were requested but are deferred (TODO(COR-2-freeze)).
    pub deferred_modes: Vec<RetrievalMode>,
}

/// Run the mode×K×class loop over a gold set. For each runnable mode it raises
/// the vector fanout to `≥ max(ladder)`, retrieves once per query, maps bodies→
/// doc_ids via `body_to_doc_id`, and aggregates with [`evaluate_gold_set`].
/// Deferred modes (FTS/BM25) are recorded in `deferred_modes`, not run.
///
/// This is the WIRED experiment scaffold. The **real** run (frozen corpus, real
/// labels, in `--release`) is IR-C / deferred; see
/// `dev/plans/runs/IR-B-deferred-on-corpus-freeze.md`.
///
/// A runnable mode's retrieval error (e.g. `Engine::search` returning `Err`) is
/// **surfaced, never scored**: it propagates out of this fn and aborts the run
/// with the real error, so a failed retrieval can never masquerade as an empty
/// result set (ordinary misses, or a correct abstention for a negative query) in
/// the measured JSON report. A genuinely empty *successful* retrieval is still
/// scored normally — only the `Err` path aborts.
pub fn run_experiment(
    engine: &Engine,
    gold: &GoldSet,
    body_to_doc_id: &HashMap<String, String>,
    modes: &[RetrievalMode],
    ladder: &[usize],
) -> Result<ExperimentResult, String> {
    let deepest = ladder.iter().copied().max().unwrap_or(HEADLINE_K);
    let fanout = deepest.max(DEFAULT_FANOUT);
    engine.set_search_limit_for_test(fanout);

    let mut per_mode = BTreeMap::new();
    let mut deferred_modes = Vec::new();
    for &mode in modes {
        if !mode.is_runnable_now() {
            deferred_modes.push(mode);
            continue;
        }
        let result = evaluate_gold_set(gold, ladder, |q| {
            let bodies = run_mode_bodies(engine, &q.query, mode)?;
            Ok(map_bodies_to_doc_ids(&bodies, body_to_doc_id))
        })?;
        per_mode.insert(mode, result);
    }
    Ok(ExperimentResult { fanout, per_mode, deferred_modes })
}

/// Map retrieved bodies back to doc_ids, preserving rank; unmapped bodies are
/// dropped (mirrors `eu8_ir_validation::map_bodies_to_doc_ids`).
pub fn map_bodies_to_doc_ids(bodies: &[String], map: &HashMap<String, String>) -> Vec<String> {
    bodies.iter().filter_map(|b| map.get(b).cloned()).collect()
}

fn round4(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

/// Serialize an [`ExperimentResult`] to the structured JSON shape IR-C/IR-2 will
/// consume. NO thresholds, NO verdict — measured structure only.
pub fn experiment_to_json(gold: &GoldSet, result: &ExperimentResult) -> Value {
    let per_mode: serde_json::Map<String, Value> = result
        .per_mode
        .iter()
        .map(|(mode, by_k)| {
            let k_obj: serde_json::Map<String, Value> = by_k
                .iter()
                .map(|(k, r)| {
                    let per_class: serde_json::Map<String, Value> = r
                        .per_class
                        .iter()
                        .map(|(cls, agg)| {
                            (
                                cls.label().to_string(),
                                json!({
                                    "n": agg.n,
                                    "strict_evidence_recall": round4(agg.strict()),
                                    "graded_evidence_recall": round4(agg.graded()),
                                    "supporting_coverage": round4(agg.supporting()),
                                }),
                            )
                        })
                        .collect();
                    (
                        k.to_string(),
                        json!({
                            "overall": {
                                "n": r.overall.n,
                                "strict_evidence_recall": round4(r.overall.strict()),
                                "graded_evidence_recall": round4(r.overall.graded()),
                                "supporting_coverage": round4(r.overall.supporting()),
                            },
                            "per_class": per_class,
                            "negative_class": {
                                "n": r.negative.n,
                                "abstained": r.negative.abstained,
                                "false_positive_rate": round4(r.negative.false_positive_rate()),
                            },
                        }),
                    )
                })
                .collect();
            (mode.label().to_string(), Value::Object(k_obj))
        })
        .collect();

    json!({
        "_comment": "IR-B (IR-1 Phase 2) Evidence Recall@K — STRUCTURE only. \
                     No thresholds, no verdict (Phase 4 / IR-2 / HITL). Real-corpus \
                     numbers are DEFERRED to the COR-2 freeze (IR-C).",
        "measure": "evidence_recall_at_k",
        "headline_k": HEADLINE_K,
        "k_ladder": K_LADDER,
        "fanout": result.fanout,
        "corpus_hash": gold.corpus_hash,
        "qrels_version": gold.qrels_version,
        "query_count": gold.queries.len(),
        "deferred_modes": result.deferred_modes.iter().map(|m| m.label()).collect::<Vec<_>>(),
        "per_mode": per_mode,
    })
}
