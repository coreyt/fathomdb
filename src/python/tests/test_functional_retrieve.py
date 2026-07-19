"""X1 functional retrieve harness (Python SDK) — 0.8.0 Slice 30 / G2+G3.

Opens a REAL engine, writes canonical nodes (with `logical_id`s) + op-store rows,
then exercises the governed `read.*` namespace end-to-end across the FFI:

  * `read.get` / `read.get_many` return the written ACTIVE nodes by id;
    a superseded version is NOT returned; a missing id is `None`.
  * `read.collection` / `read.mutations` return the op-store rows with the
    cursor + mandatory limit honored.
  * `admin.configure` path is exercised (the append_only_log registration).

Shares ONE fixture (`functional_retrieve_fixture.json`) with the TypeScript
harness (`src/ts/tests/functional-retrieve.test.ts`); the cross-binding
equivalence is asserted against the same corpus + ids so both bindings are shown
to surface equivalent rows for the same DB. No mocking of the database.
"""

from __future__ import annotations

import json
from pathlib import Path

from fathomdb import Engine, admin, read

# 0.8.20 (R-20-E3): `source_id` is mandatory on every canonical write.
_SOURCE_ID = "py-test:functional-retrieve"

_FIXTURE = Path(__file__).resolve().parent / "functional_retrieve_fixture.json"


def _load_fixture() -> dict:
    return json.loads(_FIXTURE.read_text(encoding="utf-8"))


def _seed(engine: Engine, fixture: dict) -> None:
    # Supersede F1: write the old body first, then the new body (same logical_id)
    # so the active version is the second write — read.get must return only it.
    sup = fixture["superseded"]
    engine.write(
        [
            {
                "kind": sup["kind"],
                "body": sup["old_body"],
                "logical_id": sup["logical_id"],
                "source_id": _SOURCE_ID,
            }
        ]
    )
    for node in fixture["nodes"]:
        engine.write(
            [
                {
                    "kind": node["kind"],
                    "body": node["body"],
                    "logical_id": node["logical_id"],
                    "source_id": _SOURCE_ID,
                }
            ]
        )
    # Register the append_only_log collection (admin.configure is latest_state;
    # the op-store rows need an append_only_log collection registered via write).
    engine.write(
        [
            {
                "admin_schema": {
                    "name": fixture["collection"],
                    "kind": "append_only_log",
                    "schema_json": "{\"type\":\"object\"}",
                    "retention_json": "{}",
                }
            }
        ]
    )
    for row in fixture["op_rows"]:
        engine.write(
            [
                {
                    "op_store": {
                        "collection": fixture["collection"],
                        "record_key": row["record_key"],
                        "body": row["body"],
                    }
                }
            ]
        )


def test_read_get_returns_active_node_by_id(db_path: str) -> None:
    fixture = _load_fixture()
    engine = Engine.open(db_path)
    try:
        _seed(engine, fixture)

        got = read.get(engine, "F2")
        assert got is not None
        assert got.logical_id == "F2"
        assert got.kind == "fact"
        assert got.body == "water boils at 100C"
        assert got.write_cursor > 0

        # Superseded F1: only the active (new) body is returned.
        f1 = read.get(engine, "F1")
        assert f1 is not None
        assert f1.body == "the sky is blue"

        # Missing id → None (normal absence, not an error).
        assert read.get(engine, "DOES_NOT_EXIST") is None
    finally:
        engine.close()


def test_read_get_many_preserves_order_with_none(db_path: str) -> None:
    fixture = _load_fixture()
    engine = Engine.open(db_path)
    try:
        _seed(engine, fixture)
        rows = read.get_many(engine, ["N1", "MISSING", "F2"])
        assert len(rows) == 3
        assert rows[0] is not None and rows[0].body == "remember to hydrate"
        assert rows[1] is None
        assert rows[2] is not None and rows[2].body == "water boils at 100C"
    finally:
        engine.close()


def test_read_collection_and_mutations_honor_cursor_and_limit(db_path: str) -> None:
    fixture = _load_fixture()
    engine = Engine.open(db_path)
    try:
        _seed(engine, fixture)

        page1 = read.collection(engine, fixture["collection"], limit=3)
        assert [r.record_key for r in page1] == ["e0", "e1", "e2"]
        assert all(r.collection == fixture["collection"] for r in page1)
        assert all(r.op_kind == "append" for r in page1)

        cursor = page1[-1].id
        page2 = read.collection(engine, fixture["collection"], after_id=cursor, limit=3)
        assert [r.record_key for r in page2] == ["e3", "e4"]
        assert all(r.id > cursor for r in page2)

        # read.mutations is the alias over the same read-back.
        muts = read.mutations(engine, fixture["collection"], limit=100)
        assert [r.id for r in muts] == [r.id for r in (page1 + page2)]
    finally:
        engine.close()


def test_admin_configure_path_exercised(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        receipt = admin.configure(engine, name="latest_default", body="{}")
        assert isinstance(receipt.cursor, int)
    finally:
        engine.close()


def test_cross_binding_equivalence_values(db_path: str) -> None:
    """Python half of the cross-binding equivalence check. The TS harness asserts
    these SAME values (read.get bodies + the ordered op-store record_key list) for
    the SAME fixture, proving Py ≡ TS for each read verb on the same DB."""

    fixture = _load_fixture()
    engine = Engine.open(db_path)
    try:
        _seed(engine, fixture)

        # read.get bodies for the three logical ids (active-only).
        bodies: list[str] = []
        for lid in ["F1", "F2", "N1"]:
            record = read.get(engine, lid)
            assert record is not None, f"{lid} must be active and present"
            bodies.append(record.body)
        assert bodies == ["the sky is blue", "water boils at 100C", "remember to hydrate"]

        # op-store record_keys in id order.
        keys = [r.record_key for r in read.collection(engine, fixture["collection"], limit=1000)]
        assert keys == ["e0", "e1", "e2", "e3", "e4"]
    finally:
        engine.close()
