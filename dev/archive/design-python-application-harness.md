# Design: Python Application Harness For End-To-End Storage And Retrieval

## Purpose

Define a Python application that uses the shipped `fathomdb` Python library to:

- write data through every current engine write path
- verify retrieval through every current Python-readable surface
- prove that canonical state, derived projections, provenance, and recovery
  behaviors all work from Python

This is a harness-style application, not a general end-user product. Its job is
to exercise the Python bindings and the underlying engine in realistic,
repeatable scenarios.

## Design Boundary

The harness must use the current Python API exactly as it exists today.

That means:

- writes are broad:
  - nodes
  - node retires
  - edges
  - edge retires
  - chunks
  - vec inserts
  - runs
  - steps
  - actions
  - upsert/supersession behavior
- reads are narrower:
  - node-oriented query execution
  - admin integrity/semantic reports
  - trace by `source_ref`
  - excision
  - projection rebuilds

Important current limitation:

- the Python library does **not** currently expose direct runtime-table reads
  such as `read_run`, `read_step`, `read_action`, or provenance-event listing
- therefore the harness must verify runtime-table writes indirectly through
  `trace_source`, `check_semantics`, and behavior that depends on those rows
  rather than by directly selecting them

This is a real product constraint, not a test omission.

## Summary

Build a small Python package, tentatively named `python_app_harness`, with two
 execution profiles:

1. `baseline`
   - opens `Engine` without `vector_dimension`
   - verifies non-vector storage and retrieval
   - asserts vector reads degrade cleanly

2. `vector_enabled`
   - opens `Engine` with `vector_dimension`
   - writes embeddings with `VecInsert`
   - verifies vector retrieval and vector repair paths

The `vector_enabled` profile requires the Python extension to be built with the
Rust `sqlite-vec` feature enabled. Passing `vector_dimension` alone is not
sufficient if the extension was built without that feature.

The application should organize work as scenario-driven workloads. Each
scenario writes a coherent unit of data and then runs one or more retrieval
assertions. The harness succeeds only if all scenarios pass.

## Application Shape

Use this layout:

```text
python_app_harness/
  __init__.py
  app.py
  engine_factory.py
  scenarios/
    __init__.py
    canonical.py
    graph.py
    runtime.py
    recovery.py
    vector.py
  verify.py
  models.py
tests/
  test_baseline_harness.py
  test_vector_harness.py
```

### Main Roles

`engine_factory.py`

- open `fathomdb.Engine`
- choose baseline vs vector-enabled mode
- centralize DB-path and `vector_dimension` setup

`models.py`

- define stable workload constants:
  - logical IDs
  - source refs
  - chunk IDs
  - run/step/action IDs
  - reusable JSON payloads

`scenarios/*.py`

- each module writes a specific class of data
- each module returns a small result object containing:
  - scenario name
  - source refs used
  - logical IDs created
  - expected retrieval assertions

`verify.py`

- centralize retrieval assertions
- no scenario should hide its own verification logic in ad hoc print/debug code

`app.py`

- orchestrate all scenarios
- print a compact report
- exit nonzero on any verification failure

## Scenario Set

The harness should implement these scenarios.

### Scenario 1: Canonical Node + Chunk + FTS

Write:

- one `NodeInsert` for a `Meeting`
- one `ChunkInsert` linked to that node

Verify:

- `db.nodes("Meeting").filter_logical_id_eq(...).execute()` returns exactly one
  node
- `text_search(...)` returns the same node
- returned `properties` JSON decodes as expected
- `check_integrity()` shows no missing FTS rows

Purpose:

- proves canonical node storage
- proves chunk storage
- proves derived FTS projection visibility

### Scenario 2: Node Upsert / Supersession

Write:

- `NodeInsert(upsert=False)` version 1
- `NodeInsert(upsert=True)` version 2 for the same `logical_id`

Verify:

- `filter_logical_id_eq(...)` returns exactly one active node
- active node properties match version 2, not version 1
- `trace_source(source_ref_v1)` and `trace_source(source_ref_v2)` both show the
  expected node-row counts
- `check_semantics()` remains clean

Purpose:

- proves active-state semantics from Python
- proves supersession does not break retrieval

### Scenario 3: Graph Edge + Traversal

Write:

- two task nodes
- one `EdgeInsert` of kind `DEPENDS_ON`

Verify:

- a traversal query from task A with
  `traverse(direction=OUT, label="DEPENDS_ON", max_depth=1)` returns task B

Purpose:

- proves edge storage
- proves graph traversal retrieval

### Scenario 4: Edge Retire

Write:

- retire the previously created dependency edge via `EdgeRetire`

Verify:

- the same traversal query now returns no downstream node
- `check_semantics()` remains clean

Purpose:

- proves active-edge semantics and retired-edge exclusion

### Scenario 5: Runtime Tables (`runs`, `steps`, `actions`)

Write one `WriteRequest` containing:

- `RunInsert`
- `StepInsert`
- `ActionInsert`
- optionally one related canonical node whose `source_ref` points at the action

Verify through current public Python APIs:

- `trace_source(action_source_ref)` shows the expected `action_rows`
- `trace_source(node_source_ref)` shows the expected node lineage when the node
  is anchored to an action
- `check_semantics()` reports zero broken step/action foreign-key counts

Purpose:

- proves runtime-table writes
- proves typed runtime rows participate correctly in provenance and FK checks

Important note:

- because the current Python API does not expose direct `read_run` /
  `read_step` / `read_action`, this scenario is considered fully verified when
  trace and semantic checks match expectations

### Scenario 6: Node Retire And Semantic Detection

Write:

- a node with an active incoming or outgoing edge
- retire the node with `NodeRetire`

Verify:

- node lookup by `logical_id` returns no active node
- traversal no longer resolves the retired node as active state
- `check_semantics()` reports the expected dangling-edge count if the graph is
  intentionally left inconsistent

Purpose:

- proves node retire behavior
- proves semantic diagnostics are visible from Python

This scenario should have two variants:

1. `clean_retire`
   - graph is retired consistently
   - semantic report remains clean
2. `dangling_retire`
   - edge is intentionally left active
   - semantic report must flag dangling edges

### Scenario 7: Provenance Warnings / Require Mode

Run in two subcases:

1. `warn_mode`
   - open `Engine(..., provenance_mode=WARN)`
   - submit a canonical write with `source_ref=None`
   - assert `WriteReceipt.provenance_warnings` is non-empty

2. `require_mode`
   - open `Engine(..., provenance_mode=REQUIRE)`
   - submit the same write
   - assert `InvalidWriteError` is raised

Purpose:

- proves Python callers observe the provenance policy correctly

### Scenario 8: Trace + Excise

Write:

- one or more nodes/chunks under a dedicated `source_ref`

Verify before excision:

- `trace_source(...)` shows the expected node counts and logical IDs
- text search finds the node

Excise:

- call `db.admin.excise_source(source_ref)`

Verify after excision:

- `trace_source(...)` still reports the historical excised rows as expected
- active-state query no longer returns the node
- FTS query no longer returns the node

Purpose:

- proves source-ref tracing
- proves source-ref excision
- proves post-excise projection cleanup

### Scenario 9: Safe Export

Write:

- at least one canonical node/chunk pair

Verify:

- `safe_export(...)` succeeds
- returned manifest has:
  - non-empty `sha256`
  - positive `page_count`
  - valid `schema_version`

Purpose:

- proves Python can drive recovery/export tooling

### Scenario 10: Projection Repair

Use admin operations only:

- `rebuild_missing()`
- `rebuild(target=FTS)`
- `rebuild(target=ALL)`

Verify:

- each returns a structurally valid `ProjectionRepairReport`
- `targets` match the requested operation
- no subsequent `check_integrity()` / `check_semantics()` regression appears

Purpose:

- proves projection repair surfaces are callable and stable from Python

### Scenario 11: Vector Insert And Vector Search

This scenario runs only in `vector_enabled` mode.

Write:

- a node
- a chunk
- one `VecInsert` with a deterministic embedding

Verify:

- `vector_search(...)` returns the node
- `QueryRows.was_degraded` is `False`
- `rebuild(target=VEC)` succeeds
- `check_semantics()` shows no stale vec rows for the happy path

Purpose:

- proves vector storage and retrieval from Python when capability is enabled

### Scenario 12: Vector Degradation

This scenario runs only in `baseline` mode.

Verify:

- a `vector_search(...)` call returns:
  - empty `nodes`
  - `was_degraded == True`

Purpose:

- proves the non-vector contract is correctly exposed to Python

## Storage Matrix

The harness must cover every current persisted or projection-backed write path:

| Storage Path | Covered By |
|---|---|
| `nodes` | Scenarios 1, 2, 6, 8 |
| `edges` | Scenarios 3, 4, 6 |
| `chunks` | Scenarios 1, 8, 11 |
| `fts_nodes` derived projection | Scenarios 1, 8, 10 |
| `vec_nodes_active` derived projection | Scenarios 11, 12 |
| `runs` | Scenario 5 |
| `steps` | Scenario 5 |
| `actions` | Scenario 5 |
| retire/supersession state | Scenarios 2, 4, 6, 8 |
| provenance-linked lineage | Scenarios 5, 7, 8 |

`optional_backfills` should **not** be treated as a storage surface. They are
request-time tasks, not durable user data.

## Retrieval Matrix

Because the Python read surface is narrower than the write surface, retrieval
must be verified through the currently supported APIs:

| Retrieval Surface | What It Proves |
|---|---|
| `filter_logical_id_eq(...)` | active canonical node visibility |
| `text_search(...)` | FTS projection and chunk-to-node resolution |
| `vector_search(...)` | vector projection and chunk-to-node resolution |
| `traverse(...)` | edge storage and active-edge resolution |
| `trace_source(...)` | source-ref lineage across nodes/edges/actions |
| `check_integrity()` | FTS coverage and physical/FK health |
| `check_semantics()` | semantic consistency, runtime FK health, vec/FTS staleness |
| `excise_source(...)` followed by query | reversible source-lineage removal |
| `safe_export(...)` | export/readback admin surface |

## Data Model For The Harness

Use a small but rich synthetic workload:

- `Meeting`
- `Task`
- `Person`
- `Document`
- `Run`
- `Step`
- `Action`

Example relationships:

- `Meeting -[GENERATED_TASK]-> Task`
- `Task -[DEPENDS_ON]-> Task`
- `Person -[ATTENDED]-> Meeting`

Use deterministic IDs and source refs where possible:

- `meeting:q1-budget`
- `task:follow-up`
- `run:planner-001`
- `step:planner-001:1`
- `action:planner-001:tool-1`
- `source:meeting-import`
- `source:planner-action-1`

This keeps scenario output readable and trace assertions stable.

## Verification Style

Each scenario should produce explicit assertions, not only smoke checks.

Examples:

- exact active node count
- exact logical IDs returned
- exact `TraceReport.node_rows` / `action_rows`
- exact semantic counters that should be zero
- explicit degraded-vs-non-degraded vector behavior

The harness should fail on the first unexpected retrieval mismatch.

## Test Strategy

Implement two test modules.

### `test_baseline_harness.py`

Run:

- Scenarios 1 through 10
- Scenario 12

Assertions:

- all non-vector write/retrieval paths succeed
- vector queries degrade rather than error

### `test_vector_harness.py`

Run:

- Scenarios 1 through 11 except degradation-only checks

Open the engine with:

- `Engine.open(path, vector_dimension=<fixed small dimension>)`

Build requirement:

- the Python package must be built in a configuration that enables both
  `python` and `sqlite-vec`

Assertions:

- vector inserts are accepted
- vector retrieval returns nodes instead of degrading
- vec repair/admin calls succeed

## CLI Behavior

`app.py` should support:

```bash
python -m python_app_harness.app --db /tmp/harness.db --mode baseline
python -m python_app_harness.app --db /tmp/harness.db --mode vector
```

Outputs:

- one line per scenario: `PASS` / `FAIL`
- short failure message with scenario name and assertion detail
- final summary count

No rich TUI or heavy logging is needed.

## Acceptance Criteria

The design is satisfied when an implementation can:

1. write through every current Python-exposed `WriteRequest` path
2. verify retrieval through every currently available Python read/admin surface
3. run cleanly in both baseline and vector-enabled modes
4. clearly distinguish:
   - canonical retrieval success
   - projection retrieval success
   - degradation behavior
   - semantic/report-based verification for runtime tables

## Explicit Non-Goals

Do not require the harness to verify runtime tables by direct row reads from
Python. The current Python API does not expose those methods yet.

Do not use direct `sqlite3` queries from Python as a substitute for missing
library features. The point of the harness is to validate the Python library
surface, not bypass it.

Do not add richer query semantics such as `select(...)`, temporal queries, or
lambda-based predicates just for the harness.

## Recommended Next Implementation Step

Implement this harness as a second Python package under `python-app-harness/`
or a sibling directory under `python/examples/`, starting with:

1. engine factory
2. Scenario 1, 3, 5, 8, and 12
3. baseline test module

That subset proves the main architectural shape before expanding to the full
scenario matrix.
