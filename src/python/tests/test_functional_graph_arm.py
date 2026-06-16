"""Slice 30 (R3) — Python functional harness: use_graph_arm parameter.

Exercises:
  - use_graph_arm=False is the default; results are byte-identical to the
    two-arm pipeline.
  - use_graph_arm=True surfaces BFS-reachable nodes via temporal fact-edges.
  - use_graph_arm type validation: non-bool raises TypeError.
  - Temporal filter: edges with t_invalid in the past do not contribute.

All test databases are isolated per-test via the ``db_path`` fixture.
No embedder needed — FTS search only.
"""

from __future__ import annotations

import pytest

import fathomdb


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def open_engine(path: str) -> fathomdb.Engine:
    return fathomdb.Engine.open(path, use_default_embedder=False)


def _node(logical_id: str, body: str, kind: str = "doc") -> dict:
    return {"kind": kind, "body": body, "logical_id": logical_id}


def _edge(
    from_id: str,
    to_id: str,
    logical_id: str,
    *,
    t_invalid: str | None = None,
    body: str | None = None,
) -> dict:
    item: dict = {
        "kind": "link",
        "from": from_id,
        "to": to_id,
        "logical_id": logical_id,
    }
    if t_invalid is not None:
        item["t_invalid"] = t_invalid
    if body is not None:
        item["body"] = body
    return {"edge": item}


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


def test_graph_arm_default_is_false(db_path: str) -> None:
    """use_graph_arm defaults to False; search() and search(use_graph_arm=False)
    must produce identical projectionCursor and body order."""
    engine = open_engine(db_path)
    try:
        engine.write([
            _node("n1", "alpha bravo delta"),
            _node("n2", "charlie delta echo"),
        ])
        r_default = engine.search("delta")
        r_explicit = engine.search("delta", use_graph_arm=False)
        assert r_default.projection_cursor == r_explicit.projection_cursor
        assert [h.body for h in r_default.results] == [h.body for h in r_explicit.results]
    finally:
        engine.close()


def test_edge_body_enables_edge_fact_seeding(db_path: str) -> None:
    """Binding completeness: an edge written with a ``body`` through the Python
    write API must be projected into ``search_index_edges`` so the C1 graph arm can
    seed from edge-fact FTS (source A). The query matches ONLY the edge body (not
    the entity node bodies), so a graph-arm hit can only appear if the edge body
    reached the engine. Pre-fix (edge body dropped → NULL) this returned no graph
    hits for an edge-body query."""
    engine = open_engine(db_path)
    try:
        engine.write([
            _node("alice", "alice profile record"),
            _node("bob", "bob profile record"),
            # Edge body carries the distinctive query term; entity bodies do NOT.
            _edge("alice", "bob", "e-ab", body="quarterly acquisition agreement"),
        ])
        engine.drain(timeout_s=30)
        res = engine.search("acquisition agreement", use_graph_arm=True)
        graph_bodies = {h.body for h in res.results if h.branch == "graph_arm"}
        assert graph_bodies & {"alice profile record", "bob profile record"}, (
            "edge-fact (source A) seeding must surface the connected entities for an "
            f"edge-body query; got graph_arm bodies={graph_bodies}"
        )
    finally:
        engine.close()


def test_graph_arm_type_validation(db_path: str) -> None:
    """use_graph_arm must be a bool; non-bool raises TypeError."""
    engine = open_engine(db_path)
    try:
        engine.write([_node("n1", "test body")])
        with pytest.raises(TypeError, match="use_graph_arm"):
            engine.search("test", use_graph_arm=1)  # type: ignore[arg-type]
        with pytest.raises(TypeError, match="use_graph_arm"):
            engine.search("test", use_graph_arm="true")  # type: ignore[arg-type]
    finally:
        engine.close()


def test_graph_arm_enabled_does_not_crash(db_path: str) -> None:
    """use_graph_arm=True must run without error even with edges in the graph."""
    engine = open_engine(db_path)
    try:
        engine.write([
            _node("n1", "alice anchor search text"),
            _node("n2", "bob reachable via live edge"),
            _edge("n1", "n2", "e12"),
        ])
        result = engine.search("alice anchor", use_graph_arm=True)
        # At minimum, the search must not raise and must return some results.
        assert result is not None
        assert hasattr(result, "results")
    finally:
        engine.close()


def test_graph_arm_drops_expired_edge(db_path: str) -> None:
    """Edges with t_invalid in the past must NOT contribute graph arm candidates.

    Setup: n1 (searchable) --expired-edge--> n2 (not searchable).
    With use_graph_arm=True, n2 must NOT appear in results.
    """
    engine = open_engine(db_path)
    try:
        engine.write([
            _node("n1", "sentinel query anchor"),
            _node("n2", "unreachable via expired edge zz99"),
            _edge("n1", "n2", "e12", t_invalid="2000-01-01T00:00:00Z"),
        ])
        result = engine.search("sentinel query", use_graph_arm=True)
        bodies = [h.body for h in result.results]
        assert not any("unreachable via expired edge" in b for b in bodies), (
            f"graph arm must NOT surface n2 via an expired edge; got: {bodies}"
        )
    finally:
        engine.close()
