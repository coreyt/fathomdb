"""Scenarios exercising the adaptive text-search surface end-to-end.

Each sub-scenario opens its own sibling database so the registered FTS
property schemas and seeded nodes do not collide with other harness
scenarios, and teardown falls out of scope when the engine handle is
closed. The scenarios mirror the dedicated fixtures in
``python/tests/test_text_search_surface.py`` and
``crates/fathomdb/tests/text_search_surface.rs`` but drive the public
SDK surface through the scenario runner rather than pytest.
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


_ADAPTIVE_GOAL_KIND = "AdaptiveGoal"
_ADAPTIVE_KI_KIND = "AdaptiveKnowledgeItem"


def _seed_adaptive_goals(engine) -> None:
    """Seed three Goal-like nodes with deterministic chunk text."""
    engine.write(
        WriteRequest(
            label="adaptive-seed-goals",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="adaptive-goal-budget",
                    kind=_ADAPTIVE_GOAL_KIND,
                    properties={"title": "Budget meeting"},
                    source_ref="adaptive-seed",
                    upsert=False,
                    chunk_policy=ChunkPolicy.PRESERVE,
                ),
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="adaptive-goal-quarterly",
                    kind=_ADAPTIVE_GOAL_KIND,
                    properties={"title": "Quarterly planning"},
                    source_ref="adaptive-seed",
                    upsert=False,
                    chunk_policy=ChunkPolicy.PRESERVE,
                ),
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="adaptive-goal-roadmap",
                    kind=_ADAPTIVE_GOAL_KIND,
                    properties={"title": "Engineering roadmap"},
                    source_ref="adaptive-seed",
                    upsert=False,
                    chunk_policy=ChunkPolicy.PRESERVE,
                ),
            ],
            chunks=[
                ChunkInsert(
                    id="adaptive-goal-budget-chunk",
                    node_logical_id="adaptive-goal-budget",
                    text_content="budget meeting notes for finance review",
                ),
                ChunkInsert(
                    id="adaptive-goal-quarterly-chunk",
                    node_logical_id="adaptive-goal-quarterly",
                    text_content="quarterly planning docs and action items",
                ),
                ChunkInsert(
                    id="adaptive-goal-roadmap-chunk",
                    node_logical_id="adaptive-goal-roadmap",
                    text_content="engineering roadmap deliverables",
                ),
            ],
        )
    )


def adaptive_search_strict_hit_only(context: HarnessContext) -> ScenarioResult:
    """Strict branch finds hits — relaxed fallback never fires."""
    db_path = context.sibling_db("adaptive-strict-hit-only")
    engine = context.open_engine(db_path)
    try:
        _seed_adaptive_goals(engine)
        rows = (
            engine.nodes(_ADAPTIVE_GOAL_KIND)
            .text_search("budget meeting", 10)
            .execute()
        )
        assert len(rows.hits) == 1, f"expected 1 hit, got {len(rows.hits)}"
        hit = rows.hits[0]
        assert hit.node.logical_id == "adaptive-goal-budget", (
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
    finally:
        engine.close()
    return ScenarioResult(name="adaptive_search_strict_hit_only")


def adaptive_search_strict_miss_relaxed_recovery(
    context: HarnessContext,
) -> ScenarioResult:
    """Strict branch misses on an implicit-AND term; relaxed branch recovers."""
    db_path = context.sibling_db("adaptive-relaxed-recovery")
    engine = context.open_engine(db_path)
    try:
        _seed_adaptive_goals(engine)
        rows = (
            engine.nodes(_ADAPTIVE_GOAL_KIND)
            .text_search("budget nonexistentxyzzy", 10)
            .execute()
        )
        assert rows.fallback_used is True, (
            "strict miss must trigger the relaxed fallback branch"
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
    return ScenarioResult(name="adaptive_search_strict_miss_relaxed_recovery")


def adaptive_search_mixed_chunk_and_property(
    context: HarnessContext,
) -> ScenarioResult:
    """Collapse chunk+property hits for the same node into a single chunk hit."""
    db_path = context.sibling_db("adaptive-mixed-chunk-property")
    engine = context.open_engine(db_path)
    try:
        # Register a scalar property schema on $.title so the property
        # branch matches the same term that the chunk branch matches.
        engine.admin.register_fts_property_schema(_ADAPTIVE_GOAL_KIND, ["$.title"])
        engine.write(
            WriteRequest(
                label="adaptive-dual-match",
                nodes=[
                    NodeInsert(
                        row_id=new_row_id(),
                        logical_id="adaptive-dual",
                        kind=_ADAPTIVE_GOAL_KIND,
                        properties={"title": "dualmatchneedle target"},
                        source_ref="adaptive-seed",
                        upsert=False,
                        chunk_policy=ChunkPolicy.PRESERVE,
                    )
                ],
                chunks=[
                    ChunkInsert(
                        id="adaptive-dual-chunk",
                        node_logical_id="adaptive-dual",
                        text_content="dualmatchneedle target appears in chunk body",
                    )
                ],
            )
        )
        rows = (
            engine.nodes(_ADAPTIVE_GOAL_KIND)
            .text_search("dualmatchneedle", 10)
            .execute()
        )
        assert len(rows.hits) == 1, (
            f"dedup must collapse chunk+property to one hit; got {len(rows.hits)}"
        )
        hit = rows.hits[0]
        assert hit.node.logical_id == "adaptive-dual", (
            f"unexpected hit {hit.node.logical_id!r}"
        )
        assert hit.source == SearchHitSource.CHUNK, (
            f"chunk must win the source tiebreak; got {hit.source}"
        )
    finally:
        engine.close()
    return ScenarioResult(name="adaptive_search_mixed_chunk_and_property")


def _register_recursive_payload_schema(engine) -> None:
    engine.admin.register_fts_property_schema_with_entries(
        _ADAPTIVE_KI_KIND,
        [FtsPropertyPathSpec(path="$.payload", mode=FtsPropertyPathMode.RECURSIVE)],
        separator=" ",
        exclude_paths=[],
    )


def _seed_recursive_knowledge_item(engine) -> None:
    engine.write(
        WriteRequest(
            label="adaptive-seed-ki",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="adaptive-ki-alpha",
                    kind=_ADAPTIVE_KI_KIND,
                    properties={
                        "payload": {
                            "title": "quarterly planning",
                            "notes": "budget approval",
                        }
                    },
                    source_ref="adaptive-seed",
                    upsert=False,
                    chunk_policy=ChunkPolicy.PRESERVE,
                )
            ],
        )
    )


def adaptive_search_recursive_nested_payload(
    context: HarnessContext,
) -> ScenarioResult:
    """Recursive-property schema populates attribution matched_paths."""
    db_path = context.sibling_db("adaptive-recursive-payload")
    engine = context.open_engine(db_path)
    try:
        _register_recursive_payload_schema(engine)
        _seed_recursive_knowledge_item(engine)

        rows = (
            engine.nodes(_ADAPTIVE_KI_KIND)
            .text_search("quarterly", 10)
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

        # Multi-term: both leaves must appear in matched_paths in
        # first-match-offset order.
        rows_multi = (
            engine.nodes(_ADAPTIVE_KI_KIND)
            .text_search("quarterly budget", 10)
            .with_match_attribution()
            .execute()
        )
        assert len(rows_multi.hits) == 1, (
            f"expected 1 hit for multi-term, got {len(rows_multi.hits)}"
        )
        multi_hit = rows_multi.hits[0]
        assert multi_hit.attribution is not None, (
            "attribution must be populated on multi-term hit"
        )
        paths = multi_hit.attribution.matched_paths
        assert "$.payload.title" in paths, (
            f"expected $.payload.title in matched_paths; got {paths!r}"
        )
        assert "$.payload.notes" in paths, (
            f"expected $.payload.notes in matched_paths; got {paths!r}"
        )
    finally:
        engine.close()
    return ScenarioResult(name="adaptive_search_recursive_nested_payload")


def adaptive_search_recursive_rebuild_restore(
    context: HarnessContext,
) -> ScenarioResult:
    """Recursive schema + position map survive a close/re-open round-trip."""
    db_path = context.sibling_db("adaptive-recursive-reopen")
    engine = context.open_engine(db_path)
    try:
        _register_recursive_payload_schema(engine)
        _seed_recursive_knowledge_item(engine)

        rows_before = (
            engine.nodes(_ADAPTIVE_KI_KIND)
            .text_search("quarterly", 10)
            .with_match_attribution()
            .execute()
        )
        assert len(rows_before.hits) == 1, (
            f"pre-reopen expected 1 hit, got {len(rows_before.hits)}"
        )
        hit_before = rows_before.hits[0]
        assert hit_before.attribution is not None
        paths_before = hit_before.attribution.matched_paths
    finally:
        engine.close()

    engine = context.open_engine(db_path)
    try:
        rows_after = (
            engine.nodes(_ADAPTIVE_KI_KIND)
            .text_search("quarterly", 10)
            .with_match_attribution()
            .execute()
        )
        assert len(rows_after.hits) == 1, (
            f"post-reopen expected 1 hit, got {len(rows_after.hits)}"
        )
        hit_after = rows_after.hits[0]
        assert hit_after.node.logical_id == hit_before.node.logical_id, (
            "logical_id must match across reopen"
        )
        assert hit_after.source == hit_before.source, (
            f"source must match across reopen; before={hit_before.source}, "
            f"after={hit_after.source}"
        )
        assert hit_after.attribution is not None, (
            "attribution must still populate after reopen"
        )
        assert hit_after.attribution.matched_paths == paths_before, (
            f"matched_paths must match across reopen; before={paths_before!r}, "
            f"after={hit_after.attribution.matched_paths!r}"
        )
    finally:
        engine.close()
    return ScenarioResult(name="adaptive_search_recursive_rebuild_restore")
