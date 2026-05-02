"""Single-rooted exception hierarchy for the FathomDB Python SDK.

Layout owned by `dev/design/errors.md` § Binding-facing class matrix and
`dev/design/bindings.md` § 3. Per-leaf attributes are typed; callers
dispatch on `except <Specific>` rather than on message text.
"""

from __future__ import annotations


class EngineError(Exception):
    """Catch-all base class for every engine-surfaced error.

    Concrete leaf classes carry typed attributes named in
    `dev/design/errors.md`; callers should narrow on those leaves rather than
    on the message string.
    """


class StorageError(EngineError):
    """Canonical SQLite read/write path failure."""


class ProjectionError(EngineError):
    """Projection-row commit or terminal-state accounting failure."""


class VectorError(EngineError):
    """`sqlite-vec` encode/load/query path failure."""


class EmbedderError(EngineError):
    """Embedder dispatch, timeout, or invalid-vector-return failure."""


class SchedulerError(EngineError):
    """Scheduler startup, shutdown, or queue orchestration failure."""


class OpStoreError(EngineError):
    """Op-store collection / kind / registry contract failure."""


class WriteValidationError(EngineError):
    """Submitted typed write is malformed before payload checks run."""


class SchemaValidationError(EngineError):
    """Op-store payload failed the registered `schema_id` JSON Schema."""


class OverloadedError(EngineError):
    """Backpressure exhaustion / engine overload."""


class ClosingError(EngineError):
    """Operation rejected because the engine is closing."""


class DatabaseLockedError(EngineError):
    """Database is locked by another `Engine` instance."""

    def __init__(self, *, holder_pid: int | None = None) -> None:
        super().__init__(f"database locked (holder_pid={holder_pid!r})")
        self.holder_pid = holder_pid


class CorruptionError(EngineError):
    """Open-path corruption surfaced from `Engine.open`.

    Carries the stable dispatch key `recovery_hint_code` plus the
    documentation pointer `doc_anchor` per
    `dev/design/errors.md` § Corruption detail owner.
    """

    def __init__(
        self,
        *,
        kind: str,
        stage: str,
        recovery_hint_code: str,
        doc_anchor: str,
    ) -> None:
        super().__init__(f"corruption {kind} at stage {stage} ({recovery_hint_code})")
        self.kind = kind
        self.stage = stage
        self.recovery_hint_code = recovery_hint_code
        self.doc_anchor = doc_anchor


class IncompatibleSchemaVersionError(EngineError):
    """Database schema version is incompatible with this engine build."""


class MigrationError(EngineError):
    """Schema migration failed during `Engine.open`."""


class EmbedderIdentityMismatchError(EngineError):
    """Stored vs supplied embedder identity disagree on open."""

    def __init__(
        self,
        *,
        stored_name: str,
        stored_revision: str,
        supplied_name: str,
        supplied_revision: str,
    ) -> None:
        super().__init__(
            f"embedder identity mismatch: stored {stored_name}@{stored_revision}, "
            f"supplied {supplied_name}@{supplied_revision}"
        )
        self.stored_name = stored_name
        self.stored_revision = stored_revision
        self.supplied_name = supplied_name
        self.supplied_revision = supplied_revision


class EmbedderDimensionMismatchError(EngineError):
    """Stored vs supplied embedder vector dimensions disagree."""

    def __init__(self, *, stored: int, supplied: int) -> None:
        super().__init__(
            f"embedder vector dimension mismatch: stored {stored}, supplied {supplied}"
        )
        self.stored = stored
        self.supplied = supplied


__all__ = [
    "ClosingError",
    "CorruptionError",
    "DatabaseLockedError",
    "EmbedderDimensionMismatchError",
    "EmbedderError",
    "EmbedderIdentityMismatchError",
    "EngineError",
    "IncompatibleSchemaVersionError",
    "MigrationError",
    "OpStoreError",
    "OverloadedError",
    "ProjectionError",
    "SchedulerError",
    "SchemaValidationError",
    "StorageError",
    "VectorError",
    "WriteValidationError",
]
