"""Slice 20 (G5/G6) — Python functional harness: graph_neighbors + search_expand.

These tests run against the real compiled native binding (PyO3 / fathomdb-engine).
They use FTS search exclusively (no embedder needed) so that the binding's
``graph.neighbors`` and ``graph.search_expand`` verbs can be exercised synchronously
in CI without a model download.

All test databases are isolated per-test via the ``db_path`` fixture.
"""

from __future__ import annotations

from typing import Any, cast

import pytest

import fathomdb
from fathomdb import ExpandedNode, NodeRecord, SearchExpandResult

# 0.8.20 (R-20-E3): `source_id` is mandatory on every canonical write.
_SOURCE_ID = "py-test:functional-graph"


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def open_engine(path: str) -> fathomdb.Engine:
    return fathomdb.Engine.open(path, use_default_embedder=False)


def _node(logical_id: str, body: str, kind: str = "doc") -> dict:
    return {"kind": kind, "body": body, "logical_id": logical_id, "source_id": _SOURCE_ID}


def _edge(from_id: str, to_id: str, logical_id: str) -> dict:
    """Edge write item — must be wrapped under the ``"edge"`` key."""
    return {
        "edge": {
            "kind": "link",
            "from": from_id,
            "to": to_id,
            "logical_id": logical_id,
            "source_id": _SOURCE_ID,
        }
    }


def _seed_small_graph(engine: fathomdb.Engine) -> None:
    """A→B→C, A→D (D is a direct leaf; C is two hops from A)."""
    engine.write(
        [
            _node("A", "Root node alpha unique"),
            _node("B", "Neighbor node B beta"),
            _node("C", "Hop-2 node C gamma"),
            _node("D", "Direct leaf D delta"),
            _edge("A", "B", "E-AB"),
            _edge("B", "C", "E-BC"),
            _edge("A", "D", "E-AD"),
        ]
    )


# ---------------------------------------------------------------------------
# G5 — graph.neighbors
# ---------------------------------------------------------------------------


def test_graph_neighbors_depth1_outgoing(db_path: str) -> None:
    """Depth=1 outgoing from A returns B and D (direct children)."""
    engine = open_engine(db_path)
    _seed_small_graph(engine)

    results = fathomdb.graph.neighbors(engine, "A", depth=1, direction="outgoing")
    logical_ids = sorted(n.logical_id for n in results)
    assert logical_ids == ["B", "D"], f"expected [B, D], got {logical_ids}"
    # Root A must NOT appear.
    assert all(n.logical_id != "A" for n in results)

    engine.close()


def test_graph_neighbors_depth2_outgoing(db_path: str) -> None:
    """Depth=2 outgoing from A returns B, C, D."""
    engine = open_engine(db_path)
    _seed_small_graph(engine)

    results = fathomdb.graph.neighbors(engine, "A", depth=2, direction="outgoing")
    logical_ids = sorted(n.logical_id for n in results)
    assert "B" in logical_ids
    assert "C" in logical_ids
    assert "D" in logical_ids
    assert "A" not in logical_ids

    engine.close()


def test_graph_neighbors_returns_node_records(db_path: str) -> None:
    """Each result is a proper NodeRecord with all required fields."""
    engine = open_engine(db_path)
    _seed_small_graph(engine)

    results = fathomdb.graph.neighbors(engine, "A", depth=1, direction="outgoing")
    assert len(results) > 0, "expected at least one neighbor"
    for node in results:
        assert isinstance(node, NodeRecord)
        assert isinstance(node.logical_id, str)
        assert isinstance(node.kind, str)
        assert isinstance(node.body, str)
        assert isinstance(node.write_cursor, int)

    engine.close()


def test_graph_neighbors_depth_gt3_raises_invalid_argument(db_path: str) -> None:
    """Depth > 3 must raise InvalidArgumentError, not silently truncate."""
    engine = open_engine(db_path)
    engine.write([_node("ROOT", "root body")])

    from fathomdb.errors import InvalidArgumentError

    with pytest.raises(InvalidArgumentError):
        fathomdb.graph.neighbors(engine, "ROOT", depth=4, direction="outgoing")

    engine.close()


def test_graph_neighbors_empty_result_for_isolated_node(db_path: str) -> None:
    """A node with no edges returns an empty neighbor list."""
    engine = open_engine(db_path)
    engine.write([_node("SOLO", "isolated node")])

    results = fathomdb.graph.neighbors(engine, "SOLO", depth=1, direction="outgoing")
    assert results == [], f"expected [], got {results}"

    engine.close()


def test_graph_neighbors_unknown_direction_raises(db_path: str) -> None:
    """An unrecognised direction string raises an InvalidArgumentError."""
    engine = open_engine(db_path)
    engine.write([_node("ROOT", "root body")])

    from fathomdb.errors import InvalidArgumentError

    with pytest.raises(InvalidArgumentError):
        # cast to Any to suppress pyright's Literal type check on an intentionally
        # invalid direction string (we're testing the runtime validation path).
        fathomdb.graph.neighbors(engine, "ROOT", depth=1, direction=cast(Any, "sideways"))

    engine.close()


# ---------------------------------------------------------------------------
# G6 — graph.search_expand
# ---------------------------------------------------------------------------


def test_search_expand_returns_search_expand_result(db_path: str) -> None:
    """search_expand returns a SearchExpandResult with the expected fields."""
    engine = open_engine(db_path)
    engine.write(
        [
            _node("HIT", "fzunique expand quark harness node alpha"),
            _node("NBR", "neighbor node beta"),
            _edge("HIT", "NBR", "E1"),
        ]
    )

    result = fathomdb.graph.search_expand(engine, "fzunique expand quark", depth=1)

    assert isinstance(result, SearchExpandResult)
    assert isinstance(result.search_hits, list)
    assert isinstance(result.expanded, list)
    assert isinstance(result.all_logical_ids, list)

    engine.close()


def test_search_expand_expanded_contains_neighbor(db_path: str) -> None:
    """A node reachable by one hop from the search hit appears in expanded."""
    engine = open_engine(db_path)
    engine.write(
        [
            _node("HIT2", "fzunique expand quark harness node alpha 2"),
            _node("NBR2", "neighbor node gamma"),
            _edge("HIT2", "NBR2", "E2"),
        ]
    )

    result = fathomdb.graph.search_expand(engine, "fzunique expand quark harness", depth=1)

    expanded_ids = [e.node.logical_id for e in result.expanded]
    assert "NBR2" in expanded_ids, (
        f"neighbor NBR2 must appear in expanded; expanded={expanded_ids}, "
        f"hits={[h.id for h in result.search_hits]}"
    )

    engine.close()


def test_search_expand_deduplication(db_path: str) -> None:
    """A node that is both a search hit and a traversal neighbor appears only in hits."""
    engine = open_engine(db_path)
    engine.write(
        [
            _node("DA", "dedup shimmer unique probe node alpha zeta"),
            _node("DB", "dedup shimmer unique probe node beta zeta"),
            _edge("DA", "DB", "EAB"),
        ]
    )

    result = fathomdb.graph.search_expand(engine, "dedup shimmer unique probe", depth=1)

    expanded_ids = [e.node.logical_id for e in result.expanded]
    # DB may be a search hit too (both A and B match); if so it must NOT be in expanded.
    assert "DB" not in expanded_ids, (
        f"DB is a search hit and must not appear in expanded; "
        f"expanded={expanded_ids}, hits={[h.id for h in result.search_hits]}"
    )

    engine.close()


def test_search_expand_expanded_node_is_node_record(db_path: str) -> None:
    """Each item in expanded is an ExpandedNode with a NodeRecord and hop_count."""
    engine = open_engine(db_path)
    engine.write(
        [
            _node("HIT3", "fzunique expand quark harness node alpha 3"),
            _node("CHD3", "child node delta"),
            _edge("HIT3", "CHD3", "E3"),
        ]
    )

    result = fathomdb.graph.search_expand(engine, "fzunique expand quark harness", depth=1)

    for item in result.expanded:
        assert isinstance(item, ExpandedNode)
        assert isinstance(item.node, NodeRecord)
        assert isinstance(item.hop_count, int)
        assert item.hop_count >= 1

    engine.close()
