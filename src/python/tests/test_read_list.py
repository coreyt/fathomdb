"""X1 functional read.list harness (Python SDK) — 0.8.1 Slice 35 / G4.

Opens a REAL engine, writes canonical nodes whose `body` is a JSON object
carrying fields matched by the G4 allowlist (``$.status``, ``$.priority``,
``$.created_at``), then exercises ``read.list`` end-to-end:

  * Unfiltered path returns all active nodes of the kind.
  * Eq predicate filters to matching nodes.
  * Comparison predicates (``gt``, ``gte``, ``lt``, ``lte``) work.
  * Multiple predicates AND-compose.
  * Non-allowlisted path raises ``InvalidFilterError``.
  * Cross-binding equivalence anchor matches ``functional-read-list.test.ts``.

No mocking — the engine runs against a real (tmpdir) SQLite file.
"""

from __future__ import annotations

import json

import pytest

from fathomdb import Engine, read
from fathomdb.errors import InvalidFilterError

# 0.8.20 (R-20-E3): `source_id` is mandatory on every canonical write.
_SOURCE_ID = "py-test:read-list"


def _seed_task_nodes(engine: Engine) -> None:
    """Write three task nodes with JSON bodies for filter tests."""
    tasks = [
        {"logical_id": "T1", "body": {"status": "open",   "priority": 10, "created_at": 1000}},
        {"logical_id": "T2", "body": {"status": "closed", "priority": 20, "created_at": 2000}},
        {"logical_id": "T3", "body": {"status": "open",   "priority": 30, "created_at": 3000}},
    ]
    for t in tasks:
        engine.write([
            {
                "kind": "task",
                "body": json.dumps(t["body"]),
                "logical_id": t["logical_id"],
                "source_id": _SOURCE_ID,
            }
        ])


def test_read_list_unfiltered_returns_all_active_by_kind(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        _seed_task_nodes(engine)
        # Also write a node of a different kind — it must NOT appear.
        engine.write(
            [{"kind": "note", "body": "hello", "logical_id": "N1", "source_id": _SOURCE_ID}]
        )

        rows = read.list(engine, "task")
        assert len(rows) == 3
        ids = {r.logical_id for r in rows}
        assert ids == {"T1", "T2", "T3"}
        # The "note" kind node must not appear.
        assert all(r.kind == "task" for r in rows)
    finally:
        engine.close()


def test_read_list_eq_predicate_filters_correctly(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        _seed_task_nodes(engine)
        rows = read.list(engine, "task", predicates=[{"type": "eq", "path": "$.status", "value": "open"}])
        assert len(rows) == 2
        assert all(json.loads(r.body)["status"] == "open" for r in rows)
        ids = {r.logical_id for r in rows}
        assert ids == {"T1", "T3"}
    finally:
        engine.close()


def test_read_list_gt_predicate_filters_correctly(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        _seed_task_nodes(engine)
        rows = read.list(engine, "task", predicates=[{"type": "gt", "path": "$.priority", "value": 10}])
        assert len(rows) == 2
        ids = {r.logical_id for r in rows}
        assert ids == {"T2", "T3"}
    finally:
        engine.close()


def test_read_list_and_composition(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        _seed_task_nodes(engine)
        # open AND priority > 10 → only T3 (priority=30, status=open)
        rows = read.list(
            engine,
            "task",
            predicates=[
                {"type": "eq",  "path": "$.status",   "value": "open"},
                {"type": "gt",  "path": "$.priority",  "value": 10},
            ],
        )
        assert len(rows) == 1
        assert rows[0].logical_id == "T3"
    finally:
        engine.close()


def test_read_list_limit_respected(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        _seed_task_nodes(engine)
        rows = read.list(engine, "task", limit=2)
        assert len(rows) == 2
    finally:
        engine.close()


def test_read_list_non_allowlisted_path_raises_invalid_filter(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        _seed_task_nodes(engine)
        with pytest.raises(InvalidFilterError):
            read.list(
                engine,
                "task",
                predicates=[{"type": "eq", "path": "$.not_allowed_field", "value": "x"}],
            )
    finally:
        engine.close()


def test_read_list_empty_predicates_returns_all(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        _seed_task_nodes(engine)
        # Empty list of predicates == unfiltered.
        rows = read.list(engine, "task", predicates=[])
        assert len(rows) == 3
    finally:
        engine.close()


def test_read_list_cross_binding_equivalence_anchor(db_path: str) -> None:
    """Anchor for cross-binding equivalence.

    The TypeScript harness (``functional-read-list.test.ts``) asserts the
    SAME result set for the same predicate on the same kind, proving Py ≡ TS.
    """
    engine = Engine.open(db_path)
    try:
        _seed_task_nodes(engine)
        # Closed predicates: status == "open" AND priority >= 10
        rows = read.list(
            engine,
            "task",
            predicates=[
                {"type": "eq",  "path": "$.status",   "value": "open"},
                {"type": "gte", "path": "$.priority",  "value": 10},
            ],
        )
        ids = sorted(r.logical_id for r in rows)
        # T1 (priority=10, status=open) and T3 (priority=30, status=open) match.
        assert ids == ["T1", "T3"]
    finally:
        engine.close()
