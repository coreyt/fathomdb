# Setup: sqlite-vec Capability

## Purpose

This document tracks the concrete work needed to turn vector support from a
placeholder into an explicit runtime capability.

This is a companion to:

- [design-read-execution.md](./design-read-execution.md)
- [design-typed-write.md](./design-typed-write.md)

## Current Repository State

Today the repo is intentionally incomplete:

- schema bootstrap creates `vector_profiles`
- `ensure_vector_profile()` returns `MissingCapability("sqlite-vec")`
- vector rebuild paths are deferred
- query compilation already assumes a vector-driven candidate path exists

That mismatch is acceptable in the scaffold, but it should be closed before
vector-driven reads and writes are treated as supported runtime features.

## Deliverables

1. Capability detection during schema/bootstrap.
2. A clear enable/disable model for vector profiles.
3. One active vector table naming scheme for v1.
4. Runtime errors that distinguish:
   - vector requested but extension unavailable
   - vector profile defined but disabled
   - vector profile enabled but missing projection rows
5. Tests that prove the engine degrades explicitly rather than silently.

## Decisions To Make

### 1. Loading Strategy

Choose one v1 model:

- rely on bundled/linked capability from the environment
- or support explicit extension loading at startup

The repo should not pretend both are solved yet.

### 2. Table Naming

The docs already imply versioned tables. The setup work should pick one concrete
v1 naming shape, for example:

- `vec_nodes_v1`
- plus a view or metadata pointer for the active profile

### 3. Dimension Source Of Truth

The profile metadata should define:

- embedding model or profile name
- dimension
- enabled/disabled flag
- physical table name

That metadata should be enough to validate runtime assumptions.

## Implementation Sequence

1. Extend schema bootstrap to detect vector capability and report it.
2. Define one vector profile metadata contract in `fathomdb-schema`.
3. Add admin/setup helpers to create the active vector table when capability is
   available.
4. Gate vector execution and vector write preparation on that capability report.
5. Add tests for both paths:
   - capability absent
   - capability present or mocked

## Done When

- vector availability is reported explicitly
- the engine can tell whether vector search is runnable
- the active profile metadata has one concrete v1 shape
- vector-disabled behavior is deterministic in tests
