"""Exception hierarchy assertions for the Python SDK.

Pins the single rooted hierarchy beneath `EngineError` and the leaf-class
matrix from `dev/design/errors.md`. Per ADR-0.6.0-error-taxonomy and
`dev/design/bindings.md` § 3, callers dispatch on `except <Specific>` not on
message text; this test file uses `issubclass` to enforce the hierarchy.
"""

from __future__ import annotations

import pytest

from fathomdb.errors import (
    ClosingError,
    CorruptionError,
    DatabaseLockedError,
    EmbedderDimensionMismatchError,
    EmbedderError,
    EmbedderIdentityMismatchError,
    EngineError,
    IncompatibleSchemaVersionError,
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
