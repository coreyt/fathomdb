//! JSON-based FFI surface for adaptive and fallback text search.
//!
//! Pack P7a exposes this module so that the Python and TypeScript SDKs
//! (Packs 7b / 7c) can run adaptive `text_search` and explicit
//! `fallback_search` through the engine by shipping a JSON AST and
//! receiving a JSON `SearchRows` payload. The types are plain serde
//! structures — no pyo3 / napi dependencies — so the translation path can
//! be unit- and integration-tested directly via `cargo test` without
//! linking against libpython or libnode.
//!
//! The entry point is [`execute_search_json`]: it parses a
//! [`PySearchRequest`], translates it into a [`fathomdb_query::QueryAst`]
//! plus filter chain, compiles a [`fathomdb_query::CompiledSearchPlan`]
//! via [`fathomdb_query::compile_search_plan`] (adaptive) or
//! [`fathomdb_query::compile_search_plan_from_queries`] (explicit two
//! shape), forwards the plan to
//! [`fathomdb_engine::ExecutionCoordinator::execute_compiled_search_plan`],
//! and serializes the returned [`fathomdb_query::SearchRows`] as
//! [`PySearchRows`].

use serde::{Deserialize, Serialize};

use crate::{
    ComparisonOp, Engine, EngineError, Predicate, QueryAst, QueryStep, ScalarValue, SearchHit,
    SearchHitSource, SearchMatchMode, SearchRows, TextQuery, compile_search_plan,
    compile_search_plan_from_queries,
};
use fathomdb_query::CompileError;

/// Mode tag selecting between adaptive and explicit fallback search.
///
/// `TextSearch` runs the adaptive pipeline — `relaxed_query` on the
/// request is ignored and the relaxed branch (if any) is derived from the
/// strict query via `derive_relaxed`. `FallbackSearch` uses the
/// caller-supplied `strict_query` and `relaxed_query` verbatim and is NOT
/// subject to the adaptive branch cap.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PySearchMode {
    /// Adaptive search: derive the relaxed branch from the strict query.
    TextSearch,
    /// Explicit fallback: take strict and relaxed verbatim from the request.
    FallbackSearch,
}

/// JSON request envelope for [`execute_search_json`].
///
/// Field semantics:
///  - `root_kind` — kind root of the search (reused for both branches).
///  - `strict_query` — raw user text, parsed Rust-side via
///    [`TextQuery::parse`].
///  - `relaxed_query` — optional relaxed text; ignored in `text_search`
///    mode, used verbatim in `fallback_search` mode.
///  - `mode` — adaptive vs explicit dispatch.
///  - `limit` — caller-supplied candidate cap forwarded to the compiled
///    search plan.
///  - `filters` — reuses the existing Phase 0 [`PySearchFilter`] variants
///    (`kind`, `logical_id`, `source_ref`, `content_ref`, JSON predicates) so
///    filter composition on search matches filter composition on the
///    general query path.
///  - `attribution_requested` — forwarded to
///    [`fathomdb_query::CompiledSearch::attribution_requested`] on both
///    branches; `false` by default.
#[derive(Clone, Debug, Deserialize)]
pub struct PySearchRequest {
    /// Kind root the search is scoped to.
    pub root_kind: String,
    /// Raw strict query text.
    pub strict_query: String,
    /// Optional raw relaxed query text (only consumed in
    /// [`PySearchMode::FallbackSearch`]).
    #[serde(default)]
    pub relaxed_query: Option<String>,
    /// Adaptive vs explicit dispatch.
    pub mode: PySearchMode,
    /// Candidate cap for the compiled search plan.
    pub limit: usize,
    /// Filter chain composed in order on top of the search.
    #[serde(default)]
    pub filters: Vec<PySearchFilter>,
    /// Whether the coordinator should resolve per-hit match attribution.
    #[serde(default)]
    pub attribution_requested: bool,
}

/// A single filter clause, mirroring the Phase 0 general-query FFI tag
/// set so Python / TypeScript can compose the same chain on a search
/// request that they compose on a flat query.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PySearchFilter {
    /// `kind = <kind>` predicate.
    FilterKindEq {
        /// Target kind.
        kind: String,
    },
    /// `logical_id = <id>` predicate.
    FilterLogicalIdEq {
        /// Target logical id.
        logical_id: String,
    },
    /// `source_ref = <ref>` predicate.
    FilterSourceRefEq {
        /// Target `source_ref` value.
        source_ref: String,
    },
    /// `content_ref = <ref>` predicate.
    FilterContentRefEq {
        /// Target `content_ref` value.
        content_ref: String,
    },
    /// `content_ref IS NOT NULL` predicate.
    FilterContentRefNotNull {},
    /// JSON-path text equality predicate.
    FilterJsonTextEq {
        /// Property JSON path.
        path: String,
        /// Target value.
        value: String,
    },
    /// JSON-path boolean equality predicate.
    FilterJsonBoolEq {
        /// Property JSON path.
        path: String,
        /// Target value.
        value: bool,
    },
    /// JSON-path integer strict-greater predicate.
    FilterJsonIntegerGt {
        /// Property JSON path.
        path: String,
        /// Target value.
        value: i64,
    },
    /// JSON-path integer greater-or-equal predicate.
    FilterJsonIntegerGte {
        /// Property JSON path.
        path: String,
        /// Target value.
        value: i64,
    },
    /// JSON-path integer strict-less predicate.
    FilterJsonIntegerLt {
        /// Property JSON path.
        path: String,
        /// Target value.
        value: i64,
    },
    /// JSON-path integer less-or-equal predicate.
    FilterJsonIntegerLte {
        /// Property JSON path.
        path: String,
        /// Target value.
        value: i64,
    },
    /// JSON-path timestamp strict-greater predicate.
    FilterJsonTimestampGt {
        /// Property JSON path.
        path: String,
        /// Target value (unix units matching the underlying column).
        value: i64,
    },
    /// JSON-path timestamp greater-or-equal predicate.
    FilterJsonTimestampGte {
        /// Property JSON path.
        path: String,
        /// Target value.
        value: i64,
    },
    /// JSON-path timestamp strict-less predicate.
    FilterJsonTimestampLt {
        /// Property JSON path.
        path: String,
        /// Target value.
        value: i64,
    },
    /// JSON-path timestamp less-or-equal predicate.
    FilterJsonTimestampLte {
        /// Property JSON path.
        path: String,
        /// Target value.
        value: i64,
    },
}

impl From<PySearchFilter> for QueryStep {
    fn from(value: PySearchFilter) -> Self {
        match value {
            PySearchFilter::FilterKindEq { kind } => QueryStep::Filter(Predicate::KindEq(kind)),
            PySearchFilter::FilterLogicalIdEq { logical_id } => {
                QueryStep::Filter(Predicate::LogicalIdEq(logical_id))
            }
            PySearchFilter::FilterSourceRefEq { source_ref } => {
                QueryStep::Filter(Predicate::SourceRefEq(source_ref))
            }
            PySearchFilter::FilterContentRefEq { content_ref } => {
                QueryStep::Filter(Predicate::ContentRefEq(content_ref))
            }
            PySearchFilter::FilterContentRefNotNull {} => {
                QueryStep::Filter(Predicate::ContentRefNotNull)
            }
            PySearchFilter::FilterJsonTextEq { path, value } => {
                QueryStep::Filter(Predicate::JsonPathEq {
                    path,
                    value: ScalarValue::Text(value),
                })
            }
            PySearchFilter::FilterJsonBoolEq { path, value } => {
                QueryStep::Filter(Predicate::JsonPathEq {
                    path,
                    value: ScalarValue::Bool(value),
                })
            }
            PySearchFilter::FilterJsonIntegerGt { path, value }
            | PySearchFilter::FilterJsonTimestampGt { path, value } => {
                QueryStep::Filter(Predicate::JsonPathCompare {
                    path,
                    op: ComparisonOp::Gt,
                    value: ScalarValue::Integer(value),
                })
            }
            PySearchFilter::FilterJsonIntegerGte { path, value }
            | PySearchFilter::FilterJsonTimestampGte { path, value } => {
                QueryStep::Filter(Predicate::JsonPathCompare {
                    path,
                    op: ComparisonOp::Gte,
                    value: ScalarValue::Integer(value),
                })
            }
            PySearchFilter::FilterJsonIntegerLt { path, value }
            | PySearchFilter::FilterJsonTimestampLt { path, value } => {
                QueryStep::Filter(Predicate::JsonPathCompare {
                    path,
                    op: ComparisonOp::Lt,
                    value: ScalarValue::Integer(value),
                })
            }
            PySearchFilter::FilterJsonIntegerLte { path, value }
            | PySearchFilter::FilterJsonTimestampLte { path, value } => {
                QueryStep::Filter(Predicate::JsonPathCompare {
                    path,
                    op: ComparisonOp::Lte,
                    value: ScalarValue::Integer(value),
                })
            }
        }
    }
}

/// Source of a serialized [`PySearchHit`].
///
/// Serde form is `snake_case` so the wire matches what Python / TypeScript
/// deserialize into their own enums.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PySearchHitSource {
    /// Hit from the chunk-backed full-text index (`fts_nodes`).
    Chunk,
    /// Hit from the property-backed full-text index (`fts_node_properties`).
    Property,
    /// Reserved for future vector-search attribution.
    Vector,
}

impl From<SearchHitSource> for PySearchHitSource {
    fn from(value: SearchHitSource) -> Self {
        match value {
            SearchHitSource::Chunk => Self::Chunk,
            SearchHitSource::Property => Self::Property,
            SearchHitSource::Vector => Self::Vector,
        }
    }
}

/// Whether a serialized [`PySearchHit`] came from the strict branch or
/// the relaxed fallback branch.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PySearchMatchMode {
    /// Hit matched the user's query as written.
    Strict,
    /// Hit matched only after the query was relaxed.
    Relaxed,
}

impl From<SearchMatchMode> for PySearchMatchMode {
    fn from(value: SearchMatchMode) -> Self {
        match value {
            SearchMatchMode::Strict => Self::Strict,
            SearchMatchMode::Relaxed => Self::Relaxed,
        }
    }
}

/// Node-shaped projection attached to every [`PySearchHit`].
///
/// Fields mirror `fathomdb_query::NodeRowLite` (and the Phase 0
/// `PyNodeRow` wire shape) so the Python / TypeScript SDKs can reuse
/// their existing node model when decoding search hits.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct PySearchNodeRow {
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
    /// Optional unix timestamp of last access.
    pub last_accessed_at: Option<i64>,
}

/// Per-hit attribution payload resolved when the caller sets
/// `attribution_requested = true` on the request.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct PyHitAttribution {
    /// Property paths (or `"text_content"` for chunk hits) that
    /// contributed to the match, in first-offset order.
    pub matched_paths: Vec<String>,
}

/// A single serialized search hit.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct PySearchHit {
    /// Matched node projection.
    pub node: PySearchNodeRow,
    /// Relevance score (positive — the coordinator negates raw bm25).
    pub score: f64,
    /// Which FTS surface produced the hit.
    pub source: PySearchHitSource,
    /// Strict or relaxed branch tag.
    pub match_mode: PySearchMatchMode,
    /// Optional display snippet.
    pub snippet: Option<String>,
    /// Seconds since the Unix epoch (1970-01-01 UTC), matching
    /// `nodes.created_at` which is populated via `SQLite` `unixepoch()`.
    /// Serialized directly as `i64`.
    pub written_at: i64,
    /// Opaque projection row ID (e.g. `chunks.id` for chunk hits).
    pub projection_row_id: Option<String>,
    /// Optional match-attribution payload; `None` unless
    /// `attribution_requested` was set on the request.
    pub attribution: Option<PyHitAttribution>,
}

impl From<SearchHit> for PySearchHit {
    fn from(value: SearchHit) -> Self {
        Self {
            node: PySearchNodeRow {
                row_id: value.node.row_id,
                logical_id: value.node.logical_id,
                kind: value.node.kind,
                properties: value.node.properties,
                content_ref: value.node.content_ref,
                last_accessed_at: value.node.last_accessed_at,
            },
            score: value.score,
            source: value.source.into(),
            match_mode: value.match_mode.into(),
            snippet: value.snippet,
            written_at: value.written_at,
            projection_row_id: value.projection_row_id,
            attribution: value.attribution.map(|a| PyHitAttribution {
                matched_paths: a.matched_paths,
            }),
        }
    }
}

/// Serialized result set returned by [`execute_search_json`].
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct PySearchRows {
    /// Matched hits in descending score order.
    pub hits: Vec<PySearchHit>,
    /// Whether a capability miss caused the query to degrade.
    pub was_degraded: bool,
    /// Whether the relaxed fallback branch fired.
    pub fallback_used: bool,
    /// Number of hits tagged [`PySearchMatchMode::Strict`].
    pub strict_hit_count: usize,
    /// Number of hits tagged [`PySearchMatchMode::Relaxed`].
    pub relaxed_hit_count: usize,
}

impl From<SearchRows> for PySearchRows {
    fn from(value: SearchRows) -> Self {
        Self {
            hits: value.hits.into_iter().map(PySearchHit::from).collect(),
            was_degraded: value.was_degraded,
            fallback_used: value.fallback_used,
            strict_hit_count: value.strict_hit_count,
            relaxed_hit_count: value.relaxed_hit_count,
        }
    }
}

/// Error produced by the JSON FFI translation path.
#[derive(Debug)]
pub enum SearchFfiError {
    /// The request JSON could not be deserialized.
    Parse(serde_json::Error),
    /// Plan compilation failed.
    Compile(CompileError),
    /// Coordinator execution failed.
    Engine(EngineError),
    /// Response serialization failed.
    Serialize(serde_json::Error),
}

impl std::fmt::Display for SearchFfiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "search request JSON parse error: {e}"),
            Self::Compile(e) => write!(f, "search plan compile error: {e:?}"),
            Self::Engine(e) => write!(f, "search execution error: {e}"),
            Self::Serialize(e) => write!(f, "search response serialize error: {e}"),
        }
    }
}

impl std::error::Error for SearchFfiError {}

/// Build a [`QueryAst`] carrying the request's filter chain but no text
/// search step — the strict `TextQuery` is materialized separately and
/// fed into the compile helpers directly.
fn build_filter_ast(request: &PySearchRequest) -> QueryAst {
    let steps = request
        .filters
        .iter()
        .cloned()
        .map(QueryStep::from)
        .collect();
    QueryAst {
        root_kind: request.root_kind.clone(),
        steps,
        expansions: Vec::new(),
        final_limit: None,
    }
}

/// Execute a search request given as JSON and return the JSON-encoded
/// [`PySearchRows`] response.
///
/// This is the sole entry point the Python / TypeScript FFI wrappers
/// call into. It:
///  1. Parses [`PySearchRequest`].
///  2. Parses the strict (and optional relaxed) raw query via
///     [`TextQuery::parse`].
///  3. Builds a filter-only [`QueryAst`] and compiles a
///     [`CompiledSearchPlan`](fathomdb_query::CompiledSearchPlan) via
///     [`compile_search_plan`] (adaptive) or
///     [`compile_search_plan_from_queries`] (explicit two shape).
///  4. Threads `attribution_requested` onto both branches of the plan.
///  5. Calls
///     [`ExecutionCoordinator::execute_compiled_search_plan`](fathomdb_engine::ExecutionCoordinator::execute_compiled_search_plan)
///     and serializes the returned [`SearchRows`] as [`PySearchRows`].
///
/// # Errors
/// Returns [`SearchFfiError`] on JSON parse, plan compile, engine
/// execution, or response serialization failure.
pub fn execute_search_json(engine: &Engine, request_json: &str) -> Result<String, SearchFfiError> {
    let request: PySearchRequest =
        serde_json::from_str(request_json).map_err(SearchFfiError::Parse)?;
    let strict = TextQuery::parse(&request.strict_query);
    let limit = request.limit;
    let attribution = request.attribution_requested;
    let ast = build_filter_ast(&request);

    let mut plan = match request.mode {
        PySearchMode::TextSearch => {
            // Adaptive: compile_search_plan requires a TextSearch step on
            // the AST because it runs through `compile_search` internally.
            // Inject the strict step onto the filter-only AST with the
            // caller's limit.
            let mut ast_with_text = ast;
            ast_with_text.steps.insert(
                0,
                QueryStep::TextSearch {
                    query: strict,
                    limit,
                },
            );
            compile_search_plan(&ast_with_text).map_err(SearchFfiError::Compile)?
        }
        PySearchMode::FallbackSearch => {
            let relaxed = request.relaxed_query.as_deref().map(TextQuery::parse);
            compile_search_plan_from_queries(&ast, strict, relaxed, limit, attribution)
                .map_err(SearchFfiError::Compile)?
        }
    };

    // Ensure attribution_requested is set on both branches regardless of
    // which compile helper produced the plan.
    plan.strict.attribution_requested = attribution;
    if let Some(relaxed) = plan.relaxed.as_mut() {
        relaxed.attribution_requested = attribution;
    }

    let rows: SearchRows = engine
        .coordinator()
        .execute_compiled_search_plan(&plan)
        .map_err(SearchFfiError::Engine)?;
    let py_rows = PySearchRows::from(rows);
    serde_json::to_string(&py_rows).map_err(SearchFfiError::Serialize)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::{
        PyHitAttribution, PySearchHit, PySearchHitSource, PySearchMatchMode, PySearchNodeRow,
        PySearchRows,
    };

    #[test]
    fn search_rows_serde_roundtrip_empty() {
        let rows = PySearchRows {
            hits: Vec::new(),
            was_degraded: false,
            fallback_used: false,
            strict_hit_count: 0,
            relaxed_hit_count: 0,
        };
        let json = serde_json::to_string(&rows).expect("serialize");
        let parsed: PySearchRows = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(rows, parsed);
    }

    #[test]
    fn search_rows_serde_roundtrip_with_hit() {
        let hit = PySearchHit {
            node: PySearchNodeRow {
                row_id: "row-1".into(),
                logical_id: "node-1".into(),
                kind: "Goal".into(),
                properties: r#"{"name":"test"}"#.into(),
                content_ref: Some("s3://x".into()),
                last_accessed_at: Some(1_700_000_000),
            },
            score: 1.25,
            source: PySearchHitSource::Chunk,
            match_mode: PySearchMatchMode::Strict,
            snippet: Some("... <b>test</b> ...".into()),
            written_at: 1_700_000_001,
            projection_row_id: Some("chunk-1".into()),
            attribution: Some(PyHitAttribution {
                matched_paths: vec!["$.name".into()],
            }),
        };
        let rows = PySearchRows {
            hits: vec![hit],
            was_degraded: false,
            fallback_used: true,
            strict_hit_count: 1,
            relaxed_hit_count: 0,
        };
        let json = serde_json::to_string(&rows).expect("serialize");
        let parsed: PySearchRows = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(rows, parsed);
    }

    #[test]
    fn search_hit_source_snake_case_wire_form() {
        let json = serde_json::to_string(&PySearchHitSource::Chunk).expect("serialize");
        assert_eq!(json, "\"chunk\"");
        let json = serde_json::to_string(&PySearchHitSource::Property).expect("serialize");
        assert_eq!(json, "\"property\"");
        let json = serde_json::to_string(&PySearchHitSource::Vector).expect("serialize");
        assert_eq!(json, "\"vector\"");
    }

    #[test]
    fn search_match_mode_snake_case_wire_form() {
        let json = serde_json::to_string(&PySearchMatchMode::Strict).expect("serialize");
        assert_eq!(json, "\"strict\"");
        let json = serde_json::to_string(&PySearchMatchMode::Relaxed).expect("serialize");
        assert_eq!(json, "\"relaxed\"");
    }

    #[test]
    fn search_request_deserializes_text_search_shape() {
        use super::{PySearchFilter, PySearchMode, PySearchRequest};
        let request: PySearchRequest = serde_json::from_str(
            r#"{
                "mode": "text_search",
                "root_kind": "Goal",
                "strict_query": "budget",
                "limit": 10,
                "filters": [{"type":"filter_kind_eq","kind":"Goal"}],
                "attribution_requested": true
            }"#,
        )
        .expect("parse");
        assert!(matches!(request.mode, PySearchMode::TextSearch));
        assert_eq!(request.root_kind, "Goal");
        assert_eq!(request.strict_query, "budget");
        assert_eq!(request.limit, 10);
        assert!(request.attribution_requested);
        assert!(request.relaxed_query.is_none());
        assert_eq!(request.filters.len(), 1);
        assert!(matches!(
            request.filters[0],
            PySearchFilter::FilterKindEq { ref kind } if kind == "Goal"
        ));
    }

    #[test]
    fn search_request_deserializes_fallback_search_shape() {
        use super::{PySearchMode, PySearchRequest};
        let request: PySearchRequest = serde_json::from_str(
            r#"{
                "mode": "fallback_search",
                "root_kind": "Goal",
                "strict_query": "budget",
                "relaxed_query": "budget OR alpha",
                "limit": 5,
                "filters": []
            }"#,
        )
        .expect("parse");
        assert!(matches!(request.mode, PySearchMode::FallbackSearch));
        assert_eq!(request.relaxed_query.as_deref(), Some("budget OR alpha"));
        assert!(!request.attribution_requested);
    }
}
