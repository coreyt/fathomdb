"""Admin surface tests that live closest to the Memex reproduction.

These tests guard the Python admin binding against regressions in
engine-side `check_semantics` / `check_integrity`. In particular they
cover the 0.5.2 Pack A fix where `check_semantics` crashed with
`SqliteError: no such column: fp.text_content` on any DB that had
registered a weighted (per-column) FTS property schema.
"""

from __future__ import annotations

from pathlib import Path


def test_check_semantics_survives_weighted_fts_registration(tmp_path: Path) -> None:
    """Mirror the Memex repro: register a weighted FTS property schema,
    reopen the engine, and call `check_semantics()`. Before the 0.5.2 Pack A
    fix this raised `SqliteError: no such column: fp.text_content`."""
    from fathomdb import (
        Engine,
        FtsPropertyPathMode,
        FtsPropertyPathSpec,
    )

    db_path = tmp_path / "agent.db"
    db = Engine.open(db_path)

    # Weighted schema: at least one entry carries a weight, which triggers
    # the per-column `fts_props_<kind>` layout on the engine side.
    db.admin.register_fts_property_schema_with_entries(
        "Article",
        [
            FtsPropertyPathSpec(
                path="$.title", mode=FtsPropertyPathMode.SCALAR, weight=10.0
            ),
            FtsPropertyPathSpec(
                path="$.body", mode=FtsPropertyPathMode.RECURSIVE, weight=1.0
            ),
        ],
    )

    # Reopen the engine on the same file to mirror the "fresh connection"
    # reproduction path Memex hit on its retrieval layer.
    del db
    db = Engine.open(db_path)

    report = db.admin.check_semantics()

    assert report.dangling_edges == 0
    assert report.orphaned_chunks == 0
    # The Pack A fix target: drift counting must not crash on weighted
    # per-kind FTS tables and must report zero drift on a clean DB.
    assert report.drifted_property_fts_rows == 0
