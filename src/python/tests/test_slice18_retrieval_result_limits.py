"""Slice 18 — ranked retrieval limits through the public Python SDK."""

from __future__ import annotations

import json

import pytest

from fathomdb import Engine, ProjectionRole, ProjectionSpec
from fathomdb.errors import InvalidArgumentError
from fathomdb import graph


def _write_corpus(engine: Engine) -> None:
    engine.configure_projections(
        [
            ProjectionSpec(
                name="title",
                roles=frozenset({ProjectionRole.SEARCHABLE}),
                fts=True,
            )
        ]
    )
    engine.write(
        [
            {
                "kind": "doc",
                "logical_id": f"N{n}",
                "source_id": "py-slice18:fixture",
                "body": json.dumps({"title": f"needle result {n}"}),
            }
            for n in range(101)
        ]
    )
    engine.drain(timeout_s=10)


def test_ranked_search_limits_are_public_and_truthful(db_path: str) -> None:
    engine = Engine.open(db_path, use_default_embedder=False)
    try:
        _write_corpus(engine)
        assert len(engine.search("needle").results) == 10
        assert len(engine.search_text_only("needle").results) == 10
        assert len(engine.search_projected_text("needle", "title").results) == 10
        assert len(graph.search_expand(engine, "needle", 0).search_hits) == 10

        for limit in (5, 20, 50, 100):
            assert len(engine.search("needle", limit=limit).results) == limit
            assert len(engine.search_text_only("needle", limit=limit).results) == limit
            assert (
                len(engine.search_projected_text("needle", "title", limit=limit).results)
                == limit
            )
            assert (
                len(graph.search_expand(engine, "needle", 0, search_limit=limit).search_hits)
                == limit
            )
    finally:
        engine.close()


@pytest.mark.parametrize("limit", [0, -1, 101])
def test_ranked_search_limit_rejections_are_typed(db_path: str, limit: int) -> None:
    engine = Engine.open(db_path, use_default_embedder=False)
    try:
        with pytest.raises(InvalidArgumentError):
            engine.search("needle", limit=limit)
        with pytest.raises(InvalidArgumentError):
            engine.search_text_only("needle", limit=limit)
        with pytest.raises(InvalidArgumentError):
            engine.search_projected_text("needle", "title", limit=limit)
        with pytest.raises(InvalidArgumentError):
            graph.search_expand(engine, "needle", 0, search_limit=limit)
    finally:
        engine.close()
