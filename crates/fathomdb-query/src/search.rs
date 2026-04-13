//! Result-type surface for adaptive text search.
//!
//! Phase 1 wires a strict-only execution path through the coordinator. The
//! types exposed here are intentionally forward-compatible with later phases
//! that will add a relaxed branch, match-mode attribution, and recursive
//! property extraction. Fields that are reserved for those phases are present
//! and documented but populated with defaults in Phase 1.

use crate::{Predicate, TextQuery};

/// Which branch of the adaptive text-search policy produced a given result
/// set or was used to construct a given [`CompiledSearch`].
///
/// Phase 3 runs the strict branch first, then conditionally runs a relaxed
/// branch derived from the same user query (see
/// [`crate::derive_relaxed`]). The coordinator tags each in-flight branch
/// with this enum so that merge, dedup, and counts stay straightforward.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchBranch {
    /// The strict branch: the user's query as written.
    Strict,
    /// The relaxed fallback branch derived from the strict query.
    Relaxed,
}

/// Source of a [`SearchHit`] within the FTS surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchHitSource {
    /// The hit came from the chunk-backed full-text index (`fts_nodes`).
    Chunk,
    /// The hit came from the property-backed full-text index
    /// (`fts_node_properties`).
    Property,
    /// Reserved for future vector-search attribution.
    ///
    /// No Phase 1 code path emits this variant; it is exported so that future
    /// vector wiring can be added without a breaking change to consumers that
    /// exhaustively match on [`SearchHitSource`].
    Vector,
}

/// Whether a [`SearchHit`] was produced by the strict user query or by a
/// relaxed (Phase 2+) fallback branch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchMatchMode {
    /// The hit matched the user's query exactly as written.
    Strict,
    /// Reserved: the hit matched only after the query was relaxed by an
    /// adaptive fallback pass. No Phase 1 code path emits this variant.
    Relaxed,
}

/// Coarse retrieval-modality classifier for a [`SearchHit`].
///
/// Phase 10 adds this field to the result surface so that future phases
/// which introduce a vector retrieval branch can tag their hits without a
/// breaking change to consumers. Every hit produced by the current (text-
/// only) execution paths is tagged [`RetrievalModality::Text`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetrievalModality {
    /// The hit came from a text retrieval branch (chunk or property FTS).
    Text,
    /// The hit came from a vector retrieval branch. Reserved — no current
    /// execution path emits this variant.
    Vector,
}

/// Per-hit attribution data produced by the (Phase 5) match attributor.
///
/// The struct is exported in Phase 1 to lock in the shape of
/// [`SearchHit::attribution`], but it is never populated by the current
/// execution path. All hits return `attribution: None` until Phase 5 wires
/// the attributor.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HitAttribution {
    /// Property paths (or `"text_content"` for chunk hits) that contributed to
    /// the match. Empty in Phase 1.
    pub matched_paths: Vec<String>,
}

/// A single result row emitted by adaptive text search.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchHit {
    /// The matched node, projected in the same shape the flat query surface
    /// uses.
    pub node: NodeRowLite,
    /// Raw engine score used for ordering within a block. Higher is always
    /// better, across every modality and every source:
    /// - Text hits: the FTS5 bm25 score with its sign flipped (`-bm25(...)`),
    ///   so higher score corresponds to stronger lexical relevance.
    /// - Vector hits: a negated distance (`-vector_distance`) for distance
    ///   metrics, or a direct similarity value for similarity metrics.
    ///
    /// Scores are **ordering-only within a block**. Scores from different
    /// blocks — and in particular text scores vs. vector scores — are not
    /// on a shared scale. The engine does not normalize across blocks, and
    /// callers must not compare or arithmetically combine scores across
    /// blocks.
    pub score: f64,
    /// Coarse retrieval-modality classifier. Every hit produced by the
    /// current text execution paths is tagged
    /// [`RetrievalModality::Text`]; future phases that wire vector
    /// retrieval will tag those hits [`RetrievalModality::Vector`].
    pub modality: RetrievalModality,
    /// Which FTS surface produced the hit.
    pub source: SearchHitSource,
    /// Whether this hit came from the strict or relaxed branch. `Some`
    /// for every text hit; reserved as `None` for future vector hits,
    /// which have no strict/relaxed notion.
    pub match_mode: Option<SearchMatchMode>,
    /// Short context snippet for display. `Some` for at least the chunk path
    /// (`SQLite`'s `snippet(...)`) and a trimmed window of `text_content` for
    /// the property path.
    pub snippet: Option<String>,
    /// Wall-clock timestamp (unix seconds) at which the *active* version of
    /// the node was written.
    ///
    /// Under fathomdb's soft-delete supersession model, nodes are versioned:
    /// each edit creates a new active row and marks the prior row
    /// superseded. `written_at` reflects when the **current** active row was
    /// inserted, which is "when the text that just matched was written," not
    /// "when the `logical_id` was first created." A node created two years ago
    /// but updated yesterday will show yesterday's timestamp, because
    /// yesterday's text is what the FTS index scored against.
    ///
    /// This is deliberately distinct from `superseded_at` (only populated on
    /// dead rows), `node_access_metadata.last_accessed_at` (an explicit touch,
    /// not a write), and `provenance_events.created_at` (audit event time).
    pub written_at: i64,
    /// Opaque identifier of the underlying projection row (e.g. `chunks.id`
    /// for chunk hits, or `fts_node_properties.rowid` for property hits).
    /// Useful for debugging and for future attribution paths.
    pub projection_row_id: Option<String>,
    /// Raw vector distance or similarity for vector hits. `None` for text
    /// hits.
    ///
    /// Stable public API: this field ships in v1 and is documented as
    /// modality-specific diagnostic data. Callers may read it for display
    /// or internal reranking but must **not** compare it against text-hit
    /// `score` values or use it arithmetically alongside text scores — the
    /// two are not on a shared scale.
    ///
    /// For distance metrics the raw distance is preserved (lower = closer
    /// match); callers that want a "higher is better" ordering value should
    /// read `score` instead, which is already negated appropriately for
    /// intra-block ranking.
    pub vector_distance: Option<f64>,
    /// Reserved: match-attribution payload. Always `None` in Phase 1.
    pub attribution: Option<HitAttribution>,
}

/// Minimal node-shaped projection attached to every [`SearchHit`].
///
/// This intentionally mirrors the fields of `fathomdb_engine::NodeRow` without
/// depending on the engine crate. The engine-side `execute_compiled_search`
/// materializes `SearchHit` values using its own `NodeRow` type, so the facade
/// crate converts between the two.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeRowLite {
    /// Physical row ID.
    pub row_id: String,
    /// Logical ID of the node.
    pub logical_id: String,
    /// Node kind.
    pub kind: String,
    /// JSON-encoded node properties.
    pub properties: String,
    /// Optional URI referencing external content.
    pub content_ref: Option<String>,
    /// Unix timestamp of last access, if tracked.
    pub last_accessed_at: Option<i64>,
}

/// Result set returned by an adaptive text-search execution.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchRows {
    /// Matched hits in descending score order.
    pub hits: Vec<SearchHit>,
    /// Count of strict-branch hits (Phase 1: equals `hits.len()`).
    pub strict_hit_count: usize,
    /// Count of relaxed-branch hits (Phase 1: always 0).
    pub relaxed_hit_count: usize,
    /// Count of vector-branch hits. Always `0` after Phase 10 because no
    /// vector execution path exists yet; reserved so that when vector
    /// retrieval lands in a later phase, the wire shape already has the
    /// counter and consumers do not need a breaking change.
    pub vector_hit_count: usize,
    /// Whether the relaxed fallback branch fired (Phase 1: always `false`).
    pub fallback_used: bool,
    /// Whether a capability miss caused the query to degrade to an empty
    /// result set (mirrors `QueryRows::was_degraded`).
    pub was_degraded: bool,
}

/// A compiled adaptive-search plan ready for the coordinator to execute.
///
/// Phase 2 splits the filter pipeline into two sets: `fusable_filters`
/// (pushed into the `search_hits` CTE so the CTE `LIMIT` applies after
/// filtering) and `residual_filters` (evaluated in the outer `WHERE`). The
/// coordinator emits SQL for it directly rather than reusing
/// [`crate::compile_query`], because the search SELECT projects a different
/// row shape (score, source, snippet, projection id) than the flat query
/// path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledSearch {
    /// Root kind the caller built the query against.
    pub root_kind: String,
    /// Parsed text-search intent, to be lowered into safe FTS5 syntax.
    pub text_query: TextQuery,
    /// Maximum number of candidate hits to retrieve from the FTS indexes.
    pub limit: usize,
    /// Fusable predicates pushed into the `search_hits` CTE by the coordinator.
    /// These evaluate against columns directly available on the `nodes` table
    /// joined inside the CTE (`kind`, `logical_id`, `source_ref`,
    /// `content_ref`).
    pub fusable_filters: Vec<Predicate>,
    /// Residual predicates applied in the outer `WHERE` after the CTE
    /// materializes. Currently limited to JSON-property predicates
    /// (`json_extract` on `n.properties`).
    pub residual_filters: Vec<Predicate>,
    /// Whether the caller requested per-hit match attribution. Phase 5: when
    /// `true`, the coordinator populates [`SearchHit::attribution`] on every
    /// hit by resolving FTS5 match positions against the Phase 4 position
    /// map. When `false` (the default), the position map is not read at all
    /// and `attribution` stays `None`.
    pub attribution_requested: bool,
}

/// A compiled vector-only search plan ready for the coordinator to execute.
///
/// Phase 11 delivers a standalone vector retrieval path parallel to
/// [`CompiledSearch`]. It is intentionally structurally distinct: the vector
/// path has no [`TextQuery`], no relaxed branch, and no [`SearchMatchMode`] —
/// vector hits always carry `match_mode: None` per addendum 1. The
/// coordinator consumes this carrier via
/// `ExecutionCoordinator::execute_compiled_vector_search`, which emits SQL
/// against the `vec_nodes_active` virtual table joined to `nodes`, and
/// returns a [`SearchRows`] with a single vector block (or an empty result
/// with `was_degraded = true` when the sqlite-vec capability is absent).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledVectorSearch {
    /// Root kind the caller built the query against. May be empty for
    /// kind-agnostic callers, mirroring the text path.
    pub root_kind: String,
    /// Raw vector query text passed to sqlite-vec via the `embedding MATCH`
    /// operator. This is a serialized JSON float array (e.g.
    /// `"[0.1, 0.2, 0.3, 0.4]"`) at the time the coordinator binds it.
    pub query_text: String,
    /// Maximum number of candidate hits to retrieve from the vec0 KNN scan.
    pub limit: usize,
    /// Fusable predicates pushed into the vector-search CTE by the
    /// coordinator. Evaluated against columns directly available on the
    /// `nodes` table joined inside the CTE.
    pub fusable_filters: Vec<Predicate>,
    /// Residual predicates applied in the outer `WHERE` after the CTE
    /// materializes. Currently limited to JSON-property predicates.
    pub residual_filters: Vec<Predicate>,
    /// Whether the caller requested per-hit match attribution. Per addendum
    /// 1 §Attribution on vector hits, vector hits under this flag carry
    /// `Some(HitAttribution { matched_paths: vec![] })` — an empty
    /// matched-paths list, not `None`.
    pub attribution_requested: bool,
}

/// A two-branch compiled search plan ready for the coordinator to execute.
///
/// Phase 6 factors the strict+relaxed retrieval pair into a small carrier so
/// that the adaptive [`crate::compile_search`] path and the narrow
/// `fallback_search(strict, relaxed)` helper share a single coordinator
/// routine. Both branches carry fully compiled [`CompiledSearch`] values —
/// including the same fused/residual filter chain and the same
/// `attribution_requested` flag — so merge/dedup stays branch-agnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledSearchPlan {
    /// The strict branch — always runs first.
    pub strict: CompiledSearch,
    /// The relaxed branch, or `None` when the caller did not request a
    /// fallback shape. When `None`, the coordinator runs strict only and
    /// never triggers the fallback policy.
    pub relaxed: Option<CompiledSearch>,
    /// Set when the plan originated from [`crate::derive_relaxed`] and its
    /// alternatives list was truncated past [`crate::RELAXED_BRANCH_CAP`].
    /// The `fallback_search` path always sets this to `false` because the
    /// relaxed shape is caller-provided and not subject to the cap.
    pub was_degraded_at_plan_time: bool,
}

/// A compiled unified retrieval plan for the Phase 12 `search()` entry point.
///
/// `CompiledRetrievalPlan` carries the bounded set of branches the engine-owned
/// retrieval planner may run on behalf of a single `search(query, limit)` call:
/// the text strict + optional text relaxed pair (carried structurally as the
/// existing Phase 6 [`CompiledSearchPlan`]) and an optional vector branch.
///
/// **v1 scope (Phase 12)**: the planner's `vector` branch slot is structurally
/// supported so that the coordinator's three-block fusion path is fully wired,
/// but [`crate::compile_retrieval_plan`] always sets `vector` to `None`. Read-
/// time embedding of natural-language queries is not wired into the engine in
/// v1; callers that want vector retrieval through the unified `search()`
/// entry point will get text-only results until a future phase wires the
/// embedding generator into the read path. Callers who want explicit vector
/// retrieval today use the advanced `vector_search()` override (Phase 11),
/// which takes a caller-provided vector literal.
///
/// `CompiledRetrievalPlan` is intentionally distinct from
/// [`CompiledSearchPlan`]: `CompiledSearchPlan` is the text-only carrier
/// consumed by `text_search()` and `fallback_search()`, and the two paths
/// remain separate so the text-only call sites do not pay any vector-branch
/// cost. The Phase 12 unified planner is a sibling, not a replacement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledRetrievalPlan {
    /// The text branches (strict + optional relaxed) of the unified plan.
    /// Always present — every `search()` call produces at least a strict
    /// text branch (which may itself short-circuit to empty when the query
    /// is `Empty` or a top-level `Not`).
    pub text: CompiledSearchPlan,
    /// The vector branch slot. Always `None` in v1 per the Phase 12 scope
    /// constraint above.
    pub vector: Option<CompiledVectorSearch>,
    /// Mirrors [`CompiledSearchPlan::was_degraded_at_plan_time`] for the
    /// text branches: set when the relaxed branch's alternatives list was
    /// truncated past [`crate::RELAXED_BRANCH_CAP`] at plan-construction
    /// time. Propagated to the result's `was_degraded` flag if and only if
    /// the relaxed branch actually fires at execution time.
    pub was_degraded_at_plan_time: bool,
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn search_hit_source_has_vector_variant_reserved() {
        // Compile-time exhaustiveness check: if a future change removes or
        // renames Vector, this match stops compiling and the test fails
        // loudly rather than silently breaking consumers that rely on the
        // reserved variant.
        let source = SearchHitSource::Chunk;
        match source {
            SearchHitSource::Chunk | SearchHitSource::Property | SearchHitSource::Vector => {}
        }
    }

    #[test]
    fn search_match_mode_has_strict_and_relaxed() {
        let mode = SearchMatchMode::Strict;
        match mode {
            SearchMatchMode::Strict | SearchMatchMode::Relaxed => {}
        }
    }

    #[test]
    fn compile_search_rejects_ast_without_text_search_step() {
        use crate::{CompileError, QueryBuilder, compile_search};
        let ast = QueryBuilder::nodes("Goal")
            .filter_kind_eq("Goal")
            .into_ast();
        let result = compile_search(&ast);
        assert!(
            matches!(result, Err(CompileError::MissingTextSearchStep)),
            "expected MissingTextSearchStep, got {result:?}"
        );
    }

    #[test]
    fn compile_search_accepts_text_search_step_with_filters() {
        use crate::{QueryBuilder, compile_search};
        let ast = QueryBuilder::nodes("Goal")
            .text_search("quarterly docs", 7)
            .filter_kind_eq("Goal")
            .into_ast();
        let compiled = compile_search(&ast).expect("compiles");
        assert_eq!(compiled.root_kind, "Goal");
        assert_eq!(compiled.limit, 7);
        assert_eq!(compiled.fusable_filters.len(), 1);
        assert!(compiled.residual_filters.is_empty());
    }

    #[test]
    fn compile_vector_search_rejects_ast_without_vector_search_step() {
        use crate::{CompileError, QueryBuilder, compile_vector_search};
        let ast = QueryBuilder::nodes("Goal")
            .filter_kind_eq("Goal")
            .into_ast();
        let result = compile_vector_search(&ast);
        assert!(
            matches!(result, Err(CompileError::MissingVectorSearchStep)),
            "expected MissingVectorSearchStep, got {result:?}"
        );
    }

    #[test]
    fn compile_vector_search_accepts_vector_search_step_with_filters() {
        use crate::{Predicate, QueryBuilder, compile_vector_search};
        let ast = QueryBuilder::nodes("Goal")
            .vector_search("[0.1, 0.2, 0.3, 0.4]", 7)
            .filter_kind_eq("Goal")
            .filter_json_text_eq("$.status", "active")
            .into_ast();
        let compiled = compile_vector_search(&ast).expect("compiles");
        assert_eq!(compiled.root_kind, "Goal");
        assert_eq!(compiled.query_text, "[0.1, 0.2, 0.3, 0.4]");
        assert_eq!(compiled.limit, 7);
        assert_eq!(compiled.fusable_filters.len(), 1);
        assert!(matches!(
            compiled.fusable_filters[0],
            Predicate::KindEq(ref k) if k == "Goal"
        ));
        assert_eq!(compiled.residual_filters.len(), 1);
        assert!(!compiled.attribution_requested);
    }
}
