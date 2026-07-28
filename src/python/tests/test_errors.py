"""Exception hierarchy assertions for the Python SDK.

Pins the single rooted hierarchy beneath `EngineError` and the leaf-class
matrix from `dev/design/errors.md`. Per ADR-0.6.0-error-taxonomy and
`dev/design/bindings.md` § 3, callers dispatch on `except <Specific>` not on
message text; this test file uses `issubclass` to enforce the hierarchy.
"""

from __future__ import annotations

import pytest

from fathomdb import Engine
from fathomdb.errors import (
    ClosingError,
    CorruptionError,
    DatabaseLockedError,
    EmbedderDimensionMismatchError,
    EmbedderError,
    EmbedderIdentityMismatchError,
    EmbedderNotConfiguredError,
    EngineError,
    IncompatibleSchemaVersionError,
    InvalidArgumentError,
    KindNotVectorIndexedError,
    MigrationError,
    OpStoreError,
    OverloadedError,
    ProjectionError,
    SchedulerError,
    SchemaValidationError,
    StorageError,
    VectorError,
    WriteValidationError,
)

LEAF_CLASSES = [
    StorageError,
    ProjectionError,
    VectorError,
    EmbedderError,
    SchedulerError,
    OpStoreError,
    WriteValidationError,
    SchemaValidationError,
    OverloadedError,
    ClosingError,
    DatabaseLockedError,
    CorruptionError,
    IncompatibleSchemaVersionError,
    MigrationError,
    EmbedderIdentityMismatchError,
    EmbedderDimensionMismatchError,
    EmbedderNotConfiguredError,
    KindNotVectorIndexedError,
    # 0.8.20 Slice 22 / decision #18 — `InvalidArgumentError` was absent from both
    # tables in `dev/design/errors.md` despite being a live SDK class. The
    # settlement adds it to the taxonomy of record; this row pins it here too.
    InvalidArgumentError,
]


@pytest.mark.parametrize("cls", LEAF_CLASSES)
def test_every_leaf_extends_engine_error(cls: type[EngineError]) -> None:
    assert issubclass(cls, EngineError)
    assert cls is not EngineError


def test_engine_error_is_the_single_root() -> None:
    for cls in LEAF_CLASSES:
        bases = {b for b in cls.__mro__ if b not in (cls, object, BaseException, Exception)}
        assert EngineError in bases, f"{cls.__name__} must descend from EngineError"


def test_corruption_error_carries_typed_recovery_hint() -> None:
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


def test_database_locked_carries_typed_attrs() -> None:
    err = DatabaseLockedError(holder_pid=12345)
    assert err.holder_pid == 12345


def test_embedder_identity_mismatch_carries_typed_attrs() -> None:
    err = EmbedderIdentityMismatchError(
        stored_name="model-a",
        stored_revision="0",
        supplied_name="model-b",
        supplied_revision="1",
    )
    assert err.stored_name == "model-a"
    assert err.supplied_name == "model-b"


def test_embedder_dimension_mismatch_carries_typed_attrs() -> None:
    err = EmbedderDimensionMismatchError(stored=384, supplied=768)
    assert err.stored == 384
    assert err.supplied == 768


def test_search_rejects_empty_query_via_write_validation_under_engine_error(
    db_path: str,
) -> None:
    # Per dev/design/errors.md section Binding-facing class matrix, the
    # empty-query rejection must surface as the typed WriteValidationError
    # leaf beneath the single-rooted EngineError, not as a bare ValueError.
    engine = Engine.open(db_path)
    try:
        with pytest.raises(WriteValidationError) as excinfo:
            engine.search("")
        assert isinstance(excinfo.value, EngineError)
        assert isinstance(excinfo.value, WriteValidationError)
    finally:
        engine.close()


# ---------------------------------------------------------------------------
# 0.8.20 Slice 22 (R-20-VC) — decision #18: one family at the write boundary
# ---------------------------------------------------------------------------

_SOURCE_ID = "py-test:decision-18"


def _node(logical_id: str, body: str, **extra: object) -> dict:
    return {"kind": "doc", "body": body, "logical_id": logical_id,
            "source_id": _SOURCE_ID, **extra}


def test_write_validation_boundary_is_exactly_one_error_family(db_path: str) -> None:
    """Decision #18 — every rejection from the engine's write-validation boundary
    raises ``WriteValidationError``, never ``InvalidArgumentError``.

    The consumer-visible defect this settles: the SAME ``engine.write`` call used
    to raise ``InvalidArgumentError`` for an inverted validity window but
    ``WriteValidationError`` for a non-integer bound. ``dev/design/errors.md``
    defines ``WriteValidationError`` as "malformed typed write shape", which is
    exactly what that boundary checks.
    """

    engine = Engine.open(db_path, use_default_embedder=False)
    try:
        cases = {
            "empty body": _node("B1", "   "),
            "empty logical_id": _node("", "ok"),
            "inverted window": _node("W1", "ok", valid_from=2000, valid_until=1000),
            "empty half-open window": _node("W2", "ok", valid_from=1500, valid_until=1500),
            "non-integer bound": _node("W3", "ok", valid_from="1000"),
        }
        for label, item in cases.items():
            with pytest.raises(WriteValidationError) as excinfo:
                engine.write([item])
            assert not isinstance(excinfo.value, InvalidArgumentError), (
                f"{label} must not raise InvalidArgumentError (decision #18)"
            )
    finally:
        engine.close()


def test_invalid_argument_error_survives_as_a_distinct_leaf() -> None:
    """Decision #18 narrows WHERE ``InvalidArgumentError`` is used; it does not
    remove it. It stays the message-carrying class for caller-argument rejections
    OUTSIDE the write-validation boundary (e.g. an out-of-range traversal depth).
    """

    assert issubclass(InvalidArgumentError, EngineError)
    assert not issubclass(InvalidArgumentError, WriteValidationError)
    assert not issubclass(WriteValidationError, InvalidArgumentError)
    assert str(InvalidArgumentError("depth must be 1-3, got 9")).startswith("depth must be 1-3")
