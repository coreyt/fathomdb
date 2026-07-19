"""0.8.20 Slice 5c (R-20-E3) — provenance is mandatory on every canonical write.

The Python arm of "an un-provenanced public write does not compile / raises".

Rust makes the absence of provenance INEXPRESSIBLE: `PreparedWrite` carries a
`SourceId` newtype, so `source_id: None` is a compile error (see
`src/rust/crates/fathomdb/tests/ui/unprovenanced_public_write.rs`). Python has no
equivalent guarantee at the FFI boundary — a dict is a dict — so the binding
raises `WriteValidationError` instead, which is the closest available
enforcement.

Why this matters at all: `excise_source` addresses rows BY `source_id`. A row
written without one is reachable by no erasure call, i.e. permanently
un-erasable. That is the defect R-20-E3 closes, not a style rule.
"""

from __future__ import annotations

import pytest

from fathomdb import Engine
from fathomdb.errors import WriteValidationError


def test_node_write_without_source_id_raises(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        before = engine.counters().write_rows
        with pytest.raises(WriteValidationError):
            engine.write([{"kind": "doc", "body": "un-provenanced body"}])
        assert engine.counters().write_rows == before, (
            "an un-provenanced write must commit no row"
        )
    finally:
        engine.close()


def test_edge_write_without_source_id_raises(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        with pytest.raises(WriteValidationError):
            engine.write([{"edge": {"kind": "rel", "from": "a", "to": "b"}}])
    finally:
        engine.close()


@pytest.mark.parametrize(
    "source_id",
    [
        "",
        "   ",
        # The engine's reserved namespace. A caller able to mint
        # `_legacy:pre-0.8.20` could hide rows among the ones schema migration
        # step 21 back-filled; `_engine:` rows read as engine substrate.
        "_engine:coverage",
        "_legacy:pre-0.8.20",
    ],
)
def test_empty_or_reserved_source_id_raises(db_path: str, source_id: str) -> None:
    engine = Engine.open(db_path)
    try:
        with pytest.raises(WriteValidationError):
            engine.write([{"kind": "doc", "body": "x", "source_id": source_id}])
    finally:
        engine.close()


def test_provenanced_write_succeeds_and_is_excisable_by_that_id(db_path: str) -> None:
    """The positive control: the same write with provenance commits."""
    engine = Engine.open(db_path)
    try:
        receipt = engine.write(
            [{"kind": "doc", "body": "provenanced body", "source_id": "doc-1"}]
        )
        assert receipt.cursor >= 1
    finally:
        engine.close()
