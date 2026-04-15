"""Tests for Pack F: Admin CLI (configure-fts, configure-vec, preview-impact, get-*-profile)."""

from __future__ import annotations

import json
from unittest.mock import MagicMock, patch

import pytest
from click.testing import CliRunner


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _make_impact_json(rows: int) -> str:
    return json.dumps(
        {
            "rows_to_rebuild": rows,
            "estimated_seconds": 0,
            "temp_db_size_bytes": 0,
            "current_tokenizer": "unicode61",
            "target_tokenizer": "porter unicode61 remove_diacritics 2",
        }
    )


def _make_fts_profile_json() -> str:
    return json.dumps({"kind": "Book", "tokenizer": "porter unicode61 remove_diacritics 2"})


def _make_vec_profile_json() -> str:
    return json.dumps(
        {
            "model_identity": "bge-small-en-v1.5",
            "model_version": "1.5",
            "dimensions": 384,
            "normalization_policy": "l2",
        }
    )


# ---------------------------------------------------------------------------
# 1. test_abort_without_flag_when_rows_gt_0
#    mock preview rows=5; run configure-fts without --agree flag;
#    assert exit != 0 or output contains abort/cancelled
# ---------------------------------------------------------------------------


def test_abort_without_flag_when_rows_gt_0():
    """configure-fts without --agree-to-rebuild-impact aborts when rows > 0 in CI."""
    from fathomdb._cli import cli

    runner = CliRunner(mix_stderr=False)

    mock_core = MagicMock()
    mock_core.preview_projection_impact.return_value = _make_impact_json(5)
    mock_core.set_fts_profile.return_value = _make_fts_profile_json()
    mock_admin = MagicMock()

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.preview_projection_impact.return_value = _build_impact(5)
        mock_engine.admin.configure_fts.side_effect = _impact_error(5)
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            ["admin", "configure-fts", "--db", "/tmp/test.db", "--kind", "Book",
             "--tokenizer", "unicode61"],
            # no --agree-to-rebuild-impact, input="" simulates non-tty/CI
            input="",
        )

    # Either non-zero exit or output mentions abort/cancel
    assert result.exit_code != 0 or any(
        kw in (result.output or "").lower() for kw in ("abort", "cancel", "impact")
    )


# ---------------------------------------------------------------------------
# 2. test_succeed_with_agree_flag
#    mock preview rows=5; run with --agree-to-rebuild-impact; assert exit 0
# ---------------------------------------------------------------------------


def test_succeed_with_agree_flag():
    """configure-fts with --agree-to-rebuild-impact exits 0 even when rows > 0."""
    from fathomdb._cli import cli

    runner = CliRunner(mix_stderr=False)

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.configure_fts.return_value = _build_fts_profile()
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            [
                "admin", "configure-fts",
                "--db", "/tmp/test.db",
                "--kind", "Book",
                "--tokenizer", "unicode61",
                "--agree-to-rebuild-impact",
            ],
        )

    assert result.exit_code == 0, result.output


# ---------------------------------------------------------------------------
# 3. test_no_prompt_on_zero_rows
#    mock preview rows=0; run configure-fts without flag; assert exit 0
# ---------------------------------------------------------------------------


def test_no_prompt_on_zero_rows():
    """configure-fts without flag exits 0 when rows_to_rebuild == 0."""
    from fathomdb._cli import cli

    runner = CliRunner(mix_stderr=False)

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.preview_projection_impact.return_value = _build_impact(0)
        mock_engine.admin.configure_fts.return_value = _build_fts_profile()
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            ["admin", "configure-fts", "--db", "/tmp/test.db", "--kind", "Book",
             "--tokenizer", "unicode61"],
        )

    assert result.exit_code == 0, result.output


# ---------------------------------------------------------------------------
# 4. test_preview_prints_report
#    run preview-impact; assert output contains "rows_to_rebuild" or the count
# ---------------------------------------------------------------------------


def test_preview_prints_report():
    """preview-impact prints ImpactReport fields to stdout."""
    from fathomdb._cli import cli

    runner = CliRunner(mix_stderr=False)

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.preview_projection_impact.return_value = _build_impact(3)
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            ["admin", "preview-impact", "--db", "/tmp/test.db", "--kind", "Book",
             "--target", "fts"],
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

    runner = CliRunner(mix_stderr=False)

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.get_fts_profile.return_value = None
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            ["admin", "get-fts-profile", "--db", "/tmp/test.db", "--kind", "Book"],
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

    runner = CliRunner(mix_stderr=False)

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.configure_fts.return_value = _build_fts_profile()
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            [
                "admin", "configure-fts",
                "--db", "/tmp/test.db",
                "--kind", "Book",
                "--tokenizer", "recall-optimized-english",
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

    runner = CliRunner(mix_stderr=False)

    with patch("fathomdb._cli._open_engine") as mock_open:
        mock_engine = MagicMock()
        mock_engine.admin.get_vec_profile.return_value = None
        mock_open.return_value = mock_engine

        result = runner.invoke(
            cli,
            ["admin", "get-vec-profile", "--db", "/tmp/test.db"],
        )

    assert result.exit_code == 0, result.output
    assert "No vec profile" in result.output


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

    return FtsProfile.from_wire({"kind": "Book", "tokenizer": "unicode61"})


def _impact_error(rows: int):
    from fathomdb import RebuildImpactError

    return RebuildImpactError(_build_impact(rows))
