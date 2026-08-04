"""0.8.21 Slice 45 — nested projections through the public Python binding."""

from __future__ import annotations

import json

from fathomdb import Engine, ProjectionRole, ProjectionSpec, SearchFilter


def test_nested_projection_type_collapsed_equality_and_projected_search(db_path: str) -> None:
    """A real database makes nested string ``"1"`` and numeric ``1`` equal."""
    engine = Engine.open(db_path, use_default_embedder=False)
    try:
        spec = ProjectionSpec(
            name="value",
            roles=frozenset({ProjectionRole.FILTERABLE, ProjectionRole.SEARCHABLE}),
            fts=True,
            source=("attributes", "core:value", "value"),
        )
        engine.configure_projections([spec])
        engine.write(
            [
                {
                    "kind": "doc",
                    "logical_id": "text",
                    "source_id": "py-slice45:text",
                    "body": json.dumps({"attributes": {"core:value": {"value": "1"}}}),
                },
                {
                    "kind": "doc",
                    "logical_id": "number",
                    "source_id": "py-slice45:number",
                    "body": json.dumps({"attributes": {"core:value": {"value": 1}}}),
                },
                {
                    "kind": "doc",
                    "logical_id": "different",
                    "source_id": "py-slice45:different",
                    "body": json.dumps(
                        {"note": "1", "attributes": {"core:value": {"value": "2"}}}
                    ),
                },
            ]
        )
        result = engine.search_projected_text(
            "1", "value", SearchFilter(attributes=(("value", "1"),))
        )
        assert {hit.id.value for hit in result.results} == {"text", "number"}
        assert "different" in {hit.id.value for hit in engine.search("1").results}
        hybrid = engine.search("1", SearchFilter(attributes=(("value", "1"),)))
        assert "different" not in {hit.id.value for hit in hybrid.results}
    finally:
        engine.close()
