"""Phase 12.5c: parity tests for the ``embedder`` option on ``Engine.open``.

The SDK surface accepts three shapes: ``None`` (default), ``"none"``
(explicit opt-out), and ``"builtin"`` (the Phase 12.5b Candle-based
default embedder, which silently falls back to no-embedder when the
``default-embedder`` feature is off). Any other string must raise
``ValueError`` at open time.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from fathomdb import (
    ChunkInsert,
    ChunkPolicy,
    Engine,
    NodeInsert,
    WriteRequest,
    new_row_id,
)


def _seed_budget_goals(db: Engine) -> None:
    db.admin.register_fts_property_schema("Goal", ["$.name", "$.description"])
    db.write(
        WriteRequest(
            label="seed-budget",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="budget-alpha",
                    kind="Goal",
                    properties={
                        "name": "budget alpha goal",
                        "description": "quarterly budget rollup",
                    },
                    source_ref="seed",
                    upsert=False,
                    chunk_policy=ChunkPolicy.PRESERVE,
                ),
            ],
            chunks=[
                ChunkInsert(
                    id="budget-alpha-chunk",
                    node_logical_id="budget-alpha",
                    text_content="alpha budget quarterly docs review notes",
                ),
            ],
        )
    )


def test_default_embedder_is_none(tmp_path: Path) -> None:
    db = Engine.open(tmp_path / "t.db")
    _seed_budget_goals(db)
    rows = db.query("Goal").search("budget", 10).execute()
    assert rows.vector_hit_count == 0


def test_explicit_none_embedder(tmp_path: Path) -> None:
    db = Engine.open(tmp_path / "t.db", embedder="none")
    _seed_budget_goals(db)
    rows = db.query("Goal").search("budget", 10).execute()
    assert rows.vector_hit_count == 0


def test_builtin_embedder_accepted(tmp_path: Path) -> None:
    # Under the non-feature build (current default for the Python SDK),
    # "builtin" silently falls back to no-embedder. We assert only that
    # the open succeeds and that search() still produces vector-hit-count
    # zero; Phase 12.5b will flip this when the Candle default embedder
    # is wired in behind the default-embedder feature.
    db = Engine.open(tmp_path / "t.db", embedder="builtin")
    _seed_budget_goals(db)
    rows = db.query("Goal").search("budget", 10).execute()
    assert rows.vector_hit_count == 0


def test_invalid_embedder_value_raises(tmp_path: Path) -> None:
    with pytest.raises(ValueError) as excinfo:
        Engine.open(tmp_path / "t.db", embedder="bogus")
    message = str(excinfo.value)
    assert "none" in message and "builtin" in message
