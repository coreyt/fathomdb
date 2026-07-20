"""Single-rooted exception hierarchy for the FathomDB Python SDK.

Layout owned by `dev/design/errors.md` § Binding-facing class matrix and
`dev/design/bindings.md` § 3. Concrete leaf classes are created by the
PyO3 binding (`fathomdb._fathomdb`); they inherit from `EngineError`
which inherits from Python `Exception`. Typed payload attributes
(`holder_pid`, `kind`, `stage`, `recovery_hint_code`, `doc_anchor`,
`stored`, `supplied`, `stored_name`, `stored_revision`, `supplied_name`,
`supplied_revision`) are set by the binding's
`engine_error_to_py` / `engine_open_error_to_py` translators on raise.

For Python-only construction with typed kwargs (used by binding tests),
`_install_typed_init` layers a keyword-only `__init__` onto each
PyO3-created class that stores the documented payload fields as
instance attributes.
"""

from __future__ import annotations

from fathomdb._fathomdb import (
    ClosingError as _ClosingError,
)
from fathomdb._fathomdb import (
    CorruptionError as _CorruptionError,
)
from fathomdb._fathomdb import (
    DatabaseLockedError as _DatabaseLockedError,
)
from fathomdb._fathomdb import (
    EmbedderDimensionMismatchError as _EmbedderDimensionMismatchError,
)
from fathomdb._fathomdb import (
    EmbedderError as _EmbedderError,
)
from fathomdb._fathomdb import (
    EmbedderNotConfiguredError as _EmbedderNotConfiguredError,
)
from fathomdb._fathomdb import (
    EmbedderIdentityMismatchError as _EmbedderIdentityMismatchError,
)
from fathomdb._fathomdb import (
    EngineError as _EngineError,
)
from fathomdb._fathomdb import (
    IncompatibleSchemaVersionError as _IncompatibleSchemaVersionError,
)
from fathomdb._fathomdb import (
    MigrationError as _MigrationError,
)
from fathomdb._fathomdb import (
    OpStoreError as _OpStoreError,
)
from fathomdb._fathomdb import (
    OverloadedError as _OverloadedError,
)
from fathomdb._fathomdb import (
    ProjectionError as _ProjectionError,
)
from fathomdb._fathomdb import (
    SchedulerError as _SchedulerError,
)
from fathomdb._fathomdb import (
    SchemaValidationError as _SchemaValidationError,
)
from fathomdb._fathomdb import (
    StorageError as _StorageError,
)
from fathomdb._fathomdb import (
    KindNotVectorIndexedError as _KindNotVectorIndexedError,
)
from fathomdb._fathomdb import (
    VectorError as _VectorError,
)
from fathomdb._fathomdb import (
    WriteValidationError as _WriteValidationError,
)
from fathomdb._fathomdb import (
    ExtractorError as _ExtractorError,
)
from fathomdb._fathomdb import (
    ConsolidatorError as _ConsolidatorError,
)
from fathomdb._fathomdb import (
    InvalidFilterError as _InvalidFilterError,
)
from fathomdb._fathomdb import (
    InvalidArgumentError as _InvalidArgumentError,
)
from fathomdb._fathomdb import (
    VectorEquivalenceMismatchError as _VectorEquivalenceMismatchError,
)
from fathomdb._fathomdb import (
    IllegalTransitionError as _IllegalTransitionError,
)
from fathomdb._fathomdb import (
    NotLifecycleAddressableError as _NotLifecycleAddressableError,
)
from fathomdb._fathomdb import (
    ErasureIncompleteError as _ErasureIncompleteError,
)

EngineError = _EngineError
StorageError = _StorageError
ProjectionError = _ProjectionError
VectorError = _VectorError
EmbedderError = _EmbedderError
EmbedderNotConfiguredError = _EmbedderNotConfiguredError
KindNotVectorIndexedError = _KindNotVectorIndexedError
SchedulerError = _SchedulerError
OpStoreError = _OpStoreError
WriteValidationError = _WriteValidationError
SchemaValidationError = _SchemaValidationError
OverloadedError = _OverloadedError
ClosingError = _ClosingError
DatabaseLockedError = _DatabaseLockedError
CorruptionError = _CorruptionError
IncompatibleSchemaVersionError = _IncompatibleSchemaVersionError
MigrationError = _MigrationError
EmbedderIdentityMismatchError = _EmbedderIdentityMismatchError
EmbedderDimensionMismatchError = _EmbedderDimensionMismatchError
# G11 (Slice 15) — BYO-LLM extraction harness protocol error.
ExtractorError = _ExtractorError
# 0.8.12 Slice 15 (OPP-2) — consolidation harness protocol error.
ConsolidatorError = _ConsolidatorError
# G4 (Slice 35) — filter predicate construction error (non-allowlisted path).
InvalidFilterError = _InvalidFilterError
InvalidArgumentError = _InvalidArgumentError
# 0.8.18 Slice 5 (#5 vector-equivalence probe) — query-time dense-refusal leaf.
VectorEquivalenceMismatchError = _VectorEquivalenceMismatchError
# OPP-12 Phase-1 (0.8.19 Slice 10) — lifecycle-verb typed errors. Parity-safe
# field names (S7): `from_state`/`to_state` (never `from`, a Python keyword).
IllegalTransitionError = _IllegalTransitionError
NotLifecycleAddressableError = _NotLifecycleAddressableError
# 0.8.20 Slice 5b (R-20-E5) — an erasure verb (`purge` / `excise_source`) deleted
# its rows but could not complete the erasure AT REST, typically because a
# concurrent reader pinned a WAL snapshot and `wal_checkpoint(TRUNCATE)` stayed
# busy. Retryable: re-run the verb once the reader has finished.
ErasureIncompleteError = _ErasureIncompleteError


def _install_typed_init(cls: type, fields: tuple[str, ...]) -> None:
    def __init__(self, *args, **kwargs):  # type: ignore[no-untyped-def]
        payload = {name: kwargs.pop(name, None) for name in fields}
        if kwargs:
            unexpected = ", ".join(sorted(kwargs))
            raise TypeError(f"unexpected keyword arguments: {unexpected}")
        Exception.__init__(self, *args)
        for name, value in payload.items():
            object.__setattr__(self, name, value)

    cls.__init__ = __init__  # type: ignore[method-assign]


_install_typed_init(DatabaseLockedError, ("holder_pid",))
_install_typed_init(
    CorruptionError,
    ("kind", "stage", "recovery_hint_code", "doc_anchor"),
)
_install_typed_init(
    EmbedderIdentityMismatchError,
    ("stored_name", "stored_revision", "supplied_name", "supplied_revision"),
)
_install_typed_init(EmbedderDimensionMismatchError, ("stored", "supplied"))
# 0.8.18 Slice 5 — the query-time refusal carries the divergence `reason`.
_install_typed_init(VectorEquivalenceMismatchError, ("reason",))
# OPP-12 Phase-1 (0.8.19 Slice 10) — lifecycle-verb payloads.
_install_typed_init(IllegalTransitionError, ("from_state", "to_state", "legal"))
_install_typed_init(NotLifecycleAddressableError, ("id_space",))
# 0.8.20 Slice 5b — the incomplete-erasure refusal carries the uncompleted stage.
_install_typed_init(ErasureIncompleteError, ("stage", "detail"))


__all__ = [
    "ClosingError",
    "CorruptionError",
    "DatabaseLockedError",
    "EmbedderDimensionMismatchError",
    "EmbedderError",
    "EmbedderIdentityMismatchError",
    "EmbedderNotConfiguredError",
    "ConsolidatorError",
    "EngineError",
    "ErasureIncompleteError",
    "ExtractorError",
    "IllegalTransitionError",
    "IncompatibleSchemaVersionError",
    "InvalidArgumentError",
    "InvalidFilterError",
    "KindNotVectorIndexedError",
    "MigrationError",
    "NotLifecycleAddressableError",
    "OpStoreError",
    "OverloadedError",
    "ProjectionError",
    "SchedulerError",
    "SchemaValidationError",
    "StorageError",
    "VectorEquivalenceMismatchError",
    "VectorError",
    "WriteValidationError",
]
