"""EU-6 — Python binding surface for `use_default_embedder` + EU-5b
`OpenReport` fields.

Per `dev/plans/prompts/0.7.1-EMBEDDER-UNDEFER-HANDOFF.md` §EU-6, the
Python binding exposes the EU-5b binary opt-in selector via the
`Engine.open(path, use_default_embedder=...)` kwarg:

- `True` → `EmbedderChoice::Default` (engine materialises the pinned
  bge-small embedder via the EU-3 loader; weights fetched from HF on
  first use).
- `False` (default) → `EmbedderChoice::None` (no embedder; vector
  writes fail with `EmbedderNotConfiguredError`).

These tests also assert that the four EU-5a1/5a2/5b `OpenReport` fields
round-trip through the binding:

- `embedder_download_ms`
- `embedder_events`
- `embedder_mean_centering_required`
- `embedder_mean_vec_pinned`

Network-hitting tests honour `FATHOMDB_SKIP_NETWORK_TESTS` per EU-5c.
"""

from __future__ import annotations

import os

import pytest

from fathomdb import Engine
from fathomdb._fathomdb import Engine as _NativeEngine

# Threshold mirrors `fathomdb_engine::MEAN_VEC_PIN_THRESHOLD` (256).
_MEAN_VEC_PIN_THRESHOLD = 256


def _skip_if_no_network() -> None:
    if os.environ.get("FATHOMDB_SKIP_NETWORK_TESTS"):
        pytest.skip("FATHOMDB_SKIP_NETWORK_TESTS set; skipping network-hitting test")


def test_open_default_embedder_succeeds(db_path: str) -> None:
    _skip_if_no_network()
    engine = Engine.open(db_path, use_default_embedder=True)
    try:
        report = engine.open_report()
        assert report.default_embedder.name == "fathomdb-bge-small-en-v1.5"
        assert report.default_embedder.dimension == 384
    finally:
        engine.close()


def test_open_default_embedder_false_does_not_download(db_path: str) -> None:
    engine = Engine.open(db_path, use_default_embedder=False)
    try:
        report = engine.open_report()
        # No download attempted; no DefaultEmbedderDownload event.
        assert report.embedder_download_ms is None
        for event in report.embedder_events:
            assert event["kind"] != "DefaultEmbedderDownload"
    finally:
        engine.close()


def test_open_default_embedder_default_kwarg_is_false(db_path: str) -> None:
    """Omitting the kwarg must behave like `False` — no network."""

    engine = Engine.open(db_path)
    try:
        report = engine.open_report()
        assert report.embedder_download_ms is None
        for event in report.embedder_events:
            assert event["kind"] != "DefaultEmbedderDownload"
    finally:
        engine.close()


def test_open_report_carries_mean_centering_required(db_path: str, tmp_path) -> None:
    """Workspace identity is bge-small (EU-5b lock-flip), so the static
    capability flag is ``True`` regardless of whether the embedder is
    materialised. Both the ``False`` and ``True`` kwarg paths must
    surface the field — that's the EU-6 binding-coverage point."""

    engine_false = Engine.open(db_path, use_default_embedder=False)
    try:
        report_false = engine_false.open_report()
        # Field type round-trips as a bool through the binding.
        assert isinstance(report_false.embedder_mean_centering_required, bool)
        # Per EU-5b lock-flip — workspace identity is bge-small.
        assert report_false.embedder_mean_centering_required is True
    finally:
        engine_false.close()

    _skip_if_no_network()
    path_true = str(tmp_path / "mc_true.sqlite")
    engine_true = Engine.open(path_true, use_default_embedder=True)
    try:
        report_true = engine_true.open_report()
        assert report_true.embedder_mean_centering_required is True
    finally:
        engine_true.close()


def test_open_report_carries_mean_vec_pinned_initial_state(db_path: str) -> None:
    """Fresh workspace → mean_vec_pinned is False (no writes yet)."""

    _skip_if_no_network()
    engine = Engine.open(db_path, use_default_embedder=True)
    try:
        report = engine.open_report()
        assert report.embedder_mean_vec_pinned is False
    finally:
        engine.close()


def test_open_report_mean_vec_pinned_transitions_after_threshold(
    db_path: str,
) -> None:
    """After 256+ vector writes + reopen, mean_vec_pinned becomes True.

    Uses the `test-hooks`-gated native seam `_write_vector_for_test`
    because the public Python surface does not yet expose typed vector
    writes (deferred to a later slice). The seam exists solely so the
    cross-binding pin-transition witness is observable end-to-end through
    the binding.
    """

    _skip_if_no_network()
    engine = Engine.open(db_path, use_default_embedder=True)
    try:
        native: _NativeEngine = engine._native  # type: ignore[attr-defined]
        native._configure_vector_kind_for_test("doc")
        for i in range(_MEAN_VEC_PIN_THRESHOLD):
            native._write_vector_for_test("doc", f"doc-{i}")
    finally:
        engine.close()

    engine2 = Engine.open(db_path, use_default_embedder=True)
    try:
        report = engine2.open_report()
        assert report.embedder_mean_vec_pinned is True
    finally:
        engine2.close()
