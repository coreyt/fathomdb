"""Type stubs for the PyO3 extension `fathomdb._fathomdb`.

Mirrors the surface emitted by `src/rust/crates/fathomdb-py/src/lib.rs`.
Hand-maintained — keep in sync with the binding's `#[pyclass]` /
`create_exception!` / `#[pyfunction]` exports.
"""

from typing import Any, Iterable

class WriteReceipt:
    cursor: int

class SoftFallback:
    branch: str

class SearchResult:
    projection_cursor: int
    soft_fallback: SoftFallback | None
    results: list[str]

class CounterSnapshot:
    queries: int
    writes: int
    write_rows: int
    admin_ops: int
    cache_hit: int
    cache_miss: int

class MigrationStepReport:
    step_id: int
    duration_ms: int | None
    failed: bool

class EmbedderIdentity:
    name: str
    revision: str
    dimension: int

class OpenReport:
    schema_version_before: int
    schema_version_after: int
    migration_steps: list[MigrationStepReport]
    embedder_warmup_ms: int
    query_backend: str
    default_embedder: EmbedderIdentity

class Engine:
    @staticmethod
    def open(path: str) -> "Engine": ...
    def open_report(self) -> OpenReport: ...
    def write(self, batch: Iterable[Any]) -> WriteReceipt: ...
    def search(self, query: str) -> SearchResult: ...
    def close(self) -> None: ...
    def drain(self, timeout_s: float = ...) -> None: ...
    def counters(self) -> CounterSnapshot: ...
    def set_profiling(self, enabled: bool) -> None: ...
    def set_slow_threshold_ms(self, value: int) -> None: ...
    def attach_logging_subscriber(
        self,
        logger: Any,
        heartbeat_interval_ms: int | None = ...,
    ) -> None: ...

def admin_configure(engine: Engine, name: str, body: str) -> WriteReceipt: ...
def force_panic_for_test() -> None: ...

class EngineError(Exception): ...
class StorageError(EngineError): ...
class ProjectionError(EngineError): ...
class VectorError(EngineError): ...
class KindNotVectorIndexedError(VectorError): ...
class EmbedderError(EngineError): ...
class EmbedderNotConfiguredError(EmbedderError): ...
class SchedulerError(EngineError): ...
class OpStoreError(EngineError): ...
class WriteValidationError(EngineError): ...
class SchemaValidationError(EngineError): ...
class OverloadedError(EngineError): ...
class ClosingError(EngineError): ...

class DatabaseLockedError(EngineError):
    holder_pid: int | None
    def __init__(self, *args: Any, holder_pid: int | None = ...) -> None: ...

class CorruptionError(EngineError):
    kind: str
    stage: str
    recovery_hint_code: str
    doc_anchor: str
    def __init__(
        self,
        *args: Any,
        kind: str = ...,
        stage: str = ...,
        recovery_hint_code: str = ...,
        doc_anchor: str = ...,
    ) -> None: ...

class IncompatibleSchemaVersionError(EngineError): ...
class MigrationError(EngineError): ...

class EmbedderIdentityMismatchError(EngineError):
    stored_name: str
    stored_revision: str
    supplied_name: str
    supplied_revision: str
    def __init__(
        self,
        *args: Any,
        stored_name: str = ...,
        stored_revision: str = ...,
        supplied_name: str = ...,
        supplied_revision: str = ...,
    ) -> None: ...

class EmbedderDimensionMismatchError(EngineError):
    stored: int
    supplied: int
    def __init__(self, *args: Any, stored: int = ..., supplied: int = ...) -> None: ...
