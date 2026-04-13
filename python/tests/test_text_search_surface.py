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


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
