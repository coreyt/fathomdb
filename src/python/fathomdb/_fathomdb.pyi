"""Type stubs for the PyO3 extension `fathomdb._fathomdb`.

Mirrors the surface emitted by `src/rust/crates/fathomdb-py/src/lib.rs`.
Hand-maintained — keep in sync with the binding's `#[pyclass]` /
`create_exception!` / `#[pyfunction]` exports.
"""

from typing import Any, Iterable

from fathomdb.types import EmbedderEvent

class WriteReceipt:
    cursor: int

class SoftFallback:
    branch: str

class SearchHit:
    id: int
    kind: str
    body: str
    score: float
    branch: str

class SearchResult:
    projection_cursor: int
    soft_fallback: SoftFallback | None
    results: list[SearchHit]

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
    # EU-5a1/5a2/5b — surfaced by EU-6.
    embedder_download_ms: int | None
    embedder_events: list[EmbedderEvent]
    embedder_mean_centering_required: bool
    embedder_mean_vec_pinned: bool

class Engine:
    @staticmethod
    def open(path: str, use_default_embedder: bool = ...) -> "Engine": ...
    # NOTE: `_configure_vector_kind_for_test` and `_write_vector_for_test`
    # are intentionally NOT declared here. They only exist on the binary
    # when the `test-hooks` Cargo feature is enabled (see
    # `src/rust/crates/fathomdb-py/src/lib.rs::#[cfg(any(test, feature =
    # "test-hooks"))]`), and that feature is dev-only — it is not part of
    # the shipped wheel's feature axis (`pyproject.toml [tool.maturin]
    # features` and `release.yml` build-python's `args:`). Advertising
    # them on the public stub would imply they are callable by end users,
    # which is false.
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
