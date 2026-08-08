"""Slice 23 — direct text-only result-prefix stability through Python."""

from __future__ import annotations

from fathomdb import Engine
from fathomdb.types import ReadView, SearchResult


QUERY = "s23prefix"


def _writes() -> list[dict]:
    nodes = [
        {
            "kind": "doc",
            "body": f"{QUERY} matching document {rank:02}",
            "logical_id": f"N{rank:02}",
            "source_id": "py-test:slice23",
        }
        for rank in range(50)
    ]
    return nodes + [
        {
            "edge": {
                "kind": "link",
                "from": "N00",
                "to": "N01",
                "logical_id": "E10",
                "source_id": "py-test:slice23",
                # Fusion deduplicates matching node and edge candidates by body.
                "body": f"{QUERY} matching document 10",
            }
        }
    ]


def _assert_ordered_prefix(small: SearchResult, large: SearchResult) -> None:
    assert len(small.results) == 10
    assert len(large.results) == 50
    actual = [(hit.id, hit.score) for hit in small.results]
    expected = [(hit.id, hit.score) for hit in large.results[: len(small.results)]]
    assert actual == expected


def test_direct_text_limits_are_prefix_stable_through_python(db_path: str) -> None:
    engine = Engine.open(db_path, use_default_embedder=False)
    try:
        engine.write(_writes())
        _assert_ordered_prefix(
            engine.search_text_only(QUERY, limit=10),
            engine.search_text_only(QUERY, limit=50),
        )
    finally:
        engine.close()


def test_direct_text_view_limits_are_prefix_stable_through_python(db_path: str) -> None:
    engine = Engine.open(db_path, use_default_embedder=False)
    try:
        engine.write(_writes())
        view = ReadView(valid_as_of=1_700_000_000)
        _assert_ordered_prefix(
            engine.search_text_only(QUERY, view=view, limit=10),
            engine.search_text_only(QUERY, view=view, limit=50),
        )
    finally:
        engine.close()
