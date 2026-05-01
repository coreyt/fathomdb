"""Phase 13b: parity tests for the unified Python ``SearchBuilder``.

These mirror the five Phase 13a Rust roundtrip tests in
``crates/fathomdb/tests/python_search_ffi.rs`` for ``PySearchMode::Search``.
Each test asserts equivalent semantics through the Python SDK surface so
the unified ``search`` FFI mode is exercised end to end from the tethered
``Query.search`` builder.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from fathomdb import (
    BuilderValidationError,
    ChunkInsert,
    ChunkPolicy,
    Engine,
    FtsPropertyPathMode,
    FtsPropertyPathSpec,
    NodeInsert,
    SearchBuilder,
    SearchMatchMode,
    SearchRows,
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
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="budget-bravo",
                    kind="Goal",
                    properties={
                        "name": "budget bravo goal",
                        "description": "annual budget summary",
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
                ChunkInsert(
                    id="budget-bravo-chunk",
                    node_logical_id="budget-bravo",
                    text_content="bravo budget annual summary notes",
                ),
            ],
        )
    )


def _seed_budget_task(db: Engine) -> None:
    db.admin.register_fts_property_schema("Task", ["$.name", "$.description"])
    db.write(
        WriteRequest(
            label="seed-budget-task",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="budget-task",
                    kind="Task",
                    properties={
                        "name": "budget task",
                        "description": "reconcile quarterly budget figures",
                    },
                    source_ref="seed",
                    upsert=False,
                    chunk_policy=ChunkPolicy.PRESERVE,
                ),
            ],
            chunks=[
                ChunkInsert(
                    id="budget-task-chunk",
                    node_logical_id="budget-task",
                    text_content="task budget reconciliation notes",
                ),
            ],
        )
    )


def _seed_recursive_note(db: Engine, logical_id: str, body: str) -> None:
    db.admin.register_fts_property_schema_with_entries(
        "Note",
        [FtsPropertyPathSpec(path="$.payload", mode=FtsPropertyPathMode.RECURSIVE)],
        separator=" ",
        exclude_paths=[],
    )
    db.write(
        WriteRequest(
            label="seed-note",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id=logical_id,
                    kind="Note",
                    properties={"payload": {"body": body}},
                    source_ref="seed",
                    upsert=False,
                    chunk_policy=ChunkPolicy.PRESERVE,
                ),
            ],
        )
    )


# ---------------------------------------------------------------------------
# Mirrors `search_basic_populates_search_rows` in python_search_ffi.rs.
# ---------------------------------------------------------------------------
def test_search_basic_populates_search_rows(tmp_path: Path) -> None:
    db = Engine.open(tmp_path / "t.db")
    _seed_budget_goals(db)

    builder = db.query("Goal").search("budget", 10)
    assert isinstance(builder, SearchBuilder)
    rows = builder.execute()
    assert isinstance(rows, SearchRows)
    assert len(rows.hits) >= 1
    assert rows.strict_hit_count == len(rows.hits)
    assert rows.relaxed_hit_count == 0
    assert rows.vector_hit_count == 0
    assert rows.fallback_used is False
    assert rows.was_degraded is False

    hit = rows.hits[0]
    assert hit.score > 0
    assert hit.match_mode == SearchMatchMode.STRICT
    assert hit.node.kind == "Goal"
    assert hit.attribution is None


# ---------------------------------------------------------------------------
# Mirrors `search_with_filter_kind_eq_is_fused`.
# ---------------------------------------------------------------------------
def test_search_with_filter_kind_eq_is_fused(tmp_path: Path) -> None:
    db = Engine.open(tmp_path / "t.db")
    _seed_budget_goals(db)
    _seed_budget_task(db)

    # Control: no kind filter, root_kind="" — both Goals and Task match.
    control = db.query("").search("budget", 10).execute()
    assert any(h.node.kind == "Task" for h in control.hits), (
        "control (no kind filter) must include Task hits"
    )

    filtered = db.query("").search("budget", 10).filter_kind_eq("Goal").execute()
    assert len(filtered.hits) >= 1
    assert all(h.node.kind == "Goal" for h in filtered.hits)
    assert len(filtered.hits) < len(control.hits)


# ---------------------------------------------------------------------------
# Mirrors `search_with_filter_json_text_eq_post_filter`.
# ---------------------------------------------------------------------------
def test_search_with_filter_json_text_eq_post_filter(tmp_path: Path) -> None:
    db = Engine.open(tmp_path / "t.db")
    _seed_budget_goals(db)

    rows = (
        db.query("Goal")
        .search("budget", 10)
        .filter_json_text_eq("$.name", "budget alpha goal")
        .execute()
    )
    assert len(rows.hits) >= 1
    assert all(h.node.logical_id == "budget-alpha" for h in rows.hits), (
        f"json filter must restrict to budget-alpha, got "
        f"{[h.node.logical_id for h in rows.hits]!r}"
    )


# ---------------------------------------------------------------------------
# Mirrors `search_with_attribution_on_recursive_schema`.
# ---------------------------------------------------------------------------
def test_search_with_attribution_on_recursive_schema(tmp_path: Path) -> None:
    db = Engine.open(tmp_path / "t.db")
    _seed_recursive_note(db, "note-search-attrib", "shipping quarterly docs")

    rows = db.query("Note").search("shipping", 10).with_match_attribution().execute()
    assert len(rows.hits) >= 1
    hit = rows.hits[0]
    assert hit.attribution is not None, "attribution populated when requested"
    assert "$.payload.body" in hit.attribution.matched_paths, (
        f"expected $.payload.body in matched_paths; got {hit.attribution.matched_paths!r}"
    )


# ---------------------------------------------------------------------------
# Mirrors `search_empty_query_returns_empty_search_rows`.
# ---------------------------------------------------------------------------
def test_search_empty_query_returns_empty_search_rows(tmp_path: Path) -> None:
    db = Engine.open(tmp_path / "t.db")
    _seed_budget_goals(db)

    rows = db.query("Goal").search("", 10).execute()
    assert isinstance(rows, SearchRows)
    assert rows.hits == ()
    assert rows.strict_hit_count == 0
    assert rows.relaxed_hit_count == 0
    assert rows.vector_hit_count == 0
    assert rows.fallback_used is False


# ---------------------------------------------------------------------------
# Item 7: filter_json_fused_* surface. These mirror the Rust search.rs
# in-module tests for the BuilderValidationError contract.
# ---------------------------------------------------------------------------
def test_filter_json_fused_text_eq_requires_registered_schema(tmp_path: Path) -> None:
    db = Engine.open(tmp_path / "t.db")
    with pytest.raises(BuilderValidationError):
        (
            db.query("Note")
            .search("anything", 5)
            .filter_json_fused_text_eq("$.title", "hello")
        )


def test_filter_json_fused_text_eq_rejects_path_not_in_schema(tmp_path: Path) -> None:
    db = Engine.open(tmp_path / "t.db")
    db.admin.register_fts_property_schema("Note", ["$.title"])
    with pytest.raises(BuilderValidationError):
        (
            db.query("Note")
            .search("anything", 5)
            .filter_json_fused_text_eq("$.not_indexed", "hello")
        )


def test_filter_json_fused_text_eq_succeeds_with_registered_schema(
    tmp_path: Path,
) -> None:
    db = Engine.open(tmp_path / "t.db")
    db.admin.register_fts_property_schema("Note", ["$.title"])
    # Should not raise — we exercise the builder chain only (no execute
    # since no seed data).
    builder = (
        db.query("Note")
        .search("anything", 5)
        .filter_json_fused_text_eq("$.title", "hello")
    )
    assert isinstance(builder, SearchBuilder)


def test_filter_json_fused_timestamp_gt_validates(tmp_path: Path) -> None:
    db = Engine.open(tmp_path / "t.db")
    db.admin.register_fts_property_schema("Note", ["$.written_at"])
    builder = (
        db.query("Note")
        .text_search("x", 5)
        .filter_json_fused_timestamp_gt("$.written_at", 1_700_000_000)
    )
    # The builder returns a TextSearchBuilder — just assert it composes.
    assert builder is not None


def test_filter_json_fused_text_eq_regression_post_filter_unchanged(
    tmp_path: Path,
) -> None:
    # Regression guard: the existing non-fused filter_json_text_eq
    # still composes without requiring a registered schema.
    db = Engine.open(tmp_path / "t.db")
    builder = (
        db.query("Note").search("anything", 5).filter_json_text_eq("$.status", "active")
    )
    assert builder is not None


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
