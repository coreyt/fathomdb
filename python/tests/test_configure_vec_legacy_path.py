"""Regression test: deprecated ``admin.configure_vec(embedder)`` must not hang.

The legacy embedder form of ``configure_vec`` is kept as a
backwards-compat shim that emits ``DeprecationWarning``. Memex reported
that on schema v25 the shim hung (presumably inside
``preview_projection_impact('*', 'vec')`` / ``set_vec_profile``).

This test runs the legacy path against a real engine and requires it to
return or raise cleanly within a bounded wall-clock budget.
"""

from __future__ import annotations

import threading
import warnings
from pathlib import Path

from fathomdb import Engine


class _Identity:
    model_identity = "test/model"
    model_version = "v1"
    dimensions = 384
    normalization_policy = "l2"


class _DummyEmbedder:
    def identity(self) -> _Identity:
        return _Identity()

    def max_tokens(self) -> int:
        return 512


def _run_with_timeout(fn, timeout_s: float):
    """Run *fn* in a thread; return (result, exc, timed_out)."""
    result: list = [None]
    exc: list[BaseException | None] = [None]

    def _target() -> None:
        try:
            result[0] = fn()
        except BaseException as e:  # noqa: BLE001
            exc[0] = e

    t = threading.Thread(target=_target, daemon=True)
    t.start()
    t.join(timeout_s)
    return result[0], exc[0], t.is_alive()


def test_configure_vec_deprecated_embedder_form_does_not_hang(
    tmp_path: Path,
) -> None:
    """Deprecated ``configure_vec(embedder)`` must return or raise within 5s."""
    engine = Engine.open(tmp_path / "legacy.db")
    try:

        def _call() -> object:
            with warnings.catch_warnings():
                warnings.simplefilter("ignore", DeprecationWarning)
                return engine.admin.configure_vec(
                    _DummyEmbedder(), agree_to_rebuild_impact=True
                )

        result, exc, timed_out = _run_with_timeout(_call, timeout_s=5.0)
        assert not timed_out, (
            "legacy configure_vec(embedder) hung for >5s — Memex regression"
        )
        if exc is not None:
            # Raising a clear error is acceptable; hanging is not.
            assert not isinstance(exc, KeyboardInterrupt)
        else:
            assert result is not None
    finally:
        engine.close()


def test_configure_vec_deprecated_embedder_form_emits_warning(
    tmp_path: Path,
) -> None:
    """The deprecated shape must still emit DeprecationWarning (contract)."""
    engine = Engine.open(tmp_path / "legacy2.db")
    try:
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            try:
                engine.admin.configure_vec(
                    _DummyEmbedder(), agree_to_rebuild_impact=True
                )
            except Exception:
                # If the fix routes to an error, we still want the warning.
                pass
        assert any(issubclass(w.category, DeprecationWarning) for w in caught), (
            f"expected DeprecationWarning, got {[w.category for w in caught]}"
        )
    finally:
        engine.close()
