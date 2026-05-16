"""AC-060b — payload-vs-schema validation surfaces as
`SchemaValidationError` and writes no row.

A registered op-store schema is the contract the engine validates
against on each op-store write; a body that violates the schema must
fail BEFORE the row commits.
"""

from __future__ import annotations

import pytest

from fathomdb import Engine, admin
from fathomdb.errors import SchemaValidationError


def test_op_store_write_violating_schema_raises_and_writes_no_row(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        schema_body = '{"type":"object","required":["foo"]}'
        admin.configure(engine, name="strict_col", body=schema_body)
        before = engine._native.counters().write_rows

        with pytest.raises(SchemaValidationError):
            engine.write(
                [
                    {
                        "op_store": {
                            "collection": "strict_col",
                            "record_key": "k1",
                            "schema_id": "strict_col",
                            "body": "{}",
                        }
                    }
                ]
            )
        after = engine._native.counters().write_rows
        assert after == before, "schema-violating write must not commit a row"
    finally:
        engine.close()
