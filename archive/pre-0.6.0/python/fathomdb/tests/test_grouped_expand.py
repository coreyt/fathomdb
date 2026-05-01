"""Pack 5: Python binding for SearchBuilder.expand + execute_grouped.

Tests that SearchBuilder gains .expand() and .execute_grouped() to mirror
the Query class grouped-expand path.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from fathomdb import (
    ChunkInsert,
    ChunkPolicy,
    EdgeInsert,
    Engine,
    NodeInsert,
    SearchBuilder,
    WriteRequest,
    new_row_id,
)


# ---------------------------------------------------------------------------
# Test 1: SearchBuilder.expand returns a SearchBuilder
# ---------------------------------------------------------------------------
def test_search_builder_has_expand_method(tmp_path: Path) -> None:
    db = Engine.open(tmp_path / "t.db")
    db.admin.register_fts_property_schema("WMGoal", ["$.name"])
    result = (
        db.query("WMGoal")
        .search("budget goal", 10)
        .expand(slot="items", direction="out", label="HAS_ITEM", max_depth=1)
    )
    assert isinstance(result, SearchBuilder)


# ---------------------------------------------------------------------------
# Test 2: Full E2E — search + expand + execute_grouped
# ---------------------------------------------------------------------------
def test_search_builder_expand_execute_grouped_e2e(tmp_path: Path) -> None:
    db = Engine.open(tmp_path / "t.db")
    db.admin.register_fts_property_schema("Goal", ["$.name", "$.description"])

    goal_id = new_row_id()
    item1_id = new_row_id()
    item2_id = new_row_id()

    db.write(
        WriteRequest(
            label="seed-e2e",
            nodes=[
                NodeInsert(
                    row_id=goal_id,
                    logical_id="goal-budget",
                    kind="Goal",
                    properties={
                        "name": "budget goal",
                        "description": "quarterly budget",
                    },
                    source_ref="seed",
                    upsert=False,
                    chunk_policy=ChunkPolicy.PRESERVE,
                ),
                NodeInsert(
                    row_id=item1_id,
                    logical_id="item-1",
                    kind="Item",
                    properties={"name": "item one"},
                    source_ref="seed",
                    upsert=False,
                    chunk_policy=ChunkPolicy.PRESERVE,
                ),
                NodeInsert(
                    row_id=item2_id,
                    logical_id="item-2",
                    kind="Item",
                    properties={"name": "item two"},
                    source_ref="seed",
                    upsert=False,
                    chunk_policy=ChunkPolicy.PRESERVE,
                ),
            ],
            edges=[
                EdgeInsert(
                    row_id=new_row_id(),
                    logical_id="edge-goal-item1",
                    source_logical_id="goal-budget",
                    target_logical_id="item-1",
                    kind="HAS_ITEM",
                    properties={},
                    source_ref="seed",
                    upsert=False,
                ),
                EdgeInsert(
                    row_id=new_row_id(),
                    logical_id="edge-goal-item2",
                    source_logical_id="goal-budget",
                    target_logical_id="item-2",
                    kind="HAS_ITEM",
                    properties={},
                    source_ref="seed",
                    upsert=False,
                ),
            ],
            chunks=[
                ChunkInsert(
                    id="chunk-goal-budget",
                    node_logical_id="goal-budget",
                    text_content="budget goal quarterly planning",
                )
            ],
        )
    )

    result = (
        db.query("Goal")
        .search("budget goal", 10)
        .expand(slot="items", direction="out", label="HAS_ITEM", max_depth=1)
        .execute_grouped()
    )

    assert len(result.roots) == 1
    assert len(result.expansions) == 1
    assert result.expansions[0].slot == "items"


# ---------------------------------------------------------------------------
# Test 3: Per-originator limit via .limit()
# ---------------------------------------------------------------------------
def test_search_builder_skewed_fanout_per_originator_limit(tmp_path: Path) -> None:
    db = Engine.open(tmp_path / "t.db")
    db.admin.register_fts_property_schema("Origin", ["$.name"])

    nodes = []
    edges = []

    for i in range(3):
        origin_lid = f"origin-{i}"
        nodes.append(
            NodeInsert(
                row_id=new_row_id(),
                logical_id=origin_lid,
                kind="Origin",
                properties={"name": f"origin node {i}"},
                source_ref="seed",
                upsert=False,
                chunk_policy=ChunkPolicy.PRESERVE,
            )
        )
        for j in range(10):
            child_lid = f"child-{i}-{j}"
            nodes.append(
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id=child_lid,
                    kind="Child",
                    properties={"name": f"child {j} of origin {i}"},
                    source_ref="seed",
                    upsert=False,
                    chunk_policy=ChunkPolicy.PRESERVE,
                )
            )
            edges.append(
                EdgeInsert(
                    row_id=new_row_id(),
                    logical_id=f"edge-{i}-{j}",
                    source_logical_id=origin_lid,
                    target_logical_id=child_lid,
                    kind="HAS_CHILD",
                    properties={},
                    source_ref="seed",
                    upsert=False,
                )
            )

    db.write(
        WriteRequest(
            label="seed-fanout",
            nodes=nodes,
            edges=edges,
        )
    )

    result = (
        db.query("Origin")
        .search("origin", 10)
        .expand(slot="children", direction="out", label="HAS_CHILD", max_depth=1)
        .limit(3)
        .execute_grouped()
    )

    assert len(result.expansions) == 1
    for expansion_root in result.expansions[0].roots:
        assert len(expansion_root.nodes) <= 3


# ---------------------------------------------------------------------------
# Test 4: expand with JsonPathEq filter on target nodes
# ---------------------------------------------------------------------------
def test_search_builder_expand_with_json_path_eq_filter(tmp_path: Path) -> None:
    db = Engine.open(tmp_path / "t.db")
    db.admin.register_fts_property_schema("Root", ["$.name"])

    nodes = [
        NodeInsert(
            row_id=new_row_id(),
            logical_id="root-1",
            kind="Root",
            properties={"name": "root node search"},
            source_ref="seed",
            upsert=False,
            chunk_policy=ChunkPolicy.PRESERVE,
        )
    ]
    edges = []

    for i in range(5):
        lid = f"decision-{i}"
        nodes.append(
            NodeInsert(
                row_id=new_row_id(),
                logical_id=lid,
                kind="Item",
                properties={"name": f"decision item {i}", "kind": "decision"},
                source_ref="seed",
                upsert=False,
                chunk_policy=ChunkPolicy.PRESERVE,
            )
        )
        edges.append(
            EdgeInsert(
                row_id=new_row_id(),
                logical_id=f"edge-decision-{i}",
                source_logical_id="root-1",
                target_logical_id=lid,
                kind="HAS_ITEM",
                properties={},
                source_ref="seed",
                upsert=False,
            )
        )

    for i in range(5):
        lid = f"action-{i}"
        nodes.append(
            NodeInsert(
                row_id=new_row_id(),
                logical_id=lid,
                kind="Item",
                properties={"name": f"action item {i}", "kind": "action"},
                source_ref="seed",
                upsert=False,
                chunk_policy=ChunkPolicy.PRESERVE,
            )
        )
        edges.append(
            EdgeInsert(
                row_id=new_row_id(),
                logical_id=f"edge-action-{i}",
                source_logical_id="root-1",
                target_logical_id=lid,
                kind="HAS_ITEM",
                properties={},
                source_ref="seed",
                upsert=False,
            )
        )

    db.write(
        WriteRequest(
            label="seed-filter",
            nodes=nodes,
            edges=edges,
        )
    )

    # filter format uses the same dict shape as PyQueryStep filter variants:
    # {"type": "filter_json_text_eq", "path": "$.kind", "value": "decision"}
    result = (
        db.query("Root")
        .search("root node", 10)
        .expand(
            slot="decisions",
            direction="out",
            label="HAS_ITEM",
            max_depth=1,
            filter={"type": "filter_json_text_eq", "path": "$.kind", "value": "decision"},
        )
        .execute_grouped()
    )

    assert len(result.expansions) == 1
    assert result.expansions[0].slot == "decisions"
    assert len(result.expansions[0].roots) == 1, "one root group (root-1)"
    decision_nodes = result.expansions[0].roots[0].nodes
    assert len(decision_nodes) == 5, "filter must return exactly 5 decision nodes, not all 10"
    for node in decision_nodes:
        props = node.properties  # already a decoded dict
        assert props.get("kind") == "decision", f"filter leaked non-decision node: {props}"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
