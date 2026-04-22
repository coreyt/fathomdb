"""Pack B: AdminClient.configure_embedding database-wide embedding identity."""

from __future__ import annotations

from pathlib import Path
from unittest.mock import MagicMock


def test_configure_embedding_fresh_engine_activated(tmp_path: Path) -> None:
    """Fresh DB + configure_embedding activates a new profile."""
    from fathomdb import Engine

    db = Engine.open(tmp_path / "agent.db")

    mock_identity = MagicMock()
    mock_identity.model_identity = "fake-model"
    mock_identity.model_version = "1.0"
    mock_identity.dimensions = 64
    mock_identity.normalization_policy = "l2"

    mock_embedder = MagicMock()
    mock_embedder.identity.return_value = mock_identity

    result = db.admin.configure_embedding(mock_embedder)
    assert result["outcome"] == "activated"
    assert isinstance(result["profile_id"], int)
    assert result["profile_id"] > 0


def test_configure_embedding_identical_is_noop(tmp_path: Path) -> None:
    """Calling configure_embedding twice with the same identity is a no-op."""
    from fathomdb import Engine

    db = Engine.open(tmp_path / "agent.db")

    mock_identity = MagicMock()
    mock_identity.model_identity = "fake-model"
    mock_identity.model_version = "1.0"
    mock_identity.dimensions = 64
    mock_identity.normalization_policy = "l2"

    mock_embedder = MagicMock()
    mock_embedder.identity.return_value = mock_identity

    first = db.admin.configure_embedding(mock_embedder)
    second = db.admin.configure_embedding(mock_embedder)
    assert first["outcome"] == "activated"
    assert second["outcome"] == "unchanged"
    assert second["profile_id"] == first["profile_id"]
