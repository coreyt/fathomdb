//! Result-type surface for adaptive text search.
//!
//! Phase 1 wires a strict-only execution path through the coordinator. The
//! types exposed here are intentionally forward-compatible with later phases
//! that will add a relaxed branch, match-mode attribution, and recursive
//! property extraction. Fields that are reserved for those phases are present
//! and documented but populated with defaults in Phase 1.

use crate::{Predicate, TextQuery};

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
    /// Monotonically increasing relevance score. Phase 1 derives this from
    /// `-bm25(...)` so that larger values represent better matches; callers
    /// may sort descending by this field.
    pub score: f64,
    /// Which FTS surface produced the hit.
    pub source: SearchHitSource,
    /// Whether this hit came from the strict or relaxed branch. Always
    /// [`SearchMatchMode::Strict`] in Phase 1.
    pub match_mode: SearchMatchMode,
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
    /// Whether the relaxed fallback branch fired (Phase 1: always `false`).
    pub fallback_used: bool,
    /// Whether a capability miss caused the query to degrade to an empty
    /// result set (mirrors `QueryRows::was_degraded`).
    pub was_degraded: bool,
}

/// A compiled adaptive-search plan ready for the coordinator to execute.
///
/// Phase 1 keeps this intentionally thin: it carries the parsed text query,
/// the caller-specified limit, and any filter predicates the builder
/// accumulated. The coordinator emits SQL for it directly rather than reusing
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
    /// Row-level filter predicates accumulated from the builder pipeline.
    /// Applied by the coordinator in the outer `WHERE` clause. Filter fusion
    /// (pushing predicates into the FTS CTE) is deferred to Phase 2.
    pub filters: Vec<Predicate>,
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
        assert_eq!(compiled.filters.len(), 1);
    }
}
