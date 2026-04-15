"""Tests for Pack C: profile management (FtsProfile, VecProfile, AdminClient methods)."""

from __future__ import annotations

import json
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest


# ---------------------------------------------------------------------------
# 1. test_get_fts_profile_none_pre_configure
# ---------------------------------------------------------------------------


def test_get_fts_profile_none_pre_configure(tmp_path: Path) -> None:
    """Fresh DB: get_fts_profile returns None for an unknown kind."""
    from fathomdb import Engine

    db = Engine.open(tmp_path / "agent.db")
    result = db.admin.get_fts_profile("Book")
    assert result is None


# ---------------------------------------------------------------------------
# 2. test_get_vec_profile_none_pre_configure
# ---------------------------------------------------------------------------


def test_get_vec_profile_none_pre_configure(tmp_path: Path) -> None:
    """Fresh DB: get_vec_profile returns None before any vec profile is set."""
    from fathomdb import Engine

    db = Engine.open(tmp_path / "agent.db")
    result = db.admin.get_vec_profile()
    assert result is None


# ---------------------------------------------------------------------------
# 3. test_rebuild_impact_error_raised_when_rows_gt_0
# ---------------------------------------------------------------------------


def test_rebuild_impact_error_raised_when_rows_gt_0(tmp_path: Path) -> None:
    """When preview returns rows_to_rebuild > 0, configure_fts raises RebuildImpactError."""
    from fathomdb import Engine, RebuildImpactError

    db = Engine.open(tmp_path / "agent.db")

    impact_json = json.dumps({
        "rows_to_rebuild": 5,
        "estimated_seconds": 2,
        "temp_db_size_bytes": 1024,
        "current_tokenizer": "unicode61",
        "target_tokenizer": "porter unicode61 remove_diacritics 2",
    })

    with patch.object(db.admin._core, "preview_projection_impact", return_value=impact_json):
        with pytest.raises(RebuildImpactError) as exc_info:
            db.admin.configure_fts("Book", "porter unicode61 remove_diacritics 2")

    assert exc_info.value.report.rows_to_rebuild == 5


# ---------------------------------------------------------------------------
# 4. test_configure_fts_proceeds_with_agree_flag
# ---------------------------------------------------------------------------


def test_configure_fts_proceeds_with_agree_flag(tmp_path: Path) -> None:
    """With agree_to_rebuild_impact=True, configure_fts does not raise and returns FtsProfile."""
    from fathomdb import Engine, FtsProfile

    db = Engine.open(tmp_path / "agent.db")

    impact_json = json.dumps({
        "rows_to_rebuild": 5,
        "estimated_seconds": 2,
        "temp_db_size_bytes": 1024,
        "current_tokenizer": None,
        "target_tokenizer": "unicode61",
    })
    profile_json = json.dumps({
        "kind": "Book",
        "tokenizer": "unicode61",
        "active_at": None,
        "created_at": 1000000,
    })

    with patch.object(db.admin._core, "preview_projection_impact", return_value=impact_json):
        with patch.object(db.admin._core, "set_fts_profile", return_value=profile_json):
            result = db.admin.configure_fts("Book", "unicode61", agree_to_rebuild_impact=True)

    assert isinstance(result, FtsProfile)
    assert result.kind == "Book"
    assert result.tokenizer == "unicode61"


# ---------------------------------------------------------------------------
# 5. test_fts_profile_roundtrip
# ---------------------------------------------------------------------------


def test_fts_profile_roundtrip(tmp_path: Path) -> None:
    """configure_fts then get_fts_profile returns a matching FtsProfile."""
    from fathomdb import Engine, FtsProfile

    db = Engine.open(tmp_path / "agent.db")

    # Fresh DB has no rows, so impact.rows_to_rebuild == 0; no agree needed
    profile = db.admin.configure_fts("Article", "unicode61")

    assert isinstance(profile, FtsProfile)
    assert profile.kind == "Article"
    assert profile.tokenizer == "unicode61"

    fetched = db.admin.get_fts_profile("Article")
    assert fetched is not None
    assert fetched.kind == "Article"
    assert fetched.tokenizer == "unicode61"


# ---------------------------------------------------------------------------
# 6. test_preset_name_resolution
# ---------------------------------------------------------------------------


def test_preset_name_resolution(tmp_path: Path) -> None:
    """Using a preset name stores the expanded tokenizer string."""
    from fathomdb import Engine, FtsProfile

    db = Engine.open(tmp_path / "agent.db")

    profile = db.admin.configure_fts("Book", "recall-optimized-english")

    assert isinstance(profile, FtsProfile)
    assert profile.tokenizer == "porter unicode61 remove_diacritics 2"


# ---------------------------------------------------------------------------
# 7. test_preview_projection_impact_returns_impact_report
# ---------------------------------------------------------------------------


def test_preview_projection_impact_returns_impact_report(tmp_path: Path) -> None:
    """preview_projection_impact returns an ImpactReport with correct fields."""
    from fathomdb import Engine, ImpactReport

    db = Engine.open(tmp_path / "agent.db")
    report = db.admin.preview_projection_impact("Book", "fts")

    assert isinstance(report, ImpactReport)
    assert report.rows_to_rebuild >= 0
    assert report.estimated_seconds >= 0
    assert report.temp_db_size_bytes >= 0


# ---------------------------------------------------------------------------
# 8. test_configure_vec_roundtrip
# ---------------------------------------------------------------------------


def test_configure_vec_roundtrip(tmp_path: Path) -> None:
    """configure_vec with a mock embedder stores a VecProfile, get_vec_profile returns it."""
    from fathomdb import Engine, VecProfile

    db = Engine.open(tmp_path / "agent.db")

    # Create a mock embedder
    mock_identity = MagicMock()
    mock_identity.model_identity = "test-model"
    mock_identity.model_version = "1.0"
    mock_identity.dimensions = 128
    mock_identity.normalization_policy = "none"

    mock_embedder = MagicMock()
    mock_embedder.identity.return_value = mock_identity

    impact_json = json.dumps({
        "rows_to_rebuild": 0,
        "estimated_seconds": 0,
        "temp_db_size_bytes": 0,
        "current_tokenizer": None,
        "target_tokenizer": None,
    })

    with patch.object(db.admin._core, "preview_projection_impact", return_value=impact_json):
        result = db.admin.configure_vec(mock_embedder)

    assert isinstance(result, VecProfile)
    assert result.model_identity == "test-model"
    assert result.dimensions == 128

    fetched = db.admin.get_vec_profile()
    assert fetched is not None
    assert fetched.model_identity == "test-model"


# ---------------------------------------------------------------------------
# 9. test_rebuild_impact_error_has_report
# ---------------------------------------------------------------------------


def test_rebuild_impact_error_has_report(tmp_path: Path) -> None:
    """RebuildImpactError.report has the expected fields from the impact data."""
    from fathomdb import Engine, RebuildImpactError

    db = Engine.open(tmp_path / "agent.db")

    impact_json = json.dumps({
        "rows_to_rebuild": 42,
        "estimated_seconds": 10,
        "temp_db_size_bytes": 8192,
        "current_tokenizer": "unicode61",
        "target_tokenizer": "trigram",
    })

    with patch.object(db.admin._core, "preview_projection_impact", return_value=impact_json):
        try:
            db.admin.configure_fts("Doc", "trigram")
            pytest.fail("Expected RebuildImpactError")
        except RebuildImpactError as exc:
            assert exc.report.rows_to_rebuild == 42
            assert exc.report.estimated_seconds == 10
            assert exc.report.temp_db_size_bytes == 8192
            assert exc.report.current_tokenizer == "unicode61"
            assert exc.report.target_tokenizer == "trigram"


# ---------------------------------------------------------------------------
# 10. test_async_mode_returns_fast
# ---------------------------------------------------------------------------


def test_async_mode_returns_fast(tmp_path: Path) -> None:
    """configure_fts with mode=RebuildMode.ASYNC returns without blocking."""
    import time

    from fathomdb import Engine, FtsProfile, RebuildMode

    db = Engine.open(tmp_path / "agent.db")

    start = time.monotonic()
    profile = db.admin.configure_fts("FastKind", "unicode61", mode=RebuildMode.ASYNC)
    elapsed = time.monotonic() - start

    assert isinstance(profile, FtsProfile)
    # Should complete in well under 5 seconds for an empty DB
    assert elapsed < 5.0
