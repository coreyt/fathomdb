"""Tests for Pack C: profile management (FtsProfile, VecProfile, AdminClient methods)."""

from __future__ import annotations

import json
from pathlib import Path
from unittest.mock import MagicMock

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
    result = db.admin.get_vec_profile("Document")
    assert result is None


# ---------------------------------------------------------------------------
# 3. test_rebuild_impact_error_raised_when_rows_gt_0
# ---------------------------------------------------------------------------


def test_rebuild_impact_error_raised_when_rows_gt_0(tmp_path: Path) -> None:
    """When preview returns rows_to_rebuild > 0, configure_fts raises RebuildImpactError."""
    from fathomdb._admin import AdminClient
    from fathomdb import RebuildImpactError

    impact_json = json.dumps(
        {
            "rows_to_rebuild": 5,
            "estimated_seconds": 2,
            "temp_db_size_bytes": 1024,
            "current_tokenizer": "unicode61",
            "target_tokenizer": "porter unicode61 remove_diacritics 2",
        }
    )

    mock_core = MagicMock()
    mock_core.preview_projection_impact.return_value = impact_json
    admin = AdminClient(mock_core)

    with pytest.raises(RebuildImpactError) as exc_info:
        admin.configure_fts("Book", "porter unicode61 remove_diacritics 2")

    assert exc_info.value.report.rows_to_rebuild == 5


# ---------------------------------------------------------------------------
# 4. test_configure_fts_proceeds_with_agree_flag
# ---------------------------------------------------------------------------


def test_configure_fts_proceeds_with_agree_flag(tmp_path: Path) -> None:
    """With agree_to_rebuild_impact=True, configure_fts does not raise and returns FtsProfile."""
    from fathomdb._admin import AdminClient
    from fathomdb import FtsProfile

    impact_json = json.dumps(
        {
            "rows_to_rebuild": 5,
            "estimated_seconds": 2,
            "temp_db_size_bytes": 1024,
            "current_tokenizer": None,
            "target_tokenizer": "unicode61",
        }
    )
    profile_json = json.dumps(
        {
            "kind": "Book",
            "tokenizer": "unicode61",
            "active_at": None,
            "created_at": 1000000,
        }
    )
    schema_json = json.dumps(
        {
            "kind": "Book",
            "property_paths": ["$.title"],
            "entries": [{"path": "$.title", "mode": "scalar"}],
            "exclude_paths": [],
            "separator": " ",
            "format_version": 1,
        }
    )

    mock_core = MagicMock()
    mock_core.preview_projection_impact.return_value = impact_json
    mock_core.set_fts_profile.return_value = profile_json
    mock_core.describe_fts_property_schema.return_value = schema_json
    mock_core.register_fts_property_schema_with_entries.return_value = schema_json
    admin = AdminClient(mock_core)

    result = admin.configure_fts("Book", "unicode61", agree_to_rebuild_impact=True)

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
    # configure_fts requires a registered property schema (ARCH-005).
    db.admin.register_fts_property_schema("Article", ["$.title"])

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
    # configure_fts requires a registered property schema (ARCH-005).
    db.admin.register_fts_property_schema("Book", ["$.title"])

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
    from fathomdb._admin import AdminClient
    from fathomdb import VecProfile

    # Create a mock embedder
    mock_identity = MagicMock()
    mock_identity.model_identity = "test-model"
    mock_identity.model_version = "1.0"
    mock_identity.dimensions = 128
    mock_identity.normalization_policy = "none"

    mock_embedder = MagicMock()
    mock_embedder.identity.return_value = mock_identity

    impact_json = json.dumps(
        {
            "rows_to_rebuild": 0,
            "estimated_seconds": 0,
            "temp_db_size_bytes": 0,
            "current_tokenizer": None,
            "target_tokenizer": None,
        }
    )
    profile_json = json.dumps(
        {
            "model_identity": "test-model",
            "model_version": "1.0",
            "dimensions": 128,
            "active_at": None,
            "created_at": 1000000,
        }
    )

    mock_core = MagicMock()
    mock_core.preview_projection_impact.return_value = impact_json
    mock_core.set_vec_profile.return_value = profile_json
    mock_core.get_vec_profile.return_value = profile_json
    admin = AdminClient(mock_core)

    result = admin.configure_vec(mock_embedder)

    assert isinstance(result, VecProfile)
    assert result.model_identity == "test-model"
    assert result.dimensions == 128

    fetched = admin.get_vec_profile("Document")
    assert fetched is not None
    assert fetched.model_identity == "test-model"


# ---------------------------------------------------------------------------
# 9. test_rebuild_impact_error_has_report
# ---------------------------------------------------------------------------


def test_rebuild_impact_error_has_report(tmp_path: Path) -> None:
    """RebuildImpactError.report has the expected fields from the impact data."""
    from fathomdb._admin import AdminClient
    from fathomdb import RebuildImpactError

    impact_json = json.dumps(
        {
            "rows_to_rebuild": 42,
            "estimated_seconds": 10,
            "temp_db_size_bytes": 8192,
            "current_tokenizer": "unicode61",
            "target_tokenizer": "trigram",
        }
    )

    mock_core = MagicMock()
    mock_core.preview_projection_impact.return_value = impact_json
    admin = AdminClient(mock_core)

    try:
        admin.configure_fts("Doc", "trigram")
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
    """configure_fts returns quickly on an empty DB (mode kwarg removed in 0.5.1)."""
    import time

    from fathomdb import Engine, FtsProfile

    db = Engine.open(tmp_path / "agent.db")
    # Pre-register a schema so configure_fts has something to re-register.
    db.admin.register_fts_property_schema("FastKind", ["$.name"])

    start = time.monotonic()
    profile = db.admin.configure_fts("FastKind", "unicode61")
    elapsed = time.monotonic() - start

    assert isinstance(profile, FtsProfile)
    # Should complete in well under 5 seconds for an empty DB
    assert elapsed < 5.0


# ---------------------------------------------------------------------------
# 11. test_fts_path_spec_weight_to_wire
# ---------------------------------------------------------------------------


def test_fts_path_spec_weight_to_wire() -> None:
    """FtsPropertyPathSpec.to_wire() includes weight when set."""
    from fathomdb import FtsPropertyPathSpec

    spec = FtsPropertyPathSpec(path="$.title", weight=10.0)
    wire = spec.to_wire()
    assert wire["weight"] == 10.0


# ---------------------------------------------------------------------------
# 12. test_fts_path_spec_no_weight_omits_key
# ---------------------------------------------------------------------------


def test_fts_path_spec_no_weight_omits_key() -> None:
    """FtsPropertyPathSpec.to_wire() omits weight key when weight is None."""
    from fathomdb import FtsPropertyPathSpec

    spec = FtsPropertyPathSpec(path="$.body")
    wire = spec.to_wire()
    assert "weight" not in wire


# ---------------------------------------------------------------------------
# 13. test_fts_property_schema_record_from_wire_preserves_weight
# ---------------------------------------------------------------------------


def test_fts_property_schema_record_from_wire_preserves_weight() -> None:
    """FtsPropertySchemaRecord.from_wire() round-trips weight from the wire dict."""
    from fathomdb import FtsPropertySchemaRecord

    wire = {
        "kind": "Article",
        "property_paths": ["$.title"],
        "entries": [{"path": "$.title", "mode": "scalar", "weight": 5.0}],
        "exclude_paths": [],
        "separator": " ",
        "format_version": 1,
    }
    record = FtsPropertySchemaRecord.from_wire(wire)
    assert len(record.entries) == 1
    assert record.entries[0].weight == 5.0


def test_fts_property_schema_record_from_wire_no_weight() -> None:
    """FtsPropertySchemaRecord.from_wire() sets weight=None when absent."""
    from fathomdb import FtsPropertySchemaRecord

    wire = {
        "kind": "Article",
        "property_paths": ["$.title"],
        "entries": [{"path": "$.title", "mode": "scalar"}],
        "exclude_paths": [],
        "separator": " ",
        "format_version": 1,
    }
    record = FtsPropertySchemaRecord.from_wire(wire)
    assert len(record.entries) == 1
    assert record.entries[0].weight is None


# ---------------------------------------------------------------------------
# ARCH-005: configure_fts auto-re-registers the FTS property schema
# ---------------------------------------------------------------------------


def test_configure_fts_reregisters_existing_schema_on_tokenizer_change(
    tmp_path: Path,
) -> None:
    """After configure_fts(kind, new_tokenizer), the existing property schema is
    re-registered so the new tokenizer is applied to the index.

    This mirrors the TypeScript `configureFts` behavior (ARCH-005). We assert
    that `register_fts_property_schema_with_entries` is invoked after
    `set_fts_profile`, passing through the existing schema's entries.
    """
    from fathomdb._admin import AdminClient

    impact_json = json.dumps(
        {
            "rows_to_rebuild": 0,
            "estimated_seconds": 0,
            "temp_db_size_bytes": 0,
            "current_tokenizer": "unicode61",
            "target_tokenizer": "porter unicode61 remove_diacritics 2",
        }
    )
    profile_json = json.dumps(
        {
            "kind": "Note",
            "tokenizer": "porter unicode61 remove_diacritics 2",
            "active_at": None,
            "created_at": 1000000,
        }
    )
    schema_json = json.dumps(
        {
            "kind": "Note",
            "property_paths": ["$.title", "$.body"],
            "entries": [
                {"path": "$.title", "mode": "scalar"},
                {"path": "$.body", "mode": "scalar"},
            ],
            "exclude_paths": [],
            "separator": " ",
            "format_version": 1,
        }
    )

    mock_core = MagicMock()
    mock_core.preview_projection_impact.return_value = impact_json
    mock_core.set_fts_profile.return_value = profile_json
    mock_core.describe_fts_property_schema.return_value = schema_json
    mock_core.register_fts_property_schema_with_entries.return_value = schema_json
    admin = AdminClient(mock_core)

    admin.configure_fts("Note", "recall-optimized-english")

    # set_fts_profile was called with the resolved tokenizer.
    mock_core.set_fts_profile.assert_called_once()
    profile_request = json.loads(mock_core.set_fts_profile.call_args.args[0])
    assert profile_request["kind"] == "Note"
    assert profile_request["tokenizer"] == "porter unicode61 remove_diacritics 2"

    # re-registration uses the existing schema entries.
    mock_core.register_fts_property_schema_with_entries.assert_called_once()
    reg_request = json.loads(
        mock_core.register_fts_property_schema_with_entries.call_args.args[0]
    )
    assert reg_request["kind"] == "Note"
    assert [entry["path"] for entry in reg_request["entries"]] == [
        "$.title",
        "$.body",
    ]
    assert reg_request["separator"] == " "
    assert reg_request["exclude_paths"] == []


def test_configure_fts_raises_when_no_schema_registered(tmp_path: Path) -> None:
    """configure_fts requires a registered FTS property schema for the kind.

    Without one, it raises ValueError rather than silently skipping — callers
    must either register a schema first or use a different surface.
    """
    from fathomdb._admin import AdminClient

    impact_json = json.dumps(
        {
            "rows_to_rebuild": 0,
            "estimated_seconds": 0,
            "temp_db_size_bytes": 0,
            "current_tokenizer": None,
            "target_tokenizer": "unicode61",
        }
    )
    profile_json = json.dumps(
        {
            "kind": "Unknown",
            "tokenizer": "unicode61",
            "active_at": None,
            "created_at": 1000000,
        }
    )
    # describe returns a null-kind payload for an unregistered schema.
    empty_schema_json = json.dumps({"kind": None})

    mock_core = MagicMock()
    mock_core.preview_projection_impact.return_value = impact_json
    mock_core.set_fts_profile.return_value = profile_json
    mock_core.describe_fts_property_schema.return_value = empty_schema_json
    admin = AdminClient(mock_core)

    with pytest.raises(ValueError, match="no FTS property schema"):
        admin.configure_fts("Unknown", "unicode61")


def test_configure_fts_does_not_persist_profile_when_schema_missing(
    tmp_path: Path,
) -> None:
    """Real-DB integration: when no FTS property schema is registered, the
    ValueError path must not leave behind a mutated FTS profile.

    Regression test for ARCH-005 reviewer Note 1: previously `set_fts_profile`
    ran before the schema existence check, so callers saw an exception while
    the tokenizer profile had already been written. After the fix, the schema
    check runs first and nothing is persisted on the error path.
    """
    from fathomdb import Engine

    db = Engine.open(tmp_path / "agent.db")

    # Sanity: no profile exists yet for this kind.
    assert db.admin.get_fts_profile("NoSchemaKind") is None

    with pytest.raises(ValueError, match="no FTS property schema"):
        db.admin.configure_fts("NoSchemaKind", "porter")

    # The profile must NOT have been written: the operation failed, so state
    # should be unchanged.
    assert db.admin.get_fts_profile("NoSchemaKind") is None


def test_configure_fts_rejects_mode_kwarg(tmp_path: Path) -> None:
    """The `mode` parameter was removed in 0.5.1; passing it raises TypeError."""
    from fathomdb._admin import AdminClient

    mock_core = MagicMock()
    admin = AdminClient(mock_core)

    with pytest.raises(TypeError, match="mode"):
        admin.configure_fts("Note", "unicode61", mode="async")
