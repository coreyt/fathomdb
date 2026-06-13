"""Slice 20 (G5/G6) — graph traversal namespace.

Exposes bounded BFS and hybrid search-plus-expansion:

* ``graph.neighbors`` — G5 bounded BFS from a node, depth 1–3, direction-
  aware, cycle-guarded, hard-capped at 50 results. Edges with ``t_invalid``
  in the past are not traversed (valid-time filter).

* ``graph.search_expand`` — G6 composite: FTS/vector search (G1) + bounded
  BFS expansion from each hit. Nodes appearing in both the search hit set and
  the traversal reach appear only once in ``search_hits`` (deduplication:
  search score takes priority).

The native binding (``fathomdb._fathomdb``) performs all reads via the
ReaderWorkerPool with DEFERRED-tx snapshot isolation. Direction strings are
case-sensitive: ``"outgoing"``, ``"incoming"``, ``"both"``.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Literal, cast

from fathomdb._fathomdb import NodeRecord as _NativeNodeRecord
from fathomdb._fathomdb import graph_neighbors as _native_graph_neighbors
from fathomdb._fathomdb import search_expand as _native_search_expand
from fathomdb.types import ExpandedNode, NodeRecord, SearchExpandResult, SearchHit, SoftFallbackBranch

if TYPE_CHECKING:
    from fathomdb.engine import Engine

#: Valid values for the ``direction`` parameter.
TraversalDirection = Literal["outgoing", "incoming", "both"]


def _to_node_record(native: _NativeNodeRecord) -> NodeRecord:
    return NodeRecord(
        logical_id=native.logical_id,
        kind=native.kind,
        body=native.body,
        write_cursor=native.write_cursor,
    )


def neighbors(
    engine: "Engine",
    logical_id: str,
    depth: int,
    direction: TraversalDirection = "both",
) -> list[NodeRecord]:
    """G5 — bounded BFS from ``logical_id`` over ``canonical_edges``.

    Args:
        engine:     An open FathomDB engine.
        logical_id: The root node's stable identity string.
        depth:      Hop limit — **must be 1, 2, or 3**. Depth > 3 raises
                    ``InvalidArgumentError``.
        direction:  Edge direction to follow: ``"outgoing"`` (from→to),
                    ``"incoming"`` (to→from), or ``"both"``.

    Returns:
        List of reachable ``NodeRecord``s (root excluded), hard-capped at 50.
        Edges with ``t_invalid`` in the past are silently skipped (valid-time
        filter). Returns an empty list when the root has no reachable neighbors
        within the given depth.

    Raises:
        ``InvalidArgumentError``: depth > 3 or an unrecognised direction string.
        ``EngineError`` subclasses: storage or engine-level failures.
    """
    if not logical_id:
        raise ValueError("graph.neighbors requires a non-empty logical_id")
    from fathomdb.errors import InvalidArgumentError

    if not isinstance(depth, int) or isinstance(depth, bool) or depth < 0:
        raise InvalidArgumentError(
            f"graph.neighbors depth must be a non-negative integer; got {depth!r}"
        )
    native_nodes = _native_graph_neighbors(engine._native, logical_id, depth, direction)
    return [_to_node_record(n) for n in native_nodes]


def search_expand(
    engine: "Engine",
    query: str,
    depth: int,
    *,
    source_type: str | None = None,
    kind: str | None = None,
    created_after: int | None = None,
    status: str | None = None,
) -> SearchExpandResult:
    """G6 — FTS/vector search followed by bounded BFS expansion.

    Runs ``engine.search(query, ...)`` (G1), then expands each hit via
    ``graph.neighbors(hit_logical_id, depth, both)``.  Nodes that appear in
    both the search hit set and the traversal reach appear **only** in
    ``search_hits`` (deduplication: search score takes priority).

    Args:
        engine:       An open FathomDB engine.
        query:        Free-text or embedding query string (same as ``engine.search``).
        depth:        BFS hop limit for expansion — **must be 0–3**. Depth 0
                      skips expansion (returns search hits only). Depth > 3
                      raises ``InvalidArgumentError``.
        source_type:  Optional metadata filter passed to the search step.
        kind:         Optional kind filter passed to the search step.
        created_after: Optional lower-bound (unix seconds) for the ``created_at``
                       column, passed to the search step.
        status:       Optional status string filter passed to the search step.

    Returns:
        ``SearchExpandResult`` with three fields:
        - ``search_hits``: original RRF-scored results from the search step.
        - ``expanded``: nodes reachable from any hit within ``depth`` hops
          that are NOT already in ``search_hits``.
        - ``all_logical_ids``: deduplicated union of both sets.

    Raises:
        ``InvalidArgumentError``: depth > 3.
        ``EngineError`` subclasses: storage or engine-level failures.
    """
    if not query:
        raise ValueError("graph.search_expand requires a non-empty query")
    from fathomdb.errors import InvalidArgumentError

    if not isinstance(depth, int) or isinstance(depth, bool) or depth < 0:
        raise InvalidArgumentError(
            f"graph.search_expand depth must be a non-negative integer; got {depth!r}"
        )
    native_result = _native_search_expand(
        engine._native,
        query,
        depth,
        source_type,
        kind,
        created_after,
        status,
    )
    search_hits = [
        SearchHit(
            id=h.id,
            kind=h.kind,
            body=h.body,
            score=h.score,
            branch=cast(SoftFallbackBranch, h.branch),
        )
        for h in native_result.search_hits
    ]
    expanded = [
        ExpandedNode(
            node=_to_node_record(e.node),
            hop_count=e.hop_count,
        )
        for e in native_result.expanded
    ]
    return SearchExpandResult(
        search_hits=search_hits,
        expanded=expanded,
        all_logical_ids=list(native_result.all_logical_ids),
    )


__all__ = ["TraversalDirection", "neighbors", "search_expand"]
