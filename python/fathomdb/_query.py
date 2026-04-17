from __future__ import annotations

import json
from typing import TYPE_CHECKING

from ._feedback import run_with_feedback
from ._fathomdb import EngineCore
from .errors import BuilderValidationError
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

if TYPE_CHECKING:
    # `typing.Self` is 3.11+. The project targets >=3.10, so pull it from
    # `typing_extensions` under a TYPE_CHECKING guard — no runtime cost and
    # no new runtime dependency.
    from typing_extensions import Self


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
        return self._with_step(
            {"type": "vector_search", "query": query, "limit": limit}
        )

    def search(self, query: str, limit: int) -> "SearchBuilder":
        """Enter the unified :class:`SearchBuilder` surface.

        Returns a :class:`SearchBuilder` whose ``.execute()`` produces a
        :class:`~fathomdb.SearchRows`. Unlike :meth:`text_search`, the
        unified ``search`` entry point runs a single strict branch with no
        adaptive relaxation — it mirrors the Phase 12 Rust ``SearchBuilder``
        surface verbatim. The filter surface matches
        :class:`TextSearchBuilder` so the same chain composes on either
        path.

        Args:
            query: Raw user text. Parsed engine-side.
            limit: Maximum number of results.
        """
        return SearchBuilder(
            core=self._core,
            root_kind=self._root_kind,
            strict_query=query,
            limit=limit,
        )

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
        value = (
            direction.value if isinstance(direction, TraverseDirection) else direction
        )
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
        return self._with_step(
            {"type": "filter_logical_id_eq", "logical_id": logical_id}
        )

    def filter_kind_eq(self, kind: str) -> "Query":
        """Filter nodes to those with the given kind."""
        return self._with_step({"type": "filter_kind_eq", "kind": kind})

    def filter_source_ref_eq(self, source_ref: str) -> "Query":
        """Filter nodes to those with the given source reference."""
        return self._with_step(
            {"type": "filter_source_ref_eq", "source_ref": source_ref}
        )

    def filter_content_ref_not_null(self) -> "Query":
        """Filter nodes to those where ``content_ref`` is not NULL."""
        return self._with_step({"type": "filter_content_ref_not_null"})

    def filter_content_ref_eq(self, content_ref: str) -> "Query":
        """Filter nodes to those with the given ``content_ref`` URI."""
        return self._with_step(
            {"type": "filter_content_ref_eq", "content_ref": content_ref}
        )

    def filter_json_text_eq(self, path: str, value: str) -> "Query":
        """Filter nodes where the JSON property at *path* equals *value*."""
        return self._with_step(
            {"type": "filter_json_text_eq", "path": path, "value": value}
        )

    def filter_json_bool_eq(self, path: str, value: bool) -> "Query":
        """Filter nodes where the JSON boolean at *path* equals *value*."""
        return self._with_step(
            {"type": "filter_json_bool_eq", "path": path, "value": value}
        )

    def filter_json_integer_gt(self, path: str, value: int) -> "Query":
        """Filter nodes where the JSON integer at *path* is greater than *value*."""
        return self._with_step(
            {"type": "filter_json_integer_gt", "path": path, "value": value}
        )

    def filter_json_integer_gte(self, path: str, value: int) -> "Query":
        """Filter nodes where the JSON integer at *path* is greater than or equal to *value*."""
        return self._with_step(
            {"type": "filter_json_integer_gte", "path": path, "value": value}
        )

    def filter_json_integer_lt(self, path: str, value: int) -> "Query":
        """Filter nodes where the JSON integer at *path* is less than *value*."""
        return self._with_step(
            {"type": "filter_json_integer_lt", "path": path, "value": value}
        )

    def filter_json_integer_lte(self, path: str, value: int) -> "Query":
        """Filter nodes where the JSON integer at *path* is less than or equal to *value*."""
        return self._with_step(
            {"type": "filter_json_integer_lte", "path": path, "value": value}
        )

    def filter_json_timestamp_gt(self, path: str, value: int) -> "Query":
        """Filter nodes where the JSON timestamp at *path* is after *value*."""
        return self._with_step(
            {"type": "filter_json_timestamp_gt", "path": path, "value": value}
        )

    def filter_json_timestamp_gte(self, path: str, value: int) -> "Query":
        """Filter nodes where the JSON timestamp at *path* is at or after *value*."""
        return self._with_step(
            {"type": "filter_json_timestamp_gte", "path": path, "value": value}
        )

    def filter_json_timestamp_lt(self, path: str, value: int) -> "Query":
        """Filter nodes where the JSON timestamp at *path* is before *value*."""
        return self._with_step(
            {"type": "filter_json_timestamp_lt", "path": path, "value": value}
        )

    def filter_json_timestamp_lte(self, path: str, value: int) -> "Query":
        """Filter nodes where the JSON timestamp at *path* is at or before *value*."""
        return self._with_step(
            {"type": "filter_json_timestamp_lte", "path": path, "value": value}
        )

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
        value = (
            direction.value if isinstance(direction, TraverseDirection) else direction
        )
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

    def compile(
        self, *, progress_callback=None, feedback_config: FeedbackConfig | None = None
    ) -> CompiledQuery:
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
                    operation=lambda: self._core.compile_grouped_ast(
                        self._ast_payload()
                    ),
                )
            )
        )

    def explain(
        self, *, progress_callback=None, feedback_config: FeedbackConfig | None = None
    ) -> QueryPlan:
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

    def execute(
        self, *, progress_callback=None, feedback_config: FeedbackConfig | None = None
    ) -> QueryRows:
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
                    operation=lambda: self._core.execute_grouped_ast(
                        self._ast_payload()
                    ),
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

    def _clone(self, **overrides) -> "Self":
        # `relaxed_query` is intentionally excluded: it is a
        # fallback-only parameter and `TextSearchBuilder.__init__` does
        # not accept it. `FallbackSearchBuilder._clone` overrides this
        # method to re-introduce the parameter.
        params = {
            "core": self._core,
            "root_kind": self._root_kind,
            "strict_query": self._strict_query,
            "limit": self._limit,
            "filters": list(self._filters),
            "attribution_requested": self._attribution_requested,
        }
        params.update(overrides)
        return type(self)(**params)

    def _with_filter(self, filter_step: dict) -> "Self":
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

    def with_match_attribution(self) -> "Self":
        """Request per-hit match attribution on the returned rows."""
        return self._clone(attribution_requested=True)

    def filter_kind_eq(self, kind: str) -> "Self":
        """Filter hits to those with the given kind."""
        return self._with_filter({"type": "filter_kind_eq", "kind": kind})

    def filter_logical_id_eq(self, logical_id: str) -> "Self":
        """Filter hits to those with the given logical ID."""
        return self._with_filter(
            {"type": "filter_logical_id_eq", "logical_id": logical_id}
        )

    def filter_source_ref_eq(self, source_ref: str) -> "Self":
        """Filter hits to those with the given source reference."""
        return self._with_filter(
            {"type": "filter_source_ref_eq", "source_ref": source_ref}
        )

    def filter_content_ref_eq(self, content_ref: str) -> "Self":
        """Filter hits to those with the given ``content_ref`` URI."""
        return self._with_filter(
            {"type": "filter_content_ref_eq", "content_ref": content_ref}
        )

    def filter_content_ref_not_null(self) -> "Self":
        """Filter hits to those whose ``content_ref`` is not NULL."""
        return self._with_filter({"type": "filter_content_ref_not_null"})

    def filter_json_text_eq(self, path: str, value: str) -> "Self":
        """Filter hits where the JSON property at *path* equals *value*."""
        return self._with_filter(
            {"type": "filter_json_text_eq", "path": path, "value": value}
        )

    def filter_json_bool_eq(self, path: str, value: bool) -> "Self":
        """Filter hits where the JSON boolean at *path* equals *value*."""
        return self._with_filter(
            {"type": "filter_json_bool_eq", "path": path, "value": value}
        )

    def filter_json_integer_gt(self, path: str, value: int) -> "Self":
        """Filter hits where the JSON integer at *path* is greater than *value*."""
        return self._with_filter(
            {"type": "filter_json_integer_gt", "path": path, "value": value}
        )

    def filter_json_integer_gte(self, path: str, value: int) -> "Self":
        """Filter hits where the JSON integer at *path* is greater than or equal to *value*."""
        return self._with_filter(
            {"type": "filter_json_integer_gte", "path": path, "value": value}
        )

    def filter_json_integer_lt(self, path: str, value: int) -> "Self":
        """Filter hits where the JSON integer at *path* is less than *value*."""
        return self._with_filter(
            {"type": "filter_json_integer_lt", "path": path, "value": value}
        )

    def filter_json_integer_lte(self, path: str, value: int) -> "Self":
        """Filter hits where the JSON integer at *path* is less than or equal to *value*."""
        return self._with_filter(
            {"type": "filter_json_integer_lte", "path": path, "value": value}
        )

    def filter_json_timestamp_gt(self, path: str, value: int) -> "Self":
        """Filter hits where the JSON timestamp at *path* is after *value*."""
        return self._with_filter(
            {"type": "filter_json_timestamp_gt", "path": path, "value": value}
        )

    def filter_json_timestamp_gte(self, path: str, value: int) -> "Self":
        """Filter hits where the JSON timestamp at *path* is at or after *value*."""
        return self._with_filter(
            {"type": "filter_json_timestamp_gte", "path": path, "value": value}
        )

    def filter_json_timestamp_lt(self, path: str, value: int) -> "Self":
        """Filter hits where the JSON timestamp at *path* is before *value*."""
        return self._with_filter(
            {"type": "filter_json_timestamp_lt", "path": path, "value": value}
        )

    def filter_json_timestamp_lte(self, path: str, value: int) -> "Self":
        """Filter hits where the JSON timestamp at *path* is at or before *value*."""
        return self._with_filter(
            {"type": "filter_json_timestamp_lte", "path": path, "value": value}
        )

    def _fused_validation_kind(self, method: str) -> str:
        """Resolve the kind currently bound to this builder for fusion validation.

        Mirrors the Rust ``validate_fusable_property_path`` helper. For
        ``TextSearchBuilder`` the kind is always ``self._root_kind``.
        ``FallbackSearchBuilder`` overrides this to walk the filter chain
        for the most recent ``filter_kind_eq``.
        """
        if not self._root_kind:
            raise BuilderValidationError(
                f"filter_json_fused_* methods require a specific kind; "
                f"provide a root kind via Engine.nodes(...) or call "
                f"filter_kind_eq(..) before {method!r}, or switch to the "
                f"post-filter filter_json_* family"
            )
        return self._root_kind

    def _validate_fused_property_path(self, kind: str, path: str, method: str) -> None:
        """Client-side fusion gate mirroring the Rust builder contract.

        Raises :class:`BuilderValidationError` if no FTS property schema is
        registered for *kind* or if *path* is not included in the schema.
        """
        try:
            schema_json = self._core.describe_fts_property_schema(kind)
        except Exception as exc:  # pragma: no cover - defensive
            raise BuilderValidationError(
                f"kind {kind!r} has no registered property-FTS schema; "
                f"register one with admin.register_fts_property_schema(..) "
                f"before using filter_json_fused_* methods, or use the "
                f"post-filter filter_json_* family for non-fused semantics"
            ) from exc
        try:
            schema = json.loads(schema_json)
        except json.JSONDecodeError as exc:  # pragma: no cover - defensive
            raise BuilderValidationError(
                f"could not decode property-FTS schema payload for kind {kind!r}"
            ) from exc
        if not schema:
            raise BuilderValidationError(
                f"kind {kind!r} has no registered property-FTS schema; "
                f"register one with admin.register_fts_property_schema(..) "
                f"before using filter_json_fused_* methods, or use the "
                f"post-filter filter_json_* family for non-fused semantics"
            )
        paths = schema.get("property_paths") or []
        if path not in paths:
            raise BuilderValidationError(
                f"kind {kind!r} has a registered property-FTS schema but "
                f"path {path!r} is not in its include list; add the path "
                f"to the schema or use the post-filter filter_json_* family"
            )

    def filter_json_fused_text_eq(self, path: str, value: str) -> "Self":
        """Filter hits where the JSON text property at *path* equals *value*.

        Pushes the predicate into the inner search CTE so the CTE LIMIT
        applies *after* the filter.

        Raises :class:`BuilderValidationError` if the bound kind has no
        registered property-FTS schema or the schema does not cover *path*.
        """
        kind = self._fused_validation_kind("filter_json_fused_text_eq")
        self._validate_fused_property_path(kind, path, "filter_json_fused_text_eq")
        return self._with_filter(
            {"type": "filter_json_fused_text_eq", "path": path, "value": value}
        )

    def filter_json_fused_timestamp_gt(self, path: str, value: int) -> "Self":
        """Fused JSON-timestamp strict-greater filter. See :meth:`filter_json_fused_text_eq`."""
        kind = self._fused_validation_kind("filter_json_fused_timestamp_gt")
        self._validate_fused_property_path(kind, path, "filter_json_fused_timestamp_gt")
        return self._with_filter(
            {"type": "filter_json_fused_timestamp_gt", "path": path, "value": value}
        )

    def filter_json_fused_timestamp_gte(self, path: str, value: int) -> "Self":
        """Fused JSON-timestamp greater-or-equal filter. See :meth:`filter_json_fused_text_eq`."""
        kind = self._fused_validation_kind("filter_json_fused_timestamp_gte")
        self._validate_fused_property_path(
            kind, path, "filter_json_fused_timestamp_gte"
        )
        return self._with_filter(
            {"type": "filter_json_fused_timestamp_gte", "path": path, "value": value}
        )

    def filter_json_fused_timestamp_lt(self, path: str, value: int) -> "Self":
        """Fused JSON-timestamp strict-less filter. See :meth:`filter_json_fused_text_eq`."""
        kind = self._fused_validation_kind("filter_json_fused_timestamp_lt")
        self._validate_fused_property_path(kind, path, "filter_json_fused_timestamp_lt")
        return self._with_filter(
            {"type": "filter_json_fused_timestamp_lt", "path": path, "value": value}
        )

    def filter_json_fused_timestamp_lte(self, path: str, value: int) -> "Self":
        """Fused JSON-timestamp less-or-equal filter. See :meth:`filter_json_fused_text_eq`."""
        kind = self._fused_validation_kind("filter_json_fused_timestamp_lte")
        self._validate_fused_property_path(
            kind, path, "filter_json_fused_timestamp_lte"
        )
        return self._with_filter(
            {"type": "filter_json_fused_timestamp_lte", "path": path, "value": value}
        )

    def filter_json_fused_text_in(self, path: str, values: list[str]) -> "Self":
        """Fused IN-set filter for a JSON text property.

        Pushes the predicate into the search CTE's inner WHERE so the LIMIT
        applies after filtering. Requires an FTS property schema for the bound
        kind that includes ``path``.

        Raises :class:`BuilderValidationError` if the bound kind has no
        registered property-FTS schema, or if ``path`` is not indexed.
        """
        if not values:
            raise ValueError("filter_json_fused_text_in: values must not be empty")
        kind = self._fused_validation_kind("filter_json_fused_text_in")
        self._validate_fused_property_path(kind, path, "filter_json_fused_text_in")
        return self._with_filter(
            {"type": "filter_json_fused_text_in", "path": path, "values": values}
        )

    def filter_json_text_in(self, path: str, values: list[str]) -> "Self":
        """Non-fused IN-set filter for a JSON text property.

        Applied as a residual WHERE clause on the nodes driver scan.
        No FTS property schema is required.
        """
        if not values:
            raise ValueError("filter_json_text_in: values must not be empty")
        return self._with_filter(
            {"type": "filter_json_text_in", "path": path, "values": values}
        )

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
    ) -> None:
        # Adaptive text_search never takes a caller-supplied relaxed
        # query — the relaxed branch is derived engine-side via
        # `derive_relaxed`. Passing `relaxed_query=...` here is a
        # programming error (see `FallbackSearchBuilder` for the
        # explicit two-shape path); reject it at the boundary.
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

    def _fused_validation_kind(self, method: str) -> str:
        """Override for FallbackSearchBuilder: resolve kind from filter chain.

        The fallback builder is kind-agnostic by default (``root_kind=""``).
        Fusion requires a concrete kind to look up an FTS property schema,
        so walk the accumulated filter chain for the most recent
        ``filter_kind_eq`` step. If neither a root kind nor a chained
        ``filter_kind_eq`` is present, raise
        :class:`BuilderValidationError`.
        """
        if self._root_kind:
            return self._root_kind
        for step in reversed(self._filters):
            if step.get("type") == "filter_kind_eq":
                kind = step.get("kind")
                if kind:
                    return kind
        raise BuilderValidationError(
            f"filter_json_fused_* methods require a specific kind; call "
            f"filter_kind_eq(..) before {method!r} or switch to the "
            f"post-filter filter_json_* family"
        )

    def _clone(self, **overrides) -> "Self":
        # Override the base clone to thread `relaxed_query` through, which
        # is excluded from `_SearchBuilderBase._clone` because
        # `TextSearchBuilder.__init__` does not accept it.
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

    def execute(
        self, *, progress_callback=None, feedback_config: FeedbackConfig | None = None
    ) -> SearchRows:
        """Execute the explicit fallback search and return :class:`SearchRows`."""
        return self._execute(
            operation_kind="query.fallback_search",
            progress_callback=progress_callback,
            feedback_config=feedback_config,
        )


class SearchBuilder(_SearchBuilderBase):
    """Tethered builder returned by :meth:`Query.search`.

    Runs the Phase 12 unified retrieval planner: a strict text branch, a
    relaxed text branch derived engine-side from the strict query via
    ``derive_relaxed`` (fires when the strict branch underflows), and a
    reserved vector stage that stays dormant in v1 because read-time
    embedding of natural-language queries is not yet wired in.

    Callers chain filter methods and :meth:`with_match_attribution` on
    top of :meth:`Query.search` the same way they do on
    :meth:`Query.text_search`. Terminal :meth:`execute` always returns
    :class:`SearchRows`. For an explicit caller-supplied relaxed query
    without engine-side derivation, use :meth:`Engine.fallback_search`.
    """

    _mode = "search"

    def __init__(
        self,
        *,
        core: EngineCore,
        root_kind: str,
        strict_query: str,
        limit: int,
        filters: list[dict] | None = None,
        attribution_requested: bool = False,
        expansions: list[dict] | None = None,
        expand_limit: int | None = None,
    ) -> None:
        # The unified `search` mode, like adaptive `text_search`, never
        # takes a caller-supplied relaxed query. The wire shape always
        # sets `relaxed_query` to null.
        super().__init__(
            core=core,
            root_kind=root_kind,
            strict_query=strict_query,
            limit=limit,
            relaxed_query=None,
            filters=filters,
            attribution_requested=attribution_requested,
        )
        self._expansions: list[dict] = list(expansions or [])
        self._expand_limit: int | None = expand_limit

    def _clone(self, **overrides) -> "SearchBuilder":
        params = {
            "core": self._core,
            "root_kind": self._root_kind,
            "strict_query": self._strict_query,
            "limit": self._limit,
            "filters": list(self._filters),
            "attribution_requested": self._attribution_requested,
            "expansions": list(self._expansions),
            "expand_limit": self._expand_limit,
        }
        params.update(overrides)
        return type(self)(**params)

    def expand(
        self,
        *,
        slot: str,
        direction: "TraverseDirection | str",
        label: str,
        max_depth: int,
        filter: dict | None = None,
    ) -> "SearchBuilder":
        """Register a named expansion slot for grouped query execution."""
        value = (
            direction.value if isinstance(direction, TraverseDirection) else direction
        )
        expansion = {
            "slot": slot,
            "direction": value,
            "label": label,
            "max_depth": max_depth,
            "filter": filter,
        }
        return self._clone(expansions=[*self._expansions, expansion])

    def limit(self, limit: int) -> "SearchBuilder":
        """Cap the per-originator expansion result count."""
        return self._clone(expand_limit=limit)

    def _grouped_ast_payload(self) -> str:
        return json.dumps(
            {
                "root_kind": self._root_kind,
                "steps": [
                    {
                        "type": "text_search",
                        "query": self._strict_query,
                        "limit": self._limit,
                    }
                ],
                "expansions": self._expansions,
                "final_limit": self._expand_limit,
            }
        )

    def compile_grouped(
        self, *, progress_callback=None, feedback_config: FeedbackConfig | None = None
    ) -> CompiledGroupedQuery:
        """Compile the search + expansion into grouped SQL without executing."""
        return CompiledGroupedQuery.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="query.compile_grouped",
                    metadata={"root_kind": self._root_kind},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.compile_grouped_ast(
                        self._grouped_ast_payload()
                    ),
                )
            )
        )

    def execute_grouped(
        self, *, progress_callback=None, feedback_config: FeedbackConfig | None = None
    ) -> GroupedQueryRows:
        """Execute the search + expansion and return grouped rows."""
        return GroupedQueryRows.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="query.execute_grouped",
                    metadata={"root_kind": self._root_kind},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.execute_grouped_ast(
                        self._grouped_ast_payload()
                    ),
                )
            )
        )

    def execute(
        self, *, progress_callback=None, feedback_config: FeedbackConfig | None = None
    ) -> SearchRows:
        """Execute the unified search and return :class:`SearchRows`."""
        return self._execute(
            operation_kind="query.search",
            progress_callback=progress_callback,
            feedback_config=feedback_config,
        )
