# Design: Moderate Python Binding Layer

## Purpose

Define a Python library for `fathomdb` that is:

- straightforward for Python applications to use
- faithful to the current Rust facade
- moderate in scope
- thin enough to keep the Rust engine as the single source of truth

This is not a proposal for a second engine or a Python-native reimplementation.
The Python library is a binding and ergonomics layer over the existing Rust
surface in [`crates/fathomdb`](../crates/fathomdb/src/lib.rs).

## Goals

1. Make the common Python path obvious:
   - open a database
   - submit writes
   - run graph/text/vector queries
   - use admin/recovery operations
2. Keep the Python API close to the Rust facade so behavior stays predictable.
3. Hide Rust-only details that are awkward in Python:
   - lifetime-based `Session<'a>`
   - manual JSON string serialization
   - low-level engine/coordinator/writer handle splits
4. Preserve recoverability semantics:
   - provenance mode remains explicit
   - admin operations remain first-class
   - projection rebuild and excision stay available
5. Ship as a normal Python package with wheels.

## Non-Goals

1. Do not invent a Python-only query language.
2. Do not add ORM-style schema inference or model registration.
3. Do not bypass Rust validation by writing SQLite directly from Python.
4. Do not expose every internal Rust type one-for-one if that makes the Python
   API worse.
5. Do not make the initial Python layer async. The Rust engine is synchronous
   internally today.

## Current Rust Surface To Bind

Today the public Rust facade exposes:

- `Engine::open`
- `Engine::query(kind)` returning a `QueryBuilder`
- `Engine::writer()`
- `Engine::coordinator()`
- `Engine::admin()`

Query builder capabilities currently include:

- `nodes(kind)`
- `vector_search(query, limit)`
- `text_search(query, limit)`
- `traverse(direction, label, max_depth)`
- `filter_logical_id_eq`
- `filter_kind_eq`
- `filter_source_ref_eq`
- `filter_json_text_eq`
- `limit`
- `compile`

Write capabilities currently include:

- node insert / upsert / retire
- edge insert / upsert / retire
- chunk insert
- vector insert
- run / step / action insert and upsert
- optional projection backfills

Admin capabilities currently include:

- `check_integrity`
- `check_semantics`
- `rebuild_projections`
- `rebuild_missing_projections`
- `trace_source`
- `excise_source`
- `safe_export`

The Python library should cover all of these.

## Decision

Use a two-layer Python design:

1. A small Rust extension module built with `PyO3`
2. A thin pure-Python package that provides Pythonic models and helpers

This is the right moderate point.

Rejected alternatives:

- Expose only a raw `PyO3` surface with Rust-shaped structs everywhere
  - Too awkward for Python users
- Build most behavior in Python and treat Rust as a low-level SQL executor
  - Risks semantic drift from the real engine
- Add a separate Rust crate just for Python bindings
  - Avoidable in v1; the public facade crate can grow a `python` feature and
    `cdylib` target without adding another engine crate

## Package Layout

Use this layout:

```text
/
  crates/
    fathomdb/
      Cargo.toml
      src/
        lib.rs
        python.rs
        ffi_types.rs
  python/
    pyproject.toml
    fathomdb/
      __init__.py
      _types.py
      _query.py
      _admin.py
      errors.py
      py.typed
  python/tests/
```

### Rust Side

Keep the binding code inside `crates/fathomdb` behind a `python` feature:

- avoid adding another Rust crate in v1
- bind against the stable facade rather than engine internals
- keep all Python-visible semantics anchored in the same public Rust API

`crates/fathomdb/Cargo.toml` should eventually add:

- `pyo3` as an optional dependency
- a `python` feature
- `crate-type = ["rlib", "cdylib"]`

### Python Side

The published Python package should be named `fathomdb`.

The native extension module should be private:

- import path: `fathomdb._fathomdb`

The pure-Python package is where application-facing ergonomics live:

- dataclasses
- enums
- helper constructors
- error aliases
- query builder façade

## Public Python API

### Top-Level Entry Point

The main application object should be `Engine`.

```python
from fathomdb import Engine

db = Engine.open("agent.db", provenance_mode="warn", vector_dimension=1536)
```

Python should expose:

```python
class Engine:
    @classmethod
    def open(
        cls,
        database_path: str | os.PathLike[str],
        *,
        provenance_mode: ProvenanceMode = ProvenanceMode.WARN,
        vector_dimension: int | None = None,
    ) -> "Engine": ...

    def nodes(self, kind: str) -> "Query": ...
    def query(self, kind: str) -> "Query": ...

    def write(self, request: "WriteRequest") -> "WriteReceipt": ...
    def submit(self, request: "WriteRequest") -> "WriteReceipt": ...

    @property
    def admin(self) -> "AdminClient": ...
```

Notes:

- `query(kind)` is an alias for `nodes(kind)`, mirroring Rust intent.
- `write()` is the Python primary verb, even if the Rust internals use
  `writer.submit(...)`.
- Do not expose `writer()`, `coordinator()`, and `admin()` as three separate
  raw handles initially. That split is useful inside Rust, but too mechanical
  for Python.

### Lifecycle and Resource Management

Only one `Engine` may be open per database file.  A second `Engine.open()` on
the same path raises `DatabaseLockedError` immediately, with the holder PID
in the message.

```python
# Preferred: context manager guarantees cleanup
with Engine.open("agent.db") as db:
    db.write(...)

# Manual lifecycle
db = Engine.open("agent.db")
try:
    db.write(...)
finally:
    db.close()
```

`close()` is idempotent.  Any operation after close raises
`FathomError: engine is closed`.

**GC-safe Drop:**  If the Python object is garbage-collected without an
explicit `close()` call, the Rust `Drop` impl releases the GIL via
`py.allow_threads()` before shutting down the writer thread.  This prevents
a GIL deadlock where the writer thread needs the GIL for pyo3-log but the
main thread holds it while waiting for the thread join.  Explicit `close()`
or context manager usage is still recommended.

Also expose ID helpers at module scope:

```python
def new_id() -> str: ...
def new_row_id() -> str: ...
```

These should call through to the existing Rust helpers so Python applications
do not invent their own row-id format.

### Query API

The Python query builder should stay close to the current Rust builder.

```python
results = (
    db.nodes("Meeting")
      .text_search("budget review", limit=5)
      .traverse(direction="out", label="HAS_TASK", max_depth=1)
      .filter_json_text_eq("$.status", "active")
      .limit(10)
      .execute()
)
```

Expose:

```python
class Query:
    def vector_search(self, query: str, limit: int) -> "Query": ...
    def text_search(self, query: str, limit: int) -> "Query": ...
    def traverse(
        self,
        *,
        direction: TraverseDirection | str,
        label: str,
        max_depth: int,
    ) -> "Query": ...
    def filter_logical_id_eq(self, logical_id: str) -> "Query": ...
    def filter_kind_eq(self, kind: str) -> "Query": ...
    def filter_source_ref_eq(self, source_ref: str) -> "Query": ...
    def filter_json_text_eq(self, path: str, value: str) -> "Query": ...
    def limit(self, limit: int) -> "Query": ...

    def compile(self) -> "CompiledQuery": ...
    def explain(self) -> "QueryPlan": ...
    def execute(self) -> "QueryRows": ...
```

Design constraints:

- no Python lambda predicates in v1
- no magic AST inspection of Python callables
- no `select(...)` projection API until the Rust engine supports it

That keeps the Python surface honest. The architecture docs show richer
examples, but the Python binding should only expose the capabilities that
already exist in the Rust API.

### Writes

Python writes should use dataclasses rather than raw dicts.

```python
from fathomdb import (
    WriteRequest,
    NodeInsert,
    ChunkInsert,
    ChunkPolicy,
)

request = WriteRequest(
    label="meeting-ingest",
    nodes=[
        NodeInsert(
            row_id="01H...",
            logical_id="meeting:budget-2026-03-25",
            kind="Meeting",
            properties={"title": "Budget review", "status": "active"},
            source_ref="action:123",
            upsert=True,
            chunk_policy=ChunkPolicy.REPLACE,
        )
    ],
    chunks=[
        ChunkInsert(
            id="chunk-1",
            node_logical_id="meeting:budget-2026-03-25",
            text_content="Transcript text...",
        )
    ],
)

receipt = db.write(request)
```

Expose dataclasses for:

- `NodeInsert`
- `EdgeInsert`
- `NodeRetire`
- `EdgeRetire`
- `ChunkInsert`
- `VecInsert`
- `RunInsert`
- `StepInsert`
- `ActionInsert`
- `OptionalProjectionTask`
- `WriteRequest`
- `WriteReceipt`

`VecInsert` is retained as a low-level/admin/import binding. Normal
application code should prefer canonical chunks plus configured vector
projection; the managed-vector target design makes FathomDB responsible for
async/incremental vector rows.

### Admin Surface

Admin operations should be grouped under `Engine.admin`.

```python
report = db.admin.check_integrity()
trace = db.admin.trace_source("action:123")
repair = db.admin.rebuild(target="fts")
manifest = db.admin.safe_export("exports/db.sqlite")
```

Expose:

```python
class AdminClient:
    def check_integrity(self) -> IntegrityReport: ...
    def check_semantics(self) -> SemanticReport: ...
    def rebuild(
        self,
        target: ProjectionTarget | str = ProjectionTarget.ALL,
    ) -> ProjectionRepairReport: ...
    def rebuild_missing(self) -> ProjectionRepairReport: ...
    def trace_source(self, source_ref: str) -> TraceReport: ...
    def excise_source(self, source_ref: str) -> TraceReport: ...
    def safe_export(
        self,
        destination_path: str | os.PathLike[str],
        *,
        force_checkpoint: bool = True,
    ) -> SafeExportManifest: ...
```

This is intentionally a thin semantic rename over the Rust admin surface:

- `rebuild()` maps to `rebuild_projections`
- `rebuild_missing()` maps to `rebuild_missing_projections`

### Reads Outside The Query Builder

The Rust coordinator currently exposes:

- `read_run`
- `read_step`
- `read_action`
- `read_active_runs`
- `query_provenance_events`

These should not be top-level Python methods on day one unless they are needed
by an actual Python workflow. They can be exposed under `Engine.admin` or
`Engine.debug` later.

Moderate means not exporting every test/debug helper immediately.

## Python Types And Data Mapping

### JSON Properties

Rust currently uses `String` for JSON payload fields such as:

- `NodeInsert.properties`
- `EdgeInsert.properties`
- `RunInsert.properties`
- `StepInsert.properties`
- `ActionInsert.properties`
- row `properties`

Python should not make callers manually serialize JSON strings.

Decision:

- Python accepts `dict`, `list`, `str`, `int`, `float`, `bool`, or `None`
- the pure-Python layer serializes these to JSON text before calling Rust
- row `properties` fields are decoded back into Python objects on read

Provide one explicit escape hatch:

```python
RawJson(text: str)
```

Use cases:

- callers already have canonical JSON text
- tests want byte-for-byte control over payloads

### Enums

Expose Python enums for:

- `ProvenanceMode`
- `ChunkPolicy`
- `ProjectionTarget`
- `TraverseDirection`

These should subclass `str` and `Enum` so they are easy to serialize and easy
to pass from normal Python code.

### Result Models

Use frozen dataclasses for returned rows and reports:

- `NodeRow`
- `RunRow`
- `StepRow`
- `ActionRow`
- `CompiledQuery`
- `QueryRows`
- `QueryPlan`
- `IntegrityReport`
- `SemanticReport`
- `TraceReport`
- `ProjectionRepairReport`
- `SafeExportManifest`

`QueryRows` should match the Rust shape:

```python
@dataclass(frozen=True)
class QueryRows:
    nodes: list[NodeRow]
    runs: list[RunRow]
    steps: list[StepRow]
    actions: list[ActionRow]
    was_degraded: bool
```

This aligns with the existing Rust decision to keep results simple and put
diagnostics in `QueryPlan` rather than the read result itself.

## Error Model

The binding layer should map Rust errors into a small Python exception tree.

```python
class FathomError(Exception): ...
class SqliteError(FathomError): ...
class SchemaError(FathomError): ...
class InvalidWriteError(FathomError): ...
class CapabilityMissingError(FathomError): ...
class WriterRejectedError(FathomError): ...
class DatabaseLockedError(FathomError): ...
class BridgeError(FathomError): ...
class IoError(FathomError): ...
class CompileError(FathomError): ...
```

Rules:

- preserve the original Rust error message
- map by semantic category, not by every internal enum variant
- keep Python stack traces readable

## Logging

`pyo3-log` is initialized in the `#[pymodule]` init function and bridges Rust
diagnostic events into Python's standard `logging` module.  The bridge is
automatic and requires no configuration from the Python caller — events appear
under logger names like `fathomdb_engine.writer`, `fathomdb_engine.sqlite`, etc.

Python applications control fathomdb log verbosity through normal Python
logging configuration:

```python
import logging
logging.getLogger("fathomdb_engine").setLevel(logging.DEBUG)
```

The bridge works via tracing's `"log"` feature: when no native Rust tracing
subscriber is active (the normal case in Python), tracing events are emitted as
`log` records, which `pyo3-log` forwards to Python.  This is zero-configuration
for the application developer.

## Rust/Python Boundary

### Decision

Make the native extension intentionally small.

The extension should own:

- engine lifetime
- calling into Rust without the GIL
- validating and converting Python request payloads into Rust types
- converting Rust results into Python-friendly plain objects

The pure-Python layer should own:

- dataclass construction
- ergonomic method names
- JSON convenience
- aliases and helper constructors

### Extension Surface

The native `_fathomdb` module should expose a few stable primitives:

```python
class EngineCore:
    @classmethod
    def open(...): ...
    def execute_ast(self, ast: QueryAstPayload) -> QueryRowsPayload: ...
    def compile_ast(self, ast: QueryAstPayload) -> CompiledQueryPayload: ...
    def explain_ast(self, ast: QueryAstPayload) -> QueryPlanPayload: ...
    def submit_write(self, request: WriteRequestPayload) -> WriteReceiptPayload: ...
    def check_integrity(self) -> IntegrityReportPayload: ...
    def check_semantics(self) -> SemanticReportPayload: ...
    def rebuild_projections(self, target: str) -> ProjectionRepairReportPayload: ...
    def rebuild_missing_projections(self) -> ProjectionRepairReportPayload: ...
    def trace_source(self, source_ref: str) -> TraceReportPayload: ...
    def excise_source(self, source_ref: str) -> TraceReportPayload: ...
    def safe_export(self, destination: str, force_checkpoint: bool) -> SafeExportManifestPayload: ...
```

`Payload` here means FFI-specific structs or dict-like objects, not raw engine
types. The binding layer should not require every existing Rust struct to grow
`Serialize`/`Deserialize` just to satisfy Python.

### FFI DTOs

Introduce dedicated Python-FFI DTO structs in `crates/fathomdb`:

- `PyQueryAst`
- `PyQueryRows`
- `PyWriteRequest`
- `PyNodeInsert`
- `PyIntegrityReport`
- and similar peers

Each DTO should implement conversion to or from the existing Rust facade types.

Rationale:

- avoids leaking `PyO3` concerns into engine crates
- keeps Python-specific shape decisions local
- allows Python-friendly JSON values without changing internal Rust structs

### GIL Behavior

All blocking Rust calls should release the GIL:

- `Engine.open`
- query compile/execute/explain
- write submit
- admin operations

Use `Python::allow_threads` around Rust calls that may touch SQLite, the writer
thread, filesystem, or hashing.

## Session Design

Rust has a `Session<'a>` type today. Python should not expose it initially.

Reasons:

- the Rust session currently adds no distinct capability beyond holding an
  engine reference
- Python has no useful equivalent of the Rust lifetime-based borrow here
- exposing it now adds API surface without value

If session-scoped settings emerge later, Python can add:

```python
with db.session() as session:
    ...
```

But not in the first cut.

## Packaging And Distribution

Use `maturin` for packaging.

### Python Build

Add `python/pyproject.toml` configured to build the extension from
`crates/fathomdb`.

Expected outcomes:

- wheel install for normal Python users
- source install for contributors with Rust toolchain
- one published package name: `fathomdb`

### Versioning

The Python package version should match the workspace version.

The extension should expose:

- `__version__`
- protocol or build metadata only if needed later

Do not invent a separate version stream for the Python package.

## Testing Strategy

Implementation must follow TDD.

### Python Unit Tests

Add Python tests for:

1. JSON payload encoding and decoding
2. enum conversion
3. error mapping
4. query builder AST assembly
5. convenience aliases such as `Engine.query == Engine.nodes`

### Python Integration Tests

Add real DB integration tests for:

1. open database and bootstrap schema
2. insert node + chunk + query text
3. upsert node and observe active-state read behavior
4. trace by `source_ref`
5. excise by `source_ref`
6. safe export manifest round trip
7. vector path when built with the `sqlite-vec` feature
8. degraded vector read path when feature is absent

### Cross-Language Contract Tests

Add a small contract suite that uses the same temporary database from:

- Rust tests
- Python tests

The point is not shared test code. The point is shared expected semantics:

- write receipt warning behavior
- active-row resolution
- projection rebuild outcomes
- trace and excision semantics

## Implementation Slices

### Slice 1: Packaging And Engine Open

Tests first:

- package imports
- `Engine.open(...)`
- `admin.check_integrity()`

Implementation:

- `PyO3` feature in `crates/fathomdb`
- private `_fathomdb` extension
- pure-Python `Engine`

### Slice 2: Query Builder

Tests first:

- `nodes().text_search().limit().execute()`
- `compile()`
- `explain()`
- degraded vector behavior

Implementation:

- pure-Python `Query`
- FFI query AST DTOs
- Rust conversion into `QueryBuilder` / `QueryAst`

### Slice 3: Writes

Tests first:

- node/chunk write round trip
- upsert semantics
- provenance warning vs require behavior

Implementation:

- write dataclasses
- JSON encoding helpers
- FFI DTO conversions to `WriteRequest`

### Slice 4: Admin And Recovery

Tests first:

- `trace_source`
- `excise_source`
- `rebuild`
- `safe_export`

Implementation:

- `AdminClient`
- report model mapping

## Example Usage

```python
from fathomdb import (
    ChunkInsert,
    ChunkPolicy,
    Engine,
    NodeInsert,
    ProvenanceMode,
    WriteRequest,
    new_row_id,
)

db = Engine.open(
    "agent.db",
    provenance_mode=ProvenanceMode.WARN,
    vector_dimension=1536,
)

receipt = db.write(
    WriteRequest(
        label="meeting-ingest",
        nodes=[
            NodeInsert(
                row_id=new_row_id(),
                logical_id="meeting:2026-03-25-budget",
                kind="Meeting",
                properties={
                    "title": "Budget review",
                    "status": "active",
                },
                source_ref="action:meeting-import",
                upsert=True,
                chunk_policy=ChunkPolicy.REPLACE,
            )
        ],
        chunks=[
            ChunkInsert(
                id="chunk:meeting:2026-03-25-budget:0",
                node_logical_id="meeting:2026-03-25-budget",
                text_content="Transcript text...",
            )
        ],
    )
)

rows = (
    db.nodes("Meeting")
      .text_search("budget", limit=5)
      .filter_json_text_eq("$.status", "active")
      .limit(10)
      .execute()
)

trace = db.admin.trace_source("action:meeting-import")
```

## Done When

The Python binding layer is ready when:

1. A Python application can install `fathomdb` and open a local database with
   one import.
2. Writes accept normal Python JSON-like values instead of requiring JSON text.
3. Query building is fluent and maps directly onto the current Rust query
   surface.
4. Admin and recovery operations are available from Python.
5. The Python layer does not introduce semantics that the Rust facade does not
   already support.
6. Python integration tests pass against the real Rust engine.
