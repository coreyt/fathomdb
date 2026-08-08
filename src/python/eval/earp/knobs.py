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
        name="projections.declare",
        classification=KnobClass.SEMANTIC,
        call_path="Engine.configure_projections(specs=)",
        witness="projection_witnesses.configure_delta",
        reason=(
            "REPLACES the pre-S7 `configure_projections` INDEXING entry -- one call "
            "path, one entry. That entry classified the capability before any config "
            "could express it; now that `scenario.projections.declare` can, the "
            "config-facing name and SEMANTIC (it alters stored data and results) are "
            "the truthful record. Fresh database per scenario by policy."
        ),
    ),
    KnobEntry(
        name="projections.readiness_timeout_s",
        classification=KnobClass.RUNTIME,
        call_path="fathomdb.read.projections",
        witness="projection_witnesses.readiness",
        reason=(
            "Bound on the vector-readiness poll. A spec still `embedding` at the "
            "bound is the typed blocker dense_readiness_timeout, never an empty "
            "retrieval result."
        ),
    ),
    KnobEntry(
        name="fts_tokenizer",
        classification=KnobClass.UNSUPPORTED,
        call_path=None,
        witness=None,
        reason=(
            "No supported custom tokenizer implementations in the Python SDK; a "
            "stored identity is not proof of a runtime, so earp.v1 cannot honestly "
            "declare one."
        ),
    ),
    KnobEntry(
        name="vector_embedder",
        classification=KnobClass.UNSUPPORTED,
        call_path=None,
        witness=None,
        reason=(
            "Custom embedder implementations are unsupported by the Python SDK "
            "(earp.md); the only embedder lever is scenario.engine."
            "use_default_embedder."
        ),
    ),
    KnobEntry(
        name="projections.drop",
        classification=KnobClass.UNSUPPORTED,
        call_path=None,
        witness=None,
        reason=(
            "Scenarios own a fresh database per run, so there is never anything to "
            "drop; a drop surface would be untestable dead weight."
        ),
    ),
    KnobEntry(
        name="projections.source",
        classification=KnobClass.UNSUPPORTED,
        call_path=None,
        witness=None,
        reason=(
            "Nested source-segment projections are a wider surface than the "
            "witnesses S7 owes; deferred to keep the catalog honest."
        ),
    ),
    KnobEntry(
        name="priced_arm.mem0",
        classification=KnobClass.UNSUPPORTED,
        call_path=None,
        witness=None,
        reason=(
            "A dormant local-OSS Mem0OSSAdapter exists in r2_parity_eval, but its "
            "commissioning as a priced EARP arm stays an HITL scope decision; S9 "
            "ships the enforcement machinery plus exactly one adapter (the R2 "
            "identical-answerer) so the money gate is the slice's center."
        ),
    ),
    KnobEntry(
        name="priced_arm.extractor",
        classification=KnobClass.UNSUPPORTED,
        call_path=None,
        witness=None,
        reason=(
            "No in-repo extractor answerer protocol is in standing use; building "
            "a new network adapter inside S9 would dilute the money gate, and its "
            "commissioning is an HITL scope decision."
        ),
    ),
    KnobEntry(
        name="priced_arm.gpu",
        classification=KnobClass.UNSUPPORTED,
        call_path=None,
        witness=None,
        reason=(
            "The GPU arm has no in-repo answerer protocol in standing use; its "
            "commissioning is an HITL scope decision, recorded here so its "
            "absence is a refusal rather than silence."
        ),
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
