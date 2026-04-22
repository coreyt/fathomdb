"""Pack C: per-kind configure_vec + get_vec_index_status Python parity."""

from __future__ import annotations

import warnings
from pathlib import Path
from unittest.mock import MagicMock


def _seed_active_profile(db_path: Path, dimensions: int = 384) -> int:
    """Insert an active embedding profile row directly via sqlite3."""
    import sqlite3

    conn = sqlite3.connect(str(db_path))
    try:
        conn.execute(
            "INSERT INTO vector_embedding_profiles "
            "(profile_name, model_identity, model_version, dimensions, "
            "normalization_policy, max_tokens, active, activated_at, created_at) "
            "VALUES ('test-profile', 'test/model', 'v1', ?, 'l2', 512, 1, "
            "unixepoch(), unixepoch())",
            (dimensions,),
        )
        conn.commit()
        return conn.execute(
            "SELECT profile_id FROM vector_embedding_profiles WHERE active = 1"
        ).fetchone()[0]
    finally:
        conn.close()


def test_configure_vec_per_kind_and_status(tmp_path: Path) -> None:
    """End-to-end: configure_vec("Kind") enqueues backfill; status reflects it."""
    from fathomdb import Engine

    db_path = tmp_path / "agent.db"
    db = Engine.open(db_path)

    # Seed 2 nodes+chunks of kind "Note" via the writer.
    for i in range(2):
        db.write(
            label=f"seed-{i}",
            nodes=[
                {
                    "row_id": f"row-{i}",
                    "logical_id": f"note:{i}",
                    "kind": "Note",
                    "properties": "{}",
                }
            ],
            chunks=[
                {
                    "id": f"chunk-{i}",
                    "node_logical_id": f"note:{i}",
                    "text_content": f"body {i}",
                }
            ],
        )

    # Close so we can directly seed the embedding profile.
    db.close()
    _seed_active_profile(db_path, dimensions=384)

    db = Engine.open(db_path)
    outcome = db.admin.configure_vec("Note", source="chunks")
    assert outcome["kind"] == "Note"
    assert outcome["enqueued_backfill_rows"] == 2
    assert outcome["was_already_enabled"] is False

    status = db.admin.get_vec_index_status("Note")
    assert status["enabled"] is True
    assert status["pending_backfill"] == 2
    assert status["pending_incremental"] == 0
    assert status["embedding_identity"] == "test/model"


def test_configure_vec_deprecated_embedder_shim_emits_warning(tmp_path: Path) -> None:
    """Calling configure_vec(embedder) (old shape) emits DeprecationWarning and still works."""
    from fathomdb._admin import AdminClient

    mock_core = MagicMock()
    # preview_projection_impact -> rows_to_rebuild=0 so RebuildImpactError does not fire.
    import json as _json

    mock_core.preview_projection_impact.return_value = _json.dumps(
        {
            "rows_to_rebuild": 0,
            "estimated_seconds": 0,
            "temp_db_size_bytes": 0,
            "current_tokenizer": None,
            "target_tokenizer": None,
        }
    )
    mock_core.set_vec_profile.return_value = _json.dumps(
        {
            "model_identity": "foo/bar",
            "model_version": "v1",
            "dimensions": 384,
            "active_at": 1,
            "created_at": 1,
        }
    )

    admin = AdminClient(mock_core)

    class DummyIdentity:
        model_identity = "foo/bar"
        model_version = "v1"
        dimensions = 384
        normalization_policy = "l2"

    class DummyEmbedder:
        def identity(self) -> DummyIdentity:
            return DummyIdentity()

    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        admin.configure_vec(DummyEmbedder())
        assert any(
            issubclass(w.category, DeprecationWarning) for w in caught
        ), f"expected DeprecationWarning, got {[w.category for w in caught]}"
    mock_core.set_vec_profile.assert_called_once()
