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
# H-3: check-integrity and check-semantics
# ---------------------------------------------------------------------------


def _build_integrity_report():
    from fathomdb._types import IntegrityReport

    return IntegrityReport.from_wire(
        {
            "physical_ok": True,
            "foreign_keys_ok": True,
            "missing_fts_rows": 0,
            "missing_property_fts_rows": 0,
            "duplicate_active_logical_ids": 0,
            "operational_missing_collections": 0,
            "operational_missing_last_mutations": 0,
            "warnings": [],
        }
    )


def _build_semantic_report():
    from fathomdb._types import SemanticReport

    return SemanticReport.from_wire(
        {
            "orphaned_chunks": 0,
            "null_source_ref_nodes": 0,
            "broken_step_fk": 0,
            "broken_action_fk": 0,
            "stale_fts_rows": 0,
            "fts_rows_for_superseded_nodes": 0,
            "stale_property_fts_rows": 0,
            "orphaned_property_fts_rows": 0,
            "mismatched_kind_property_fts_rows": 0,
            "duplicate_property_fts_rows": 0,
            "drifted_property_fts_rows": 0,
            "dangling_edges": 0,
            "orphaned_supersession_chains": 0,
            "stale_vec_rows": 0,
            "vec_rows_for_superseded_nodes": 0,
            "missing_operational_current_rows": 0,
            "stale_operational_current_rows": 0,
            "disabled_collection_mutations": 0,
            "orphaned_last_access_metadata_rows": 0,
            "warnings": [],
        }
    )


def test_check_integrity_exits_zero_and_prints_json():
    """check-integrity exits 0 and prints JSON with physical_ok field."""
    from fathomdb._cli import cli

    runner = CliRunner()

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.check_integrity.return_value = _build_integrity_report()
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            ["admin", "check-integrity", "--db", "/tmp/test.db"],
        )

    assert result.exit_code == 0, result.output
    data = json.loads(result.output)
    assert "physical_ok" in data
    assert data["physical_ok"] is True


def test_check_semantics_exits_zero_and_prints_json():
    """check-semantics exits 0 and prints JSON with orphaned_chunks field."""
    from fathomdb._cli import cli

    runner = CliRunner()

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.check_semantics.return_value = _build_semantic_report()
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            ["admin", "check-semantics", "--db", "/tmp/test.db"],
        )

    assert result.exit_code == 0, result.output
    data = json.loads(result.output)
    assert "orphaned_chunks" in data
    assert data["orphaned_chunks"] == 0


# ---------------------------------------------------------------------------
# H-1: rebuild
# ---------------------------------------------------------------------------


def _build_projection_repair_report():
    from fathomdb._types import ProjectionRepairReport

    return ProjectionRepairReport.from_wire(
        {
            "targets": ["fts", "vec"],
            "rebuilt_rows": 42,
            "notes": [],
        }
    )


def test_rebuild_all_exits_zero_and_prints_json():
    """rebuild --target all exits 0 and prints JSON with rebuilt_rows field."""
    from fathomdb._cli import cli

    runner = CliRunner()

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.rebuild.return_value = _build_projection_repair_report()
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            ["admin", "rebuild", "--db", "/tmp/test.db", "--target", "all"],
        )

    assert result.exit_code == 0, result.output
    data = json.loads(result.output)
    assert "rebuilt_rows" in data
    assert data["rebuilt_rows"] == 42


def test_rebuild_default_target_is_all():
    """rebuild without --target defaults to 'all' and succeeds."""
    from fathomdb._cli import cli

    runner = CliRunner()

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.rebuild.return_value = _build_projection_repair_report()
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            ["admin", "rebuild", "--db", "/tmp/test.db"],
        )

    assert result.exit_code == 0, result.output
    mock_engine.admin.rebuild.assert_called_once_with("all")


def test_rebuild_fts_target():
    """rebuild --target fts passes 'fts' to admin.rebuild."""
    from fathomdb._cli import cli

    runner = CliRunner()

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.rebuild.return_value = _build_projection_repair_report()
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            ["admin", "rebuild", "--db", "/tmp/test.db", "--target", "fts"],
        )

    assert result.exit_code == 0, result.output
    mock_engine.admin.rebuild.assert_called_once_with("fts")


# ---------------------------------------------------------------------------
# H-2: FTS property schema CRUD
# ---------------------------------------------------------------------------


def _build_fts_property_schema_record():
    from fathomdb._types import FtsPropertySchemaRecord

    return FtsPropertySchemaRecord.from_wire(
        {
            "kind": "Article",
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


def test_register_fts_property_schema_exits_zero_and_prints_json():
    """register-fts-property-schema exits 0 and prints JSON with kind field."""
    from fathomdb._cli import cli

    runner = CliRunner()

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.register_fts_property_schema.return_value = (
            _build_fts_property_schema_record()
        )
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            [
                "admin",
                "register-fts-property-schema",
                "--db",
                "/tmp/test.db",
                "--kind",
                "Article",
                "--paths",
                "$.title",
                "--paths",
                "$.body",
            ],
        )

    assert result.exit_code == 0, result.output
    data = json.loads(result.output)
    assert data["kind"] == "Article"
    assert "$.title" in data["property_paths"]


def test_describe_fts_property_schema_returns_json():
    """describe-fts-property-schema prints JSON when schema exists."""
    from fathomdb._cli import cli

    runner = CliRunner()

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.describe_fts_property_schema.return_value = (
            _build_fts_property_schema_record()
        )
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            [
                "admin",
                "describe-fts-property-schema",
                "--db",
                "/tmp/test.db",
                "--kind",
                "Article",
            ],
        )

    assert result.exit_code == 0, result.output
    data = json.loads(result.output)
    assert data["kind"] == "Article"


def test_describe_fts_property_schema_none_prints_message():
    """describe-fts-property-schema prints a message when schema does not exist."""
    from fathomdb._cli import cli

    runner = CliRunner()

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.describe_fts_property_schema.return_value = None
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            [
                "admin",
                "describe-fts-property-schema",
                "--db",
                "/tmp/test.db",
                "--kind",
                "Missing",
            ],
        )

    assert result.exit_code == 0, result.output
    assert "Missing" in result.output
    assert "No FTS property schema" in result.output


def test_list_fts_property_schemas_exits_zero_and_prints_json():
    """list-fts-property-schemas exits 0 and prints a JSON list."""
    from fathomdb._cli import cli

    runner = CliRunner()

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.list_fts_property_schemas.return_value = [
            _build_fts_property_schema_record()
        ]
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            ["admin", "list-fts-property-schemas", "--db", "/tmp/test.db"],
        )

    assert result.exit_code == 0, result.output
    data = json.loads(result.output)
    assert isinstance(data, list)
    assert len(data) == 1
    assert data[0]["kind"] == "Article"


def test_remove_fts_property_schema_exits_zero_and_prints_success():
    """remove-fts-property-schema exits 0 and prints a success message."""
    from fathomdb._cli import cli

    runner = CliRunner()

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.remove_fts_property_schema.return_value = None
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            [
                "admin",
                "remove-fts-property-schema",
                "--db",
                "/tmp/test.db",
                "--kind",
                "Article",
            ],
        )

    assert result.exit_code == 0, result.output
    assert "Article" in result.output or "removed" in result.output.lower()


# ---------------------------------------------------------------------------
# H-B commands: restore-vector-profiles, regen-vectors, trace-source,
# excise-source, restore-logical-id, purge-logical-id, safe-export
# ---------------------------------------------------------------------------


def _build_projection_repair_report():
    from fathomdb._types import ProjectionRepairReport, ProjectionTarget

    return ProjectionRepairReport(
        targets=[ProjectionTarget.VEC],
        rebuilt_rows=42,
        notes=["ok"],
    )


def _build_trace_report():
    from fathomdb._types import TraceReport

    return TraceReport(
        source_ref="src://test",
        node_rows=3,
        edge_rows=1,
        action_rows=0,
        operational_mutation_rows=0,
        node_logical_ids=["lid1"],
        action_ids=[],
        operational_mutation_ids=[],
    )


def _build_logical_restore_report():
    from fathomdb._types import LogicalRestoreReport

    return LogicalRestoreReport(
        logical_id="lid-abc",
        was_noop=False,
        restored_node_rows=2,
        restored_edge_rows=0,
        restored_chunk_rows=0,
        restored_fts_rows=0,
        restored_property_fts_rows=0,
        restored_vec_rows=0,
        skipped_edges=[],
    )


def _build_logical_purge_report():
    from fathomdb._types import LogicalPurgeReport

    return LogicalPurgeReport(
        logical_id="lid-abc",
        was_noop=False,
        deleted_node_rows=5,
        deleted_edge_rows=1,
        deleted_chunk_rows=0,
        deleted_fts_rows=0,
        deleted_vec_rows=0,
    )


def _build_safe_export_manifest():
    from fathomdb._types import SafeExportManifest

    return SafeExportManifest(
        exported_at=1700000000,
        sha256="abcdef1234567890",
        schema_version=3,
        protocol_version=1,
        page_count=100,
    )


def _build_vector_regen_report():
    from fathomdb._types import VectorRegenerationReport

    return VectorRegenerationReport(
        profile="main",
        table_name="vec_Document",
        dimension=384,
        total_chunks=10,
        regenerated_rows=10,
        contract_persisted=True,
        notes=[],
    )


def test_restore_vector_profiles_prints_report():
    """restore-vector-profiles calls admin.restore_vector_profiles and prints JSON."""
    from fathomdb._cli import cli

    runner = CliRunner()

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.restore_vector_profiles.return_value = (
            _build_projection_repair_report()
        )
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            ["admin", "restore-vector-profiles", "--db", "/tmp/test.db"],
        )

    assert result.exit_code == 0, result.output
    data = json.loads(result.output)
    assert data["rebuilt_rows"] == 42
    assert "vec" in data["targets"]


def test_regen_vectors_calls_admin_regen():
    """regen-vectors opens engine with embedder string and calls admin.regenerate_vector_embeddings."""
    from fathomdb._cli import cli

    runner = CliRunner()

    with patch("fathomdb.Engine.open") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.regenerate_vector_embeddings.return_value = (
            _build_vector_regen_report()
        )
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            [
                "admin", "regen-vectors",
                "--db", "/tmp/test.db",
                "--embedder", "bge-small-en-v1.5",
                "--kind", "Document",
            ],
        )

    assert result.exit_code == 0, result.output
    data = json.loads(result.output)
    assert data["regenerated_rows"] == 10


def test_trace_source_prints_report():
    """trace-source calls admin.trace_source and prints JSON."""
    from fathomdb._cli import cli

    runner = CliRunner()

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.trace_source.return_value = _build_trace_report()
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            ["admin", "trace-source", "--db", "/tmp/test.db", "--source-ref", "src://test"],
        )

    assert result.exit_code == 0, result.output
    data = json.loads(result.output)
    assert data["source_ref"] == "src://test"
    assert data["node_rows"] == 3


def test_excise_source_prints_report():
    """excise-source calls admin.excise_source and prints JSON."""
    from fathomdb._cli import cli

    runner = CliRunner()

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.excise_source.return_value = _build_trace_report()
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            ["admin", "excise-source", "--db", "/tmp/test.db", "--source-ref", "src://test"],
        )

    assert result.exit_code == 0, result.output
    data = json.loads(result.output)
    assert data["source_ref"] == "src://test"
    assert data["node_rows"] == 3


def test_restore_logical_id_prints_report():
    """restore-logical-id calls admin.restore_logical_id and prints JSON."""
    from fathomdb._cli import cli

    runner = CliRunner()

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.restore_logical_id.return_value = _build_logical_restore_report()
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            ["admin", "restore-logical-id", "--db", "/tmp/test.db", "--logical-id", "lid-abc"],
        )

    assert result.exit_code == 0, result.output
    data = json.loads(result.output)
    assert data["logical_id"] == "lid-abc"
    assert data["restored_node_rows"] == 2


def test_purge_logical_id_prints_report():
    """purge-logical-id calls admin.purge_logical_id and prints JSON."""
    from fathomdb._cli import cli

    runner = CliRunner()

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.purge_logical_id.return_value = _build_logical_purge_report()
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            ["admin", "purge-logical-id", "--db", "/tmp/test.db", "--logical-id", "lid-abc"],
        )

    assert result.exit_code == 0, result.output
    data = json.loads(result.output)
    assert data["logical_id"] == "lid-abc"
    assert data["deleted_node_rows"] == 5


def test_safe_export_prints_manifest():
    """safe-export calls admin.safe_export and prints JSON."""
    from fathomdb._cli import cli

    runner = CliRunner()

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.safe_export.return_value = _build_safe_export_manifest()
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            ["admin", "safe-export", "--db", "/tmp/test.db", "--destination", "/tmp/export.db"],
        )

    assert result.exit_code == 0, result.output
    data = json.loads(result.output)
    assert data["sha256"] == "abcdef1234567890"
    assert data["page_count"] == 100
