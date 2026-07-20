"""0.8.11 Slice 40 (#17) — unified Filter grammar parity (Python SDK).

Mirrors `src/ts/tests/filter-unification.test.ts` (cross-binding X1 parity) and
the Rust `slice40_filter_unification.rs` engine suite. Exercises the unified
`fathomdb.Filter` over BOTH backends from a real engine (no mocking):

  * the five closed FilterTerm variants;
  * `SearchFilter` <-> `Filter` sugar round-trip (D4);
  * vec0 (`engine.search`) dispatch typed-rejects a `Json` term (D3);
  * `read.list(filter=...)` accepts the full set incl. the `Kind`/`SourceType`
    constant-folds (the engine performs the authoritative dispatch).

`read.list` stays the SAME governed verb (no new surface member); the unified
grammar rides an additive `filter=` keyword.
"""

from __future__ import annotations

import json

import pytest

from fathomdb import Engine, Filter, SearchFilter, read
from fathomdb.errors import InvalidFilterError
from fathomdb.filter import (

    CreatedAfter,
    Json,
    Kind,
    SourceType,
    Status,
    from_search_filter,
)

# 0.8.20 (R-20-E3): `source_id` is mandatory on every canonical write.
_SOURCE_ID = "py-test:filter-unification"



def _seed_todo_nodes(engine: Engine) -> None:
    rows = [
        {"logical_id": "A", "body": {"status": "open", "created_at": 100, "priority": 5}},
        {"logical_id": "B", "body": {"status": "done", "created_at": 200, "priority": 1}},
        {"logical_id": "C", "body": {"status": "open", "created_at": 300, "priority": 9}},
    ]
    for r in rows:
        engine.write([
            {
                "kind": "todo",
                "body": json.dumps(r["body"]),
                "logical_id": r["logical_id"],
                "source_id": _SOURCE_ID,
            }
        ])


# ----- D4 sugar round-trip --------------------------------------------------


def test_searchfilter_round_trips_through_unified_filter() -> None:
    sf = SearchFilter(source_type="todo", kind="todo", created_after=1000, status="open")
    unified = from_search_filter(sf)
    assert unified.terms == (
        SourceType("todo"),
        Kind("todo"),
        CreatedAfter(1000),
        Status("open"),
    )
    # lossless round-trip back to the shipped sugar
    assert unified.to_search_filter() == sf
    # all-None lowers to empty terms (unfiltered)
    assert from_search_filter(SearchFilter()).terms == ()


# ----- D3 typed rejection (vec0 / search backend) ---------------------------


def test_filter_to_search_filter_typed_rejects_json() -> None:
    f = Filter((Json({"type": "gt", "path": "$.priority", "value": 3}),))
    with pytest.raises(InvalidFilterError):
        f.to_search_filter()


def test_engine_search_typed_rejects_json_filter(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        f = Filter((Json({"type": "eq", "path": "$.status", "value": "open"}),))
        with pytest.raises(InvalidFilterError):
            engine.search("anything", filter=f)
    finally:
        engine.close()


# ----- D3 read.list(filter=...) full set + constant-folds -------------------


def test_read_list_filter_accepts_full_set(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        _seed_todo_nodes(engine)
        f = Filter((
            Status("open"),
            CreatedAfter(150),
            Json({"type": "gt", "path": "$.priority", "value": 3}),
        ))
        rows = read.list(engine, "todo", filter=f)
        ids = sorted(r.logical_id for r in rows)
        assert ids == ["C"], f"open AND created_at>=150 AND priority>3 => only C; got {ids}"
    finally:
        engine.close()


def test_read_list_filter_kind_constant_fold(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        _seed_todo_nodes(engine)
        # Kind matching the partition: no-op (all rows).
        all_rows = read.list(engine, "todo", filter=Filter((Kind("todo"),)))
        assert len(all_rows) == 3
        # Kind mismatching the partition: constant-folds to empty.
        none_rows = read.list(engine, "todo", filter=Filter((Kind("note"),)))
        assert none_rows == []
    finally:
        engine.close()


def test_read_list_filter_source_type_constant_fold(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        _seed_todo_nodes(engine)
        # resolve_source_type("todo") == "todo" => pass-all.
        match_rows = read.list(engine, "todo", filter=Filter((SourceType("todo"),)))
        assert len(match_rows) == 3
        # mismatching source_type constant-folds to empty.
        empty_rows = read.list(engine, "todo", filter=Filter((SourceType("email"),)))
        assert empty_rows == []
    finally:
        engine.close()


def test_read_list_rejects_both_predicates_and_filter(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        with pytest.raises(ValueError):
            read.list(
                engine,
                "todo",
                predicates=[{"type": "eq", "path": "$.status", "value": "open"}],
                filter=Filter((Status("open"),)),
            )
    finally:
        engine.close()


# ----- X1 cross-binding anchor ----------------------------------------------


def test_filter_term_closed_set_anchor() -> None:
    """The five FilterTerm variants exist and lower to the native term shape
    (mirror of the TS discriminated union)."""
    terms = Filter((
        SourceType("s"),
        Kind("k"),
        CreatedAfter(1),
        Status("st"),
        Json({"type": "eq", "path": "$.status", "value": "open"}),
    )).to_native_terms()
    assert [t["term"] for t in terms] == [
        "source_type",
        "kind",
        "created_after",
        "status",
        "json",
    ]
