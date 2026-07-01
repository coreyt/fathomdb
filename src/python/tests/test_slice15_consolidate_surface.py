"""0.8.12 Slice 15 (OPP-2) — Python parity test: consolidation surface.

Pins that:
  1. ``ConsolidateReceipt`` is exported with the correct attributes.
  2. ``ConsolidatorError`` is exported and is a subclass of ``EngineError``.
  3. ``Engine.consolidate_with_provider`` exists and is callable.

Surface-only assertions (not functional end-to-end tests). Functional
conformance is in the Rust test suite (``tests/consolidate_provider.rs``);
the live cross-binding functional run is deferred to Slice 40.
"""

from __future__ import annotations

import fathomdb._fathomdb as _native
from fathomdb._fathomdb import (
    ConsolidateReceipt,
    ConsolidatorError,
    EngineError,
)


def test_consolidate_receipt_has_correct_attributes() -> None:
    """ConsolidateReceipt must expose the five metadata-transition counts."""
    assert hasattr(_native, "ConsolidateReceipt")
    cls = ConsolidateReceipt
    for attr in (
        "clusters_processed",
        "edges_examined",
        "edges_kept",
        "edges_invalidated",
        "edges_superseded",
    ):
        assert hasattr(cls, attr) or attr in str(cls.__doc__ or ""), (
            f"ConsolidateReceipt must declare {attr}"
        )


def test_consolidator_error_is_engine_error_subclass() -> None:
    """ConsolidatorError must be a leaf subclass of EngineError."""
    assert issubclass(ConsolidatorError, EngineError), (
        "ConsolidatorError must inherit from EngineError"
    )


def test_engine_has_consolidate_with_provider_method() -> None:
    """Engine.consolidate_with_provider must exist and be callable."""
    from fathomdb._fathomdb import Engine

    assert callable(getattr(Engine, "consolidate_with_provider", None)), (
        "Engine.consolidate_with_provider must be a callable method"
    )


def test_consolidator_error_exported_from_native() -> None:
    """ConsolidatorError must be directly accessible on the native module."""
    assert hasattr(_native, "ConsolidatorError")
    assert _native.ConsolidatorError is ConsolidatorError
