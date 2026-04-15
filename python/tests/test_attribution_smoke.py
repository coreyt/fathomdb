"""Smoke tests verifying SearchHit.attribution / HitAttribution.matched_paths
passthrough in the Python bindings (Pack C-2)."""

from __future__ import annotations

import dataclasses
from pathlib import Path


def test_search_hit_has_attribution_field() -> None:
    """SearchHit must declare an 'attribution' field (structural check)."""
    from fathomdb import SearchHit

    fields = {f.name for f in dataclasses.fields(SearchHit)}
    assert "attribution" in fields, (
        f"SearchHit is missing 'attribution' field; fields present: {sorted(fields)}"
    )


def test_hit_attribution_has_matched_paths_field() -> None:
    """HitAttribution must declare a 'matched_paths' field (structural check)."""
    from fathomdb import HitAttribution

    fields = {f.name for f in dataclasses.fields(HitAttribution)}
    assert "matched_paths" in fields, (
        f"HitAttribution is missing 'matched_paths' field; fields present: {sorted(fields)}"
    )


def test_hit_attribution_from_wire_round_trips_paths() -> None:
    """HitAttribution.from_wire must preserve matched_paths values."""
    from fathomdb._types import HitAttribution

    payload = {"matched_paths": ["$.title", "$.payload.body"]}
    att = HitAttribution.from_wire(payload)
    assert att.matched_paths == ("$.title", "$.payload.body"), (
        f"Expected matched_paths to round-trip; got {att.matched_paths!r}"
    )


def test_hit_attribution_from_wire_empty_paths() -> None:
    """HitAttribution.from_wire must handle absent matched_paths gracefully."""
    from fathomdb._types import HitAttribution

    att = HitAttribution.from_wire({})
    assert att.matched_paths == (), (
        f"Expected empty tuple for missing matched_paths; got {att.matched_paths!r}"
    )


def test_search_hit_attribution_none_when_not_requested(tmp_path: Path) -> None:
    """Without with_match_attribution(), SearchHit.attribution must be None."""
    from fathomdb import (
        ChunkPolicy,
        Engine,
        FtsPropertyPathMode,
        FtsPropertyPathSpec,
        NodeInsert,
        WriteRequest,
        new_row_id,
    )

    db = Engine.open(tmp_path / "smoke.db")
    db.admin.register_fts_property_schema_with_entries(
        "SmokeThing",
        [FtsPropertyPathSpec(path="$.title", mode=FtsPropertyPathMode.SCALAR)],
        separator=" ",
        exclude_paths=[],
    )
    db.write(
        WriteRequest(
            label="smoke-seed",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="smoke-node-1",
                    kind="SmokeThing",
                    properties={"title": "hello world fathomdb"},
                    source_ref="smoke",
                    upsert=False,
                    chunk_policy=ChunkPolicy.PRESERVE,
                )
            ],
        )
    )
    rows = db.query("SmokeThing").text_search("fathomdb", 10).execute()
    assert rows.hits, "smoke seed should produce at least one hit"
    for hit in rows.hits:
        assert hit.attribution is None, (
            f"attribution must be None without with_match_attribution(); "
            f"got {hit.attribution!r}"
        )


def test_search_hit_attribution_present_when_requested(tmp_path: Path) -> None:
    """With with_match_attribution(), SearchHit.attribution must not be None."""
    from fathomdb import (
        ChunkPolicy,
        Engine,
        FtsPropertyPathMode,
        FtsPropertyPathSpec,
        NodeInsert,
        WriteRequest,
        new_row_id,
    )

    db = Engine.open(tmp_path / "smoke_attr.db")
    db.admin.register_fts_property_schema_with_entries(
        "AttrItem",
        [FtsPropertyPathSpec(path="$.title", mode=FtsPropertyPathMode.SCALAR)],
        separator=" ",
        exclude_paths=[],
    )
    db.write(
        WriteRequest(
            label="attr-seed",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="attr-node-1",
                    kind="AttrItem",
                    properties={"title": "matched attribution smoke"},
                    source_ref="smoke",
                    upsert=False,
                    chunk_policy=ChunkPolicy.PRESERVE,
                )
            ],
        )
    )
    rows = (
        db.query("AttrItem").text_search("smoke", 10).with_match_attribution().execute()
    )
    assert rows.hits, "attribution smoke seed should produce at least one hit"
    attributed = [h for h in rows.hits if h.attribution is not None]
    assert attributed, "at least one hit must have attribution when requested"
    att = attributed[0].attribution
    assert isinstance(att.matched_paths, tuple), (
        f"matched_paths must be a tuple; got {type(att.matched_paths)}"
    )
