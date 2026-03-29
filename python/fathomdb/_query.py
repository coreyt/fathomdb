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
    TraverseDirection,
)


class Query:
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
        return self._with_step({"type": "vector_search", "query": query, "limit": limit})

    def text_search(self, query: str, limit: int) -> "Query":
        return self._with_step({"type": "text_search", "query": query, "limit": limit})

    def traverse(
        self,
        *,
        direction: TraverseDirection | str,
        label: str,
        max_depth: int,
    ) -> "Query":
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
        return self._with_step({"type": "filter_logical_id_eq", "logical_id": logical_id})

    def filter_kind_eq(self, kind: str) -> "Query":
        return self._with_step({"type": "filter_kind_eq", "kind": kind})

    def filter_source_ref_eq(self, source_ref: str) -> "Query":
        return self._with_step({"type": "filter_source_ref_eq", "source_ref": source_ref})

    def filter_json_text_eq(self, path: str, value: str) -> "Query":
        return self._with_step({"type": "filter_json_text_eq", "path": path, "value": value})

    def filter_json_bool_eq(self, path: str, value: bool) -> "Query":
        return self._with_step({"type": "filter_json_bool_eq", "path": path, "value": value})

    def filter_json_integer_gt(self, path: str, value: int) -> "Query":
        return self._with_step({"type": "filter_json_integer_gt", "path": path, "value": value})

    def filter_json_integer_gte(self, path: str, value: int) -> "Query":
        return self._with_step({"type": "filter_json_integer_gte", "path": path, "value": value})

    def filter_json_integer_lt(self, path: str, value: int) -> "Query":
        return self._with_step({"type": "filter_json_integer_lt", "path": path, "value": value})

    def filter_json_integer_lte(self, path: str, value: int) -> "Query":
        return self._with_step({"type": "filter_json_integer_lte", "path": path, "value": value})

    def filter_json_timestamp_gt(self, path: str, value: int) -> "Query":
        return self._with_step({"type": "filter_json_timestamp_gt", "path": path, "value": value})

    def filter_json_timestamp_gte(self, path: str, value: int) -> "Query":
        return self._with_step({"type": "filter_json_timestamp_gte", "path": path, "value": value})

    def filter_json_timestamp_lt(self, path: str, value: int) -> "Query":
        return self._with_step({"type": "filter_json_timestamp_lt", "path": path, "value": value})

    def filter_json_timestamp_lte(self, path: str, value: int) -> "Query":
        return self._with_step({"type": "filter_json_timestamp_lte", "path": path, "value": value})

    def expand(
        self,
        *,
        slot: str,
        direction: TraverseDirection | str,
        label: str,
        max_depth: int,
    ) -> "Query":
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
        return self._with_limit(limit)

    def compile(self, *, progress_callback=None, feedback_config: FeedbackConfig | None = None) -> CompiledQuery:
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
