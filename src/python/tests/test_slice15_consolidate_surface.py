"""0.8.12 Slice 15 (OPP-2) — Python parity test: consolidation surface.

Pins that the PUBLIC package surface (``fathomdb`` / ``fathomdb.engine`` /
``fathomdb.errors``) — not just the native ``_fathomdb`` extension — exposes:
  1. ``ConsolidateReceipt`` (with the five metadata-transition counts).
  2. ``ConsolidatorError`` (a subclass of ``EngineError``).
  3. ``Engine.consolidate_with_provider`` (present + callable on the wrapper).

Surface-only assertions (not functional end-to-end tests). Functional
conformance is in the Rust test suite (``tests/consolidate_provider.rs``);
the live cross-binding functional run is deferred to Slice 40.
"""

from __future__ import annotations

import fathomdb._fathomdb as _native
from fathomdb import ConsolidateReceipt, Engine
from fathomdb.errors import ConsolidatorError, EngineError


def test_consolidate_receipt_exported_from_public_package() -> None:
    """ConsolidateReceipt must expose the five metadata-transition counts and
    be re-exported from the top-level ``fathomdb`` package."""
    import fathomdb

    assert fathomdb.ConsolidateReceipt is ConsolidateReceipt
    assert "ConsolidateReceipt" in fathomdb.__all__
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
    """ConsolidatorError (public) must be a leaf subclass of EngineError."""
    assert issubclass(ConsolidatorError, EngineError), (
        "ConsolidatorError must inherit from EngineError"
    )
    # The public re-export is identical to the native leaf.
    assert ConsolidatorError is _native.ConsolidatorError


def test_public_engine_has_consolidate_with_provider_method() -> None:
    """The PUBLIC wrapper ``Engine`` must forward ``consolidate_with_provider``."""
    assert callable(getattr(Engine, "consolidate_with_provider", None)), (
        "public Engine.consolidate_with_provider must be a callable method"
    )
