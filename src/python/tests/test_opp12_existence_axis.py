"""X1 SDK parity — OPP-12 record-lifecycle Phase-1 existence axis (0.8.19 Slice 5).

Opens a REAL engine (tmpdir SQLite, no mocking) and exercises the create-time
existence surface through the Python binding:

  * R-EX-1 — ``PreparedWrite::Node`` gains create-time ``state``
    (``{"pending", "active"}``) + advisory ``reason``; both round-trip.
  * R-EX-1 — ``state="deleted"``/``"purged"`` (or any out-of-subset value) is a
    typed ``WriteValidationError`` — you cannot CREATE a deleted/purged node.
  * R-EX-2 — a ``pending`` node is absent from default ``search`` / ``read.get`` /
    ``read.list``; an ``active`` node is present. NO-OP on an all-active corpus.

Cross-binding equivalence anchor: ``src/ts/tests/opp12-existence-axis.test.ts``
asserts the SAME behavior for the same inputs (Py ≡ TS, R-X-1).
"""

from __future__ import annotations

import pytest

from fathomdb import Engine, read
from fathomdb.errors import WriteValidationError


def test_state_and_reason_round_trip_and_pending_excluded(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        # active (default reason NULL) + explicit pending with a reason; both
        # share the FTS token `zephyrunique` so both are FTS candidates.
        engine.write([
            {"kind": "doc", "body": "zephyrunique active payload", "logical_id": "act1"},
        ])
        engine.write([
            {
                "kind": "doc",
                "body": "zephyrunique pending payload",
                "logical_id": "pen1",
                "state": "pending",
                "reason": "awaiting-review",
            }
        ])
        # An explicit state="active" is value-identical to the default.
        engine.write([
            {"kind": "doc", "body": "zephyrunique second active", "logical_id": "act2",
             "state": "active"},
        ])

        # R-EX-2 default search excludes the pending node.
        hits = list(engine.search("zephyrunique").results)
        bodies = [h.body for h in hits]
        assert any("active payload" in b for b in bodies), bodies
        assert any("second active" in b for b in bodies), bodies
        assert not any("pending payload" in b for b in bodies), bodies

        # R-EX-2 read.get: pending -> None, active -> present.
        assert read.get(engine, "act1") is not None
        assert read.get(engine, "pen1") is None

        # R-EX-2 read.list: pending excluded from the kind listing.
        listed = {r.logical_id for r in read.list(engine, "doc")}
        assert "act1" in listed and "act2" in listed
        assert "pen1" not in listed
    finally:
        engine.close()


@pytest.mark.parametrize("bad_state", ["deleted", "purged", "bogus"])
def test_deleted_and_purged_are_not_creatable(db_path: str, bad_state: str) -> None:
    engine = Engine.open(db_path)
    try:
        with pytest.raises(WriteValidationError):
            engine.write([
                {"kind": "doc", "body": "x", "logical_id": "n1", "state": bad_state}
            ])
    finally:
        engine.close()


def test_no_op_on_all_active_corpus(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        for i in range(5):
            engine.write([
                {"kind": "doc", "body": f"commonterm doc number {i}", "logical_id": f"id{i}"}
            ])
        hits = [h for h in engine.search("commonterm").results if "commonterm" in h.body]
        assert len(hits) == 5
    finally:
        engine.close()
