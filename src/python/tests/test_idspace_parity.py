"""0.8.19 Slice 15 / C-2 typed ``SearchHit.id`` swap (TC-8, R-ID-2 / R-X-1) —
Python SDK X1 parity.

Mirrors the engine contract in
``src/rust/crates/fathomdb-engine/tests/tc8_idspace_swap.rs`` and the TypeScript
half ``src/ts/tests/idspace-parity.test.ts`` (same corpus, same assertions):
``SearchHit.id`` is the typed ``IdSpace`` (``{space, value}``), non-null and
id-space-total — a governed node (carries a ``logical_id``) surfaces the
``logical`` (``l:``) space; a doc-seeded node surfaces the ``content`` (``h:``)
space. The bare ``value`` round-trips (prefix + value reconstructs the pre-0.8.19
``stable_id`` string). The pre-C-2 int ``write_cursor`` id is engine-internal
and no longer surfaced.
"""

from __future__ import annotations

import time

from fathomdb import Engine, IdSpace, SearchHit


def _search_after_projection(engine: Engine, query: str) -> list[SearchHit]:
    deadline = time.monotonic() + 10.0
    last: list[SearchHit] = []
    while time.monotonic() < deadline:
        result = engine.search(query)
        last = list(result.results)
        if last:
            return last
        time.sleep(0.02)
    return last


def test_governed_hit_id_is_logical_space(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        engine.write(
            [{"kind": "person", "body": "idspace governed entity payload", "logical_id": "gov-py-1"}]
        )
        engine.drain(timeout_s=30)
        hits = _search_after_projection(engine, "governed")
        assert hits, "expected a governed hit"
        hit = hits[0]
        # Typed carrier (not a magic-prefixed string): a real IdSpace with the
        # lowercase discriminant + bare value.
        assert isinstance(hit.id, IdSpace)
        assert hit.id.space == "logical"
        assert hit.id.value == "gov-py-1"
        # Prefixed form is byte-identical to the pre-0.8.19 stable_id.
        assert f"l:{hit.id.value}" == "l:gov-py-1"
    finally:
        engine.close()


def test_doc_seeded_hit_id_is_content_space(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        engine.write([{"kind": "doc", "body": "idspace anonymous docseeded xyzzy"}])
        engine.drain(timeout_s=30)
        hits = _search_after_projection(engine, "docseeded")
        assert hits, "expected a doc-seeded hit"
        hit = hits[0]
        assert isinstance(hit.id, IdSpace)
        assert hit.id.space == "content"
        assert len(hit.id.value) == 64
        assert all(c in "0123456789abcdef" for c in hit.id.value)
    finally:
        engine.close()


def test_id_is_non_null_and_space_total(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        engine.write(
            [
                {"kind": "person", "body": "idspace total governed totalterm", "logical_id": "tot-1"},
                {"kind": "doc", "body": "idspace total anonymous totalterm"},
            ]
        )
        engine.drain(timeout_s=30)
        hits = _search_after_projection(engine, "totalterm")
        assert len(hits) >= 2
        for hit in hits:
            assert hit.id.space in ("logical", "content", "passage")
            assert hit.id.value  # non-null
        spaces = {hit.id.space for hit in hits}
        assert "logical" in spaces
        assert "content" in spaces
    finally:
        engine.close()
