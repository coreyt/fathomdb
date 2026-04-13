from __future__ import annotations

from pathlib import Path

import pytest

from fathomdb import (
    ChunkInsert,
    ChunkPolicy,
    Engine,
    FallbackSearchBuilder,
    FtsPropertyPathMode,
    FtsPropertyPathSpec,
    HitAttribution,
    NodeInsert,
    QueryRows,
    RetrievalModality,
    SearchHit,
    SearchHitSource,
    SearchMatchMode,
    SearchRows,
    TextSearchBuilder,
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


def test_text_search_returns_search_rows_with_populated_fields(tmp_path: Path) -> None:
    db = Engine.open(tmp_path / "t.db")
    _seed_budget_goals(db)

    builder = db.query("Goal").text_search("quarterly", 10)
    assert isinstance(builder, TextSearchBuilder)
    rows = builder.execute()
    assert isinstance(rows, SearchRows)
    assert len(rows.hits) >= 1
    hit = rows.hits[0]
    assert isinstance(hit, SearchHit)
    assert hit.score > 0
    assert isinstance(hit.source, SearchHitSource)
    assert hit.match_mode == SearchMatchMode.STRICT
    assert hit.snippet is not None
    assert hit.written_at > 0
    assert hit.projection_row_id is not None
    assert hit.attribution is None
    assert hit.node.kind == "Goal"
    assert hit.node.logical_id.startswith("budget-")


def test_text_search_zero_hits_returns_empty_search_rows(tmp_path: Path) -> None:
    db = Engine.open(tmp_path / "t.db")
    _seed_budget_goals(db)

    rows = db.query("Goal").text_search("zzznopeterm", 10).execute()
    assert isinstance(rows, SearchRows)
    assert rows.hits == ()
    assert rows.strict_hit_count == 0
    assert rows.fallback_used is False
    assert rows.was_degraded is False


def test_text_search_with_filter_kind_eq_chains(tmp_path: Path) -> None:
    db = Engine.open(tmp_path / "t.db")
    _seed_budget_goals(db)

    rows = (
        db.query("Goal")
        .text_search("budget", 10)
        .filter_kind_eq("Goal")
        .execute()
    )
    assert len(rows.hits) >= 1
    for hit in rows.hits:
        assert hit.node.kind == "Goal"


def test_text_search_with_match_attribution_populates_leaves(tmp_path: Path) -> None:
    db = Engine.open(tmp_path / "t.db")

    # Register a schema where $.payload is recursive so the engine walks
    # every scalar leaf and produces per-leaf position-map rows. This is
    # the only code path that populates HitAttribution.matched_paths with
    # non-empty entries.
    db.admin.register_fts_property_schema_with_entries(
        "KnowledgeItem",
        [
            FtsPropertyPathSpec(path="$.title", mode=FtsPropertyPathMode.SCALAR),
            FtsPropertyPathSpec(path="$.payload", mode=FtsPropertyPathMode.RECURSIVE),
        ],
        separator=" ",
        exclude_paths=[],
    )
    db.write(
        WriteRequest(
            label="seed-knowledge",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="ki-alpha",
                    kind="KnowledgeItem",
                    properties={
                        "title": "alpha doc",
                        "payload": {
                            "body": "quarterly rollup summary",
                            "notes": ["review pending"],
                        },
                    },
                    source_ref="seed",
                    upsert=False,
                    chunk_policy=ChunkPolicy.PRESERVE,
                ),
            ],
        )
    )

    rows = (
        db.query("KnowledgeItem")
        .text_search("quarterly", 10)
        .with_match_attribution()
        .execute()
    )
    assert len(rows.hits) >= 1
    attributed = [h for h in rows.hits if h.attribution is not None]
    assert attributed, "at least one hit should have attribution populated"
    att = attributed[0].attribution
    assert isinstance(att, HitAttribution)
    assert isinstance(att.matched_paths, tuple)
    assert len(att.matched_paths) >= 1, (
        f"recursive schema must populate matched_paths; got {att.matched_paths!r}"
    )
    assert any(p.startswith("$.payload.") for p in att.matched_paths), (
        f"expected at least one $.payload.* match path; got {att.matched_paths!r}"
    )

    # Baseline: without with_match_attribution(), attribution is always None.
    plain = db.query("KnowledgeItem").text_search("quarterly", 10).execute()
    assert plain.hits, "baseline query should still return hits"
    assert all(h.attribution is None for h in plain.hits)


def test_text_search_strict_miss_triggers_relaxed_fallback(tmp_path: Path) -> None:
    db = Engine.open(tmp_path / "t.db")
    _seed_budget_goals(db)

    rows = (
        db.query("Goal")
        .text_search("budget quarterly zzznopeterm", 10)
        .execute()
    )
    assert rows.fallback_used is True
    assert len(rows.hits) >= 1
    assert any(h.match_mode == SearchMatchMode.RELAXED for h in rows.hits)


def test_fallback_search_two_shape(tmp_path: Path) -> None:
    db = Engine.open(tmp_path / "t.db")
    _seed_budget_goals(db)

    builder = db.fallback_search("zzznope1 zzznope2", "budget OR nothing", 10)
    assert isinstance(builder, FallbackSearchBuilder)
    rows = builder.filter_kind_eq("Goal").execute()
    assert rows.fallback_used is True
    assert len(rows.hits) >= 1


def test_fallback_search_strict_only_matches_text_search(tmp_path: Path) -> None:
    db = Engine.open(tmp_path / "t.db")
    _seed_budget_goals(db)

    a = db.query("Goal").text_search("budget", 10).filter_kind_eq("Goal").execute()
    b = (
        db.fallback_search("budget", None, 10)
        .filter_kind_eq("Goal")
        .execute()
    )
    assert a.hits == b.hits
    assert a.strict_hit_count == b.strict_hit_count
    assert a.relaxed_hit_count == b.relaxed_hit_count
    assert a.fallback_used == b.fallback_used
    assert a.was_degraded == b.was_degraded


def test_node_query_execute_still_returns_query_rows(tmp_path: Path) -> None:
    db = Engine.open(tmp_path / "t.db")
    _seed_budget_goals(db)

    rows = db.query("Goal").execute()
    assert isinstance(rows, QueryRows)
    assert not isinstance(rows, SearchRows)


def test_text_search_empty_query_returns_empty_search_rows(tmp_path: Path) -> None:
    db = Engine.open(tmp_path / "t.db")
    _seed_budget_goals(db)

    rows = db.query("Goal").text_search("", 10).execute()
    assert isinstance(rows, SearchRows)
    assert rows.hits == ()


def test_text_search_whitespace_query_returns_empty_search_rows(tmp_path: Path) -> None:
    db = Engine.open(tmp_path / "t.db")
    _seed_budget_goals(db)

    rows = db.query("Goal").text_search("   ", 10).execute()
    assert isinstance(rows, SearchRows)
    assert rows.hits == ()


def test_text_search_builder_rejects_relaxed_query_kwarg(tmp_path: Path) -> None:
    """P7b-1: ``TextSearchBuilder.__init__`` must not silently discard a
    ``relaxed_query`` kwarg. It is a fallback-only parameter; passing it to
    the adaptive builder is a programming error."""
    db = Engine.open(tmp_path / "t.db")
    _seed_budget_goals(db)

    with pytest.raises(TypeError):
        TextSearchBuilder(
            core=db._core,  # type: ignore[attr-defined]
            root_kind="Goal",
            strict_query="budget",
            limit=10,
            relaxed_query="budget OR meeting",  # type: ignore[call-arg]
        )


def test_text_search_hits_carry_text_modality_and_no_vector_distance(
    tmp_path: Path,
) -> None:
    """Phase 10 sanity: every text-path hit is tagged TEXT and has no
    vector_distance populated."""
    db = Engine.open(tmp_path / "t.db")
    _seed_budget_goals(db)

    rows = db.query("Goal").text_search("budget", 10).execute()
    assert len(rows.hits) >= 1
    for hit in rows.hits:
        assert hit.modality == RetrievalModality.TEXT
        assert hit.vector_distance is None
        assert hit.match_mode is not None


def test_search_rows_vector_hit_count_is_zero_in_phase_10(tmp_path: Path) -> None:
    """Phase 10 introduces no vector execution path, so vector_hit_count
    is always zero."""
    db = Engine.open(tmp_path / "t.db")
    _seed_budget_goals(db)

    rows = db.query("Goal").text_search("budget", 10).execute()
    assert rows.vector_hit_count == 0


def _open_recursive_payload_engine(tmp_path: Path, db_name: str) -> Engine:
    db = Engine.open(tmp_path / db_name)
    db.admin.register_fts_property_schema_with_entries(
        "Item",
        [FtsPropertyPathSpec(path="$.payload", mode=FtsPropertyPathMode.RECURSIVE)],
        separator=" ",
        exclude_paths=[],
    )
    return db


def _write_recursive_item(db: Engine, logical_id: str, payload: dict) -> None:
    db.write(
        WriteRequest(
            label=f"seed-{logical_id}",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id=logical_id,
                    kind="Item",
                    properties={"payload": payload},
                    source_ref="seed",
                    upsert=False,
                    chunk_policy=ChunkPolicy.PRESERVE,
                ),
            ],
        )
    )


def test_recursive_property_fts_empty_then_nonempty_in_array(tmp_path: Path) -> None:
    db = _open_recursive_payload_engine(tmp_path, "rfts1.db")
    _write_recursive_item(db, "item-1", {"xs": ["", "x"]})
    rows = db.query("Item").text_search("x", 10).execute()
    assert any(h.node.logical_id == "item-1" for h in rows.hits)


def test_recursive_property_fts_two_empties_then_nonempty_in_array(
    tmp_path: Path,
) -> None:
    db = _open_recursive_payload_engine(tmp_path, "rfts2.db")
    _write_recursive_item(db, "item-2", {"xs": ["", "", "x"]})
    rows = db.query("Item").text_search("x", 10).execute()
    assert any(h.node.logical_id == "item-2" for h in rows.hits)


def test_recursive_property_fts_empty_then_nonempty_sibling_keys(
    tmp_path: Path,
) -> None:
    db = _open_recursive_payload_engine(tmp_path, "rfts3.db")
    _write_recursive_item(db, "item-3", {"a": "", "b": "x"})
    rows = db.query("Item").text_search("x", 10).execute()
    assert any(h.node.logical_id == "item-3" for h in rows.hits)


def test_recursive_property_fts_nested_empty_then_nonempty_sibling_keys(
    tmp_path: Path,
) -> None:
    db = _open_recursive_payload_engine(tmp_path, "rfts4.db")
    _write_recursive_item(db, "item-4", {"inner": {"a": "", "b": "x"}})
    rows = db.query("Item").text_search("x", 10).execute()
    assert any(h.node.logical_id == "item-4" for h in rows.hits)


def test_recursive_property_fts_descent_past_empty_sibling_into_nested_subtree(
    tmp_path: Path,
) -> None:
    db = _open_recursive_payload_engine(tmp_path, "rfts5.db")
    _write_recursive_item(db, "item-5", {"a": "", "b": {"c": "x"}})
    rows = db.query("Item").text_search("x", 10).execute()
    assert any(h.node.logical_id == "item-5" for h in rows.hits)


def test_recursive_property_fts_all_empty_payload_writes_succeed(
    tmp_path: Path,
) -> None:
    cases = [
        ("e0", {}),
        ("e1", {"a": ""}),
        ("e2", {"xs": []}),
        ("e3", {"xs": [""]}),
        ("e4", {"xs": ["", ""]}),
        ("e5", {"xs": ["", "", ""]}),
    ]
    for idx, (logical_id, payload) in enumerate(cases):
        db = _open_recursive_payload_engine(tmp_path, f"rfts_empty_{idx}.db")
        _write_recursive_item(db, logical_id, payload)
        rows = db.query("Item").text_search("x", 10).execute()
        assert rows.hits == (), (
            f"all-empty payload {payload!r} unexpectedly matched 'x'"
        )


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
