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
