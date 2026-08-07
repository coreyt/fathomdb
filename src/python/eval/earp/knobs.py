"""The knob catalog.

Keyed on **whether a concrete SDK call path exists** -- never on whether a knob
happens to be an `EngineConfig` field. Those are different questions, and
conflating them writes a false statement about the SDK into a test:
`slow_threshold_ms` is an `EngineConfig` field that `Engine.open` never
forwards, yet it has its own live path and is therefore supported.

Completeness is asserted by two BOUNDED introspections -- over `EngineConfig`'s
fields and over the search signatures -- each asserting coverage of a known
surface rather than letting reflection define the surface. A hand-maintained
list can under-cover as easily as it can misclassify, and the first revision of
this catalog omitted eight real call paths.
"""

from __future__ import annotations

from eval.earp.schema.models import KnobClass, KnobEntry

CATALOG: tuple[KnobEntry, ...] = (
    KnobEntry(
        name="use_default_embedder",
        classification=KnobClass.SEMANTIC,
        call_path="Engine.open(use_default_embedder=)",
        witness="open_report.default_embedder",
        reason="The only EngineConfig-adjacent setting that reaches native open.",
    ),
    KnobEntry(
        name="slow_threshold_ms",
        classification=KnobClass.RUNTIME,
        call_path="Engine.set_slow_threshold_ms",
        witness="open_report.slow_threshold_ms",
        reason=(
            "An EngineConfig field Engine.open never forwards, yet independently "
            "supported through its own setter."
        ),
    ),
    KnobEntry(
        name="profiling",
        classification=KnobClass.OBSERVABILITY,
        call_path="Engine.set_profiling",
        witness="search_result.explanation",
        reason="Sibling setter of slow_threshold_ms.",
    ),
    KnobEntry(
        name="embedder_pool_size",
        classification=KnobClass.UNSUPPORTED,
        call_path=None,
        witness=None,
        reason="EngineConfig field; never forwarded to native open and no independent path.",
    ),
    KnobEntry(
        name="scheduler_runtime_threads",
        classification=KnobClass.UNSUPPORTED,
        call_path=None,
        witness=None,
        reason="EngineConfig field; never forwarded to native open and no independent path.",
    ),
    KnobEntry(
        name="provenance_row_cap",
        classification=KnobClass.UNSUPPORTED,
        call_path=None,
        witness=None,
        reason="EngineConfig field; never forwarded to native open and no independent path.",
    ),
    KnobEntry(
        name="embedder_call_timeout_ms",
        classification=KnobClass.UNSUPPORTED,
        call_path=None,
        witness=None,
        reason="EngineConfig field; never forwarded to native open and no independent path.",
    ),
    KnobEntry(
        name="rerank_depth",
        classification=KnobClass.SEMANTIC,
        call_path="Engine.search(rerank_depth=)",
        witness="query_trace.rerank_depth",
        reason="CE rerank depth; alters the returned ranking.",
    ),
    KnobEntry(
        name="use_graph_arm",
        classification=KnobClass.SEMANTIC,
        call_path="Engine.search(use_graph_arm=)",
        witness="query_trace.use_graph_arm",
        reason="Adds a third RRF arm; alters the fused ranking.",
    ),
    KnobEntry(
        name="alpha",
        classification=KnobClass.SEMANTIC,
        call_path="Engine.search(alpha=)",
        witness="query_trace.alpha",
        reason=(
            "Branch weighting. The engine CLAMPS out-of-range values rather than "
            "refusing, so the resolver bounds it instead."
        ),
    ),
    KnobEntry(
        name="pool_n",
        classification=KnobClass.SEMANTIC,
        call_path="Engine.search(pool_n=)",
        witness="query_trace.pool_n",
        reason="CE-rerank POOL SIZE -- not a result-depth control, and not a way around the depth cap.",
    ),
    KnobEntry(
        name="limit",
        classification=KnobClass.SEMANTIC,
        call_path="Engine.search(limit=)",
        witness="search_result.n",
        reason=(
            "The public result limit (0.8.22 Slice 18): bounds visible result "
            "cardinality on every search verb, and with it the deepest measurable K. "
            "Witnessed by exact cardinality."
        ),
    ),
    KnobEntry(
        name="explain",
        classification=KnobClass.OBSERVABILITY,
        call_path="Engine.search(explain=)",
        witness="search_result.explanation",
        reason="Produces the Explanation/QueryTrace sidecar; changes no result.",
    ),
    KnobEntry(
        name="view",
        classification=KnobClass.SEMANTIC,
        call_path="Engine.search(view=)",
        witness="query_trace.view",
        reason="Changes which nodes are eligible for retrieval, so it can move a recall number.",
    ),
    KnobEntry(
        name="filter",
        classification=KnobClass.SEMANTIC,
        call_path="Engine.search(filter=)",
        witness="query_trace.filter",
        reason="Restricts the candidate set. No config surface yet; recorded so it cannot be forgotten.",
    ),
    KnobEntry(
        name="projection_name",
        classification=KnobClass.SEMANTIC,
        call_path="Engine.search_projected_text(name=)",
        witness="search_result.projection_cursor",
        reason="The projection queried. Required positional -- without it the call cannot run.",
    ),
    KnobEntry(
        name="depth",
        classification=KnobClass.SEMANTIC,
        call_path="graph.search_expand(depth=)",
        witness="expanded_node.hops",
        reason="BFS traversal depth on the graph-expansion path.",
    ),
    KnobEntry(
        name="source_type",
        classification=KnobClass.SEMANTIC,
        call_path="graph.search_expand(source_type=)",
        witness="expanded_node.source_type",
        reason="Traversal filter; restricts the expanded set.",
    ),
    KnobEntry(
        name="kind",
        classification=KnobClass.SEMANTIC,
        call_path="graph.search_expand(kind=)",
        witness="expanded_node.kind",
        reason="Traversal filter; restricts the expanded set.",
    ),
    KnobEntry(
        name="created_after",
        classification=KnobClass.SEMANTIC,
        call_path="graph.search_expand(created_after=)",
        witness="expanded_node.created_at",
        reason="Traversal filter; restricts the expanded set.",
    ),
    KnobEntry(
        name="status",
        classification=KnobClass.SEMANTIC,
        call_path="graph.search_expand(status=)",
        witness="expanded_node.status",
        reason="Traversal filter; restricts the expanded set.",
    ),
    KnobEntry(
        name="enable_telemetry",
        classification=KnobClass.OBSERVABILITY,
        call_path="Engine.enable_telemetry",
        witness="open_report.telemetry",
        reason="Opt-in, no-egress telemetry; changes no result.",
    ),
    KnobEntry(
        name="drain",
        classification=KnobClass.RUNTIME,
        call_path="Engine.drain(timeout_s=)",
        witness="open_report.embedder_events",
        reason=(
            "Determines whether async projection work is visible before search. "
            "Not a relevance lever, but it CAN move a recall number, so it is "
            "recorded rather than treated as inert."
        ),
    ),
    KnobEntry(
        name="configure_projections",
        classification=KnobClass.INDEXING,
        call_path="Engine.configure_projections",
        witness="projection_delta",
        reason="Declares filter/FTS/vector projections; requires a fresh database per scenario by policy.",
    ),
    KnobEntry(
        name="attach_logging_subscriber",
        classification=KnobClass.UNSUPPORTED,
        call_path=None,
        witness=None,
        reason=(
            "The call path EXISTS but is inert -- its own docstring defers subscriber "
            "wiring to a later slice. This is the catalog's proof of purpose: a path "
            "that exists and does nothing must still be present to be tested."
        ),
    ),
)

CATALOG_BY_NAME = {entry.name: entry for entry in CATALOG}

__all__ = ["CATALOG", "CATALOG_BY_NAME"]
