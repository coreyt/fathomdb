"""Tests for Pack F: Admin CLI (configure-fts, configure-vec, preview-impact, get-*-profile)."""

from __future__ import annotations

import json
from unittest.mock import MagicMock, patch

import pytest
from click.testing import CliRunner


# ---------------------------------------------------------------------------
# Private helpers
# ---------------------------------------------------------------------------


def _build_impact(rows: int):
    from fathomdb import ImpactReport

    return ImpactReport.from_wire(
        {
            "rows_to_rebuild": rows,
            "estimated_seconds": 0,
            "temp_db_size_bytes": 0,
            "current_tokenizer": "unicode61",
            "target_tokenizer": "porter unicode61 remove_diacritics 2",
        }
    )


def _build_fts_profile():
    from fathomdb import FtsProfile

    return FtsProfile.from_wire(
        {"kind": "Book", "tokenizer": "unicode61", "active_at": None, "created_at": 0}
    )


def _build_vec_profile():
    from fathomdb import VecProfile

    return VecProfile.from_wire(
        {
            "model_identity": "bge-small-en-v1.5",
            "model_version": "1.5",
            "dimensions": 384,
            "active_at": None,
            "created_at": 0,
        }
    )


def _impact_error(rows: int):
    from fathomdb import RebuildImpactError

    return RebuildImpactError(_build_impact(rows))


# ---------------------------------------------------------------------------
# 1. test_abort_without_flag_when_rows_gt_0
#    mock AdminClient.preview_projection_impact to return rows=5;
#    run configure-fts without --agree flag; assert exit != 0 or output
#    contains abort/cancelled
# ---------------------------------------------------------------------------


def test_abort_without_flag_when_rows_gt_0():
    """configure-fts without --agree-to-rebuild-impact aborts when rows > 0."""
    from fathomdb._cli import cli

    runner = CliRunner()

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        # configure_fts raises RebuildImpactError when rows>0 and agree=False
        mock_engine.admin.configure_fts.side_effect = _impact_error(5)
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            [
                "admin",
                "configure-fts",
                "--db",
                "/tmp/test.db",
                "--kind",
                "Book",
                "--tokenizer",
                "unicode61",
            ],
            # empty input simulates non-interactive / CI (no tty)
            input="",
        )

    # Either non-zero exit or output mentions abort/cancel/impact
    output_lower = (result.output or "").lower()
    assert result.exit_code != 0 or any(
        kw in output_lower for kw in ("abort", "cancel", "impact", "rebuild")
    )


# ---------------------------------------------------------------------------
# 2. test_succeed_with_agree_flag
#    mock preview rows=5; run with --agree-to-rebuild-impact; assert exit 0
# ---------------------------------------------------------------------------


def test_succeed_with_agree_flag():
    """configure-fts with --agree-to-rebuild-impact exits 0 even when rows > 0."""
    from fathomdb._cli import cli

    runner = CliRunner()

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.configure_fts.return_value = _build_fts_profile()
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            [
                "admin",
                "configure-fts",
                "--db",
                "/tmp/test.db",
                "--kind",
                "Book",
                "--tokenizer",
                "unicode61",
                "--agree-to-rebuild-impact",
            ],
        )

    assert result.exit_code == 0, result.output


# ---------------------------------------------------------------------------
# 3. test_no_prompt_on_zero_rows
#    mock preview rows=0; run configure-fts without flag; assert exit 0
# ---------------------------------------------------------------------------


def test_no_prompt_on_zero_rows():
    """configure-fts without flag exits 0 when configure_fts does not raise."""
    from fathomdb._cli import cli

    runner = CliRunner()

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        # configure_fts does NOT raise (rows_to_rebuild == 0 internally)
        mock_engine.admin.configure_fts.return_value = _build_fts_profile()
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            [
                "admin",
                "configure-fts",
                "--db",
                "/tmp/test.db",
                "--kind",
                "Book",
                "--tokenizer",
                "unicode61",
            ],
        )

    assert result.exit_code == 0, result.output


# ---------------------------------------------------------------------------
# 4. test_preview_prints_report
#    run preview-impact; assert output contains "rows_to_rebuild" or the count
# ---------------------------------------------------------------------------


def test_preview_prints_report():
    """preview-impact prints ImpactReport fields to stdout."""
    from fathomdb._cli import cli

    runner = CliRunner()

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.preview_projection_impact.return_value = _build_impact(3)
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            [
                "admin",
                "preview-impact",
                "--db",
                "/tmp/test.db",
                "--kind",
                "Book",
                "--target",
                "fts",
            ],
        )

    assert result.exit_code == 0, result.output
    assert "rows_to_rebuild" in result.output or "3" in result.output


# ---------------------------------------------------------------------------
# 5. test_get_fts_profile_no_profile_message
#    mock get_fts_profile returning None; assert output contains "No FTS profile"
# ---------------------------------------------------------------------------


def test_get_fts_profile_no_profile_message():
    """get-fts-profile prints a human-readable message when no profile is set."""
    from fathomdb._cli import cli

    runner = CliRunner()

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.get_fts_profile.return_value = None
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            [
                "admin",
                "get-fts-profile",
                "--db",
                "/tmp/test.db",
                "--kind",
                "Book",
            ],
        )

    assert result.exit_code == 0, result.output
    assert "No FTS profile" in result.output


# ---------------------------------------------------------------------------
# 6. test_preset_name_accepted
#    configure-fts with --tokenizer "recall-optimized-english"; assert succeeds
# ---------------------------------------------------------------------------


def test_preset_name_accepted():
    """configure-fts accepts the 'recall-optimized-english' preset name."""
    from fathomdb._cli import cli

    runner = CliRunner()

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.configure_fts.return_value = _build_fts_profile()
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            [
                "admin",
                "configure-fts",
                "--db",
                "/tmp/test.db",
                "--kind",
                "Book",
                "--tokenizer",
                "recall-optimized-english",
                "--agree-to-rebuild-impact",
            ],
        )

    assert result.exit_code == 0, result.output


# ---------------------------------------------------------------------------
# 7. test_get_vec_profile_no_profile_message
#    mock get_vec_profile returning None; assert output contains "No vec profile"
# ---------------------------------------------------------------------------


def test_get_vec_profile_no_profile_message():
    """get-vec-profile prints a human-readable message when no profile is set."""
    from fathomdb._cli import cli

    runner = CliRunner()

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.get_vec_profile.return_value = None
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            ["admin", "get-vec-profile", "--db", "/tmp/test.db", "--kind", "Document"],
        )

    assert result.exit_code == 0, result.output
    assert "No vec profile" in result.output


# ---------------------------------------------------------------------------
# Gap 4: _resolve_embedder correctness tests
# ---------------------------------------------------------------------------


def test_resolve_embedder_bge_short_alias_returns_builtin():
    """_resolve_embedder("bge-small-en-v1.5") returns a BuiltinEmbedder instance."""
    from fathomdb._cli import _resolve_embedder
    from fathomdb.embedders import BuiltinEmbedder

    result = _resolve_embedder("bge-small-en-v1.5")
    assert isinstance(result, BuiltinEmbedder)


def test_resolve_embedder_bge_full_alias_returns_builtin():
    """_resolve_embedder("BAAI/bge-small-en-v1.5") returns a BuiltinEmbedder instance."""
    from fathomdb._cli import _resolve_embedder
    from fathomdb.embedders import BuiltinEmbedder

    result = _resolve_embedder("BAAI/bge-small-en-v1.5")
    assert isinstance(result, BuiltinEmbedder)


def test_resolve_embedder_known_model_normalization_policy_is_l2():
    """_resolve_embedder for a known preset uses normalization_policy='l2'."""
    from fathomdb._cli import _resolve_embedder

    embedder = _resolve_embedder("text-embedding-3-small")
    assert embedder.identity().normalization_policy == "l2"


def test_resolve_embedder_unknown_model_normalization_policy_is_l2():
    """_resolve_embedder for an unknown model uses normalization_policy='l2'."""
    from fathomdb._cli import _resolve_embedder

    embedder = _resolve_embedder("unknown-model-xyz")
    assert embedder.identity().normalization_policy == "l2"


def test_resolve_embedder_bge_builtin_identity_matches_rust_constants():
    """BuiltinEmbedder identity matches Rust builtin constants."""
    from fathomdb._cli import _resolve_embedder

    embedder = _resolve_embedder("bge-small-en-v1.5")
    identity = embedder.identity()
    assert identity.model_identity == "BAAI/bge-small-en-v1.5"
    assert identity.model_version == "main"
    assert identity.dimensions == 384
    assert identity.normalization_policy == "l2"


def test_resolve_embedder_known_preset_dimensions():
    """_resolve_embedder returns correct dimensions for known presets."""
    from fathomdb._cli import _resolve_embedder

    assert _resolve_embedder("bge-base-en-v1.5").identity().dimensions == 768
    assert _resolve_embedder("bge-large-en-v1.5").identity().dimensions == 1024
    assert _resolve_embedder("text-embedding-3-small").identity().dimensions == 1536
    assert _resolve_embedder("text-embedding-3-large").identity().dimensions == 3072
    assert _resolve_embedder("jina-embeddings-v2-base-en").identity().dimensions == 768


# ---------------------------------------------------------------------------
# H-C: Operational collection lifecycle CLI (11 commands)
# ---------------------------------------------------------------------------


def _make_collection_record(name="test_col"):
    from fathomdb._types import OperationalCollectionKind, OperationalCollectionRecord

    return OperationalCollectionRecord(
        name=name,
        kind=OperationalCollectionKind.APPEND_ONLY_LOG,
        schema_json="{}",
        retention_json="{}",
        validation_json="",
        secondary_indexes_json="[]",
        format_version=1,
        created_at=1000,
        disabled_at=None,
    )


def test_describe_operational_collection_found():
    """admin describe-operational-collection returns JSON with name and kind."""
    from fathomdb._cli import cli

    runner = CliRunner()
    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.describe_operational_collection.return_value = (
            _make_collection_record("my_col")
        )
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            [
                "admin",
                "describe-operational-collection",
                "--db",
                "/tmp/test.db",
                "--name",
                "my_col",
            ],
        )

    assert result.exit_code == 0, result.output
    data = json.loads(result.output)
    assert data["name"] == "my_col"
    assert "kind" in data


def test_describe_operational_collection_not_found():
    """admin describe-operational-collection prints message when collection missing."""
    from fathomdb._cli import cli

    runner = CliRunner()
    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.describe_operational_collection.return_value = None
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            [
                "admin",
                "describe-operational-collection",
                "--db",
                "/tmp/test.db",
                "--name",
                "missing_col",
            ],
        )

    assert result.exit_code == 0, result.output
    assert "missing_col" in result.output


def test_register_operational_collection():
    """admin register-operational-collection returns JSON with name and kind."""
    from fathomdb._cli import cli

    runner = CliRunner()
    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.register_operational_collection.return_value = (
            _make_collection_record("new_col")
        )
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            [
                "admin",
                "register-operational-collection",
                "--db",
                "/tmp/test.db",
                "--name",
                "new_col",
                "--kind",
                "append_only_log",
                "--schema-json",
                "{}",
                "--retention-json",
                "{}",
            ],
        )

    assert result.exit_code == 0, result.output
    data = json.loads(result.output)
    assert data["name"] == "new_col"
    assert "kind" in data


def test_trace_operational_collection():
    """admin trace-operational-collection returns JSON with collection_name."""
    from fathomdb._cli import cli
    from fathomdb._types import OperationalTraceReport

    runner = CliRunner()
    trace = OperationalTraceReport(
        collection_name="trace_col",
        record_key=None,
        mutation_count=0,
        current_count=0,
        mutations=[],
        current_rows=[],
    )
    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.trace_operational_collection.return_value = trace
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            [
                "admin",
                "trace-operational-collection",
                "--db",
                "/tmp/test.db",
                "--name",
                "trace_col",
            ],
        )

    assert result.exit_code == 0, result.output
    data = json.loads(result.output)
    assert data["collection_name"] == "trace_col"


def test_rebuild_operational_current():
    """admin rebuild-operational-current returns JSON with collections_rebuilt."""
    from fathomdb._cli import cli
    from fathomdb._types import OperationalRepairReport

    runner = CliRunner()
    report = OperationalRepairReport(collections_rebuilt=2, current_rows_rebuilt=5)
    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.rebuild_operational_current.return_value = report
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            [
                "admin",
                "rebuild-operational-current",
                "--db",
                "/tmp/test.db",
            ],
        )

    assert result.exit_code == 0, result.output
    data = json.loads(result.output)
    assert data["collections_rebuilt"] == 2
    assert data["current_rows_rebuilt"] == 5


def test_validate_operational_history():
    """admin validate-operational-history returns JSON with collection_name."""
    from fathomdb._cli import cli
    from fathomdb._types import OperationalHistoryValidationReport

    runner = CliRunner()
    report = OperationalHistoryValidationReport(
        collection_name="hist_col",
        checked_rows=10,
        invalid_row_count=0,
        issues=[],
    )
    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.validate_operational_collection_history.return_value = report
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            [
                "admin",
                "validate-operational-history",
                "--db",
                "/tmp/test.db",
                "--name",
                "hist_col",
            ],
        )

    assert result.exit_code == 0, result.output
    data = json.loads(result.output)
    assert data["collection_name"] == "hist_col"


def test_rebuild_operational_secondary_indexes():
    """admin rebuild-operational-secondary-indexes returns JSON with collection_name."""
    from fathomdb._cli import cli
    from fathomdb._types import OperationalSecondaryIndexRebuildReport

    runner = CliRunner()
    report = OperationalSecondaryIndexRebuildReport(
        collection_name="idx_col",
        mutation_entries_rebuilt=3,
        current_entries_rebuilt=1,
    )
    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.rebuild_operational_secondary_indexes.return_value = report
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            [
                "admin",
                "rebuild-operational-secondary-indexes",
                "--db",
                "/tmp/test.db",
                "--name",
                "idx_col",
            ],
        )

    assert result.exit_code == 0, result.output
    data = json.loads(result.output)
    assert data["collection_name"] == "idx_col"


def test_plan_operational_retention():
    """admin plan-operational-retention returns JSON with planned_at."""
    from fathomdb._cli import cli
    from fathomdb._types import OperationalRetentionPlanReport

    runner = CliRunner()
    report = OperationalRetentionPlanReport(
        planned_at=9999,
        collections_examined=1,
        items=[],
    )
    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.plan_operational_retention.return_value = report
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            [
                "admin",
                "plan-operational-retention",
                "--db",
                "/tmp/test.db",
                "--now",
                "9999",
            ],
        )

    assert result.exit_code == 0, result.output
    data = json.loads(result.output)
    assert data["planned_at"] == 9999


def test_run_operational_retention():
    """admin run-operational-retention returns JSON with executed_at."""
    from fathomdb._cli import cli
    from fathomdb._types import OperationalRetentionRunReport

    runner = CliRunner()
    report = OperationalRetentionRunReport(
        executed_at=8888,
        collections_examined=2,
        collections_acted_on=1,
        dry_run=False,
        items=[],
    )
    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.run_operational_retention.return_value = report
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            [
                "admin",
                "run-operational-retention",
                "--db",
                "/tmp/test.db",
                "--now",
                "8888",
            ],
        )

    assert result.exit_code == 0, result.output
    data = json.loads(result.output)
    assert data["executed_at"] == 8888


def test_compact_operational_collection():
    """admin compact-operational-collection returns JSON with collection_name."""
    from fathomdb._cli import cli
    from fathomdb._types import OperationalCompactionReport

    runner = CliRunner()
    report = OperationalCompactionReport(
        collection_name="compact_col",
        deleted_mutations=0,
        dry_run=True,
        before_timestamp=None,
    )
    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.compact_operational_collection.return_value = report
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            [
                "admin",
                "compact-operational-collection",
                "--db",
                "/tmp/test.db",
                "--name",
                "compact_col",
                "--dry-run",
            ],
        )

    assert result.exit_code == 0, result.output
    data = json.loads(result.output)
    assert data["collection_name"] == "compact_col"


def test_purge_operational_collection():
    """admin purge-operational-collection returns JSON with collection_name."""
    from fathomdb._cli import cli
    from fathomdb._types import OperationalPurgeReport

    runner = CliRunner()
    report = OperationalPurgeReport(
        collection_name="purge_col",
        deleted_mutations=7,
        before_timestamp=5000,
    )
    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.purge_operational_collection.return_value = report
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            [
                "admin",
                "purge-operational-collection",
                "--db",
                "/tmp/test.db",
                "--name",
                "purge_col",
                "--before-timestamp",
                "5000",
            ],
        )

    assert result.exit_code == 0, result.output
    data = json.loads(result.output)
    assert data["collection_name"] == "purge_col"
    assert data["deleted_mutations"] == 7


def test_disable_operational_collection():
    """admin disable-operational-collection returns JSON with name and kind."""
    from fathomdb._cli import cli

    runner = CliRunner()
    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.disable_operational_collection.return_value = (
            _make_collection_record("dis_col")
        )
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            [
                "admin",
                "disable-operational-collection",
                "--db",
                "/tmp/test.db",
                "--name",
                "dis_col",
            ],
        )

    assert result.exit_code == 0, result.output
    data = json.loads(result.output)
    assert data["name"] == "dis_col"
    assert "kind" in data
