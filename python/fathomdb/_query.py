from __future__ import annotations

import json

from ._feedback import run_with_feedback
from ._fathomdb import EngineCore
from ._types import (
    CompiledGroupedQuery,
    CompiledQuery,
    FeedbackConfig,
    GroupedQueryRows,
    QueryPlan,
    QueryRows,
    SearchRows,
    TraverseDirection,
)


class Query:
    """Fluent, immutable query builder for fetching nodes from fathomdb.

    Instances are created via :meth:`Engine.nodes`.  Each filter or traversal
    method returns a new Query, leaving the original unchanged.  Terminal
    methods (:meth:`execute`, :meth:`compile`, :meth:`explain`) send the
    assembled AST to the engine.
    """

    def __init__(
        self,
        core: EngineCore,
        root_kind: str,
        *,
        steps: list[dict] | None = None,
        expansions: list[dict] | None = None,
        final_limit: int | None = None,
    ) -> None:
        self._core = core
        self._root_kind = root_kind
        self._steps = list(steps or [])
        self._expansions = list(expansions or [])
        self._final_limit = final_limit

    def _with_step(self, step: dict) -> "Query":
        return Query(
            self._core,
            self._root_kind,
            steps=[*self._steps, step],
            expansions=self._expansions,
            final_limit=self._final_limit,
        )

    def _with_expansion(self, expansion: dict) -> "Query":
        return Query(
            self._core,
            self._root_kind,
            steps=self._steps,
            expansions=[*self._expansions, expansion],
            final_limit=self._final_limit,
        )

    def _with_limit(self, limit: int | None) -> "Query":
        return Query(
            self._core,
            self._root_kind,
            steps=self._steps,
            expansions=self._expansions,
            final_limit=limit,
        )

    def _ast_payload(self) -> str:
        return json.dumps(
            {
                "root_kind": self._root_kind,
                "steps": self._steps,
                "expansions": self._expansions,
                "final_limit": self._final_limit,
            }
        )

    def vector_search(self, query: str, limit: int) -> "Query":
        """Add a vector similarity search step.

        Args:
            query: The text query to embed and search against.
            limit: Maximum number of nearest neighbours to return.
        """
        return self._with_step({"type": "vector_search", "query": query, "limit": limit})

    def text_search(self, query: str, limit: int) -> "TextSearchBuilder":
        """Enter the adaptive text-search surface.

        Returns a :class:`TextSearchBuilder` whose ``.execute()`` produces a
        :class:`~fathomdb.SearchRows`. The adaptive pipeline runs the user's
        query strictly first and falls back to a relaxed branch if the
        strict branch produces no hits. Filter methods on the returned
        builder mirror :class:`Query`'s filter surface so the same filter
        chain composes on both paths.

        Args:
            query: Raw user text. Parsed engine-side.
            limit: Maximum number of results per branch.
        """
        return TextSearchBuilder(
            core=self._core,
            root_kind=self._root_kind,
            strict_query=query,
            limit=limit,
        )

    def traverse(
        self,
        *,
        direction: TraverseDirection | str,
        label: str,
        max_depth: int,
    ) -> "Query":
        """Traverse edges from matched nodes.

        Args:
            direction: "in" or "out" relative to current nodes.
            label: Edge kind to follow.
            max_depth: Maximum traversal depth.
        """
        value = direction.value if isinstance(direction, TraverseDirection) else direction
        return self._with_step(
            {
                "type": "traverse",
                "direction": value,
                "label": label,
                "max_depth": max_depth,
            }
        )

    def filter_logical_id_eq(self, logical_id: str) -> "Query":
        """Filter nodes to those with the given logical ID."""
        return self._with_step({"type": "filter_logical_id_eq", "logical_id": logical_id})

    def filter_kind_eq(self, kind: str) -> "Query":
        """Filter nodes to those with the given kind."""
        return self._with_step({"type": "filter_kind_eq", "kind": kind})

    def filter_source_ref_eq(self, source_ref: str) -> "Query":
        """Filter nodes to those with the given source reference."""
        return self._with_step({"type": "filter_source_ref_eq", "source_ref": source_ref})

    def filter_content_ref_not_null(self) -> "Query":
        """Filter nodes to those where ``content_ref`` is not NULL."""
        return self._with_step({"type": "filter_content_ref_not_null"})

    def filter_content_ref_eq(self, content_ref: str) -> "Query":
        """Filter nodes to those with the given ``content_ref`` URI."""
        return self._with_step({"type": "filter_content_ref_eq", "content_ref": content_ref})

    def filter_json_text_eq(self, path: str, value: str) -> "Query":
        """Filter nodes where the JSON property at *path* equals *value*."""
        return self._with_step({"type": "filter_json_text_eq", "path": path, "value": value})

    def filter_json_bool_eq(self, path: str, value: bool) -> "Query":
        """Filter nodes where the JSON boolean at *path* equals *value*."""
        return self._with_step({"type": "filter_json_bool_eq", "path": path, "value": value})

    def filter_json_integer_gt(self, path: str, value: int) -> "Query":
        """Filter nodes where the JSON integer at *path* is greater than *value*."""
        return self._with_step({"type": "filter_json_integer_gt", "path": path, "value": value})

    def filter_json_integer_gte(self, path: str, value: int) -> "Query":
        """Filter nodes where the JSON integer at *path* is greater than or equal to *value*."""
        return self._with_step({"type": "filter_json_integer_gte", "path": path, "value": value})

    def filter_json_integer_lt(self, path: str, value: int) -> "Query":
        """Filter nodes where the JSON integer at *path* is less than *value*."""
        return self._with_step({"type": "filter_json_integer_lt", "path": path, "value": value})

    def filter_json_integer_lte(self, path: str, value: int) -> "Query":
        """Filter nodes where the JSON integer at *path* is less than or equal to *value*."""
        return self._with_step({"type": "filter_json_integer_lte", "path": path, "value": value})

    def filter_json_timestamp_gt(self, path: str, value: int) -> "Query":
        """Filter nodes where the JSON timestamp at *path* is after *value*."""
        return self._with_step({"type": "filter_json_timestamp_gt", "path": path, "value": value})

    def filter_json_timestamp_gte(self, path: str, value: int) -> "Query":
        """Filter nodes where the JSON timestamp at *path* is at or after *value*."""
        return self._with_step({"type": "filter_json_timestamp_gte", "path": path, "value": value})

    def filter_json_timestamp_lt(self, path: str, value: int) -> "Query":
        """Filter nodes where the JSON timestamp at *path* is before *value*."""
        return self._with_step({"type": "filter_json_timestamp_lt", "path": path, "value": value})

    def filter_json_timestamp_lte(self, path: str, value: int) -> "Query":
        """Filter nodes where the JSON timestamp at *path* is at or before *value*."""
        return self._with_step({"type": "filter_json_timestamp_lte", "path": path, "value": value})

    def expand(
        self,
        *,
        slot: str,
        direction: TraverseDirection | str,
        label: str,
        max_depth: int,
    ) -> "Query":
        """Register a named expansion slot for grouped query execution.

        Args:
            slot: Name for this expansion in the grouped result.
            direction: "in" or "out" relative to root nodes.
            label: Edge kind to follow.
            max_depth: Maximum traversal depth.
        """
        value = direction.value if isinstance(direction, TraverseDirection) else direction
        return self._with_expansion(
            {
                "slot": slot,
                "direction": value,
                "label": label,
                "max_depth": max_depth,
            }
        )

    def limit(self, limit: int) -> "Query":
        """Cap the number of result rows returned by the query."""
        return self._with_limit(limit)

    def compile(self, *, progress_callback=None, feedback_config: FeedbackConfig | None = None) -> CompiledQuery:
        """Compile the query into SQL without executing it."""
        return CompiledQuery.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="query.compile",
                    metadata={"root_kind": self._root_kind},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.compile_ast(self._ast_payload()),
                )
            )
        )

    def compile_grouped(
        self, *, progress_callback=None, feedback_config: FeedbackConfig | None = None
    ) -> CompiledGroupedQuery:
        """Compile the query with expansions into SQL without executing it."""
        return CompiledGroupedQuery.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="query.compile_grouped",
                    metadata={"root_kind": self._root_kind},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.compile_grouped_ast(self._ast_payload()),
                )
            )
        )

    def explain(self, *, progress_callback=None, feedback_config: FeedbackConfig | None = None) -> QueryPlan:
        """Return the query execution plan without running the query."""
        return QueryPlan.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="query.explain",
                    metadata={"root_kind": self._root_kind},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.explain_ast(self._ast_payload()),
                )
            )
        )

    def execute(self, *, progress_callback=None, feedback_config: FeedbackConfig | None = None) -> QueryRows:
        """Execute the query and return matching rows."""
        return QueryRows.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="query.execute",
                    metadata={"root_kind": self._root_kind},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.execute_ast(self._ast_payload()),
                )
            )
        )

    def execute_grouped(
        self, *, progress_callback=None, feedback_config: FeedbackConfig | None = None
    ) -> GroupedQueryRows:
        """Execute the query with expansions and return grouped rows."""
        return GroupedQueryRows.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="query.execute_grouped",
                    metadata={"root_kind": self._root_kind},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.execute_grouped_ast(self._ast_payload()),
                )
            )
        )


class _SearchBuilderBase:
    """Shared filter surface for adaptive and fallback search builders."""

    _mode: str

    def __init__(
        self,
        *,
        core: EngineCore,
        root_kind: str,
        strict_query: str,
        limit: int,
        relaxed_query: str | None = None,
        filters: list[dict] | None = None,
        attribution_requested: bool = False,
    ) -> None:
        self._core = core
        self._root_kind = root_kind
        self._strict_query = strict_query
        self._relaxed_query = relaxed_query
        self._limit = limit
        self._filters: list[dict] = list(filters or [])
        self._attribution_requested = attribution_requested

    def _clone(self, **overrides):
        params = {
            "core": self._core,
            "root_kind": self._root_kind,
            "strict_query": self._strict_query,
            "relaxed_query": self._relaxed_query,
            "limit": self._limit,
            "filters": list(self._filters),
            "attribution_requested": self._attribution_requested,
        }
        params.update(overrides)
        return type(self)(**params)

    def _with_filter(self, filter_step: dict):
        return self._clone(filters=[*self._filters, filter_step])

    def _request_payload(self) -> str:
        return json.dumps(
            {
                "mode": self._mode,
                "root_kind": self._root_kind,
                "strict_query": self._strict_query,
                "relaxed_query": self._relaxed_query,
                "limit": self._limit,
                "filters": self._filters,
                "attribution_requested": self._attribution_requested,
            }
        )

    def with_match_attribution(self):
        """Request per-hit match attribution on the returned rows."""
        return self._clone(attribution_requested=True)

    def filter_kind_eq(self, kind: str):
        """Filter hits to those with the given kind."""
        return self._with_filter({"type": "filter_kind_eq", "kind": kind})

    def filter_logical_id_eq(self, logical_id: str):
        """Filter hits to those with the given logical ID."""
        return self._with_filter({"type": "filter_logical_id_eq", "logical_id": logical_id})

    def filter_source_ref_eq(self, source_ref: str):
        """Filter hits to those with the given source reference."""
        return self._with_filter({"type": "filter_source_ref_eq", "source_ref": source_ref})

    def filter_content_ref_eq(self, content_ref: str):
        """Filter hits to those with the given ``content_ref`` URI."""
        return self._with_filter({"type": "filter_content_ref_eq", "content_ref": content_ref})

    def filter_content_ref_not_null(self):
        """Filter hits to those whose ``content_ref`` is not NULL."""
        return self._with_filter({"type": "filter_content_ref_not_null"})

    def filter_json_text_eq(self, path: str, value: str):
        """Filter hits where the JSON property at *path* equals *value*."""
        return self._with_filter({"type": "filter_json_text_eq", "path": path, "value": value})

    def filter_json_bool_eq(self, path: str, value: bool):
        """Filter hits where the JSON boolean at *path* equals *value*."""
        return self._with_filter({"type": "filter_json_bool_eq", "path": path, "value": value})

    def filter_json_integer_gt(self, path: str, value: int):
        """Filter hits where the JSON integer at *path* is greater than *value*."""
        return self._with_filter({"type": "filter_json_integer_gt", "path": path, "value": value})

    def filter_json_integer_gte(self, path: str, value: int):
        """Filter hits where the JSON integer at *path* is greater than or equal to *value*."""
        return self._with_filter({"type": "filter_json_integer_gte", "path": path, "value": value})

    def filter_json_integer_lt(self, path: str, value: int):
        """Filter hits where the JSON integer at *path* is less than *value*."""
        return self._with_filter({"type": "filter_json_integer_lt", "path": path, "value": value})

    def filter_json_integer_lte(self, path: str, value: int):
        """Filter hits where the JSON integer at *path* is less than or equal to *value*."""
        return self._with_filter({"type": "filter_json_integer_lte", "path": path, "value": value})

    def filter_json_timestamp_gt(self, path: str, value: int):
        """Filter hits where the JSON timestamp at *path* is after *value*."""
        return self._with_filter({"type": "filter_json_timestamp_gt", "path": path, "value": value})

    def filter_json_timestamp_gte(self, path: str, value: int):
        """Filter hits where the JSON timestamp at *path* is at or after *value*."""
        return self._with_filter({"type": "filter_json_timestamp_gte", "path": path, "value": value})

    def filter_json_timestamp_lt(self, path: str, value: int):
        """Filter hits where the JSON timestamp at *path* is before *value*."""
        return self._with_filter({"type": "filter_json_timestamp_lt", "path": path, "value": value})

    def filter_json_timestamp_lte(self, path: str, value: int):
        """Filter hits where the JSON timestamp at *path* is at or before *value*."""
        return self._with_filter({"type": "filter_json_timestamp_lte", "path": path, "value": value})

    def _execute(
        self,
        *,
        operation_kind: str,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> SearchRows:
        payload = self._request_payload()
        return SearchRows.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind=operation_kind,
                    metadata={"root_kind": self._root_kind},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.execute_search(payload),
                )
            )
        )


class TextSearchBuilder(_SearchBuilderBase):
    """Fluent builder for adaptive :meth:`Query.text_search` execution.

    Returned from :meth:`Query.text_search`. Terminal :meth:`execute` ships
    the request to the engine's adaptive search pipeline (strict branch
    first, relaxed fallback if the strict branch yields nothing).
    """

    _mode = "text_search"

    def __init__(
        self,
        *,
        core: EngineCore,
        root_kind: str,
        strict_query: str,
        limit: int,
        filters: list[dict] | None = None,
        attribution_requested: bool = False,
        relaxed_query: str | None = None,
    ) -> None:
        super().__init__(
            core=core,
            root_kind=root_kind,
            strict_query=strict_query,
            limit=limit,
            relaxed_query=None,
            filters=filters,
            attribution_requested=attribution_requested,
        )

    def execute(
        self, *, progress_callback=None, feedback_config: FeedbackConfig | None = None
    ) -> SearchRows:
        """Execute the adaptive text search and return :class:`SearchRows`."""
        return self._execute(
            operation_kind="query.text_search",
            progress_callback=progress_callback,
            feedback_config=feedback_config,
        )


class FallbackSearchBuilder(_SearchBuilderBase):
    """Fluent builder for explicit two-shape fallback search.

    Returned from :meth:`Engine.fallback_search`. The caller supplies both
    the strict query and an optional relaxed query verbatim — neither is
    adaptively rewritten. Terminal :meth:`execute` returns
    :class:`SearchRows`.
    """

    _mode = "fallback_search"

    def execute(
        self, *, progress_callback=None, feedback_config: FeedbackConfig | None = None
    ) -> SearchRows:
        """Execute the explicit fallback search and return :class:`SearchRows`."""
        return self._execute(
            operation_kind="query.fallback_search",
            progress_callback=progress_callback,
            feedback_config=feedback_config,
        )
