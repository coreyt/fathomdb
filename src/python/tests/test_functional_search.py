"""X1 functional search harness (Python SDK) — 0.8.0 Slice 5 / G1.

Opens a REAL engine, writes a small corpus, `search()`es, and asserts the
structured `SearchHit` shape end-to-end across the FFI (id / kind / body /
score / branch present and correctly typed). Also asserts cross-binding
equivalence against `functional_search_fixture.json` — the SAME corpus +
queries the TypeScript harness (`src/ts/tests/functional-search.test.ts`)
runs, so both bindings are shown to surface equivalent hits for the same DB +
query.

This is the seed of the write -> search -> retrieve -> admin harness every
later slice extends. No mocking of the database.
"""

from __future__ import annotations

import json
import time
from pathlib import Path

from fathomdb import Engine, SearchFilter, SearchHit

_FIXTURE = Path(__file__).resolve().parent / "functional_search_fixture.json"


def _load_fixture() -> dict:
    return json.loads(_FIXTURE.read_text(encoding="utf-8"))


def _search_after_projection(engine: Engine, query: str) -> list[SearchHit]:
    """Search until projection has caught up and the FTS branch can match.

    Writes project asynchronously; poll briefly for a non-empty result.
    """

    deadline = time.monotonic() + 10.0
    last: list[SearchHit] = []
    while time.monotonic() < deadline:
        result = engine.search(query)
        last = list(result.results)
        if last:
            return last
        time.sleep(0.02)
    return last


def test_functional_search_hit_shape_across_ffi(db_path: str) -> None:
    fixture = _load_fixture()
    engine = Engine.open(db_path)
    try:
        for doc in fixture["corpus"]:
            engine.write([{"kind": doc["kind"], "body": doc["body"]}])
        engine.drain(timeout_s=30)

        hits = _search_after_projection(engine, "structured")
        assert hits, "expected at least one structured hit"
        for hit in hits:
            assert isinstance(hit, SearchHit)
            # id is the canonical write_cursor (interim identity carrier).
            assert isinstance(hit.id, int)
            assert hit.id > 0
            assert isinstance(hit.kind, str) and hit.kind
            assert isinstance(hit.body, str) and hit.body
            assert isinstance(hit.score, float)
            assert hit.branch in ("vector", "text")
    finally:
        engine.close()


def test_functional_search_cross_binding_equivalence(db_path: str) -> None:
    """Python half of the cross-binding equivalence check: the SAME corpus +
    queries the TypeScript harness uses must yield the SAME body set."""

    fixture = _load_fixture()
    engine = Engine.open(db_path)
    try:
        for doc in fixture["corpus"]:
            engine.write([{"kind": doc["kind"], "body": doc["body"]}])
        engine.drain(timeout_s=30)

        for case in fixture["queries"]:
            hits = _search_after_projection(engine, case["query"])
            got = sorted(hit.body for hit in hits)
            expected = sorted(case["expected_bodies"])
            assert got == expected, (
                f"query {case['query']!r}: Python binding returned {got}, "
                f"cross-binding fixture expects {expected}"
            )
            # Every hit from the FTS-only corpus carries the text branch tag.
            assert all(hit.branch == "text" for hit in hits)
    finally:
        engine.close()


# Slice 10 / X1 — RRF-fused order shared by both bindings. The text branch ranks
# by `write_cursor` (insertion order), so "retrieval" surfaces alpha (written
# first) before delta. Both the Python and TS harnesses assert this exact order,
# proving cross-binding RRF-ordering equivalence.
_RRF_ORDER_QUERY = "retrieval"
_RRF_EXPECTED_ORDER = [
    "alpha structured retrieval document",
    "delta retrieval and ranking notes",
]


def test_functional_rrf_fused_order_cross_binding(db_path: str) -> None:
    fixture = _load_fixture()
    engine = Engine.open(db_path)
    try:
        for doc in fixture["corpus"]:
            engine.write([{"kind": doc["kind"], "body": doc["body"]}])
        engine.drain(timeout_s=30)

        hits = _search_after_projection(engine, _RRF_ORDER_QUERY)
        assert [hit.body for hit in hits] == _RRF_EXPECTED_ORDER, (
            "RRF-fused order must match the TS binding (rank by write_cursor)"
        )
        # Fused score is sorted descending.
        scores = [hit.score for hit in hits]
        assert scores == sorted(scores, reverse=True)
    finally:
        engine.close()


def test_functional_filtered_search_prunes(db_path: str) -> None:
    """Slice 10 / X1 — a `SearchFilter` prunes results. "retrieval" matches a
    `note` (alpha) and a `doc` (delta); filtering `kind="note"` drops the doc."""

    fixture = _load_fixture()
    engine = Engine.open(db_path)
    try:
        for doc in fixture["corpus"]:
            engine.write([{"kind": doc["kind"], "body": doc["body"]}])
        engine.drain(timeout_s=30)

        unfiltered = _search_after_projection(engine, "retrieval")
        assert {hit.kind for hit in unfiltered} == {"note", "doc"}

        filtered = engine.search("retrieval", SearchFilter(kind="note"))
        assert [hit.body for hit in filtered.results] == ["alpha structured retrieval document"]
        assert all(hit.kind == "note" for hit in filtered.results)

        # A filter on the NULL-plumbed `status` prunes everything.
        empty = engine.search("retrieval", SearchFilter(status="open"))
        assert empty.results == []
    finally:
        engine.close()
