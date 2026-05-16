"""AC-060a — typed error payload coverage.

The engine returns enum variants with typed fields; the binding
translator (`engine_error_to_py` / `engine_open_error_to_py`) attaches
those fields as Python attributes on the raised exception so callers
can dispatch and inspect without parsing message text.
"""

from __future__ import annotations

import pytest

from fathomdb import Engine
from fathomdb.errors import (
    CorruptionError,
    DatabaseLockedError,
    EmbedderDimensionMismatchError,
    EmbedderError,
    EmbedderNotConfiguredError,
    EngineError,
    KindNotVectorIndexedError,
    VectorError,
)


def test_database_locked_error_attr_round_trip() -> None:
    err = DatabaseLockedError(holder_pid=12345)
    assert err.holder_pid == 12345


def test_database_locked_engine_triggered(db_path: str) -> None:
    """Opening the same database twice in one process must surface
    `DatabaseLockedError` with the `holder_pid` attribute populated
    (or `None` when the lockfile is unparseable)."""

    a = Engine.open(db_path)
    try:
        with pytest.raises(DatabaseLockedError) as excinfo:
            Engine.open(db_path)
        assert hasattr(excinfo.value, "holder_pid")
        assert excinfo.value.holder_pid is None or isinstance(
            excinfo.value.holder_pid, int
        )
    finally:
        a.close()


def test_corruption_error_attrs_round_trip() -> None:
    err = CorruptionError(
        kind="HeaderMalformed",
        stage="HeaderProbe",
        recovery_hint_code="E_CORRUPT_HEADER",
        doc_anchor="design/recovery.md#header-malformed",
    )
    assert err.kind == "HeaderMalformed"
    assert err.stage == "HeaderProbe"
    assert err.recovery_hint_code == "E_CORRUPT_HEADER"
    assert err.doc_anchor == "design/recovery.md#header-malformed"


def test_embedder_not_configured_is_distinct_leaf_under_embedder_error() -> None:
    err = EmbedderNotConfiguredError("no embedder")
    assert isinstance(err, EmbedderNotConfiguredError)
    assert isinstance(err, EmbedderError)
    assert isinstance(err, EngineError)
    assert EmbedderNotConfiguredError is not EmbedderError


def test_kind_not_vector_indexed_is_distinct_leaf_under_vector_error() -> None:
    err = KindNotVectorIndexedError("kind X not vector indexed")
    assert isinstance(err, KindNotVectorIndexedError)
    assert isinstance(err, VectorError)
    assert isinstance(err, EngineError)
    assert KindNotVectorIndexedError is not VectorError


def test_embedder_dimension_mismatch_attrs_round_trip() -> None:
    err = EmbedderDimensionMismatchError(stored=384, supplied=768)
    assert err.stored == 384
    assert err.supplied == 768
    assert isinstance(err.stored, int)
    assert isinstance(err.supplied, int)
