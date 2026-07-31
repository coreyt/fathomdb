"""TC-138 — graph search expansion preserves the public search-hit contract.

This is a pure-Python wrapper test.  It isolates the conversion at the SDK
boundary so that a native hit carrying the current binding fields cannot leak
its native identity object or drop provenance / reranking metadata.
"""

from __future__ import annotations

import sys
import types
from types import SimpleNamespace
from typing import Any, cast


if "fathomdb" not in sys.modules and "fathomdb._fathomdb" not in sys.modules:
    _fake = types.ModuleType("fathomdb._fathomdb")

    class _Dummy:
        pass

    def _fake_getattr(name: str) -> object:
        return _Dummy

    _fake.__getattr__ = _fake_getattr  # type: ignore[attr-defined]
    sys.modules["fathomdb._fathomdb"] = _fake

from fathomdb import graph  # noqa: E402
from fathomdb.types import IdSpace  # noqa: E402


def test_search_expand_maps_idspace_and_search_metadata(monkeypatch) -> None:  # type: ignore[no-untyped-def]
    """TC-138: graph hits match `Engine.search`'s public `SearchHit` shape."""
    native_hit = SimpleNamespace(
        id=SimpleNamespace(space="logical", value="graph-hit"),
        kind="doc",
        body="graph contract body",
        score=0.42,
        branch="text",
        source_id="source-graph-contract",
        ce_score=0.73,
    )
    native_result = SimpleNamespace(search_hits=[native_hit], expanded=[], all_logical_ids=["graph-hit"])
    monkeypatch.setattr(graph, "_native_search_expand", lambda *args: native_result)

    result = graph.search_expand(
        cast(Any, SimpleNamespace(_native=object())), "graph contract", depth=1
    )

    hit = result.search_hits[0]
    assert isinstance(hit.id, IdSpace)
    assert hit.id == IdSpace(space="logical", value="graph-hit")
    assert hit.source_id == "source-graph-contract"
    assert hit.ce_score == 0.73
