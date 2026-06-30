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
            # G0 Phase-2 (CONCERN-4) — `source_id` is present across the FFI and
            # is None for every two-arm hit (only graph-arm hits carry it).
            assert hasattr(hit, "source_id")
            assert hit.source_id is None
            # Cause-A (0.8.11.2) — `stable_id` crosses the FFI. Doc-seeded corpus
            # nodes carry NULL logical_id, so the stable id is the `"h:"`
            # content-hash of the body (never None for a real node hit).
            assert hasattr(hit, "stable_id")
            assert isinstance(hit.stable_id, str) and hit.stable_id.startswith("h:")
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


def test_functional_row_cursors_one_to_one(db_path: str) -> None:
    """Slice 15 / X1 / G0 — `WriteReceipt.row_cursors` is 1:1 with the batch in
    input order and deterministic on a fresh DB. The exact values ([1, 2, 3]
    then [4, 5]) are what the TypeScript harness also asserts, proving Py ≡ TS
    `row_cursors` for the same DB + writes (cross-binding equivalence)."""

    engine = Engine.open(db_path)
    try:
        first = engine.write(
            [
                {"kind": "doc", "body": "rc-a"},
                {"kind": "doc", "body": "rc-b"},
                {"kind": "doc", "body": "rc-c"},
            ]
        )
        assert list(first.row_cursors) == [1, 2, 3]
        assert first.cursor == 3
        assert first.row_cursors[-1] == first.cursor

        second = engine.write([{"kind": "doc", "body": "rc-d"}, {"kind": "doc", "body": "rc-e"}])
        assert list(second.row_cursors) == [4, 5]
        assert second.cursor == 5
    finally:
        engine.close()


def test_functional_supersession_write_surfaces_row_cursor(db_path: str) -> None:
    """Slice 15 / X1 / G0 — a supersession write (same `logical_id`) is accepted
    by the SDK and returns its per-row cursor. Active-only read visibility
    (filtering the superseded version out of `search`) is reserved for G2 +
    shadow reconciliation (Slice 16); here we assert the write path round-trips
    `logical_id` + `row_cursors`. The TS harness asserts the same values."""

    engine = Engine.open(db_path)
    try:
        v1 = engine.write([{"kind": "doc", "body": "fact v1", "logical_id": "L1"}])
        v2 = engine.write([{"kind": "doc", "body": "fact v2", "logical_id": "L1"}])
        assert list(v1.row_cursors) == [1]
        assert list(v2.row_cursors) == [2]
        assert v2.cursor > v1.cursor
    finally:
        engine.close()


def test_functional_dangling_edge_count_across_ffi(db_path: str) -> None:
    """Slice 20 / X1 / G8 — a write whose batch contains a dangling edge returns
    `dangling_edge_endpoints` > 0; a clean batch returns 0. The TS harness
    (`functional-dangling-edges.test.ts`) asserts the SAME values for the SAME
    batches, proving Py ≡ TS for the dangling count (real engine, no mocks)."""

    engine = Engine.open(db_path)
    try:
        # Clean batch: the edge's endpoints resolve to live `logical_id` nodes
        # inserted later in the SAME batch (cross-row) -> 0 dangling.
        clean = engine.write(
            [
                {"kind": "doc", "body": "n1", "logical_id": "N1"},
                {"kind": "doc", "body": "n2", "logical_id": "N2"},
                {"edge": {"kind": "rel", "from": "N1", "to": "N2"}},
            ]
        )
        assert clean.dangling_edge_endpoints == 0

        # Dangling batch: both endpoints reference missing `logical_id`s -> 2
        # (flag-and-count: the write still succeeds).
        dangling = engine.write([{"edge": {"kind": "rel", "from": "GHOST_A", "to": "GHOST_B"}}])
        assert dangling.dangling_edge_endpoints == 2
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
