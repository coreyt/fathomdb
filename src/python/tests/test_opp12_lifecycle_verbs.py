"""X1 SDK parity — OPP-12 record-lifecycle Phase-1 lifecycle verbs (0.8.19 Slice 10).

Opens a REAL engine (tmpdir SQLite, no mocking) and exercises the
``transition``/``purge`` lifecycle verbs through the Python binding:

  * R-TR-1 — each legal ``transition`` move succeeds; each illegal move raises a
    typed ``IllegalTransitionError`` carrying ``from_state``/``to_state``/``legal``
    (parity-safe field names — S7).
  * gap-6 — promote/undelete CLEAR ``reason`` (the node re-appears in reads);
    reject/soft-delete SET it and the node leaves default reads.
  * §3 — a lifecycle verb on a non-``l:`` (``h:``/``p:``) id raises
    ``NotLifecycleAddressableError`` carrying ``id_space``.
  * R-PG-1/2 — ``purge`` requires deleted-first, is idempotent, and erases the
    node from every read path.

Cross-binding equivalence anchor: ``src/ts/tests/opp12-lifecycle-verbs.test.ts``
asserts the SAME behavior for the same inputs (Py ≡ TS, R-X-1).
"""

from __future__ import annotations

import pytest

from fathomdb import Engine, read
from fathomdb.errors import (
    IllegalTransitionError,
    InvalidArgumentError,
    NotLifecycleAddressableError,
)


def test_legal_transitions_and_reason_semantics(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        # promote: pending → active clears reason; node becomes visible.
        engine.write([
            {"kind": "doc", "body": "quarantined", "logical_id": "p1",
             "state": "pending", "reason": "awaiting-review"}
        ])
        assert read.get(engine, "p1") is None
        engine.transition("p1", "active")
        assert read.get(engine, "p1") is not None

        # soft-delete: active → deleted sets reason; node leaves default reads.
        engine.write([{"kind": "doc", "body": "live", "logical_id": "a1"}])
        engine.transition("a1", "deleted", "user-deleted")
        assert read.get(engine, "a1") is None
        # undelete: deleted → active restores visibility.
        engine.transition("a1", "active")
        assert read.get(engine, "a1") is not None
    finally:
        engine.close()


def test_illegal_transition_is_typed_with_fields(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        engine.write([{"kind": "doc", "body": "x", "logical_id": "a1"}])
        with pytest.raises(IllegalTransitionError) as exc:
            engine.transition("a1", "purged")
        err = exc.value
        assert err.from_state == "active"
        assert err.to_state == "purged"
        # `deleted` is the legal path out of `active` (delete-then-purge).
        assert err.legal == ["deleted"]

        # A self-loop is also illegal.
        with pytest.raises(IllegalTransitionError):
            engine.transition("a1", "active")

        # An unknown lifecycle string is a boundary argument error (never a
        # silent success).
        with pytest.raises(InvalidArgumentError):
            engine.transition("a1", "bogus")
    finally:
        engine.close()


@pytest.mark.parametrize("bad_id", ["h:deadbeef", "p:7"])
def test_non_logical_ids_refused(db_path: str, bad_id: str) -> None:
    engine = Engine.open(db_path)
    try:
        with pytest.raises(NotLifecycleAddressableError) as t_exc:
            engine.transition(bad_id, "deleted")
        assert t_exc.value.id_space in {"content", "passage"}
        with pytest.raises(NotLifecycleAddressableError):
            engine.purge(bad_id)
    finally:
        engine.close()


def test_purge_requires_deleted_first_and_is_idempotent(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        engine.write([{"kind": "doc", "body": "x", "logical_id": "a1"}])
        # active (not deleted) → precondition failure.
        with pytest.raises(IllegalTransitionError) as exc:
            engine.purge("a1")
        assert exc.value.from_state == "active"
        assert exc.value.to_state == "purged"

        engine.transition("a1", "deleted")
        engine.purge("a1")
        # Fully erased: unretrievable in every read path.
        assert read.get(engine, "a1") is None
        # Idempotent: a second purge (now absent) is a no-op success.
        engine.purge("a1")
        engine.purge("never-existed")
    finally:
        engine.close()
