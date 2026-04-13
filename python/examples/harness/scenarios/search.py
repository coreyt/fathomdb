"""Scenarios exercising the unified ``search()`` surface end-to-end.

Mirror the :mod:`adaptive_search` pack but drive the Phase 12 unified
:meth:`~fathomdb.Query.search` entry point. The unified surface has no
caller-supplied relaxed query but still derives a relaxed branch inside
``compile_retrieval_plan`` so the strict-miss recovery path is observable.

Each sub-scenario opens its own sibling database so the registered FTS
property schemas and seeded nodes do not collide with other harness
scenarios, and teardown falls out of scope when the engine handle is
closed.
"""

from __future__ import annotations

from fathomdb import (
    ChunkInsert,
    ChunkPolicy,
    FtsPropertyPathMode,
    FtsPropertyPathSpec,
    NodeInsert,
    SearchHitSource,
    SearchMatchMode,
    WriteRequest,
    new_row_id,
)

from ..models import HarnessContext, ScenarioResult


_SEARCH_GOAL_KIND = "UnifiedSearchGoal"
_SEARCH_KI_KIND = "UnifiedSearchKnowledgeItem"
_SEARCH_TASK_KIND = "UnifiedSearchTask"


def _seed_search_goals(engine) -> None:
    """Seed Goal nodes plus chunks so the strict branch has work to do."""
    engine.write(
        WriteRequest(
            label="unified-search-seed-goals",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="unified-search-budget",
                    kind=_SEARCH_GOAL_KIND,
                    properties={"title": "Budget meeting"},
                    source_ref="unified-search-seed",
                    upsert=False,
                    chunk_policy=ChunkPolicy.PRESERVE,
                ),
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="unified-search-quarterly",
                    kind=_SEARCH_GOAL_KIND,
                    properties={"title": "Quarterly planning"},
                    source_ref="unified-search-seed",
                    upsert=False,
                    chunk_policy=ChunkPolicy.PRESERVE,
                ),
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="unified-search-roadmap",
                    kind=_SEARCH_GOAL_KIND,
                    properties={"title": "Engineering roadmap"},
                    source_ref="unified-search-seed",
                    upsert=False,
                    chunk_policy=ChunkPolicy.PRESERVE,
                ),
            ],
            chunks=[
                ChunkInsert(
                    id="unified-search-budget-chunk",
                    node_logical_id="unified-search-budget",
                    text_content="budget meeting notes for finance review",
                ),
                ChunkInsert(
                    id="unified-search-quarterly-chunk",
                    node_logical_id="unified-search-quarterly",
                    text_content="quarterly planning docs and action items",
                ),
                ChunkInsert(
                    id="unified-search-roadmap-chunk",
                    node_logical_id="unified-search-roadmap",
                    text_content="engineering roadmap deliverables",
                ),
            ],
        )
    )


def unified_search_strict_hit_populates_rows(context: HarnessContext) -> ScenarioResult:
    """Strict branch finds hits — derived relaxed branch never fires."""
    db_path = context.sibling_db("unified-search-strict-hit")
    engine = context.open_engine(db_path)
    try:
        _seed_search_goals(engine)
        rows = (
            engine.nodes(_SEARCH_GOAL_KIND)
            .search("budget meeting", 10)
            .execute()
        )
        assert len(rows.hits) == 1, f"expected 1 hit, got {len(rows.hits)}"
        hit = rows.hits[0]
        assert hit.node.logical_id == "unified-search-budget", (
            f"unexpected hit {hit.node.logical_id!r}"
        )
        assert hit.match_mode == SearchMatchMode.STRICT, (
            f"expected STRICT, got {hit.match_mode}"
        )
        assert rows.fallback_used is False, "fallback must not fire on strict hit"
        assert rows.was_degraded is False, "search must not degrade"
        assert rows.strict_hit_count == 1, (
            f"strict_hit_count={rows.strict_hit_count}"
        )
        assert rows.relaxed_hit_count == 0, (
            f"relaxed_hit_count={rows.relaxed_hit_count}"
        )
        assert rows.vector_hit_count == 0, (
            f"v1 vector branch must stay empty; got {rows.vector_hit_count}"
        )
    finally:
        engine.close()
    return ScenarioResult(name="unified_search_strict_hit_populates_rows")


def unified_search_strict_miss_relaxed_recovery(
    context: HarnessContext,
) -> ScenarioResult:
    """Strict branch misses on an implicit-AND term; derived relaxed recovers."""
    db_path = context.sibling_db("unified-search-relaxed-recovery")
    engine = context.open_engine(db_path)
    try:
        _seed_search_goals(engine)
        rows = (
            engine.nodes(_SEARCH_GOAL_KIND)
            .search("budget nonexistentxyzzy", 10)
            .execute()
        )
        assert rows.fallback_used is True, (
            "strict miss must trigger the derived relaxed branch"
        )
        assert rows.strict_hit_count == 0, (
            f"strict_hit_count={rows.strict_hit_count}"
        )
        assert len(rows.hits) > 0, "relaxed branch should recover at least one hit"
        assert rows.relaxed_hit_count > 0, (
            f"relaxed_hit_count={rows.relaxed_hit_count}"
        )
        assert any(h.match_mode == SearchMatchMode.RELAXED for h in rows.hits), (
            "at least one hit must carry match_mode=RELAXED"
        )
    finally:
        engine.close()
    return ScenarioResult(name="unified_search_strict_miss_relaxed_recovery")


def unified_search_filter_kind_eq_fuses(context: HarnessContext) -> ScenarioResult:
    """``filter_kind_eq`` chained on SearchBuilder restricts to the kind."""
    db_path = context.sibling_db("unified-search-filter-kind")
    engine = context.open_engine(db_path)
    try:
        _seed_search_goals(engine)
        engine.admin.register_fts_property_schema(_SEARCH_TASK_KIND, ["$.title"])
        # Seed a Task that contains the same budget term so the filter has
        # something to exclude.
        engine.write(
            WriteRequest(
                label="unified-search-seed-task",
                nodes=[
                    NodeInsert(
                        row_id=new_row_id(),
                        logical_id="unified-search-task-budget",
                        kind=_SEARCH_TASK_KIND,
                        properties={"title": "budget review"},
                        source_ref="unified-search-seed",
                        upsert=False,
                        chunk_policy=ChunkPolicy.PRESERVE,
                    )
                ],
                chunks=[
                    ChunkInsert(
                        id="unified-search-task-budget-chunk",
                        node_logical_id="unified-search-task-budget",
                        text_content="budget alignment task notes",
                    )
                ],
            )
        )

        filtered = (
            engine.nodes(_SEARCH_GOAL_KIND)
            .search("budget", 10)
            .filter_kind_eq(_SEARCH_GOAL_KIND)
            .execute()
        )
        assert len(filtered.hits) > 0, "strict branch must return at least one Goal"
        assert all(h.node.kind == _SEARCH_GOAL_KIND for h in filtered.hits), (
            f"filter_kind_eq must exclude Task rows; got {[h.node.kind for h in filtered.hits]!r}"
        )
        assert all(h.node.logical_id != "unified-search-task-budget" for h in filtered.hits), (
            "Task row leaked past filter_kind_eq"
        )
    finally:
        engine.close()
    return ScenarioResult(name="unified_search_filter_kind_eq_fuses")


def _register_recursive_payload_schema(engine) -> None:
    engine.admin.register_fts_property_schema_with_entries(
        _SEARCH_KI_KIND,
        [FtsPropertyPathSpec(path="$.payload", mode=FtsPropertyPathMode.RECURSIVE)],
        separator=" ",
        exclude_paths=[],
    )


def _seed_recursive_knowledge_item(engine) -> None:
    engine.write(
        WriteRequest(
            label="unified-search-seed-ki",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="unified-search-ki-alpha",
                    kind=_SEARCH_KI_KIND,
                    properties={
                        "payload": {
                            "title": "quarterly planning",
                            "notes": "budget approval",
                        }
                    },
                    source_ref="unified-search-seed",
                    upsert=False,
                    chunk_policy=ChunkPolicy.PRESERVE,
                )
            ],
        )
    )


def unified_search_with_match_attribution(context: HarnessContext) -> ScenarioResult:
    """Recursive-property schema populates attribution matched_paths."""
    db_path = context.sibling_db("unified-search-attribution")
    engine = context.open_engine(db_path)
    try:
        _register_recursive_payload_schema(engine)
        _seed_recursive_knowledge_item(engine)

        rows = (
            engine.nodes(_SEARCH_KI_KIND)
            .search("quarterly", 10)
            .with_match_attribution()
            .execute()
        )
        assert len(rows.hits) == 1, f"expected 1 hit, got {len(rows.hits)}"
        hit = rows.hits[0]
        assert hit.source == SearchHitSource.PROPERTY, (
            f"recursive property hit must report PROPERTY source; got {hit.source}"
        )
        assert hit.match_mode == SearchMatchMode.STRICT, (
            f"expected STRICT, got {hit.match_mode}"
        )
        assert hit.attribution is not None, "attribution must be populated"
        assert "$.payload.title" in hit.attribution.matched_paths, (
            f"expected $.payload.title in matched_paths; got "
            f"{hit.attribution.matched_paths!r}"
        )
    finally:
        engine.close()
    return ScenarioResult(name="unified_search_with_match_attribution")
