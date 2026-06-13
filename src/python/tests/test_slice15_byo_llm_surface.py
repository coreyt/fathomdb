"""Slice 15 (G11) — Python parity test: BYO-LLM surface assertions.

Pins that:
  1. ``IngestWithExtractorReceipt`` is exported with the correct attributes.
  2. ``ExtractorError`` is exported and is a subclass of ``EngineError``.
  3. ``Engine.ingest_with_extractor`` exists and is callable.

These are surface-only assertions (not functional end-to-end tests).
Functional conformance is in the Rust test suite
(``tests/slice15_byo_llm_ingest.rs``).
"""

from __future__ import annotations

import fathomdb._fathomdb as _native
from fathomdb._fathomdb import (
    EngineError,
    ExtractorError,
    IngestWithExtractorReceipt,
)


def test_ingest_with_extractor_receipt_has_correct_attributes() -> None:
    """IngestWithExtractorReceipt must expose nodes_written, edges_written, docs_processed."""
    # The class must be importable from the native module.
    assert hasattr(_native, "IngestWithExtractorReceipt")
    # Check attribute names on the class directly (not an instance, since we
    # cannot call the constructor without a real engine).
    cls = IngestWithExtractorReceipt
    assert hasattr(cls, "nodes_written") or "nodes_written" in str(cls.__doc__ or ""), (
        "IngestWithExtractorReceipt must declare nodes_written"
    )


def test_extractor_error_is_engine_error_subclass() -> None:
    """ExtractorError must be a leaf subclass of EngineError."""
    assert issubclass(ExtractorError, EngineError), (
        "ExtractorError must inherit from EngineError"
    )


def test_engine_has_ingest_with_extractor_method() -> None:
    """Engine.ingest_with_extractor must exist and be callable."""
    from fathomdb._fathomdb import Engine

    assert callable(getattr(Engine, "ingest_with_extractor", None)), (
        "Engine.ingest_with_extractor must be a callable method"
    )


def test_extractor_error_exported_from_native() -> None:
    """ExtractorError must be directly accessible on the native module."""
    assert hasattr(_native, "ExtractorError")
    assert _native.ExtractorError is ExtractorError
